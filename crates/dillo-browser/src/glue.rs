//! Port of src/ui.cc `UI::handle` + src/uicmd.cc `CustTabs::handle`,
//! `tab_btn_cb`, `close_tab_btn_cb`, `win_cb` — seam
//! [`ShellGlue`] reliant le `DwApp` natif (winit) au [`Chrome`]
//! logique.
//!
//! Amont, la hiérarchie FLTK fait que chaque event passe par
//! `UI::handle`/`CustTabs::handle` **avant** le `FltkViewport` du
//! contenu — ici `DwApp` offre chaque event traduit
//! ([`ShellEv`]) à [`Glue::event`] qui dispatche vers le chrome
//! (strip d'onglets, barres, findbar) puis, à défaut, vers la
//! `SkiaView`/`Layout` de l'onglet actif (coords translatées de
//! `main`).
//!
//! Le dessin est composé par [`Glue::draw`] — ≙ `UI::draw` +
//! blit de la vue de l'onglet actif dans le back-buffer fenêtre.
//! [`Glue::tick`] draine les effets de bord différés : ops fenêtres
//! natives ([`WinOp`]), navigation `a_Timeout_add(0)` (verbatim
//! `defer_nav`) et le minuteur du `TipWin`.
//!
//! Déviation documentée : le dispatch des callbacks boutons se fait
//! à `FL_PUSH` (le glue), alors que FLTK invoque `do_callback` à
//! `FL_RELEASE` dans les bornes du widget — approximation notée
//! dans `docs/PORTING.md` (les menus/popups amont *sont* ouverts
//! sur push par `Fl_Menu_Button`, cas dominant).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::missing_errors_doc
)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use dillo_dw::events::ButtonEventType;
use dillo_dw::view::View;
use dillo_net::cache::{self};
use dillo_net::control::{Control, CtlOps};
use dillo_net::url::url_new;
use dillo_net::{capi, timeout};
use dillo_platform::app::{DwApp, ShellEv, ShellGlue};
use dillo_platform::platform::SkiaPlatform;
use dillo_platform::winit::dpi::LogicalSize;
use dillo_platform::winit::event::{ElementState, KeyEvent};
use dillo_platform::winit::keyboard::{Key, NamedKey};
use dillo_platform::winit::window::WindowAttributes;

use crate::bw::BwRegistry;
use crate::dialog::{AutoCancelRunner, DialogRunner};
use crate::doc::{
    self, DocBrowserOps, DocCtx, DocDispatch, DocDpiOps, DocImageFactory, DocNavOps, DocUiOps,
    GlueOp, IoQueue,
};
use crate::keys::{keysym, KeysCommand};
use crate::prefs::Prefs;
use crate::ui::{
    chrome_widget, cust_input_key, findbar_key, ui_shortcut, Chrome, Focus, Hit, Ui, WinOp,
    MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT,
};
use crate::uicmd::{UiCmd, UiShell};

/// Boutons souris X11 verbatim (`FL_BACK_MOUSE`/`FL_FORWARD_MOUSE`
/// — ui.cc:871-874 : back/forward sur les boutons 4/5).
const MOUSE_BACK: i32 = 4;
/// `FL_FORWARD_MOUSE`.
const MOUSE_FORWARD: i32 = 5;

/// Glue plateforme↔shell — détient l'état navigateur partagé
/// ([`BwRegistry`], [`UiCmd`], prefs) et le mapping
/// `DwApp`-win ↔ `Chrome`-`tabs`.
///
/// Usage : construire avec [`Glue::new`], installer via
/// `DwApp::set_shell`, puis appeler les commandes `UiCmd` avec
/// `&mut glue.reg`, `&mut glue.chrome` (`UiShell`),
/// `&glue.prefs.borrow()`.
pub struct Glue {
    /// Chrome — impl `UiShell` (modèle logique + dessin).
    pub chrome: Chrome,
    /// Registre des `BrowserWindow` (≙ globaux `a_Bw_*` amont).
    pub reg: Rc<BwRegistry>,
    /// Prefs partagées — **le même `Rc`** que `chrome.prefs` (≙
    /// global `prefs` amont).
    pub prefs: Rc<RefCell<Prefs>>,
    /// Dispatch `a_UIcmd_*` (≙ uicmd.cc).
    pub uicmd: UiCmd,
    /// File d'ops navigateur émises par les seams réseau (≙ les
    /// `a_UIcmd_*` synchrones amont — drainées au tick, cf.
    /// [`crate::doc`]).
    pub queue: IoQueue,
    /// Contexte document (viewers/dispatch/factory) — ≙ les
    /// globaux `prefs`/`bws` vus par les viewers amont. Champ de
    /// rétention : le contexte doit vivre tant que le glue.
    pub doc_ctx: Rc<DocCtx>,
    /// Runner de dialogs (≙ `a_Dialog_*`/`fl_*` amont). Runner
    /// natif injectable ; défaut [`AutoCancelRunner`] (≙ dialog
    /// annulé — dégradation tolérante). Rétention ≙ `doc_ctx`.
    pub dialogs: Rc<RefCell<dyn DialogRunner>>,
    /// `Control` partagé (≙ globaux `bw_waiting`/`bw_waiting_req` +
    /// table fd→req). `Rc` : le `Chrome` l'appelle depuis
    /// `control_notify_finish` (`a_UIcmd_finish_loading`).
    pub ctl: Rc<RefCell<Control>>,
    /// Listener du socket de contrôle `~/.dillo/ctl/<pid>`
    /// (≙ `a_Control_listen` amont — Unix seul, cfg ≙
    /// `ENABLE_CONTROL_SOCKET`).
    #[cfg(unix)]
    ctl_listener: Option<dillo_net::control::unix::CtlListener>,
    /// `tabs` chrome → win `DwApp`.
    win_of: HashMap<usize, usize>,
    /// win `DwApp` → `tabs` chrome.
    tab_of: HashMap<usize, usize>,
    /// Origine de temps pour le `TipWin` (≙ `GetTickCount`/`Fl`
    /// event time amont — secondes depuis le démarrage).
    t0: Instant,
}

/// Traduit `logical_key` winit en keysym X11/FLTK (≙
/// `Fl::event_key()` — valeurs `keysym::*`).
///
/// Caractères : le point de code Unicode si < 0x100 (latin-1 =
/// keysym), sinon la convention X11 `0x0100_0000 | cp` (verbatim
/// `FLTK/utf8` — approx. documentée hors latin-1).
#[must_use]
pub fn winit_keysym(ev: &KeyEvent) -> i32 {
    match &ev.logical_key {
        Key::Named(n) => named_keysym(*n),
        Key::Character(s) => s.chars().next().map_or(0, |c| {
            let cp = c as u32;
            if cp < 0x100 {
                cp as i32
            } else {
                0x0100_0000 | cp as i32
            }
        }),
        Key::Unidentified(_) | Key::Dead(_) => 0,
    }
}

/// `NamedKey` → keysym (table verbatim X11).
fn named_keysym(n: NamedKey) -> i32 {
    use NamedKey as N;
    match n {
        N::Backspace => keysym::BACKSPACE,
        N::Tab => keysym::TAB,
        N::Enter => keysym::ENTER,
        N::Escape => keysym::ESCAPE,
        N::Home => keysym::HOME,
        N::ArrowLeft => keysym::LEFT,
        N::ArrowUp => keysym::UP,
        N::ArrowRight => keysym::RIGHT,
        N::ArrowDown => keysym::DOWN,
        N::PageUp => keysym::PAGE_UP,
        N::PageDown => keysym::PAGE_DOWN,
        N::End => keysym::END,
        N::PrintScreen => keysym::PRINT,
        N::Insert => keysym::INSERT,
        N::ContextMenu => keysym::MENU,
        N::Delete => keysym::DELETE,
        N::BrowserBack => keysym::BACK,
        N::BrowserForward => keysym::FORWARD,
        N::BrowserStop => keysym::STOP,
        N::BrowserRefresh => keysym::REFRESH,
        N::Standby => keysym::SLEEP,
        N::BrowserHome => keysym::HOME_PAGE,
        N::LaunchMail => keysym::MAIL,
        N::BrowserSearch => keysym::SEARCH,
        N::BrowserFavorites => keysym::FAVORITES,
        N::AudioVolumeMute => keysym::VOLUME_MUTE,
        N::AudioVolumeDown => keysym::VOLUME_DOWN,
        N::AudioVolumeUp => keysym::VOLUME_UP,
        N::MediaPlay | N::MediaPlayPause => keysym::MEDIA_PLAY,
        N::MediaStop => keysym::MEDIA_STOP,
        N::MediaTrackPrevious => keysym::MEDIA_PREV,
        N::MediaTrackNext => keysym::MEDIA_NEXT,
        N::F1 => keysym::F1,
        N::F2 => keysym::F1 + 1,
        N::F3 => keysym::F1 + 2,
        N::F4 => keysym::F1 + 3,
        N::F5 => keysym::F1 + 4,
        N::F6 => keysym::F1 + 5,
        N::F7 => keysym::F1 + 6,
        N::F8 => keysym::F1 + 7,
        N::F9 => keysym::F1 + 8,
        N::F10 => keysym::F1 + 9,
        N::F11 => keysym::F1 + 10,
        N::F12 => keysym::F1 + 11,
        _ => 0,
    }
}

/// `Fl::event_text()` ≙ `KeyEvent::text` — ignoré si caractère de
/// contrôle (verbatim `dIscntrl` dans `Keys::getKeyCmd`).
fn event_text(ev: &KeyEvent) -> Option<String> {
    let t = ev.text.as_ref()?;
    if t.chars().next().is_some_and(char::is_control) {
        None
    } else {
        Some(t.to_string())
    }
}

/// Construit les `HtmlPrefs` verbatim depuis `Prefs`
/// (≙ `prefs.*` lus par `html.cc`/`plain.cc`).
#[must_use]
pub fn html_prefs(p: &Prefs) -> Rc<dillo_html::sink::HtmlPrefs> {
    Rc::new(dillo_html::sink::HtmlPrefs {
        limit_text_width: p.limit_text_width,
        show_tooltip: p.show_tooltip,
        show_extra_warnings: p.show_extra_warnings,
        parse_embedded_css: p.parse_embedded_css,
        load_stylesheets: p.load_stylesheets,
        load_images: p.load_images,
        mark_unloaded_images: p.mark_unloaded_images,
        contrast_visited_color: p.contrast_visited_color,
        middle_click_opens_new_tab: p.middle_click_opens_new_tab,
        focus_new_tab: p.focus_new_tab,
        enterpress_forces_submit: p.enterpress_forces_submit,
        buffered_drawing: p.buffered_drawing,
        style: dillo_css::styleengine::StyleEnginePrefs {
            font_serif: p.font_serif.clone().unwrap_or_default(),
            font_sans_serif: p.font_sans_serif.clone().unwrap_or_default(),
            font_cursive: p.font_cursive.clone().unwrap_or_default(),
            font_fantasy: p.font_fantasy.clone().unwrap_or_default(),
            font_monospace: p.font_monospace.clone().unwrap_or_default(),
            font_factor: p.font_factor,
            font_min_size: p.font_min_size,
            font_max_size: p.font_max_size,
            bg_color: p.bg_color,
            allow_white_bg: p.allow_white_bg,
            white_bg_replacement: p.white_bg_replacement,
            load_background_images: p.load_background_images,
            parse_embedded_css: p.parse_embedded_css,
        },
    })
}

impl Glue {
    /// Ctor — `prefs` ≙ globale amont (déjà parsée : `UiCmd::new`
    /// capture `save_dir` ≙ `a_UIcmd_init`). Enregistre les seams
    /// cache/capi (≙ les globaux amont câblés statiquement) et les
    /// viewers MIME (≙ `a_Mime_init` + `a_Dicache_init`).
    #[must_use]
    pub fn new(platform: Rc<RefCell<SkiaPlatform>>, prefs: Rc<RefCell<Prefs>>) -> Self {
        let uicmd = UiCmd::new(&prefs.borrow());
        let mut chrome = Chrome::new(platform, Rc::clone(&prefs));
        let reg = Rc::new(BwRegistry::new());
        let queue = doc::io_queue();
        let dialogs: Rc<RefCell<dyn DialogRunner>> = Rc::new(RefCell::new(AutoCancelRunner));
        let ctx = DocCtx::new(
            Rc::clone(&reg),
            queue.clone(),
            Rc::clone(&prefs),
            html_prefs(&prefs.borrow()),
            Rc::clone(&dialogs),
        );
        // `a_Mime_init` + `a_Dicache_init` (viewers + factory image).
        doc::register_doc_viewers(&ctx);
        dillo_images::dicache::register_image_viewers();
        dillo_images::dicache::dicache_set_image_factory(Box::new(DocImageFactory::new(
            ctx.clone(),
        )));
        // Seams capi/cache — ≙ les globaux `a_UIcmd_*`/`a_Nav_*`/
        // `a_Bw_*`/`a_Dpiapi_*` résolus statiquement amont.
        cache::cache_set_seams(
            Box::new(DocBrowserOps::new(reg.clone(), queue.clone())),
            Box::new(DocUiOps::new(queue.clone())),
            Box::new(DocNavOps::new(reg.clone(), queue.clone())),
            Box::new(dillo_images::dicache::DicacheOpsImpl),
            Box::new(DocDispatch::new(ctx.clone())),
        );
        capi::capi_set_seams(
            Box::new(DocUiOps::new(queue.clone())),
            Box::new(DocNavOps::new(reg.clone(), queue.clone())),
            Box::new(DocDpiOps::new(queue.clone())),
        );
        let ctl = Rc::new(RefCell::new(Control::new()));
        chrome.set_control(Rc::clone(&ctl));
        Self {
            chrome,
            reg,
            prefs,
            uicmd,
            queue,
            doc_ctx: ctx,
            dialogs,
            ctl,
            #[cfg(unix)]
            ctl_listener: None,
            win_of: HashMap::new(),
            tab_of: HashMap::new(),
            t0: Instant::now(),
        }
    }

    /// `a_Control_init` — installe le listener du socket de
    /// contrôle (`~/.dillo/ctl/<pid>`, Unix seulement ≙
    /// `ENABLE_CONTROL_SOCKET` amont).
    #[cfg(unix)]
    pub fn set_ctl_listener(&mut self, l: dillo_net::control::unix::CtlListener) {
        self.ctl_listener = Some(l);
    }

    /// `tabs` chrome d'une fenêtre native.
    #[must_use]
    pub fn tabs_of_win(&self, win: usize) -> Option<usize> {
        self.tab_of.get(&win).copied()
    }

    /// `ui` actif de la fenêtre (≙ `wizard()->value()`).
    fn active_ui(&self, win: usize) -> Option<usize> {
        self.tabs_of_win(win)
            .and_then(|t| self.chrome.tabs(t))
            .and_then(super::ui::Tabs::value)
    }

    /// `ui_mut` de l'onglet actif de la fenêtre.
    fn active_ui_mut(&mut self, win: usize) -> Option<&mut Ui> {
        self.active_ui(win).and_then(|ui| self.chrome.ui_mut(ui))
    }

    /// `true` si `(x, y)` est dans le strip d'onglets de la
    /// fenêtre `tabs` (hauteur `ctab_h` — verbatim `Pack`/`Control`
    /// en haut du `CustTabs`).
    fn strip_hit(&mut self, win: usize, x: i32, y: i32, button: i32) -> Option<bool> {
        let tabs = self.tabs_of_win(win)?;
        let t = self.chrome.tabs(tabs)?;
        if y >= t.ctab_h {
            return Some(false);
        }
        // verbatim `close_tab_btn_cb` — gauche seulement.
        if t.close_at(x, y) {
            if button == MOUSE_LEFT {
                if let Some(ui) = t.value() {
                    if let Some(bw) = self.chrome.bw_by_ui(&self.reg, ui) {
                        self.uicmd.close_bw(&self.reg, &mut self.chrome, bw);
                    }
                }
            }
            return Some(true);
        }
        // verbatim `tab_btn_cb`.
        if let Some(i) = t.tab_at(x, y) {
            let tui = t.pack[i].ui;
            let right_closes = self.prefs.borrow().right_click_closes_tab;
            if button == MOUSE_LEFT {
                // `tabs->switch_tab(btn)`.
                self.chrome.switch_tab(tabs, i as i32);
            } else if (button == MOUSE_RIGHT && right_closes)
                || (button == MOUSE_MIDDLE && !right_closes)
            {
                if let Some(bw) = self.chrome.bw_by_ui(&self.reg, tui) {
                    self.uicmd.close_bw(&self.reg, &mut self.chrome, bw);
                }
            }
        }
        // Zone morte du strip : consommée (rien d'autre dessous).
        Some(true)
    }

    /// `FL_PUSH` — verbatim `UI::handle` (strip → chrome → contenu
    /// → boutons 4/5 back/forw).
    #[allow(clippy::too_many_arguments)]
    fn press(
        &mut self,
        app: &mut DwApp,
        win: usize,
        x: i32,
        y: i32,
        button: i32,
        clicks: i32,
        state: i32,
    ) -> bool {
        if let Some(hit) = self.strip_hit(win, x, y, button) {
            if hit {
                app.request_redraw(win);
            }
            return hit;
        }
        let Some(ui) = self.active_ui(win) else {
            return false;
        };
        let Some(bw) = self.chrome.bw_by_ui(&self.reg, ui) else {
            return false;
        };
        self.chrome.event_xy = (x, y); // ≙ Fl::event_x/y pour les popups
        let hit = self.chrome.ui(ui).map_or(Hit::None, |u| u.hit(x, y));
        match hit {
            Hit::Main => {
                // ≙ `FltkViewport::handle` — scrollbars puis layout.
                let Some(u) = self.active_ui_mut(win) else {
                    return false;
                };
                let (vx, vy) = (x - u.main.x, y - u.main.y);
                let view = Rc::clone(&u.view);
                if u.scroll.press(&view, vx as f32, vy as f32) {
                    app.request_redraw(win);
                    return true;
                }
                let (cx, cy) = {
                    let v = view.borrow();
                    (v.to_canvas_x(vx), v.to_canvas_y(vy))
                };
                if let Some(l) = self.chrome.layout_of(ui) {
                    l.borrow_mut().button_event(
                        ButtonEventType::ButtonPress,
                        clicks,
                        cx,
                        cy,
                        state,
                        button,
                    );
                }
                app.request_redraw(win);
                true
            }
            Hit::None => {
                // verbatim ui.cc:866-874 — boutons Back/Forward
                // souris hors widgets.
                if button == MOUSE_BACK {
                    let p = self.prefs.borrow();
                    self.uicmd.back(&self.reg, &mut self.chrome, &p, bw);
                    true
                } else if button == MOUSE_FORWARD {
                    let p = self.prefs.borrow();
                    self.uicmd.forw(&self.reg, &mut self.chrome, &p, bw);
                    true
                } else {
                    false
                }
            }
            _ => {
                // Dispatch verbatim `b1_cb`/`clear_cb`/`filemenu_cb`/…
                let platform = Rc::clone(self.chrome.platform_rc());
                let mut paste = move || {
                    let t = platform.borrow().clipboard_text();
                    if t.is_empty() {
                        None
                    } else {
                        String::from_utf8(t).ok()
                    }
                };
                let consumed = chrome_widget(
                    &self.uicmd,
                    &self.reg,
                    &mut self.chrome,
                    self.prefs.as_ref(),
                    bw,
                    hit,
                    button,
                    x,
                    y,
                    &mut paste,
                );
                if consumed {
                    app.request_redraw(win);
                }
                consumed
            }
        }
    }

    /// `FL_RELEASE` — verbatim `UI::handle` : scrollbars, contenu,
    /// puis middle-click paste (`Fl::paste(*Clear)` ≙ clipboard →
    /// location + open) quand rien n'a réclamé.
    fn release(
        &mut self,
        app: &mut DwApp,
        win: usize,
        x: i32,
        y: i32,
        button: i32,
        clicks: i32,
        state: i32,
    ) -> bool {
        let Some(ui) = self.active_ui(win) else {
            return false;
        };
        let Some(u) = self.active_ui_mut(win) else {
            return false;
        };
        if y < u.rect.y {
            return true; // strip (press déjà consommé)
        }
        let in_main = u.main.contains(x, y);
        let (vx, vy) = (x - u.main.x, y - u.main.y);
        let view = Rc::clone(&u.view);
        if u.scroll.release() {
            app.request_redraw(win);
            return true;
        }
        let mut claimed = false;
        if in_main {
            let (cx, cy) = {
                let v = view.borrow();
                (v.to_canvas_x(vx), v.to_canvas_y(vy))
            };
            if let Some(l) = self.chrome.layout_of(ui) {
                claimed = l.borrow_mut().button_event(
                    ButtonEventType::ButtonRelease,
                    clicks,
                    cx,
                    cy,
                    state,
                    button,
                );
            }
        }
        if !claimed && button == MOUSE_MIDDLE {
            // verbatim `UI::handle` FL_RELEASE : `paste_url` si
            // `middle_click_drags_page == 0` — `Fl::paste(*Clear)` ≙
            // clipboard dans Location + `location_cb`.
            let p = self.prefs.borrow();
            if !p.middle_click_drags_page {
                if let Some(bw) = self.chrome.bw_by_ui(&self.reg, ui) {
                    let platform = Rc::clone(self.chrome.platform_rc());
                    let text = platform.borrow().clipboard_text();
                    drop(p);
                    if !text.is_empty() {
                        if let Ok(t) = String::from_utf8(text) {
                            self.chrome.set_location(ui, &t);
                            let pr = self.prefs.borrow();
                            self.uicmd.open_urlstr(
                                &self.reg,
                                &mut self.chrome,
                                &pr,
                                bw,
                                t.as_bytes(),
                            );
                        }
                    }
                    return true;
                }
            }
        }
        if in_main {
            app.request_redraw(win);
        }
        claimed || in_main
    }

    /// `FL_MOVE`/`FL_DRAG` — drag scrollbar actif, sinon motion
    /// layout (coords translatées de `main`).
    fn move_(&mut self, app: &mut DwApp, win: usize, x: i32, y: i32, state: i32) -> bool {
        let Some(ui) = self.active_ui(win) else {
            return false;
        };
        let Some(u) = self.active_ui_mut(win) else {
            return false;
        };
        let (vx, vy) = (x - u.main.x, y - u.main.y);
        let view = Rc::clone(&u.view);
        if u.scroll.drag_move(&view, vx as f32, vy as f32) {
            app.request_redraw(win);
            return true;
        }
        if !u.main.contains(x, y) {
            return false;
        }
        let (cx, cy) = {
            let v = view.borrow();
            (v.to_canvas_x(vx), v.to_canvas_y(vy))
        };
        if let Some(l) = self.chrome.layout_of(ui) {
            l.borrow_mut().motion_notify(cx, cy, state);
        }
        true
    }

    /// `FL_MOUSEWHEEL` — verbatim `CustTabs::handle` :
    /// `scroll_switches_tabs` sur le strip, sinon scroll de la vue
    /// (≙ `Fl_Scrollbar::handle` : `value - e_dy * linesize`).
    fn wheel(&mut self, app: &mut DwApp, win: usize, dx: f32, dy: f32) -> bool {
        let Some(tabs) = self.tabs_of_win(win) else {
            return false;
        };
        let on_strip = {
            let t = self.chrome.tabs(tabs);
            let (mx, my) = app
                .win_view(win)
                .map_or((-1, -1), |v| v.borrow().mouse_pos());
            t.is_some_and(|t| my >= 0 && my < t.ctab_h && mx >= 0)
        };
        let (switch, reverse) = {
            let p = self.prefs.borrow();
            (p.scroll_switches_tabs, p.scroll_switches_tabs_reverse)
        };
        if on_strip && switch {
            // verbatim : dir = dy ? dy : dx ; >0 → next, <0 → prev.
            let mut dir = if dy == 0.0 { dx } else { dy };
            if reverse {
                dir = -dir;
            }
            if dir > 0.0 {
                self.chrome.next_tab(tabs);
            } else if dir < 0.0 {
                self.chrome.prev_tab(tabs);
            }
            app.request_redraw(win);
            return true;
        }
        let Some(ui) = self.active_ui(win) else {
            return false;
        };
        let Some(u) = self.chrome.ui(ui) else {
            return false;
        };
        let (x, y) = app
            .win_view(win)
            .map_or((-1, -1), |v| v.borrow().mouse_pos());
        if !u.main.contains(x, y) {
            return false;
        }
        // ≙ `Fl_Scrollbar::handle(FL_MOUSEWHEEL)` — pas de ligne.
        let view = Rc::clone(&u.view);
        let step = view.borrow().scroll_step();
        let (sx, sy) = view.borrow().scroll_pos();
        view.borrow_mut()
            .scroll_to(sx - (dx * step) as i32, sy - (dy * step) as i32);
        app.request_redraw(win);
        true
    }

    /// `FL_KEYBOARD`+`FL_SHORTCUT` — verbatim `UI::handle` /
    /// `CustTabs::handle` : input focalisé (findbar, location)
    /// d'abord, puis `Keys::getKeyCmd` → strip puis UI.
    fn key(&mut self, app: &mut DwApp, win: usize, ev: &KeyEvent, state: i32) -> bool {
        if ev.state != ElementState::Pressed {
            return false; // verbatim : seul le press agit
        }
        let Some(ui) = self.active_ui(win) else {
            return false;
        };
        let Some(bw) = self.chrome.bw_by_ui(&self.reg, ui) else {
            return false;
        };
        let key = winit_keysym(ev);
        let text = event_text(ev);
        let (shift, ctrl, alt, meta) = (
            state & 0x1 != 0,
            state & 0x4 != 0,
            state & 0x8 != 0,
            state & 0x20 != 0,
        );

        // verbatim : FL_KEYBOARD va au widget focalisé d'abord —
        // findbar (`MyInput`) puis location (`CustInput`).
        {
            let focused = self
                .chrome
                .ui(ui)
                .map(|u| (u.findbar_visible && u.findbar_state.input_focused, u.focus));
            if let Some((fb, focus)) = focused {
                if fb
                    && findbar_key(
                        &self.uicmd,
                        &self.reg,
                        &mut self.chrome,
                        bw,
                        key,
                        text.as_deref(),
                        shift,
                        ctrl,
                        alt,
                        meta,
                    )
                {
                    app.request_redraw(win);
                    return true;
                }
                if focus == Focus::Location {
                    let p = self.prefs.borrow();
                    if cust_input_key(
                        &self.uicmd,
                        &self.reg,
                        &mut self.chrome,
                        &p,
                        bw,
                        key,
                        text.as_deref(),
                        state,
                    ) {
                        app.request_redraw(win);
                        return true;
                    }
                }
            }
        }

        // `Keys::getKeyCmd()` verbatim (`key` = keysym si le texte
        // est un contrôle, sinon le codepoint texte).
        let cmd = self
            .chrome
            .keys
            .key_cmd(key, text.as_deref().map_or(&[], str::as_bytes), state);
        if cmd == KeysCommand::Nop || cmd == KeysCommand::Invalid {
            return false;
        }
        // verbatim `CustTabs::handle` (raccourcis d'onglets) avant
        // `UI::handle` — l'ordre amont : CustTabs reçoit le shortcut
        // en premier (il est le widget fenêtre le plus haut).
        let p = self.prefs.borrow();
        match cmd {
            KeysCommand::NewTab => {
                self.uicmd
                    .open_url_nt(&self.reg, &mut self.chrome, &p, bw, None, 1);
            }
            KeysCommand::CloseTab => {
                self.uicmd.close_bw(&self.reg, &mut self.chrome, bw);
            }
            KeysCommand::LeftTab => {
                if let Some(tabs) = self.tabs_of_win(win) {
                    self.chrome.prev_tab(tabs);
                }
            }
            KeysCommand::RightTab => {
                if let Some(tabs) = self.tabs_of_win(win) {
                    self.chrome.next_tab(tabs);
                }
            }
            KeysCommand::NewWindow => {
                self.uicmd
                    .open_url_nw(&self.reg, &mut self.chrome, &p, bw, None);
            }
            KeysCommand::CloseAll => {
                // verbatim `a_Timeout_add(0.0, a_UIcmd_close_all_bw)`.
                UiShell::defer_close_all(&mut self.chrome);
            }
            _ => {
                if !ui_shortcut(&self.uicmd, &self.reg, &mut self.chrome, &p, bw, cmd) {
                    return false;
                }
            }
        }
        app.request_redraw(win);
        true
    }

    /// `win_cb` verbatim (uicmd.cc:532) — dialog « Close window? »
    /// si >1 onglet, puis `close_bw` de chaque onglet.
    fn close(&mut self, win: usize) -> bool {
        let Some(tabs) = self.tabs_of_win(win) else {
            return false;
        };
        let uis: Vec<usize> = self
            .chrome
            .tabs(tabs)
            .map(|t| t.pack.iter().map(|b| b.ui).collect())
            .unwrap_or_default();
        let ntabs = uis.len();
        if ntabs == 0 {
            return false;
        }
        let mut choice = 1;
        if self.prefs.borrow().show_quit_dialog && ntabs > 1 {
            choice = self.chrome.dialog_choice(
                "Dillo: Close window?",
                "Window contains more than one tab.",
                &["Close", "Cancel"],
            );
        }
        if choice == 1 {
            // verbatim `while (ntabs-- > 0) close_bw(value())`.
            for ui in uis {
                // la valeur active change à chaque close — verbatim
                // utilise `wizard()->value()` ; `close_bw` retire le
                // bon onglet via `tab_ui_remove(ui)`.
                if let Some(bw) = self.chrome.bw_by_ui(&self.reg, ui) {
                    self.uicmd.close_bw(&self.reg, &mut self.chrome, bw);
                }
            }
        }
        true // consommé : la fenêtre meurt via `WinOp::Close`
    }

    /// `resize` verbatim — `Wizard->resize` cascade : rects `Ui` +
    /// rearrange + viewport + strip.
    fn resize(&mut self, win: usize, w: u32, h: u32) -> bool {
        if let Some(tabs) = self.tabs_of_win(win) {
            self.chrome
                .layout_win(tabs, w.max(1) as i32, h.max(1) as i32);
        }
        false // la fenêtre gère déjà le back-buffer
    }

    /// `Ui::handle` `FL_ENTER/LEAVE` → layout de l'onglet actif.
    fn crossing(&mut self, win: usize, state: i32, enter: bool) -> bool {
        let Some(ui) = self.active_ui(win) else {
            return false;
        };
        let Some(u) = self.chrome.ui(ui) else {
            return false;
        };
        let (x, y) = u.view.borrow().mouse_pos();
        let view = Rc::clone(&u.view);
        let (cx, cy) = {
            let v = view.borrow();
            (v.to_canvas_x(x), v.to_canvas_y(y))
        };
        if let Some(l) = self.chrome.layout_of(ui) {
            if enter {
                l.borrow_mut().enter_notify(cx, cy, state);
            } else {
                l.borrow_mut().leave_notify(state);
            }
        }
        true
    }

    /// Applique un [`WinOp`] sur le `DwApp` (effets de bord natifs
    /// ≙ `Fl_Window` amont).
    fn apply_win_op(&mut self, app: &mut DwApp, op: WinOp) {
        match op {
            WinOp::Open { tabs, w, h } => {
                let title = self.chrome.win_title(tabs).map_or("Dillo:", |t| {
                    if t.is_empty() {
                        "Dillo:"
                    } else {
                        t
                    }
                });
                let idx = app.open_window_attrs(
                    WindowAttributes::default()
                        .with_title(title.to_string())
                        .with_inner_size(LogicalSize::new(
                            f64::from(w.max(1)),
                            f64::from(h.max(1)),
                        )),
                );
                self.win_of.insert(tabs, idx);
                self.tab_of.insert(idx, tabs);
            }
            WinOp::Position { tabs, x, y } => {
                if let Some(&w) = self.win_of.get(&tabs) {
                    app.set_position(w, x, y);
                }
            }
            WinOp::Close { tabs } => {
                if let Some(w) = self.win_of.remove(&tabs) {
                    self.tab_of.remove(&w);
                    app.close_window(w);
                }
            }
            WinOp::SetTitle { tabs, title } => {
                if let Some(&w) = self.win_of.get(&tabs) {
                    app.set_title(w, &title);
                }
            }
            WinOp::RedrawAll => {
                for &w in self.win_of.values() {
                    app.request_redraw(w);
                }
            }
        }
    }
}

impl ShellGlue for Glue {
    /// `UI::handle`/`CustTabs::handle` verbatim — le chrome voit
    /// chaque event avant le viewport ; `true` = consommé.
    fn event(&mut self, app: &mut DwApp, ev: &ShellEv<'_>) -> bool {
        match *ev {
            ShellEv::Press {
                win,
                x,
                y,
                button,
                clicks,
                state,
            } => self.press(app, win, x, y, button, clicks, state),
            ShellEv::Release {
                win,
                x,
                y,
                button,
                clicks,
                state,
            } => self.release(app, win, x, y, button, clicks, state),
            ShellEv::Move { win, x, y, state } => self.move_(app, win, x, y, state),
            ShellEv::Enter { win, state } => self.crossing(win, state, true),
            ShellEv::Leave { win, state } => self.crossing(win, state, false),
            ShellEv::Wheel { win, dx, dy, .. } => self.wheel(app, win, dx, dy),
            ShellEv::Key { win, event, state } => self.key(app, win, event, state),
            ShellEv::Resize { win, w, h } => self.resize(win, w, h),
            ShellEv::Close { win } => self.close(win),
        }
    }

    /// `UI::draw` + blit viewport — compose dans le back-buffer de
    /// la fenêtre : strip + chrome de l'onglet actif + sa vue
    /// (rendue d'abord si damage ≙ `FltkViewport::draw`).
    fn draw(&mut self, app: &mut DwApp, win: usize) {
        let Some(tabs) = self.tabs_of_win(win) else {
            return;
        };
        // Render contenu de l'onglet actif (verbatim — damage).
        if let Some(ui) = self.active_ui(win) {
            let view = self.chrome.ui(ui).map(|u| Rc::clone(&u.view));
            if let Some(view) = view {
                let (full, mut rects) = view.borrow_mut().take_damage();
                if full {
                    let (w, h) = view.borrow().view_size();
                    rects = vec![dillo_dw::types::Rectangle {
                        x: 0,
                        y: 0,
                        width: w,
                        height: h,
                    }];
                }
                if !rects.is_empty() {
                    {
                        let mut v = view.borrow_mut();
                        for r in &rects {
                            v.fill_bg(r);
                        }
                        if let Some(l) = self.chrome.layout_of(ui) {
                            let mut lg = l.borrow_mut();
                            for r in &rects {
                                lg.draw(&mut *v, r);
                            }
                        }
                    }
                    // Scrollbars ≙ `Fl_Scrollbar` enfants — dessinées
                    // après le contenu dans le même back-buffer.
                    if let Some(u) = self.chrome.ui(ui) {
                        u.scroll.draw(&view);
                    }
                }
            }
        }
        // Composition dans le pixmap fenêtre.
        if let Some(view) = app.win_view(win) {
            view.borrow_mut()
                .with_pixmap(|px| self.chrome.draw_window(tabs, px));
        }
    }

    /// Drain des différés (verbatim `a_Timeout_add(0)` + ops
    /// `Fl_Window`) + pompe réseau/contrôle (verbatim `a_IOwatch` et
    /// `a_Timeout_add` de `main()` amont — ici drainés à chaque tick
    /// de la boucle winit, cf. `WakeUp`/`wait_until` côté `DwApp`).
    fn tick(&mut self, app: &mut DwApp) {
        // Réseau : timers, DNS/HTTP/IO (`a_Timeout_add(0.01)` + idles
        // de `dillo.cc`), puis les opérations que les callbacks ont
        // différées (a_UIcmd_*, nav_*).
        timeout::poll_timeouts();
        let _ = capi::capi_poll();
        self.pump_control();
        self.drain_queue();
        while let Some(op) = self.chrome.app_ops.pop_front() {
            self.apply_win_op(app, op);
        }
        // Nav différée (≙ callbacks `a_Timeout_add(0.0)` de nav.c).
        for (bw, op) in self.chrome.drain_nav() {
            let p = self.prefs.borrow();
            self.uicmd
                .nav_op_now(&self.reg, &mut self.chrome, &p, bw, op);
        }
        if self.chrome.take_close_all() {
            let p = self.prefs.borrow();
            self.uicmd
                .close_all_bw(&self.reg, &mut self.chrome, &p, false);
        }
        // `TipWin` (verbatim `tipwin.c` timeout FLTK) — minuteur de
        // présentation des tooltips du chrome ; `mouse_root` =
        // position fenêtre approximée par la vue (seam documentée).
        let now = self.t0.elapsed().as_secs_f64();
        let _ = self.chrome.tipwin.tick(now, (0, 0), i32::MAX);
    }

    /// `Fl::wait() > 0` — la boucle vit tant qu'une fenêtre chrome
    /// reste vivante (verbatim `num tabgroups`).
    fn wants_exit(&mut self) -> bool {
        self.chrome.win_count() == 0
    }

    /// Réveil à la prochaine échéance (≙ timeout `Fl::wait(0.01)`
    /// amont : réveil rapide (≤ 10 ms) tant que du trafic réseau est
    /// en cours, sinon échéance du prochain timer / wait infini.
    fn wake_after(&self) -> Option<std::time::Duration> {
        use std::time::Duration;
        let busy = Duration::from_millis(10);
        let due = timeout::next_due();
        if capi::capi_conns_active() > 0 {
            Some(due.map_or(busy, |d| d.min(busy)))
        } else {
            due
        }
    }
}

/// Seam de contrôle (verbatim `control.c` : résolution `bw=`,
/// `open_url`, `close_window`, `shutdown`, `send`/`inject`).
impl CtlOps for Glue {
    /// `find_bw_cb` verbatim — première `BrowserWindow` vivante.
    fn first_active_bw(&mut self) -> Option<u32> {
        self.reg.first_id()
    }

    /// `a_UIcmd_reload_all_active` (verbatim).
    fn reload_all(&mut self) {
        let p = self.prefs.borrow();
        self.uicmd
            .reload_all_active(&self.reg, &mut self.chrome, &p);
    }

    /// `a_UIcmd_has_finished` (verbatim).
    fn has_finished(&mut self, bw: u32) -> bool {
        UiCmd::has_finished(&self.reg, bw)
    }

    /// `a_UIcmd_open_urlstr` (verbatim — l'URL str est parsée là-bas).
    fn open_urlstr(&mut self, bw: u32, u: &str) {
        let p = self.prefs.borrow();
        self.uicmd
            .open_urlstr(&self.reg, &mut self.chrome, &p, bw, u.as_bytes());
    }

    /// `a_UIcmd_get_location_text` (verbatim).
    fn location_text(&mut self, bw: u32) -> String {
        self.uicmd
            .get_location_text(&self.reg, &mut self.chrome, bw)
    }

    /// `a_UIcmd_get_page_title` (verbatim).
    fn page_title(&mut self, bw: u32) -> Option<String> {
        self.uicmd.get_page_title(&self.reg, &mut self.chrome, bw)
    }

    /// `a_UIcmd_set_msg` (verbatim).
    fn set_msg(&mut self, bw: u32, msg: &str) {
        self.uicmd.set_msg(&self.reg, &mut self.chrome, bw, msg);
    }

    /// `a_Capi_get_buf` (verbatim).
    fn get_buf(&mut self, url_str: &str) -> Option<Vec<u8>> {
        let url = url_new(url_str.as_bytes(), None)?;
        capi::capi_get_buf(&url)
    }

    /// `a_Cache_get_header` (verbatim).
    fn get_header(&mut self, url_str: &str) -> Option<Vec<u8>> {
        let url = url_new(url_str.as_bytes(), None)?;
        cache::cache_get_header(&url)
    }

    /// `a_Cache_entry_inject` (verbatim — `true` si l'URL est valide).
    fn entry_inject(&mut self, url_str: &str, body: &[u8], flags: u32) -> bool {
        let Some(url) = url_new(url_str.as_bytes(), None) else {
            return false;
        };
        cache::cache_entry_inject(&url, body, flags);
        true
    }

    /// `a_UIcmd_repush` (verbatim).
    fn repush(&mut self, bw: u32) {
        let p = self.prefs.borrow();
        self.uicmd.repush(&self.reg, &mut self.chrome, &p, bw);
    }

    /// `a_UIcmd_close_all_bw((void*)1)` (verbatim) + fin de `main()`
    /// amont : purge cache + timeouts.
    fn close_all(&mut self) {
        let p = self.prefs.borrow();
        self.uicmd
            .close_all_bw(&self.reg, &mut self.chrome, &p, true);
        cache::cache_freeall();
        timeout::timeout_freeall();
    }

    /// `a_UIcmd_by_name` (verbatim — `0` = succès).
    fn by_name(&mut self, bw: u32, name: &str) -> i32 {
        let p = self.prefs.borrow();
        self.uicmd
            .by_name(&self.reg, &mut self.chrome, &p, bw, name)
    }

    /// `getpid` (verbatim).
    fn pid(&mut self) -> i64 {
        i64::from(std::process::id())
    }
}

impl Glue {
    /// Nom du fichier de prefs attendu par `run` (≙ `RC_PREFS`).
    #[must_use]
    pub fn rcfile(&self) -> String {
        String::from(crate::paths::RC_PREFS)
    }

    /// Accès registre (contrôle « from version » du doc-client).
    #[must_use]
    pub fn registry(&self) -> &Rc<BwRegistry> {
        &self.reg
    }

    /// Accepte les connexions du socket de contrôle + poll des
    /// requêtes (≙ `a_IOwatch` sur `Control_listen_fd` et les fds
    /// de requêtes amont).
    fn pump_control(&mut self) {
        #[cfg(unix)]
        if let Some(l) = &self.ctl_listener {
            // `Control_listen_cb` verbatim — accept non-bloquant,
            // une requête `Control` par socket.
            loop {
                match l.accept() {
                    Ok(Some(io)) => {
                        self.ctl.borrow_mut().accept(Box::new(io));
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
        let ctl = Rc::clone(&self.ctl);
        let mut delay = None;
        if let Ok(mut c) = ctl.try_borrow_mut() {
            c.poll(self);
            delay = c.take_wait_timeout();
        }
        // `a_Timeout_add(timeout, handle_wait_timeout)` verbatim.
        if let Some(d) = delay {
            let ctl2 = Rc::clone(&ctl);
            timeout::timeout_add(d, move || {
                if let Ok(mut c) = ctl2.try_borrow_mut() {
                    c.on_wait_timeout();
                }
            });
        }
    }

    /// Joue les ops navigateur différées par les seams réseau
    /// (≙ les `a_UIcmd_*` synchrones amont — file ≙ `a_Timeout_add(0)`).
    fn drain_queue(&mut self) {
        let ops: Vec<GlueOp> = self.queue.borrow_mut().drain(..).collect();
        for op in ops {
            self.apply_op(op);
        }
    }

    /// Dispatch d'une [`GlueOp`] vers `UiCmd`/`Chrome` (mapping
    /// verbatim des `a_*` amont).
    fn apply_op(&mut self, op: GlueOp) {
        let p = self.prefs.borrow();
        let reg = &self.reg;
        let shell = &mut self.chrome;
        match op {
            GlueOp::SetMsg { bw, msg } => self.uicmd.set_msg(reg, shell, bw, &msg),
            GlueOp::SetPageTitle { bw, title } => {
                self.uicmd.set_page_title(reg, shell, bw, title.as_deref());
            }
            GlueOp::SetBugProg { bw, n } => self.uicmd.set_bug_prog(reg, shell, bw, n),
            GlueOp::SetButtonsSens { bw } => UiCmd::set_buttons_sens(reg, shell, bw),
            GlueOp::SetLocationText { bw, text } => {
                self.uicmd.set_location_text(reg, shell, bw, &text);
            }
            GlueOp::SetPageProg { bw, bytes, cmd } => {
                self.uicmd.set_page_prog(reg, shell, bw, bytes, cmd);
            }
            GlueOp::SetImgProg {
                bw,
                got,
                total,
                cmd,
            } => {
                self.uicmd.set_img_prog(reg, shell, bw, got, total, cmd);
            }
            GlueOp::FinishLoading { bw } => UiCmd::finish_loading(reg, shell, bw),
            GlueOp::OpenUrl { bw, url } => {
                self.uicmd.open_url(reg, shell, &p, bw, Some(&url));
            }
            GlueOp::OpenUrlStr { bw, url } => {
                self.uicmd.open_urlstr(reg, shell, &p, bw, url.as_bytes());
            }
            GlueOp::OpenUrlNw { bw, url } => {
                self.uicmd.open_url_nw(reg, shell, &p, bw, Some(&url));
            }
            GlueOp::OpenUrlNt { bw, url, focus } => {
                self.uicmd
                    .open_url_nt(reg, shell, &p, bw, Some(&url), i32::from(focus));
            }
            GlueOp::Redirection0 { bw, url } => {
                self.uicmd.redirection0(reg, shell, &p, bw, &url);
            }
            GlueOp::Repush { bw } => self.uicmd.repush(reg, shell, &p, bw),
            GlueOp::PagePopup {
                bw,
                has_bugs,
                css_urls,
            } => {
                self.uicmd.page_popup(reg, shell, bw, has_bugs, &css_urls);
            }
            GlueOp::LinkPopup { bw, url, page_url } => {
                self.uicmd.link_popup(shell, bw, &url, page_url.as_ref());
            }
            GlueOp::ImagePopup {
                bw,
                url,
                loaded,
                page_url,
                link,
            } => {
                self.uicmd
                    .image_popup(shell, bw, &url, loaded, Some(&page_url), link.as_ref());
            }
            GlueOp::FormPopup {
                bw,
                page_url,
                form,
                showing_hiddens,
            } => {
                self.uicmd
                    .form_popup(shell, bw, &page_url, form, showing_hiddens);
            }
            GlueOp::BugmeterPopup { bw, .. } => {
                // `a_UIcmd_bugmeter_popup` verbatim (l'URL page est
                // recalculée depuis l'historique comme amont).
                self.uicmd.bugmeter_popup(reg, shell, bw);
            }
            GlueOp::SaveLink { bw, url, filename } => {
                self.uicmd.save_link(reg, shell, bw, &url, filename);
            }
            GlueOp::Stop { bw } => self.uicmd.stop(reg, shell, &p, bw),
            GlueOp::NavPush { bw, url, requester } => {
                if let Some(b) = reg.get(bw) {
                    let mut seams = crate::uicmd::NavSeams {
                        reg,
                        shell,
                        prefs: &p,
                        uicmd: &self.uicmd,
                    };
                    b.nav.borrow_mut().push(bw, &url, requester, &mut seams);
                }
            }
            GlueOp::NavReload { bw } => self.uicmd.reload(reg, shell, &p, bw),
            GlueOp::NavCancelExpectIfEq { bw, url } => {
                if let Some(b) = reg.get(bw) {
                    let mut seams = crate::uicmd::NavSeams {
                        reg,
                        shell,
                        prefs: &p,
                        uicmd: &self.uicmd,
                    };
                    b.nav.borrow_mut().cancel_expect_if_eq(bw, &url, &mut seams);
                }
            }
            GlueOp::ExpectDone { bw } => self.uicmd.expect_done(reg, shell, &p, bw),
            GlueOp::DpiDialog { bw, server, msg } => {
                // `a_Dpiapi_dialog` ≙ `a_Dialog_msg` amont.
                let _ = bw;
                shell.dialog_msg("Dpi", &format!("{server}: {msg}"));
            }
            GlueOp::ChatAdd { msg } => {
                self.chrome.bookmarks.chat_add(None, Some(&msg), None);
            }
            GlueOp::ReloadAll => self.uicmd.reload_all_active(reg, shell, &p),
            GlueOp::CloseAll => self.uicmd.close_all_bw(reg, shell, &p, true),
            GlueOp::ByName { bw, name } => {
                let _ = self.uicmd.by_name(reg, shell, &p, bw, &name);
            }
        }
    }
}
