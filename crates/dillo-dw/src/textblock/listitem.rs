//! `dw/alignedtextblock.cc` + `dw/listitem.cc` —
//! `AlignedTextblock`/`ListItem` : colonnes de listes alignées.
//!
//! `AlignedTextblock` est abstraite amont (`getValue`/`setMaxValue`
//! purs) — seul `ListItem` est instancié. Le port garde donc un seul
//! kind [`crate::widget::WidgetKind::Textblock`] et distingue les
//! items par `TextblockData::aligned: Some(…)` ; les virtuels
//! amont (`getValue`, `setMaxValue`, `usesMaxGeneratorWidth`)
//! deviennent les branches `aligned.is_some()` des méthodes ci-
//! dessous.
//!
//! `List` amont est un pointeur partagé à compte de références
//! manuel entre les textblocks du groupe — le port utilise
//! `Rc<RefCell<AlignedList>>` : `unref` `NULLé` le slot (verbatim) et
//! le dernier `Rc` libère la liste (≙ `delete this` à
//! `refCount == 0`).

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::layout::Layout;
use crate::style::{ListStylePosition, Style};
use crate::WidgetId;

use super::TextblockData;

/// `AlignedTextblock::List` (alignedtextblock.hh:17-36).
/// `refCount` amont ≙ `Rc` strong count — non porté : `unref`
/// `NULLé` le slot et le dernier `Rc` libère la liste.
#[derive(Debug, Default)]
pub struct AlignedList {
    /// `textblocks` — `None` ≙ slot `NULLé` par `unref`.
    pub textblocks: Vec<Option<WidgetId>>,
    /// `values`.
    pub values: Vec<i32>,
    /// `maxValue`.
    pub max_value: i32,
}

/// État `AlignedTextblock` — `list` + `listPos`
/// (alignedtextblock.hh:38-39).
pub struct AlignedData {
    /// `list` — partagée entre les textblocks du groupe.
    pub list: Rc<RefCell<AlignedList>>,
    /// `listPos`.
    pub list_pos: usize,
}

impl AlignedList {
    /// `List::add` (alignedtextblock.cc:45-52) — retourne
    /// `listPos`. `refCount++` ≙ le clone `Rc` détenu par
    /// l'appelant.
    fn add(&mut self, w: WidgetId) -> usize {
        self.textblocks.push(Some(w));
        self.values.push(0);
        self.textblocks.len() - 1
    }

    /// `List::unref` (alignedtextblock.cc:54-61) — `NULLé` le
    /// slot ; `delete this` à `refCount == 0` ≙ drop du dernier
    /// `Rc`.
    pub(crate) fn unref(&mut self, pos: usize) {
        debug_assert!(self
            .textblocks
            .get(pos)
            .is_some_and(std::option::Option::is_some));
        if let Some(slot) = self.textblocks.get_mut(pos) {
            *slot = None;
        }
    }
}

impl Layout {
    /// `new ListItem (ref, limitTextWidth)` (listitem.cc:27-34) —
    /// `AlignedTextblock(limitTextWidth)` puis
    /// `setRefTextblock(ref)`.
    pub fn listitem_new(&mut self, ref_item: Option<WidgetId>, limit_text_width: bool) -> WidgetId {
        let w = self.tb_new(limit_text_width, false);
        self.tb_set_ref_textblock(w, ref_item);
        w
    }

    /// `AlignedTextblock::setRefTextblock` (alignedtextblock.cc:68-77).
    /// `ref` sans état aligné ≙ `ref == NULL` amont (un vrai
    /// `AlignedTextblock` a toujours `aligned`).
    pub fn tb_set_ref_textblock(&mut self, w: WidgetId, ref_item: Option<WidgetId>) {
        let list = ref_item
            .and_then(|r| self.node_tb(r, |tb| tb.aligned.as_ref().map(|a| a.list.clone())))
            .flatten()
            .unwrap_or_else(|| Rc::new(RefCell::new(AlignedList::default())));
        let list_pos = list.borrow_mut().add(w);
        self.node_tb_mut(w, |tb| {
            tb.aligned = Some(AlignedData { list, list_pos });
        });
        self.tb_update_value(w);
    }

    /// `AlignedTextblock::updateValue` (alignedtextblock.cc:87-103)
    /// — verbatim : si la nouvelle valeur dépasse le max du groupe,
    /// le max est poussé sur tous les membres ; sinon l'ancien max
    /// est appliqué à ce textblock seul.
    pub(crate) fn tb_update_value(&mut self, w: WidgetId) {
        let Some((list, list_pos)) = self
            .node_tb(w, |tb| {
                tb.aligned.as_ref().map(|a| (a.list.clone(), a.list_pos))
            })
            .flatten()
        else {
            return;
        };
        // `getValue` virtuel amont : `AlignedTableCell::getValue`
        // quand `cell == Aligned`, sinon `ListItem::getValue`
        // (verbatim — même dispatch pour `setMaxValue` plus bas).
        let is_cell = self
            .node_tb(w, |tb| {
                matches!(tb.cell, Some(crate::table::TableCellKind::Aligned { .. }))
            })
            .unwrap_or(false);
        let value = if is_cell {
            self.cell_get_value(w)
        } else {
            self.node_tb(w, Self::listitem_get_value).unwrap_or(0)
        };
        let (max_value, targets) = {
            let mut l = list.borrow_mut();
            if l.values.len() <= list_pos {
                l.values.resize(list_pos + 1, 0);
            }
            l.values[list_pos] = value;
            if value > l.max_value {
                l.max_value = value;
                let targets: Vec<(WidgetId, i32)> = l
                    .textblocks
                    .iter()
                    .enumerate()
                    .filter_map(|(i, t)| t.map(|w2| (w2, l.values.get(i).copied().unwrap_or(0))))
                    .collect();
                (l.max_value, targets)
            } else {
                (l.max_value, vec![(w, value)])
            }
        };
        for (target, target_value) in targets {
            // `setMaxValue` virtuel amont — dispatch cell/item.
            let target_is_cell = self
                .node_tb(target, |tb| {
                    matches!(tb.cell, Some(crate::table::TableCellKind::Aligned { .. }))
                })
                .unwrap_or(false);
            if target_is_cell {
                self.cell_set_max_value(target, max_value, target_value);
            } else {
                self.listitem_set_max_value(target, max_value, target_value);
            }
        }
    }

    /// `ListItem::getValue` (listitem.cc:69-74) — verbatim :
    /// `words[0]->size.width + words[0]->origSpace`, `0` si vide.
    fn listitem_get_value(tb: &TextblockData) -> i32 {
        match tb.words.first() {
            Some(w0) => w0.size.width + i32::from(w0.orig_space),
            None => 0,
        }
    }

    /// `ListItem::setMaxValue` (listitem.cc:76-81) — verbatim :
    /// padding interne, offset de 1re ligne, redraw complet,
    /// `queueResize (0, true)`.
    fn listitem_set_max_value(&mut self, w: WidgetId, max_value: i32, value: i32) {
        self.node_tb_mut(w, |tb| {
            tb.left_inner_padding = max_value;
            tb.line1_offset = -value;
            tb.redraw_y = 0;
        });
        self.widget_queue_resize(w, 0, true);
    }

    /// `ListItem::initWithWidget` (listitem.cc:46-53) — verbatim.
    pub fn listitem_init_with_widget(&mut self, w: WidgetId, widget: WidgetId, style: &Rc<Style>) {
        self.node_tb_mut(w, |tb| tb.has_listitem_value = true);
        self.tb_add_widget(w, widget, style);
        self.tb_add_space(w, style);
        if style.attrs.list_style_position == ListStylePosition::Outside {
            self.tb_update_value(w);
        }
    }

    /// `ListItem::initWithText` (listitem.cc:55-62) — verbatim.
    pub fn listitem_init_with_text(&mut self, w: WidgetId, text: &[u8], style: &Rc<Style>) {
        self.node_tb_mut(w, |tb| tb.has_listitem_value = true);
        self.tb_add_text(w, text, text.len(), style);
        self.tb_add_space(w, style);
        if style.attrs.list_style_position == ListStylePosition::Outside {
            self.tb_update_value(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imgbuf::{Imgbuf, ImgbufType};
    use crate::layout::IdleCallback;
    use crate::platform::Platform;
    use crate::style::{Color, Font, FontAttrs, Style, StyleAttrs, Tooltip};
    use crate::view::View;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// `Platform` minimale (≙ mock de `iterator::tests`) —
    /// `text_width` = 1px/octet.
    #[derive(Default)]
    struct MockPlatform {
        /// Ids idle alloués.
        next_id: i32,
    }

    impl Platform for MockPlatform {
        fn set_layout(&mut self, _l: usize) {}
        fn attach_view(&mut self, _v: &mut dyn View) {}
        fn detach_view(&mut self, _v: &mut dyn View) {}
        fn text_width(&mut self, _f: &Font, t: &[u8]) -> i32 {
            t.len() as i32
        }
        fn text_to_upper(&mut self, t: &[u8]) -> Vec<u8> {
            t.to_vec()
        }
        fn text_to_lower(&mut self, t: &[u8]) -> Vec<u8> {
            t.to_vec()
        }
        fn next_glyph(&mut self, _t: &[u8], i: i32) -> i32 {
            i + 1
        }
        fn prev_glyph(&mut self, _t: &[u8], i: i32) -> i32 {
            i - 1
        }
        fn dpi_x(&mut self) -> f64 {
            96.0
        }
        fn dpi_y(&mut self) -> f64 {
            96.0
        }
        fn add_idle(&mut self, _l: usize, _cb: IdleCallback) -> i32 {
            self.next_id += 1;
            self.next_id
        }
        fn remove_idle(&mut self, _id: i32) {}
        fn take_pending_idles(&mut self, _l: usize) -> Vec<IdleCallback> {
            Vec::new()
        }
        fn create_font(&mut self, attrs: &FontAttrs, _te: bool) -> Rc<Font> {
            Rc::new(Font {
                attrs: attrs.clone(),
                ascent: 10,
                descent: 2,
                space_width: 4,
                x_height: 5,
                zero_width: 5,
            })
        }
        fn font_exists(&mut self, _n: &[u8]) -> bool {
            false
        }
        fn create_color(&mut self, c: i32) -> Color {
            Color::new(c)
        }
        fn create_tooltip(&mut self, t: &[u8]) -> Rc<Tooltip> {
            Tooltip::create(t, None)
        }
        fn cancel_tooltip(&mut self) {}
        fn create_imgbuf(&mut self, _t: ImgbufType, _w: i32, _h: i32, _g: f64) -> Rc<dyn Imgbuf> {
            unreachable!("imgbuf non utilisé par les tests listitem")
        }
        fn copy_selection(&mut self, _t: &[u8], _d: i32) {}
        fn get_resource_factory(&mut self) -> Option<Rc<dyn crate::ui::ResourceFactory>> {
            None
        }
    }

    fn make_layout() -> Layout {
        Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true)
    }

    /// `getValue` ≙ `words[0].width + origSpace` (listitem.cc:69).
    fn value_of(l: &Layout, w: WidgetId) -> i32 {
        l.node_tb(w, |tb| {
            tb.words
                .first()
                .map_or(0, |w0| w0.size.width + i32::from(w0.orig_space))
        })
        .unwrap_or(0)
    }

    #[test]
    fn listitem_lifecycle_and_value() {
        let mut l = make_layout();
        let a = l.listitem_new(None, false);
        assert!(l.node_tb(a, |tb| tb.aligned.is_some()).unwrap_or(false));
        // `ListItem::usesMaxGeneratorWidth` → true (listitem.cc:42).
        assert!(l
            .node_tb(a, |tb| l.tb_uses_max_generator_width(a, tb))
            .unwrap_or(false));

        let st = Style::create(&StyleAttrs::default());
        l.listitem_init_with_text(a, b"1.", &st);
        assert!(l.node_tb(a, |tb| tb.has_listitem_value).unwrap_or(false));

        // `updateValue` : la valeur devient le max → appliquée
        // (`leftInnerPadding` = max, `line1Offset` = -value).
        let va = value_of(&l, a);
        let (pad, off) = l
            .node_tb(a, |tb| (tb.left_inner_padding, tb.line1_offset))
            .unwrap_or((-1, 0));
        assert_eq!((pad, off), (va, -va));
    }

    #[test]
    fn listitem_shared_list_max_propagates() {
        let mut l = make_layout();
        let st = Style::create(&StyleAttrs::default());
        let a = l.listitem_new(None, false);
        l.listitem_init_with_text(a, b"1.", &st);
        let va = value_of(&l, a);

        // `ref` partage la `List` — le nouvel item hérite du max
        // existant à `updateValue` (alignedtextblock.cc:97-102).
        let b = l.listitem_new(Some(a), false);
        let (pad_b, off_b) = l
            .node_tb(b, |tb| (tb.left_inner_padding, tb.line1_offset))
            .unwrap_or((-1, 0));
        assert_eq!((pad_b, off_b), (va, 0));

        // Une valeur plus grande pousse le max sur tous les
        // membres (alignedtextblock.cc:94-100).
        l.listitem_init_with_text(b, b"123.", &st);
        let vb = value_of(&l, b);
        assert!(vb > va);
        let (pad_a2, off_a2) = l
            .node_tb(a, |tb| (tb.left_inner_padding, tb.line1_offset))
            .unwrap_or((-1, 0));
        let (pad_b2, off_b2) = l
            .node_tb(b, |tb| (tb.left_inner_padding, tb.line1_offset))
            .unwrap_or((-1, 0));
        assert_eq!((pad_a2, off_a2), (vb, -va));
        assert_eq!((pad_b2, off_b2), (vb, -vb));
    }

    #[test]
    fn listitem_unref_on_delete() {
        let mut l = make_layout();
        let st = Style::create(&StyleAttrs::default());
        let a = l.listitem_new(None, false);
        l.listitem_init_with_text(a, b"1.", &st);
        let b = l.listitem_new(Some(a), false);
        l.listitem_init_with_text(b, b"2.", &st);

        let list = l
            .node_tb(a, |tb| tb.aligned.as_ref().map(|d| d.list.clone()))
            .flatten()
            .unwrap();
        // `~AlignedTextblock` → `list->unref (listPos)` : le slot
        // est NULLé, l'autre membre reste (alignedtextblock.cc:54).
        l.widget_delete(a);
        {
            let borrowed = list.borrow();
            assert!(borrowed.textblocks[0].is_none());
            assert_eq!(borrowed.textblocks[1], Some(b));
        }
    }
}
