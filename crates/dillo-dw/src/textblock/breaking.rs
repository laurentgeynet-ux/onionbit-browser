//! `textblock_linebreaking.cc` — bouclage, césure, césure auto.
//!
//! Port verbatim : `justifyLine`, `addLine`, `processWord`,
//! `wordWrap`, `wrapWordInFlow`, `wrapWordOofRef`,
//! `balanceBreakPosAndHeight`, `searchBreakPos`, `searchMinBap`,
//! `considerHyphenation`, `isHyphenationCandidate`,
//! `calcLinePartHeight`, `handleWordExtremes`,
//! `correctLastWordExtremes`, `hyphenateWord`, `moveWordIndices`,
//! `accumulateWordForLine`, `accumulateWordData`,
//! `calcLineBreakWidth`, `initLine1Offset`, `alignLine`,
//! `calcTextOffset`, `rewrap`, `fillParagraphs`, `initNewLine`,
//! `calcBorders`, `showMissingLines`, `removeTemporaryLines`,
//! `getSpace*/getLine*Shrinkability/Stretchability`, plus
//! `mustBorderBeRegarded`/`getWidgetRegardingBorderForLine`
//! (textblock.cc:3263 — placés ici, utilisés par le wrap).
//!
//! Adaptations modèle D : paramètres de sortie C++ → petites
//! structures (`SearchCtx`, `BalanceCtx`) ; `NotSoSimpleVector::
//! insert` ≙ `Vec::splice` avec clones du mot original ;
//! pointeurs `Widget*`/`OutOfFlowMgr*` ≙ `WidgetId`/`Rc<
//! RefCell<dyn OofMgr>>`.

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use super::{
    is_textblock_kind, penalty, Line, LineAlignment, Paragraph, TextblockData, Word,
    HYPHEN_DRAW_CHAR, PENALTY_HYPHEN, WORD_CAN_BE_HYPHENATED, WORD_DIV_CHAR_AT_EOL,
    WORD_DRAW_AS_ONE_TEXT, WORD_END, WORD_START, WORD_UNBREAKABLE_FOR_MIN_WIDTH,
};
use crate::hyphenator::Hyphenator;
use crate::layout::Layout;
use crate::oof::{make_parent_ref_in_flow, NUM_OOFM, OOFM_FLOATS};
use crate::style::{DisplayType, TextAlignType, VAlignType};
use crate::types::{content_type, ContentData, Extremes};
use crate::WidgetId;

/// État mutable partagé par `searchBreakPos`/`hyphenateWord` —
/// ≙ les pointeurs `int *searchUntil`, `*diffWords`,
/// `*wordIndexEnd`, `*addIndex1` amont.
pub(crate) struct SearchCtx {
    /// `*searchUntil` — borne haute de recherche de césure.
    pub search_until: i32,
    /// `*diffWords` — nb de mots insérés par césure (avant/à
    /// `word_index`).
    pub diff_words: i32,
    /// `*wordIndexEnd`.
    pub word_index_end: i32,
    /// `*addIndex1` — index additionnel à décaler lors des
    /// insertions (`lastFloatPos` amont dans `balanceBreak…`).
    pub add_index1: i32,
}

/// Paramètres/résultats de `balanceBreakPosAndHeight` — ≙
/// `*thereWillBeMoreSpace`, `*lastFloatPos`, `*height`, `*breakPos`.
pub(crate) struct BalanceCtx {
    /// `*thereWillBeMoreSpace`.
    pub there_will_be_more_space: bool,
    /// `*lastFloatPos`.
    pub last_float_pos: i32,
    /// `*height` (doit être initialisé, ≙ contrat amont).
    pub height: i32,
    /// `*breakPos` — résultat (non initialisé requis amont ; ici
    /// toujours écrit avant usage).
    pub break_pos: i32,
    /// `*diffWords`/`*wordIndexEnd`/`*addIndex1` partagés.
    pub ctx: SearchCtx,
}

impl Layout {
    // ----------------------------------------------------------------
    // Bordures de ligne / floats (textblock.cc:3263-3308 + hh:696-706).
    // ----------------------------------------------------------------

    /// `getWidgetRegardingBorderForLine (line)` →
    /// `(firstWord, lastWord)` ; `(lineNo)` ; `(firstWord, lastWord)`
    /// (textblock.cc:3263-3308). Un `RegardingBorder` enfant est
    /// toujours premier mot d'une ligne et exclu quand il constitue
    /// un nouveau container de floats.
    pub(crate) fn tb_widget_regarding_border_for_line(
        &self,
        tb: &TextblockData,
        first_word: i32,
        _last_word: i32,
    ) -> Option<WidgetId> {
        let word = tb.words.get(first_word.max(0) as usize)?;
        if word.content.r#type != content_type::WIDGET_IN_FLOW {
            return None;
        }
        let ContentData::Widget(widget) = word.content.data else {
            return None;
        };
        // `instanceOf (RegardingBorder)` et pas un nouveau
        // container de floats (verbatim).
        let is_rb = self.node(widget, |n| crate::oof::is_regarding_border_kind(&n.kind));
        if is_rb && !self.is_oof_container(widget, OOFM_FLOATS) {
            Some(widget)
        } else {
            None
        }
    }

    /// `mustBorderBeRegarded (lineNo)` (hh:702-705) — utilisable
    /// pour une ligne pas encore créée.
    pub(crate) fn tb_must_border_be_regarded(&self, tb: &TextblockData, line_no: i32) -> bool {
        let first_word = if line_no == 0 {
            0
        } else {
            tb.lines
                .get(line_no as usize - 1)
                .map_or(0, |l| l.last_word + 1)
        };
        let last_word = if (line_no as usize) < tb.lines.len() {
            tb.lines[line_no as usize].last_word
        } else {
            tb.words.len() as i32 - 1
        };
        self.tb_widget_regarding_border_for_line(tb, first_word, last_word)
            .is_none()
    }

    /// `mustBorderBeRegarded (line)` (hh:698-700).
    pub(crate) fn tb_must_border_be_regarded_line(&self, tb: &TextblockData, line: &Line) -> bool {
        self.tb_widget_regarding_border_for_line(tb, line.first_word, line.last_word)
            .is_none()
    }

    // ----------------------------------------------------------------
    // Justification et création de ligne.
    // ----------------------------------------------------------------

    /// `justifyLine` (linebreaking.cc:249-304) — répartition
    /// cumulée pour éviter les erreurs d'arrondi (doc
    /// rounding-errors).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    fn tb_justify_line(&self, tb: &mut TextblockData, line_index: usize, diff: i32) {
        let (first, last) = {
            let l = &tb.lines[line_index];
            (l.first_word, l.last_word)
        };
        if diff > 0 {
            let mut sum = 0i32;
            for i in first..last {
                sum += space_stretchability(&tb.words[i as usize]);
            }
            if sum > 0 {
                let mut cum = 0i32;
                let mut diff_cum = 0i32;
                for i in first..last {
                    cum += space_stretchability(&tb.words[i as usize]);
                    let space_diff = cum * diff / sum - diff_cum;
                    diff_cum += space_diff;
                    let w = &mut tb.words[i as usize];
                    w.eff_space = (i32::from(w.orig_space) + space_diff) as i16;
                }
            }
        } else if diff < 0 {
            let mut sum = 0i32;
            for i in first..last {
                sum += space_shrinkability(&tb.words[i as usize]);
            }
            if sum > 0 {
                let mut cum = 0i32;
                let mut diff_cum = 0i32;
                for i in first..last {
                    cum += space_shrinkability(&tb.words[i as usize]);
                    let space_diff = cum * diff / sum - diff_cum;
                    diff_cum += space_diff;
                    let w = &mut tb.words[i as usize];
                    w.eff_space = (i32::from(w.orig_space) + space_diff) as i16;
                }
            }
        }
    }

    /// `addLine` (linebreaking.cc:306-496) — crée la ligne
    /// `[firstWord, lastWord]`, calcule largeur/offsets/hauteurs,
    /// positionne les références OOF, puis `initNewLine`.
    #[allow(clippy::too_many_arguments)]
    fn tb_add_line(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        first_word: i32,
        last_word: i32,
        new_last_oof_pos: i32,
        temporary: bool,
        min_height: i32,
    ) {
        let mut line_width = if last_word >= first_word {
            // `totalWidth` inclut le trait de césure (voulu).
            tb.words[last_word as usize].total_width
        } else {
            0 // ligne vide
        };

        // `lineWidth` relatif à `leftOffset` — ajouter
        // `line1OffsetEff` (négatif pour les items) sur la 1re ligne.
        if tb.lines.is_empty() {
            line_width += tb.line1_offset_eff;
        }

        let line_index = tb.lines.len();
        tb.lines.push(Line {
            first_word,
            last_word,
            top: 0,
            margin_ascent: 0,
            margin_descent: 0,
            border_ascent: 0,
            border_descent: 0,
            content_ascent: 0,
            content_descent: 0,
            break_space: 0,
            text_offset: 0,
            max_line_width: 0,
            last_oof_ref_positioned_before_this_line: 0,
            left_offset: 0,
            right_offset: 0,
            alignment: LineAlignment::Left,
        });

        if !temporary {
            // Si la dernière ligne était temporaire, celle-ci l'est
            // aussi, même non demandée (verbatim).
            if tb.lines.len() == 1 || tb.non_temporary_lines == tb.lines.len() as i32 - 1 {
                tb.non_temporary_lines = tb.lines.len() as i32;
            }
        }

        // `mustBorderBeRegarded (line)` — variante `Line*`
        // verbatim (linebreaking.cc:368).
        let regard_border = self.tb_must_border_be_regarded_line(tb, &tb.lines[line_index]);
        let node_left = self.node(w, super::super::widget::WidgetNode::box_offset_x)
            + tb.left_inner_padding
            + if line_index == 0 {
                tb.line1_offset_eff
            } else {
                0
            };
        let node_rest = self.node(w, super::super::widget::WidgetNode::box_rest_width);
        {
            let line = &mut tb.lines[line_index];
            line.left_offset = (if regard_border {
                tb.new_line_left_border
            } else {
                0
            })
            .max(node_left);
            line.right_offset = (if regard_border {
                tb.new_line_right_border
            } else {
                0
            })
            .max(node_rest);
        }

        self.tb_align_line(w, tb, line_index);
        self.tb_calc_text_offset(tb, line_index, tb.line_break_width);

        for i in first_word..last_word {
            let word = &tb.words[i as usize];
            line_width += i32::from(word.eff_space) - i32::from(word.orig_space);
        }

        // Inclure les floats gauches dans `maxLineWidth` (la
        // correction de `sizeAllocateImpl` est sans rapport ;
        // les floats droits sont traités dans `OofMgr::getSize`).
        let box_off = self.node(w, super::super::widget::WidgetNode::box_offset_x);
        let left_off = tb.lines[line_index].left_offset;
        line_width += left_off - box_off;

        if tb.lines.len() == 1 {
            let l = &mut tb.lines[line_index];
            l.max_line_width = line_width;
            l.last_oof_ref_positioned_before_this_line = -1;
        } else {
            let prev_max = tb.lines[line_index - 1].max_line_width;
            let prev_oof = tb.lines[line_index - 1].last_oof_ref_positioned_before_this_line;
            let l = &mut tb.lines[line_index];
            l.max_line_width = line_width.max(prev_max);
            l.last_oof_ref_positioned_before_this_line = prev_oof;
        }

        for i in first_word..=last_word {
            self.tb_accumulate_word_for_line(w, tb, line_index, i);
        }

        if line_index == 0 {
            tb.lines[line_index].top = 0;
        } else {
            // Collapse des marges de lignes adjacentes (cf.
            // `Line::totalHeight`).
            let prev_top = tb.lines[line_index - 1].top;
            let prev_total = {
                let prev = &tb.lines[line_index - 1];
                let margin_next =
                    tb.lines[line_index].margin_ascent - tb.lines[line_index].border_ascent;
                prev.border_ascent
                    + prev.border_descent
                    + (prev.margin_descent - prev.border_descent)
                        .max(margin_next)
                        .max(prev.break_space)
            };
            tb.lines[line_index].top = prev_top + prev_total;
        }

        // Les lignes vides (possibles avec floats) de hauteur nulle
        // peuvent boucler sans fin — hauteur positive forcée.
        {
            let l = &mut tb.lines[line_index];
            l.border_ascent = l.border_ascent.max(min_height);
            l.margin_ascent = l.margin_ascent.max(min_height);
        }

        tb.must_queue_resize = true;

        let mut x_widget = tb.lines[line_index].text_offset;
        let y_line = self.tb_y_offset_of_line_created(w, tb, &tb.lines[line_index]);
        for i in first_word..=last_word {
            let word = &tb.words[i as usize];
            if let Some(r) = &word.word_img_renderer {
                if let Some(wr) = r
                    .borrow_mut()
                    .as_any_mut()
                    .downcast_mut::<super::WordImgRenderer>()
                {
                    wr.set_data(x_widget, line_index as i32);
                }
            }
            if let Some(r) = &word.space_img_renderer {
                if let Some(sr) = r
                    .borrow_mut()
                    .as_any_mut()
                    .downcast_mut::<super::SpaceImgRenderer>()
                {
                    sr.set_data(x_widget, line_index as i32);
                }
            }

            if word.content.r#type == content_type::WIDGET_OOF_REF {
                if let ContentData::WidgetReference(wr) = word.content.data {
                    let widget = wr.widget;
                    let oofm_index = self.get_oofm_index(widget);
                    if let Some(mgr) = self.search_out_of_flow_mgr(w, oofm_index) {
                        // Cf. `sizeAllocate` : position verticale —
                        // devrait être centralisée (verbatim).
                        let x_rel = x_widget;
                        let y_rel =
                            y_line + (tb.lines[line_index].border_ascent - word.size.ascent);
                        if let Some((x_ref, y_ref)) = self.tb_find_size_request_reference(
                            w,
                            tb,
                            tb.oof.oof_container[oofm_index],
                        ) {
                            mgr.borrow_mut().tell_position2(
                                self,
                                widget,
                                x_ref + x_rel,
                                y_ref + y_rel,
                            );
                        } else {
                            mgr.borrow_mut()
                                .tell_incomplete_position2(self, widget, w, x_rel, y_rel);
                        }
                    }
                }
            }
            x_widget += word.size.width + i32::from(word.eff_space);
        }

        tb.lines[line_index].last_oof_ref_positioned_before_this_line = tb.lines[line_index]
            .last_oof_ref_positioned_before_this_line
            .max(new_last_oof_pos);

        self.tb_init_new_line(w, tb);
    }

    /// `processWord` (linebreaking.cc:498-556) — `wordWrap` puis
    /// `handleWordExtremes` ; si la liste de mots a changé
    /// (césures), tout le paragraphe est recalculé (verbatim —
    /// le mot césuré peut précéder `wordIndex`, impossible à
    /// reconstruire simplement).
    pub(crate) fn tb_process_word(&mut self, w: WidgetId, tb: &mut TextblockData, word_index: i32) {
        let diff_words = self.tb_word_wrap(w, tb, word_index, false);

        if diff_words == 0 {
            self.tb_handle_word_extremes(w, tb, word_index);
        } else {
            let first_word = if tb.paragraphs.is_empty() {
                0
            } else {
                let fw = tb.paragraphs.last().map_or(0, |p| p.first_word);
                tb.paragraphs.pop();
                fw
            };
            let last_index = word_index + diff_words;
            for i in first_word..=last_index {
                self.tb_handle_word_extremes(w, tb, i);
            }
        }
    }

    /// `wordWrap` (linebreaking.cc:558-584) — retourne combien la
    /// liste de mots a grandi au niveau ou avant `wordIndex`.
    fn tb_word_wrap(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_index: i32,
        wrap_all: bool,
    ) -> i32 {
        if !wrap_all {
            self.tb_remove_temporary_lines(w, tb);
        }
        self.tb_init_line1_offset(w, tb, word_index);
        {
            let word = &mut tb.words[word_index as usize];
            word.eff_space = word.orig_space;
        }
        self.tb_accumulate_word_data(w, tb, word_index);

        let ret = if tb.words[word_index as usize].content.r#type == content_type::WIDGET_OOF_REF {
            self.tb_wrap_word_oof_ref(w, tb, word_index, wrap_all)
        } else {
            self.tb_wrap_word_in_flow(w, tb, word_index, wrap_all)
        };
        // `AlignedTableCell::wordWrap` — détection `textAlignChar`
        // + `updateValue` verbatim (alignedtablecell.cc:152-179).
        if matches!(tb.cell, Some(crate::table::TableCellKind::Aligned { .. })) {
            self.cell_word_wrap_after(w, word_index);
        }
        ret
    }

    /// `wrapWordInFlow` (linebreaking.cc:586-860) — boucle de
    /// création de lignes, recherche de césure, gestion des floats.
    #[allow(unused_assignments)] // `thereWillBeMoreSpace` réécrit
                                 // par `*tbms` amont — mort des deux côtés (verbatim).
    fn tb_wrap_word_in_flow(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_index: i32,
        wrap_all: bool,
    ) -> i32 {
        let mut diff_words = 0i32;
        let mut penalty_index = tb.calc_penalty_index_for_new_line();

        let mut new_line;
        loop {
            // `thereWillBeMoreSpace` : vrai si des floats rendent
            // cette ligne plus étroite que les suivantes — une ligne
            // peut alors rester vide (verbatim).
            let regard_border = self.tb_must_border_be_regarded(tb, tb.lines.len() as i32);
            let mut there_will_be_more_space = if regard_border {
                tb.new_line_has_float_left || tb.new_line_has_float_right
            } else {
                false
            };

            let mut temp_new_line = false;
            let first_index = if tb.lines.is_empty() {
                0
            } else {
                tb.lines.last().map_or(0, |l| l.last_word + 1)
            };
            let mut search_until = 0i32;

            if word_index < first_index {
                // Mot déjà dans une ligne existante.
                new_line = false;
            } else if wrap_all
                && word_index >= first_index
                && word_index == tb.words.len() as i32 - 1
            {
                new_line = true;
                search_until = word_index;
                temp_new_line = true;
            } else if word_index >= first_index
                && tb.words[word_index as usize]
                    .badness_and_penalty
                    .line_must_be_broken(penalty_index)
            {
                new_line = true;
                search_until = word_index;
            } else {
                // Couper quand trop serré et qu'un point de césure
                // existe (sauf `thereWillBeMoreSpace` — verbatim).
                let mut possible_line_break = false;
                let mut i = first_index;
                while !(there_will_be_more_space || possible_line_break) && i < word_index {
                    if tb.words[i as usize]
                        .badness_and_penalty
                        .line_can_be_broken(penalty_index)
                    {
                        possible_line_break = true;
                    }
                    i += 1;
                }

                if (there_will_be_more_space || possible_line_break)
                    && tb.words[word_index as usize]
                        .badness_and_penalty
                        .line_too_tight()
                {
                    new_line = true;
                    search_until = word_index - 1;
                } else {
                    new_line = false;
                }
            }

            if !new_line && !wrap_all {
                // Pas de nouvelle ligne ; `mustQueueResize` doit
                // quand même être posé pour que `flush` →
                // `queueResize` → `sizeRequestImpl` →
                // `showMissingLines` re-appelle avec `wrapAll`.
                tb.must_queue_resize = true;
            }

            if new_line {
                self.tb_accumulate_word_data(w, tb, word_index);

                let mut balance = BalanceCtx {
                    there_will_be_more_space,
                    last_float_pos: if tb.lines.is_empty() {
                        -1
                    } else {
                        tb.lines
                            .last()
                            .map_or(-1, |l| l.last_oof_ref_positioned_before_this_line)
                    },
                    height: 1, // supposé par calcBorders (voir là)
                    break_pos: 0,
                    ctx: SearchCtx {
                        search_until,
                        diff_words,
                        word_index_end: word_index,
                        add_index1: 0,
                    },
                };
                balance.ctx.add_index1 = balance.last_float_pos;
                self.tb_balance_break_pos_and_height(
                    w,
                    tb,
                    word_index,
                    first_index,
                    temp_new_line,
                    penalty_index,
                    true,
                    wrap_all,
                    regard_border,
                    &mut balance,
                );
                there_will_be_more_space = balance.there_will_be_more_space;

                let mut float_handled;
                let y_new_line = self.tb_y_offset_of_line_to_be_created(w, tb, None);
                loop {
                    let start_search = first_index.max(balance.last_float_pos + 1);
                    let mut new_float_pos = -1i32;

                    // Étape 1 : chercher le prochain float.
                    let mut i = start_search;
                    while new_float_pos == -1 && i <= balance.break_pos {
                        if let ContentData::WidgetReference(wr) = tb.words[i as usize].content.data
                        {
                            let widget = wr.widget;
                            let mut j = 0usize;
                            while new_float_pos == -1 && j < NUM_OOFM {
                                if let Some(mgr) = self.search_out_of_flow_mgr(w, j) {
                                    let m = mgr.borrow();
                                    if m.affects_left_border(self, widget)
                                        || m.affects_right_border(self, widget)
                                    {
                                        new_float_pos = i;
                                    }
                                }
                                j += 1;
                            }
                        }
                        i += 1;
                    }

                    if new_float_pos == -1 {
                        float_handled = false;
                    } else {
                        float_handled = true;

                        // Étape 2 : positionner le float et
                        // recalculer la ligne.
                        balance.last_float_pos = new_float_pos;
                        let widget = match tb.words[new_float_pos as usize].content.data {
                            ContentData::WidgetReference(wr) => wr.widget,
                            _ => w,
                        };
                        let oofm_index = self.widget_oof_index(widget) as usize;
                        if let Some(mgr) = self.search_out_of_flow_mgr(w, oofm_index) {
                            if mgr.borrow().may_affect_borders_at_all() {
                                // « x » pas tout à fait correct —
                                // sans importance (verbatim TODO).
                                let x_rel =
                                    self.node(w, super::super::widget::WidgetNode::box_offset_x);
                                let y_rel = y_new_line;
                                if let Some((x_ref, y_ref)) = self.tb_find_size_request_reference(
                                    w,
                                    tb,
                                    tb.oof.oof_container[oofm_index],
                                ) {
                                    mgr.borrow_mut().tell_position1(
                                        self,
                                        widget,
                                        x_ref + x_rel,
                                        y_ref + y_rel,
                                    );
                                } else {
                                    mgr.borrow_mut()
                                        .tell_incomplete_position1(self, widget, w, x_rel, y_rel);
                                }
                            }
                        }

                        self.tb_balance_break_pos_and_height(
                            w,
                            tb,
                            word_index,
                            first_index,
                            temp_new_line,
                            penalty_index,
                            false,
                            wrap_all,
                            regard_border,
                            &mut balance,
                        );
                        there_will_be_more_space = balance.there_will_be_more_space;
                    }
                    if !float_handled {
                        break;
                    }
                }

                let min_height = if first_index <= balance.break_pos {
                    // Ligne non vide : hauteur depuis le contenu.
                    1
                } else {
                    // Ligne vide : hauteurs de floats pour éviter
                    // trop de lignes de 1px (verbatim).
                    if tb.new_line_has_float_left && tb.new_line_has_float_right {
                        tb.new_line_left_float_height
                            .min(tb.new_line_right_float_height)
                            .max(1)
                    } else if tb.new_line_has_float_left {
                        tb.new_line_left_float_height.max(1)
                    } else if tb.new_line_has_float_right {
                        tb.new_line_right_float_height.max(1)
                    } else {
                        1
                    }
                };

                self.tb_add_line(
                    w,
                    tb,
                    first_index,
                    balance.break_pos,
                    balance.last_float_pos,
                    temp_new_line,
                    min_height,
                );

                for i in balance.break_pos + 1..=balance.ctx.word_index_end {
                    self.tb_accumulate_word_data(w, tb, i);
                }

                diff_words = balance.ctx.diff_words;
                search_until = balance.ctx.search_until;
                let _ = search_until;
                // `word` est relu via `tb.words[word_index]` à
                // chaque accès (≙ re-`getRef` après insertion).
                penalty_index = tb.calc_penalty_index_for_new_line();
            }
            if !new_line {
                break;
            }
        }

        if tb.words[word_index as usize].content.r#type == content_type::WIDGET_IN_FLOW {
            // `parentRef` de l'enfant, si le mot est déjà dans une
            // ligne (verbatim — cf. commentaire amont).
            let first_word_without_line = if tb.lines.is_empty() {
                0
            } else {
                tb.lines.last().map_or(0, |l| l.last_word + 1)
            };
            if word_index >= first_word_without_line {
                if let ContentData::Widget(child) = tb.words[word_index as usize].content.data {
                    let pref = make_parent_ref_in_flow(tb.lines.len() as i32);
                    self.node_mut(child, |n| n.parent_ref = pref);
                }
            }
        }

        diff_words
    }

    /// `wrapWordOofRef` (linebreaking.cc:862-907) — positionne le
    /// widget OOF non affectant les bords (éléments positionnés —
    /// `tellPosition1` non implémenté pour eux amont) et fixe le
    /// `parentRef` de la `WidgetReference`.
    fn tb_wrap_word_oof_ref(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_index: i32,
        _wrap_all: bool,
    ) -> i32 {
        let widget = match tb.words[word_index as usize].content.data {
            ContentData::WidgetReference(wr) => Some(wr.widget),
            _ => None,
        };
        let y_new_line = self.tb_y_offset_of_line_to_be_created(w, tb, None);

        if let Some(widget) = widget {
            let oofm_index = self.get_oofm_index(widget);
            if let Some(mgr) = self.search_out_of_flow_mgr(w, oofm_index) {
                if !mgr.borrow().may_affect_borders_at_all() {
                    let x_rel = self.node(w, super::super::widget::WidgetNode::box_offset_x);
                    let y_rel = y_new_line;
                    if let Some((x_ref, y_ref)) =
                        self.tb_find_size_request_reference(w, tb, tb.oof.oof_container[oofm_index])
                    {
                        mgr.borrow_mut()
                            .tell_position1(self, widget, x_ref + x_rel, y_ref + y_rel);
                    } else {
                        mgr.borrow_mut()
                            .tell_incomplete_position1(self, widget, w, x_rel, y_rel);
                    }
                }
            }

            // `parentRef` du widget référencé (cf. wrapWordInFlow).
            let first_word_without_line = if tb.lines.is_empty() {
                0
            } else {
                tb.lines.last().map_or(0, |l| l.last_word + 1)
            };
            if word_index >= first_word_without_line {
                if let Some(word) = tb.words.get_mut(word_index as usize) {
                    if let ContentData::WidgetReference(ref mut wr) = word.content.data {
                        wr.parent_ref = make_parent_ref_in_flow(tb.lines.len() as i32);
                    }
                }
            }
        }

        0 // liste des mots inchangée
    }

    /// `balanceBreakPosAndHeight` (linebreaking.cc:909-1004) —
    /// itère `calcBorders`/`searchBreakPos`/`calcLinePartHeight`
    /// jusqu'à convergence (la hauteur n'augmente plus ; la 1re
    /// passe n'est jamais rejetée — verbatim).
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::fn_params_excessive_bools)] // bools verbatim amont.
    fn tb_balance_break_pos_and_height(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_index: i32,
        first_index: i32,
        temp_new_line: bool,
        penalty_index: usize,
        border_is_calculated: bool,
        wrap_all: bool,
        regard_border: bool,
        balance: &mut BalanceCtx,
    ) {
        let mut run_no = 1;
        loop {
            if !(border_is_calculated && run_no == 1) {
                // `borderIsCalculated` n'est valide qu'en 1re passe.
                self.tb_calc_borders(w, tb, balance.last_float_pos, balance.height);
                balance.there_will_be_more_space = if regard_border {
                    tb.new_line_has_float_left || tb.new_line_has_float_right
                } else {
                    false
                };
                for i in first_index..=balance.ctx.word_index_end {
                    self.tb_accumulate_word_data(w, tb, i);
                }
            }

            let twbms = balance.there_will_be_more_space;
            let new_break_pos = self.tb_search_break_pos(
                w,
                tb,
                word_index,
                first_index,
                temp_new_line,
                penalty_index,
                twbms,
                wrap_all,
                &mut balance.ctx,
            );
            // `addIndex1` = `lastFloatPos` amont (même storage —
            // resynchroniser après les césures).
            balance.last_float_pos = balance.ctx.add_index1;
            let new_height = Self::tb_calc_line_part_height(tb, first_index, new_break_pos);

            if run_no != 1 && new_height >= balance.height {
                if new_height == balance.height {
                    // Convergence : la nouvelle position de césure
                    // est adoptée (verbatim).
                    balance.break_pos = new_break_pos;
                }
                // newHeight > height : rejeter les nouvelles
                // valeurs, moins bonnes (verbatim).
                break;
            }
            balance.height = new_height;
            balance.break_pos = new_break_pos;
            run_no += 1;
        }
    }

    /// `searchBreakPos` (linebreaking.cc:1006-1116) — boucle
    /// césure/hyphenation jusqu'à ce qu'une ligne soit ajoutée ;
    /// ajuste `searchUntil`/`wordIndexEnd`/`addIndex1` via `ctx`.
    #[allow(clippy::too_many_arguments)]
    fn tb_search_break_pos(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_index: i32,
        first_index: i32,
        temp_new_line: bool,
        penalty_index: usize,
        there_will_be_more_space: bool,
        wrap_all: bool,
        ctx: &mut SearchCtx,
    ) -> i32 {
        let _ = temp_new_line;
        let _ = wrap_all;
        loop {
            if first_index > ctx.search_until {
                // Ligne vide.
                debug_assert_eq!(ctx.search_until, first_index - 1);
                return first_index - 1;
            }
            if there_will_be_more_space
                && tb.words[first_index as usize]
                    .badness_and_penalty
                    .line_too_tight()
            {
                let hyphenated_word = self.tb_consider_hyphenation(tb, first_index, first_index);
                if hyphenated_word == -1 {
                    return first_index - 1;
                }
                let n = self.tb_hyphenate_word(w, tb, hyphenated_word, ctx);
                ctx.search_until += n;
                if hyphenated_word <= word_index {
                    ctx.word_index_end += n;
                }
                continue;
            }

            // Ligne non vide.
            let break_pos = self.tb_search_min_bap(
                tb,
                first_index,
                ctx.search_until,
                penalty_index,
                there_will_be_more_space,
                wrap_all,
            );
            let hyphenated_word = self.tb_consider_hyphenation(tb, first_index, break_pos);
            if hyphenated_word == -1 {
                return break_pos;
            }
            // `hyphenateWord` devrait indiquer si quelque chose a
            // changé (TODO amont verbatim).
            let n = self.tb_hyphenate_word(w, tb, hyphenated_word, ctx);
            ctx.search_until += n;
            if hyphenated_word <= word_index {
                ctx.word_index_end += n;
                ctx.diff_words += n;
            }
            for i in break_pos + 1..=ctx.word_index_end {
                self.tb_accumulate_word_data(w, tb, i);
            }
        }
    }

    /// `searchMinBap` (linebreaking.cc:1118-1188) — position de
    /// `badnessAndPenalty` minimal (`<=` pour le tie-break
    /// verbatim) ; correction `correctAtEnd` sur le dernier mot.
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    fn tb_search_min_bap(
        &self,
        tb: &TextblockData,
        first_word: i32,
        last_word: i32,
        penalty_index: usize,
        _there_will_be_more_space: bool,
        correct_at_end: bool,
    ) -> i32 {
        let mut pos = -1i32;
        for i in first_word..=last_word {
            // `<=` plutôt que `<` : tend à mettre plus de mots par
            // ligne (commentaire amont — le cas `==` n'arrive pas
            // en pratique).
            if pos == -1
                || tb.words[i as usize]
                    .badness_and_penalty
                    .compare_to(penalty_index, &tb.words[pos as usize].badness_and_penalty)
                    <= 0
            {
                pos = i;
            }
        }

        if correct_at_end && last_word == tb.words.len() as i32 - 1 {
            // Sans césure ni espace, le dernier mot a une pénalité
            // inf — corrigée ici sans la modifier (verbatim ; un
            // -inf historique cassait test/table-1.html).
            let w = &tb.words[last_word as usize];
            let mut corrected_bap = w.badness_and_penalty;
            corrected_bap.set_penalty(0);
            if pos != -1
                && corrected_bap
                    .compare_to(penalty_index, &tb.words[pos as usize].badness_and_penalty)
                    <= 0
            {
                pos = last_word;
            }
        }
        pos
    }

    /// `considerHyphenation` (linebreaking.cc:1190-1223) — suggère
    /// un mot à césurer : ligne serrée → dernier mot césurable ;
    /// ligne lâche → premier mot de la ligne suivante.
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    fn tb_consider_hyphenation(&self, tb: &TextblockData, first_index: i32, break_pos: i32) -> i32 {
        let mut hyphenated_word = -1i32;
        let word_break = &tb.words[break_pos as usize];

        // Ligne serrée : une partie du dernier mot pourrait passer
        // sur la ligne suivante (verbatim).
        if word_break.badness_and_penalty.is_tight() {
            for i in (first_index..=break_pos).rev() {
                let w1 = &tb.words[i as usize];
                if w1.badness_and_penalty.is_tight() && Self::tb_is_hyphenation_candidate(w1) {
                    hyphenated_word = i;
                }
            }
        }

        // Ligne lâche : le premier mot de la ligne suivante pourrait
        // remonter (verbatim).
        if word_break.badness_and_penalty.is_loose() && break_pos + 1 < tb.words.len() as i32 {
            let w2 = &tb.words[break_pos as usize + 1];
            if Self::tb_is_hyphenation_candidate(w2) {
                hyphenated_word = break_pos + 1;
            }
        }
        hyphenated_word
    }

    /// `isHyphenationCandidate` (linebreaking.cc:1225-1233).
    fn tb_is_hyphenation_candidate(word: &Word) -> bool {
        word.flags & WORD_CAN_BE_HYPHENATED != 0
            && word.style.attrs.x_lang[0] != 0
            && Self::tb_is_break_allowed_in_word(word)
            && word.content.r#type == content_type::TEXT
            && match &word.content.data {
                ContentData::Text(t) => Hyphenator::is_hyphenation_candidate(t),
                _ => false,
            }
    }

    /// `isBreakAllowedInWord` (hh:677-678) → `isBreakAllowed(style)`.
    fn tb_is_break_allowed_in_word(word: &Word) -> bool {
        Self::tb_is_break_allowed(word.style.as_ref())
    }

    /// `calcLinePartHeight` (linebreaking.cc:1235-1249).
    fn tb_calc_line_part_height(tb: &TextblockData, first_word: i32, last_word: i32) -> i32 {
        let mut ascent = 0i32;
        let mut descent = 0i32;
        for i in first_word..=last_word {
            let w = &tb.words[i as usize];
            ascent = ascent.max(w.size.ascent);
            descent = descent.max(w.size.descent);
        }
        (ascent + descent).max(1)
    }

    /// `handleWordExtremes` (linebreaking.cc:1251-1390) — pendant
    /// de `wordWrap` pour les extremes : accumule `parMin`/
    /// `parMax` et leurs maxima par paragraphe.
    fn tb_handle_word_extremes(&mut self, w: WidgetId, tb: &mut TextblockData, word_index: i32) {
        self.tb_init_line1_offset(w, tb, word_index);

        let mut word_extremes = Extremes::default();
        self.tb_get_word_extremes(w, tb, &tb.words[word_index as usize], &mut word_extremes);

        if word_index == 0 {
            word_extremes.min_width += tb.line1_offset_eff;
            word_extremes.min_width_intrinsic += tb.line1_offset_eff;
            word_extremes.max_width += tb.line1_offset_eff;
            word_extremes.max_width_intrinsic += tb.line1_offset_eff;
        }

        let need_new_paragraph = tb.paragraphs.is_empty()
            || tb.words[tb
                .paragraphs
                .last()
                .map_or(0, |p| p.last_word.max(0) as usize)]
            .badness_and_penalty
            .line_must_be_broken(1);
        if need_new_paragraph {
            let prev = tb.paragraphs.last().cloned();
            let mut par = Paragraph {
                first_word: word_index,
                last_word: word_index,
                par_min: 0,
                par_min_intrinsic: 0,
                par_adjustment_width: 0,
                par_max: 0,
                par_max_intrinsic: 0,
                max_par_min: 0,
                max_par_min_intrinsic: 0,
                max_par_adjustment_width: 0,
                max_par_max: 0,
                max_par_max_intrinsic: 0,
            };
            if let Some(p) = prev {
                par.max_par_min = p.max_par_min;
                par.max_par_min_intrinsic = p.max_par_min_intrinsic;
                par.max_par_max = p.max_par_max;
                par.max_par_max_intrinsic = p.max_par_max_intrinsic;
                par.max_par_adjustment_width = p.max_par_adjustment_width;
            }
            tb.paragraphs.push(par);
        }

        let last_par_idx = tb.paragraphs.len() - 1;
        let (corr_diff_min, corr_diff_max) = if word_index > tb.paragraphs[last_par_idx].first_word
        {
            let last_word = &tb.words[word_index as usize - 1];
            let dm = if last_word.badness_and_penalty.line_can_be_broken(1)
                && last_word.flags & WORD_UNBREAKABLE_FOR_MIN_WIDTH == 0
            {
                0
            } else {
                i32::from(last_word.orig_space) - i32::from(last_word.hyphen_width)
            };
            (
                dm,
                i32::from(last_word.orig_space) - i32::from(last_word.hyphen_width),
            )
        } else {
            (0, 0)
        };

        // Minimum : entre deux césures *possibles* (la shrinkability
        // ne joue pas de rôle — verbatim).
        {
            let word = &tb.words[word_index as usize];
            let par = &mut tb.paragraphs[last_par_idx];
            par.par_min += word_extremes.min_width + i32::from(word.hyphen_width) + corr_diff_min;
            par.par_min_intrinsic +=
                word_extremes.min_width_intrinsic + i32::from(word.hyphen_width) + corr_diff_min;
            par.par_adjustment_width +=
                word_extremes.adjustment_width + i32::from(word.hyphen_width) + corr_diff_min;
            par.max_par_min = par.max_par_min.max(par.par_min);
            par.max_par_min_intrinsic = par.max_par_min_intrinsic.max(par.par_min_intrinsic);
            par.max_par_adjustment_width =
                par.max_par_adjustment_width.max(par.par_adjustment_width);
        }
        {
            let word = &tb.words[word_index as usize];
            if word.badness_and_penalty.line_can_be_broken(1)
                && word.flags & WORD_UNBREAKABLE_FOR_MIN_WIDTH == 0
            {
                let par = &mut tb.paragraphs[last_par_idx];
                par.par_min = 0;
                par.par_min_intrinsic = 0;
                par.par_adjustment_width = 0;
            }
        }
        // Maximum : entre deux césures *nécessaires* (verbatim).
        {
            let word = &tb.words[word_index as usize];
            let par = &mut tb.paragraphs[last_par_idx];
            par.par_max += word_extremes.max_width + i32::from(word.hyphen_width) + corr_diff_max;
            par.par_max_intrinsic +=
                word_extremes.max_width_intrinsic + i32::from(word.hyphen_width) + corr_diff_max;
            par.max_par_max = par.max_par_max.max(par.par_max);
            par.max_par_max_intrinsic = par.max_par_max_intrinsic.max(par.par_max_intrinsic);
            par.last_word = word_index;
        }
    }

    /// `correctLastWordExtremes` (linebreaking.cc:1392-1406) —
    /// appelée quand le dernier mot change (espace, césures) ;
    /// s'il permet une césure, les minima courants repartent à 0.
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_correct_last_word_extremes(&mut self, tb: &mut TextblockData) {
        if let Some(par_idx) = tb.paragraphs.len().checked_sub(1) {
            if let Some(word) = tb.words.last() {
                if word.badness_and_penalty.line_can_be_broken(1)
                    && word.flags & WORD_UNBREAKABLE_FOR_MIN_WIDTH == 0
                {
                    let par = &mut tb.paragraphs[par_idx];
                    par.par_min = 0;
                    par.par_min_intrinsic = 0;
                    par.par_adjustment_width = 0;
                }
            }
        }
    }

    /// `hyphenateWord` (linebreaking.cc:1408-1515) — césure le mot
    /// via `Hyphenator`, insère les parties, recalcule tailles,
    /// pénalités, ancres, indices. Retourne `numBreaks`.
    fn tb_hyphenate_word(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_index: i32,
        ctx: &mut SearchCtx,
    ) -> i32 {
        let lang = tb.words[word_index as usize].style.attrs.x_lang;
        let hyphenator = Hyphenator::get_hyphenator(&lang);
        let text = match &tb.words[word_index as usize].content.data {
            ContentData::Text(t) => t.clone(),
            _ => Vec::new(),
        };
        let platform = self.platform.clone();
        let break_pos = hyphenator.hyphenate_word(&platform, &text);
        let num_breaks = break_pos.len() as i32;

        if num_breaks > 0 {
            let orig_word = tb.words[word_index as usize].clone();
            let orig_text = text.clone();
            let mut word_size = vec![crate::types::Requisition::default(); num_breaks as usize + 1];
            self.tb_calc_text_sizes(
                &orig_text,
                orig_text.len(),
                orig_word.style.as_ref(),
                num_breaks as usize,
                &break_pos,
                &mut word_size,
            );

            // `words->insert (wordIndex, numBreaks)` ≙ clones du mot
            // original (NotSoSimpleVector verbatim).
            let inserts: Vec<Word> = (0..num_breaks).map(|_| orig_word.clone()).collect();
            tb.words
                .splice(word_index as usize..word_index as usize, inserts);

            self.tb_move_word_indices(w, tb, word_index, num_breaks, ctx);

            // Ajuste les index d'ancres (verbatim).
            for anchor in &mut tb.anchors {
                if anchor.word_index > word_index {
                    anchor.word_index += num_breaks;
                }
            }

            for i in 0..=num_breaks {
                let idx = (word_index + i) as usize;
                let style = orig_word.style.clone();
                self.tb_fill_word(
                    w,
                    tb,
                    idx as i32,
                    word_size[i as usize].width,
                    word_size[i as usize].ascent,
                    word_size[i as usize].descent,
                    0,
                    &style,
                );
                let start = if i == 0 {
                    0
                } else {
                    break_pos[i as usize - 1] as usize
                };
                let end = if i == num_breaks {
                    orig_text.len()
                } else {
                    break_pos[i as usize] as usize
                };
                {
                    let wref = &mut tb.words[idx];
                    wref.content.r#type = content_type::TEXT;
                    wref.content.data = ContentData::Text(orig_text[start..end].to_vec());
                    if i == 0 {
                        wref.flags |= WORD_START;
                    } else {
                        wref.flags &= !WORD_START;
                    }
                    if i == num_breaks {
                        wref.flags |= WORD_END;
                    } else {
                        wref.flags &= !WORD_END;
                    }
                }
                if i < num_breaks {
                    let p1 = penalty(PENALTY_HYPHEN, 0);
                    let p2 = penalty(PENALTY_HYPHEN, 1);
                    {
                        let wref = &mut tb.words[idx];
                        wref.badness_and_penalty.set_penalties(p1, p2);
                    }
                    // `"\xe2\x80\x90"` trait d'union inconditionnel
                    // (verbatim — `hyphenDrawChar`).
                    let hw = {
                        let style = tb.words[idx].style.clone();
                        let font = style.attrs.font.clone();
                        font.map_or(0, |f| {
                            self.platform.borrow_mut().text_width(&f, HYPHEN_DRAW_CHAR)
                        })
                    };
                    let wref = &mut tb.words[idx];
                    wref.hyphen_width = hw as i16;
                    wref.flags |= WORD_DRAW_AS_ONE_TEXT
                        | WORD_DIV_CHAR_AT_EOL
                        | WORD_UNBREAKABLE_FOR_MIN_WIDTH;
                } else if orig_word.content.space {
                    let space_style = orig_word.space_style.clone();
                    self.tb_fill_space(w, tb, idx as i32, &space_style);
                }
            }

            // `accumulateWordData` dépend des bordures (floats),
            // elles-mêmes des widgets déjà vus — tous les mots
            // doivent être cohérents avant l'accumulation (verbatim).
            for i in 0..=num_breaks {
                self.tb_accumulate_word_data(w, tb, word_index + i);
            }
        } else {
            tb.words[word_index as usize].flags &= !WORD_CAN_BE_HYPHENATED;
        }
        num_breaks
    }

    /// `moveWordIndices` (linebreaking.cc:1517-1561) — décale les
    /// références (mgrs OOF, `lastOofRef…` des lignes, bornes des
    /// paragraphes, `addIndex1`).
    fn tb_move_word_indices(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_index: i32,
        num: i32,
        ctx: &mut SearchCtx,
    ) {
        for i in 0..NUM_OOFM {
            if let Some(mgr) = self.search_out_of_flow_mgr(w, i) {
                mgr.borrow_mut()
                    .move_external_indices(self, w, word_index, num);
            }
        }

        // `lastOofRefPositionedBeforeThisLine` est croissant — on
        // peut s'arrêter dès qu'une ligne est < wordIndex (verbatim).
        for line in tb.lines.iter_mut().rev() {
            if line.last_oof_ref_positioned_before_this_line < word_index {
                break;
            }
            line.last_oof_ref_positioned_before_this_line += num;
        }

        // Contrairement aux lignes, les derniers paragraphes sont
        // déjà construits (on itère depuis la fin — verbatim).
        for par in tb.paragraphs.iter_mut().rev() {
            if par.last_word <= word_index {
                break;
            }
            par.last_word += num;
            if par.first_word > word_index {
                par.first_word += num;
            }
        }

        // Indices additionnels (verbatim — extensible au besoin).
        if ctx.add_index1 >= word_index {
            ctx.add_index1 += num;
        }
    }

    /// `accumulateWordForLine` (linebreaking.cc:1563-1623) — met à
    /// jour les ascents/descents de la ligne depuis un mot, et les
    /// `parentRef` des widgets/references in-flow.
    fn tb_accumulate_word_for_line(
        &mut self,
        _w: WidgetId,
        tb: &mut TextblockData,
        line_index: usize,
        word_index: i32,
    ) {
        let word = &tb.words[word_index as usize];

        let mut len = word.style.attrs.font.as_ref().map_or(0, |f| f.ascent);
        if word.style.attrs.valign == VAlignType::Super {
            len += len / 2;
        }
        tb.lines[line_index].content_ascent = tb.lines[line_index].content_ascent.max(len);

        let mut len = word.style.attrs.font.as_ref().map_or(0, |f| f.descent);
        if word.style.attrs.valign == VAlignType::Sub {
            len += word.style.attrs.font.as_ref().map_or(0, |f| f.ascent) / 3;
        }
        tb.lines[line_index].content_descent = tb.lines[line_index].content_descent.max(len);

        let (border_ascent, border_descent, margin_ascent, margin_descent);
        let word = &tb.words[word_index as usize];
        if word.content.r#type == content_type::WIDGET_IN_FLOW {
            // TODO amont : considérer extraSpace ?
            margin_ascent = word.size.ascent;
            margin_descent = word.size.descent;
            let mut bt = 0;
            let mut bb = 0;
            if let ContentData::Widget(child) = word.content.data {
                if let Some(cs) = self.widget_style(child) {
                    bt = cs.attrs.margin.top;
                    bb = cs.attrs.margin.bottom;
                }
                border_ascent = margin_ascent - bt;
                border_descent = margin_descent - bb;
                let pref = make_parent_ref_in_flow(line_index as i32);
                self.node_mut(child, |n| n.parent_ref = pref);
            } else {
                border_ascent = margin_ascent;
                border_descent = margin_descent;
            }
        } else {
            border_ascent = word.size.ascent;
            border_descent = word.size.descent;
            margin_ascent = word.size.ascent;
            margin_descent = word.size.descent;
            if word.content.r#type == content_type::BREAK {
                if let ContentData::BreakSpace(bs) = word.content.data {
                    let l = &mut tb.lines[line_index];
                    l.break_space = l.break_space.max(bs);
                }
            } else if word.content.r#type == content_type::WIDGET_OOF_REF {
                if let Some(word) = tb.words.get_mut(word_index as usize) {
                    if let ContentData::WidgetReference(ref mut wr) = word.content.data {
                        wr.parent_ref = make_parent_ref_in_flow(line_index as i32);
                    }
                }
            }
        }

        let l = &mut tb.lines[line_index];
        l.border_ascent = l.border_ascent.max(border_ascent);
        l.border_descent = l.border_descent.max(border_descent);
        l.margin_ascent = l.margin_ascent.max(margin_ascent);
        l.margin_descent = l.margin_descent.max(margin_descent);
    }

    /// `accumulateWordData` (linebreaking.cc:1625-1716) — largeur
    /// cumulée, ascents/descents max, stretch/shrinkability et
    /// `calcBadness` du mot dans la ligne courante.
    pub(crate) fn tb_accumulate_word_data(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        word_index: i32,
    ) {
        // Le mot est typiquement dans la dernière ligne — recherche
        // linéaire depuis la fin (verbatim le plus rapide).
        let mut line_index = tb.lines.len();
        while line_index > 0 && word_index <= tb.lines[line_index - 1].last_word {
            line_index -= 1;
        }
        let first_word_of_line = if line_index == 0 {
            0
        } else {
            tb.lines[line_index - 1].last_word + 1
        };

        let line_break_width = self.tb_calc_line_break_width(w, tb, line_index as i32);

        if word_index == first_word_of_line {
            // Premier mot de la ligne (existante ou non).
            let word = &mut tb.words[word_index as usize];
            word.total_width = word.size.width + i32::from(word.hyphen_width);
            word.max_ascent = word.size.ascent;
            word.max_descent = word.size.descent;
            word.total_space_stretchability = 0;
            word.total_space_shrinkability = 0;
        } else {
            let (
                prev_total,
                prev_orig,
                prev_hyphen,
                prev_ascent,
                prev_descent,
                prev_stretch,
                prev_shrink,
            ) = {
                let p = &tb.words[word_index as usize - 1];
                (
                    p.total_width,
                    i32::from(p.orig_space),
                    i32::from(p.hyphen_width),
                    p.max_ascent,
                    p.max_descent,
                    p.total_space_stretchability,
                    p.total_space_shrinkability,
                )
            };
            let prev_stretchability = space_stretchability(&tb.words[word_index as usize - 1]);
            let prev_shrinkability = space_shrinkability(&tb.words[word_index as usize - 1]);
            let word = &mut tb.words[word_index as usize];
            word.total_width = prev_total + prev_orig - prev_hyphen
                + word.size.width
                + i32::from(word.hyphen_width);
            word.max_ascent = prev_ascent.max(word.size.ascent);
            word.max_descent = prev_descent.max(word.size.descent);
            word.total_space_stretchability = prev_stretch + prev_stretchability;
            word.total_space_shrinkability = prev_shrink + prev_shrinkability;
        }

        let (total_width, space_stretch, space_shrink, line_stretch, line_shrink) = {
            let word = &tb.words[word_index as usize];
            (
                word.total_width,
                word.total_space_stretchability,
                word.total_space_shrinkability,
                self.tb_get_line_stretchability(tb, word_index),
                Self::tb_get_line_shrinkability(),
            )
        };
        tb.words[word_index as usize]
            .badness_and_penalty
            .calc_badness(
                total_width,
                line_break_width,
                space_stretch + line_stretch,
                space_shrink + line_shrink,
            );
    }

    /// `calcLineBreakWidth` (linebreaking.cc:1718-1748) —
    /// `lineBreakWidth` moins padding interne, viewport (si
    /// `limitTextWidth`), `line1OffsetEff` en 1re ligne et bords
    /// de floats.
    fn tb_calc_line_break_width(&self, w: WidgetId, tb: &TextblockData, line_index: i32) -> i32 {
        let mut lbw = tb.line_break_width - tb.left_inner_padding;
        let box_diff = self.node(w, super::super::widget::WidgetNode::box_diff_width);
        if tb.limit_text_width
            && self.get_uses_viewport()
            // margin/border/padding seront retirés plus tard via
            // OOFM (verbatim).
            && lbw - box_diff > self.get_width_viewport() - 10
        {
            lbw = self.get_width_viewport() - 10;
        }
        if line_index == 0 {
            lbw -= tb.line1_offset_eff;
        }

        let (left_border, right_border) = if self.tb_must_border_be_regarded(tb, line_index) {
            (tb.new_line_left_border, tb.new_line_right_border)
        } else {
            (0, 0)
        };
        let left_border =
            left_border.max(self.node(w, super::super::widget::WidgetNode::box_offset_x));
        let right_border =
            right_border.max(self.node(w, super::super::widget::WidgetNode::box_rest_width));
        lbw - (left_border + right_border)
    }

    /// `initLine1Offset` (linebreaking.cc:1750-1781) — calcule
    /// `line1OffsetEff` au premier mot : `ignoreLine1OffsetSometimes`
    /// si le mot ne rentre pas, sinon `line1Offset + text-indent`
    /// (sauf premier widget de type bloc — verbatim).
    fn tb_init_line1_offset(&mut self, w: WidgetId, tb: &mut TextblockData, word_index: i32) {
        let _ = w;
        if word_index != 0 {
            return;
        }
        let word = &tb.words[0];
        if tb.ignore_line1_offset_sometimes
            && tb.line1_offset + word.size.width > tb.line_break_width
        {
            tb.line1_offset_eff = 0;
            return;
        }
        let mut indent = 0i32;
        let is_block_widget = word.content.r#type == content_type::WIDGET_IN_FLOW
            && match word.content.data {
                ContentData::Widget(child) => self.w_is_block_level(child),
                _ => false,
            };
        if !is_block_widget {
            // `textIndent` en pourcentage ou absolu (verbatim).
            if let Some(style) = self.node(w, |n| n.style.clone()) {
                let ti = style.attrs.text_indent;
                if crate::style::is_per_length(ti) {
                    indent =
                        crate::style::multiply_with_per_length_rounded(tb.line_break_width, ti);
                } else {
                    indent = crate::style::abs_length_val(ti);
                }
            }
        }
        tb.line1_offset_eff = tb.line1_offset + indent;
    }

    /// `alignLine` (linebreaking.cc:1783-1873) — `effSpace` =
    /// `origSpace`, alignement depuis le style du premier mot non
    /// `OOF_REF` (seuls les `display: inline*` sont affectés par
    /// `text-align` — verbatim) ; justification sauf dernière
    /// ligne de paragraphe (sauf si la ligne serait trop large).
    fn tb_align_line(&mut self, w: WidgetId, tb: &mut TextblockData, line_index: usize) {
        let (first_word, last_word) = {
            let l = &tb.lines[line_index];
            (l.first_word, l.last_word)
        };
        // Ligne vide (`firstWord > lastWord`) : plage vide.
        for i in first_word..=last_word.max(first_word - 1) {
            let w2 = &mut tb.words[i as usize];
            w2.eff_space = w2.orig_space;
        }

        // L'alignement des floats etc. ne nous intéresse pas.
        let mut first_not_oof = first_word;
        while first_not_oof <= last_word
            && tb.words[first_not_oof as usize].content.r#type == content_type::WIDGET_OOF_REF
        {
            first_not_oof += 1;
        }

        let mut alignment = LineAlignment::Left;
        if first_not_oof <= last_word
            && tb.words[first_not_oof as usize].content.r#type != content_type::BREAK
        {
            let first_word_ref = &tb.words[first_not_oof as usize];
            let last_word_ref = &tb.words[last_word as usize];
            let line_break_width = tb.line_break_width
                - (tb.lines[line_index].left_offset + tb.lines[line_index].right_offset);

            // Seuls les éléments inline sont affectés par
            // `text-align` (commentaire amont verbatim).
            let mut align_style = TextAlignType::Left;
            if matches!(
                first_word_ref.style.attrs.display,
                DisplayType::Inline | DisplayType::InlineBlock
            ) {
                align_style = self
                    .node(w, |n| n.style.clone())
                    .map_or(TextAlignType::Left, |s| s.attrs.text_align);
            }

            match align_style {
                TextAlignType::Right => alignment = LineAlignment::Right,
                TextAlignType::Center => alignment = LineAlignment::Center,
                TextAlignType::Justify => {
                    alignment = LineAlignment::Left;
                    // Pas de justification de la dernière ligne
                    // d'un paragraphe (BREAK ou dernier mot) — sauf
                    // si la ligne serait autrement trop large
                    // (solution imparfaite — verbatim).
                    if !(last_word_ref.content.r#type == content_type::BREAK
                        || last_word == tb.words.len() as i32 - 1)
                        || line_break_width < last_word_ref.total_width
                    {
                        self.tb_justify_line(
                            tb,
                            line_index,
                            line_break_width - last_word_ref.total_width,
                        );
                    }
                }
                // LEFT / STRING / autre → LEFT (verbatim).
                _ => alignment = LineAlignment::Left,
            }
        }
        // Ligne vide ou BREAK seul → LEFT (verbatim).
        tb.lines[line_index].alignment = alignment;
    }

    /// `calcTextOffset` (linebreaking.cc:1875-1924) — `textOffset`
    /// selon l'alignement ; borné à `leftOffset` pour les lignes
    /// trop larges (images — verbatim).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_calc_text_offset(
        &self,
        tb: &mut TextblockData,
        line_index: usize,
        total_width: i32,
    ) {
        let line = &tb.lines[line_index];
        let line_width = if line.first_word <= line.last_word {
            tb.words[line.last_word as usize].total_width
        } else {
            0
        };
        let mut text_offset = match line.alignment {
            LineAlignment::Left => line.left_offset,
            LineAlignment::Right => total_width - line.right_offset - line_width,
            LineAlignment::Center => {
                (line.left_offset + total_width - line.right_offset - line_width) / 2
            }
        };
        // Lignes trop larges (images dépassant le viewport).
        if text_offset < line.left_offset {
            text_offset = line.left_offset;
        }
        tb.lines[line_index].text_offset = text_offset;
    }

    /// `rewrap` (linebreaking.cc:1926-1998) — reconstruit les
    /// lignes depuis `wrapRefLines` (resize viewport ou changement
    /// de taille d'enfant) ; recalcule les tailles des widgets.
    pub(crate) fn tb_rewrap(&mut self, w: WidgetId, tb: &mut TextblockData) {
        if tb.wrap_ref_lines == -1 {
            return;
        }
        // Les lignes depuis `wrapRefLines` sont reconstruites.
        tb.lines.truncate(tb.wrap_ref_lines.max(0) as usize);
        tb.non_temporary_lines = tb.non_temporary_lines.min(tb.wrap_ref_lines);
        self.tb_init_new_line(w, tb);

        let first_word = if let Some(l) = tb.lines.last() {
            l.last_word + 1
        } else {
            0
        };
        tb.last_word_drawn = tb.last_word_drawn.min(first_word - 1);

        let mut i = first_word;
        while i < tb.words.len() as i32 {
            match tb.words[i as usize].content.r#type {
                t if t == content_type::WIDGET_IN_FLOW => {
                    if let ContentData::Widget(child) = tb.words[i as usize].content.data {
                        let mut size = tb.words[i as usize].size;
                        self.tb_calc_size_of_widget_in_flow(w, tb, i, child, &mut size);
                        tb.words[i as usize].size = size;
                    }
                }
                t if t == content_type::WIDGET_OOF_REF => {
                    if let ContentData::WidgetReference(wr) = tb.words[i as usize].content.data {
                        let widget = wr.widget;
                        let oofm_index = self.get_oofm_index(widget);
                        if let Some(mgr) = self.search_out_of_flow_mgr(w, oofm_index) {
                            let mut size = tb.words[i as usize].size;
                            mgr.borrow_mut()
                                .calc_widget_ref_size(self, widget, &mut size);
                            tb.words[i as usize].size = size;
                        }
                    }
                }
                _ => {}
            }
            // `wordWrap` peut insérer des mots — on relit `words[i]`
            // à chaque passage (note historique verbatim).
            self.tb_word_wrap(w, tb, i, false);
            i += 1;
        }
        tb.wrap_ref_lines = -1;
    }

    /// `fillParagraphs` (linebreaking.cc:2000-2055) — pendant de
    /// `rewrap` pour les extremes ; `wrapRefParagraphs` réfère des
    /// *lignes* (verbatim).
    pub(crate) fn tb_fill_paragraphs(&mut self, w: WidgetId, tb: &mut TextblockData) {
        let _ = w;
        if tb.wrap_ref_paragraphs == -1 {
            return;
        }
        // `wrapRefParagraphs` réfère les lignes, pas les
        // paragraphes (verbatim).
        let first_word_of_line = if !tb.lines.is_empty() && tb.wrap_ref_paragraphs > 0 {
            // `wrapRefParagraphs` peut dépasser `lines.size()`
            // (floats ? à clarifier amont).
            let line_no = tb.wrap_ref_paragraphs.min(tb.lines.len() as i32) - 1;
            tb.lines[line_no.max(0) as usize].last_word + 1
        } else {
            0
        };

        let par_no = if !tb.paragraphs.is_empty()
            && first_word_of_line > tb.paragraphs.last().map_or(0, |p| p.first_word)
        {
            // Liste de paragraphes partiellement construite —
            // compléter seulement la suite (verbatim).
            tb.paragraphs.len() as i32
        } else {
            self.tb_find_paragraph_of_word(tb, first_word_of_line)
                .max(0)
        };

        tb.paragraphs.truncate(par_no.max(0) as usize);
        let first_word = if let Some(p) = tb.paragraphs.last() {
            p.last_word + 1
        } else {
            0
        };
        for i in first_word..tb.words.len() as i32 {
            self.tb_handle_word_extremes(w, tb, i);
        }
        tb.wrap_ref_paragraphs = -1;
    }

    /// `initNewLine` (linebreaking.cc:2057-2071).
    pub(crate) fn tb_init_new_line(&mut self, w: WidgetId, tb: &mut TextblockData) {
        let last_oof = if tb.lines.is_empty() {
            -1
        } else {
            tb.lines
                .last()
                .map_or(-1, |l| l.last_oof_ref_positioned_before_this_line)
        };
        self.tb_calc_borders(w, tb, last_oof, 1);
        tb.new_line_ascent = 0;
        tb.new_line_descent = 0;
    }

    /// `calcBorders` (linebreaking.cc:2073-2180) — calcule les
    /// bords/hauteurs de floats de la ligne en construction via
    /// les mgrs OOF (`effOofRef` = max(lastOofRef, firstWord-1) —
    /// commentaire floats d'un textblock imbriqué, verbatim).
    fn tb_calc_borders(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        last_oof_ref: i32,
        height: i32,
    ) {
        tb.new_line_has_float_left = false;
        tb.new_line_has_float_right = false;
        tb.new_line_left_border = 0;
        tb.new_line_right_border = 0;
        tb.new_line_left_float_height = 0;
        tb.new_line_right_float_height = 0;

        let mut oofm_defined = false;
        for i in 0..NUM_OOFM {
            if !oofm_defined
                && self
                    .tb_find_size_request_reference(w, tb, tb.oof.oof_container[i])
                    .is_some()
            {
                oofm_defined = true;
            }
        }

        if oofm_defined {
            let first_word_of_line = if tb.lines.is_empty() {
                0
            } else {
                tb.lines.last().map_or(0, |l| l.last_word + 1)
            };
            let eff_oof_ref = last_oof_ref.max(first_word_of_line - 1);
            let y_rel = self.tb_y_offset_of_line_to_be_created(w, tb, None);

            for i in 0..NUM_OOFM {
                let Some(mgr) = self.search_out_of_flow_mgr(w, i) else {
                    continue;
                };
                let Some((_, y_ref)) =
                    self.tb_find_size_request_reference(w, tb, tb.oof.oof_container[i])
                else {
                    continue;
                };
                // Floats d'un textblock imbriqué : seuls les refs
                // > premier mot de la nouvelle ligne comptent —
                // d'où `effOofRef` (commentaire verbatim amont).
                let y = y_ref + y_rel;
                let mut m = mgr.borrow_mut();
                let this_has_left = m.has_float_left(self, y, height, Some(w), eff_oof_ref);
                tb.new_line_has_float_left = tb.new_line_has_float_left || this_has_left;
                let this_has_right = m.has_float_right(self, y, height, Some(w), eff_oof_ref);
                tb.new_line_has_float_right = tb.new_line_has_float_right || this_has_right;

                // « max » pas tout à fait correct pour les hauteurs
                // (verbatim — un seul mgr retourne des valeurs).
                if this_has_left {
                    tb.new_line_left_border = tb.new_line_left_border.max(
                        m.get_left_border(self, y, height, Some(w), eff_oof_ref)
                            - self.w_get_generator_x(w, i),
                    );
                    tb.new_line_left_float_height = tb
                        .new_line_left_float_height
                        .max(m.get_left_float_height(self, y, height, Some(w), eff_oof_ref));
                }
                if this_has_right {
                    tb.new_line_right_border = tb.new_line_right_border.max(
                        m.get_right_border(self, y, height, Some(w), eff_oof_ref)
                            - self.w_get_generator_rest(w, i),
                    );
                    tb.new_line_right_float_height = tb
                        .new_line_right_float_height
                        .max(m.get_right_float_height(self, y, height, Some(w), eff_oof_ref));
                }
            }
        }
    }

    /// `showMissingLines` (linebreaking.cc:2182-2237) — wrappe les
    /// mots non encore dans une ligne avec `wrapAll` ; « mot
    /// temporaire » in-flow ajouté quand le dernier mot est une
    /// référence OOF, puis retiré (verbatim).
    pub(crate) fn tb_show_missing_lines(&mut self, w: WidgetId, tb: &mut TextblockData) {
        let temp_word = tb
            .words
            .last()
            .is_some_and(|w2| w2.content.r#type == content_type::WIDGET_OOF_REF);
        let first_word_to_wrap = if tb.lines.is_empty() {
            0
        } else {
            tb.lines.last().map_or(0, |l| l.last_word + 1)
        };

        if temp_word {
            let size = crate::types::Requisition::default();
            let style = self.node(w, |n| n.style.clone()).unwrap_or_else(|| {
                crate::style::Style::create(&crate::style::StyleAttrs::default())
            });
            self.tb_add_text0(w, tb, b"", 0, WORD_START | WORD_END, &style, &size);
        }

        let mut i = first_word_to_wrap;
        while i < tb.words.len() as i32 {
            self.tb_word_wrap(w, tb, i, true);
            i += 1;
        }

        // Retire le mot temporaire (seule référence : la ligne).
        if temp_word {
            self.tb_cleanup_word(w, tb, tb.words.len() as i32 - 1);
            tb.words.pop();
            if let Some(l) = tb.lines.last_mut() {
                if l.last_word > tb.words.len() as i32 - 1 {
                    l.last_word = tb.words.len() as i32 - 1;
                }
            }
        }

        debug_assert!(
            tb.lines.is_empty()
                || tb.lines.last().map_or(-1, |l| l.last_word) == tb.words.len() as i32 - 1
        );
    }

    /// `removeTemporaryLines` (linebreaking.cc:2239-2258) — tronque
    /// à `nonTemporaryLines` et re-accumule (les données des mots
    /// suivants sont fausses — verbatim).
    fn tb_remove_temporary_lines(&mut self, w: WidgetId, tb: &mut TextblockData) {
        if tb.non_temporary_lines < tb.lines.len() as i32 {
            tb.lines.truncate(tb.non_temporary_lines.max(0) as usize);
            let first_word = if let Some(l) = tb.lines.last() {
                l.last_word + 1
            } else {
                0
            };
            // Recalcule tout pour simplicité (verbatim — les mots
            // de la 1re ligne temporaire étaient en fait corrects).
            for i in first_word..tb.words.len() as i32 {
                self.tb_accumulate_word_data(w, tb, i);
            }
        }
    }

    /// `getLineShrinkability` (linebreaking.cc:2278-2281) —
    /// `return 0` verbatim.
    fn tb_get_line_shrinkability() -> i32 {
        0
    }

    /// `getLineStretchability` (linebreaking.cc:2283-2309) — `0` en
    /// justifié, sinon `stretchabilityFactor * (maxAscent +
    /// maxDescent) / 100` (verbatim).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    fn tb_get_line_stretchability(&self, tb: &TextblockData, last_word_index: i32) -> i32 {
        let last_word = &tb.words[last_word_index as usize];
        if last_word.space_style.attrs.text_align == TextAlignType::Justify {
            0
        } else {
            super::stretchability_factor() * (last_word.max_ascent + last_word.max_descent) / 100
        }
    }

    // ----------------------------------------------------------------
    // Bordures verticales / offsets de ligne (textblock.cc:433-451,
    // 3310-3376) — déplacés ici : utilisés par le wrap.
    // ----------------------------------------------------------------

    /// `calcVerticalBorder` (textblock.cc:433-451) — marge
    /// collapsée ou empilée selon padding/border nul (verbatim).
    pub(crate) fn tb_calc_vertical_border(
        widget_padding: i32,
        widget_border: i32,
        widget_margin: i32,
        line_border_total: i32,
        line_margin_total: i32,
    ) -> i32 {
        if widget_padding == 0 && widget_border == 0 {
            if line_margin_total - line_border_total >= widget_margin {
                line_margin_total
            } else {
                widget_margin + line_border_total
            }
        } else {
            line_margin_total + widget_padding + widget_border + widget_margin
        }
    }

    /// `yOffsetOfLineToBeCreated` (textblock.cc:3310-3345) —
    /// approché par construction (la marge haute de la nouvelle
    /// ligne collapsée n'est connue qu'après création ; utilisé
    /// pour les floats — verbatim, l'imprécision est acceptée).
    pub(crate) fn tb_y_offset_of_line_to_be_created(
        &self,
        w: WidgetId,
        tb: &TextblockData,
        last_margin: Option<&mut i32>,
    ) -> i32 {
        let style = self.widget_style(w);
        let (pad_top, border_top, margin_top) = style.as_ref().map_or((0, 0, 0), |s| {
            (
                s.attrs.padding.top,
                s.attrs.border_width.top,
                s.attrs.margin.top,
            )
        });
        let extra_top = self.node(w, |n| n.extra_space.top);
        let result;
        if tb.lines.is_empty() {
            result =
                Self::tb_calc_vertical_border(pad_top, border_top + extra_top, margin_top, 0, 0);
            if let Some(m) = last_margin {
                *m = margin_top;
            }
        } else {
            let first_line = &tb.lines[0];
            let last_line = tb.lines.last();
            result = Self::tb_calc_vertical_border(
                pad_top,
                border_top,
                margin_top + extra_top,
                first_line.border_ascent,
                first_line.margin_ascent,
            ) - first_line.border_ascent
                + last_line.map_or(0, |l| l.top + l.total_height(0));
            if let Some(m) = last_margin {
                *m = last_line.map_or(0, |l| l.margin_descent - l.border_descent);
            }
        }
        result
    }

    /// `yOffsetOfLineCreated` (textblock.cc:3347-3376) — mêmes
    /// réserves d'exactitude que `yOffsetOfLineToBeCreated`
    /// (verbatim). Utilisable sans allocation.
    pub(crate) fn tb_y_offset_of_line_created(
        &self,
        w: WidgetId,
        tb: &TextblockData,
        line: &Line,
    ) -> i32 {
        let style = self.widget_style(w);
        let (pad_top, border_top, margin_top) = style.as_ref().map_or((0, 0, 0), |s| {
            (
                s.attrs.padding.top,
                s.attrs.border_width.top,
                s.attrs.margin.top,
            )
        });
        let extra_top = self.node(w, |n| n.extra_space.top);
        let first_line = &tb.lines[0];
        Self::tb_calc_vertical_border(
            pad_top,
            border_top,
            margin_top + extra_top,
            first_line.border_ascent,
            first_line.margin_ascent,
        ) - first_line.border_ascent
            + line.top
    }

    /// `getWordExtremes` (textblock.cc:458-467) — extremes d'un mot
    /// : extremes du widget in-flow, ou `size.width` sinon.
    pub(crate) fn tb_get_word_extremes(
        &mut self,
        _w: WidgetId,
        _tb: &TextblockData,
        word: &Word,
        extremes: &mut Extremes,
    ) {
        if word.content.r#type == content_type::WIDGET_IN_FLOW {
            if let ContentData::Widget(child) = word.content.data {
                self.widget_get_extremes(child, extremes, 0, &[], &[], &[]);
            }
        } else {
            extremes.min_width = word.size.width;
            extremes.min_width_intrinsic = word.size.width;
            extremes.max_width = word.size.width;
            extremes.max_width_intrinsic = word.size.width;
            extremes.adjustment_width = word.size.width;
        }
    }

    /// `findParagraphOfWord` (textblock.cc:1662-1683) — recherche
    /// binaire ; `-1` si la liste de paragraphes est incomplète
    /// (voir `fillParagraphs`).
    pub(crate) fn tb_find_paragraph_of_word(&self, tb: &TextblockData, word_index: i32) -> i32 {
        let _ = self;
        if word_index < 0
            || word_index >= tb.words.len() as i32
            || tb
                .paragraphs
                .last()
                .is_none_or(|p| word_index > p.last_word)
        {
            return -1;
        }
        let mut low = 0usize;
        let mut high = tb.paragraphs.len() - 1;
        loop {
            let index = usize::midpoint(low, high);
            let par = &tb.paragraphs[index];
            if word_index >= par.first_word {
                if word_index <= par.last_word {
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

    // ----------------------------------------------------------------
    // Générateurs (textblock.cc:3113-3251) — overrides de
    // `OOFAwareWidget` dispatchés par `w_get_generator_*`.
    // ----------------------------------------------------------------

    /// `Textblock::getGeneratorX` (textblock.cc:3113-3129) —
    /// `findSizeRequestReference (oofmIndex)` ≙ référence =
    /// `oofContainer[oofmIndex]` (hh:688-690) ;
    /// `assertNotReached` → `debug_assert` + 0 (floats seulement).
    pub(crate) fn tb_get_generator_x(
        &self,
        w: WidgetId,
        tb: &TextblockData,
        oofm_index: usize,
    ) -> i32 {
        if let Some((x_ref, _)) =
            self.tb_find_size_request_reference(w, tb, tb.oof.oof_container[oofm_index])
        {
            x_ref
        } else {
            debug_assert!(false, "getGeneratorX: référence absente");
            0
        }
    }

    /// `Textblock::getGeneratorY` (textblock.cc:3131-3145).
    pub(crate) fn tb_get_generator_y(
        &self,
        w: WidgetId,
        tb: &TextblockData,
        oofm_index: usize,
    ) -> i32 {
        if let Some((_, y_ref)) =
            self.tb_find_size_request_reference(w, tb, tb.oof.oof_container[oofm_index])
        {
            y_ref
        } else {
            debug_assert!(false, "getGeneratorY: référence absente");
            0
        }
    }

    /// `Textblock::getGeneratorRest` (textblock.cc:3147-3168).
    pub(crate) fn tb_get_generator_rest(
        &mut self,
        w: WidgetId,
        tb: &TextblockData,
        oofm_index: usize,
    ) -> i32 {
        let container = tb.oof.oof_container[oofm_index];
        if let Some(c) = container {
            if let Some((x_ref, _)) = self.tb_find_size_request_reference(w, tb, Some(c)) {
                return self.w_get_generator_width(c)
                    - (x_ref + self.tb_get_generator_width(w, tb));
            }
        }
        debug_assert!(false, "getGeneratorRest: référence absente");
        0
    }

    /// `Textblock::getGeneratorWidth` (textblock.cc:3170-3213) —
    /// `lineBreakWidth` si `usesMaxGeneratorWidth`, sinon
    /// `max(wThis, wChild)` où `wChild` sonde les textblocks
    /// enfants après la dernière ligne (cas test verbatim).
    pub(crate) fn tb_get_generator_width(&mut self, w: WidgetId, tb: &TextblockData) -> i32 {
        if self.tb_uses_max_generator_width(w, tb) {
            tb.line_break_width
        } else {
            // `wChild` : les enfants textblock non encore dans une
            // ligne (commentaire verbatim amont — float imbriqué).
            let mut w_child = 0i32;
            let first_word_after_last_line = tb.lines.last().map_or(0, |l| l.last_word + 1);
            for i in first_word_after_last_line..tb.words.len() as i32 {
                let word = &tb.words[i as usize];
                if word.content.r#type == content_type::WIDGET_IN_FLOW {
                    if let ContentData::Widget(child) = word.content.data {
                        if self.node(child, |n| is_textblock_kind(&n.kind)) {
                            if let Some(x_rel) = self
                                .tb_find_size_request_reference(w, tb, Some(w))
                                .map(|(x, _)| x)
                            {
                                let child_w = self
                                    .with_tb(child, |l, tbc| l.tb_get_generator_width(child, tbc));
                                w_child = w_child.max(x_rel + child_w.unwrap_or(0));
                            }
                        }
                    }
                }
            }
            let w0 = tb.lines.last().map_or(0, |l| l.max_line_width);
            let w_this = (w0
                + tb.left_inner_padding
                + self.node(w, super::super::widget::WidgetNode::box_diff_width))
            .min(tb.line_break_width);
            w_this.max(w_child)
        }
    }

    /// `Textblock::getMaxGeneratorWidth` (textblock.cc:3215-3221).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_get_max_generator_width(&self, tb: &TextblockData) -> i32 {
        tb.line_break_width
    }

    /// `Textblock::usesMaxGeneratorWidth` (textblock.cc:3223-3251)
    /// — `treatAsInline` → `false` ; sinon `toplevel ||
    /// (block && !(float|abspos|fixpos))` (verbatim).
    pub(crate) fn tb_uses_max_generator_width(&self, w: WidgetId, tb: &TextblockData) -> bool {
        // `ListItem::usesMaxGeneratorWidth` (listitem.cc:42-44) et
        // `tablecell::usesMaxGeneratorWidth` (tablecell.hh:19)
        // retournent `true` inconditionnellement.
        if tb.aligned.is_some() || tb.cell.is_some() {
            return true;
        }
        if tb.treat_as_inline {
            false
        } else {
            let toplevel = self.widget_parent(w).is_none();
            let attrs = self.widget_style(w).map(|s| s.attrs.clone());
            let (block, vloat, abspos, fixpos) =
                attrs.as_ref().map_or((false, false, false, false), |a| {
                    (
                        a.display == DisplayType::Block,
                        crate::oof::test_style_float(a),
                        crate::oof::test_style_absolutely_positioned(a),
                        crate::oof::test_style_fixedly_positioned(a),
                    )
                });
            toplevel || (block && !(vloat || abspos || fixpos))
        }
    }
}

/// `getSpaceStretchability` (linebreaking.cc:2268-2275) —
/// `origSpace / 2` en justifié, sinon `0` (verbatim).
fn space_stretchability(word: &Word) -> i32 {
    if word.space_style.attrs.text_align == TextAlignType::Justify {
        i32::from(word.orig_space) / 2
    } else {
        0
    }
}

/// `getSpaceShrinkability` (linebreaking.cc:2260-2266) —
/// `origSpace / 3` en justifié, sinon `0` (verbatim).
fn space_shrinkability(word: &Word) -> i32 {
    if word.space_style.attrs.text_align == TextAlignType::Justify {
        i32::from(word.orig_space) / 3
    } else {
        0
    }
}
