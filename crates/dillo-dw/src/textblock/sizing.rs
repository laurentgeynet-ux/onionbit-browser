//! `Textblock` — dimensionnement : `sizeRequestImpl`,
//! `getExtremesSimpl`, `sizeAllocateImpl`, disponibilité pour les
//! enfants, notifications de conteneur et références OOF.
//!
//! Port verbatim de `textblock.cc` (sections sizing) adapté à
//! l'arène `Layout`/`WidgetId`.

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use crate::layout::Layout;
use crate::oof::{
    get_parent_ref_in_flow_sub_ref, get_parent_ref_oof_sub_ref, handled_by_stacking_context_mgr,
    is_parent_ref_oof, sl, NUM_OOFM, OOFM_FLOATS,
};
use crate::style::{ClearType, LENGTH_AUTO};
use crate::types::{
    content_type, Allocation, ContentData, Extremes, GettingWidgetAtPointContext, Requisition,
};
use crate::widget::split_height_preserve_ascent;
use crate::WidgetId;

use super::TextblockData;

impl Layout {
    // ------------------------------------------------------------
    // Requête de taille (textblock.cc:309-456).
    // ------------------------------------------------------------

    /// `Textblock::sizeRequestImpl` (textblock.cc:309-421).
    ///
    /// Re-wrappe si la largeur dispo ou la position dans le conteneur
    /// de flottants a changé, remplit les lignes manquantes, calcule
    /// la requête à partir des lignes (fusion marges verticales
    /// verbatim `calcVerticalBorder`), borne à `lineBreakWidth` si
    /// `usesMaxGeneratorWidth`, puis `correctRequisition` +
    /// `correctRequisitionByOOF` (ordre inhabituel verbatim — voir
    /// commentaire amont).
    pub(crate) fn tb_size_request_impl(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        requisition: &mut Requisition,
        references: &[WidgetId],
        x: &[i32],
        y: &[i32],
    ) {
        tb.size_request_params.fill(references, x, y);

        // Re-wrap si (i) la largeur de césure a changé ou (ii) la
        // position dans le conteneur de flottants a changé
        // (simplification verbatim — commentaire amont).
        let new_line_break_width = self.widget_get_avail_width(w, true);
        let fc = self.tb_find_size_request_reference(w, tb, tb.oof.oof_container[OOFM_FLOATS]);

        if new_line_break_width != tb.line_break_width
            || fc.is_some_and(|(nx, ny)| nx != tb.wrap_ref_lines_fcx || ny != tb.wrap_ref_lines_fcy)
        {
            tb.line_break_width = new_line_break_width;
            tb.wrap_ref_lines = 0;

            if fc.is_none() {
                // Amont assigne `newFCX`/`newFCY` **non initialisés**
                // quand `!fcDefined` — le port borne à (0, 0)
                // (déviation documentée : les valeurs ne sont de
                // toute façon jamais relues utilement).
                tb.wrap_ref_lines_fcx = 0;
                tb.wrap_ref_lines_fcy = 0;
            }
        }

        self.tb_rewrap(w, tb);
        self.tb_show_missing_lines(w, tb);

        if tb.lines.is_empty() {
            requisition.width = tb.left_inner_padding
                + self.node(w, super::super::widget::WidgetNode::box_diff_width);
            requisition.ascent = self.node(w, super::super::widget::WidgetNode::box_offset_y);
            requisition.descent = self.node(w, super::super::widget::WidgetNode::box_rest_height);
        } else {
            let first_line = tb.lines[0].clone();
            let last_line = tb.lines.last().unwrap_or(&first_line).clone();
            let style = self
                .widget_style(w)
                .map_or_else(crate::style::StyleAttrs::default, |s| s.attrs.clone());

            // `breakSpace` de la dernière ligne ignoré — les césures
            // en fin de bloc ne sont pas visibles (verbatim).
            requisition.width = last_line.max_line_width
                + tb.left_inner_padding
                + self.node(w, super::super::widget::WidgetNode::box_diff_width);

            // Fusion marge haut du widget / marge haut de la
            // première boîte de ligne.
            requisition.ascent = calc_vertical_border(
                style.padding.top,
                style.border_width.top,
                style.margin.top + self.node(w, |n| n.extra_space.top),
                first_line.border_ascent,
                first_line.margin_ascent,
            );

            // Idem bas : `lastLine->top - firstLine->borderAscent +
            // lastLine->borderAscent` vaut 0 pour une seule ligne
            // (verbatim — pas de cas spécial).
            requisition.descent = last_line.top - first_line.border_ascent
                + last_line.border_ascent
                + calc_vertical_border(
                    style.padding.bottom,
                    style.border_width.bottom,
                    style.margin.bottom + self.node(w, |n| n.extra_space.bottom),
                    last_line.border_descent,
                    last_line.margin_descent,
                );
        }

        if self.tb_uses_max_generator_width(w, tb) && requisition.width < tb.line_break_width {
            requisition.width = tb.line_break_width;
        }

        self.widget_correct_requisition(w, requisition, split_height_preserve_ascent, true, false);

        // Correction OOF *après* `correctRequisition` — verbatim
        // (commentaire amont : sinon la taille peut redevenir trop
        // petite pour les flottants → resize idle en boucle).
        self.correct_requisition_by_oof(w, requisition, split_height_preserve_ascent);
    }

    /// `numSizeRequestReferences` (textblock.cc:423-426).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_num_size_request_references(&self, tb: &TextblockData) -> usize {
        tb.num_size_references as usize
    }

    /// `sizeRequestReference` (textblock.cc:428-431).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_size_request_reference(
        &self,
        tb: &TextblockData,
        index: usize,
    ) -> Option<WidgetId> {
        tb.size_references.get(index).copied().flatten()
    }

    /// `calcVerticalBorder` (textblock.cc:433-456) — fonction libre
    /// `calc_vertical_border` ci-dessous ; `getWordExtremes`
    /// (textblock.cc:458-466) est dans `breaking.rs` (verbatim —
    /// utilisée par le calcul de `parMin`/`parMax`).
    ///
    /// `Textblock::getExtremesSimpl` (textblock.cc:468-527) —
    /// cumuls de paragraphe + `leftInnerPadding + boxDiffWidth`,
    /// `correctExtremes` puis `correctExtremesByOOF` (ordre
    /// verbatim — commentaire amont).
    pub(crate) fn tb_get_extremes_simpl(&mut self, w: WidgetId, extremes: &mut Extremes) {
        self.with_tb(w, |l, tb| {
            l.tb_fill_paragraphs(w, tb);

            if let Some(last_par) = tb.paragraphs.last() {
                extremes.min_width = last_par.max_par_min;
                extremes.min_width_intrinsic = last_par.max_par_min_intrinsic;
                extremes.max_width = last_par.max_par_max;
                extremes.max_width_intrinsic = last_par.max_par_max_intrinsic;
                extremes.adjustment_width = last_par.max_par_adjustment_width;
            } else {
                // Page vide (verbatim).
                *extremes = Extremes::default();
            }

            let diff =
                tb.left_inner_padding + l.node(w, super::super::widget::WidgetNode::box_diff_width);
            extremes.min_width += diff;
            extremes.min_width_intrinsic += diff;
            extremes.max_width += diff;
            extremes.max_width_intrinsic += diff;
            extremes.adjustment_width += diff;
        });

        self.widget_correct_extremes(w, extremes, true);
        self.correct_extremes_by_oof(w, extremes);
    }

    /// `numGetExtremesReferences` (textblock.cc:529-532).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_num_get_extremes_references(&self, tb: &TextblockData) -> usize {
        tb.num_size_references as usize
    }

    /// `getExtremesReference` (textblock.cc:534-537).
    #[allow(clippy::unused_self)] // signatures verbatim amont.
    pub(crate) fn tb_get_extremes_reference(
        &self,
        tb: &TextblockData,
        index: usize,
    ) -> Option<WidgetId> {
        tb.size_references.get(index).copied().flatten()
    }

    /// `notifySetAsTopLevel` (textblock.cc:539-545) — le dispatch
    /// `w_notify_set_as_top_level` a déjà appelé la partie
    /// `OOFAwareWidget` ; reste `numSizeReferences = 0`.
    pub(crate) fn tb_notify_set_as_top_level(&mut self, w: WidgetId) {
        let _ = self.node_tb_mut(w, |tb| tb.num_size_references = 0);
    }

    /// `notifySetParent` (textblock.cc:547-568) — après la partie
    /// `OOFAwareWidget` : containing blocks ≠ soi-même, dédupliqués,
    /// dans `sizeReferences`.
    pub(crate) fn tb_notify_set_parent(&mut self, w: WidgetId) {
        let _ = self.node_tb_mut(w, |tb| {
            tb.num_size_references = 0;
            for i in 0..NUM_OOFM {
                let Some(oc) = tb.oof.oof_container[i] else {
                    continue;
                };
                if oc == w {
                    continue;
                }
                // Dédupliqué (verbatim).
                if tb
                    .size_references
                    .iter()
                    .take(tb.num_size_references as usize)
                    .any(|r| *r == Some(oc))
                {
                    continue;
                }
                if (tb.num_size_references as usize) < tb.size_references.len() {
                    tb.size_references[tb.num_size_references as usize] = Some(oc);
                    tb.num_size_references += 1;
                }
            }
        });
    }

    // ------------------------------------------------------------
    // Allocation (textblock.cc:570-733).
    // ------------------------------------------------------------

    /// `sizeAllocateImpl` (textblock.cc:570-733).
    ///
    /// Alloue les widgets in-flow, suit `redrawY` (déplacement /
    /// changement de hauteur des enfants — optimisations verbatim
    /// ligne seule + BREAK optionnel), puis `sizeAllocateStart/End`
    /// et mise à jour des ancres (`changeAnchor`).
    pub(crate) fn tb_size_allocate_impl(&mut self, w: WidgetId, allocation: &Allocation) {
        self.with_tb(w, |l, tb| {
            l.tb_show_missing_lines(w, tb);
        });
        self.oof_size_allocate_start(w, allocation);

        if self.node(w, |n| {
            allocation.x != n.allocation.x
                || allocation.y != n.allocation.y
                || allocation.width != n.allocation.width
        }) {
            let _ = self.node_tb_mut(w, |tb| tb.redraw_y = 0);
        }

        self.with_tb(w, |l, tb| {
            for line_index in 0..tb.lines.len() {
                // `allocation->width` peut différer de la largeur de
                // césure (flottants) ; bornée par `lineBreakWidth`
                // si plus grande (verbatim + TODO amont).
                l.tb_calc_text_offset(tb, line_index, allocation.width.min(tb.line_break_width));

                let (text_offset, first_word, last_word) = {
                    let line = &tb.lines[line_index];
                    (line.text_offset, line.first_word, line.last_word)
                };
                let mut x_cursor = text_offset;

                for word_index in first_word.max(0)..=last_word {
                    let Some(word_idx) = usize::try_from(word_index).ok() else {
                        continue;
                    };
                    let Some(word) = tb.words.get(word_idx) else {
                        continue;
                    };

                    if word_index == tb.last_word_drawn + 1 {
                        let line = tb.lines[line_index].clone();
                        tb.redraw_y = tb.redraw_y.min(tb.line_y_offset_widget(&line, allocation));
                    }

                    let content = word.content.data.clone();
                    let content_type = word.content.r#type;
                    let eff_space = word.eff_space;
                    let word_size = word.size;
                    if content_type == content_type::WIDGET_IN_FLOW {
                        if let ContentData::Widget(child) = content {
                            let line = tb.lines[line_index].clone();
                            let child_alloc = Allocation {
                                x: x_cursor + allocation.x,
                                // align=bottom (ligne de base) —
                                // variantes commentées verbatim.
                                y: tb.line_y_offset_canvas(&line, allocation)
                                    + (line.border_ascent - word_size.ascent),
                                width: word_size.width,
                                ascent: word_size.ascent,
                                descent: word_size.descent,
                            };

                            let old = l.node(child, |n| n.allocation);
                            if child_alloc.x != old.x
                                || child_alloc.y != old.y
                                || child_alloc.width != old.width
                            {
                                // Position ou largeur changée →
                                // redessiner depuis cette ligne.
                                let line = tb.lines[line_index].clone();
                                tb.redraw_y =
                                    tb.redraw_y.min(tb.line_y_offset_widget(&line, allocation));
                                if l.node(child, super::super::widget::WidgetNode::was_allocated) {
                                    tb.redraw_y = tb.redraw_y.min(old.y - allocation.y);
                                }
                            } else if child_alloc.ascent + child_alloc.descent
                                != old.ascent + old.descent
                            {
                                // Hauteur changée → redessiner à
                                // partir du changement (pas la ligne
                                // entière — verbatim, l'enfant peut
                                // être une table pleine page).
                                // Possible seulement si le widget est
                                // le seul mot de la ligne (+ BREAK).
                                let line = tb.lines[line_index].clone();
                                let only_word = line.last_word == line.first_word
                                    || (line.last_word == line.first_word + 1
                                        && tb
                                            .words
                                            .get(usize::try_from(line.last_word).unwrap_or(0))
                                            .is_some_and(|lw| {
                                                lw.content.r#type == content_type::BREAK
                                            }));
                                if only_word {
                                    let child_changed_y = (child_alloc.y - allocation.y
                                        + child_alloc.ascent
                                        + child_alloc.descent)
                                        .min(old.y - allocation.y + old.ascent + old.descent);
                                    tb.redraw_y = tb.redraw_y.min(child_changed_y);
                                } else {
                                    tb.redraw_y =
                                        tb.redraw_y.min(tb.line_y_offset_widget(&line, allocation));
                                }
                            }
                            l.widget_size_allocate(child, &child_alloc);
                        }
                    }

                    x_cursor += word_size.width + i32::from(eff_space);
                }
            }
        });

        self.oof_size_allocate_end(w);

        self.with_tb(w, |l, tb| {
            let anchors: Vec<(Vec<u8>, i32)> = tb
                .anchors
                .iter()
                .map(|a| (a.name.clone(), a.word_index))
                .collect();
            for (name, word_index) in anchors {
                let y = if word_index >= tb.words.len() as i32
                    || tb.lines.is_empty()
                    || tb.lines.last().is_some_and(|ll| word_index > ll.last_word)
                {
                    // Lignes pas encore créées — verbatim.
                    allocation.y + allocation.ascent + allocation.descent
                } else {
                    let li = l.tb_find_line_of_word(tb, word_index);
                    let line = tb.lines[li as usize].clone();
                    tb.line_y_offset_canvas(&line, allocation)
                };
                l.change_anchor(w, &name, y);
            }
        });
    }

    /// `calcExtraSpaceImpl` (textblock.cc:735-754) —
    /// `sizeRequestParams.fill` puis `clearPosition` max des mgrs
    /// (base `OOFAwareWidget::calcExtraSpaceImpl` ≙ vide).
    pub(crate) fn tb_calc_extra_space_impl(
        &mut self,
        w: WidgetId,
        tb: &mut TextblockData,
        references: &[WidgetId],
        x: &[i32],
        y: &[i32],
    ) {
        tb.size_request_params.fill(references, x, y);

        let mut clear_position = 0;
        for i in 0..NUM_OOFM {
            let defined = self.tb_find_size_request_reference(w, tb, tb.oof.oof_container[i]);
            if defined.is_none() {
                continue;
            }
            if let Some(mgr) = self.search_out_of_flow_mgr(w, i) {
                let pos = {
                    let mut m = mgr.borrow_mut();
                    m.get_clear_position(self, w)
                };
                clear_position = clear_position.max(pos);
            }
        }

        self.node_mut(w, |n| {
            n.extra_space.top = n.extra_space.top.max(clear_position);
        });
    }

    // ------------------------------------------------------------
    // Disponibilité pour les enfants (textblock.cc:756-876).
    // ------------------------------------------------------------

    /// `getAvailWidthOfChild` (textblock.cc:756-808).
    ///
    /// OOF géré par son mgr ; sinon `auto` → largeur dispo moins
    /// `boxDiffWidth + leftInnerPadding` (clamp min/max verbatim),
    /// non-`auto` → base ; puis borne `extremes.maxWidth` quand
    /// in-flow et `!usesMaxGeneratorWidth` (verbatim).
    pub(crate) fn tb_get_avail_width_of_child(
        &mut self,
        w: WidgetId,
        child: WidgetId,
        force_value: bool,
    ) -> i32 {
        let oof = self.widget_is_oof(child)
            && self
                .widget_out_of_flow_mgr(w, child)
                .is_some_and(|m| m.borrow().dealing_with_size_of_child(child));
        if oof {
            if let Some(mgr) = self.widget_out_of_flow_mgr(w, child) {
                return mgr
                    .borrow_mut()
                    .get_avail_width_of_child(self, child, force_value);
            }
        }

        let (left_inner, uses_max) = self
            .node_tb(w, |tb| {
                (
                    tb.left_inner_padding,
                    self.tb_uses_max_generator_width(w, tb),
                )
            })
            .unwrap_or((0, false));

        let child_style = self.widget_style(child);
        let mut width = if child_style
            .as_ref()
            .is_some_and(|s| s.attrs.width == LENGTH_AUTO)
        {
            if force_value {
                let mut width = (self.widget_get_avail_width(w, true)
                    - self.node(w, super::super::widget::WidgetNode::box_diff_width)
                    - left_inner)
                    .max(0);
                if width != -1 {
                    // Clamp min/max-width en tenant compte de
                    // `leftInnerPadding` (verbatim).
                    if let Some(s) = &child_style {
                        let max_width = self.widget_calc_width(
                            child,
                            s.attrs.max_width,
                            -1,
                            Some(w),
                            -1,
                            false,
                        );
                        if max_width != -1 && width > max_width - left_inner {
                            width = max_width - left_inner;
                        }
                        let min_width = self.widget_calc_width(
                            child,
                            s.attrs.min_width,
                            -1,
                            Some(w),
                            -1,
                            false,
                        );
                        if min_width != -1 && width < min_width - left_inner {
                            width = min_width - left_inner;
                        }
                    }
                }
                width
            } else {
                -1
            }
        } else {
            self.w_get_avail_width_of_child(w, child, force_value)
        };

        if force_value && self.node(child, |n| n.container) == Some(w) && !uses_max {
            let mut extremes = Extremes::default();
            self.widget_get_extremes(w, &mut extremes, 0, &[], &[], &[]);
            let limit = extremes.max_width
                - self.node(w, super::super::widget::WidgetNode::box_diff_width)
                - left_inner;
            if width > limit {
                width = limit;
            }
        }
        width
    }

    /// `getAvailHeightOfChild` (textblock.cc:810-818) — mgr OOF ou
    /// base (verbatim).
    pub(crate) fn tb_get_avail_height_of_child(
        &mut self,
        w: WidgetId,
        child: WidgetId,
        force_value: bool,
    ) -> i32 {
        let oof = self.widget_is_oof(child)
            && self
                .widget_out_of_flow_mgr(w, child)
                .is_some_and(|m| m.borrow().dealing_with_size_of_child(child));
        if oof {
            if let Some(mgr) = self.widget_out_of_flow_mgr(w, child) {
                return mgr
                    .borrow_mut()
                    .get_avail_height_of_child(self, child, force_value);
            }
        }
        self.w_get_avail_height_of_child(w, child, force_value)
    }

    /// `containerSizeChangedForChildren` (textblock.cc:820-833) —
    /// itération verbatim des `WIDGET_IN_FLOW` +
    /// `containerSizeChangedForChildrenOOF`.
    pub(crate) fn tb_container_size_changed_for_children(&mut self, w: WidgetId) {
        let children: Vec<WidgetId> = self
            .node_tb(w, |tb| {
                tb.words
                    .iter()
                    .filter(|wd| wd.content.r#type == content_type::WIDGET_IN_FLOW)
                    .filter_map(|wd| match wd.content.data {
                        ContentData::Widget(c) => Some(c),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        for c in children {
            self.widget_container_size_changed(c);
        }
        self.oof_container_size_changed_for_children(w);
    }

    /// `affectsSizeChangeContainerChild` (textblock.cc:835-851) —
    /// les changements d'extrêmes affectent la taille de l'enfant
    /// quand `!usesMaxGeneratorWidth` (verbatim).
    pub(crate) fn tb_affects_size_change_container_child(
        &mut self,
        w: WidgetId,
        child: WidgetId,
    ) -> bool {
        let uses_max = self
            .node_tb(w, |tb| self.tb_uses_max_generator_width(w, tb))
            .unwrap_or(false);
        if !uses_max && self.node(w, |n| n.extremes_queued() || n.extremes_changed()) {
            true
        } else {
            self.w_affects_size_change_container_child(w, child)
        }
    }

    /// `resizeDrawImpl` (textblock.cc:858-876) — `queueDrawArea`
    /// depuis `redrawY`, `lastWordDrawn`, reset `redrawY` (verbatim).
    pub(crate) fn tb_resize_draw_impl(&mut self, w: WidgetId) {
        let (redraw_y, width, height, last_line_last_word) = self
            .node_tb(w, |tb| {
                (
                    tb.redraw_y,
                    self.node(w, |n| n.allocation.width),
                    self.node(w, super::super::widget::WidgetNode::get_height),
                    tb.lines.last().map(|l| l.last_word),
                )
            })
            .unwrap_or((0, 0, 0, None));

        self.widget_queue_draw_area(w, 0, redraw_y, width, height - redraw_y);

        if let Some(lw) = last_line_last_word {
            let _ = self.node_tb_mut(w, |tb| tb.last_word_drawn = lw);
        }
        let _ = self.node_tb_mut(w, |tb| {
            tb.redraw_y = self.node(w, super::super::widget::WidgetNode::get_height)
        });
    }

    /// `markSizeChange` (textblock.cc:878-910) — OOF → mgr ;
    /// sinon `wrapRefLines` min + `markExtremesChange` (verbatim —
    /// `ref == -1` : pas de ligne pour l'instant, `addLine` gèrera).
    pub(crate) fn tb_mark_size_change(&mut self, w: WidgetId, ref_: i32) {
        if is_parent_ref_oof(ref_) {
            if let Some(mgr) = self.parent_ref_out_of_flow_mgr(w, ref_) {
                mgr.borrow_mut()
                    .mark_size_change(self, get_parent_ref_oof_sub_ref(ref_));
            }
        } else {
            if ref_ != -1 {
                let sub = get_parent_ref_in_flow_sub_ref(ref_);
                let _ = self.node_tb_mut(w, |tb| {
                    if tb.wrap_ref_lines == -1 {
                        tb.wrap_ref_lines = sub;
                    } else {
                        tb.wrap_ref_lines = tb.wrap_ref_lines.min(sub);
                    }
                });
            }
            // Verbatim : « workaround » — même ref vers
            // markExtremesChange (la structure des lignes peut
            // avoir changé).
            self.w_mark_extremes_change(w, ref_);
        }
    }

    /// `markExtremesChange` (textblock.cc:912-937).
    pub(crate) fn tb_mark_extremes_change(&mut self, w: WidgetId, ref_: i32) {
        if is_parent_ref_oof(ref_) {
            if let Some(mgr) = self.parent_ref_out_of_flow_mgr(w, ref_) {
                mgr.borrow_mut()
                    .mark_extremes_change(self, get_parent_ref_oof_sub_ref(ref_));
            }
        } else if ref_ != -1 {
            let sub = get_parent_ref_in_flow_sub_ref(ref_);
            let _ = self.node_tb_mut(w, |tb| {
                if tb.wrap_ref_paragraphs == -1 {
                    tb.wrap_ref_paragraphs = sub;
                } else {
                    tb.wrap_ref_paragraphs = tb.wrap_ref_paragraphs.min(sub);
                }
            });
        }
    }

    /// `updateReference` (textblock.cc:3080-3090) — `queueResize`
    /// seulement s'il y a des mots ou `clear` (verbatim).
    pub(crate) fn tb_update_reference(&mut self, w: WidgetId, ref_: i32) {
        let has_words_or_clear = self.node_tb(w, |tb| !tb.words.is_empty()).unwrap_or(false)
            || self
                .widget_style(w)
                .is_some_and(|s| s.attrs.clear != ClearType::None);
        if has_words_or_clear {
            self.widget_queue_resize(w, ref_, false);
        }
    }

    /// `widgetRefSizeChanged` (textblock.cc:3092-3097).
    pub(crate) fn tb_widget_ref_size_changed(&mut self, w: WidgetId, external_index: i32) {
        let line_no = self
            .node_tb(w, |tb| self.tb_find_line_of_word(tb, external_index))
            .unwrap_or(-1);
        let n_lines = self.node_tb(w, |tb| tb.lines.len() as i32).unwrap_or(0);
        if line_no >= 0 && line_no < n_lines {
            self.widget_queue_resize(w, crate::oof::make_parent_ref_in_flow(line_no), true);
        }
    }

    /// `oofSizeChanged` (textblock.cc:3099-3111) — `queueResize`
    /// + `containerSizeChanged` si extrêmes et
    ///   `!usesMaxGeneratorWidth` (verbatim).
    pub(crate) fn tb_oof_size_changed(&mut self, w: WidgetId, extremes_changed: bool) {
        self.widget_queue_resize(w, -1, extremes_changed);
        let uses_max = self
            .node_tb(w, |tb| self.tb_uses_max_generator_width(w, tb))
            .unwrap_or(false);
        if extremes_changed && !uses_max {
            self.widget_container_size_changed(w);
        }
    }

    /// `queueDrawRange` (textblock.cc:3051-3078).
    pub(crate) fn tb_queue_draw_range(&mut self, w: WidgetId, index1: i32, index2: i32) {
        self.with_tb(w, |l, tb| {
            if tb.words.is_empty() {
                return;
            }
            let last = tb.words.len() as i32 - 1;
            let from = index1.min(index2).min(last).max(0);
            let to = index1.max(index2).min(last).max(0);

            let line1idx = l.tb_find_line_of_word(tb, from);
            let line2idx = l.tb_find_line_of_word(tb, to);
            if line1idx < 0 || line2idx < 0 {
                return;
            }
            let (line1, line2) = (
                tb.lines[line1idx as usize].clone(),
                tb.lines[line2idx as usize].clone(),
            );
            let alloc = l.node(w, |n| n.allocation);
            let y = tb.line_y_offset_widget(&line1, &alloc) + line1.border_ascent
                - line1.content_ascent;
            let h = tb.line_y_offset_widget(&line2, &alloc)
                + line2.border_ascent
                + line2.content_descent
                - y;
            l.widget_queue_draw_area(w, 0, y, alloc.width, h);
        });
    }

    /// `getWidgetAtPointLevel` (textblock.cc:2891-2954) —
    /// `SL_IN_FLOW` : widgets in-flow de la ligne sous `y`
    /// (dernier au premier, hors stacking ctx) ; `SL_OOF_REF` :
    /// références OOF interrompant le dessin, mgr descendant ;
    /// sinon base `OOFAwareWidget` (verbatim).
    pub(crate) fn tb_get_widget_at_point_level(
        &mut self,
        w: WidgetId,
        x: i32,
        y: i32,
        level: i32,
        context: &mut GettingWidgetAtPointContext,
    ) -> Option<WidgetId> {
        match level {
            sl::IN_FLOW => {
                let rel_y = y - self.node(w, |n| n.allocation.y);
                let line_index = self
                    .node_tb(w, |tb| self.tb_find_line_index_when_allocated(w, tb, rel_y))
                    .unwrap_or(-1);
                let n_lines = self.node_tb(w, |tb| tb.lines.len() as i32).unwrap_or(0);
                if line_index < 0 || line_index >= n_lines {
                    return None;
                }
                let (first, last) = self
                    .node_tb(w, |tb| {
                        let line = &tb.lines[line_index as usize];
                        (line.first_word, line.last_word)
                    })
                    .unwrap_or((0, -1));
                for word_index in (first..=last).rev() {
                    let child = self.node_tb(w, |tb| {
                        usize::try_from(word_index)
                            .ok()
                            .and_then(|i| tb.words.get(i))
                            .and_then(|wd| {
                                if wd.content.r#type == content_type::WIDGET_IN_FLOW {
                                    match &wd.content.data {
                                        ContentData::Widget(c) => Some(*c),
                                        _ => None,
                                    }
                                } else {
                                    None
                                }
                            })
                    });
                    let Some(child) = child.flatten() else {
                        continue;
                    };
                    if !handled_by_stacking_context_mgr(self, child) {
                        if let Some(found) = self.w_get_widget_at_point(child, x, y, context) {
                            return Some(found);
                        }
                    }
                }
                None
            }
            sl::OOF_REF => {
                // TODO amont : inefficace (liste séparée ?).
                for oofm_index in (0..NUM_OOFM).rev() {
                    let words_count = self.node_tb(w, |tb| tb.words.len()).unwrap_or(0);
                    for word_index in (0..words_count).rev() {
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
                            if let Some(found) =
                                self.widget_get_widget_at_point_interrupted(rw, x, y, context)
                            {
                                return Some(found);
                            }
                        }
                    }
                }
                None
            }
            _ => self.oof_get_widget_at_point_level(w, x, y, level, context),
        }
    }
}

/// `calcVerticalBorder` (textblock.cc:433-456) — fusion de la
/// marge verticale du widget et de la boîte de ligne.
#[must_use]
pub(crate) fn calc_vertical_border(
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
