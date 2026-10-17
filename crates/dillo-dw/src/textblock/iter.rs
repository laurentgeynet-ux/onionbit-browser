//! `textblock_iterator.cc` — `Textblock::TextblockIterator` :
//! `OofAwareIterator` dont la section en-flux est la liste des
//! mots (`TbInFlow`), avec `highlight`/`unhighlight`/`getAllocation`
//! verbatim.

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use crate::layout::Layout;
use crate::oof::{OofAwareIterator, OofInFlow};
use crate::types::{content_type, Allocation, Content, ContentData, HighlightLayer};
use crate::WidgetId;

use super::WORD_END;
use super::WORD_START;

/// `Textblock::TextblockIterator` — en pratique un
/// [`OofAwareIterator`] portant `TbInFlow` (≙ sous-classe amont,
/// les virtuels in-flow sont délégués au trait `OofInFlow`).
pub type TextblockIterator = OofAwareIterator;

/// `new TextblockIterator (textblock, mask, atEnd)`
/// (textblock_iterator.cc:28-33) — `OofAwareWidgetIterator` avec
/// `numParts = words->size()`.
#[must_use]
pub fn textblock_iterator(
    layout: &Layout,
    w: WidgetId,
    mask: i16,
    at_end: bool,
) -> TextblockIterator {
    let n = layout.node_tb(w, |tb| tb.words.len() as i32).unwrap_or(0);
    OofAwareIterator::new(w, mask, at_end, Box::new(TbInFlow { widget: w }), layout, n)
}

/// `TextblockIterator::createWordIndexIterator`
/// (textblock_iterator.cc:36-43) — itérateur positionné sur
/// `wordIndex` en flux.
#[must_use]
pub fn textblock_word_index_iterator(
    layout: &Layout,
    w: WidgetId,
    mask: i16,
    word_index: i32,
) -> TextblockIterator {
    let n = layout.node_tb(w, |tb| tb.words.len() as i32).unwrap_or(0);
    let mut it = OofAwareIterator::new(w, mask, false, Box::new(TbInFlow { widget: w }), layout, n);
    it.set_values(layout, 0, word_index);
    it
}

/// Section en-flux de `TextblockIterator` — verbatim
/// `textblock_iterator.cc:46-283`.
pub struct TbInFlow {
    /// Le textblock (≙ `getWidget()` amont).
    widget: WidgetId,
}

impl OofInFlow for TbInFlow {
    /// `numContentsInFlow` (textblock_iterator.cc:269-272) —
    /// `words->size()`.
    fn num_contents_in_flow(&self, layout: &Layout) -> i32 {
        layout
            .node_tb(self.widget, |tb| tb.words.len() as i32)
            .unwrap_or(0)
    }

    /// `getContentInFlow` (textblock_iterator.cc:274-281) —
    /// `words[index]->content`.
    fn get_content_in_flow(&self, layout: &Layout, index: i32, content: &mut Content) {
        *content = layout
            .node_tb(self.widget, |tb| {
                usize::try_from(index)
                    .ok()
                    .and_then(|i| tb.words.get(i).map(|w| w.content.clone()))
            })
            .flatten()
            .unwrap_or(Content {
                r#type: content_type::INVALID,
                space: false,
                data: ContentData::None,
            });
    }

    /// Clone polymorphe (≙ `clone` de la sous-classe amont).
    fn clone_box(&self) -> Box<dyn OofInFlow> {
        Box::new(TbInFlow {
            widget: self.widget,
        })
    }

    /// `highlight` in-flow (textblock_iterator.cc:49-112) —
    /// étend `hlStart`/`hlEnd` vers `index`, puis
    /// `queueDrawRange (index1, index2)` si changé (verbatim).
    fn highlight_in_flow(
        &self,
        layout: &mut Layout,
        index: i32,
        start: i32,
        end: i32,
        layer: HighlightLayer,
    ) {
        let w = self.widget;
        let l = layer as usize;
        let (index1, mut index2) = (index, index);
        let changed = layout.with_tb(w, |lg, tb| {
            let _ = lg;
            let (old_start_index, old_start_char, old_end_index, old_end_char) = (
                tb.hl_start[l].index,
                tb.hl_start[l].n_char,
                tb.hl_end[l].index,
                tb.hl_end[l].n_char,
            );

            // Rien de surligné → démarrer sur `index` (verbatim).
            if tb.hl_start[l].index > tb.hl_end[l].index {
                tb.hl_start[l].index = index;
                tb.hl_end[l].index = index;
            }

            if tb.hl_start[l].index >= index {
                index2 = tb.hl_start[l].index;
                tb.hl_start[l].index = index;
                tb.hl_start[l].n_char = start;
            }
            if tb.hl_end[l].index <= index {
                index2 = tb.hl_end[l].index;
                tb.hl_end[l].index = index;
                tb.hl_end[l].n_char = end;
            }

            old_start_index != tb.hl_start[l].index
                || old_start_char != tb.hl_start[l].n_char
                || old_end_index != tb.hl_end[l].index
                || old_end_char != tb.hl_end[l].n_char
        });
        if changed.unwrap_or(false) {
            layout.tb_queue_draw_range(w, index1, index2);
        }
    }

    /// `unhighlight` in-flow (textblock_iterator.cc:116-166) —
    /// `direction == 0` : tout retirer (`1, 0`) ; `> 0` : avancer
    /// `hlStart` ; `< 0` : reculer `hlEnd` (`nChar = INT_MAX`
    /// verbatim).
    fn unhighlight_in_flow(
        &self,
        layout: &mut Layout,
        index: i32,
        direction: i32,
        layer: HighlightLayer,
    ) {
        let w = self.widget;
        let l = layer as usize;
        let (mut index1, mut index2) = (index, index);
        let changed = layout.with_tb(w, |lg, tb| {
            let _ = lg;
            // Rien de surligné → sortie (verbatim).
            if tb.hl_start[l].index > tb.hl_end[l].index {
                return false;
            }
            let (old_start_index, old_start_char, old_end_index, old_end_char) = (
                tb.hl_start[l].index,
                tb.hl_start[l].n_char,
                tb.hl_end[l].index,
                tb.hl_end[l].n_char,
            );

            if direction == 0 {
                index1 = tb.hl_start[l].index;
                index2 = tb.hl_end[l].index;
                tb.hl_start[l].index = 1;
                tb.hl_end[l].index = 0;
            } else if direction > 0 && tb.hl_start[l].index <= index {
                index1 = tb.hl_start[l].index;
                tb.hl_start[l].index = index + 1;
                tb.hl_start[l].n_char = 0;
            } else if direction < 0 && tb.hl_end[l].index >= index {
                index1 = tb.hl_end[l].index;
                tb.hl_end[l].index = index - 1;
                tb.hl_end[l].n_char = i32::MAX;
            }

            old_start_index != tb.hl_start[l].index
                || old_start_char != tb.hl_start[l].n_char
                || old_end_index != tb.hl_end[l].index
                || old_end_char != tb.hl_end[l].n_char
        });
        if changed.unwrap_or(false) {
            layout.tb_queue_draw_range(w, index1, index2);
        }
    }

    /// `getAllocation` in-flow (textblock_iterator.cc:172-267) —
    /// ligne existante ou valeurs calculées « comme `addLine` »
    /// pour une ligne pas encore créée (verbatim : `leftOffset`
    /// de la prochaine ligne, `lineBorderAscent` scanné sur les
    /// mots restants).
    fn get_allocation_in_flow(
        &self,
        layout: &Layout,
        index: i32,
        start: i32,
        end: i32,
        allocation: &mut Allocation,
    ) {
        let w = self.widget;
        let Some((word, first_word_of_line, text_offset, line_y_canvas, line_border_ascent)) =
            layout
                .node_tb(w, |tb| {
                    let word = usize::try_from(index)
                        .ok()
                        .and_then(|i| tb.words.get(i))
                        .cloned()?;

                    let line_index = layout.tb_find_line_of_word(tb, index);
                    if line_index == -1 {
                        // Ligne inexistante — valeurs calculées « comme
                        // `addLine` » (verbatim commentaire amont).
                        let prev = tb.lines.last();
                        let first_word = prev.map_or(0, |l| l.last_word + 1);

                        // `textOffset` = le futur `leftOffset` de la
                        // ligne (le vrai `textOffset` n'est calculable
                        // qu'une fois la ligne complète — verbatim).
                        let regard_border =
                            layout.tb_must_border_be_regarded(tb, tb.lines.len() as i32);
                        let text_offset = (if regard_border {
                            tb.new_line_left_border
                        } else {
                            0
                        })
                        .max(
                            layout.node(w, super::super::widget::WidgetNode::box_offset_x)
                                + tb.left_inner_padding
                                + if tb.lines.is_empty() {
                                    tb.line1_offset_eff
                                } else {
                                    0
                                },
                        );

                        let y = layout.tb_y_offset_of_line_to_be_created(w, tb, None);

                        let mut line_border_ascent = 0;
                        for i in first_word..tb.words.len() as i32 {
                            if let Some(wd) = tb.words.get(i as usize) {
                                let border_ascent =
                                    if wd.content.r#type == content_type::WIDGET_IN_FLOW {
                                        if let ContentData::Widget(child) = wd.content.data {
                                            wd.size.ascent
                                                - layout
                                                    .widget_style(child)
                                                    .map_or(0, |s| s.attrs.margin.top)
                                        } else {
                                            wd.size.ascent
                                        }
                                    } else {
                                        wd.size.ascent
                                    };
                                line_border_ascent = line_border_ascent.max(border_ascent);
                            }
                        }
                        Some((word, first_word, text_offset, y, line_border_ascent))
                    } else {
                        // Ligne existante — cas simple (verbatim).
                        let line = &tb.lines[line_index as usize];
                        let alloc = layout.node(w, |n| n.allocation);
                        Some((
                            word,
                            line.first_word,
                            line.text_offset,
                            tb.line_y_offset_canvas(line, &alloc),
                            line.border_ascent,
                        ))
                    }
                })
                .flatten()
        else {
            return;
        };

        allocation.x = layout.node(w, |n| n.allocation.x) + text_offset;
        for i in first_word_of_line..index {
            if let Some(wd) = layout
                .node_tb(w, |tb| {
                    tb.words
                        .get(i as usize)
                        .map(|wd| (wd.size.width, wd.eff_space))
                })
                .flatten()
            {
                allocation.x += wd.0 + i32::from(wd.1);
            }
        }
        if start > 0 && word.content.r#type == content_type::TEXT {
            if let ContentData::Text(t) = &word.content.data {
                allocation.x += layout.tb_text_width(
                    t,
                    0,
                    start as usize,
                    &word.style,
                    word.flags & WORD_START != 0,
                    word.flags & WORD_END != 0 && t.get(start as usize).is_none(),
                );
            }
        }
        allocation.y = line_y_canvas + line_border_ascent - word.size.ascent;

        allocation.width = word.size.width;
        if word.content.r#type == content_type::TEXT {
            if let ContentData::Text(t) = &word.content.data {
                let word_end = t.len() as i32;
                if start > 0 || end < word_end {
                    // `end` peut être `INT_MAX` (verbatim).
                    let end = end.min(word_end);
                    allocation.width = layout.tb_text_width(
                        t,
                        start as usize,
                        (end - start) as usize,
                        &word.style,
                        word.flags & WORD_START != 0 && start == 0,
                        word.flags & WORD_END != 0 && t.get(end as usize).is_none(),
                    );
                }
            }
        }
        allocation.ascent = word.size.ascent;
        allocation.descent = word.size.descent;
    }
}
