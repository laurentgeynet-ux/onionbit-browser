//! `textblock.cc` — dessin, décorations, hit-testing, événements.
//!
//! Lookups de lignes/mots (`findLineIndex*`, `findLineOfWord`,
//! `findWord`) puis `drawLevel`/`drawLine`/`drawWord`/`drawSpace`/
//! `drawText`/`decorateText` et la gestion des événements.

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use std::rc::Rc;

use dillo_common::dlib::is_punct;

use super::words::unreachable_font;
use super::{
    TextblockData, HYPHEN_DRAW_CHAR, WORD_DIV_CHAR_AT_EOL, WORD_DRAW_AS_ONE_TEXT, WORD_START,
};
use crate::layout::Layout;
use crate::oof::sl;
use crate::style::{text_decoration, Shading, Style, TextTransform, VAlignType};
use crate::types::{content_type, Allocation, ContentData, Rectangle, HIGHLIGHT_NUM_LAYERS};
use crate::view::View;
use crate::WidgetId;

impl Layout {
    /// `findLineIndex (y)` (textblock.cc:1573-1577) — dispatch
    /// allocated/non-allocated. API amont sans appelant interne
    /// pour l'instant (verbatim).
    #[allow(dead_code)]
    pub(crate) fn tb_find_line_index(&self, w: WidgetId, tb: &TextblockData, y: i32) -> i32 {
        if self.node(w, super::super::widget::WidgetNode::was_allocated) {
            self.tb_find_line_index_when_allocated(w, tb, y)
        } else {
            self.tb_find_line_index_when_not_allocated(w, tb, y)
        }
    }

    /// `findLineIndexWhenNotAllocated` (textblock.cc:1579-1591)
    /// — atteint via `findLineIndex` uniquement (verbatim).
    #[allow(dead_code)]
    fn tb_find_line_index_when_not_allocated(
        &self,
        w: WidgetId,
        tb: &TextblockData,
        y: i32,
    ) -> i32 {
        if tb.lines.is_empty() {
            return -1;
        }
        let style = self.widget_style(w);
        let (pad_top, border_top, margin_top) = style.as_ref().map_or((0, 0, 0), |s| {
            (
                s.attrs.padding.top,
                s.attrs.border_width.top,
                s.attrs.margin.top,
            )
        });
        let extra_top = self.node(w, |n| n.extra_space.top);
        let first = &tb.lines[0];
        let ascent = Self::tb_calc_vertical_border(
            pad_top,
            border_top,
            margin_top + extra_top,
            first.border_ascent,
            first.margin_ascent,
        );
        self.tb_find_line_index_at(tb, y, ascent)
    }

    /// `findLineIndexWhenAllocated` (textblock.cc:1593-1598) —
    /// `assert (wasAllocated)` → `debug_assert`.
    pub(crate) fn tb_find_line_index_when_allocated(
        &self,
        w: WidgetId,
        tb: &TextblockData,
        y: i32,
    ) -> i32 {
        debug_assert!(self.node(w, super::super::widget::WidgetNode::was_allocated));
        let ascent = self.node(w, |n| n.allocation.ascent);
        self.tb_find_line_index_at(tb, y, ascent)
    }

    /// `findLineIndex (y, ascent)` (textblock.cc:1600-1638) —
    /// recherche par pas binaires ; l'espace *sous* la ligne fait
    /// partie de la ligne (commentaire verbatim `Dw_page_find_link`).
    fn tb_find_line_index_at(&self, tb: &TextblockData, y: i32, ascent: i32) -> i32 {
        let _ = self;
        // Seul `ascent` est lu (verbatim).
        let alloc = Allocation {
            ascent,
            ..Allocation::default()
        };

        let max_index = tb.lines.len() as i32 - 1;
        let mut low = 0i32;
        let mut step = (tb.lines.len() as i32 + 1) >> 1;
        while step > 1 {
            let index = low + step;
            if index <= max_index && tb.line_y_offset_widget_i(index, &alloc) <= y {
                low = index;
            }
            step = (step + 1) >> 1;
        }
        if low < max_index && tb.line_y_offset_widget_i(low + 1, &alloc) <= y {
            low += 1;
        }
        low
    }

    /// `findLineOfWord` (textblock.cc:1640-1660) — recherche
    /// binaire ; `-1` pour les mots hors des lignes existantes
    /// (les lignes pas encore créées comptent — verbatim).
    pub(crate) fn tb_find_line_of_word(&self, tb: &TextblockData, word_index: i32) -> i32 {
        let _ = self;
        if word_index < 0
            || word_index >= tb.words.len() as i32
            || tb.lines.is_empty()
            || word_index > tb.lines.last().map_or(-1, |l| l.last_word)
        {
            return -1;
        }
        let mut low = 0usize;
        let mut high = tb.lines.len() - 1;
        loop {
            let index = usize::midpoint(low, high);
            let line = &tb.lines[index];
            if word_index >= line.first_word {
                if word_index <= line.last_word {
                    return index as i32;
                }
                low = index + 1;
            } else {
                if index == 0 {
                    return -1;
                }
                high = index - 1;
            }
        }
    }

    /// `findWord` (textblock.cc:1688-1724) — mot sous le pointeur
    /// en coords widget, ou `None` ; `in_space` vrai si le corps
    /// ou l'espace du mot contient le point (corps testé sur la
    /// hauteur du mot, espace sur celle de la fonte — verbatim).
    pub(crate) fn tb_find_word(
        &self,
        w: WidgetId,
        tb: &TextblockData,
        x: i32,
        y: i32,
        in_space: &mut bool,
    ) -> Option<i32> {
        *in_space = false;
        let line_index = self.tb_find_line_index_when_allocated(w, tb, y);
        if line_index >= tb.lines.len() as i32 {
            return None;
        }
        let alloc = self.node(w, |n| n.allocation);
        let line = &tb.lines[line_index.max(0) as usize];
        let y_widget_base = tb.line_y_offset_widget(line, &alloc) + line.border_ascent;
        if y_widget_base + line.border_descent <= y {
            return None;
        }

        let mut x_cursor = line.text_offset;
        for word_index in line.first_word..=line.last_word.max(line.first_word - 1) {
            let word = &tb.words[word_index as usize];
            let last_x_cursor = x_cursor;
            x_cursor += word.size.width + i32::from(word.eff_space);
            if last_x_cursor <= x && x_cursor > x {
                if x >= x_cursor - i32::from(word.eff_space) {
                    // Zone de l'espace de queue (verbatim —
                    // hauteur de la fonte de l'espace, mot
                    // suivant non-BREAK).
                    let space_font = word.space_style.attrs.font.clone();
                    if word_index < line.last_word
                        && tb.words[word_index as usize + 1].content.r#type != content_type::BREAK
                        && y > y_widget_base - space_font.as_ref().map_or(0, |f| f.ascent)
                        && y <= y_widget_base + space_font.as_ref().map_or(0, |f| f.descent)
                    {
                        *in_space = true;
                        return Some(word_index);
                    }
                } else {
                    // Corps du mot — hauteur propre au mot.
                    if y > y_widget_base - word.size.ascent
                        && y <= y_widget_base + word.size.descent
                    {
                        return Some(word_index);
                    }
                }
                break;
            }
        }
        None
    }
}

// ------------------------------------------------------------------
// Dessin — textblock.cc:1185-1571, 1731-1775.
// ------------------------------------------------------------------

impl Layout {
    /// `decorateText` (textblock.cc:1185-1213) — underline /
    /// overline / line-through, positions verbatim en fractions de
    /// `font->ascent`/`descent`/`xHeight`.
    fn tb_decorate_text(
        view: &mut dyn View,
        style: &Style,
        shading: Shading,
        x: i32,
        y_base: i32,
        width: i32,
    ) {
        let font = style.attrs.font.clone().unwrap_or_else(unreachable_font);
        let Some(color) = style.attrs.color else {
            return;
        };
        let height = 1 + font.x_height / 12;
        if style.attrs.text_decoration & text_decoration::UNDERLINE != 0 {
            let y = y_base + font.descent / 3;
            view.draw_rectangle(&color, shading, true, x, y, width, height);
        }
        if style.attrs.text_decoration & text_decoration::OVERLINE != 0 {
            let y = y_base - font.ascent;
            view.draw_rectangle(&color, shading, true, x, y, width, height);
        }
        if style.attrs.text_decoration & text_decoration::LINE_THROUGH != 0 {
            let y = y_base + (font.descent - font.ascent) / 2 + font.descent / 4;
            view.draw_rectangle(&color, shading, true, x, y, width, height);
        }
    }

    /// `drawText` (textblock.cc:1215-1280) — applique
    /// `text-transform` puis `view->drawText`. `is_start`/`is_end`
    /// repèrent les bornes du mot logique (césure) — verbatim.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn tb_draw_text(
        &mut self,
        view: &mut dyn View,
        style: &Style,
        shading: Shading,
        x: i32,
        y: i32,
        text: &[u8],
        start: usize,
        len: usize,
        is_start: bool,
        _is_end: bool,
    ) {
        if len == 0 {
            return;
        }
        let mut drawn: Option<Vec<u8>> = None;
        match style.attrs.text_transform {
            TextTransform::None => {}
            TextTransform::Uppercase => {
                let mut p = self.platform.borrow_mut();
                drawn = Some(p.text_to_upper(&text[start..start + len]));
            }
            TextTransform::Lowercase => {
                let mut p = self.platform.borrow_mut();
                drawn = Some(p.text_to_lower(&text[start..start + len]));
            }
            TextTransform::Capitalize => {
                // `isStart == false` : le premier caractère de ce
                // tronçon n'est pas le début du mot logique → pas
                // de transformation (verbatim).
                if is_start {
                    // `\bug` amont : ponctuation non-ASCII non
                    // reconnue (verbatim).
                    let mut initial_seen = false;
                    for &c in &text[..start] {
                        if !is_punct(c) {
                            initial_seen = true;
                        }
                    }
                    if !initial_seen {
                        let sub = &text[start..start + len];
                        let mut after = 0usize;
                        while after < sub.len() && is_punct(sub[after]) {
                            after += 1;
                        }
                        if after < sub.len() && sub[after] != 0 {
                            let mut p = self.platform.borrow_mut();
                            after = p.next_glyph(sub, after as i32) as usize;
                        }
                        if after > len {
                            after = len;
                        }
                        let initial = {
                            let mut p = self.platform.borrow_mut();
                            p.text_to_upper(&sub[..after])
                        };
                        let mut s = initial;
                        s.extend_from_slice(&sub[after..]);
                        drawn = Some(s);
                    }
                }
            }
        }
        let font = style.attrs.font.clone().unwrap_or_else(unreachable_font);
        let Some(color) = style.attrs.color else {
            return;
        };
        match &drawn {
            Some(s) => view.draw_text(&font, &color, shading, x, y, s),
            None => view.draw_text(&font, &color, shading, x, y, &text[start..start + len]),
        }
    }

    /// `drawWord` (textblock.cc:1282-1335) — les mots césurés sont
    /// dessinés d'un bloc (`DRAW_AS_ONE_TEXT` — kerning verbatim) ;
    /// le tiret `hyphenDrawChar` est concaténé pour le dernier mot
    /// `DIV_CHAR_AT_EOL` de la ligne.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn tb_draw_word(
        &mut self,
        w: WidgetId,
        tb: &TextblockData,
        line_index: usize,
        word_index1: usize,
        word_index2: usize,
        view: &mut dyn View,
        area: &Rectangle,
        x_widget: i32,
        y_widget_base: i32,
    ) {
        let Some(word1) = tb.words.get(word_index1) else {
            return;
        };
        let style = word1.style.clone();
        let line = &tb.lines[line_index];
        let draw_hyphen = word_index2 as i32 == line.last_word
            && tb
                .words
                .get(word_index2)
                .is_some_and(|wd| wd.flags & WORD_DIV_CHAR_AT_EOL != 0);

        if style.has_background() {
            let mut width = 0;
            for i in word_index1..=word_index2 {
                if let Some(wd) = tb.words.get(i) {
                    width += wd.size.width;
                }
            }
            width += i32::from(tb.words.get(word_index2).map_or(0, |wd| wd.hyphen_width));
            self.widget_draw_box(
                w,
                view,
                &style,
                area,
                x_widget,
                y_widget_base - line.border_ascent,
                width,
                line.border_ascent + line.border_descent,
                false,
            );
        }

        if word_index1 == word_index2 && !draw_hyphen {
            // Cas simple — pas de copie dans un buffer (verbatim).
            let word = &tb.words[word_index1];
            if let ContentData::Text(text) = &word.content.data {
                let text = text.clone();
                self.tb_draw_word0(
                    w,
                    tb,
                    word_index1,
                    word_index2,
                    &text,
                    word.size.width,
                    false,
                    &style,
                    view,
                    area,
                    x_widget,
                    y_widget_base,
                );
            }
        } else {
            // Concaténer tous les mots (verbatim — kerning
            // préservé par le dessin en un seul buffer).
            let mut text = Vec::new();
            let mut total_width = 0;
            for i in word_index1..=word_index2 {
                if let Some(wd) = tb.words.get(i) {
                    if let ContentData::Text(t) = &wd.content.data {
                        text.extend_from_slice(t);
                    }
                    total_width += wd.size.width;
                }
            }
            if draw_hyphen {
                text.extend_from_slice(HYPHEN_DRAW_CHAR);
            }
            self.tb_draw_word0(
                w,
                tb,
                word_index1,
                word_index2,
                &text,
                total_width,
                draw_hyphen,
                &style,
                view,
                area,
                x_widget,
                y_widget_base,
            );
        }
    }

    /// `drawWord0` (textblock.cc:1337-1439) — dessine le texte puis
    /// les couches de surbrillance `hlStart`/`hlEnd` (verbatim —
    /// `isEndTotal` lit le flag `WORD_START`, quirk amont).
    #[allow(clippy::too_many_arguments)]
    fn tb_draw_word0(
        &mut self,
        w: WidgetId,
        tb: &TextblockData,
        word_index1: usize,
        word_index2: usize,
        text: &[u8],
        total_width: i32,
        draw_hyphen: bool,
        style: &Style,
        view: &mut dyn View,
        _area: &Rectangle,
        x_widget: i32,
        y_widget_base: i32,
    ) {
        let alloc = self.node(w, |n| n.allocation);
        let x_world = alloc.x + x_widget;

        // Baseline ajustée `<SUB>`/`<SUP>` (verbatim).
        let font = style.attrs.font.clone().unwrap_or_else(unreachable_font);
        let mut y_widget_base = y_widget_base;
        if style.attrs.valign == VAlignType::Sub {
            y_widget_base += font.ascent / 3;
        } else if style.attrs.valign == VAlignType::Super {
            y_widget_base -= font.ascent / 2;
        }
        let y_world_base = y_widget_base + alloc.y;

        let is_start_total = tb
            .words
            .get(word_index1)
            .is_some_and(|wd| wd.flags & WORD_START != 0);
        // Verbatim : `isEndTotal` lit `WORD_START` (pas `WORD_END`)
        // — quirk amont conservé.
        let is_end_total = tb
            .words
            .get(word_index2)
            .is_some_and(|wd| wd.flags & WORD_START != 0);
        self.tb_draw_text(
            view,
            style,
            Shading::Normal,
            x_world,
            y_world_base,
            text,
            0,
            text.len(),
            is_start_total,
            is_end_total,
        );

        if style.attrs.text_decoration != 0 {
            Self::tb_decorate_text(
                view,
                style,
                Shading::Normal,
                x_world,
                y_world_base,
                total_width,
            );
        }

        for layer in 0..HIGHLIGHT_NUM_LAYERS {
            let (hl_start, hl_end) = (tb.hl_start[layer], tb.hl_end[layer]);
            if word_index1 as i32 <= hl_end.index && word_index2 as i32 >= hl_start.index {
                let word_len = text.len() as i32;

                let mut first_char_idx = if hl_start.index < word_index1 as i32 {
                    0
                } else {
                    let mut idx = tb
                        .words
                        .get(hl_start.index as usize)
                        .and_then(|wd| match &wd.content.data {
                            ContentData::Text(t) => Some(t.len() as i32),
                            _ => None,
                        })
                        .map_or(0, |l| hl_start.n_char.min(l));
                    // Mots TEXT entre wordIndex1 et hlStart
                    // (verbatim — tous TEXT).
                    for i in word_index1..hl_start.index.max(0) as usize {
                        if let Some(ContentData::Text(t)) =
                            tb.words.get(i).map(|wd| &wd.content.data)
                        {
                            idx += t.len() as i32;
                        }
                    }
                    idx
                };

                let mut last_char_idx = if hl_end.index > word_index2 as i32 {
                    word_len
                } else {
                    let mut idx = tb
                        .words
                        .get(hl_end.index as usize)
                        .and_then(|wd| match &wd.content.data {
                            ContentData::Text(t) => Some(t.len() as i32),
                            _ => None,
                        })
                        .map_or(0, |l| hl_end.n_char.min(l));
                    for i in word_index1..hl_end.index.max(0) as usize {
                        if let Some(ContentData::Text(t)) =
                            tb.words.get(i).map(|wd| &wd.content.data)
                        {
                            idx += t.len() as i32;
                        }
                    }
                    idx
                };
                first_char_idx = first_char_idx.clamp(0, word_len);
                last_char_idx = last_char_idx.clamp(0, word_len);

                let mut x_start = x_world;
                if first_char_idx != 0 {
                    x_start += self.tb_text_width(
                        text,
                        0,
                        first_char_idx as usize,
                        style,
                        is_start_total,
                        is_end_total && text.get(first_char_idx as usize).is_none(),
                    );
                }
                // Sans tiret et couverture totale, `totalWidth`
                // est réutilisé (optimisation verbatim — avec un
                // tiret le texte est plus long).
                let width = if !draw_hyphen && first_char_idx == 0 && last_char_idx == word_len {
                    total_width
                } else {
                    self.tb_text_width(
                        text,
                        first_char_idx as usize,
                        (last_char_idx - first_char_idx) as usize,
                        style,
                        is_start_total && first_char_idx == 0,
                        is_end_total && text.get(last_char_idx as usize).is_none(),
                    )
                };
                if width > 0 {
                    // Fond du texte surligné : `backgroundColor` du
                    // style ou fond du widget (verbatim).
                    let word_bg_color = style
                        .attrs
                        .background_color
                        .or_else(|| self.widget_get_bg_color(w));
                    if let Some(bg) = word_bg_color {
                        view.draw_rectangle(
                            &bg,
                            Shading::Inverse,
                            true,
                            x_start,
                            y_world_base - font.ascent,
                            width,
                            font.ascent + font.descent,
                        );
                    }

                    // Texte surligné.
                    self.tb_draw_text(
                        view,
                        style,
                        Shading::Inverse,
                        x_start,
                        y_world_base,
                        text,
                        first_char_idx as usize,
                        (last_char_idx - first_char_idx) as usize,
                        is_start_total && first_char_idx == 0,
                        is_end_total && last_char_idx == word_len,
                    );

                    if style.attrs.text_decoration != 0 {
                        Self::tb_decorate_text(
                            view,
                            style,
                            Shading::Inverse,
                            x_start,
                            y_world_base,
                            width,
                        );
                    }
                }
            }
        }
    }

    /// `drawSpace` (textblock.cc:1441-1479) — baseline
    /// `<SUB>`/`<SUP>`, surbrillance `hlStart/hlEnd`, décoration
    /// verbatim.
    #[allow(clippy::too_many_arguments)] // signatures verbatim amont.
    pub(crate) fn tb_draw_space(
        &mut self,
        w: WidgetId,
        tb: &TextblockData,
        word_index: usize,
        view: &mut dyn View,
        _area: &Rectangle,
        x_widget: i32,
        y_widget_base: i32,
    ) {
        let Some(word) = tb.words.get(word_index) else {
            return;
        };
        let alloc = self.node(w, |n| n.allocation);
        let x_world = alloc.x + x_widget;
        let style = word.space_style.clone();
        let eff_space = i32::from(word.eff_space);

        let font = style.attrs.font.clone().unwrap_or_else(unreachable_font);
        let mut y_widget_base = y_widget_base;
        if style.attrs.valign == VAlignType::Sub {
            y_widget_base += font.ascent / 3;
        } else if style.attrs.valign == VAlignType::Super {
            y_widget_base -= font.ascent / 2;
        }
        let y_world_base = alloc.y + y_widget_base;

        let mut highlight = false;
        for layer in 0..HIGHLIGHT_NUM_LAYERS {
            if tb.hl_start[layer].index <= word_index as i32
                && tb.hl_end[layer].index > word_index as i32
            {
                highlight = true;
                break;
            }
        }
        if highlight {
            let space_bg_color = style
                .attrs
                .background_color
                .or_else(|| self.widget_get_bg_color(w));
            if let Some(bg) = space_bg_color {
                view.draw_rectangle(
                    &bg,
                    Shading::Inverse,
                    true,
                    x_world,
                    y_world_base - font.ascent,
                    eff_space,
                    font.ascent + font.descent,
                );
            }
        }
        if style.attrs.text_decoration != 0 {
            let shading = if highlight {
                Shading::Inverse
            } else {
                Shading::Normal
            };
            Self::tb_decorate_text(view, &style, shading, x_world, y_world_base, eff_space);
        }
    }

    /// `drawLine` (textblock.cc:1481-1571) — clipping `area`,
    /// widgets in-flow (`intersects` + `draw`), runs
    /// `DRAW_AS_ONE_TEXT` de même style dessinés d'un bloc, espaces
    /// (fond `spaceStyle` + `drawSpace`), `BREAK` final ignoré.
    pub(crate) fn tb_draw_line(
        &mut self,
        w: WidgetId,
        tb: &TextblockData,
        line_index: usize,
        view: &mut dyn View,
        area: &Rectangle,
        context: &mut crate::types::DrawingContext,
    ) {
        let alloc = self.node(w, |n| n.allocation);
        let Some(line) = tb.lines.get(line_index).cloned() else {
            return;
        };
        let mut x_widget = line.text_offset;
        let y_widget_base = tb.line_y_offset_widget(&line, &alloc) + line.border_ascent;

        let mut word_index = line.first_word;
        while word_index <= line.last_word && x_widget < area.x + area.width {
            let Some(word) = tb.words.get(word_index as usize) else {
                break;
            };
            let mut word_size = word.size.width;
            let ctype = word.content.r#type;
            let hyphen_width = i32::from(word.hyphen_width);
            let mut eff_space = i32::from(word.eff_space);
            let content = word.content.data.clone();
            let word_style = word.style.clone();
            let space_style = word.space_style.clone();

            if x_widget + word_size + hyphen_width + eff_space >= area.x
                && (ctype == content_type::TEXT || ctype == content_type::WIDGET_IN_FLOW)
            {
                if word_size > 0 {
                    if ctype == content_type::WIDGET_IN_FLOW {
                        if let ContentData::Widget(child) = content {
                            if !crate::oof::handled_by_stacking_context_mgr(self, child) {
                                if let Some(child_area) = self.widget_intersects(child, w, area) {
                                    self.w_draw(child, view, &child_area, context);
                                }
                            }
                        }
                    } else {
                        // Run `DRAW_AS_ONE_TEXT` de même style —
                        // dessiné d'un bloc (kerning verbatim).
                        let mut word_index2 = word_index;
                        while word_index2 < line.last_word
                            && tb
                                .words
                                .get(word_index2 as usize)
                                .is_some_and(|wd| wd.flags & WORD_DRAW_AS_ONE_TEXT != 0)
                            && tb
                                .words
                                .get(word_index2 as usize + 1)
                                .is_some_and(|wd| Rc::ptr_eq(&word_style, &wd.style))
                        {
                            word_index2 += 1;
                        }

                        self.tb_draw_word(
                            w,
                            tb,
                            line_index,
                            word_index as usize,
                            word_index2 as usize,
                            view,
                            area,
                            x_widget,
                            y_widget_base,
                        );
                        word_size = 0;
                        for i in word_index..=word_index2 {
                            if let Some(wd) = tb.words.get(i as usize) {
                                word_size += wd.size.width;
                            }
                        }
                        word_index = word_index2;
                        eff_space = tb
                            .words
                            .get(word_index as usize)
                            .map_or(0, |wd| i32::from(wd.eff_space));
                    }
                }

                if eff_space > 0
                    && word_index < line.last_word
                    && tb
                        .words
                        .get(word_index as usize + 1)
                        .is_some_and(|wd| wd.content.r#type != content_type::BREAK)
                {
                    let space_style2 = tb
                        .words
                        .get(word_index as usize)
                        .map_or_else(|| space_style.clone(), |wd| wd.space_style.clone());
                    if space_style2.has_background() {
                        self.widget_draw_box(
                            w,
                            view,
                            &space_style2,
                            area,
                            x_widget + word_size,
                            y_widget_base - line.border_ascent,
                            eff_space,
                            line.border_ascent + line.border_descent,
                            false,
                        );
                    }
                    self.tb_draw_space(
                        w,
                        tb,
                        word_index as usize,
                        view,
                        area,
                        x_widget + word_size,
                        y_widget_base,
                    );
                }
            }
            x_widget += word_size + eff_space;
            word_index += 1;
        }
    }

    /// `drawLevel` (textblock.cc:1731-1775) — `SL_IN_FLOW` : lignes
    /// intersectant `area` ; `SL_OOF_REF` : références OOF
    /// interrompant le dessin (mort verbatim — `IMPL_POS=false` ;
    /// amont lit `content.widget` sur un mot `WIDGET_OOF_REF`,
    /// quirk sans effet car `doesWidgetOOFInterruptDrawing` est
    /// toujours faux) ; sinon base `OOFAwareWidget`.
    pub(crate) fn tb_draw_level(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        area: &Rectangle,
        level: i32,
        context: &mut crate::types::DrawingContext,
    ) {
        match level {
            sl::IN_FLOW => {
                let start = self
                    .node_tb(w, |tb| {
                        self.tb_find_line_index_when_allocated(w, tb, area.y)
                    })
                    .unwrap_or(-1);
                self.with_tb(w, |l, tb| {
                    let mut line_index = start.max(0) as usize;
                    while line_index < tb.lines.len() {
                        let line = tb.lines[line_index].clone();
                        let alloc = l.node(w, |n| n.allocation);
                        if tb.line_y_offset_widget(&line, &alloc) >= area.y + area.height {
                            break;
                        }
                        l.tb_draw_line(w, tb, line_index, view, area, context);
                        line_index += 1;
                    }
                });
            }
            sl::OOF_REF => {
                // TODO amont : inefficace. Mort verbatim sous
                // `IMPL_POS=false` — `doesWidgetOOFInterruptDrawing`
                // retourne toujours `false`.
                for oofm_index in 0..crate::oof::NUM_OOFM {
                    let words_count = self.node_tb(w, |tb| tb.words.len()).unwrap_or(0);
                    for word_index in 0..words_count {
                        let ref_widget = self.node_tb(w, |tb| {
                            tb.words.get(word_index).and_then(|wd| {
                                if wd.content.r#type == content_type::WIDGET_OOF_REF {
                                    if let ContentData::WidgetReference(wr) = &wd.content.data {
                                        if self.get_oofm_index(wr.widget) == oofm_index {
                                            return Some(wr.widget);
                                        }
                                    }
                                }
                                None
                            })
                        });
                        let Some(rw) = ref_widget.flatten() else {
                            continue;
                        };
                        if self.does_widget_oof_interrupt_drawing(rw) {
                            self.widget_draw_interruption(rw, view, area, context);
                        }
                    }
                }
            }
            _ => self.oof_draw_level(w, view, area, level, context),
        }
    }
}
