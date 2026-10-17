//! Port de `dw/bullet.cc` (+ `bullet.hh`) — `dw::Bullet` : puce
//! de liste, `core::Widget` direct (leaf, sans état propre).
//! `iterator` → `EmptyIterator` : verbatim — amont commente le
//! `TextIterator` comme `\bug Not implemented`.

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use crate::layout::Layout;
use crate::style::{ListStyleType, Shading};
use crate::types::{DrawingContext, Extremes, Rectangle, Requisition};
use crate::view::View;
use crate::widget::WidgetKind;
use crate::WidgetId;

impl Layout {
    /// `new Bullet ()` (bullet.cc:29-33) — `WidgetKind::Bullet`.
    pub fn bullet_new(&mut self) -> WidgetId {
        self.add_widget_node(WidgetKind::Bullet)
    }

    /// `xHeight` du style (≙ `getStyle()->font->xHeight` — `None`
    /// ≙ fonte/style absents, `0`).
    fn bullet_x_height(&self, w: WidgetId) -> i32 {
        self.node(w, |n| {
            n.style
                .as_ref()
                .and_then(|s| s.attrs.font.as_ref().map(|f| f.x_height))
                .unwrap_or(0)
        })
    }

    /// `Bullet::sizeRequestSimpl` (bullet.cc:39-44) — verbatim.
    pub(crate) fn bullet_size_request_simpl(&self, w: WidgetId, requisition: &mut Requisition) {
        let x_height = self.bullet_x_height(w);
        requisition.width = (x_height * 4 / 5).max(1);
        requisition.ascent = x_height.max(1);
        requisition.descent = 0;
    }

    /// `Bullet::getExtremesSimpl` (bullet.cc:46-52) — verbatim :
    /// les cinq largeurs = `max (xHeight * 4 / 5, 1)`.
    pub(crate) fn bullet_get_extremes_simpl(&self, w: WidgetId, extremes: &mut Extremes) {
        let width = (self.bullet_x_height(w) * 4 / 5).max(1);
        extremes.min_width = width;
        extremes.max_width = width;
        extremes.adjustment_width = width;
        extremes.min_width_intrinsic = width;
        extremes.max_width_intrinsic = width;
    }

    /// `Bullet::draw` (bullet.cc:57-86) — verbatim : carré,
    /// cercle creux ou disque selon `listStyleType`.
    pub(crate) fn bullet_draw(
        &self,
        w: WidgetId,
        view: &mut dyn View,
        _area: &Rectangle,
        _context: &mut DrawingContext,
    ) {
        let Some((alloc, x_height, color, list_style_type)) = self.node(w, |n| {
            let s = n.style.as_ref()?;
            Some((
                n.allocation,
                s.attrs.font.as_ref().map_or(0, |f| f.x_height),
                s.attrs.color?,
                s.attrs.list_style_type,
            ))
        }) else {
            // Style/couleur absents ≙ NULL amont — dégradé.
            return;
        };
        let l = alloc.width.min(alloc.ascent);
        let x = alloc.x;
        let y = alloc.y + alloc.ascent - x_height;
        match list_style_type {
            ListStyleType::Square => {
                view.draw_rectangle(&color, Shading::Normal, false, x, y, l, l);
            }
            ListStyleType::Circle => {
                view.draw_arc(
                    &color,
                    Shading::Normal,
                    false,
                    x + l / 2,
                    y + l / 2,
                    l,
                    l,
                    0,
                    360,
                );
            }
            // `LIST_STYLE_TYPE_DISC` + `default` (verbatim).
            _ => {
                view.draw_arc(
                    &color,
                    Shading::Normal,
                    true,
                    x + l / 2,
                    y + l / 2,
                    l,
                    l,
                    0,
                    360,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imgbuf::{Imgbuf, ImgbufType};
    use crate::layout::IdleCallback;
    use crate::platform::Platform;
    use crate::style::{Color, Font, FontAttrs, LineType, Style, StyleAttrs, Tooltip};
    use crate::types::{Content, Point};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// `Platform` minimale (≙ mock de `iterator::tests`).
    #[derive(Default)]
    struct MockPlatform {
        /// Ids idle alloués.
        next_id: i32,
    }

    /// Fonte de test `xHeight = 5`.
    fn test_font() -> Rc<Font> {
        Rc::new(Font {
            attrs: FontAttrs {
                name: Vec::new(),
                size: 10,
                weight: 400,
                letter_spacing: 0,
                font_variant: crate::style::FontVariant::Normal,
                style: crate::style::FontStyle::Normal,
            },
            ascent: 10,
            descent: 2,
            space_width: 4,
            x_height: 5,
            zero_width: 5,
        })
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
        fn create_font(&mut self, _attrs: &FontAttrs, _te: bool) -> Rc<Font> {
            test_font()
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
            unreachable!("imgbuf non utilisé par les tests bullet")
        }
        fn copy_selection(&mut self, _t: &[u8], _d: i32) {}
        fn get_resource_factory(&mut self) -> Option<Rc<dyn crate::ui::ResourceFactory>> {
            None
        }
    }

    /// `View` enregistreur pour `Bullet::draw`.
    #[derive(Default)]
    struct RecView {
        /// `(filled, cx, cy, w, h)` des `drawArc`.
        arcs: Vec<(bool, i32, i32, i32, i32)>,
        /// `(x, y, w, h)` des `drawRectangle`.
        rects: Vec<(i32, i32, i32, i32)>,
    }

    impl View for RecView {
        fn set_layout(&mut self, _l: Option<usize>) {}
        fn set_canvas_size(&mut self, _w: i32, _a: i32, _d: i32) {}
        fn set_cursor(&mut self, _c: crate::style::Cursor) {}
        fn set_bg_color(&mut self, _c: &Color) {}
        fn uses_viewport(&mut self) -> bool {
            true
        }
        fn get_h_scrollbar_thickness(&mut self) -> i32 {
            0
        }
        fn get_v_scrollbar_thickness(&mut self) -> i32 {
            0
        }
        fn get_scrollbar_on_left(&mut self) -> i32 {
            0
        }
        fn scroll_to(&mut self, _x: i32, _y: i32) {}
        fn set_viewport_size(&mut self, _w: i32, _h: i32, _ht: i32, _vt: i32) {}
        fn start_drawing(&mut self, _a: &Rectangle) {}
        fn finish_drawing(&mut self, _a: &Rectangle) {}
        fn queue_draw(&mut self, _a: &Rectangle) {}
        fn queue_draw_total(&mut self) {}
        fn cancel_queue_draw(&mut self) {}
        fn draw_point(&mut self, _c: &Color, _s: Shading, _x: i32, _y: i32) {}
        fn draw_line(&mut self, _c: &Color, _s: Shading, _x1: i32, _y1: i32, _x2: i32, _y2: i32) {}
        fn draw_typed_line(
            &mut self,
            _c: &Color,
            _s: Shading,
            _t: LineType,
            _w: i32,
            _x1: i32,
            _y1: i32,
            _x2: i32,
            _y2: i32,
        ) {
        }
        fn draw_rectangle(
            &mut self,
            _c: &Color,
            _s: Shading,
            _f: bool,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
        ) {
            self.rects.push((x, y, w, h));
        }
        fn draw_arc(
            &mut self,
            _c: &Color,
            _s: Shading,
            f: bool,
            cx: i32,
            cy: i32,
            w: i32,
            h: i32,
            _a1: i32,
            _a2: i32,
        ) {
            self.arcs.push((f, cx, cy, w, h));
        }
        fn draw_polygon(&mut self, _c: &Color, _s: Shading, _f: bool, _cv: bool, _p: &[Point]) {}
        fn draw_text(&mut self, _f: &Font, _c: &Color, _s: Shading, _x: i32, _y: i32, _t: &[u8]) {}
        fn draw_simple_wrapped_text(
            &mut self,
            _f: &Font,
            _c: &Color,
            _s: Shading,
            _x: i32,
            _y: i32,
            _w: i32,
            _h: i32,
            _t: &[u8],
        ) {
        }
        fn draw_image(
            &mut self,
            _b: &Rc<dyn Imgbuf>,
            _xr: i32,
            _yr: i32,
            _x: i32,
            _y: i32,
            _w: i32,
            _h: i32,
        ) {
        }
        fn get_clipping_view(
            &mut self,
            _x: i32,
            _y: i32,
            _w: i32,
            _h: i32,
        ) -> std::boxed::Box<dyn View> {
            std::boxed::Box::new(RecView::default())
        }
        fn merge_clipping_view(&mut self, _cv: std::boxed::Box<dyn View>) {}
    }

    /// Bullet stylé (fonte `x_height` = 5, couleur noire).
    fn make_bullet(l: &mut Layout, list_style_type: ListStyleType) -> WidgetId {
        let w = l.bullet_new();
        let attrs = StyleAttrs {
            font: Some(test_font()),
            color: Some(Color::new(0)),
            list_style_type,
            ..StyleAttrs::default()
        };
        l.widget_set_style(w, &Style::create(&attrs));
        w
    }

    #[test]
    fn bullet_size_request_and_extremes() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let w = make_bullet(&mut l, ListStyleType::Disc);
        // `xHeight = 5` → width = max(5*4/5,1) = 4, ascent = 5.
        let mut req = Requisition::default();
        l.w_size_request_simpl(w, &mut req);
        assert_eq!((req.width, req.ascent, req.descent), (4, 5, 0));
        let mut ext = Extremes::default();
        l.w_get_extremes_simpl(w, &mut ext);
        assert_eq!(ext.min_width, 4);
        assert_eq!(ext.max_width, 4);
        assert_eq!(ext.adjustment_width, 4);
        assert_eq!(ext.min_width_intrinsic, 4);
        assert_eq!(ext.max_width_intrinsic, 4);
    }

    #[test]
    fn bullet_draw_variants() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        for (lst, want_arc_filled) in [
            (ListStyleType::Disc, Some(true)),
            (ListStyleType::Circle, Some(false)),
            (ListStyleType::Square, None),
        ] {
            let w = make_bullet(&mut l, lst);
            l.node_mut(w, |n| {
                n.allocation = crate::types::Allocation {
                    x: 10,
                    y: 20,
                    width: 6,
                    ascent: 8,
                    descent: 0,
                };
            });
            let mut view = RecView::default();
            let area = Rectangle {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            };
            let mut ctx = DrawingContext::default();
            l.bullet_draw(w, &mut view, &area, &mut ctx);
            // `l = min(6, 8) = 6` ; `y = 20 + 8 - 5 = 23`.
            if let Some(filled) = want_arc_filled {
                assert_eq!(view.arcs, vec![(filled, 10 + 3, 23 + 3, 6, 6)]);
                assert_eq!(view.rects, [] as [(i32, i32, i32, i32); 0]);
            } else {
                assert_eq!(view.rects, vec![(10, 23, 6, 6)]);
                assert_eq!(view.arcs, [] as [(bool, i32, i32, i32, i32); 0]);
            }
        }
    }

    #[test]
    fn bullet_iterator_is_empty() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let w = make_bullet(&mut l, ListStyleType::Disc);
        // `Bullet::iterator` → `EmptyIterator` (verbatim `\bug
        // Not implemented` amont).
        let mut it = l.w_iterator(w, Content::mask_for_selection(true), false);
        assert!(!crate::iterator::DwIterator::next(it.as_mut(), &l));
    }
}
