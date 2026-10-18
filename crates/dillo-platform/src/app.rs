//! `app` — boucle `winit` + dispatch des événements (≙
//! `FltkViewport::handle` + `Fl::run`).
//!
//! Traduction verbatim `fltkviewbase.cc`/`fltkviewport.cc` :
//!
//! - `getDwButtonState` → `DwWin::dw_state` (`FL_SHIFT`≙
//!   `SHIFT_MASK`, `FL_CTRL`≙`CONTROL_MASK`, `FL_ALT`≙`META_MASK`,
//!   boutons 1-3) — winit `ModifiersChanged` + suivi des boutons.
//! - `FL_PUSH`/`FL_RELEASE` → `Layout::button_event`
//!   (`Fl::event_clicks()+1` ≙ double-clic < 500 ms même bouton).
//! - `FL_MOVE`/`FL_DRAG` → `motion_notify` ; `FL_ENTER`/`LEAVE` →
//!   `enter_notify`/`leave_notify`.
//! - `FL_MOUSEWHEEL` → `scroll_to` (delta ≙ `linesize` ≙
//!   `scroll_step` — verbatim `Fl_Scrollbar::handle`).
//! - `FL_KEYBOARD` → le parent (verbatim : `return 0` → raccourci
//!   shell) — en mode chrome c'est la seam [`ShellGlue::event`]
//!   qui joue `UI::handle` (les events atteignent le chrome avant
//!   le viewport de l'onglet actif).
//! - Damage → `take_damage` → `Layout::draw` par région →
//!   `present` (≙ `draw_area` + `fl_push_clip`).
//! - Idle : `drain_idles` ≙ `Fl::add_idle` + [`ShellGlue::tick`]
//!   (nav différée/timeouts — `poll_timeouts` partagé).
//!
//! Multi-fenêtres : amont = une `Fl_Double_Window` par
//! `BrowserWindow` pilotées par `Fl::run`. Ici [`DwApp`] tient un
//! `Vec<DwWin>` (1 fenêtre ≙ 1 `DwWin`), les `create_window`
//! demandées pendant un handler sont matérialisées en fin de
//! passe (winit exige `ActiveEventLoop`) — verbatim
//! `a_UIcmd_browser_window_new` côté ordre observable (la fenêtre
//! existe avant l'URL ouverte dedans).
//!
//! Scrollbars : amont ce sont de vrais `Fl_Scrollbar` enfants du
//! viewport ; ici [`ScrollbarGlue`] les dessine dans le pixmap de
//! la vue et hit-teste les clics — réutilisable par le shell pour
//! la vue de l'onglet actif (épaisseur `SCROLLBAR_THICKNESS` = 15,
//! `scrollbarOnLeft` verbatim).
//!
//! `attach_view` : amont le layout tient le widget par pointeur
//! non-possédant → ici [`SharedView`] (`view.rs`), proxy `Weak`
//! sur le `Canvas` possédé par la `SkiaView` de la fenêtre (le
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
#![allow(clippy::doc_markdown)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::too_many_lines)]

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use dillo_dw::events::{button_state, ButtonEventType};
use dillo_dw::layout::Layout;
use dillo_dw::types::{Rectangle, ScrollCommand};
use dillo_dw::ui::{Resource, TextKey};
use dillo_dw::view::View;
use tiny_skia::{FillRule, Paint, PathBuilder, Rect, Transform};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
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
pub enum ScrollbarHit {
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

/// Triplet `(vtrack, vthumb, htrack, hthumb)` — rectangles
/// scrollbar en coords *view*.
type ScrollbarRects = (Option<Rect>, Option<Rect>, Option<Rect>, Option<Rect>);

/// Scrollbars dessinées + drag — ≙ les `Fl_Scrollbar` enfants du
/// viewport amont (approximation documentée : track + pouce, sans
/// flèches). Réutilisable sur n'importe quelle `SkiaView` (vue de
/// fenêtre standalone ou vue d'onglet pilotée par le shell).
#[derive(Default)]
pub struct ScrollbarGlue {
    /// Pouces en cours de drag.
    drag: Option<ScrollbarHit>,
    /// Offset du grab dans le pouce.
    drag_off: i32,
}

impl ScrollbarGlue {
    /// Rectangles scrollbar en coords *view* (≙ `adjustScrollbars
    /// AndGadgetsAllocation` + thumb proportionnel).
    #[must_use]
    pub fn rects(view: &Rc<RefCell<SkiaView>>) -> ScrollbarRects {
        let (vw, vh, hsb, vsb, on_left, sx, sy, cw, ch) = {
            let mut v = view.borrow_mut();
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
    #[must_use]
    pub fn hit(view: &Rc<RefCell<SkiaView>>, x: f32, y: f32) -> Option<ScrollbarHit> {
        let (vt, vth, ht, hth) = Self::rects(view);
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

    /// Dessine les scrollbars dans le back-buffer de `view` (≙
    /// draw des `Fl_Scrollbar` enfants).
    pub fn draw(&self, view: &Rc<RefCell<SkiaView>>) {
        let (vt, vth, ht, hth) = Self::rects(view);
        let mut v = view.borrow_mut();
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

    /// ≙ `FL_PUSH` scrollbar : thumb → début drag, track →
    /// page scroll (`ScreenUp/Down/Left/Right` verbatim).
    /// Retourne `true` si l'event a été mangé par la scrollbar.
    pub fn press(&mut self, view: &Rc<RefCell<SkiaView>>, x: f32, y: f32) -> bool {
        match Self::hit(view, x, y) {
            Some(hit @ (ScrollbarHit::VThumb | ScrollbarHit::HThumb)) => {
                let (_, vth, _, hth) = Self::rects(view);
                self.drag_off = match hit {
                    ScrollbarHit::VThumb => vth.map_or(0, |r| (y - r.y()) as i32),
                    _ => hth.map_or(0, |r| (x - r.x()) as i32),
                };
                self.drag = Some(hit);
                true
            }
            Some(ScrollbarHit::VTrack) => {
                let (_, vth, ..) = Self::rects(view);
                let cmd = if vth.is_some_and(|r| y < r.y()) {
                    ScrollCommand::ScreenUp
                } else {
                    ScrollCommand::ScreenDown
                };
                view.borrow_mut().scroll(cmd);
                true
            }
            Some(ScrollbarHit::HTrack) => {
                let (.., hth) = Self::rects(view);
                let cmd = if hth.is_some_and(|r| x < r.x()) {
                    ScrollCommand::ScreenLeft
                } else {
                    ScrollCommand::ScreenRight
                };
                view.borrow_mut().scroll(cmd);
                true
            }
            Some(ScrollbarHit::Corner) => true,
            None => false,
        }
    }

    /// Drag d'un pouce — `scroll_to` proportionnel (≙
    /// `Fl_Scrollbar::handle(FL_DRAG)`). Retourne `true` si un
    /// drag était en cours.
    pub fn drag_move(&mut self, view: &Rc<RefCell<SkiaView>>, x: f32, y: f32) -> bool {
        match self.drag {
            Some(ScrollbarHit::VThumb) => {
                let (vtrack, vthumb, ..) = Self::rects(view);
                let (Some(track), Some(thumb)) = (vtrack, vthumb) else {
                    return false;
                };
                let range = track.height() - thumb.height();
                let frac = (y - track.y() - self.drag_off as f32) / range.max(1.0);
                let (sx, ch, vh) = {
                    let v = view.borrow();
                    let (_, vh) = v.view_size();
                    let (_, ch) = v.canvas_size();
                    (v.scroll_pos().0, ch, vh)
                };
                view.borrow_mut()
                    .scroll_to(sx, (frac * (ch - vh).max(0) as f32) as i32);
                true
            }
            Some(ScrollbarHit::HThumb) => {
                let (.., htrack, hthumb) = Self::rects(view);
                let (Some(track), Some(thumb)) = (htrack, hthumb) else {
                    return false;
                };
                let range = track.width() - thumb.width();
                let frac = (x - track.x() - self.drag_off as f32) / range.max(1.0);
                let (sy, cw, vw) = {
                    let v = view.borrow();
                    let (vw, _) = v.view_size();
                    let (cw, _) = v.canvas_size();
                    (v.scroll_pos().1, cw, vw)
                };
                view.borrow_mut()
                    .scroll_to((frac * (cw - vw).max(0) as f32) as i32, sy);
                true
            }
            _ => false,
        }
    }

    /// Relâche un drag en cours (≙ `FL_RELEASE` sur le pouce).
    /// Retourne `true` si l'event a été mangé.
    pub fn release(&mut self) -> bool {
        self.drag.take().is_some()
    }
}

/// Événement traduit pour le chrome — ≙ `Fl::event_*` lus par
/// `UI::handle` avant les enfants (verbatim ui.cc). `win` = index
/// de fenêtre ([`DwApp::open_window`]).
#[derive(Debug)]
pub enum ShellEv<'a> {
    /// `FL_PUSH` — coords fenêtre, bouton dw (1..5), `clicks` ≙
    /// `Fl::event_clicks()+1`, `state` ≙ `getDwButtonState`.
    Press {
        /// Index fenêtre.
        win: usize,
        /// Coords fenêtre.
        x: i32,
        /// Coords fenêtre.
        y: i32,
        /// Bouton (1=gauche, 2=milieu, 3=droit).
        button: i32,
        /// Compteur de clic (double-clic < 500 ms).
        clicks: i32,
        /// Masque mods+boutons ≙ `getDwButtonState`.
        state: i32,
    },
    /// `FL_RELEASE`.
    Release {
        /// Index fenêtre.
        win: usize,
        /// Coords fenêtre.
        x: i32,
        /// Coords fenêtre.
        y: i32,
        /// Bouton.
        button: i32,
        /// Compteur de clic (≙ `Fl::event_clicks()+1` du press).
        clicks: i32,
        /// État mods.
        state: i32,
    },
    /// `FL_MOVE`/`FL_DRAG`.
    Move {
        /// Index fenêtre.
        win: usize,
        /// Coords fenêtre.
        x: i32,
        /// Coords fenêtre.
        y: i32,
        /// État mods+boutons.
        state: i32,
    },
    /// `FL_ENTER`.
    Enter {
        /// Index fenêtre.
        win: usize,
        /// État mods.
        state: i32,
    },
    /// `FL_LEAVE`.
    Leave {
        /// Index fenêtre.
        win: usize,
        /// État mods.
        state: i32,
    },
    /// `FL_MOUSEWHEEL` (delta ≙ `e_dx`/`e_dy` en pas de ligne).
    Wheel {
        /// Index fenêtre.
        win: usize,
        /// Delta horizontal.
        dx: f32,
        /// Delta vertical.
        dy: f32,
        /// État mods.
        state: i32,
    },
    /// `FL_KEYBOARD` — event winit (≙ `Fl::event_key`/`event_text`
    /// + état mods).
    Key {
        /// Index fenêtre.
        win: usize,
        /// Événement clavier winit.
        event: &'a KeyEvent,
        /// Masque `FL_*` brut ≙ `Fl::event_state()` (verbatim —
        /// `keys.cc` le lit tel quel, pas `getDwButtonState`).
        state: i32,
    },
    /// `resize` de la fenêtre (`FLTK::resize` amont → rearrange
    /// chrome + viewport).
    Resize {
        /// Index fenêtre.
        win: usize,
        /// Nouvelle taille.
        w: u32,
        /// Nouvelle taille.
        h: u32,
    },
    /// `WM_DELETE` (≙ `Fl::callback` fenêtre → `close_bw`).
    Close {
        /// Index fenêtre.
        win: usize,
    },
}

/// Seam « chrome » — ≙ `UI`/`CustTabs` : le widget fenêtre reçoit
/// chaque event **avant** le viewport (verbatim `UI::handle`).
/// Retour `true` = event consommé (pas de dispatch viewport).
///
/// Le shell compose aussi le frame : [`ShellGlue::draw`] écrit le
/// chrome + le contenu de l'onglet actif dans le back-buffer de la
/// fenêtre (≙ `UI::draw` + sous-arbre FLTK).
pub trait ShellGlue {
    /// `UI::handle` — `true` si consommé.
    fn event(&mut self, _app: &mut DwApp, _ev: &ShellEv<'_>) -> bool {
        false
    }
    /// `UI::draw` — compose dans `app.win_view(win)` (back-buffer).
    fn draw(&mut self, _app: &mut DwApp, _win: usize) {}
    /// Après la passe d'events (≙ drains entre events + timeouts
    /// FLTK) — nav différée, `poll_timeouts`, socket de contrôle.
    fn tick(&mut self, _app: &mut DwApp) {}
    /// `true` → `el.exit()` (≙ sortie boucle quand plus de
    /// fenêtres — `Fl::wait() > 0`).
    fn wants_exit(&mut self) -> bool {
        false
    }
    /// ≙ `Fl::focus()` resté sur un contrôle de formulaire in-page
    /// de l'onglet actif : `Some((res, view))` = la page détient le
    /// clavier (saisie à router à `res` avant le chrome ; `view` =
    /// la vue de l'onglet — son canvas porte le damage du caret,
    /// distinct du back-buffer fenêtre). `None` quand le chrome
    /// détient le clavier (location/findbar ≙ `CustInput`,
    /// popup ≙ grab `Fl_Menu`) ou qu'aucun contrôle n'a le focus —
    /// la défocalisation du caret in-page est alors faite par
    /// l'impl (≙ `FL_UNFOCUS` de l'enfant quand `Fl::focus` part
    /// vers un widget du chrome).
    #[allow(clippy::type_complexity)]
    fn page_focus(&mut self, _win: usize) -> Option<(Rc<dyn Resource>, Rc<RefCell<SkiaView>>)> {
        None
    }
    /// Réveil demandé ≙ le `timeout` de `Fl::wait(t)` — `Some(d)`
    /// → `ControlFlow::WaitUntil(now+d)` (réseau/timers actifs) ;
    /// `None` → `Wait` (boucle endormie, verbatim amont idle).
    fn wake_after(&self) -> Option<std::time::Duration> {
        None
    }
}

/// `DwWin` — état par fenêtre (≙ une `Fl_Double_Window` +
/// son `FltkViewport` amont).
pub struct DwWin {
    /// Vue racine / back-buffer (en mode chrome : pixmap composé
    /// par [`ShellGlue::draw`] ; standalone : viewport `Layout`).
    view: Rc<RefCell<SkiaView>>,
    /// Layout attaché (≙ `theLayout`) — mode standalone.
    layout: Option<Rc<RefCell<Layout>>>,
    /// Surface de présentation (créée quand le `create_window`
    /// différé aboutit).
    surface: Option<PlatformSurface>,
    /// `WindowId` winit une fois créée.
    wid: Option<WindowId>,
    /// Attributs demandés (création différée ≙ `Fl_Window::show`).
    attrs: Option<WindowAttributes>,
    /// Titre de fenêtre (≙ `Fl_Window` label).
    title: String,
    /// Modifiers courants (masque `button_state::*` — `Alt→META_MASK`
    /// verbatim `getDwButtonState`).
    mods: i32,
    /// Masque FLTK brut (≙ `Fl::event_state()` lu par `keys.cc` /
    /// `UI::handle` — `FL_SHIFT=1`, `FL_CTRL=4`, `FL_ALT=8`,
    /// `FL_META=0x20` ; distinct de `mods` où `Alt→META_MASK`).
    fl_mods: i32,
    /// Boutons enfoncés (masque `BUTTON*_MASK`).
    buttons_down: i32,
    /// ≙ `Fl::event_clicks` — compteur de clic courant.
    clicks: i32,
    /// `(instant, bouton)` du dernier press (double-clic 500 ms).
    last_click: Option<(Instant, i32)>,
    /// Scrollbars de la vue de fenêtre (mode standalone).
    scroll: ScrollbarGlue,
    /// Handler clavier du shell (≙ `FL_KEYBOARD` remonté au
    /// parent — verbatim `return 0` → raccourci).
    key_handler: RefCell<Option<Rc<RefCell<dyn FnMut(&KeyEvent)>>>>,
    /// `CloseRequested` signalé (le shell décide).
    close_requested: bool,
}

impl DwWin {
    fn new(platform: &Rc<RefCell<SkiaPlatform>>, title: &str) -> Self {
        Self {
            view: Rc::new(RefCell::new(SkiaView::new(platform, 1, 1))),
            layout: None,
            surface: None,
            wid: None,
            attrs: Some(WindowAttributes::default().with_title(title)),
            title: title.to_string(),
            mods: 0,
            fl_mods: 0,
            buttons_down: 0,
            clicks: 0,
            last_click: None,
            scroll: ScrollbarGlue::default(),
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
    pub fn set_key_handler(&self, h: Rc<RefCell<dyn FnMut(&KeyEvent)>>) {
        *self.key_handler.borrow_mut() = Some(h);
    }

    /// `close_requested` vu par le shell.
    #[must_use]
    pub fn close_requested(&self) -> bool {
        self.close_requested
    }

    /// `getDwButtonState` verbatim (fltkviewbase.cc:201).
    #[must_use]
    pub fn dw_state(&self) -> i32 {
        self.mods | self.buttons_down
    }

    /// `Fl::event_state()` brut (masque `FL_*` lu par `keys.cc` et
    /// les inputs chrome — verbatim).
    #[must_use]
    pub fn fl_state(&self) -> i32 {
        self.fl_mods | self.buttons_to_fl()
    }

    /// `FL_BUTTON1..3` dans le masque `Fl::event_state`
    /// (0x100/0x200/0x400 — Enumerations.H verbatim).
    fn buttons_to_fl(&self) -> i32 {
        let mut m = 0;
        if self.buttons_down & button_state::BUTTON1_MASK != 0 {
            m |= 0x0100;
        }
        if self.buttons_down & button_state::BUTTON2_MASK != 0 {
            m |= 0x0200;
        }
        if self.buttons_down & button_state::BUTTON3_MASK != 0 {
            m |= 0x0400;
        }
        m
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
    pub fn next_clicks(&mut self, button: i32) -> i32 {
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

    /// ≙ `FL_PUSH`/`FL_RELEASE` — scrollbar d'abord (verbatim
    /// ordre de `handle`), puis `Layout::button_event`.
    pub fn mouse(&mut self, press: bool, x: f32, y: f32, button: i32) {
        let state = self.dw_state();
        let (cx, cy) = {
            let v = self.view.borrow();
            (v.to_canvas_x(x as i32), v.to_canvas_y(y as i32))
        };
        if press {
            if self.scroll.press(&self.view, x, y) {
                self.request_redraw();
                return;
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
            if self.scroll.release() {
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

    /// `redraw` standalone — consomme le damage et présente (≙
    /// `Fl_Widget::draw` + `draw_area` + `fl_push_clip`).
    pub fn redraw(&mut self) {
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
        self.scroll.draw(&self.view);
        self.present();
    }

    /// `present` — blit back-buffer → surface (≙ `fl_copy_offscreen`
    /// + swap).
    pub fn present(&mut self) {
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
    pub fn request_redraw(&self) {
        if let Some(s) = &self.surface {
            s.window().request_redraw();
        }
    }
}

/// `DwApp` — `ApplicationHandler` winit multi-fenêtres : une
/// [`DwWin`] par fenêtre (≙ les `Fl_Double_Window` amont), une
/// [`SkiaPlatform`] partagée (≙ `FltkPlatform` singleton), un
/// [`ShellGlue`] optionnel (≙ `UI` recevant les events avant le
/// viewport).
pub struct DwApp {
    /// Plateforme (fontes/idles/clipboard ≙ `FltkPlatform`).
    platform: Rc<RefCell<SkiaPlatform>>,
    /// Fenêtres ouvertes (index = slot stable ≙ `bw` order).
    wins: Vec<DwWin>,
    /// Index des fenêtres demandées pas encore créées (winit
    /// exige `ActiveEventLoop` — verbatim : la fenêtre est posée
    /// avant l'event suivant).
    pending: VecDeque<usize>,
    /// `WindowId` → index fenêtre.
    wids: HashMap<WindowId, usize>,
    /// Seam chrome (≙ `UI::handle`/`UI::draw` amont).
    shell: Option<Rc<RefCell<dyn ShellGlue>>>,
    /// Fenêtres à détruire en fin de passe (≙ `delete` différé).
    dead: Vec<usize>,
}

impl DwApp {
    /// Ctor — plateforme + première fenêtre demandée (≙
    /// `Fl_Window::show` différé par winit : matérialisée à
    /// `resumed`). Compat mode standalone (une fenêtre).
    #[must_use]
    pub fn new(title: &str) -> Self {
        let mut app = Self::new_empty();
        app.open_window_attrs(WindowAttributes::default().with_title(title));
        app
    }

    /// Ctor sans fenêtre initiale — mode shell (≙ `main` amont où
    /// la première fenêtre vient de `a_UIcmd_browser_window_new`,
    /// donc d'un `WinOp`-like du shell, pas du ctor). Le shell
    /// **doit** être installé via [`DwApp::set_shell`] sinon la
    /// boucle sort immédiatement (`Fl::wait() > 0` sans fenêtre).
    #[must_use]
    pub fn new_empty() -> Self {
        let platform = Rc::new(RefCell::new(SkiaPlatform::new()));
        Self {
            platform,
            wins: Vec::new(),
            pending: VecDeque::new(),
            wids: HashMap::new(),
            shell: None,
            dead: Vec::new(),
        }
    }

    /// `a_UIcmd_browser_window_new` côté fenêtre — demande une
    /// nouvelle fenêtre ; retourne son index (création winit
    /// différée à la fin de la passe d'events).
    pub fn open_window(&mut self, title: &str) -> usize {
        self.open_window_attrs(WindowAttributes::default().with_title(title))
    }

    /// Variante avec attributs complets (position/taille ≙
    /// `Fl_Window::resize`).
    pub fn open_window_attrs(&mut self, attrs: WindowAttributes) -> usize {
        let title = attrs.title.clone();
        let mut w = DwWin::new(&self.platform, &title);
        w.attrs = Some(attrs);
        // Réutilise un slot mort sinon append (index stables).
        let idx = self
            .wins
            .iter()
            .position(|w| w.surface.is_none() && w.attrs.is_none())
            .unwrap_or_else(|| {
                self.wins.push(DwWin::new(&self.platform, ""));
                self.wins.len() - 1
            });
        self.wins[idx] = w;
        self.pending.push_back(idx);
        idx
    }

    /// Nombre de fenêtres vivantes.
    #[must_use]
    pub fn win_count(&self) -> usize {
        self.wins
            .iter()
            .filter(|w| w.surface.is_some() || w.attrs.is_some())
            .count()
    }

    /// Vue racine d'une fenêtre (back-buffer / viewport).
    #[must_use]
    pub fn win_view(&self, win: usize) -> Option<Rc<RefCell<SkiaView>>> {
        self.wins.get(win).map(|w| Rc::clone(&w.view))
    }

    /// `DwWin` d'une fenêtre.
    #[must_use]
    pub fn win(&mut self, win: usize) -> Option<&mut DwWin> {
        self.wins.get_mut(win)
    }

    /// Titre courant (`set_title` ≙ `window->label`).
    pub fn set_title(&mut self, win: usize, title: &str) {
        if let Some(w) = self.wins.get_mut(win) {
            w.title = title.to_string();
            if let Some(s) = &w.surface {
                s.window().set_title(title);
            }
        }
    }

    /// Demande la fermeture d'une fenêtre (≙ `delete window`
    /// différé à la fin de la passe — verbatim
    /// `a_UIcmd_close_bw`).
    pub fn close_window(&mut self, win: usize) {
        if win < self.wins.len() {
            self.dead.push(win);
        }
    }

    /// `win->position(x, y)` — coords écran en px (verbatim prefs
    /// `xpos`/`ypos`). Édite les attrs si la fenêtre est encore en
    /// attente de création, sinon `set_outer_position`.
    pub fn set_position(&mut self, win: usize, x: i32, y: i32) {
        if let Some(w) = self.wins.get_mut(win) {
            if let Some(attrs) = &mut w.attrs {
                attrs.position = Some(PhysicalPosition::new(x, y).into());
            } else if let Some(s) = &w.surface {
                s.window().set_outer_position(PhysicalPosition::new(x, y));
            }
        }
    }

    /// `w.redraw()` sur une fenêtre (≙ `Fl::flush` ciblé).
    pub fn request_redraw(&mut self, win: usize) {
        if let Some(w) = self.wins.get(win) {
            w.request_redraw();
        }
    }

    /// Shell chrome (≙ `UI::handle`/`draw` amont).
    pub fn set_shell(&mut self, shell: Rc<RefCell<dyn ShellGlue>>) {
        self.shell = Some(shell);
    }

    /// `run` — `EventLoop::new` + `run_app` (≙ `Fl::run`).
    ///
    /// # Errors
    /// `AppError` sur échec event loop/fenêtre/surface.
    pub fn run(mut self) -> Result<(), AppError> {
        let el = EventLoop::new()?;
        // `Fl::wait()` bloquant par défaut — `about_to_wait`
        // rebascule en `WaitUntil` quand le shell a des échéances.
        el.set_control_flow(ControlFlow::Wait);
        el.run_app(&mut self)?;
        Ok(())
    }

    /// Plateforme (pour le shell — fontes, resource factory).
    #[must_use]
    pub fn platform(&self) -> Rc<RefCell<SkiaPlatform>> {
        Rc::clone(&self.platform)
    }

    /// Vue racine de la première fenêtre (compat standalone).
    #[must_use]
    pub fn view(&self) -> Rc<RefCell<SkiaView>> {
        Rc::clone(&self.wins[0].view)
    }

    /// Attache un layout à la première fenêtre (compat).
    pub fn attach_layout(&mut self, layout: Rc<RefCell<Layout>>) {
        self.wins[0].attach_layout(layout);
    }

    /// Détache (compat).
    pub fn detach_layout(&mut self) {
        self.wins[0].detach_layout();
    }

    /// `key_handler` de la première fenêtre (compat).
    pub fn set_key_handler(&self, h: Rc<RefCell<dyn FnMut(&KeyEvent)>>) {
        self.wins[0].set_key_handler(h);
    }

    /// `close_requested` de la première fenêtre (compat).
    #[must_use]
    pub fn close_requested(&self) -> bool {
        self.wins.first().is_some_and(|w| w.close_requested)
    }

    /// Matérialise les fenêtres demandées (≙ `Fl_Window::show`
    /// groupé — winit exige `ActiveEventLoop`).
    fn drain_pending(&mut self, el: &ActiveEventLoop) {
        while let Some(i) = self.pending.pop_front() {
            let Some(w) = self.wins.get_mut(i) else {
                continue;
            };
            if w.surface.is_some() {
                continue;
            }
            let Some(attrs) = w.attrs.take() else {
                continue;
            };
            match el.create_window(attrs) {
                Ok(win) => {
                    let win = std::sync::Arc::new(win);
                    w.wid = Some(win.id());
                    self.wids.insert(win.id(), i);
                    // ≙ `Fl::screen_dpi` par moniteur — calibre les
                    // métriques `dpiX`/`dpiY` (tailles pt → px).
                    self.platform.borrow().set_scale_factor(win.scale_factor());
                    let size = win.inner_size();
                    match PlatformSurface::new(win) {
                        Ok(s) => {
                            w.surface = Some(s);
                            w.view.borrow_mut().resize(size.width, size.height);
                            w.request_redraw();
                        }
                        Err(e) => {
                            tracing::warn!("surface: {e}");
                            self.dead.push(i);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("create_window: {e}");
                    self.dead.push(i);
                }
            }
        }
    }

    /// Détruit les fenêtres marquées (≙ `delete window`) — les
    /// slots restent en place (tombstones) : les index fenêtre sont
    /// stables pour le mapping shell ↔ chrome.
    fn drain_dead(&mut self) {
        for i in self.dead.drain(..) {
            if let Some(w) = self.wins.get_mut(i) {
                if let Some(wid) = w.wid.take() {
                    self.wids.remove(&wid);
                }
                w.surface = None; // drop → destruction fenêtre winit
                w.attrs = None;
                w.layout = None;
            }
        }
    }

    /// `true` si au moins une fenêtre est vivante ou demandée
    /// (≙ `Fl::wait() > 0` tant que des `Fl_Window` existent).
    fn any_alive(&self) -> bool {
        self.wins
            .iter()
            .any(|w| w.surface.is_some() || w.attrs.is_some())
    }
}

impl ApplicationHandler for DwApp {
    /// `resumed` — création des fenêtres demandées (≙
    /// `Fl_Window::show` différé par winit).
    fn resumed(&mut self, el: &ActiveEventLoop) {
        self.drain_pending(el);
        self.drain_dead();
    }

    /// `window_event` — dispatch ≙ `FltkViewport::handle` ; le
    /// shell ([`ShellGlue`]) voit d'abord les events consommables
    /// (verbatim `UI::handle` parent du viewport).
    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(wi) = self.wids.get(&id).copied() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                let consumed = self.shell_event(&ShellEv::Close { win: wi });
                if !consumed {
                    self.wins[wi].close_requested = true;
                    self.dead.push(wi);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // winit renvoie un `Resized` juste après — on ne fait
                // que recalibrer les métriques (≙ `screen_dpi`).
                self.platform.borrow().set_scale_factor(scale_factor);
            }
            WindowEvent::Resized(sz) => {
                {
                    let w = &mut self.wins[wi];
                    w.view.borrow_mut().resize(sz.width, sz.height);
                    if let Some(l) = &w.layout {
                        l.borrow_mut()
                            .viewport_size_changed(sz.width as i32, sz.height as i32);
                    }
                    w.view.borrow_mut().queue_draw_total();
                }
                self.shell_event(&ShellEv::Resize {
                    win: wi,
                    w: sz.width,
                    h: sz.height,
                });
            }
            WindowEvent::RedrawRequested => {
                if self.shell.is_some() {
                    let g = self.shell.clone();
                    if let Some(g) = g {
                        g.borrow_mut().draw(self, wi);
                    }
                    self.wins[wi].present();
                } else {
                    self.wins[wi].redraw();
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                // `getDwButtonState` verbatim (FL_ALT→META_MASK) +
                // masque FLTK brut pour `Fl::event_state()`
                // (`FL_SHIFT=1/FL_CTRL=4/FL_ALT=8/FL_META=0x20` —
                // super_key ≙ Meta réel).
                let st = m.state();
                self.wins[wi].mods = i32::from(st.shift_key()) * button_state::SHIFT_MASK
                    + i32::from(st.control_key()) * button_state::CONTROL_MASK
                    + i32::from(st.alt_key()) * button_state::META_MASK;
                self.wins[wi].fl_mods = i32::from(st.shift_key())
                    | i32::from(st.control_key()) << 2
                    | i32::from(st.alt_key()) << 3
                    | i32::from(st.super_key()) << 5;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as i32, position.y as i32);
                let st;
                {
                    let w = &mut self.wins[wi];
                    w.view.borrow_mut().set_mouse_pos(x, y);
                    st = w.dw_state();
                }
                let dragging = {
                    let w = &mut self.wins[wi];
                    w.scroll
                        .drag_move(&w.view, position.x as f32, position.y as f32)
                };
                if dragging {
                    self.wins[wi].request_redraw();
                } else {
                    let consumed = self.shell_event(&ShellEv::Move {
                        win: wi,
                        x,
                        y,
                        state: st,
                    });
                    if !consumed {
                        let (cx, cy) = {
                            let v = self.wins[wi].view.borrow();
                            (v.to_canvas_x(x), v.to_canvas_y(y))
                        };
                        if let Some(l) = &self.wins[wi].layout {
                            l.borrow_mut().motion_notify(cx, cy, st);
                        }
                    }
                }
            }
            WindowEvent::CursorEntered { .. } => {
                let st = self.wins[wi].dw_state();
                let consumed = self.shell_event(&ShellEv::Enter { win: wi, state: st });
                if !consumed {
                    let (cx, cy) = {
                        let v = self.wins[wi].view.borrow();
                        let (x, y) = v.mouse_pos();
                        (v.to_canvas_x(x), v.to_canvas_y(y))
                    };
                    if let Some(l) = &self.wins[wi].layout {
                        l.borrow_mut().enter_notify(cx, cy, st);
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                let st = self.wins[wi].dw_state();
                self.wins[wi].view.borrow_mut().set_mouse_pos(-1, -1);
                let consumed = self.shell_event(&ShellEv::Leave { win: wi, state: st });
                if !consumed {
                    if let Some(l) = &self.wins[wi].layout {
                        l.borrow_mut().leave_notify(st);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = DwWin::dw_button(button);
                let (x, y) = self.wins[wi].view.borrow().mouse_pos();
                let press = state == ElementState::Pressed;
                if press {
                    // verbatim : scrollbar d'abord (ordre de
                    // `handle`) — en mode standalone la vue de
                    // fenêtre a les siennes ; le shell gère celles
                    // de l'onglet actif lui-même.
                    if self.shell.is_none() {
                        let w = &mut self.wins[wi];
                        let view = Rc::clone(&w.view);
                        if w.scroll.press(&view, x as f32, y as f32) {
                            w.request_redraw();
                            self.finish_pass(el);
                            return;
                        }
                    }
                    let n = self.wins[wi].next_clicks(b);
                    let mask = DwWin::button_mask(b);
                    self.wins[wi].buttons_down |= mask;
                    let st = self.wins[wi].dw_state();
                    let consumed = self.shell_event(&ShellEv::Press {
                        win: wi,
                        x,
                        y,
                        button: b,
                        clicks: n,
                        state: st,
                    });
                    if !consumed {
                        let (cx, cy) = {
                            let v = self.wins[wi].view.borrow();
                            (v.to_canvas_x(x), v.to_canvas_y(y))
                        };
                        if let Some(l) = &self.wins[wi].layout {
                            l.borrow_mut().button_event(
                                ButtonEventType::ButtonPress,
                                n,
                                cx,
                                cy,
                                st,
                                b,
                            );
                        }
                    }
                    self.wins[wi].request_redraw();
                } else {
                    if self.wins[wi].scroll.release() {
                        self.wins[wi].request_redraw();
                        self.finish_pass(el);
                        return;
                    }
                    self.wins[wi].buttons_down &= !DwWin::button_mask(b);
                    let st = self.wins[wi].dw_state();
                    let clicks = self.wins[wi].clicks;
                    let consumed = self.shell_event(&ShellEv::Release {
                        win: wi,
                        x,
                        y,
                        button: b,
                        clicks,
                        state: st,
                    });
                    if !consumed {
                        let (cx, cy) = {
                            let v = self.wins[wi].view.borrow();
                            (v.to_canvas_x(x), v.to_canvas_y(y))
                        };
                        if let Some(l) = &self.wins[wi].layout {
                            l.borrow_mut().button_event(
                                ButtonEventType::ButtonRelease,
                                clicks,
                                cx,
                                cy,
                                st,
                                b,
                            );
                        }
                    }
                    self.wins[wi].request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let st = self.wins[wi].dw_state();
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x, y),
                    MouseScrollDelta::PixelDelta(p) => {
                        let step = self.wins[wi].view.borrow().scroll_step();
                        ((p.x as f32) / step.max(1.0), (p.y as f32) / step.max(1.0))
                    }
                };
                let consumed = self.shell_event(&ShellEv::Wheel {
                    win: wi,
                    dx,
                    dy,
                    state: st,
                });
                if !consumed {
                    // ≙ `Fl_Scrollbar::handle(FL_MOUSEWHEEL)` —
                    // `value - e_dy * linesize`.
                    let step = self.wins[wi].view.borrow().scroll_step();
                    let (sx, sy) = self.wins[wi].view.borrow().scroll_pos();
                    self.wins[wi]
                        .view
                        .borrow_mut()
                        .scroll_to(sx - (dx * step) as i32, sy - (dy * step) as i32);
                    self.wins[wi].request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // ≙ FLTK : la touche va d'abord au widget enfant
                // focusé (`Fl::focus()` ≙ contrôle de formulaire
                // in-page) puis remonte au parent si non consommée.
                // En mode chrome le focus vit sur le canvas de la
                // vue de l'onglet actif (`u.view`, partagé avec le
                // layout via `SharedView`) — le shell le résout et
                // retire le caret si le chrome détient le clavier
                // (≙ `Fl::focus` unique). Standalone : la vue de la
                // fenêtre partage le canvas du layout.
                // `state` = masque FLTK brut (verbatim `keys.cc` lit
                // `Fl::event_state()`).
                let focused = if let Some(g) = self.shell.clone() {
                    g.borrow_mut().page_focus(wi)
                } else {
                    let res = self.wins[wi].view.borrow().focused_resource();
                    res.map(|r| (r, Rc::clone(&self.wins[wi].view)))
                };
                let consumed = focused
                    .as_ref()
                    .is_some_and(|(r, v)| self.dispatch_text_key(wi, r, v, &event));
                if !consumed {
                    let st = self.wins[wi].fl_state();
                    let consumed = self.shell_event(&ShellEv::Key {
                        win: wi,
                        event: &event,
                        state: st,
                    });
                    if !consumed {
                        let h = self.wins[wi].key_handler.borrow().clone();
                        if let Some(h) = h {
                            h.borrow_mut()(&event);
                        }
                    }
                }
            }
            _ => {}
        }
        self.finish_pass(el);
    }

    /// `about_to_wait` — drain des idle callbacks (≙
    /// `Fl::add_idle` quand la file d'events est vide) + damage
    /// en attente + tick shell (nav différée/timeouts/contrôle).
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // ≙ `Fl::add_idle` — drain appelle les callbacks avec
        // `&mut Layout` (verbatim `Layout::processIdles`).
        for w in &mut self.wins {
            if let Some(l) = &w.layout {
                l.borrow_mut().drain_idles();
            }
        }
        if let Some(g) = self.shell.clone() {
            g.borrow_mut().tick(self);
        }
        for w in &self.wins {
            if w.view.borrow().has_damage() {
                w.request_redraw();
            }
        }
        self.drain_pending(el);
        self.drain_dead();
        // `Fl::wait(t)` — le shell indique la prochaine échéance
        // (timers `a_Timeout_add`, conns réseau, socket de contrôle) ;
        // `Wait` sinon ≙ `Fl::wait()` bloquant amont.
        let wake = self.shell.as_ref().and_then(|g| g.borrow().wake_after());
        match wake {
            Some(d) => el.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now() + d)),
            None => el.set_control_flow(ControlFlow::Wait),
        }
        let exit = self
            .shell
            .as_ref()
            .is_some_and(|g| g.borrow_mut().wants_exit());
        // verbatim : `while (Fl::wait() > 0)` — la boucle vit tant
        // qu'il reste des fenêtres vivantes ou demandées.
        if exit || (self.pending.is_empty() && !self.any_alive()) {
            el.exit();
        }
    }
}

impl DwApp {
    /// Délivre une touche au contrôle de formulaire focusé —
    /// adaptation : amont le widget FLTK enfant la reçoit de
    /// `Fl::handle` (`Fl_Input`/`Fl_Text_Editor`). `view` = vue du
    /// contrôle (canvas du damage — onglet actif en mode chrome,
    /// ≠ back-buffer fenêtre). `true` = consommé (≙ `handle`
    /// retourne 1, pas de remontée au shell).
    fn dispatch_text_key(
        &mut self,
        wi: usize,
        res: &Rc<dyn Resource>,
        view: &Rc<RefCell<SkiaView>>,
        event: &KeyEvent,
    ) -> bool {
        let Some(tr) = res.as_text_resource() else {
            return false;
        };
        if event.state != ElementState::Pressed {
            // Key-up : non géré par le contrôle mais consommé
            // (le widget enfant l'aurait reçu, pas le shell).
            return true;
        }
        let consumed = match &event.logical_key {
            Key::Named(NamedKey::Backspace) => tr.key_event(TextKey::Backspace),
            Key::Named(NamedKey::Delete) => tr.key_event(TextKey::Delete),
            Key::Named(NamedKey::ArrowLeft) => tr.key_event(TextKey::Left),
            Key::Named(NamedKey::ArrowRight) => tr.key_event(TextKey::Right),
            Key::Named(NamedKey::Home) => tr.key_event(TextKey::Home),
            Key::Named(NamedKey::End) => tr.key_event(TextKey::End),
            Key::Named(NamedKey::Enter) => tr.key_event(TextKey::Enter),
            Key::Named(NamedKey::Space) => {
                tr.insert_text(" ");
                true
            }
            Key::Named(_) => false, // Tab/Escape/F* → shell.
            _ => {
                let ctrl = self.wins[wi].fl_mods & button_state::CONTROL_MASK != 0;
                if ctrl {
                    // Ctrl+V ≙ `Fl::paste` dans le contrôle focusé
                    // (les autres combinaisons remontent au shell).
                    if event
                        .logical_key
                        .to_text()
                        .is_some_and(|t| t.eq_ignore_ascii_case("v"))
                    {
                        let text = self.platform.borrow().clipboard_text();
                        tr.insert_text(&String::from_utf8_lossy(&text));
                        true
                    } else {
                        false
                    }
                } else if let Some(t) = &event.text {
                    tr.insert_text(t);
                    true
                } else {
                    false
                }
            }
        };
        if consumed {
            // Damage de la zone du contrôle (≙ redraw() du widget
            // enfant après édition/déplacement du caret).
            if let Some(a) = res.allocation() {
                view.borrow_mut().queue_draw(&Rectangle {
                    x: a.x,
                    y: a.y,
                    width: a.width,
                    height: a.ascent + a.descent,
                });
            }
            self.wins[wi].request_redraw();
        }
        consumed
    }

    /// Offre un event au shell — `true` si consommé (≙
    /// `UI::handle` retourne non-0).
    fn shell_event(&mut self, ev: &ShellEv<'_>) -> bool {
        let Some(g) = self.shell.clone() else {
            return false;
        };
        let mut g = g.borrow_mut();
        g.event(self, ev)
    }

    /// Fin de passe : tick shell (nav différée ≙ callbacks
    /// `a_Timeout_add(0)`) + créations/destructions différées.
    fn finish_pass(&mut self, el: &ActiveEventLoop) {
        if let Some(g) = self.shell.clone() {
            g.borrow_mut().tick(self);
        }
        self.drain_pending(el);
        self.drain_dead();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Fl::event_button` mapping verbatim (1/2/3).
    #[test]
    fn dw_button_maps_fltk_order() {
        assert_eq!(DwWin::dw_button(MouseButton::Left), 1);
        assert_eq!(DwWin::dw_button(MouseButton::Middle), 2);
        assert_eq!(DwWin::dw_button(MouseButton::Right), 3);
    }

    /// `FL_BUTTON1..3` → `BUTTON*_MASK` (fltkviewbase.cc:201).
    #[test]
    fn button_mask_matches_fltk() {
        assert_eq!(DwWin::button_mask(1), button_state::BUTTON1_MASK);
        assert_eq!(DwWin::button_mask(2), button_state::BUTTON2_MASK);
        assert_eq!(DwWin::button_mask(3), button_state::BUTTON3_MASK);
        assert_eq!(DwWin::button_mask(4), 0);
    }

    /// ≙ `Fl::event_clicks` — double-clic < 500 ms même bouton.
    #[test]
    fn clicks_count_double_click() {
        let mut app = DwApp::new("t");
        assert_eq!(app.wins[0].next_clicks(1), 1);
        assert_eq!(app.wins[0].next_clicks(1), 2, "même bouton <500ms → +1");
        assert_eq!(app.wins[0].next_clicks(2), 1, "autre bouton → reset");
    }

    /// `DwApp::new` headless — pas de fenêtre avant `resumed`.
    #[test]
    fn app_without_window() {
        let app = DwApp::new("t");
        assert_eq!(app.wins[0].dw_state(), 0);
        assert!(!app.close_requested());
        // Pas de scrollbar → hit-test toujours None.
        assert_eq!(ScrollbarGlue::hit(&app.wins[0].view, 95.0, 10.0), None);
    }

    /// Scrollbars visibles → hit-test track/thumb/corner.
    #[test]
    fn scrollbar_hit_test() {
        let app = DwApp::new("t");
        {
            let mut v = app.wins[0].view.borrow_mut();
            v.set_viewport_size(100, 100, 15, 15);
            v.set_canvas_size(100, 500, 0);
        }
        // thumb vertical : hauteur 85*100/500 = 17px en haut.
        assert_eq!(
            ScrollbarGlue::hit(&app.wins[0].view, 92.0, 5.0),
            Some(ScrollbarHit::VThumb)
        );
        // track vertical sous le pouce.
        assert_eq!(
            ScrollbarGlue::hit(&app.wins[0].view, 92.0, 60.0),
            Some(ScrollbarHit::VTrack)
        );
        // coin : hors des deux tracks (disjoints) → None.
        assert_eq!(ScrollbarGlue::hit(&app.wins[0].view, 92.0, 92.0), None);
        // contenu → None.
        assert_eq!(ScrollbarGlue::hit(&app.wins[0].view, 50.0, 50.0), None);
    }

    /// Clic track vertical ≙ `vscrollbar->handle(FL_PUSH)` →
    /// `scroll(ScreenDown)` — verbatim ordre de `handle`.
    #[test]
    fn scrollbar_track_click_pages() {
        let mut app = DwApp::new("t");
        {
            let mut v = app.wins[0].view.borrow_mut();
            v.set_viewport_size(100, 100, 15, 15);
            v.set_canvas_size(100, 1000, 0);
        }
        app.wins[0].mouse(true, 92.0, 80.0, 1);
        let sy = app.wins[0].view.borrow().scroll_pos().1;
        assert!(sy > 0, "track sous le pouce → page down: {sy}");
    }

    /// Drag du pouce → `scroll_to` proportionnel.
    #[test]
    fn scrollbar_thumb_drag_scrolls() {
        let mut app = DwApp::new("t");
        {
            let mut v = app.wins[0].view.borrow_mut();
            v.set_viewport_size(100, 100, 15, 15);
            v.set_canvas_size(100, 1000, 0);
        }
        app.wins[0].mouse(true, 92.0, 5.0, 1);
        assert_eq!(app.wins[0].scroll.drag, Some(ScrollbarHit::VThumb));
        let view = app.wins[0].view.clone();
        app.wins[0].scroll.drag_move(&view, 92.0, 60.0);
        assert!(app.wins[0].view.borrow().scroll_pos().1 > 0);
        app.wins[0].mouse(false, 92.0, 60.0, 1);
        assert_eq!(app.wins[0].scroll.drag, None);
    }
}
