//! `app` — boucle `winit` + dispatch des événements (≙
//! `FltkViewport::handle` + `Fl::run`).
//!
//! Traduction verbatim `fltkviewbase.cc`/`fltkviewport.cc` :
//!
//! - `getDwButtonState` → `DwApp::dw_state` (`FL_SHIFT`≙
//!   `SHIFT_MASK`, `FL_CTRL`≙`CONTROL_MASK`, `FL_ALT`≙`META_MASK`,
//!   boutons 1-3) — winit `ModifiersChanged` + suivi des boutons.
//! - `FL_PUSH`/`FL_RELEASE` → `Layout::button_event`
//!   (`Fl::event_clicks()+1` ≙ double-clic < 500 ms même bouton).
//! - `FL_MOVE`/`FL_DRAG` → `motion_notify` ; `FL_ENTER`/`LEAVE` →
//!   `enter_notify`/`leave_notify`.
//! - `FL_MOUSEWHEEL` → `scroll_to` (delta ≙ `linesize` ≙
//!   `scroll_step` — verbatim `Fl_Scrollbar::handle`).
//! - `FL_KEYBOARD` → le parent (verbatim : `return 0` → raccourci
//!   shell — seam [`DwApp::set_key_handler`], Dillo route ses
//!   keybindings dans `ui.cc` — déviation documentée A10).
//! - Damage → `take_damage` → `Layout::draw` par région →
//!   `present` (≙ `draw_area` + `fl_push_clip`).
//! - Idle : `Platform::take_pending_idles` ≙ `Fl::add_idle`
//!   (drainé dans `about_to_wait`).
//!
//! Scrollbars : amont ce sont de vrais `Fl_Scrollbar` enfants ;
//! ici `DwApp` les dessine dans le pixmap et hit-teste les clics
//! (≙ `vscrollbar->handle(event)` délégué avant le contenu) —
//! épaisseur `SCROLLBAR_THICKNESS` = 15, `scrollbarOnLeft`
//! verbatim.
//!
//! `attach_view` : amont le layout tient le widget par pointeur
//! non-possédant → ici [`SharedView`] (`view.rs`), proxy `Weak`
//! sur le `Canvas` possédé par la `SkiaView` de `DwApp` (le
//! canvas plutôt que la vue : le layout rappelle `view.*` pendant
//! `Layout::draw`, qui tient déjà `RefMut<SkiaView>` — un proxy
//! sur la vue paniquerait, `RefCell` distinct).

// Casts verbatim amont : FLTK manipule des int coords/px —
// conversions i32↔f32 identiques à `fltkview*` (doc A10).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::type_complexity)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use dillo_dw::events::{button_state, ButtonEventType};
use dillo_dw::layout::Layout;
use dillo_dw::types::{Rectangle, ScrollCommand};
use dillo_dw::view::View;
use tiny_skia::{FillRule, Paint, PathBuilder, Rect, Transform};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{WindowAttributes, WindowId};

use crate::platform::SkiaPlatform;
use crate::surface::{PlatformSurface, SurfaceError};
use crate::view::{SharedView, SkiaView, SCROLLBAR_THICKNESS};

/// Erreurs applicatives (surface/fenêtre winit).
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Erreur OS `create_window`.
    #[error("winit: {0}")]
    Winit(#[from] winit::error::OsError),
    /// `EventLoop::new` / `run_app`.
    #[error("winit event loop: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),
    /// Surface softbuffer.
    #[error(transparent)]
    Surface(#[from] SurfaceError),
}

/// Hit-test scrollbar (≙ `vscrollbar->handle` ciblé amont).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrollbarHit {
    /// Track verticale (page au clic).
    VTrack,
    /// Pouce vertical (drag).
    VThumb,
    /// Track horizontale.
    HTrack,
    /// Pouce horizontal.
    HThumb,
    /// Coin (gadget) — ignoré amont.
    Corner,
}

/// `DwApp` — `ApplicationHandler` winit : fenêtre + surface +
/// vue + layout + dispatch événements (≙ `FltkViewport` + boucle).
pub struct DwApp {
    /// Plateforme (fontes/idles/clipboard ≙ `FltkPlatform`).
    platform: Rc<RefCell<SkiaPlatform>>,
    /// Vue racine (≙ widget `FltkViewport`).
    view: Rc<RefCell<SkiaView>>,
    /// Layout attaché (≙ `theLayout`).
    layout: Option<Rc<RefCell<Layout>>>,
    /// Surface de présentation (créée à `resumed`).
    surface: Option<PlatformSurface>,
    /// Titre de fenêtre (≙ `Fl_Window` label).
    title: String,
    /// Modifiers courants (masque `button_state::*`).
    mods: i32,
    /// Boutons enfoncés (masque `BUTTON*_MASK`).
    buttons_down: i32,
    /// ≙ `Fl::event_clicks` — compteur de clic courant.
    clicks: i32,
    /// `(instant, bouton)` du dernier press (double-clic 500 ms).
    last_click: Option<(Instant, i32)>,
    /// Drag scrollbar en cours.
    scroll_drag: Option<ScrollbarHit>,
    /// Offset du grab dans le pouce.
    scroll_drag_off: i32,
    /// Handler clavier du shell (≙ `FL_KEYBOARD` remonté au
    /// parent — verbatim `return 0` → raccourci).
    key_handler: RefCell<Option<Rc<RefCell<dyn FnMut(&winit::event::KeyEvent)>>>>,
    /// `CloseRequested` signalé (le shell décide).
    close_requested: bool,
}

/// Triplet `(vtrack, vthumb, htrack, hthumb)` — rectangles
/// scrollbar en coords *view*.
type ScrollbarRects = (Option<Rect>, Option<Rect>, Option<Rect>, Option<Rect>);

impl DwApp {
    /// Ctor — plateforme + vue (la fenêtre est créée à `resumed`,
    /// ≙ `Fl_Window::show` différé par winit).
    #[must_use]
    pub fn new(title: &str) -> Self {
        let platform = Rc::new(RefCell::new(SkiaPlatform::new()));
        let view = Rc::new(RefCell::new(SkiaView::new(&platform, 1, 1)));
        Self {
            platform,
            view,
            layout: None,
            surface: None,
            title: title.to_string(),
            mods: 0,
            buttons_down: 0,
            clicks: 0,
            last_click: None,
            scroll_drag: None,
            scroll_drag_off: 0,
            key_handler: RefCell::new(None),
            close_requested: false,
        }
    }

    /// Attache un `Layout` (≙ `layout->attachView(view)` +
    /// `platform->attachView`) — `SharedView` non-possédant.
    pub fn attach_layout(&mut self, layout: Rc<RefCell<Layout>>) {
        layout
            .borrow_mut()
            .attach_view(Box::new(SharedView::new(&self.view)));
        self.layout = Some(layout);
    }

    /// Détache le layout (`detach_view` verbatim).
    pub fn detach_layout(&mut self) {
        if let Some(l) = &self.layout {
            l.borrow_mut().detach_view();
        }
        self.layout = None;
    }

    /// `key_handler` — le shell reçoit les événements clavier
    /// (verbatim : `FltkViewport` remonte `FL_KEYBOARD` au parent).
    pub fn set_key_handler(&self, h: Rc<RefCell<dyn FnMut(&winit::event::KeyEvent)>>) {
        *self.key_handler.borrow_mut() = Some(h);
    }

    /// `close_requested` vu par le shell.
    #[must_use]
    pub fn close_requested(&self) -> bool {
        self.close_requested
    }

    /// `getDwButtonState` verbatim (fltkviewbase.cc:201).
    fn dw_state(&self) -> i32 {
        self.mods | self.buttons_down
    }

    /// `Fl::event_button` — `Left=1, Middle=2, Right=3` verbatim.
    fn dw_button(b: MouseButton) -> i32 {
        match b {
            MouseButton::Left => 1,
            MouseButton::Middle => 2,
            MouseButton::Right => 3,
            MouseButton::Back => 4,
            MouseButton::Forward => 5,
            MouseButton::Other(x) => i32::from(x),
        }
    }

    /// Masque `BUTTON*_MASK` d'un bouton (`FL_BUTTON1..3`).
    fn button_mask(b: i32) -> i32 {
        match b {
            1 => button_state::BUTTON1_MASK,
            2 => button_state::BUTTON2_MASK,
            3 => button_state::BUTTON3_MASK,
            _ => 0,
        }
    }

    /// ≙ `Fl::event_clicks()+1` — double-clic si même bouton en
    /// < 500 ms (constante FLTK).
    fn next_clicks(&mut self, button: i32) -> i32 {
        let now = Instant::now();
        self.clicks = match self.last_click {
            Some((t, b)) if b == button && now.duration_since(t) < Duration::from_millis(500) => {
                self.clicks + 1
            }
            _ => 1,
        };
        self.last_click = Some((now, button));
        self.clicks
    }

    /// Rectangles scrollbar en coords *view* (≙ `adjustScrollbars
    /// AndGadgetsAllocation` + thumb proportionnel).
    fn scrollbar_rects(&self) -> ScrollbarRects {
        let (vw, vh, hsb, vsb, on_left, sx, sy, cw, ch) = {
            let mut v = self.view.borrow_mut();
            let (a, b) = v.view_size();
            let (hs, vs) = v.scrollbars_visible();
            let (px, py) = v.scroll_pos();
            let (w2, h2) = v.canvas_size();
            (a, b, hs, vs, v.get_scrollbar_on_left() != 0, px, py, w2, h2)
        };
        let t = SCROLLBAR_THICKNESS as f32;
        let vtrack = vsb
            .then(|| {
                Rect::from_xywh(
                    if on_left { 0.0 } else { vw as f32 - t },
                    0.0,
                    t,
                    (vh as f32 - t).max(0.0),
                )
            })
            .flatten();
        let vthumb = vtrack.and_then(|r| {
            if ch <= 0 {
                return None;
            }
            let thumb_h = (r.height() * vh as f32 / ch as f32).clamp(12.0, r.height());
            let range = (ch - vh).max(1) as f32;
            let ty = r.y() + (r.height() - thumb_h) * sy as f32 / range;
            Rect::from_xywh(r.x(), ty, t, thumb_h)
        });
        let htrack = hsb
            .then(|| {
                Rect::from_xywh(
                    if on_left { t } else { 0.0 },
                    vh as f32 - t,
                    (vw as f32 - t).max(0.0),
                    t,
                )
            })
            .flatten();
        let hthumb = htrack.and_then(|r| {
            if cw <= 0 {
                return None;
            }
            let thumb_w = (r.width() * vw as f32 / cw as f32).clamp(12.0, r.width());
            let range = (cw - vw).max(1) as f32;
            let tx = r.x() + (r.width() - thumb_w) * sx as f32 / range;
            Rect::from_xywh(tx, r.y(), thumb_w, t)
        });
        (vtrack, vthumb, htrack, hthumb)
    }

    /// Hit-test `(x, y)` vue → scrollbar (≙ ordre `vscrollbar->
    /// handle` avant le contenu).
    fn hit_scrollbar(&self, x: f32, y: f32) -> Option<ScrollbarHit> {
        let (vt, vth, ht, hth) = self.scrollbar_rects();
        let inside =
            |r: Rect| x >= r.x() && y >= r.y() && x < r.x() + r.width() && y < r.y() + r.height();
        if vth.is_some_and(inside) {
            Some(ScrollbarHit::VThumb)
        } else if hth.is_some_and(inside) {
            Some(ScrollbarHit::HThumb)
        } else if vt.is_some_and(inside) {
            if ht.is_some_and(inside) {
                Some(ScrollbarHit::Corner)
            } else {
                Some(ScrollbarHit::VTrack)
            }
        } else if ht.is_some_and(inside) {
            Some(ScrollbarHit::HTrack)
        } else {
            None
        }
    }

    /// Dessine les scrollbars dans le back-buffer (≙ draw des
    /// `Fl_Scrollbar` enfants — approximation documentée :
    /// track + pouce, sans flèches).
    fn draw_scrollbars(&self) {
        let (vt, vth, ht, hth) = self.scrollbar_rects();
        let mut v = self.view.borrow_mut();
        let Some(mut px) = v.pixmap_mut() else {
            return;
        };
        let track = Paint {
            shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::from_rgba8(
                0xe0, 0xe0, 0xe0, 255,
            )),
            ..Paint::default()
        };
        let thumb = Paint {
            shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::from_rgba8(
                0xa0, 0xa0, 0xa0, 255,
            )),
            ..Paint::default()
        };
        for (r, p) in [(vt, &track), (vth, &thumb), (ht, &track), (hth, &thumb)] {
            let Some(r) = r else { continue };
            let mut pb = PathBuilder::new();
            pb.push_rect(r);
            if let Some(path) = pb.finish() {
                px.fill_path(&path, p, FillRule::Winding, Transform::default(), None);
            }
        }
    }

    /// Drag d'un pouce — `scroll_to` proportionnel (≙
    /// `Fl_Scrollbar::handle(FL_DRAG)`).
    fn scrollbar_drag(&mut self, x: f32, y: f32) {
        match self.scroll_drag {
            Some(ScrollbarHit::VThumb) => {
                let (vtrack, vthumb, ..) = self.scrollbar_rects();
                let (Some(track), Some(thumb)) = (vtrack, vthumb) else {
                    return;
                };
                let range = track.height() - thumb.height();
                let frac = (y - track.y() - self.scroll_drag_off as f32) / range.max(1.0);
                let (sx, ch, vh) = {
                    let v = self.view.borrow();
                    let (_, vh) = v.view_size();
                    let (_, ch) = v.canvas_size();
                    (v.scroll_pos().0, ch, vh)
                };
                self.view
                    .borrow_mut()
                    .scroll_to(sx, (frac * (ch - vh).max(0) as f32) as i32);
                self.request_redraw();
            }
            Some(ScrollbarHit::HThumb) => {
                let (.., htrack, hthumb) = self.scrollbar_rects();
                let (Some(track), Some(thumb)) = (htrack, hthumb) else {
                    return;
                };
                let range = track.width() - thumb.width();
                let frac = (x - track.x() - self.scroll_drag_off as f32) / range.max(1.0);
                let (sy, cw, vw) = {
                    let v = self.view.borrow();
                    let (vw, _) = v.view_size();
                    let (cw, _) = v.canvas_size();
                    (v.scroll_pos().1, cw, vw)
                };
                self.view
                    .borrow_mut()
                    .scroll_to((frac * (cw - vw).max(0) as f32) as i32, sy);
                self.request_redraw();
            }
            _ => {}
        }
    }

    /// ≙ `FL_PUSH`/`FL_RELEASE` — scrollbar d'abord (verbatim
    /// ordre de `handle`), puis `Layout::button_event`.
    fn mouse(&mut self, press: bool, x: f32, y: f32, button: i32) {
        let state = self.dw_state();
        let (cx, cy) = {
            let v = self.view.borrow();
            (v.to_canvas_x(x as i32), v.to_canvas_y(y as i32))
        };
        if press {
            match self.hit_scrollbar(x, y) {
                Some(hit @ (ScrollbarHit::VThumb | ScrollbarHit::HThumb)) => {
                    let (_, vth, _, hth) = self.scrollbar_rects();
                    self.scroll_drag_off = match hit {
                        ScrollbarHit::VThumb => vth.map_or(0, |r| (y - r.y()) as i32),
                        _ => hth.map_or(0, |r| (x - r.x()) as i32),
                    };
                    self.scroll_drag = Some(hit);
                    return;
                }
                Some(ScrollbarHit::VTrack) => {
                    let (_, vth, ..) = self.scrollbar_rects();
                    let cmd = if vth.is_some_and(|r| y < r.y()) {
                        ScrollCommand::ScreenUp
                    } else {
                        ScrollCommand::ScreenDown
                    };
                    self.view.borrow_mut().scroll(cmd);
                    self.request_redraw();
                    return;
                }
                Some(ScrollbarHit::HTrack) => {
                    let (.., hth) = self.scrollbar_rects();
                    let cmd = if hth.is_some_and(|r| x < r.x()) {
                        ScrollCommand::ScreenLeft
                    } else {
                        ScrollCommand::ScreenRight
                    };
                    self.view.borrow_mut().scroll(cmd);
                    self.request_redraw();
                    return;
                }
                Some(ScrollbarHit::Corner) => return,
                None => {}
            }
            let n = self.next_clicks(button);
            let mask = Self::button_mask(button);
            self.buttons_down |= mask;
            if let Some(l) = &self.layout {
                l.borrow_mut().button_event(
                    ButtonEventType::ButtonPress,
                    n,
                    cx,
                    cy,
                    state | mask,
                    button,
                );
            }
        } else {
            if self.scroll_drag.is_some() {
                self.scroll_drag = None;
                return;
            }
            self.buttons_down &= !Self::button_mask(button);
            if let Some(l) = &self.layout {
                l.borrow_mut().button_event(
                    ButtonEventType::ButtonRelease,
                    self.clicks,
                    cx,
                    cy,
                    state,
                    button,
                );
            }
        }
        self.request_redraw();
    }

    /// `redraw` — consomme le damage et présente (≙
    /// `Fl_Widget::draw` + `draw_area` + `fl_push_clip`).
    fn redraw(&mut self) {
        let (full, mut rects) = self.view.borrow_mut().take_damage();
        if full {
            let (w, h) = self.view.borrow().view_size();
            rects = vec![Rectangle {
                x: 0,
                y: 0,
                width: w,
                height: h,
            }];
        }
        if rects.is_empty() {
            return;
        }
        {
            let mut v = self.view.borrow_mut();
            for r in &rects {
                v.fill_bg(r);
            }
            if let Some(l) = &self.layout {
                let mut lg = l.borrow_mut();
                for r in &rects {
                    lg.draw(&mut *v, r);
                }
            }
        }
        self.draw_scrollbars();
        if let Some(s) = &mut self.surface {
            let v = self.view.borrow();
            let px = v.pixmap();
            if let Some(px) = px.as_deref() {
                if let Err(e) = s.present(px) {
                    tracing::warn!("present: {e}");
                }
            }
        }
    }

    /// `request_redraw` — ≙ `Fl_Widget::redraw`.
    fn request_redraw(&self) {
        if let Some(s) = &self.surface {
            s.window().request_redraw();
        }
    }

    /// `run` — `EventLoop::new` + `run_app` (≙ `Fl::run`).
    ///
    /// # Errors
    /// `AppError` sur échec event loop/fenêtre/surface.
    pub fn run(mut self) -> Result<(), AppError> {
        let el = EventLoop::new()?;
        el.run_app(&mut self)?;
        Ok(())
    }

    /// Plateforme (pour le shell — fontes, resource factory).
    #[must_use]
    pub fn platform(&self) -> Rc<RefCell<SkiaPlatform>> {
        Rc::clone(&self.platform)
    }

    /// Vue racine.
    #[must_use]
    pub fn view(&self) -> Rc<RefCell<SkiaView>> {
        Rc::clone(&self.view)
    }
}

impl ApplicationHandler for DwApp {
    /// `resumed` — création fenêtre + surface (≙ `Fl_Window::show`).
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.surface.is_some() {
            return;
        }
        let win = match el.create_window(WindowAttributes::default().with_title(&self.title)) {
            Ok(w) => std::sync::Arc::new(w),
            Err(e) => {
                tracing::warn!("create_window: {e}");
                el.exit();
                return;
            }
        };
        match PlatformSurface::new(win) {
            Ok(s) => self.surface = Some(s),
            Err(e) => {
                tracing::warn!("surface: {e}");
                el.exit();
            }
        }
        self.request_redraw();
    }

    /// `window_event` — dispatch ≙ `FltkViewport::handle`.
    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.close_requested = true;
                el.exit();
            }
            WindowEvent::Resized(sz) => {
                self.view.borrow_mut().resize(sz.width, sz.height);
                if let Some(l) = &self.layout {
                    l.borrow_mut()
                        .viewport_size_changed(sz.width as i32, sz.height as i32);
                }
                self.view.borrow_mut().queue_draw_total();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::ModifiersChanged(m) => {
                // `getDwButtonState` verbatim (FL_SHIFT/CTRL/ALT).
                let st = m.state();
                self.mods = i32::from(st.shift_key()) * button_state::SHIFT_MASK
                    + i32::from(st.control_key()) * button_state::CONTROL_MASK
                    + i32::from(st.alt_key()) * button_state::META_MASK;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as i32, position.y as i32);
                self.view.borrow_mut().set_mouse_pos(x, y);
                if self.scroll_drag.is_some() {
                    self.scrollbar_drag(position.x as f32, position.y as f32);
                } else {
                    let (cx, cy) = {
                        let v = self.view.borrow();
                        (v.to_canvas_x(x), v.to_canvas_y(y))
                    };
                    if let Some(l) = &self.layout {
                        l.borrow_mut().motion_notify(cx, cy, self.dw_state());
                    }
                }
            }
            WindowEvent::CursorEntered { .. } => {
                let (cx, cy, st) = {
                    let v = self.view.borrow();
                    let (x, y) = v.mouse_pos();
                    (v.to_canvas_x(x), v.to_canvas_y(y), self.dw_state())
                };
                if let Some(l) = &self.layout {
                    l.borrow_mut().enter_notify(cx, cy, st);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.view.borrow_mut().set_mouse_pos(-1, -1);
                if let Some(l) = &self.layout {
                    l.borrow_mut().leave_notify(self.dw_state());
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = self.view.borrow().mouse_pos();
                self.mouse(
                    state == ElementState::Pressed,
                    x as f32,
                    y as f32,
                    Self::dw_button(button),
                );
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // ≙ `Fl_Scrollbar::handle(FL_MOUSEWHEEL)` —
                // `value - e_dy * linesize`.
                let step = self.view.borrow().scroll_step();
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * step, y * step),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                let (sx, sy) = self.view.borrow().scroll_pos();
                self.view
                    .borrow_mut()
                    .scroll_to(sx - dx as i32, sy - dy as i32);
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // ≙ `FL_KEYBOARD` → `return 0` (remonte au parent) ;
                // seam : callback shell (Dillo route dans ui.cc).
                let h = self.key_handler.borrow().clone();
                if let Some(h) = h {
                    h.borrow_mut()(&event);
                }
            }
            _ => {}
        }
    }

    /// `about_to_wait` — drain des idle callbacks (≙
    /// `Fl::add_idle` quand la file d'events est vide) + damage
    /// en attente.
    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        // ≙ `Fl::add_idle` — le drain appelle les callbacks avec
        // `&mut Layout` (verbatim `Layout::processIdles`).
        if let Some(l) = &self.layout {
            l.borrow_mut().drain_idles();
        }
        if self.view.borrow().has_damage() {
            self.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Fl::event_button` mapping verbatim (1/2/3).
    #[test]
    fn dw_button_maps_fltk_order() {
        assert_eq!(DwApp::dw_button(MouseButton::Left), 1);
        assert_eq!(DwApp::dw_button(MouseButton::Middle), 2);
        assert_eq!(DwApp::dw_button(MouseButton::Right), 3);
    }

    /// `FL_BUTTON1..3` → `BUTTON*_MASK` (fltkviewbase.cc:201).
    #[test]
    fn button_mask_matches_fltk() {
        assert_eq!(DwApp::button_mask(1), button_state::BUTTON1_MASK);
        assert_eq!(DwApp::button_mask(2), button_state::BUTTON2_MASK);
        assert_eq!(DwApp::button_mask(3), button_state::BUTTON3_MASK);
        assert_eq!(DwApp::button_mask(4), 0);
    }

    /// ≙ `Fl::event_clicks` — double-clic < 500 ms même bouton.
    #[test]
    fn clicks_count_double_click() {
        let mut app = DwApp::new("t");
        assert_eq!(app.next_clicks(1), 1);
        assert_eq!(app.next_clicks(1), 2, "même bouton <500ms → +1");
        assert_eq!(app.next_clicks(2), 1, "autre bouton → reset");
    }

    /// `DwApp::new` headless — pas de fenêtre avant `resumed`.
    #[test]
    fn app_without_window() {
        let app = DwApp::new("t");
        assert_eq!(app.dw_state(), 0);
        assert!(!app.close_requested());
        // Pas de scrollbar → hit-test toujours None.
        assert_eq!(app.hit_scrollbar(95.0, 10.0), None);
    }

    /// Scrollbars visibles → hit-test track/thumb/corner.
    #[test]
    fn scrollbar_hit_test() {
        let app = DwApp::new("t");
        {
            let mut v = app.view.borrow_mut();
            v.set_viewport_size(100, 100, 15, 15);
            v.set_canvas_size(100, 500, 0);
        }
        // thumb vertical : hauteur 85*100/500 = 17px en haut.
        assert_eq!(app.hit_scrollbar(92.0, 5.0), Some(ScrollbarHit::VThumb));
        // track vertical sous le pouce.
        assert_eq!(app.hit_scrollbar(92.0, 60.0), Some(ScrollbarHit::VTrack));
        // coin : hors des deux tracks (disjoints) → None.
        assert_eq!(app.hit_scrollbar(92.0, 92.0), None);
        // contenu → None.
        assert_eq!(app.hit_scrollbar(50.0, 50.0), None);
    }

    /// Clic track vertical ≙ `vscrollbar->handle(FL_PUSH)` →
    /// `scroll(ScreenDown)` — verbatim ordre de `handle`.
    #[test]
    fn scrollbar_track_click_pages() {
        let mut app = DwApp::new("t");
        {
            let mut v = app.view.borrow_mut();
            v.set_viewport_size(100, 100, 15, 15);
            v.set_canvas_size(100, 1000, 0);
        }
        app.mouse(true, 92.0, 80.0, 1);
        let sy = app.view.borrow().scroll_pos().1;
        assert!(sy > 0, "track sous le pouce → page down: {sy}");
    }

    /// Drag du pouce → `scroll_to` proportionnel.
    #[test]
    fn scrollbar_thumb_drag_scrolls() {
        let mut app = DwApp::new("t");
        {
            let mut v = app.view.borrow_mut();
            v.set_viewport_size(100, 100, 15, 15);
            v.set_canvas_size(100, 1000, 0);
        }
        app.mouse(true, 92.0, 5.0, 1);
        assert_eq!(app.scroll_drag, Some(ScrollbarHit::VThumb));
        app.scrollbar_drag(92.0, 60.0);
        assert!(app.view.borrow().scroll_pos().1 > 0);
        app.mouse(false, 92.0, 60.0, 1);
        assert_eq!(app.scroll_drag, None);
    }
}
