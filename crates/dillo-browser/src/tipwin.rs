//! Port of `src/tipwin.cc` + `tipwin.hh` — tooltip custom partagé.
//!
//! `TipWin` ≙ `Fl_Menu_Window` borderless 1×1 autosizé : buffer 256
//! bytes (`tip[256]`, snprintf truncate), bgcolor =
//! `fl_color_cube(FL_NUM_RED-1, FL_NUM_GREEN-1, FL_NUM_BLUE-2)`
//! (jaune tooltip `#ffffbf`), délais verbatim 0.8s show / 0.2s recent
//! / 0.8s expiration du flag recent.
//!
//! Les widgets amont (`TipWinButton`, `TipWinInput`, `CustButton`)
//! sont des IDs de [`crate::ui::WItem`] — leurs `handle` deviennent
//! [`TipWin::on_enter`]/[`TipWin::on_hide`]/[`TipWin::on_key`].
//! `CustButton` (highlight hover) est modélisé par
//! [`CustButtonState`].
//!
//! L'ordonnancement (`Fl::add_timeout`/`remove_timeout`) est une
//! **horloge externe** : l'app pousse `now` à [`TipWin::tick`] (≙ le
//! timer FLTK) — pas de thread, verbatim single-thread.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::unreadable_literal)]
#![forbid(unsafe_code)]

/// `char tip[256]` — snprintf(tip, sizeof(tip)-1) → 255 bytes max.
const TIP_MAX: usize = 255;

/// `Fl::add_timeout(0.8, show_timeout)` — délai normal.
pub const SHOW_DELAY: f64 = 0.8;
/// `recent ? 0.2 : 0.8` — délai si tooltip récent.
pub const RECENT_DELAY: f64 = 0.2;
/// `Fl::add_timeout(0.8, recent_timeout)` — fenêtre "récent".
pub const RECENT_TIMEOUT: f64 = 0.8;

/// État de visibilité ≙ `shown()` + timers en vol.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TipState {
    /// `hide()`.
    Hidden,
    /// `show_timeout` en vol (deadline absolue).
    Pending(f64),
    /// Affiché (`position + show`).
    Shown,
}

/// `TipWin` — fenêtre tooltip partagée (`my_tipwin` ≙ instance
/// unique du [`crate::ui::Chrome`]).
#[derive(Debug)]
pub struct TipWin {
    /// `tip[256]` (troncature UTF-8-safe ≙ snprintf byte-truncate ;
    /// amont peut couper un multibyte — notre choix : clamp UTF-8,
    /// divergence documentée ADR-0002).
    pub tip: String,
    /// `bgcolor` — RGBA du cube FLTK `(4,7,3)` ≙ `#ffffbf`.
    pub bgcolor: (u8, u8, u8),
    /// `recent` — tooltip masqué il y a < 0.8s.
    pub recent: bool,
    /// `cur_widget` — widget demandeur (`WId` ≙ `void *wid`).
    pub cur_widget: Option<usize>,
    /// État + deadline.
    pub state: TipState,
    /// `recent_timeout` en vol (deadline).
    recent_deadline: Option<f64>,
    /// Taille calculée par `value()` (`fl_measure` + 8px marges).
    pub size: (i32, i32),
    /// Position posée par `show_timeout` (`x_root`, `ty`).
    pub pos: (i32, i32),
    /// `prefs.show_ui_tooltip` (lu à `do_show` — verbatim).
    pub enabled: bool,
}

impl TipWin {
    /// `TipWin::TipWin` (verbatim) — `set_override`, `recent=0`,
    /// `tip=""`, `bgcolor = cube(4,7,3)`.
    #[must_use]
    pub fn new(show_ui_tooltip: bool) -> Self {
        TipWin {
            tip: String::new(),
            bgcolor: color_cube_rgb(4, 7, 3),
            recent: false,
            cur_widget: None,
            state: TipState::Hidden,
            recent_deadline: None,
            size: (0, 0),
            pos: (0, 0),
            enabled: show_ui_tooltip,
        }
    }

    /// `TipWin::value` (verbatim) — `snprintf(tip, 255, s)` + mesure :
    /// `fl_measure` ≙ `measure` injectée (texte → (w,h)),
    /// `size(W+8, H+8)`, `redraw` ≙ retour `true` = redessiner.
    pub fn value(&mut self, s: &str, measure: impl FnOnce(&str) -> (i32, i32)) -> bool {
        // verbatim : snprintf(tip, sizeof(tip)-1) — byte truncate.
        let b = s.as_bytes();
        let n = b.len().min(TIP_MAX);
        let mut end = n;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        self.tip = s[..end].to_string();
        let (mut w, mut h) = measure(&self.tip);
        w += 8;
        h += 8;
        self.size = (w, h);
        true
    }

    /// `TipWin::do_show` (verbatim) — track widget + timeout
    /// `recent ? 0.2 : 0.8` si `prefs.show_ui_tooltip`.
    pub fn do_show(&mut self, wid: usize, now: f64) {
        self.cur_widget = Some(wid);
        if self.enabled {
            let delay = if self.recent {
                RECENT_DELAY
            } else {
                SHOW_DELAY
            };
            self.state = TipState::Pending(now + delay);
        }
    }

    /// `TipWin::do_hide` (verbatim) — `remove_timeout(show)` ;
    /// si `shown()` → hide + `recent=1` + `recent_timeout(0.8)`.
    pub fn do_hide(&mut self, now: f64) {
        if matches!(self.state, TipState::Shown) {
            self.state = TipState::Hidden;
            self.recent = true;
            self.recent_deadline = Some(now + RECENT_TIMEOUT);
        } else {
            self.state = TipState::Hidden; // remove_timeout(show_timeout)
        }
    }

    /// `TipWin::cancel` (verbatim `tipwin.hh` inline) — si `wid` est
    /// le demandeur → `cur_widget=NULL` + `do_hide`.
    pub fn cancel(&mut self, wid: usize, now: f64) {
        if self.cur_widget == Some(wid) {
            self.cur_widget = None;
            self.do_hide(now);
        }
    }

    /// `TipWin::recent_tooltip(val)`.
    pub fn recent_tooltip(&mut self, val: bool) {
        self.recent = val;
    }

    /// `show_timeout` + `recent_timeout` (verbatim) — l'app pousse
    /// `now` ; si le show arrive à échéance, calcule la position
    /// (`event_x_root`, `event_y_root+20`, flip au-dessus si hors
    /// écran, clamp `ty>=0`) et passe `Shown`.
    ///
    /// Retourne `true` ≙ le tooltip vient d'apparaître (redessiner).
    pub fn tick(&mut self, now: f64, mouse_root: (i32, i32), screen_h: i32) -> bool {
        if let Some(d) = self.recent_deadline {
            if now >= d {
                self.recent_deadline = None;
                self.recent_tooltip(false);
            }
        }
        if let TipState::Pending(d) = self.state {
            if now >= d {
                // verbatim show_timeout : ty = y_root+20, flip si
                // ty + h() > scr_h → y_root-20-h(), clamp 0.
                let mut ty = mouse_root.1 + 20;
                if ty + self.size.1 > screen_h {
                    ty = mouse_root.1 - 20 - self.size.1;
                }
                if ty < 0 {
                    ty = 0;
                }
                self.pos = (mouse_root.0, ty);
                self.state = TipState::Shown;
                self.recent_tooltip(false);
                return true;
            }
        }
        false
    }

    /// `TipWinButton::handle`/`TipWinInput::handle` — `FL_ENTER`
    /// ≙ `value(tooltip)` + `do_show(wid)`. Retourne `redraw`.
    pub fn on_enter(
        &mut self,
        wid: usize,
        tooltip: &str,
        now: f64,
        measure: impl FnOnce(&str) -> (i32, i32),
    ) -> bool {
        self.value(tooltip, measure);
        self.do_show(wid, now);
        true
    }

    /// `FL_PUSH`/`FL_RELEASE`/`FL_HIDE`/`FL_LEAVE` ≙ `do_hide` —
    /// commun aux deux widgets amont.
    pub fn on_hide(&mut self, _wid: usize, now: f64) {
        self.do_hide(now);
    }

    /// `TipWinInput` ajoute `FL_KEYBOARD` → `do_hide`.
    pub fn on_key(&mut self, _wid: usize, now: f64) {
        self.do_hide(now);
    }
}

/// `fl_color_cube(r,g,b)` → RGB — verbatim FLTK : cube `5×8×5`,
/// `r*255/4`, `g*255/7`, `b*255/4` (≙ `fl_cmap` entries).
#[must_use]
pub const fn color_cube_rgb(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    (
        (r as u32 * 255 / 4) as u8,
        (g as u32 * 255 / 7) as u8,
        (b as u32 * 255 / 4) as u8,
    )
}

/// `CustButton` verbatim — `norm_color`/`light_color` + highlight
/// hover (`FL_ENTER` → `light_color`, `LEAVE|RELEASE|HIDE` → norm,
/// `DEACTIVATE` quand color != norm → norm).
#[derive(Clone, Copy, Debug)]
pub struct CustButtonState {
    /// `norm_color = color()` (couleur du bouton à la création).
    pub norm: u32,
    /// `light_color = PREFS_UI_BUTTON_HIGHLIGHT_COLOR`.
    pub light: u32,
    /// `active()` (verbatim — inactive ⇒ pas de highlight).
    pub active: bool,
    /// Couleur courante effective.
    pub current: u32,
}

impl CustButtonState {
    /// `CustButton::CustButton` — `norm=color()`, `light=PREFS_...`.
    /// `highlight` est une couleur 0xRRGGBB (`-1` amont = palette
    /// FLTK — notre modèle stocke des RGBA indices `Color`).
    #[must_use]
    pub fn new(norm: u32, light: u32) -> Self {
        CustButtonState {
            norm,
            light,
            active: true,
            current: norm,
        }
    }

    /// `CustButton::handle` (verbatim) — `ev` ≙ FL event :
    /// `Enter`/`Leave`/`Release`/`Hide`/`Deactivate`.
    ///
    /// Retourne `true` ≙ `redraw()` amont.
    pub fn event(&mut self, ev: CustEvent) -> bool {
        if self.active {
            match ev {
                CustEvent::Enter => {
                    self.current = self.light;
                    return true;
                }
                CustEvent::Leave | CustEvent::Release | CustEvent::Hide => {
                    self.current = self.norm;
                    return true;
                }
                CustEvent::Deactivate => {}
            }
        } else if matches!(ev, CustEvent::Deactivate) && self.current != self.norm {
            self.current = self.norm;
            return true;
        }
        false
    }

    /// `hl_color(col)` verbatim.
    pub fn hl_color(&mut self, col: u32) {
        self.light = col;
    }
}

/// FL events utilisés par `CustButton::handle` (verbatim subset).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustEvent {
    /// `FL_ENTER`.
    Enter,
    /// `FL_LEAVE`.
    Leave,
    /// `FL_RELEASE`.
    Release,
    /// `FL_HIDE`.
    Hide,
    /// `FL_DEACTIVATE`.
    Deactivate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tipwin_bgcolor_is_tooltip_yellow() {
        // fl_color_cube(4,7,3) → (255,255,191) ≙ #ffffbf
        assert_eq!(color_cube_rgb(4, 7, 3), (255, 255, 191));
    }

    #[test]
    fn value_truncates_and_measures() {
        let mut t = TipWin::new(true);
        let long = "x".repeat(400);
        t.value(&long, |s| (s.len() as i32, 14));
        assert_eq!(t.tip.len(), 255); // snprintf(tip,255) verbatim
        assert_eq!(t.size, (255 + 8, 14 + 8)); // +8 marges verbatim
    }

    #[test]
    fn show_hide_timing_verbatim() {
        let mut t = TipWin::new(true);
        // do_show → pending à now+0.8
        t.do_show(7, 0.0);
        assert_eq!(t.state, TipState::Pending(0.8));
        assert_eq!(t.cur_widget, Some(7));
        // tick avant deadline → rien
        assert!(!t.tick(0.5, (100, 100), 600));
        // à deadline → shown, position = mouse +20 y
        assert!(t.tick(0.8, (100, 100), 600));
        assert_eq!(t.state, TipState::Shown);
        assert_eq!(t.pos, (100, 120));
        // hide → recent=1, deadline +0.8
        t.do_hide(1.0);
        assert!(t.recent);
        assert_eq!(t.state, TipState::Hidden);
        // recent → show à +0.2 (verbatim)
        t.do_show(9, 1.1);
        assert_eq!(t.state, TipState::Pending(1.3));
        // recent_timeout expire le flag
        t.tick(1.85, (0, 0), 600);
        assert!(!t.recent);
    }

    #[test]
    fn position_flips_offscreen() {
        let mut t = TipWin::new(true);
        t.size = (50, 30);
        t.state = TipState::Pending(0.0);
        // ty = y+20 ; ty+h > scr_h → au-dessus (y-20-h)
        t.tick(0.0, (10, 590), 600);
        assert_eq!(t.pos, (10, 590 - 20 - 30));
    }

    #[test]
    fn cancel_only_own_widget() {
        let mut t = TipWin::new(true);
        t.do_show(5, 0.0);
        t.tick(1.0, (0, 0), 600);
        assert_eq!(t.state, TipState::Shown);
        t.cancel(6, 2.0); // autre widget → rien
        assert_eq!(t.state, TipState::Shown);
        t.cancel(5, 2.0);
        assert_eq!(t.state, TipState::Hidden);
        assert_eq!(t.cur_widget, None);
    }

    #[test]
    fn disabled_pref_never_shows() {
        let mut t = TipWin::new(false); // show_ui_tooltip=false
        t.do_show(1, 0.0);
        assert_eq!(t.state, TipState::Hidden); // pas de pending
        assert_eq!(t.cur_widget, Some(1)); // widget tracké quand même
    }

    #[test]
    fn cust_button_highlight_verbatim() {
        let mut b = CustButtonState::new(0xbbbbbb, 0xff0000);
        assert_eq!(b.current, 0xbbbbbb);
        b.event(CustEvent::Enter);
        assert_eq!(b.current, 0xff0000); // light_color
        b.event(CustEvent::Leave);
        assert_eq!(b.current, 0xbbbbbb); // restore
        b.event(CustEvent::Enter);
        b.event(CustEvent::Release);
        assert_eq!(b.current, 0xbbbbbb);
        // inactif : pas de highlight ; DEACTIVATE restaure
        b.active = false;
        b.event(CustEvent::Enter);
        assert_eq!(b.current, 0xbbbbbb);
        b.current = 0xff0000;
        b.event(CustEvent::Deactivate);
        assert_eq!(b.current, 0xbbbbbb);
    }
}
