//! `textblock.cc` — construction des mots et événements.
//!
//! Fonctions portées verbatim : `initWord`/`cleanupWord`/
//! `*ImgRenderer`, `fillWord`, `addWord`, `textWidth`,
//! `calcTextSize`/`calcTextSizes`, `addText`/`addText0`,
//! `calcSizeOfWidgetInFlow`, `findSizeRequestReference`,
//! `addWidget`, `addAnchor`, `addSpace`/`addBreakOption`/
//! `fillSpace`/`setBreakOption`/`isBreakAllowed`,
//! `addParbreak`/`addLinebreak`/`breakAdded`, `handOverBreak`,
//! `flush`, `changeLinkColor`, `changeWordStyle`,
//! `sendSelectionEvent` + `buttonPressImpl`/`buttonReleaseImpl`/
//! `motionNotifyImpl`/`enterNotifyImpl`/`leaveNotifyImpl`,
//! `removeChild`.
//!
//! Adaptations modèle D :
//! - `Widget*` → `WidgetId`, `layout->*` → `self`/arène.
//! - `content.text` ≙ `textZone->strndup` → `Vec<u8>` possédé.
//! - `widget->style->ref/unref` ≙ `Rc<Style>` (RAII).
//! - Renderers enregistrés via `Rc<RefCell<dyn ImgRenderer>>` ;
//!   `setData` repasse par `as_any_mut().downcast_mut` (le modèle
//!   arène ne peut pas partager `Rc<RefCell<T>>` et
//!   `Rc<RefCell<dyn>>` pour le même objet).
//! - `selectionHandleEvent` ≙ `SelectionState::handle_event` —
//!   stub phase sélection (retourne `false`, cf. layout.rs).

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use std::cell::RefCell;
use std::rc::Rc;

use dillo_common::dlib;
use dillo_common::lout::misc;
use dillo_common::lout::unicode;

use super::{
    penalty, Anchor, BadnessAndPenalty, TextblockData, Word, WordImgRenderer, DIV_CHARS,
    HOVER_TOOLTIP, HYPHEN_DRAW_CHAR, PENALTY_FORCE_BREAK, PENALTY_PROHIBIT_BREAK,
    WORD_CAN_BE_HYPHENATED, WORD_DIV_CHAR_AT_EOL, WORD_DRAW_AS_ONE_TEXT, WORD_END,
    WORD_PERM_DIV_CHAR, WORD_START, WORD_TOPLEFT_OF_LINE, WORD_UNBREAKABLE_FOR_MIN_WIDTH,
};
use crate::layout::Layout;
use crate::oof::{make_parent_ref_in_flow, make_parent_ref_oof, test_style_out_of_flow, NUM_OOFM};
use crate::style::{DisplayType, Style, StyleAttrs, VAlignType, WhiteSpace};
use crate::types::{content_type, Content, ContentData, Requisition, WidgetReference};
use crate::widget::WidgetKind;
use crate::WidgetId;

/// `core::SelectionState::END_OF_WORD` (selection.hh:201).
pub const SELECTION_END_OF_WORD: i32 = 1 << 30;

/// Style placeholder ≙ `word->style = NULL` dans `initWord`
/// (toujours écrasé par `fillWord` avant toute lecture — verbatim).
pub(crate) fn null_style() -> Rc<Style> {
    thread_local! {
        static S: Rc<Style> = Style::create(&StyleAttrs::default());
    }
    S.with(std::clone::Clone::clone)
}

impl Layout {
    /// `Textblock (limitTextWidth, treatAsInline)` + `setButtonSensitive`
    /// + `initNewLine` (textblock.cc:207-276). Les vecteurs `lines`/
    ///   `paragraphs`/`words`/`anchors` démarrent vides (amont
    ///   `SimpleVector(1)` — capacité 1, taille 0).
    pub fn tb_new(&mut self, limit_text_width: bool, treat_as_inline: bool) -> WidgetId {
        let tb = TextblockData::new(limit_text_width, treat_as_inline);
        let w = self.add_widget_node(WidgetKind::Textblock { tb: Box::new(tb) });
        // `setButtonSensitive (true)` verbatim.
        self.node_mut(w, |n| {
            n.button_sensitive = true;
            n.button_sensitive_set = true;
        });
        // `initNewLine` — verbatim (linebreaking.cc:2057).
        self.with_tb(w, |l, tb| l.tb_init_new_line(w, tb));
        w
    }

    /// `~Textblock` (textblock.cc:278-307) — appelé depuis
    /// `widget_delete` : `NULLé` le tooltip hover (verbatim), les mots
    /// sont « nettoyés » (enfants `WIDGET_IN_FLOW` détruits par le
    /// chemin générique — `cleanupWord` ≙ `delete` du widget) et les
    /// ancres retirées de la table.
    pub(crate) fn tb_drop(&mut self, w: WidgetId) {
        // `~AlignedTextblock` → `list->unref (listPos)`
        // (alignedtextblock.cc:82-85) — le destructeur de la
        // sous-classe court avant `~Textblock` (verbatim). Le
        // `refCount` amont ≙ `Rc` : le dernier clone libère.
        self.node_tb_mut(w, |tb| {
            if let Some(aligned) = tb.aligned.take() {
                aligned.list.borrow_mut().unref(aligned.list_pos);
            }
        });
        // `hoverTooltip = NULL` verbatim (static partagée).
        HOVER_TOOLTIP.with(|t| *t.borrow_mut() = None);
        // `removeAnchor (anchor->name)` pour chaque ancre.
        let names = self
            .node_tb(w, |tb| {
                tb.anchors
                    .iter()
                    .map(|a| a.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for name in names {
            self.remove_anchor(&name);
        }
        // `for (i): cleanupWord (i)` verbatim — détache les
        // renderers d'image mot/espace (le `style->unref` est RAII).
        self.with_tb(w, |l, tb| {
            for i in 0..tb.words.len() {
                l.tb_cleanup_word(w, tb, i as i32);
            }
        });
        // `words = NULL` amont : les mots sont des données
        // internes au nœud — détruits avec lui (RAII).
    }

    // ----------------------------------------------------------------
    // initWord / cleanupWord / renderers (textblock.cc:1801-1874).
    // ----------------------------------------------------------------

    /// `initWord` — champs style/renderers à NULL (≙ placeholder).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_init_word(&self, tb: &mut TextblockData, word_no: i32) {
        if let Some(word) = tb.words.get_mut(word_no as usize) {
            word.style = null_style();
            word.space_style = null_style();
            word.word_img_renderer = None;
            word.space_img_renderer = None;
        }
    }

    /// `cleanupWord` (textblock.cc:1810-1825) — amont `delete`s le
    /// widget `WIDGET_IN_FLOW` et le `WidgetReference` `WIDGET_OOF_REF` ;
    /// en arène, le widget est détaché/supprimé par le chemin
    /// générique (`widget_delete`) — ici : retrait des renderers.
    /// (Appelé verbatim quand un mot sort de `words`.)
    pub(crate) fn tb_cleanup_word(&mut self, w: WidgetId, tb: &mut TextblockData, word_no: i32) {
        // `removeWordImgRenderer` + `removeSpaceImgRenderer`.
        self.tb_remove_word_img_renderer(w, tb, word_no);
        self.tb_remove_space_img_renderer(w, tb, word_no);
        // `style->unref`/`spaceStyle->unref` ≙ drop Rc (RAII).
    }

    /// `removeWordImgRenderer` (textblock.cc:1827-1837).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_remove_word_img_renderer(
        &mut self,
        _w: WidgetId,
        tb: &mut TextblockData,
        word_no: i32,
    ) {
        if let Some(word) = tb.words.get_mut(word_no as usize) {
            if word.word_img_renderer.is_some() {
                if let Some(bi) = word.style.attrs.background_image.clone() {
                    if let Some(r) = &word.word_img_renderer {
                        bi.borrow().remove_external_img_renderer(r);
                    }
                }
                word.word_img_renderer = None;
            }
        }
    }

    /// `setWordImgRenderer` (textblock.cc:1839-1849).
    pub(crate) fn tb_set_word_img_renderer(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_no: i32,
    ) {
        let has_bg = tb
            .words
            .get(word_no as usize)
            .is_some_and(|word| word.style.attrs.background_image.is_some());
        if has_bg {
            let r: Rc<RefCell<dyn crate::imgrenderer::ImgRenderer>> = Rc::new(RefCell::new(
                WordImgRenderer::new(&self.arena, self.id, w, word_no),
            ));
            if let Some(word) = tb.words.get_mut(word_no as usize) {
                if let Some(bi) = &word.style.attrs.background_image {
                    bi.borrow().put_external_img_renderer(&r);
                }
                word.word_img_renderer = Some(r);
            }
        } else if let Some(word) = tb.words.get_mut(word_no as usize) {
            word.word_img_renderer = None;
        }
    }

    /// `removeSpaceImgRenderer` (textblock.cc:1851-1861).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_remove_space_img_renderer(
        &mut self,
        _w: WidgetId,
        tb: &mut TextblockData,
        word_no: i32,
    ) {
        if let Some(word) = tb.words.get_mut(word_no as usize) {
            if word.space_img_renderer.is_some() {
                if let Some(bi) = word.space_style.attrs.background_image.clone() {
                    if let Some(r) = &word.space_img_renderer {
                        bi.borrow().remove_external_img_renderer(r);
                    }
                }
                word.space_img_renderer = None;
            }
        }
    }

    /// `setSpaceImgRenderer` (textblock.cc:1863-1873).
    pub(crate) fn tb_set_space_img_renderer(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_no: i32,
    ) {
        let has_bg = tb
            .words
            .get(word_no as usize)
            .is_some_and(|word| word.space_style.attrs.background_image.is_some());
        if has_bg {
            let r: Rc<RefCell<dyn crate::imgrenderer::ImgRenderer>> = Rc::new(RefCell::new(
                super::SpaceImgRenderer::new(&self.arena, self.id, w, word_no),
            ));
            if let Some(word) = tb.words.get_mut(word_no as usize) {
                if let Some(bi) = &word.space_style.attrs.background_image {
                    bi.borrow().put_external_img_renderer(&r);
                }
                word.space_img_renderer = Some(r);
            }
        } else if let Some(word) = tb.words.get_mut(word_no as usize) {
            word.space_img_renderer = None;
        }
    }

    /// `fillWord` (textblock.cc:1875-1906).
    #[allow(clippy::too_many_arguments)] // signatures verbatim amont.
    pub(crate) fn tb_fill_word(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_no: i32,
        width: i32,
        ascent: i32,
        descent: i32,
        flags: i16,
        style: &Rc<Style>,
    ) {
        self.tb_remove_word_img_renderer(w, tb, word_no);
        self.tb_remove_space_img_renderer(w, tb, word_no);
        if let Some(word) = tb.words.get_mut(word_no as usize) {
            word.size.width = width;
            word.size.ascent = ascent;
            word.size.descent = descent;
            word.orig_space = 0;
            word.eff_space = 0;
            word.hyphen_width = 0;
            word.badness_and_penalty.set_penalty(PENALTY_PROHIBIT_BREAK);
            word.content.space = false;
            word.flags = flags;
            word.style = style.clone();
            word.space_style = style.clone();
        }
        self.tb_set_word_img_renderer(w, tb, word_no);
        self.tb_set_space_img_renderer(w, tb, word_no);
    }

    /// `addWord` (textblock.cc:1777-1799) — `lineBreakWidth` fixé à
    /// `getAvailWidth (true)` au premier mot.
    #[allow(clippy::too_many_arguments)] // signatures verbatim amont.
    pub(crate) fn tb_add_word(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        width: i32,
        ascent: i32,
        descent: i32,
        flags: i16,
        style: &Rc<Style>,
    ) -> i32 {
        if tb.line_break_width == -1 {
            tb.line_break_width = self.widget_get_avail_width(w, true);
        }
        tb.words.push(Word {
            size: Requisition::default(),
            orig_space: 0,
            eff_space: 0,
            hyphen_width: 0,
            flags: 0,
            content: Content {
                r#type: content_type::INVALID,
                space: false,
                data: crate::types::ContentData::None,
            },
            total_width: 0,
            max_ascent: 0,
            max_descent: 0,
            total_space_stretchability: 0,
            total_space_shrinkability: 0,
            badness_and_penalty: BadnessAndPenalty::default(),
            style: null_style(),
            space_style: null_style(),
            word_img_renderer: None,
            space_img_renderer: None,
        });
        let word_no = tb.words.len() as i32 - 1;
        self.tb_init_word(tb, word_no);
        self.tb_fill_word(w, tb, word_no, width, ascent, descent, flags, style);
        word_no
    }

    // ----------------------------------------------------------------
    // textWidth / calcTextSize / calcTextSizes (textblock.cc:1908-2291).
    // ----------------------------------------------------------------

    /// `textWidth` (textblock.cc:1908-1969) — `isStart`/`isEnd`
    /// pilotent `text-transform: capitalize` (premier glyphe du
    /// mot logique — un mot peut être coupé en plusieurs `Word`).
    pub(crate) fn tb_text_width(
        &self,
        text: &[u8],
        start: usize,
        len: usize,
        style: &Style,
        is_start: bool,
        _is_end: bool,
    ) -> i32 {
        let font = style.attrs.font.clone().unwrap_or_else(unreachable_font);
        let mut ret = 0;
        if len > 0 {
            use crate::style::TextTransform as Tt;
            let sub = &text[start..start + len];
            let mut platform = self.platform.borrow_mut();
            match style.attrs.text_transform {
                Tt::Uppercase => {
                    let s = platform.text_to_upper(sub);
                    ret = platform.text_width(&font, &s);
                }
                Tt::Lowercase => {
                    let s = platform.text_to_lower(sub);
                    ret = platform.text_width(&font, &s);
                }
                Tt::Capitalize => {
                    if is_start {
                        // Pas de moyen de connaître la ponctuation
                        // non-ASCII (bug amont).
                        let mut initial_seen = false;
                        for &c in &text[..start] {
                            if !dlib::is_punct(c) {
                                initial_seen = true;
                            }
                        }
                        if initial_seen {
                            ret = platform.text_width(&font, sub);
                        } else {
                            let mut after = 0usize;
                            while after < sub.len() && dlib::is_punct(sub[after]) {
                                after += 1;
                            }
                            if after < sub.len() {
                                after = platform.next_glyph(sub, after as i32) as usize;
                            }
                            if after > len {
                                after = len;
                            }
                            let s = platform.text_to_upper(&sub[..after]);
                            ret = platform.text_width(&font, &s)
                                + platform.text_width(&font, &sub[after..]);
                        }
                    } else {
                        ret = platform.text_width(&font, sub);
                    }
                }
                Tt::None => {
                    ret = platform.text_width(&font, sub);
                }
            }
        }
        ret
    }

    /// `calcTextSize` (textblock.cc:1970-2024) — line-height et
    /// valign sub/super ajustent ascent/descent.
    pub(crate) fn tb_calc_text_size(
        &mut self,
        text: &[u8],
        len: usize,
        style: &Style,
        size: &mut Requisition,
        is_start: bool,
        is_end: bool,
    ) {
        size.width = self.tb_text_width(text, 0, len, style, is_start, is_end);
        let font = style.attrs.font.clone().unwrap_or_else(unreachable_font);
        size.ascent = font.ascent;
        size.descent = font.descent;

        if style.attrs.line_height != crate::style::LENGTH_AUTO {
            // Hauteur relative à la taille de fonte (plus petite
            // que ascent+descent — verbatim).
            let factor = font.attrs.size as f32 / (font.ascent + font.descent) as f32;
            size.ascent = misc::round_int(f64::from(size.ascent) * f64::from(factor));
            size.descent = misc::round_int(f64::from(size.descent) * f64::from(factor));

            let height = if crate::style::is_abs_length(style.attrs.line_height) {
                crate::style::abs_length_val(style.attrs.line_height)
            } else {
                crate::style::multiply_with_per_length_rounded(
                    font.attrs.size,
                    style.attrs.line_height,
                )
            };
            let leading = height - font.attrs.size;
            size.ascent += leading / 2;
            size.descent += leading - (leading / 2);
        }

        if style.attrs.valign == VAlignType::Sub {
            let required_descent = font.descent + font.ascent / 3;
            size.descent = size.descent.max(required_descent);
        } else if style.attrs.valign == VAlignType::Super {
            let required_ascent = font.ascent + font.ascent / 2;
            size.ascent = size.ascent.max(required_ascent);
        }
    }

    /// `calcTextSizes` (textblock.cc:2254-2291) — tailles des
    /// parties d'un mot coupé par des caractères de division ; la
    /// dernière partie est calculée simplement, les autres par
    /// soustraction des suffixes (doc dw-line-breaking).
    pub(crate) fn tb_calc_text_sizes(
        &mut self,
        text: &[u8],
        text_len: usize,
        style: &Style,
        num_breaks: usize,
        break_pos: &[i32],
        word_size: &mut [Requisition],
    ) {
        // La taille de la dernière partie, simplement.
        let last_start = break_pos[num_breaks - 1] as usize;
        let mut s = Requisition::default();
        self.tb_calc_text_size(
            &text[last_start..],
            text_len - last_start,
            style,
            &mut s,
            true,
            true,
        );
        word_size[num_breaks] = s;

        // Le reste, plus compliqué (voir doc amont « Hyphens »).
        for i in (0..num_breaks).rev() {
            let start = if i == 0 { 0 } else { break_pos[i - 1] as usize };
            let mut s = Requisition::default();
            self.tb_calc_text_size(
                &text[start..],
                text_len - start,
                style,
                &mut s,
                i == 0,
                i == num_breaks - 1,
            );
            word_size[i] = s;
            for j in (i + 1)..=num_breaks {
                word_size[i].width -= word_size[j].width;
            }
        }
    }

    /// `addText` (textblock.cc:2025-2253) — découpe le mot sur les
    /// caractères de division (`divChars`), calcule les tailles des
    /// parties sans les caractères supprimés, fixe les pénalités.
    pub fn tb_add_text(&mut self, w: WidgetId, text: &[u8], len: usize, style: &Rc<Style>) {
        self.with_tb(w, |l, tb| {
            // Compte les caractères de division.
            let mut num_parts = 1usize;
            let mut pos = 0usize;
            while let Some(rest) = unicode::next_utf8_char_len(&text[pos..], len - pos) {
                let s = &text[pos..];
                let found_div = DIV_CHARS
                    .iter()
                    .position(|d| {
                        let l_div = d.s.len();
                        s.len() >= l_div && len - pos >= l_div && s[..l_div] == *d.s
                    })
                    .map_or(-1, |j| j as i32);
                if found_div != -1 {
                    if DIV_CHARS[found_div as usize].penalty_index_left != -1 {
                        num_parts += 1;
                    }
                    if DIV_CHARS[found_div as usize].penalty_index_right != -1 {
                        num_parts += 1;
                    }
                }
                pos = len - rest.len();
            }

            if num_parts == 1 {
                // Cas simple et courant : pas de caractère de
                // division ; peut encore être césuré automatiquement.
                let mut size = Requisition::default();
                l.tb_calc_text_size(&text[..len], len, style, &mut size, true, true);
                l.tb_add_text0(
                    w,
                    tb,
                    &text[..len],
                    len,
                    WORD_CAN_BE_HYPHENATED | WORD_START | WORD_END,
                    style,
                    &size,
                );
            } else {
                // Positions de césure.
                let mut n = 0usize;
                let mut total_len_char_removed = 0usize;
                let mut part_penalty_index = vec![0i32; num_parts - 1];
                let mut part_start = vec![0i32; num_parts];
                let mut part_end = vec![0i32; num_parts];
                let mut char_removed = vec![false; num_parts - 1];
                let mut can_be_hyphenated = vec![false; num_parts + 1];
                let mut perm_div_char = vec![false; num_parts - 1];
                let mut unbreakable_for_min_width = vec![false; num_parts - 1];
                can_be_hyphenated[0] = true;
                can_be_hyphenated[num_parts] = true;
                part_start[0] = 0;
                part_end[num_parts - 1] = len as i32;

                let mut pos = 0usize;
                while let Some(rest) = unicode::next_utf8_char_len(&text[pos..], len - pos) {
                    let s = &text[pos..];
                    let found_div = DIV_CHARS
                        .iter()
                        .position(|d| {
                            let l_div = d.s.len();
                            s.len() >= l_div && len - pos >= l_div && s[..l_div] == *d.s
                        })
                        .map_or(-1, |j| j as i32);
                    if found_div != -1 {
                        let j = found_div as usize;
                        let dc = &DIV_CHARS[j];
                        let l_div = dc.s.len();
                        if dc.char_removed {
                            debug_assert!(dc.penalty_index_left != -1);
                            debug_assert_eq!(dc.penalty_index_right, -1);
                            part_penalty_index[n] = dc.penalty_index_left;
                            char_removed[n] = true;
                            perm_div_char[n] = false;
                            unbreakable_for_min_width[n] = dc.unbreakable_for_min_width;
                            can_be_hyphenated[n + 1] = dc.can_be_hyphenated;
                            part_end[n] = pos as i32;
                            part_start[n + 1] = (pos + l_div) as i32;
                            n += 1;
                            total_len_char_removed += l_div;
                        } else {
                            debug_assert!(
                                dc.penalty_index_left != -1 || dc.penalty_index_right != -1
                            );
                            if dc.penalty_index_left != -1 {
                                part_penalty_index[n] = dc.penalty_index_left;
                                char_removed[n] = false;
                                perm_div_char[n] = false;
                                unbreakable_for_min_width[n] = dc.unbreakable_for_min_width;
                                can_be_hyphenated[n + 1] = dc.can_be_hyphenated;
                                part_end[n] = pos as i32;
                                part_start[n + 1] = pos as i32;
                                n += 1;
                            }
                            if dc.penalty_index_right != -1 {
                                part_penalty_index[n] = dc.penalty_index_right;
                                char_removed[n] = false;
                                perm_div_char[n] = true;
                                unbreakable_for_min_width[n] = dc.unbreakable_for_min_width;
                                can_be_hyphenated[n + 1] = dc.can_be_hyphenated;
                                part_end[n] = (pos + l_div) as i32;
                                part_start[n + 1] = (pos + l_div) as i32;
                                n += 1;
                            }
                        }
                    }
                    pos = len - rest.len();
                }

                // Texte sans les caractères supprimés (soft hyphens).
                let mut text_without_hyphens_buf = Vec::new();
                let mut break_pos_buf = Vec::new();
                let (text_without_hyphens, break_pos_without_hyphens): (&[u8], &[i32]) =
                    if total_len_char_removed == 0 {
                        // Pas de caractères supprimés : `partEnd` —
                        // la fin de la dernière partie est aussi
                        // position de césure, superflue mais
                        // inoffensive (verbatim).
                        (&text[..len], &part_end)
                    } else {
                        // Copie dans des buffers dédiés (verbatim).
                        let mut n2 = 0usize;
                        for i in 0..num_parts {
                            text_without_hyphens_buf.extend_from_slice(
                                &text[part_start[i] as usize..part_end[i] as usize],
                            );
                            n2 += (part_end[i] - part_start[i]) as usize;
                            if i < num_parts - 1 {
                                break_pos_buf.push(n2 as i32);
                            }
                        }
                        (&text_without_hyphens_buf, &break_pos_buf)
                    };

                let mut word_size = vec![Requisition::default(); num_parts];
                l.tb_calc_text_sizes(
                    text_without_hyphens,
                    len - total_len_char_removed,
                    style,
                    num_parts - 1,
                    break_pos_without_hyphens,
                    &mut word_size,
                );

                for i in 0..num_parts {
                    let mut flags = 0i16;
                    // Pas de césure auto si un caractère adjacent
                    // l'interdit (soft hyphen — verbatim).
                    if can_be_hyphenated[i] && can_be_hyphenated[i + 1] {
                        flags |= WORD_CAN_BE_HYPHENATED;
                    }
                    if i < num_parts - 1 {
                        if char_removed[i] {
                            flags |= WORD_DIV_CHAR_AT_EOL;
                        }
                        if perm_div_char[i] {
                            flags |= WORD_PERM_DIV_CHAR;
                        }
                        if unbreakable_for_min_width[i] {
                            flags |= WORD_UNBREAKABLE_FOR_MIN_WIDTH;
                        }
                        flags |= WORD_DRAW_AS_ONE_TEXT;
                    }
                    if i == 0 {
                        flags |= WORD_START;
                    }
                    if i == num_parts - 1 {
                        flags |= WORD_END;
                    }

                    l.tb_add_text0(
                        w,
                        tb,
                        &text[part_start[i] as usize..part_end[i] as usize],
                        (part_end[i] - part_start[i]) as usize,
                        flags,
                        style,
                        &word_size[i],
                    );

                    if i < num_parts - 1 {
                        let pi = part_penalty_index[i] as usize;
                        let p1 = penalty(pi, 0);
                        let p2 = penalty(pi, 1);
                        let last = tb.words.len() as i32 - 1;
                        if let Some(word) = tb.words.last_mut() {
                            l.tb_set_break_option(word, style, p1, p2, false);
                        }
                        if char_removed[i] {
                            // Seuls les traits d'union inconditionnels
                            // (U+2010) peuvent servir de caractère
                            // dessiné — verbatim commentaire amont.
                            let font = tb.words[last as usize]
                                .style
                                .attrs
                                .font
                                .clone()
                                .unwrap_or_else(unreachable_font);
                            let hw = l.platform.borrow_mut().text_width(&font, HYPHEN_DRAW_CHAR);
                            tb.words[last as usize].hyphen_width = hw as i16;
                        }
                        l.tb_accumulate_word_data(w, tb, last);
                        l.tb_correct_last_word_extremes(tb);
                    }
                }
            }
        });
    }

    /// `addText0` (textblock.cc:2436-2480) — ajoute un mot sans
    /// césures ; `content.text` ≙ `textZone->strndup`.
    #[allow(clippy::too_many_arguments)] // signatures verbatim amont.
    pub(crate) fn tb_add_text0(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        text: &[u8],
        len: usize,
        flags: i16,
        style: &Rc<Style>,
        size: &Requisition,
    ) {
        let word_no = self.tb_add_word(w, tb, size.width, size.ascent, size.descent, flags, style);
        if let Some(word) = tb.words.get_mut(word_no as usize) {
            word.content.r#type = content_type::TEXT;
            word.content.data = crate::types::ContentData::Text(text[..len].to_vec());
        }
        self.tb_process_word(w, tb, word_no);
    }

    /// `calcSizeOfWidgetInFlow` (textblock.cc:2292-2419) — retourne
    /// vrai si le widget doit être positionné en haut de la ligne.
    /// Pour les widgets de type bloc en tête de ligne, les positions
    /// sont passées à `sizeRequest` de façon itérative (xRel dépend
    /// de l'alignement, qui dépend de la largeur — verbatim).
    pub(crate) fn tb_calc_size_of_widget_in_flow(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_index: i32,
        widget: WidgetId,
        size: &mut Requisition,
    ) -> bool {
        let first_word_of_line = if tb.has_listitem_value {
            // Pour les items de liste, le mot #0 (puce, numéro)
            // compte — verbatim.
            word_index == 1
                || (word_index > 0
                    && tb.words[word_index as usize - 1].content.r#type == content_type::BREAK)
        } else {
            word_index == 0
                || tb.words[word_index as usize - 1].content.r#type == content_type::BREAK
        };

        let widget_style = self.widget_style(widget);
        let display = widget_style.as_ref().map(|s| s.attrs.display);
        if first_word_of_line
            && matches!(
                display,
                Some(DisplayType::Block | DisplayType::ListItem | DisplayType::Table)
            )
        {
            // « pass position » — le widget enfant gère lui-même
            // les floats : les bords gauche/droit ne les incluent
            // pas (verbatim).
            let left_border = self.node(w, super::super::widget::WidgetNode::box_offset_x)
                + tb.left_inner_padding
                + if tb.lines.is_empty() {
                    tb.line1_offset
                } else {
                    0
                };
            let right_border = self.node(w, super::super::widget::WidgetNode::box_rest_width);

            let mut last_margin = 0;
            let y_line = self.tb_y_offset_of_line_to_be_created(w, tb, Some(&mut last_margin));
            let y_rel =
                y_line - last_margin.min(widget_style.as_ref().map_or(0, |s| s.attrs.margin.top));

            // Références du widget enfant (`numSizeRequestReferences`
            // ≙ `w_num_size_request_references` + accesseurs).
            let mut child_refs = Vec::new();
            for i in 0..self.w_num_size_request_references(widget) {
                if let Some(r) = self.w_size_request_reference(widget, i) {
                    child_refs.push(r);
                }
            }
            let mut old_x_rel = left_border;
            loop {
                let child_params =
                    tb.size_request_params
                        .for_child(w, &child_refs, old_x_rel, y_rel);
                self.widget_size_request(
                    widget,
                    size,
                    child_params.num_pos(),
                    child_params.references(),
                    child_params.xs(),
                    child_params.ys(),
                );

                let new_x_rel = match widget_style.as_ref().map(|s| s.attrs.text_align) {
                    Some(crate::style::TextAlignType::Right) => {
                        tb.line_break_width - right_border - size.width
                    }
                    Some(crate::style::TextAlignType::Center) => {
                        (left_border + tb.line_break_width - right_border - size.width) / 2
                    }
                    // LEFT / STRING / JUSTIFY / défaut — verbatim.
                    _ => left_border,
                };
                let mut x_rel = new_x_rel;
                // Cf. `calcTextOffset` — verbatim.
                if x_rel < left_border {
                    x_rel = left_border;
                }
                if x_rel == old_x_rel {
                    break;
                }
                old_x_rel = x_rel;
            }
            true
        } else {
            // « do not pass position » — inline etc.
            self.widget_size_request(widget, size, 0, &[], &[], &[]);
            false
        }
    }

    /// `findSizeRequestReference (reference, xRef, yRef)`
    /// (textblock.cc:2420-2435).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_find_size_request_reference(
        &self,
        w: WidgetId,
        tb: &TextblockData,
        reference: Option<WidgetId>,
    ) -> Option<(i32, i32)> {
        match reference {
            Some(r) if r == w => Some((0, 0)),
            Some(r) => tb.size_request_params.find_reference(r),
            None => None,
        }
    }

    /// `addWidget` (textblock.cc:2481-2562) — `parentRef = -1`
    /// d'abord (verbatim : sinon `sizeRequest` re-wrappe ce
    /// textblock), dispatch OOF/in-flow.
    pub fn tb_add_widget(&mut self, w: WidgetId, widget: WidgetId, style: &Rc<Style>) {
        self.with_tb(w, |l, tb| {
            // `parentRef = -1` — verbatim.
            l.node_mut(widget, |n| n.parent_ref = -1);
            l.widget_set_style(widget, style);
            l.init_out_of_flow_mgrs(w);

            let style_attrs = &style.attrs;
            if test_style_out_of_flow(style_attrs) {
                let oofm_index = l.get_oofm_index(widget);
                let container = tb.oof.oof_container[oofm_index];
                if let Some(c) = container {
                    l.widget_set_parent(widget, c);
                }
                l.widget_set_generator(widget, Some(w));
                let mut size = Requisition::default();
                let mut oofm_sub_ref = 0;
                if let Some(mgr) = l.search_out_of_flow_mgr(w, oofm_index) {
                    let mut m = mgr.borrow_mut();
                    oofm_sub_ref = m.add_widget_oof(l, widget, w, tb.words.len() as i32);
                    m.calc_widget_ref_size(l, widget, &mut size);
                }
                l.node_mut(widget, |n| {
                    n.parent_ref = make_parent_ref_oof(oofm_index, oofm_sub_ref);
                });

                let word_no = l.tb_add_word(w, tb, size.width, size.ascent, size.descent, 0, style);
                let wr = WidgetReference::new(widget);
                if let Some(word) = tb.words.get_mut(word_no as usize) {
                    word.content.r#type = content_type::WIDGET_OOF_REF;
                    word.content.data = crate::types::ContentData::WidgetReference(wr);
                }
                l.widget_set_widget_reference(widget, Some(wr));
                // Après une référence OOF, la césure est permise
                // (verbatim — évite des problèmes près des floats).
                let p = tb.words.len() - 1;
                if let Some(word) = tb.words.get_mut(p) {
                    let st = style.clone();
                    l.tb_set_break_option(word, &st, 0, 0, false);
                }
            } else {
                // In-flow.
                l.widget_set_parent(widget, w);

                // Si l'enfant est lui-même un textblock, le
                // déclarer aux mgrs (verbatim `instanceOf`).
                if l.node(widget, |n| matches!(n.kind, WidgetKind::Textblock { .. })) {
                    for i in 0..NUM_OOFM {
                        if let Some(mgr) = l.search_out_of_flow_mgr(w, i) {
                            mgr.borrow_mut().add_widget_in_flow(
                                l,
                                widget,
                                Some(w),
                                tb.words.len() as i32,
                            );
                        }
                    }
                }

                let mut size = Requisition::default();
                let topleft = l.tb_calc_size_of_widget_in_flow(
                    w,
                    tb,
                    tb.words.len() as i32,
                    widget,
                    &mut size,
                );
                let flags = if topleft { WORD_TOPLEFT_OF_LINE } else { 0 };
                let word_no =
                    l.tb_add_word(w, tb, size.width, size.ascent, size.descent, flags, style);
                if let Some(word) = tb.words.get_mut(word_no as usize) {
                    word.content.r#type = content_type::WIDGET_IN_FLOW;
                    word.content.data = crate::types::ContentData::Widget(widget);
                }
            }

            l.tb_process_word(w, tb, tb.words.len() as i32 - 1);
        });
    }

    /// `addAnchor` (textblock.cc:2563-2605) — `name` copié ; `false`
    /// si l'ancre existe déjà dans l'arbre.
    pub fn tb_add_anchor(&mut self, w: WidgetId, name: &[u8], _style: &Rc<Style>) -> bool {
        self.with_tb(w, |l, tb| {
            // Une ancre ne prend pas de place : on peut appeler
            // `addAnchor` ici déjà (verbatim).
            let copy = if l.node(w, super::super::widget::WidgetNode::was_allocated) {
                let y = if tb.lines.is_empty() {
                    l.node(w, |n| n.allocation.y)
                } else {
                    l.node(w, |n| n.allocation.y)
                        + tb.line_y_offset_widget_i(
                            tb.lines.len() as i32 - 1,
                            &l.node(w, |n| n.allocation),
                        )
                };
                l.add_anchor(w, name, y)
            } else {
                l.add_anchor(w, name, -1)
            };
            if let Some(copy) = copy {
                tb.anchors.push(Anchor {
                    name: copy,
                    word_index: tb.words.len() as i32,
                });
                true
            } else {
                // \todo amont : sauvegarder l'ancre pour les
                // changements futurs de l'arbre.
                false
            }
        })
        .unwrap_or(false)
    }

    /// `addSpace` (textblock.cc:2606-2626).
    pub fn tb_add_space(&mut self, w: WidgetId, style: &Rc<Style>) {
        self.with_tb(w, |l, tb| {
            let word_index = tb.words.len() as i32 - 1;
            if word_index >= 0 {
                l.tb_fill_space(w, tb, word_index, style);
                l.tb_accumulate_word_data(w, tb, word_index);
                l.tb_correct_last_word_extremes(tb);
            }
        });
    }

    /// `addBreakOption` (textblock.cc:2627-2642) — alternative à
    /// `addSpace` pour les caractères idéographiques ; `forceBreak`
    /// permet la césure même en `pre`.
    pub fn tb_add_break_option(&mut self, w: WidgetId, style: &Rc<Style>, force_break: bool) {
        self.with_tb(w, |l, tb| {
            let word_index = tb.words.len() as i32 - 1;
            if word_index >= 0 {
                let st = style.clone();
                if let Some(word) = tb.words.get_mut(word_index as usize) {
                    l.tb_set_break_option(word, &st, 0, 0, force_break);
                }
                // Pas besoin d'`accumulateWordData` (verbatim).
                l.tb_correct_last_word_extremes(tb);
            }
        });
    }

    /// `fillSpace` (textblock.cc:2643-2701) — ne remplace pas une
    /// pénalité déjà fixée, ni un `WIDGET_OOF_REF` (verbatim).
    pub(crate) fn tb_fill_space(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_no: i32,
        style: &Rc<Style>,
    ) {
        let (is_space, is_oof_ref) = tb.words.get(word_no as usize).map_or((true, true), |word| {
            (
                word.content.space,
                word.content.r#type == content_type::WIDGET_OOF_REF,
            )
        });
        if !is_space && !is_oof_ref {
            let st = style.clone();
            if let Some(word) = tb.words.get_mut(word_no as usize) {
                self.tb_set_break_option(word, &st, 0, 0, false);
            }
            self.tb_remove_space_img_renderer(w, tb, word_no);
            if let Some(word) = tb.words.get_mut(word_no as usize) {
                word.content.space = true;
                let sw = style.attrs.font.as_ref().map_or(0, |f| f.space_width)
                    + style.attrs.word_spacing;
                word.orig_space = sw as i16;
                word.eff_space = sw as i16;
                word.space_style = style.clone();
            }
            self.tb_set_space_img_renderer(w, tb, word_no);
        }
    }

    /// `setBreakOption` (textblock.cc:2702-2720) — n'écrase pas un
    /// `lineMustBeBroken(0)` (verbatim).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_set_break_option(
        &mut self,
        word: &mut Word,
        style: &Style,
        break_penalty1: i32,
        break_penalty2: i32,
        force_break: bool,
    ) {
        if !word.badness_and_penalty.line_must_be_broken(0) {
            if force_break || Self::tb_is_break_allowed(style) {
                word.badness_and_penalty
                    .set_penalties(break_penalty1, break_penalty2);
            } else {
                word.badness_and_penalty.set_penalty(PENALTY_PROHIBIT_BREAK);
            }
        }
    }

    /// `isBreakAllowed (style)` (textblock.cc:2721-2743).
    pub(crate) fn tb_is_break_allowed(style: &Style) -> bool {
        match style.attrs.white_space {
            WhiteSpace::Normal | WhiteSpace::PreLine | WhiteSpace::PreWrap => true,
            WhiteSpace::Pre | WhiteSpace::Nowrap => false,
        }
    }

    /// `addParbreak` (textblock.cc:2744-2833) — césure de
    /// paragraphe ; hack de propagation vers le textblock parent
    /// quand le break serait le 1er mot (verbatim).
    pub fn tb_add_parbreak(&mut self, w: WidgetId, space: i32, style: &Rc<Style>) {
        self.with_tb(w, |l, tb| {
            // Un break ne peut être le premier mot (ni juste après
            // la puce d'un item — verbatim).
            if tb.words.is_empty() || (tb.has_listitem_value && tb.words.len() == 1) {
                // Cherche le textblock ancêtre où ajuster le
                // `breakSpace` (flux normal seulement — verbatim).
                let mut widget = w;
                while let Some(parent) = l.widget_parent(widget) {
                    if !l.node(parent, |n| matches!(n.kind, WidgetKind::Textblock { .. }))
                        || l.widget_is_oof(widget)
                    {
                        break;
                    }
                    // `textblock2` = parent textblock.
                    let index = if l.node_tb(parent, |t| t.has_listitem_value).unwrap_or(false) {
                        1
                    } else {
                        0
                    };
                    let is_first = l
                        .node_tb(parent, |t| {
                            t.words.get(index).is_some_and(|wd| {
                                wd.content.r#type == content_type::WIDGET_IN_FLOW
                                    && wd.content.data == crate::types::ContentData::Widget(widget)
                            })
                        })
                        .unwrap_or(false);
                    if !is_first {
                        // Textblock trouvé — ajuste le breakSpace du
                        // break précédent si nécessaire (verbatim).
                        let lineno = l.widget_in_flow_sub_ref(widget);
                        if lineno > 0 {
                            let target = l.node_tb(parent, |t| {
                                let line = t.lines.get(lineno as usize - 1)?;
                                let w2 = t.words.get(line.first_word as usize)?;
                                (w2.content.r#type == content_type::BREAK)
                                    .then_some(line.first_word)
                            });
                            if let Some(Some(first_word)) = target {
                                let needs = l.node_tb(parent, |t| {
                                    t.words[first_word as usize].content.data
                                        != crate::types::ContentData::BreakSpace(space)
                                        && matches!(
                                            t.words[first_word as usize].content.data,
                                            crate::types::ContentData::BreakSpace(bs) if bs < space
                                        )
                                });
                                if needs.unwrap_or(false) {
                                    l.node_tb_mut(parent, |t| {
                                        if let Some(wd) = t.words.get_mut(first_word as usize) {
                                            wd.content.data =
                                                crate::types::ContentData::BreakSpace(space);
                                        }
                                    });
                                    l.widget_queue_resize(
                                        parent,
                                        make_parent_ref_in_flow(lineno),
                                        false,
                                    );
                                    // `mustQueueResize = false`
                                    // (verbatim).
                                    l.node_tb_mut(parent, |t| t.must_queue_resize = false);
                                }
                            }
                        }
                        return;
                    }
                    widget = parent;
                }
                return;
            }

            // Un break juste avant ? Fusionner les espaces.
            let last_is_break = tb
                .words
                .last()
                .is_some_and(|wd| wd.content.r#type == content_type::BREAK);
            if last_is_break {
                let last_word = tb.words.len() - 1;
                let cur = match tb.words[last_word].content.data {
                    crate::types::ContentData::BreakSpace(bs) => bs,
                    _ => 0,
                };
                let new_space = cur.max(space);
                tb.words[last_word].content.data = crate::types::ContentData::BreakSpace(new_space);
                if let Some(last_line) = tb.lines.last_mut() {
                    last_line.break_space = new_space
                        .max(last_line.margin_descent - last_line.border_descent)
                        .max(last_line.break_space);
                }
                return;
            }

            let word_no = l.tb_add_word(w, tb, 0, 0, 0, 0, style);
            if let Some(word) = tb.words.get_mut(word_no as usize) {
                word.content.r#type = content_type::BREAK;
                word.badness_and_penalty.set_penalty(PENALTY_FORCE_BREAK);
                word.content.data = crate::types::ContentData::BreakSpace(space);
            }
            l.tb_break_added(tb);
            l.tb_process_word(w, tb, word_no);
        });
    }

    /// `addLinebreak` (textblock.cc:2834-2866) — un `<BR>` en ligne
    /// vide prend la hauteur de la fonte courante (verbatim).
    pub fn tb_add_linebreak(&mut self, w: WidgetId, style: &Rc<Style>) {
        self.with_tb(w, |l, tb| {
            let last_is_break = tb
                .words
                .last()
                .is_some_and(|wd| wd.content.r#type == content_type::BREAK);
            let (ascent, descent) = if tb.words.is_empty() || last_is_break {
                let f = style.attrs.font.clone().unwrap_or_else(unreachable_font);
                (f.ascent, f.descent)
            } else {
                (0, 0)
            };
            let word_no = l.tb_add_word(w, tb, 0, ascent, descent, 0, style);
            if let Some(word) = tb.words.get_mut(word_no as usize) {
                word.content.r#type = content_type::BREAK;
                word.badness_and_penalty.set_penalty(PENALTY_FORCE_BREAK);
                word.content.data = crate::types::ContentData::BreakSpace(0);
            }
            l.tb_break_added(tb);
            l.tb_process_word(w, tb, word_no);
        });
    }

    /// `breakAdded` (textblock.cc:2867-2889) — l'espace avant un
    /// break est supprimé (verbatim : un `<input width:100%>` suivi
    /// d'un `<button>` ne doit pas dépasser la ligne).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_break_added(&mut self, tb: &mut TextblockData) {
        debug_assert!(!tb.words.is_empty());
        debug_assert!(tb
            .words
            .last()
            .is_some_and(|wd| wd.content.r#type == content_type::BREAK));
        if tb.words.len() >= 2 {
            let n = tb.words.len() - 2;
            tb.words[n].orig_space = 0;
            tb.words[n].eff_space = 0;
        }
    }

    /// `handOverBreak` (textblock.cc:2956-2976) — passe l'espace de
    /// fin de ligne au textblock parent s'il n'est pas `block`.
    pub fn tb_hand_over_break(&mut self, w: WidgetId, style: &Rc<Style>) {
        self.with_tb(w, |l, tb| {
            if tb.lines.is_empty() {
                return;
            }
            let break_space = tb.lines.last().map_or(0, |ln| ln.break_space);
            if break_space == 0 {
                return;
            }
            let Some(parent) = l.widget_parent(w) else {
                return;
            };
            let is_tb = l.node(parent, |n| matches!(n.kind, WidgetKind::Textblock { .. }));
            let display_block = l
                .widget_style(parent)
                .is_some_and(|s| s.attrs.display == DisplayType::Block);
            if is_tb && !display_block {
                l.tb_add_parbreak(parent, break_space, style);
            }
        });
    }

    /// `flush` (textblock.cc:2977-2994) — les mots ajoutés ne sont
    /// pas immédiatement dessinés ; `flush` déclenche le
    /// `queueResize` différé (`mustQueueResize` — verbatim).
    pub fn tb_flush(&mut self, w: WidgetId) {
        self.with_tb(w, |l, tb| {
            if tb.must_queue_resize {
                l.widget_queue_resize(w, -1, true);
                tb.must_queue_resize = false;
            }
        });
    }

    /// `changeLinkColor` (textblock.cc:2995-3045) — recolore les
    /// mots/widgets dont `x_link == link`, redessine les lignes
    /// touchées (verbatim).
    pub fn tb_change_link_color(&mut self, w: WidgetId, link: i32, new_color: i32) {
        self.with_tb(w, |l, tb| {
            for line_index in 0..tb.lines.len() {
                let mut changed = false;
                let (first, last) = {
                    let line = &tb.lines[line_index];
                    (line.first_word, line.last_word)
                };
                for word_idx in first.max(0)..=last {
                    let Some(word) = tb.words.get(word_idx as usize) else {
                        continue;
                    };
                    if word.style.attrs.x_link != link {
                        continue;
                    }
                    if word.content.r#type == content_type::TEXT {
                        // Nouveau style (couleur changée) pour
                        // `style` et `spaceStyle` — verbatim.
                        let mut attrs = word.style.attrs.clone();
                        let c = l.platform.borrow_mut().create_color(new_color);
                        attrs.color = Some(c);
                        let new_style = Style::create(&attrs);
                        let mut attrs = word.space_style.attrs.clone();
                        let c = l.platform.borrow_mut().create_color(new_color);
                        attrs.color = Some(c);
                        let new_space_style = Style::create(&attrs);
                        let word = &mut tb.words[word_idx as usize];
                        word.style = new_style;
                        word.space_style = new_space_style;
                    } else if word.content.r#type == content_type::WIDGET_IN_FLOW {
                        if let ContentData::Widget(child) = word.content.data {
                            if let Some(ws) = l.widget_style(child) {
                                let mut attrs = ws.attrs.clone();
                                let c = l.platform.borrow_mut().create_color(new_color);
                                attrs.color = Some(c);
                                let bc = l.platform.borrow_mut().create_color(new_color);
                                attrs.set_border_color(Some(bc));
                                let ns = Style::create(&attrs);
                                l.widget_set_style(child, &ns);
                            }
                        }
                    }
                    changed = true;
                }
                if changed {
                    let (y, h) = {
                        let line = &tb.lines[line_index];
                        let alloc = l.node(w, |n| n.allocation);
                        (
                            tb.line_y_offset_widget(line, &alloc),
                            line.border_ascent + line.border_descent,
                        )
                    };
                    let aw = l.node(w, |n| n.allocation.width);
                    l.widget_queue_draw_area(w, 0, y, aw, h);
                }
            }
        });
    }

    /// `changeWordStyle` (textblock.cc:3046-3049) — corps **vide**
    /// amont (verbatim).
    pub fn tb_change_word_style(
        &mut self,
        _w: WidgetId,
        _from: i32,
        _to: i32,
        _style: &Rc<Style>,
        _include_first_space: bool,
        _include_last_space: bool,
    ) {
    }

    /// `removeChild` (textblock.cc:1172-1175) — `\bug Not
    /// implemented` amont (verbatim no-op).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_remove_child(&mut self, _w: WidgetId, _child: WidgetId) {}

    // ----------------------------------------------------------------
    // Événements (textblock.cc:944-1170).
    // ----------------------------------------------------------------

    /// `buttonPressImpl` (textblock.cc:944-947).
    pub(crate) fn tb_button_press_impl(
        &mut self,
        w: WidgetId,
        event: &crate::events::EventButton,
    ) -> bool {
        self.tb_send_selection_event(
            w,
            crate::layout::SelectionEvent::ButtonPress,
            crate::selection::SelectionMouseEvent::Button(event),
        )
    }

    /// `buttonReleaseImpl` (textblock.cc:949-952).
    pub(crate) fn tb_button_release_impl(
        &mut self,
        w: WidgetId,
        event: &crate::events::EventButton,
    ) -> bool {
        self.tb_send_selection_event(
            w,
            crate::layout::SelectionEvent::ButtonRelease,
            crate::selection::SelectionMouseEvent::Button(event),
        )
    }

    /// `motionNotifyImpl` (textblock.cc:959-999) — BUTTON1 →
    /// sélection ; sinon hover lien/tooltip/curseur (verbatim).
    pub(crate) fn tb_motion_notify_impl(
        &mut self,
        w: WidgetId,
        event: &crate::events::EventMotion,
    ) -> bool {
        if event.state & crate::events::button_state::BUTTON1_MASK != 0 {
            self.tb_send_selection_event(
                w,
                crate::layout::SelectionEvent::ButtonMotion,
                crate::selection::SelectionMouseEvent::Motion(event),
            )
        } else {
            let mut in_space = false;
            let link_old = self.node_tb(w, |tb| tb.hover_link).unwrap_or(-1);
            let tooltip_old = HOVER_TOOLTIP.with(|t| t.borrow().clone());
            let word_index = self
                .with_tb(w, |l, tb| {
                    l.tb_find_word(w, tb, event.x_widget, event.y_widget, &mut in_space)
                })
                .flatten();

            let (cursor, hover_link, hover_tooltip) = match word_index {
                None => (
                    self.widget_style(w)
                        .map_or(crate::style::Cursor::Default, |s| s.attrs.cursor),
                    -1,
                    None,
                ),
                Some(wi) => {
                    let (style, space_style) = self
                        .node_tb(w, |tb| {
                            tb.words
                                .get(wi as usize)
                                .map(|word| (word.style.clone(), word.space_style.clone()))
                        })
                        .flatten()
                        .unwrap_or_else(|| (null_style(), null_style()));
                    let st = if in_space { space_style } else { style };
                    (st.attrs.cursor, st.attrs.x_link, st.attrs.x_tooltip.clone())
                }
            };
            self.set_cursor(cursor);
            self.node_tb_mut(w, |tb| tb.hover_link = hover_link);
            HOVER_TOOLTIP.with(|t| (*t.borrow_mut()).clone_from(&hover_tooltip));

            // Tooltip show/hide (verbatim — comparaison de
            // pointeurs ≙ `Rc::ptr_eq` + `None`-ness).
            let same_tooltip = match (&tooltip_old, &hover_tooltip) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            if !same_tooltip {
                if let Some(t) = &tooltip_old {
                    t.on_leave();
                }
                if let Some(t) = &hover_tooltip {
                    t.on_enter();
                }
            } else if let Some(t) = &hover_tooltip {
                t.on_motion();
            }

            if hover_link == link_old {
                hover_link != -1
            } else {
                // LinkEnter avec -1 = LinkLeave (verbatim).
                self.emit_link_enter(Some(w), hover_link, -1, -1, -1)
            }
        }
    }

    /// `enterNotifyImpl` (textblock.cc:1001-1006) — `hoverLink = -2`
    /// pour détecter le prochain `LinkEnter` (verbatim).
    pub(crate) fn tb_enter_notify_impl(
        &mut self,
        w: WidgetId,
        _event: &crate::events::EventCrossing,
    ) {
        self.node_tb_mut(w, |tb| tb.hover_link = -2);
    }

    /// `leaveNotifyImpl` (textblock.cc:1008-1023) — quitte le
    /// viewport : émet LinkEnter(-1), ferme le tooltip (verbatim).
    pub(crate) fn tb_leave_notify_impl(
        &mut self,
        w: WidgetId,
        _event: &crate::events::EventCrossing,
    ) {
        if self.node_tb(w, |tb| tb.hover_link).unwrap_or(-1) >= 0 {
            self.emit_link_enter(Some(w), -1, -1, -1, -1);
        }
        let t = HOVER_TOOLTIP.with(|t| t.borrow_mut().take());
        if let Some(t) = t {
            t.on_leave();
        }
    }

    /// `sendSelectionEvent` (textblock.cc:1025-1170) — calcule
    /// `wordIndex`/`charPos`/`link` depuis la position du pointeur
    /// puis délègue à `selectionHandleEvent` (stub phase
    /// sélection → `false`, cf. layout.rs).
    pub(crate) fn tb_send_selection_event(
        &mut self,
        w: WidgetId,
        event_type: crate::layout::SelectionEvent,
        sel_event: crate::selection::SelectionMouseEvent<'_>,
    ) -> bool {
        let event = &sel_event.position_event();
        let Some((word_index, char_pos, link)) = self.with_tb(w, |l, tb| {
            let mut word_index;
            let mut char_pos = 0;
            let mut link = -1;
            let alloc = l.node(w, |n| n.allocation);
            if tb.words.is_empty() {
                word_index = -1;
            } else {
                let last_line_i = tb.lines.len() - 1;
                let last_line = &tb.lines[last_line_i];
                let y_first = tb.line_y_offset_canvas(&tb.lines[0], &alloc);
                let y_last = tb.line_y_offset_canvas(last_line, &alloc)
                    + last_line.border_ascent
                    + last_line.border_descent;
                if event.y_canvas < y_first {
                    word_index = 0;
                } else if event.y_canvas >= y_last {
                    word_index = tb.words.len() as i32 - 1;
                    char_pos = SELECTION_END_OF_WORD;
                } else {
                    let line_index = l.tb_find_line_index_when_allocated(w, tb, event.y_widget);
                    let line = &tb.lines[line_index as usize];
                    let (f, lword) = (line.first_word, line.last_word);
                    if event.y_widget
                        > tb.line_y_offset_widget(line, &alloc)
                            + line.border_ascent
                            + line.border_descent
                    {
                        word_index = lword;
                        char_pos = SELECTION_END_OF_WORD;
                    } else if event.x_widget < line.text_offset {
                        word_index = f;
                    } else {
                        let mut next_word_start_x = line.text_offset;
                        word_index = lword + 1;
                        for wi in f..=lword {
                            let word = &tb.words[wi as usize];
                            let word_start_x = next_word_start_x;
                            next_word_start_x += word.size.width + i32::from(word.eff_space);
                            if event.x_widget >= word_start_x && event.x_widget < next_word_start_x
                            {
                                word_index = wi;
                                let y_widget_base =
                                    tb.line_y_offset_widget(line, &alloc) + line.border_ascent;
                                if event.x_widget >= next_word_start_x - i32::from(word.eff_space) {
                                    char_pos = SELECTION_END_OF_WORD;
                                    if wi < lword
                                        && tb.words[wi as usize + 1].content.r#type
                                            != content_type::BREAK
                                        && event.y_widget
                                            <= y_widget_base + word.space_style_font_descent()
                                        && event.y_widget
                                            > y_widget_base - word.space_style_font_ascent()
                                    {
                                        link = word.space_style.attrs.x_link;
                                    }
                                } else {
                                    if event.y_widget <= y_widget_base + word.size.descent
                                        && event.y_widget > y_widget_base - word.size.ascent
                                    {
                                        link = word.style.attrs.x_link;
                                    }
                                    if word.content.r#type == content_type::TEXT {
                                        let mut glyph_x = word_start_x;
                                        let is_start_word = word.flags & WORD_START != 0;
                                        let is_end_word = word.flags & WORD_END != 0;
                                        let text = match &word.content.data {
                                            ContentData::Text(t) => t.clone(),
                                            _ => Vec::new(),
                                        };
                                        loop {
                                            let next_char_pos =
                                                l.platform.borrow_mut().next_glyph(&text, char_pos);
                                            let glyph_width = l.tb_text_width(
                                                &text,
                                                char_pos as usize,
                                                (next_char_pos - char_pos) as usize,
                                                &word.style,
                                                is_start_word && char_pos == 0,
                                                is_end_word
                                                    && *text
                                                        .get(next_char_pos as usize)
                                                        .unwrap_or(&0)
                                                        == 0,
                                            );
                                            if event.x_widget > glyph_x + glyph_width {
                                                glyph_x += glyph_width;
                                                char_pos = next_char_pos;
                                                continue;
                                            }
                                            if event.x_widget >= glyph_x + glyph_width / 2 {
                                                // Moitié droite du
                                                // caractère — cherche
                                                // les caractères
                                                // combinant (verbatim).
                                                char_pos = next_char_pos;
                                                while *text.get(char_pos as usize).unwrap_or(&0)
                                                    != 0
                                                {
                                                    let ncp = l
                                                        .platform
                                                        .borrow_mut()
                                                        .next_glyph(&text, char_pos);
                                                    if l.tb_text_width(
                                                        &text,
                                                        char_pos as usize,
                                                        (ncp - char_pos) as usize,
                                                        &word.style,
                                                        is_start_word && char_pos == 0,
                                                        is_end_word
                                                            && *text
                                                                .get(ncp as usize)
                                                                .unwrap_or(&0)
                                                                == 0,
                                                    ) != 0
                                                    {
                                                        break;
                                                    }
                                                    char_pos = ncp;
                                                }
                                            }
                                            break;
                                        }
                                    } else if event.x_widget
                                        >= (word_start_x + next_word_start_x) / 2
                                    {
                                        char_pos = SELECTION_END_OF_WORD;
                                    }
                                }
                                break;
                            }
                        }
                        if word_index > lword {
                            // À droite de la ligne — dernier mot.
                            word_index = lword;
                            char_pos = SELECTION_END_OF_WORD;
                        }
                    }
                }
            }
            (word_index, char_pos, link)
        }) else {
            return false;
        };
        // `TextblockIterator::createWordIndexIterator(this,
        // Content::maskForSelection(true), wordIndex)` +
        // `selectionHandleEvent` (textblock.cc:1164-1169) — verbatim.
        let it: Box<dyn crate::iterator::DwIterator> =
            Box::new(crate::textblock::iter::textblock_word_index_iterator(
                self,
                w,
                crate::types::Content::mask_for_selection(true),
                word_index,
            ));
        self.selection_state_handle_event(event_type, it, char_pos, link, sel_event)
    }
}

/// `font` manquant ≙ `style->font` NULL amont (contrat : un style
/// utilisé pour du texte a toujours une fonte — les mocks de test
/// la fournissent). Retourne une fonte partagée zéro-plutôt que de
/// paniquer (≙ comportement indéfini amont, port reste sain).
pub(crate) fn unreachable_font() -> Rc<crate::style::Font> {
    thread_local! {
        static F: Rc<crate::style::Font> = Rc::new(crate::style::Font {
            attrs: crate::style::FontAttrs {
                name: Vec::new(),
                size: 0,
                weight: 0,
                letter_spacing: 0,
                font_variant: crate::style::FontVariant::Normal,
                style: crate::style::FontStyle::Normal,
            },
            ascent: 0,
            descent: 0,
            space_width: 0,
            x_height: 0,
            zero_width: 0,
        });
    }
    F.with(std::clone::Clone::clone)
}

impl Word {
    /// `word->spaceStyle->font->ascent` — `0` si pas de fonte
    /// (verbatim déréférencement ; borné).
    fn space_style_font_ascent(&self) -> i32 {
        self.space_style.attrs.font.as_ref().map_or(0, |f| f.ascent)
    }
    /// `word->spaceStyle->font->descent`.
    fn space_style_font_descent(&self) -> i32 {
        self.space_style
            .attrs
            .font
            .as_ref()
            .map_or(0, |f| f.descent)
    }
}
