//! Port de `dw/ruler.cc` (+ `ruler.hh`) — `dw::Ruler` : `<hr>`,
//! `Ruler : RegardingBorder` amont ≙ `OofAwareData` embarqué dans
//! [`WidgetKind::Ruler`].
//!
//! `Ruler::draw` et `Ruler::getWidgetAtPoint` remplacent les
//! implémentations `OOFAwareWidget` amont (pas de dessin des
//! enfants OOF, hit-test = allocation seule — verbatim).

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use crate::layout::Layout;
use crate::oof::OofAwareData;
use crate::types::{DrawingContext, Extremes, Rectangle, Requisition};
use crate::view::View;
use crate::widget::WidgetKind;
use crate::WidgetId;

impl Layout {
    /// `new Ruler ()` (ruler.cc:31-35) — `WidgetKind::Ruler` avec
    /// `OofAwareData` (héritage `RegardingBorder` amont).
    pub fn ruler_new(&mut self) -> WidgetId {
        self.add_widget_node(WidgetKind::Ruler {
            oof: OofAwareData::new(),
        })
    }

    /// `Ruler::sizeRequestSimpl` (ruler.cc:43-50) — verbatim : la
    /// bordure 1px est soustraite de la largeur dispo pour le
    /// calcul du contenu.
    pub(crate) fn ruler_size_request_simpl(&mut self, w: WidgetId, requisition: &mut Requisition) {
        let avail = self.widget_get_avail_width(w, true);
        let (diff_width, off_y, rest_height) = self.node(w, |n| {
            (n.box_diff_width(), n.box_offset_y(), n.box_rest_height())
        });
        requisition.width = (avail - diff_width).max(0);
        requisition.ascent = off_y;
        requisition.descent = rest_height;
    }

    /// `Ruler::getExtremesSimpl` (ruler.cc:52-60) — verbatim.
    pub(crate) fn ruler_get_extremes_simpl(&mut self, w: WidgetId, extremes: &mut Extremes) {
        let diff = self.node(w, super::widget::WidgetNode::box_diff_width);
        extremes.min_width = diff;
        extremes.max_width = diff;
        extremes.min_width_intrinsic = extremes.min_width;
        extremes.max_width_intrinsic = extremes.max_width;
        self.widget_correct_extremes(w, extremes, false);
        extremes.adjustment_width = extremes.min_width_intrinsic.min(extremes.min_width);
    }

    /// `Ruler::draw` (ruler.cc:87-91) — `drawWidgetBox` seul :
    /// remplace `OOFAwareWidget::draw` amont (verbatim).
    pub(crate) fn ruler_draw(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        area: &Rectangle,
        _context: &mut DrawingContext,
    ) {
        self.widget_draw_widget_box(w, view, area, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imgbuf::{Imgbuf, ImgbufType};
    use crate::layout::IdleCallback;
    use crate::platform::Platform;
    use crate::style::{Color, Font, FontAttrs, Style, StyleAttrs, Tooltip};
    use crate::types::{Allocation, Content};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// `Platform` minimale (≙ mock de `iterator::tests`).
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
        fn add_idle(&mut self, _cb: IdleCallback) -> i32 {
            self.next_id += 1;
            self.next_id
        }
        fn remove_idle(&mut self, _id: i32) {}
        fn take_pending_idles(&mut self) -> Vec<IdleCallback> {
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
            unreachable!("imgbuf non utilisé par les tests ruler")
        }
        fn copy_selection(&mut self, _t: &[u8], _d: i32) {}
        fn get_resource_factory(&mut self) -> Option<Rc<dyn crate::ui::ResourceFactory>> {
            None
        }
    }

    fn make_ruler(l: &mut Layout) -> WidgetId {
        let w = l.ruler_new();
        l.widget_set_style(w, &Style::create(&StyleAttrs::default()));
        w
    }

    #[test]
    fn ruler_block_level_uses_avail_width() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let w = make_ruler(&mut l);
        assert!(l.w_is_block_level(w));
        assert!(l.w_uses_avail_width(w));
        // `instanceOf (RegardingBorder)` ≙ `is_regarding_border_kind`.
        assert!(l.node(w, |n| crate::oof::is_regarding_border_kind(&n.kind)));
    }

    #[test]
    fn ruler_size_request_uses_avail_width() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let w = make_ruler(&mut l);
        // Top-level ≙ viewport : `getAvailWidth(true)` = largeur
        // viewport par défaut du Layout — assert relationnel.
        let mut req = Requisition::default();
        l.ruler_size_request_simpl(w, &mut req);
        let avail = l.widget_get_avail_width(w, true);
        let diff = l.node(w, super::super::widget::WidgetNode::box_diff_width);
        assert_eq!(req.width, (avail - diff).max(0));
        assert_eq!(
            req.ascent,
            l.node(w, super::super::widget::WidgetNode::box_offset_y)
        );
        assert_eq!(
            req.descent,
            l.node(w, super::super::widget::WidgetNode::box_rest_height)
        );
    }

    #[test]
    fn ruler_get_widget_at_point_allocation_only() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let w = make_ruler(&mut l);
        l.node_mut(w, |n| {
            n.allocation = Allocation {
                x: 0,
                y: 0,
                width: 50,
                ascent: 3,
                descent: 0,
            };
            n.set_flags(crate::widget::WAS_ALLOCATED);
        });
        let mut ctx = crate::types::GettingWidgetAtPointContext::default();
        // Dedans → `this` ; dehors → `NULL` (ruler.cc:93-102,
        // remplace la version OOFAwareWidget).
        assert_eq!(l.w_get_widget_at_point(w, 10, 1, &mut ctx), Some(w));
        assert_eq!(l.w_get_widget_at_point(w, 60, 1, &mut ctx), None);
        assert_eq!(l.w_get_widget_at_point(w, 10, 9, &mut ctx), None);
    }

    #[test]
    fn ruler_iterator_is_empty() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let w = make_ruler(&mut l);
        let mut it = l.w_iterator(w, Content::mask_for_selection(true), false);
        assert!(!crate::iterator::DwIterator::next(it.as_mut(), &l));
    }
}
