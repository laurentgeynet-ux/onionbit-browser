//! Port of `src/ui.cc` + `src/ui.hh` (+ `CustTabs`/`CustTabButton`/
//! `CustInput`/`CustPasteButton`/`CustProgressBox`/`CustGroupHorizontal`/
//! `CustGroupVertical` de `src/uicmd.cc`).
//!
//! Le chrome Dillo est un jeu de widgets FLTK ; le port les remplace
//! par des widgets logiques auto-dessinés (ADR-0002) rendus en
//! `tiny-skia`/`cosmic-text` sur le même `Pixmap` fenêtre que le
//! contenu. La géométrie (`rearrange` des groupes, tailles de panel
//! par `PanelSize`, `ctab_h`, troncatures de labels) est **verbatim**.
//!
//! Correspondances :
//! - `CustGroupHorizontal`/`CustGroupVertical::rearrange` →
//!   [`Group::rearrange`] / [`Ui::rearrange`]
//! - `CustInput`/`TipWinInput` → [`LineInput`]
//! - `CustProgressBox` → [`WId::PageProg`]/[`WId::ImgProg`]
//! - `CustTabs`/`CustTabButton`/`Fl_Wizard`/`Fl_Pack`/`Fl_Scroll`
//!   → [`Tabs`]/[`TabBtn`]
//! - `UI::handle` (FL_SHORTCUT) → [`ui_shortcut`]
//! - `UI` (par onglet) → [`Ui`] ; l'ensemble des fenêtres et l'impl
//!   [`UiShell`] → [`Chrome`].
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::if_not_else)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::unused_self)]
#![allow(clippy::fn_params_excessive_bools)]
#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use dillo_common::utf8::end_of_char;
use dillo_dw::layout::Layout;
use dillo_dw::platform::Platform;
use dillo_dw::view::View;
use dillo_net::url::DilloUrl;
use dillo_platform::platform::SkiaPlatform;
use dillo_platform::view::{SharedView, SkiaView};

use crate::actions::{actions_init, Action};
use crate::bw::BwRegistry;
use crate::dialog::{AutoCancelRunner, DialogAnswer, DialogRunner, DialogState};
use crate::findbar::{FindHit, Findbar, FindbarLayout};
use crate::keys::{Keys, KeysCommand};
use crate::menu::{self, DocOps, MenuCtx, NullDocOps};
use crate::nav::{NavOp, NavSched};
use crate::prefs::{PanelSize, Prefs};
use crate::tipwin::TipWin;
use crate::uicmd::{UiButton, UiCmd, UiShell};

// -- constantes verbatim ----------------------------------------------------

/// `UI_MIN_W`/`UI_MIN_H` (`ui.hh`) — taille de construction confortable.
pub const UI_MIN_W: i32 = 600;
/// `UI_MIN_H`.
pub const UI_MIN_H: i32 = 200;
/// `DEFAULT_TAB_LABEL` (`uicmd.cc`).
pub const DEFAULT_TAB_LABEL: &str = "-.new.-";
/// `SCROLLBAR_THICKNESS` de la plateforme (ré-export sémantique).
pub use dillo_platform::view::SCROLLBAR_THICKNESS;

/// Fond FLTK par défaut quand `ui_main_bg_color == -1`
/// (`Fl::get_system_colors` : `#C0C0C0` X11, `#F0F0F0` Windows —
/// verbatim par plateforme).
#[cfg(windows)]
pub const FL_BG_DEFAULT: i32 = 0x00f0_f0f0;
/// `FL_BG_DEFAULT` hors Windows (`#C0C0C0`).
#[cfg(not(windows))]
pub const FL_BG_DEFAULT: i32 = 0x00c0_c0c0;

/// `UIPanelmode` (`ui.hh`) — `Normal` est compatible `false`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum UiPanelMode {
    /// `UI_NORMAL`.
    #[default]
    Normal = 0,
    /// `UI_HIDDEN`.
    Hidden = 1,
}

// -- XPM --------------------------------------------------------------------

/// Icône décodée (RGBA non prémultiplié, `None` → `a = 0`).
#[derive(Clone)]
pub struct Icon {
    /// Largeur.
    pub w: u32,
    /// Hauteur.
    pub h: u32,
    /// Pixels `0xAARRGGBB`.
    pub data: Vec<u32>,
}

/// `#RGB`/`#RRGGBB`/`#RRRGGGBBB`/`#RRRRGGGGBBBB` → `0xRRGGBB`
/// (XPM : 1–4 chiffres hex par canal ; troncation au top-byte
/// ≙ FLTK `Fl_Pixmap`).
fn xpm_rgb(hex: &str) -> Option<u32> {
    if hex.is_empty() || hex.len() % 3 != 0 || hex.len() > 12 {
        return None;
    }
    let n = hex.len() / 3; // chiffres par canal (1..=4)
    let mut c = [0u32; 3];
    for (i, ch) in c.iter_mut().enumerate() {
        let v = u32::from_str_radix(&hex[i * n..(i + 1) * n], 16).ok()?;
        // n≥2 → top byte (troncation) ; n=1 → digit ×17.
        *ch = if n == 1 { v * 17 } else { v >> (4 * (n - 2)) };
    }
    Some((c[0] << 16) | (c[1] << 8) | c[2])
}

/// Décodeur XPM (`static const char *const *_xpm[]`) — header
/// `w h ncolors cpp`, table `c c #rrggbb`/`None`, puis pixels.
#[must_use]
pub fn xpm_decode(lines: &[&str]) -> Option<Icon> {
    let hdr: Vec<&str> = lines.first()?.split_whitespace().collect();
    if hdr.len() < 4 {
        return None;
    }
    let (w, h, ncol, cpp) = (
        hdr[0].parse::<u32>().ok()?,
        hdr[1].parse::<u32>().ok()?,
        hdr[2].parse::<usize>().ok()?,
        hdr[3].parse::<usize>().ok()?,
    );
    let mut cmap: HashMap<&str, u32> = HashMap::new();
    for l in lines.iter().skip(1).take(ncol) {
        if l.len() < cpp {
            return None;
        }
        let (key, rest) = l.split_at(cpp);
        // " c #RRGGBB" ou " c None" (le `c` peut être `s`/`m` — on
        // prend la définition `c` si présente, sinon la ligne telle
        // quelle comme amont lit `c`).
        let toks: Vec<&str> = rest.split_whitespace().collect();
        let mut val = 0x0000_0000u32; // `None` → transparent
        let mut i = 0;
        while i + 1 < toks.len() {
            if toks[i] == "c" {
                let t = toks[i + 1];
                let hex = t.trim_start_matches('#');
                if !hex.eq_ignore_ascii_case("none") {
                    val = 0xff00_0000 | xpm_rgb(hex)?;
                }
                break;
            }
            i += 1;
        }
        cmap.insert(key, val);
    }
    let mut data = Vec::with_capacity((w * h) as usize);
    for l in lines.iter().skip(1 + ncol).take(h as usize) {
        if l.len() < cpp * w as usize {
            return None;
        }
        for x in 0..w as usize {
            let key = &l[x * cpp..(x + 1) * cpp];
            data.push(*cmap.get(key).unwrap_or(&0));
        }
    }
    Some(Icon { w, h, data })
}

/// `Fl_Image::desaturate` — moyenne des canaux → gris.
fn desaturate(px: u32) -> u32 {
    let a = px & 0xff00_0000;
    let r = (px >> 16) & 0xff;
    let g = (px >> 8) & 0xff;
    let b = px & 0xff;
    let v = (r + g + b) / 3;
    a | (v << 16) | (v << 8) | v
}

/// `Fl_Image::color_average(c, 0.14)` — blend vers `c` avec poids w.
fn color_average(px: u32, c: u32, w: f32) -> u32 {
    let a = px & 0xff00_0000;
    let mix =
        |p: u32, q: u32| -> u32 { (((p as f32) * (1.0 - w)) + ((q as f32) * w)).round() as u32 };
    let r = mix((px >> 16) & 0xff, (c >> 16) & 0xff);
    let g = mix((px >> 8) & 0xff, (c >> 8) & 0xff);
    let b = mix(px & 0xff, c & 0xff);
    a | (r << 16) | (g << 8) | b
}

/// `iconset` (`ui.cc`) — jeux standard et small ; `*In` dérivés
/// (`copy` + `desaturate` + `color_average(bg, .14)` verbatim).
pub struct IconSet {
    /// Images indexées par [`IconKind`].
    pub imgs: Vec<Option<Icon>>,
    /// Arrière-plan de référence pour `color_average`.
    pub bg: u32,
}

/// Identifiants d'icônes (champs `iconset` amont).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconKind {
    /// `ImgMeterOK`.
    MeterOk = 0,
    /// `ImgMeterBug`.
    MeterBug,
    /// `ImgHome`.
    Home,
    /// `ImgReload`.
    Reload,
    /// `ImgSave`.
    Save,
    /// `ImgBook`.
    Book,
    /// `ImgTools`,
    Tools,
    /// `ImgClear`.
    Clear,
    /// `ImgSearch`.
    Search,
    /// `ImgHelp`.
    Help,
    /// `ImgLeft`.
    Left,
    /// `ImgLeftIn` (désaturé).
    LeftIn,
    /// `ImgRight`.
    Right,
    /// `ImgRightIn`.
    RightIn,
    /// `ImgStop`.
    Stop,
    /// `ImgStopIn`.
    StopIn,
}

impl IconSet {
    /// `standard_icons`/`small_icons` (`ui.cc:42-83`) ; `bg` = couleur
    /// `FL_BACKGROUND_COLOR` résolue (prefs `ui_main_bg_color` ou
    /// défaut plateforme) pour le `color_average` des `*In`.
    #[must_use]
    pub fn new(small: bool, bg: u32) -> Self {
        use dillo_platform::pixmaps as px;
        let mut v: Vec<Option<Icon>> = vec![None; 16];
        let mut put = |k: IconKind, data: &[&str]| {
            v[k as usize] = xpm_decode(data);
        };
        put(IconKind::MeterOk, px::MINI_OK_XPM);
        put(IconKind::MeterBug, px::MINI_BUG_XPM);
        if small {
            put(IconKind::Home, px::HOME_S_XPM);
            put(IconKind::Reload, px::RELOAD_S_XPM);
            put(IconKind::Save, px::SAVE_S_XPM);
            put(IconKind::Book, px::BM_S_XPM);
            put(IconKind::Tools, px::TOOLS_S_XPM);
            put(IconKind::Left, px::LEFT_S_XPM);
            put(IconKind::Right, px::RIGHT_S_XPM);
            put(IconKind::Stop, px::STOP_S_XPM);
        } else {
            put(IconKind::Home, px::HOME_XPM);
            put(IconKind::Reload, px::RELOAD_XPM);
            put(IconKind::Save, px::SAVE_XPM);
            put(IconKind::Book, px::BM_XPM);
            put(IconKind::Tools, px::TOOLS_XPM);
            put(IconKind::Left, px::LEFT_XPM);
            put(IconKind::Right, px::RIGHT_XPM);
            put(IconKind::Stop, px::STOP_XPM);
        }
        put(IconKind::Clear, px::NEW_S_XPM);
        put(IconKind::Search, px::SEARCH_XPM);
        put(IconKind::Help, px::HELP_XPM);
        // *In : copy + desaturate + color_average (verbatim
        // make_toolbar — pas les *_i_xpm embarqués).
        for (k, kin) in [
            (IconKind::Left, IconKind::LeftIn),
            (IconKind::Right, IconKind::RightIn),
            (IconKind::Stop, IconKind::StopIn),
        ] {
            if let Some(src) = &v[k as usize] {
                let mut icn = src.clone();
                for p in &mut icn.data {
                    *p = color_average(desaturate(*p), bg, 0.14);
                }
                v[kin as usize] = Some(icn);
            }
        }
        IconSet { imgs: v, bg }
    }
}

// -- géométrie ----------------------------------------------------------------

/// Rectangle entier (`x, y, w, h` ≙ `Fl_Widget`).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Rect {
    /// `x()`.
    pub x: i32,
    /// `y()`.
    pub y: i32,
    /// `w()`.
    pub w: i32,
    /// `h()`.
    pub h: i32,
}

impl Rect {
    /// `(x, y)` ∈ rect.
    #[must_use]
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

/// Identifiant de widget logique du chrome (≙ pointeur `Fl_Widget*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WId {
    /// Bouton de barre d'outils (`b1_cb`, codes `UI_*`).
    Btn(UiButton),
    /// `FileButton` (`filemenu_cb`).
    FileMenu,
    /// `Clear` (`CustPasteButton` — `clear_cb` + paste).
    Clear,
    /// `Search` (`search_cb`).
    Search,
    /// `Help` (`help_cb`).
    Help,
    /// `Location` (`CustInput` — `location_cb`).
    Location,
    /// `PProg`/`IProg` (`CustProgressBox`).
    PageProg,
    /// `IProg`.
    ImgProg,
    /// `StatusOutput` (`Fl_Output`).
    StatusOut,
    /// `BugMeter` (`bugmeter_cb`).
    BugMeter,
    /// Espaceur `Fl_Box` (resizable amont) / `LocationGroup`.
    Spacer,
}

/// Widget logique (rect + visibilité + état + label/icône).
#[derive(Clone)]
pub struct WItem {
    /// Identifiant.
    pub id: WId,
    /// Géométrie.
    pub rect: Rect,
    /// `visible()` amont.
    pub visible: bool,
    /// `active()` amont (sensibilité).
    pub active: bool,
    /// `label()` / `copy_label`.
    pub label: String,
    /// `set_tooltip` (affichage = tipwin, chrome-3).
    pub tooltip: &'static str,
    /// `image()` — icône.
    pub icon: Option<Icon>,
    /// `deimage()` — icône inactive.
    pub deicon: Option<Icon>,
    /// `value()` pressé (`FL_THIN_DOWN_FRAME` amont).
    pub down: bool,
}

impl WItem {
    fn new(id: WId, x: i32, w: i32, h: i32) -> Self {
        WItem {
            id,
            rect: Rect { x, y: 0, w, h },
            visible: true,
            active: true,
            label: String::new(),
            tooltip: "",
            icon: None,
            deicon: None,
            down: false,
        }
    }
}

/// Groupe horizontal (`CustGroupHorizontal`) — enfants ordonnés +
/// `resizable` (index). La version verticale est [`Ui::rearrange`].
#[derive(Default)]
pub struct Group {
    /// Rect du groupe.
    pub rect: Rect,
    /// Enfants (ordre = ordre `Fl_Group`).
    pub children: Vec<WItem>,
    /// Index de l'enfant `resizable` (reçoit l'espace restant).
    pub resizable: Option<usize>,
}

impl Group {
    /// `CustGroupHorizontal::rearrange` verbatim : chaque enfant prend
    /// sa `w`, le resizable `w - sum` des visibles (0 + plus de
    /// resizable si débordement).
    pub fn rearrange(&mut self) {
        let mut sum = 0;
        for (i, c) in self.children.iter().enumerate() {
            if Some(i) != self.resizable && c.visible {
                sum += c.rect.w;
            }
        }
        let mut x = self.rect.x;
        let mut drop_resizable = false;
        for (i, c) in self.children.iter_mut().enumerate() {
            if Some(i) == self.resizable {
                if self.rect.w > sum {
                    c.rect = Rect {
                        x,
                        y: self.rect.y,
                        w: self.rect.w - sum,
                        h: self.rect.h,
                    };
                } else {
                    /* widgets overflow width */
                    c.rect = Rect {
                        x,
                        y: self.rect.y,
                        w: 0,
                        h: self.rect.h,
                    };
                    drop_resizable = true;
                }
            } else {
                c.rect = Rect {
                    x,
                    y: self.rect.y,
                    w: c.rect.w,
                    h: self.rect.h,
                };
            }
            if c.visible {
                x += c.rect.w;
            }
        }
        if drop_resizable {
            self.resizable = None;
        }
    }

    /// Hit-test (visible uniquement).
    #[must_use]
    pub fn widget_at(&self, x: i32, y: i32) -> Option<usize> {
        self.children
            .iter()
            .position(|c| c.visible && c.rect.contains(x, y))
    }

    /// Premier enfant de l'id donné.
    #[must_use]
    pub fn find(&self, id: WId) -> Option<usize> {
        self.children.iter().position(|c| c.id == id)
    }
}

// -- `CustInput`/`TipWinInput` → `LineInput` -----------------------------------

/// Champ texte mono-ligne (`Fl_Input` sous-ensemble verbatim utilisé
/// par `ui.cc` : `value`, `size`, `position`/`d_position`, `cut`).
#[derive(Default)]
pub struct LineInput {
    /// `value()` — contenu UTF-8.
    pub text: String,
    /// `insert_position()` — curseur (en octets).
    pub pos: usize,
    /// `mark()` — autre extrémité de la sélection.
    pub mark: usize,
}

impl LineInput {
    /// `size()` amont.
    #[must_use]
    pub fn size(&self) -> usize {
        self.text.len()
    }

    /// `value(s)` — remplace tout ; curseur au début (amont le pose
    /// à `strlen` si focusé — géré par l'appelant [`Ui::set_location`]).
    pub fn set_value(&mut self, s: &str) {
        self.text = s.to_string();
        self.pos = self.text.len();
        self.mark = self.pos;
    }

    /// `d_position(p)` / `d_position(p,m)` — borne à `size` et à une
    /// frontière UTF-8 (l'input FLTK borne au buffer).
    pub fn set_position(&mut self, p: usize, m: usize) {
        self.pos = clamp_boundary(&self.text, p);
        self.mark = clamp_boundary(&self.text, m);
    }

    /// `cut(f,t)` — retire `[f,t)` et pose le curseur à `f`.
    pub fn cut(&mut self, f: usize, t: usize) {
        let (f, t) = (clamp_boundary(&self.text, f), clamp_boundary(&self.text, t));
        if f < t {
            self.text.replace_range(f..t, "");
        }
        self.set_position(f, f);
    }

    /// `insert(text)` — remplace la sélection ou insère au curseur.
    pub fn insert(&mut self, s: &str) {
        let (a, b) = if self.pos <= self.mark {
            (self.pos, self.mark)
        } else {
            (self.mark, self.pos)
        };
        self.text.replace_range(a..b, s);
        let p = a + s.len();
        self.set_position(p, p);
    }

    /// `position(size(), 0)` — tout sélectionné (verbatim `Ctrl+A/L`
    /// et `focus_location`/`set_location`).
    pub fn select_all(&mut self) {
        self.set_position(self.size(), 0);
    }

    /// Texte sélectionné (`pos..mark` ordonné).
    #[must_use]
    pub fn selection(&self) -> &str {
        let (a, b) = if self.pos <= self.mark {
            (self.pos, self.mark)
        } else {
            (self.mark, self.pos)
        };
        &self.text[a..b]
    }
}

/// Borne `p` à `len` puis recule jusqu'à une frontière UTF-8.
fn clamp_boundary(s: &str, p: usize) -> usize {
    let mut p = p.min(s.len());
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// Édition par touche (`Fl_Input::handle` sous-ensemble : flèches,
/// Home/End, Backspace/Delete, Ctrl+C/X/V ≙ `kf_copy/cut/paste`,
/// Ctrl+J/U/W omis — non utilisés par `ui.cc`). `text` = insertion.
/// Retourne `true` si consommé.
pub fn line_input_key(
    input: &mut LineInput,
    keysym: i32,
    text: Option<&str>,
    shift: bool,
    ctrl: bool,
    copy: &mut dyn FnMut(&str),
    paste: &mut dyn FnMut() -> Option<String>,
) -> bool {
    use crate::keys::keysym;
    if ctrl {
        match keysym {
            // CustInput intercepte Ctrl+A/L avant ici (sélection).
            b if b == 'c' as i32 || b == 'x' as i32 => {
                let sel = input.selection().to_string();
                if !sel.is_empty() {
                    copy(&sel);
                    if b == 'x' as i32 {
                        let (a, z) = (input.pos.min(input.mark), input.pos.max(input.mark));
                        input.cut(a, z);
                    }
                }
                return true;
            }
            b if b == 'v' as i32 => {
                if let Some(t) = paste() {
                    input.insert(&t);
                }
                return true;
            }
            _ => return false,
        }
    }
    match keysym {
        keysym::LEFT => {
            let p = prev_boundary(&input.text, input.pos);
            input.set_position(p, if shift { input.mark } else { p });
            true
        }
        keysym::RIGHT => {
            let p = next_boundary(&input.text, input.pos);
            input.set_position(p, if shift { input.mark } else { p });
            true
        }
        keysym::HOME => {
            input.set_position(0, if shift { input.mark } else { 0 });
            true
        }
        keysym::END => {
            let n = input.size();
            input.set_position(n, if shift { input.mark } else { n });
            true
        }
        keysym::BACKSPACE => {
            if input.pos != input.mark {
                let (a, b) = (input.pos.min(input.mark), input.pos.max(input.mark));
                input.cut(a, b);
            } else if input.pos > 0 {
                let f = prev_boundary(&input.text, input.pos);
                input.cut(f, input.pos);
            }
            true
        }
        keysym::DELETE => {
            if input.pos != input.mark {
                let (a, b) = (input.pos.min(input.mark), input.pos.max(input.mark));
                input.cut(a, b);
            } else if input.pos < input.size() {
                let t = next_boundary(&input.text, input.pos);
                input.cut(input.pos, t);
            }
            true
        }
        _ => {
            if let Some(t) = text {
                if !t.is_empty() {
                    input.insert(t);
                    return true;
                }
            }
            false
        }
    }
}

fn prev_boundary(s: &str, p: usize) -> usize {
    let mut p = p.min(s.len()).saturating_sub(1);
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

fn next_boundary(s: &str, p: usize) -> usize {
    let mut p = (p.min(s.len()) + 1).min(s.len());
    while p < s.len() && !s.is_char_boundary(p) {
        p += 1;
    }
    p
}

/// Span `(début, fin)` du mot autour de `p` (≙ `word_end`/
/// `word_begin` de `Fl_Input` au double-clic — run de
/// `[A-Za-z0-9_]` ; un séparateur seul sélectionne le caractère).
fn word_span(s: &str, p: usize) -> (usize, usize) {
    let b = s.as_bytes();
    let is_w = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut a = p.min(b.len());
    let mut z = a;
    if z < b.len() && !is_w(b[z]) {
        return (a, (z + 1).min(b.len()));
    }
    while a > 0 && is_w(b[a - 1]) {
        a -= 1;
    }
    while z < b.len() && is_w(b[z]) {
        z += 1;
    }
    (a, z)
}

// -- `UI` ---------------------------------------------------------------------

/// État de focus (`Main` ≙ viewport / `Location` ≙ `CustInput`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Focus {
    /// `Main->take_focus()`.
    #[default]
    Main,
    /// `Location->take_focus()`.
    Location,
}

/// `UI` amont (par onglet) — modèle logique complet du chrome.
/// Les bools reflètent des flags verbatim (`panel_temporary`,
/// `small_icons`, `loc_drag`…).
#[allow(clippy::struct_excessive_bools)]
pub struct Ui {
    /// Rect de l'onglet (`UI` hérite `CustGroupVertical`).
    pub rect: Rect,
    /// `LocBar` (panel non-tiny).
    pub loc_bar: Option<Group>,
    /// `NavBar`.
    pub nav_bar: Option<Group>,
    /// `StatusBar` ([`WId::StatusOut`] + [`WId::BugMeter`]).
    pub status_bar: Group,
    /// `Main` (zone rendue — le `SkiaView` de l'onglet est blitté là).
    pub main: Rect,
    /// `FindBar` rect + visibilité (`fh` hauteur quand visible).
    pub findbar: Rect,
    /// `FindBar->visible()`.
    pub findbar_visible: bool,
    /// `FindBar` internals (≙ `Fl_Group` : hide/input/next/prev/check).
    pub findbar_state: Findbar,
    /// `Location` input (état texte/curseur).
    pub location: LineInput,
    /// `LocationGroup` rect (input + marge `CustInput`).
    pub location_group: Rect,
    /// Texte `Main` (label "Welcome…" ≙ groupe placeholder).
    pub main_label: String,
    /// Focus courant.
    pub focus: Focus,
    /// Rects de la fenêtre d'appartenance (≙ `w()` du groupe).
    pub win_w: i32,
    /// `h()` groupe (pour `rearrange`).
    pub win_h: i32,
    // -- vars verbatim `ui.hh` --
    /// `PanelSize` (`P_tiny`/`P_small`/`P_medium`).
    pub panel_size: i32,
    /// `Small_Icons`.
    pub small_icons: bool,
    /// `Panelmode`.
    pub panel_mode: UiPanelMode,
    /// `PanelTemporary` (`focus_location` sous panel caché).
    pub panel_temporary: bool,
    /// `bw`/`bh` — taille boutons.
    pub bw: i32,
    /// `bh`.
    pub bh: i32,
    /// `mh` — hauteur barre de menu (0 amont).
    pub mh: i32,
    /// `lh` — hauteur barre location.
    pub lh: i32,
    /// `nh` — hauteur NavBar.
    pub nh: i32,
    /// `fh` — hauteur findbar (28 verbatim).
    pub fh: i32,
    /// `sh` — hauteur status bar (20 verbatim).
    pub sh: i32,
    /// `pw` — largeur progress boxes (70 verbatim).
    pub pw: i32,
    /// `lbl` — labels sous les boutons.
    pub lbl: i32,
    /// Curseur de construction `p_xpos`.
    pub p_xpos: i32,
    /// `p_ypos`.
    pub p_ypos: i32,
    /// Vue de l'onglet (back-buffer propre ≙ `FltkViewport` par tab —
    /// seul le pixmap de l'onglet actif est présenté).
    pub view: Rc<RefCell<SkiaView>>,
    /// Scrollbars de l'onglet (≙ `Fl_Scrollbar` enfants du viewport
    /// amont) — hit/drag/draw dans le pixmap de [`Ui::view`].
    pub scroll: dillo_platform::app::ScrollbarGlue,
    /// `Location` textsize (`rint(14*font_factor)`).
    pub loc_font_px: i32,
    /// Drag bouton-1 amorcé dans `Location` (≙ `Fl_Input`
    /// `FL_PUSH`→`FL_DRAG` — la sélection suit la souris jusqu'au
    /// release).
    pub loc_drag: bool,
}

impl Ui {
    /// `UI::UI` (verbatim) — construction complète d'un chrome
    /// d'onglet : panel, Main, FindBar, StatusBar, `customize`,
    /// `panels_toggle` si `UI_HIDDEN` (`fullwindow_start`).
    pub fn new(
        platform: &Rc<RefCell<SkiaPlatform>>,
        prefs: &Prefs,
        ui_w: i32,
        ui_h: i32,
        cur_ui: Option<&Ui>,
    ) -> Self {
        let mut ui = Ui {
            rect: Rect {
                x: 0,
                y: 0,
                w: ui_w,
                h: ui_h,
            },
            loc_bar: None,
            nav_bar: None,
            status_bar: Group::default(),
            main: Rect::default(),
            findbar: Rect::default(),
            findbar_visible: false,
            findbar_state: Findbar::new(),
            location: LineInput::default(),
            location_group: Rect::default(),
            main_label: String::new(),
            focus: Focus::Main,
            win_w: ui_w,
            win_h: ui_h,
            panel_size: 0,
            small_icons: false,
            panel_mode: UiPanelMode::Normal,
            panel_temporary: false,
            bw: 0,
            bh: 0,
            mh: 0,
            lh: 0,
            nh: 0,
            fh: 0,
            sh: 0,
            pw: 0,
            lbl: 0,
            p_xpos: 0,
            p_ypos: 0,
            view: Rc::new(RefCell::new(SkiaView::new(platform, 1, 1))),
            scroll: dillo_platform::app::ScrollbarGlue::default(),
            loc_font_px: (14.0 * prefs.font_factor).round() as i32,
            loc_drag: false,
        };
        if let Some(cur) = cur_ui {
            ui.panel_size = cur.panel_size;
            ui.small_icons = cur.small_icons;
            ui.panel_mode = cur.panel_mode;
        } else {
            // Set some default values
            ui.panel_size = match prefs.panel_size {
                PanelSize::Tiny => 0,
                PanelSize::Small => 1,
                PanelSize::Medium => 2,
            };
            ui.small_icons = prefs.small_icons;
            ui.panel_mode = if prefs.fullwindow_start {
                UiPanelMode::Hidden
            } else {
                UiPanelMode::Normal
            };
        }

        // Control panel
        ui.make_panel(ui_w, prefs);

        // Render area ("Main", placeholder "Welcome..." amont —
        // label hérité du tab button ; l'UI amont met "Welcome..." )
        ui.main_label = "Welcome...".to_string();

        // Find text bar
        ui.findbar = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        }; // hidden ≙ size(0,0)

        // Status Panel
        ui.make_status_bar(ui_w, ui_h);
        ui.rearrange();
        ui.customize(prefs);

        if ui.panel_mode == UiPanelMode::Hidden {
            ui.panels_toggle();
        }
        ui
    }

    /// Taille des boutons/panel (`make_panel` verbatim — métriques
    /// par `PanelSize` × `Small_Icons` ; `nh=bh, fh=28, sh=20, pw=70`).
    fn panel_metrics(&mut self) {
        self.pw = 70;
        self.p_xpos = 0;
        self.p_ypos = 0;
        match self.panel_size {
            0 /* P_tiny */ => {
                if self.small_icons {
                    self.bw = 22; self.bh = 22; self.mh = 0; self.lh = 22; self.lbl = 0;
                } else {
                    self.bw = 28; self.bh = 28; self.mh = 0; self.lh = 28; self.lbl = 0;
                }
            }
            1 /* P_small */ => {
                if self.small_icons {
                    self.bw = 20; self.bh = 20; self.mh = 0; self.lh = 20; self.lbl = 0;
                } else {
                    self.bw = 28; self.bh = 28; self.mh = 0; self.lh = 28; self.lbl = 0;
                }
            }
            _ /* P_medium */ => {
                if self.small_icons {
                    self.bw = 42; self.bh = 36; self.mh = 0; self.lh = 22; self.lbl = 1;
                } else {
                    self.bw = 45; self.bh = 45; self.mh = 0; self.lh = 28; self.lbl = 1;
                }
            }
        }
        self.nh = self.bh;
        self.fh = 28;
        self.sh = 20;
    }

    /// `make_button` (verbatim) — `FL_FLAT_BOX`/`FL_THIN_DOWN_FRAME`,
    /// labelsize 12, `clear_visible_focus`.
    fn make_button(
        &mut self,
        label: &'static str,
        icon: Option<Icon>,
        deicon: Option<Icon>,
        btn: UiButton,
        tooltip: &'static str,
        start: bool,
    ) -> WItem {
        if start {
            self.p_xpos = 0;
        }
        let mut b = WItem::new(WId::Btn(btn), self.p_xpos, self.bw, self.bh);
        if self.lbl != 0 {
            b.label = label.to_string();
        }
        b.icon = icon;
        b.deicon = deicon;
        b.tooltip = tooltip;
        self.p_xpos += self.bw;
        b
    }

    /// `make_toolbar` (verbatim) — boutons + tooltips ; les `*In`
    /// sont pré-calculés dans [`IconSet`].
    fn make_toolbar(&mut self, icons: &IconSet) -> Vec<WItem> {
        let icon = |k: IconKind| icons.imgs[k as usize].clone();
        vec![
            self.make_button(
                "Back",
                icon(IconKind::Left),
                icon(IconKind::LeftIn),
                UiButton::Back,
                "Previous page",
                true,
            ),
            self.make_button(
                "Forw",
                icon(IconKind::Right),
                icon(IconKind::RightIn),
                UiButton::Forw,
                "Next page",
                false,
            ),
            self.make_button(
                "Home",
                icon(IconKind::Home),
                None,
                UiButton::Home,
                "Go to the Home page\nMiddle-click for new tab.",
                false,
            ),
            self.make_button(
                "Reload",
                icon(IconKind::Reload),
                None,
                UiButton::Reload,
                "Reload",
                false,
            ),
            self.make_button(
                "Save",
                icon(IconKind::Save),
                None,
                UiButton::Save,
                "Save this page",
                false,
            ),
            self.make_button(
                "Stop",
                icon(IconKind::Stop),
                icon(IconKind::StopIn),
                UiButton::Stop,
                "Stop loading",
                false,
            ),
            self.make_button(
                "Book",
                icon(IconKind::Book),
                None,
                UiButton::Book,
                "View bookmarks\nMiddle-click for new tab.",
                false,
            ),
            self.make_button(
                "Tools",
                icon(IconKind::Tools),
                None,
                UiButton::Tools,
                "Settings",
                false,
            ),
        ]
    }

    /// `make_filemenu_button` (verbatim) — `&F` (tiny) ou `&File`,
    /// `w = texte + padding(=largeur "W")`, caché si `!show_filemenu`.
    fn make_filemenu_button(&mut self, text_w: impl Fn(&str) -> i32) -> WItem {
        // "W" mesuré ≙ padding ; label affiché sans le `&` amont
        // (mnemonic — notre chrome n'a pas de mnemonic underline).
        let padding = text_w("W");
        let label = if self.panel_size == 0 { "&F" } else { "&File" };
        let shown = label.trim_start_matches('&');
        let w = text_w(shown) + padding;
        let h = if self.panel_size == 0 {
            self.bh
        } else {
            self.lh
        };
        let mut b = WItem::new(WId::FileMenu, self.p_xpos, w, h);
        b.label = shown.to_string();
        b.tooltip = "File menu";
        self.p_xpos += w;
        b
    }

    /// `make_location` (verbatim) — Clear(16) + LocationGroup + input
    /// (`ww-p_xpos-32`) + Search(16) + Help(16).
    fn make_location(&mut self, icons: &IconSet, ww: i32) -> Vec<WItem> {
        let icon = |k: IconKind| icons.imgs[k as usize].clone();
        let mut v = Vec::new();
        let mut b = WItem::new(WId::Clear, self.p_xpos, 16, self.lh);
        b.icon = icon(IconKind::Clear);
        b.tooltip = "Clear the URL box.\nMiddle-click to paste a URL.";
        self.p_xpos += 16;
        v.push(b);
        // LocationGroup (resizable) — l'input CustInput y vit avec
        // marge 3 (`DILLO_INPUTBOX` ≙ dx+3/dw+3).
        let lw = ww - self.p_xpos - 32;
        let mut g = WItem::new(WId::Location, self.p_xpos, lw, self.lh);
        g.tooltip = "Location";
        self.location_group = Rect {
            x: self.p_xpos,
            y: 0,
            w: lw,
            h: self.lh,
        };
        self.p_xpos += lw;
        v.push(g);
        let mut s = WItem::new(WId::Search, self.p_xpos, 16, self.lh);
        s.icon = icon(IconKind::Search);
        s.tooltip = "Search the Web";
        self.p_xpos += 16;
        v.push(s);
        let mut h = WItem::new(WId::Help, self.p_xpos, 16, self.lh);
        h.icon = icon(IconKind::Help);
        h.tooltip = "Help";
        self.p_xpos += 16;
        v.push(h);
        v
    }

    /// `make_progress_bars` (verbatim) — `pw` chacun, `wide` → labels
    /// "Images\n0 of 0"/"Page\n0.0 KB", `thin_up` → fine bordure.
    fn make_progress_bars(&mut self, wide: bool) -> Vec<WItem> {
        let mut v = Vec::new();
        let mut i = WItem::new(WId::ImgProg, self.p_xpos, self.pw, self.bh);
        i.label = if wide {
            "Images\n0 of 0".into()
        } else {
            "0 of 0".into()
        };
        self.p_xpos += self.pw;
        v.push(i);
        let mut p = WItem::new(WId::PageProg, self.p_xpos, self.pw, self.bh);
        p.label = if wide {
            "Page\n0.0 KB".into()
        } else {
            "0.0 KB".into()
        };
        v.push(p);
        v
    }

    /// `make_panel` (verbatim) — ordre des enfants et `resizable`.
    pub fn make_panel(&mut self, ww: i32, prefs: &Prefs) {
        let bg = if prefs.ui_main_bg_color != -1 {
            prefs.ui_main_bg_color as u32
        } else {
            FL_BG_DEFAULT as u32
        };
        let icons = IconSet::new(self.small_icons, bg);
        self.panel_metrics();

        if self.panel_size == 0
        /* P_tiny */
        {
            let mut nav = Group {
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: ww,
                    h: self.nh,
                },
                children: Vec::new(),
                resizable: None,
            };
            nav.children.extend(self.make_toolbar(&icons));
            nav.children
                .push(self.make_filemenu_button(|s| text_w(s, 12)));
            // resizable = LocationGroup (2e enfant de make_location,
            // après Clear — verbatim `resizable(LocationGroup)`).
            let loc_idx = nav.children.len();
            nav.children.extend(self.make_location(&icons, ww));
            nav.resizable = Some(loc_idx + 1);
            nav.children.extend(self.make_progress_bars(false));
            nav.rearrange();
            self.nav_bar = Some(nav);
            self.loc_bar = None;
        } else {
            // Location
            let mut loc = Group {
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: ww,
                    h: self.lh,
                },
                children: Vec::new(),
                resizable: None,
            };
            self.p_xpos = 0;
            loc.children
                .push(self.make_filemenu_button(|s| text_w(s, 12)));
            let loc_idx = loc.children.len();
            loc.children.extend(self.make_location(&icons, ww));
            loc.resizable = Some(loc_idx + 1);
            loc.rearrange();
            self.loc_bar = Some(loc);

            // Toolbar
            self.p_xpos = 0;
            self.p_ypos = 0;
            let mut nav = Group {
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: ww,
                    h: self.bh,
                },
                children: Vec::new(),
                resizable: None,
            };
            nav.children.extend(self.make_toolbar(&icons));
            // spacer resizable (Fl_Box amont)
            let sp_idx = nav.children.len();
            let mut sp = WItem::new(
                WId::Spacer,
                self.p_xpos,
                ww - self.p_xpos - 2 * self.pw,
                self.bh,
            );
            sp.label = String::new();
            nav.children.push(sp);
            nav.resizable = Some(sp_idx);
            self.p_xpos = ww - 2 * self.pw;
            nav.children
                .extend(self.make_progress_bars(self.panel_size != 1));
            nav.rearrange();
            self.nav_bar = Some(nav);
        }
    }

    /// `make_status_bar` (verbatim) — `StatusOutput` (resizable,
    /// `ww-20`) + `BugMeter` (20).
    fn make_status_bar(&mut self, ww: i32, wh: i32) {
        let bm_w = 20;
        let mut sb = Group {
            rect: Rect {
                x: 0,
                y: wh - self.sh,
                w: ww,
                h: self.sh,
            },
            children: Vec::new(),
            resizable: None,
        };
        let mut st = WItem::new(WId::StatusOut, 0, ww - bm_w, self.sh);
        st.label = "https://dillo-browser.org/".to_string();
        sb.children.push(st);
        sb.resizable = Some(0);
        let mut bug = WItem::new(WId::BugMeter, ww - bm_w, bm_w, self.sh);
        bug.tooltip = "Show HTML bugs\n(right-click for menu)";
        sb.children.push(bug);
        sb.rearrange();
        self.status_bar = sb;
    }

    /// `CustGroupVertical::rearrange` verbatim — ordre TopGroup :
    /// `LocBar`?, `NavBar`?, `Main`, `FindBar`, `StatusBar` ; le
    /// resizable (`Main`) reçoit l'espace restant.
    pub fn rearrange(&mut self) {
        // Somme des hauteurs visibles non-resizable.
        let mut sum = 0;
        let loc_h = self.loc_bar.as_ref().filter(|_| true).map_or(0, |b| {
            if b.children.iter().any(|c| c.visible) && b.rect.h > 0 {
                b.rect.h
            } else {
                0
            }
        });
        // `LocBar`/`NavBar`/`StatusBar`/`FindBar` comptent si «visibles»
        // — chez nous : barre présente = visible (hide → h=0 posée par
        // `panels_toggle`/`findbar_toggle`, verbatim STR#2639).
        let bars = [
            self.loc_bar.as_ref().map_or(0, |b| b.rect.h),
            self.nav_bar.as_ref().map_or(0, |b| b.rect.h),
            if self.findbar_visible { self.fh } else { 0 },
            if self.status_bar.rect.h > 0 {
                self.status_bar.rect.h
            } else {
                0
            },
        ];
        let _ = loc_h;
        for h in bars {
            sum += h;
        }

        let mut y = self.rect.y;
        if let Some(loc) = &mut self.loc_bar {
            let h = loc.rect.h;
            loc.rect.y = y;
            loc.rect.x = self.rect.x;
            loc.rect.w = self.rect.w;
            loc.rearrange();
            if h > 0 {
                y += h;
            }
        }
        if let Some(nav) = &mut self.nav_bar {
            let h = nav.rect.h;
            nav.rect.y = y;
            nav.rect.x = self.rect.x;
            nav.rect.w = self.rect.w;
            nav.rearrange();
            if h > 0 {
                y += h;
            }
        }
        // Main (resizable)
        let main_h = self.rect.h - sum;
        self.main = Rect {
            x: self.rect.x,
            y,
            w: self.rect.w,
            h: main_h.max(0),
        };
        y += main_h.max(0);
        // FindBar
        if self.findbar_visible {
            self.findbar = Rect {
                x: self.rect.x,
                y,
                w: self.rect.w,
                h: self.fh,
            };
            y += self.fh;
        } else {
            self.findbar = Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            };
        }
        // StatusBar
        self.status_bar.rect.y = y;
        self.status_bar.rect.x = self.rect.x;
        self.status_bar.rect.w = self.rect.w;
        if self.status_bar.rect.h > 0 {
            self.status_bar.rearrange();
        }
    }

    /// `UI::set_render_layout` (verbatim) — `Main` = viewport ; chez
    /// nous la vue reste attachée, on re-synchronise juste la géométrie
    /// (`nw->resize(0,Main->y(),Main->w(),Main->h())` amont).
    pub fn set_main_rect(&mut self) {
        let (w, h) = (self.main.w.max(1), self.main.h.max(1));
        self.view.borrow_mut().resize(w as u32, h as u32);
    }

    /// `UI::get_location`.
    #[must_use]
    pub fn get_location(&self) -> String {
        self.location.text.clone()
    }

    /// `UI::set_location` (verbatim) — `value(str)` ; curseur à la fin
    /// si l'input a le focus, 0 sinon.
    pub fn set_location(&mut self, s: &str) {
        self.location.set_value(s);
        let p = if self.focus == Focus::Location {
            self.location.size()
        } else {
            0
        };
        self.location.set_position(p, p);
    }

    /// `UI::focus_location` (verbatim) — si panel caché :
    /// `panels_toggle` + `temporaryPanels(true)` ; sélection totale.
    pub fn focus_location(&mut self) {
        if self.panel_mode == UiPanelMode::Hidden {
            self.panels_toggle();
            self.panel_temporary = true;
        }
        self.focus = Focus::Location;
        // Make text selected when already focused.
        self.location.select_all();
    }

    /// `UI::focus_main`.
    pub fn focus_main(&mut self) {
        self.focus = Focus::Main;
    }

    /// ≙ `Fl::focus()` unique : la page de cet onglet détient le
    /// clavier seulement quand ni le chrome (location ≙ `CustInput`,
    /// findbar ≙ `MyInput`) ni un popup modal (≙ grab `Fl_Menu`) ne
    /// le détiennent. Le focus in-page vit sur le canvas de
    /// [`Ui::view`] (partagé avec le `Layout` via `SharedView`).
    /// Quand le chrome possède le focus, `set_focused_resource(None)`
    /// ≙ `FL_UNFOCUS` de l'enfant (caret retiré + damage) ; un popup
    /// seul détourne les touches mais conserve le caret (≙ fenêtre
    /// de menu au-dessus — `Fl::focus` inchangé amont). Retourne la
    /// ressource focusée + la vue porteuse de son damage.
    #[allow(clippy::type_complexity)]
    pub(crate) fn page_focus_of(
        u: &Ui,
        popup_open: bool,
    ) -> Option<(Rc<dyn dillo_dw::ui::Resource>, Rc<RefCell<SkiaView>>)> {
        let owns_focus = u.focus != Focus::Main || u.findbar_state.input_focused;
        if owns_focus {
            u.view.borrow_mut().set_focused_resource(None);
        }
        if owns_focus || popup_open {
            None
        } else {
            u.view
                .borrow()
                .focused_resource()
                .map(|r| (r, Rc::clone(&u.view)))
        }
    }

    /// `UI::set_status`.
    pub fn set_status(&mut self, s: &str) {
        if let Some(i) = self.status_bar.find(WId::StatusOut) {
            self.status_bar.children[i].label = s.to_string();
        }
    }

    /// `UI::set_page_prog` (verbatim) — `cmd` 0 deactivate / 1 update
    /// ("Page\n%.1f KB" ou "%.1f KB") / 2 clear.
    pub fn set_page_prog(&mut self, nbytes: usize, cmd: i32) {
        let Some(i) = self.nav_bar.as_ref().and_then(|g| g.find(WId::PageProg)) else {
            return;
        };
        let Some(g) = self.nav_bar.as_mut() else {
            return;
        };
        let Some(w) = g.children.get_mut(i) else {
            return;
        };
        if cmd == 0 {
            w.active = false;
            return;
        }
        w.active = true;
        if cmd == 1 {
            let (prefix, magnitude) = if nbytes >= 1024 * 1024 {
                ('M', nbytes as f64 / (1024.0 * 1024.0))
            } else {
                ('K', nbytes as f64 / 1024.0)
            };
            w.label = format!(
                "{}{:.1} {}B",
                if self.panel_size == 0 { "" } else { "Page\n" },
                magnitude,
                prefix
            );
        } else if cmd == 2 {
            w.label = String::new();
        }
    }

    /// `UI::set_img_prog` (verbatim) — "Images\n%d of %d" / "%d of %d".
    pub fn set_img_prog(&mut self, n_img: i32, t_img: i32, cmd: i32) {
        let Some(i) = self.nav_bar.as_ref().and_then(|g| g.find(WId::ImgProg)) else {
            return;
        };
        let Some(g) = self.nav_bar.as_mut() else {
            return;
        };
        let Some(w) = g.children.get_mut(i) else {
            return;
        };
        if cmd == 0 {
            w.active = false;
            return;
        }
        w.active = true;
        if cmd == 1 {
            w.label = format!(
                "{}{} of {}",
                if self.panel_size == 0 { "" } else { "Images\n" },
                n_img,
                t_img
            );
        } else if cmd == 2 {
            w.label = String::new();
        }
    }

    /// `UI::set_bug_prog` (verbatim) — icône OK/Bug + label `%d`,
    /// `new_w = strlen*9 + 20`, `StatusBar->rearrange`.
    pub fn set_bug_prog(&mut self, icons_ok: &Icon, icons_bug: &Icon, n_bug: i32) {
        let Some(i) = self.status_bar.find(WId::BugMeter) else {
            return;
        };
        let mut new_w = 20;
        if n_bug == 0 {
            self.status_bar.children[i].icon = Some(icons_ok.clone());
            self.status_bar.children[i].label = String::new();
        } else {
            if n_bug == 1 {
                self.status_bar.children[i].icon = Some(icons_bug.clone());
            }
            let str_ = format!("{n_bug}");
            new_w = str_.len() as i32 * 9 + 20;
            self.status_bar.children[i].label = str_;
        }
        self.status_bar.children[i].rect.w = new_w;
        self.status_bar.rearrange();
    }

    /// `UI::customize` (verbatim) — `prefs.show_*` → `hide()`.
    pub fn customize(&mut self, prefs: &Prefs) {
        let set = |g: &mut Group, id: WId, show: bool| {
            if let Some(i) = g.find(id) {
                g.children[i].visible = show;
            }
        };
        for g in [&mut self.loc_bar, &mut self.nav_bar].into_iter().flatten() {
            if !prefs.show_back {
                set(g, WId::Btn(UiButton::Back), false);
            }
            if !prefs.show_forw {
                set(g, WId::Btn(UiButton::Forw), false);
            }
            if !prefs.show_home {
                set(g, WId::Btn(UiButton::Home), false);
            }
            if !prefs.show_reload {
                set(g, WId::Btn(UiButton::Reload), false);
            }
            if !prefs.show_save {
                set(g, WId::Btn(UiButton::Save), false);
            }
            if !prefs.show_stop {
                set(g, WId::Btn(UiButton::Stop), false);
            }
            if !prefs.show_bookmarks {
                set(g, WId::Btn(UiButton::Book), false);
            }
            if !prefs.show_tools {
                set(g, WId::Btn(UiButton::Tools), false);
            }
            if !prefs.show_clear_url {
                set(g, WId::Clear, false);
            }
            if !prefs.show_url {
                set(g, WId::Location, false);
            }
            if !prefs.show_search {
                set(g, WId::Search, false);
            }
            if !prefs.show_help {
                set(g, WId::Help, false);
            }
            if !prefs.show_progress_box {
                set(g, WId::ImgProg, false);
                set(g, WId::PageProg, false);
            }
        }
        if let Some(g) = &mut self.nav_bar {
            g.rearrange();
        }
        if let Some(g) = &mut self.loc_bar {
            g.rearrange();
        }
    }

    /// `UI::change_panel` (verbatim) — nouvelle taille d'icônes/panel
    /// à la volée : détruit les barres, reconstruit, conserve le texte
    /// de la location, re-focus.
    pub fn change_panel(&mut self, prefs: &Prefs, new_size: i32, small_icons: bool) {
        let loc_text = self.get_location();

        // Remove current panel's bars
        self.loc_bar = None;
        self.nav_bar = None;

        // Set internal vars for panel size
        self.panel_size = new_size;
        self.small_icons = small_icons;

        // make a new panel
        self.make_panel(self.rect.w, prefs);
        self.customize(prefs);
        // ≙ a_UIcmd_set_buttons_sens — fait par l'appelant (Chrome).

        self.rearrange();
        self.location.set_value(&loc_text);
        self.focus = Focus::Location;
    }

    /// `UI::button_set_sens` (verbatim — Back/Forw/Stop seulement).
    pub fn button_set_sens(&mut self, btn: UiButton, sens: bool) {
        for g in [&mut self.nav_bar, &mut self.loc_bar].into_iter().flatten() {
            if let Some(i) = g.find(WId::Btn(btn)) {
                g.children[i].active = sens;
            }
        }
    }

    /// `UI::paste_url` — déclenché sur middle-click non réclamé ;
    /// verbatim `Fl::paste(*Clear)` → sélection primaire dans Clear.
    /// Chez nous : texte clipboard → `set_location` + `open_urlstr`
    /// (dispatché par [`Chrome`]).
    #[must_use]
    pub fn wants_paste(&self) -> bool {
        true
    }

    /// `UI::findbar_toggle` (verbatim — workaround STR#2639 : taille
    /// 0,0 quand caché) + `findtext_reset` + `focus_main` côté appelant.
    pub fn findbar_toggle(&mut self, add: bool) {
        if add {
            if !self.findbar_visible {
                self.findbar = Rect {
                    x: 0,
                    y: 0,
                    w: self.rect.w,
                    h: self.fh,
                };
            }
            self.findbar_visible = true;
            // `Findbar::show` verbatim — take_focus + select-all.
            self.findbar_state.show();
        } else {
            self.findbar_state.clear_focus();
            // hide
            self.findbar = Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            };
            self.findbar_visible = false;
            // reset state + focus main — via UiCmd par Chrome.
        }
        self.rearrange();
    }

    /// `UI::panels_toggle` (verbatim — même workaround taille 0).
    pub fn panels_toggle(&mut self) {
        let hide = self.status_bar.rect.h > 0 && self.status_bar.children.iter().any(|c| c.visible);

        // hide/show panels
        if let Some(g) = &mut self.loc_bar {
            g.rect.h = if hide { 0 } else { self.lh };
        }
        if let Some(g) = &mut self.nav_bar {
            g.rect.h = if hide { 0 } else { self.nh };
        }
        if hide {
            self.status_bar.rect.h = 0;
        } else {
            self.status_bar.rect.h = self.sh;
            self.status_bar.rearrange();
        }

        self.rearrange();
        self.panel_mode = if hide {
            UiPanelMode::Hidden
        } else {
            UiPanelMode::Normal
        };
    }

    /// Hit-test global du chrome (≙ sous-arbre de widgets amont).
    /// Cherche dans LocBar/NavBar/StatusBar puis `Location`.
    #[must_use]
    pub fn widget_at(&self, x: i32, y: i32) -> Option<WId> {
        for g in [&self.loc_bar, &self.nav_bar].into_iter().flatten() {
            if let Some(i) = g.widget_at(x, y) {
                return Some(g.children[i].id);
            }
        }
        if let Some(i) = self.status_bar.widget_at(x, y) {
            return Some(self.status_bar.children[i].id);
        }
        if self.location_group.contains(x, y) {
            return Some(WId::Location);
        }
        None
    }

    /// Rect d'un widget (pour popup position — `wid->x(),wid->y()+h`).
    #[must_use]
    pub fn widget_rect(&self, id: WId) -> Option<Rect> {
        for g in [&self.loc_bar, &self.nav_bar].into_iter().flatten() {
            if let Some(i) = g.find(id) {
                return Some(g.children[i].rect);
            }
        }
        if let Some(i) = self.status_bar.find(id) {
            return Some(self.status_bar.children[i].rect);
        }
        None
    }
}

/// Largeur de texte chrome (cosmic-text, taille `px`, sans marge).
/// Séparée de `Ui` pour le calcul de `make_filemenu_button` (`&File`).
fn text_w(s: &str, px: i32) -> i32 {
    // Approximation : 8 px/car à 12pt — le vrai `measure_label` est
    // refait au draw (cosmic). Suffisant pour la géométrie amont.
    (s.len() as i32) * px * 2 / 3
}

// -- `CustTabs` → `Tabs` --------------------------------------------------------

/// Bouton d'onglet (`CustTabButton`) — `focus_num_` sert au choix du
/// focus lors de la fermeture de l'onglet actif.
pub struct TabBtn {
    /// `ui()` — handle de l'[`Ui`] associée.
    pub ui: usize,
    /// `focus_num()`.
    pub focus_num: u32,
    /// Label affiché (tronqué `set_tab_label`).
    pub label: String,
    /// Largeur (`measure_label+14` ou `tab_w` initial).
    pub w: i32,
    /// `visible()`.
    pub visible: bool,
}

/// `CustTabs` (verbatim `uicmd.cc`) — strip d'onglets + wizard
/// (`active`) + scroll horizontal implicite (`scroll_x`).
pub struct Tabs {
    /// Fenêtre (w,h) pour les calculs de resize amont.
    pub win_w: i32,
    /// `h()` fenêtre.
    pub win_h: i32,
    /// `ctab_h` — hauteur courante du strip (`tab_h` ou 1).
    pub ctab_h: i32,
    /// `tab_h` (`ui_tab_height`).
    pub tab_h: i32,
    /// `tab_w` initial (50 verbatim).
    pub tab_w: i32,
    /// `btn_w` (20 verbatim).
    pub btn_w: i32,
    /// `ctl_w` (`1*btn_w+2` verbatim).
    pub ctl_w: i32,
    /// `Pack` — boutons d'onglets (ordre = ordre Wizard).
    pub pack: Vec<TabBtn>,
    /// `Wizard` — ui affichée (`value()` = `pack[active].ui`).
    pub active: usize,
    /// `Scroll->xposition()`.
    pub scroll_x: i32,
    /// `focus_counter`.
    pub focus_counter: u32,
    /// `CloseBtn` visible (≙ `ctab_h > 1`).
    pub close_visible: bool,
}

impl Tabs {
    /// `CustTabs` ctor (verbatim) : `tab_w=50`, `tab_h=th`,
    /// `ctab_h=1`, `btn_w=20`, `ctl_w=btn_w+2`, strip caché.
    #[must_use]
    pub fn new(ww: i32, wh: i32, th: i32) -> Self {
        Tabs {
            win_w: ww,
            win_h: wh,
            ctab_h: 1,
            tab_h: th,
            tab_w: 50,
            btn_w: 20,
            ctl_w: 20 + 2,
            pack: Vec::new(),
            active: 0,
            scroll_x: 0,
            focus_counter: 0,
            close_visible: false,
        }
    }

    /// `num_tabs()`.
    #[must_use]
    pub fn num_tabs(&self) -> usize {
        self.pack.len()
    }

    /// `Wizard->value()` — ui active (`None` si vide).
    #[must_use]
    pub fn value(&self) -> Option<usize> {
        self.pack.get(self.active).map(|b| b.ui)
    }

    /// `get_btn_idx` (verbatim).
    #[must_use]
    pub fn get_btn_idx(&self, ui: usize) -> i32 {
        for (i, b) in self.pack.iter().enumerate() {
            if b.ui == ui {
                return i as i32;
            }
        }
        -1
    }

    /// `increase_focus_counter` (verbatim — wrap + renumérotation
    /// triée par `focus_num`).
    fn increase_focus_counter(&mut self) {
        if self.focus_counter < u32::MAX {
            self.focus_counter += 1;
        } else {
            /* wrap & normalize old numbers here */
            let mut order: Vec<usize> = (0..self.pack.len()).collect();
            order.sort_by_key(|&i| self.pack[i].focus_num);
            self.focus_counter = 0;
            for i in order {
                self.pack[i].focus_num = self.focus_counter;
                self.focus_counter += 1;
            }
        }
    }

    /// `update_pack_offset` (verbatim) — garde l'onglet actif visible
    /// dans le scroll (`scroll_x`).
    pub fn update_pack_offset(&mut self) {
        if self.pack.is_empty() {
            return;
        }
        let act_idx = self.active.min(self.pack.len() - 1);
        let mut x_i = 0;
        for b in self.pack.iter().take(act_idx) {
            x_i += b.w;
        }
        let bw_ = self.pack[act_idx].w;
        let x_f = x_i + bw_;
        let px_i = x_i - self.scroll_x;
        let px_f = px_i + bw_;
        let pw = self.win_w - self.ctl_w;
        if px_i < 0 {
            self.scroll_x = x_i;
        } else if px_i > pw || (px_i > 0 && px_f > pw) {
            self.scroll_x = x_i.min(x_f - pw);
        }
    }

    /// `switch_tab(btn)` (verbatim corps) — retourne le label de la
    /// nouvelle ui active pour le titre fenêtre (`None` si inchangé).
    pub fn switch_tab(&mut self, idx: i32) -> Option<usize> {
        if idx < 0 || idx as usize >= self.pack.len() {
            return None;
        }
        let idx = idx as usize;
        if idx == self.active {
            return None;
        }
        /* amont : `cbtn->ui()->show()` avant `Wizard->value()` pour
         * éviter le bug de focus clavier — no-op chez nous, l'ordre
         * observable est : couleurs (draw-time) puis activation. */
        self.active = idx;
        self.update_pack_offset();
        self.increase_focus_counter();
        self.pack[idx].focus_num = self.focus_counter;
        Some(self.pack[idx].ui)
    }

    /// `prev_tab`/`next_tab` (verbatim wrap-around).
    pub fn prev_tab(&mut self) -> Option<usize> {
        if self.pack.is_empty() {
            return None;
        }
        let idx = if self.active > 0 {
            self.active - 1
        } else {
            self.pack.len() - 1
        };
        self.switch_tab(idx as i32)
    }

    /// `next_tab`.
    pub fn next_tab(&mut self) -> Option<usize> {
        if self.pack.is_empty() {
            return None;
        }
        let idx = if self.active + 1 < self.pack.len() {
            self.active + 1
        } else {
            0
        };
        self.switch_tab(idx as i32)
    }

    /// `add_new_tab` (verbatim `uicmd.cc:286-338`) — retourne `true`
    /// si le strip vient d'apparaître (≙ redraw amont).
    pub fn add_new_tab(&mut self, ui: usize, focus: bool, label_w: impl Fn(&str) -> i32) -> bool {
        let mut strip_shown = false;
        if self.num_tabs() == 1 {
            // Show tabbar
            self.ctab_h = self.tab_h;
            self.close_visible = true;
            if let Some(b0) = self.pack.first_mut() {
                b0.w = label_w(&b0.label) + 14;
                b0.visible = true;
            }
            strip_shown = true;
        }
        self.pack.push(TabBtn {
            ui,
            focus_num: 0,
            label: DEFAULT_TAB_LABEL.to_string(),
            w: self.tab_w,
            visible: true,
        });
        if focus {
            self.switch_tab((self.pack.len() - 1) as i32);
        } else {
            // set focus counter
            self.increase_focus_counter();
            if let Some(b) = self.pack.last_mut() {
                b.focus_num = self.focus_counter;
            }
            // num_tabs()==2 → Wizard->redraw amont ≙ strip_shown
        }
        if self.num_tabs() == 1 {
            if let Some(b) = self.pack.last_mut() {
                b.visible = false;
            }
        }
        self.update_pack_offset();
        strip_shown
    }

    /// `remove_tab` (verbatim) — si l'onglet actif part, focus sur le
    /// `focus_num` max restant ; strip re-caché à 1 onglet.
    /// Retourne le rang (dans `pack`) retiré.
    pub fn remove_tab(&mut self, ui: usize) -> Option<usize> {
        let rm_idx = self.get_btn_idx(ui);
        if rm_idx < 0 {
            return None;
        }
        let rm_idx = rm_idx as usize;
        if self.active == rm_idx {
            // Active tab is being closed, switch to the "previous" one
            let mut fbtn: Option<usize> = None;
            for i in 0..self.pack.len() {
                if i != rm_idx {
                    match fbtn {
                        None => fbtn = Some(i),
                        Some(f) if self.pack[i].focus_num > self.pack[f].focus_num => {
                            fbtn = Some(i);
                        }
                        _ => {}
                    }
                }
            }
            if let Some(f) = fbtn {
                // switch_tab ajuste active/focus_num
                let _ = self.switch_tab(f as i32);
            }
        }
        self.pack.remove(rm_idx);
        if self.active > rm_idx {
            self.active -= 1;
        }
        if self.active >= self.pack.len() && !self.pack.is_empty() {
            self.active = self.pack.len() - 1;
        }
        self.update_pack_offset();

        if self.num_tabs() == 1 {
            // hide tabbar
            self.ctab_h = 1;
            self.close_visible = false;
            if let Some(b0) = self.pack.first_mut() {
                b0.w = 0;
                b0.visible = false;
            }
        }
        Some(rm_idx)
    }

    /// `set_tab_label` (verbatim) — label ≤15 chars sinon borne UTF-8
    /// + `...` ; redimensionne le bouton (`measure_label + 14`).
    pub fn set_tab_label(&mut self, ui: usize, label: &str, label_w: impl Fn(&str) -> i32) {
        let idx = self.get_btn_idx(ui);
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        // Make a label for this tab
        // verbatim : limite en *octets* (snprintf 16), bornée à une
        // frontière UTF-8 par `a_Utf8_end_of_char`.
        let mut tab_chars = 15usize;
        let label_len = label.len();
        if label_len > tab_chars {
            tab_chars = end_of_char(label.as_bytes(), tab_chars - 1) + 1;
        }
        let mut title =
            String::from_utf8_lossy(&label.as_bytes()[..tab_chars.min(label_len)]).into_owned();
        if label_len > tab_chars {
            title.push_str("...");
        }

        // Avoid unnecessary redraws
        if self.pack[idx].label != title {
            self.pack[idx].w = label_w(&title) + 14;
            self.pack[idx].label = title;
            self.update_pack_offset();
        }
    }

    /// Rect du bouton `i` en coords fenêtre (strip en haut,
    /// `scroll_x` appliqué) — pour hit-test et draw.
    #[must_use]
    pub fn tab_rect(&self, i: usize) -> Option<Rect> {
        let b = self.pack.get(i)?;
        let mut x = -self.scroll_x;
        for b in self.pack.iter().take(i) {
            x += b.w;
        }
        Some(Rect {
            x,
            y: 0,
            w: b.w,
            h: self.ctab_h,
        })
    }

    /// Hit-test strip → index de `pack` (`None` hors boutons,
    /// `usize::MAX`-1 ≙ zone CloseBtn via [`Tabs::close_at`]).
    #[must_use]
    pub fn tab_at(&self, x: i32, y: i32) -> Option<usize> {
        if y < 0 || y >= self.ctab_h {
            return None;
        }
        if x >= self.win_w - self.ctl_w {
            return None; // zone contrôle (close)
        }
        let rx = x + self.scroll_x;
        let mut acc = 0;
        for (i, b) in self.pack.iter().enumerate() {
            if !b.visible {
                continue;
            }
            if rx >= acc && rx < acc + b.w {
                return Some(i);
            }
            acc += b.w;
        }
        None
    }

    /// `x` dans la zone `CloseBtn` (verbatim `Control` group).
    #[must_use]
    pub fn close_at(&self, x: i32, y: i32) -> bool {
        self.close_visible
            && y < self.ctab_h
            && x >= self.win_w - self.ctl_w + 2
            && x < self.win_w - self.ctl_w + 2 + self.btn_w
    }
}

// -- `Chrome` : impl `UiShell` + fenêtres -----------------------------------------

/// Fenêtre chrome (≙ `Tabgroup` + `Fl_Window`) — contient un
/// [`Tabs`] ; les `Ui` vivent dans `Chrome`.
pub struct Win {
    /// `CustTabs` de la fenêtre.
    pub tabs: Tabs,
    /// Titre fenêtre (`UIcmd_set_window_labels`).
    pub title: String,
    /// Position demandée (`prefs.xpos/ypos` — appliquée par l'app).
    pub pos: Option<(i32, i32)>,
}

/// Effet de bord natif demandé par [`Chrome`] — drainé par le driver
/// [`crate::glue`] vers `DwApp` (≙ appels `Fl_Window` amont : la
/// création winit réelle est différée à la passe d'events).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WinOp {
    /// `new Fl_Window(w, h)` + `win->show()` (≙ `window_new` +
    /// `window_show` verbatim — création = présentation côté winit).
    Open {
        /// Handle `tabs` (index stable dans [`Chrome`]).
        tabs: usize,
        /// Taille demandée.
        w: i32,
        /// Taille demandée.
        h: i32,
    },
    /// `win->position(x, y)` (prefs `xpos`/`ypos`).
    Position {
        /// Handle `tabs`.
        tabs: usize,
        /// Position écran (px).
        x: i32,
        /// Position écran (px).
        y: i32,
    },
    /// `delete win` — dernier onglet fermé (`win_cb`/`close_bw`).
    Close {
        /// Handle `tabs`.
        tabs: usize,
    },
    /// `win->copy_label(title)` (≙ `UIcmd_set_window_labels`).
    SetTitle {
        /// Handle `tabs`.
        tabs: usize,
        /// Titre.
        title: String,
    },
    /// `Fl::flush()` ≙ redraw de toutes les fenêtres vivantes.
    RedrawAll,
}

/// Implémentation concrète de [`UiShell`] — registre des fenêtres/
/// onglets/UIs (≙ globaux `tabgroups` + FLTK amont). Les widgets
/// sont logiques ; le dessin passe par [`Chrome::draw_window`].
pub struct Chrome {
    /// Plateforme partagée (fontes/imgbuf pour les vues d'onglets).
    platform: Rc<RefCell<SkiaPlatform>>,
    /// Prefs partagées (≙ global `prefs` amont).
    prefs: Rc<RefCell<Prefs>>,
    /// `ui` handles → `Ui` (sparse : `Vec<Option<Ui>>`).
    uis: Vec<Option<Ui>>,
    /// Fenêtres (≙ `tabgroups`) — **tombstones** : `None` = fenêtre
    /// détruite, les index `tabs` restent stables pour le mapping
    /// `DwApp` (verbatim : les `Fl_Window*` amont sont des pointeurs
    /// stables, jamais réalloués).
    wins: Vec<Option<Win>>,
    /// Effets de bord natifs en attente — drainés par le driver
    /// ([`crate::glue`] → `DwApp`). En tests : lus, jamais appliqués.
    pub app_ops: std::collections::VecDeque<WinOp>,
    /// Compteur de handles `ui`/`tabs` (1-based).
    next_handle: usize,
    /// Layouts par ui (≙ `bw->render_layout` côté shell).
    layouts: HashMap<usize, Rc<RefCell<Layout>>>,
    /// Icônes (re-`IconSet::new` si `small_icons` change).
    icons_std: IconSet,
    /// `IconSet` small.
    icons_small: IconSet,
    /// File d'ops différées (`NavSched` ≙ `a_Timeout_add(0)`).
    deferred_nav: Vec<(u32, NavOp)>,
    /// `defer_close_all` en attente.
    deferred_close_all: bool,
    /// Répertoire de lancement (`Paths::getOldWorkingDir`).
    old_cwd: Vec<u8>,
    /// `a_Control_is_waiting`/`a_Control_notify_finish` (dilloc) —
    /// la machine [`Control`] injectée par le binaire (amont :
    /// globaux `bw_waiting`/`bw_waiting_req`).
    control: Option<Rc<RefCell<dillo_net::control::Control>>>,
    /// Chat `bookmarks` dpi (état de fonction amont → instance shell).
    pub(crate) bookmarks: crate::bookmark::Bookmarks,
    /// Globaux `popup_*`/`history_*`/`current_page_url` amont + file
    /// des popups différés (≙ `a_Timeout_add(0, Menu_popup_cb)`).
    pub menus: MenuCtx,
    /// Popup présenté — overlay modal dessiné dans le back-buffer
    /// de la fenêtre (amont : fenêtre `Fl_Menu` native — déviation
    /// ADR-0002, même modèle que les autres widgets chrome).
    pub popup: Option<menu::Popup>,
    /// Item armé par le press `(item, sub)` — dispatch au release
    /// sur le même item (≙ `Fl_Menu` : activation au relâchement).
    pub popup_armed: Option<(usize, Option<usize>)>,
    /// Sous-menu ouvert (index de l'item parent).
    pub popup_sub: Option<usize>,
    /// Item sous le curseur `(item, sub)` — surlignage + ouverture
    /// de sous-menu au survol (≙ tracking `Fl_Menu`).
    pub popup_hover: Option<(usize, Option<usize>)>,
    /// `my_tipwin()` ≙ instance unique de [`TipWin`].
    pub tipwin: TipWin,
    /// Statics `input_answer`/`input_str`/`choice_answer` + cache du
    /// menu moteurs de `a_Dialog_input` (`static pm` amont).
    dialog_state: DialogState,
    /// `DialogRunner` injecté — défaut [`AutoCancelRunner`] (annule,
    /// ≙ fermeture de fenêtre amont). `None` impossible → `Box`.
    dialog_runner: Box<dyn DialogRunner>,
    /// Seam `a_Html_*` des menus (docs vivants) — [`NullDocOps`]
    /// tant que l'app n'a pas injecté le registre (≙ `doc==NULL`).
    doc_ops: Box<dyn DocOps>,
    /// `a_Actions_link_get()` (statique amont).
    link_actions: Rc<Vec<Action>>,
    /// `a_Actions_page_get()` (statique amont).
    page_actions: Rc<Vec<Action>>,
    /// `Fl::event_x()/event_y()` du dernier event — utilisé par les
    /// `a_Menu_*_popup` (verbatim : `popup_x = Fl::event_x()`).
    pub event_xy: (i32, i32),
    /// `Keys` global amont (shortcuts affichés du file menu).
    pub keys: Keys,
    /// `DILLO_DOCDIR` amont (docdir — posé par l'app au démarrage).
    pub docdir: std::path::PathBuf,
}

impl Chrome {
    /// Nouveau shell — `platform` ≙ `FltkPlatform` global,
    /// `prefs` ≙ globales amont.
    #[must_use]
    pub fn new(platform: Rc<RefCell<SkiaPlatform>>, prefs: Rc<RefCell<Prefs>>) -> Self {
        let bg = {
            let p = prefs.borrow();
            if p.ui_main_bg_color != -1 {
                p.ui_main_bg_color as u32
            } else {
                FL_BG_DEFAULT as u32
            }
        };
        let (link_actions, page_actions) = actions_init(&prefs.borrow());
        let show_tip = prefs.borrow().show_ui_tooltip;
        Chrome {
            platform,
            prefs,
            uis: Vec::new(),
            wins: Vec::new(),
            app_ops: std::collections::VecDeque::new(),
            next_handle: 1,
            layouts: HashMap::new(),
            icons_std: IconSet::new(false, bg),
            icons_small: IconSet::new(true, bg),
            deferred_nav: Vec::new(),
            deferred_close_all: false,
            old_cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned().into_bytes())
                .unwrap_or_default(),
            control: None,
            bookmarks: crate::bookmark::Bookmarks::default(),
            menus: MenuCtx::default(),
            popup: None,
            popup_armed: None,
            popup_sub: None,
            popup_hover: None,
            tipwin: TipWin::new(show_tip),
            dialog_state: DialogState::default(),
            dialog_runner: Box::new(AutoCancelRunner),
            doc_ops: Box::new(NullDocOps),
            link_actions: Rc::new(link_actions),
            page_actions: Rc::new(page_actions),
            event_xy: (0, 0),
            keys: Keys::new(),
            docdir: std::path::PathBuf::new(),
        }
    }

    /// `Rc` plateforme partagée (clipboard/fontes — seam glue).
    #[must_use]
    pub fn platform_rc(&self) -> &Rc<RefCell<SkiaPlatform>> {
        &self.platform
    }

    /// Injecte le `DialogRunner` natif (fenêtres modales de l'app).
    pub fn set_dialog_runner(&mut self, r: Box<dyn DialogRunner>) {
        self.dialog_runner = r;
    }

    /// Injecte la seam `a_Html_*` des menus (`DocOps` ≙ accès docs).
    pub fn set_doc_ops(&mut self, d: Box<dyn DocOps>) {
        self.doc_ops = d;
    }

    /// Prochain popup à présenter (≙ timeout `Menu_popup_cb` amont).
    pub fn take_popup(&mut self) -> Option<menu::Popup> {
        self.menus.take_popup()
    }

    /// Drain des popups différés (≙ `a_Timeout_add(0, …)` : le plus
    /// récent devient l'overlay actif). `true` si changement.
    pub fn popup_activate(&mut self) -> bool {
        let mut changed = false;
        while let Some(p) = self.take_popup() {
            self.popup = Some(p);
            self.popup_armed = None;
            self.popup_sub = None;
            self.popup_hover = None;
            changed = true;
        }
        changed
    }

    /// Ferme l'overlay (≙ `Fl_Menu` quitté — clic dehors, Escape,
    /// item choisi).
    pub fn popup_close(&mut self) {
        self.popup = None;
        self.popup_armed = None;
        self.popup_sub = None;
        self.popup_hover = None;
    }

    /// Dispatch de l'item choisi (≙ `m->do_callback` dans
    /// `Menu_popup_cb`). `mouse`/`shift` ≙ `Fl::event_button`/`state`.
    #[allow(clippy::too_many_arguments)]
    pub fn popup_dispatch(
        &mut self,
        uicmd: &UiCmd,
        reg: &BwRegistry,
        popup: &mut menu::Popup,
        item: usize,
        sub: Option<usize>,
        mouse: i32,
        shift: bool,
    ) {
        let item = match (popup.items.get_mut(item), sub) {
            (Some(i), Some(s)) => i.submenu.as_mut().and_then(|v| v.get_mut(s)),
            (Some(i), None) => Some(i),
            _ => None,
        };
        let Some(item) = item else { return };
        // `menus` est sorti de `self` le temps du dispatch (verbatim
        // amont : les globaux popup_* vivent hors de la fenêtre) —
        // évite un double `&mut` (`self` est aussi le `UiShell`).
        let mut menus = std::mem::take(&mut self.menus);
        let prefs = self.prefs.clone();
        let pa = self.page_actions.clone();
        let la = self.link_actions.clone();
        let mut doc_ops = std::mem::replace(&mut self.doc_ops, Box::new(NullDocOps));
        menu::dispatch(
            &mut menus,
            uicmd,
            reg,
            self,
            &prefs,
            item,
            mouse,
            shift,
            &pa,
            &la,
            &mut *doc_ops,
        );
        self.menus = menus;
        self.doc_ops = doc_ops;
    }

    /// `Paths::setOldWorkingDir` ≙ répertoire capturé au démarrage.
    pub fn set_old_working_dir(&mut self, dir: Vec<u8>) {
        self.old_cwd = dir;
    }

    /// Injecte la machine [`dillo_net::control::Control`] du binaire ≙ `a_Control_init`
    /// (les globaux `bw_waiting`/`bw_waiting_req` y vivent).
    pub fn set_control(&mut self, ctl: Rc<RefCell<dillo_net::control::Control>>) {
        self.control = Some(ctl);
    }

    /// Vide les ops différées (≙ `a_Timeout_add(0)` — l'app draine
    /// entre deux events). Retourne `(bw, op)` en FIFO.
    pub fn drain_nav(&mut self) -> Vec<(u32, NavOp)> {
        std::mem::take(&mut self.deferred_nav)
    }

    /// `defer_close_all` en attente ?
    #[must_use]
    pub fn take_close_all(&mut self) -> bool {
        std::mem::take(&mut self.deferred_close_all)
    }

    /// `Ui` d'un handle.
    #[must_use]
    pub fn ui(&self, ui: usize) -> Option<&Ui> {
        self.uis.get(ui).and_then(|u| u.as_ref())
    }

    /// `Ui` mutable.
    pub fn ui_mut(&mut self, ui: usize) -> Option<&mut Ui> {
        self.uis.get_mut(ui).and_then(|u| u.as_mut())
    }

    /// `Tabs` contenant `ui`.
    fn tabs_mut_of(&mut self, ui: usize) -> Option<&mut Tabs> {
        self.wins
            .iter_mut()
            .flatten()
            .find(|w| w.tabs.get_btn_idx(ui) >= 0)
            .map(|w| &mut w.tabs)
    }

    /// Index fenêtre d'un handle `tabs`.
    fn win_of_tabs(&mut self, tabs: usize) -> Option<&mut Win> {
        self.wins.get_mut(tabs).and_then(|w| w.as_mut())
    }

    /// `a_UIcmd_get_bw_by_widget` — `wid` = handle `ui` (nos widgets
    /// logiques n'ont pas d'identité globale ; le shell appelle avec
    /// l'`ui` propriétaire — ≙ `ui->contains(wid)` amont).
    #[must_use]
    pub fn bw_by_ui(&self, reg: &BwRegistry, ui: usize) -> Option<u32> {
        (0..reg.len() as u32).find(|&i| reg.get(i).is_some_and(|b| b.ui.get() == ui))
    }

    /// Résolution des couleurs d'onglet (`setUIColorWdef` verbatim —
    /// `-1` → défaut FLTK : actif=BG2(blanc), inactif=BG, fg=FG(noir)).
    #[must_use]
    pub fn tab_colors(&self, active: bool) -> (u32, u32) {
        let p = self.prefs.borrow();
        let bg_main = if p.ui_main_bg_color != -1 {
            p.ui_main_bg_color
        } else {
            FL_BG_DEFAULT
        };
        let fg_def = if p.ui_fg_color != -1 {
            p.ui_fg_color
        } else {
            0
        };
        let (bg, fg) = if active {
            (
                if p.ui_tab_active_bg_color != -1 {
                    p.ui_tab_active_bg_color
                } else {
                    // ≙ Fl::get_color(FL_BACKGROUND2_COLOR) = blanc
                    if p.ui_text_bg_color != -1 {
                        p.ui_text_bg_color
                    } else {
                        0x00ff_ffff
                    }
                },
                if p.ui_tab_active_fg_color != -1 {
                    p.ui_tab_active_fg_color
                } else {
                    fg_def
                },
            )
        } else {
            (
                if p.ui_tab_bg_color != -1 {
                    p.ui_tab_bg_color
                } else {
                    bg_main
                },
                if p.ui_tab_fg_color != -1 {
                    p.ui_tab_fg_color
                } else {
                    fg_def
                },
            )
        };
        (bg as u32, fg as u32)
    }

    /// Icones selon `small_icons`.
    #[must_use]
    pub fn icons(&self, small: bool) -> &IconSet {
        if small {
            &self.icons_small
        } else {
            &self.icons_std
        }
    }
}

// -- UiShell pour Chrome --------------------------------------------------------

impl UiShell for Chrome {
    fn window_new(&mut self, w: i32, h: i32) -> usize {
        let th = self.prefs.borrow().ui_tab_height;
        // Slot stable : réutilise une tombstone sinon append (≙
        // pointeur `Tabgroup*` amont — jamais réalloué).
        let idx = self
            .wins
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.wins.push(None);
                self.wins.len() - 1
            });
        self.wins[idx] = Some(Win {
            tabs: Tabs::new(w, h, th),
            title: String::new(),
            pos: None,
        });
        self.app_ops.push_back(WinOp::Open { tabs: idx, w, h });
        idx
    }

    fn window_show(&mut self, _tabs: usize) {
        // `win->show()` — la création winit matérialisée par
        // `WinOp::Open` au drain est aussi la présentation (verbatim
        // : la fenêtre apparaît après le premier onglet).
    }

    fn window_position(&mut self, tabs: usize, x: i32, y: i32) {
        if let Some(w) = self.win_of_tabs(tabs) {
            w.pos = Some((x, y));
        }
        self.app_ops.push_back(WinOp::Position { tabs, x, y });
    }

    fn tab_ui_new(
        &mut self,
        tabs: usize,
        old_ui: usize,
        focus: bool,
    ) -> (usize, Rc<RefCell<Layout>>) {
        // `tabs->add_new_tab(old_ui, focus)` — old_ui sert à hériter
        // les réglages de panel (verbatim `UI ctor` lit `cur_ui`).
        let ui = {
            let prefs = self.prefs.borrow();
            let cur = self.uis.get(old_ui).and_then(|u| u.as_ref());
            Ui::new(&self.platform, &prefs, UI_MIN_W, UI_MIN_H, cur)
        };
        let id = self.next_handle;
        self.next_handle += 1;
        if self.uis.len() <= id {
            self.uis.resize_with(id + 1, || None);
        }
        self.uis[id] = Some(ui);

        // `new Layout(platform)` + attach SharedView ≙ viewport
        let p = Rc::clone(&self.platform);
        let pdyn: Rc<RefCell<dyn Platform>> = p;
        let limit = self.prefs.borrow().limit_text_width;
        let layout = Rc::new(RefCell::new(Layout::new(pdyn, limit)));
        let view = self.uis[id].as_ref().map_or_else(
            || Rc::new(RefCell::new(SkiaView::new(&self.platform, 1, 1))),
            |u| Rc::clone(&u.view),
        );
        layout
            .borrow_mut()
            .attach_view(Box::new(SharedView::new(&view)));
        // verbatim UIcmd_tab_new : options viewport depuis prefs
        {
            let prefs = self.prefs.borrow();
            let mut c = view.borrow_mut();
            // setScrollStep / setPageOverlap / drag scroll / scrollbar
            // side ≙ setters FltkViewport — exposés via Canvas.
            c.set_scroll_prefs(
                prefs.scroll_step,
                prefs.scroll_page_overlap,
                prefs.scrollbar_on_left,
                prefs.scrollbar_page_mode,
                prefs.middle_click_drags_page,
            );
        }
        self.layouts.insert(id, layout.clone());

        let mut title_ui = None;
        if let Some(w) = self.win_of_tabs(tabs) {
            let lw = |s: &str| text_w(s, 12);
            if w.tabs.add_new_tab(id, focus, lw) {
                // strip apparu — rien à faire côté modèle
            }
            if focus {
                title_ui = Some(id);
            }
        }
        let _ = title_ui;
        (id, layout)
    }

    fn tab_ui_remove(&mut self, ui: usize) {
        for w in self.wins.iter_mut().flatten() {
            if w.tabs.get_btn_idx(ui) >= 0 {
                w.tabs.remove_tab(ui);
                break;
            }
        }
        if let Some(u) = self.uis.get_mut(ui) {
            *u = None;
        }
        self.layouts.remove(&ui);
        // verbatim : `num_tabs()==0` → delete tabgroup + window
        // (tombstone — index stables) + effet de bord natif.
        for (i, slot) in self.wins.iter_mut().enumerate() {
            if slot.as_ref().is_some_and(|w| w.tabs.num_tabs() == 0) {
                *slot = None;
                self.app_ops.push_back(WinOp::Close { tabs: i });
            }
        }
    }

    fn tab_count(&self, tabs: usize) -> i32 {
        self.wins
            .get(tabs)
            .and_then(|w| w.as_ref())
            .map_or(0, |w| w.tabs.num_tabs() as i32)
    }

    fn tabs_of(&self, ui: usize) -> Option<usize> {
        self.wins
            .iter()
            .position(|w| w.as_ref().is_some_and(|w| w.tabs.get_btn_idx(ui) >= 0))
    }

    fn switch_tab(&mut self, tabs: usize, index: i32) {
        if let Some(w) = self.win_of_tabs(tabs) {
            let _ = w.tabs.switch_tab(index);
        }
    }

    fn switch_to_tab(&mut self, ui: usize) {
        if let Some(t) = self.tabs_mut_of(ui) {
            let idx = t.get_btn_idx(ui);
            let _ = t.switch_tab(idx);
        }
    }

    fn prev_tab(&mut self, tabs: usize) {
        if let Some(w) = self.win_of_tabs(tabs) {
            let _ = w.tabs.prev_tab();
        }
    }

    fn next_tab(&mut self, tabs: usize) {
        if let Some(w) = self.win_of_tabs(tabs) {
            let _ = w.tabs.next_tab();
        }
    }

    fn set_tab_label(&mut self, ui: usize, label: &str) {
        if let Some(t) = self.tabs_mut_of(ui) {
            t.set_tab_label(ui, label, |s| text_w(s, 12));
        }
    }

    fn is_active_tab(&self, ui: usize) -> bool {
        self.wins
            .iter()
            .flatten()
            .any(|w| w.tabs.value() == Some(ui))
    }

    fn tabs_active_ui(&self, tabs: usize) -> usize {
        self.wins
            .get(tabs)
            .and_then(|w| w.as_ref())
            .and_then(|w| w.tabs.value())
            .unwrap_or(0)
    }

    fn active_ui_handles(&self) -> Vec<usize> {
        self.wins
            .iter()
            .flatten()
            .filter_map(|w| w.tabs.value())
            .collect()
    }

    fn ui_contains(&self, ui: usize, wid: usize) -> bool {
        // `wid` = handle `ui` (nos widgets sont logiques, identifiés
        // par (ui, WId)) — `ui->contains(widget)` ≙ même onglet.
        let _ = wid;
        self.ui(ui).is_some()
    }

    fn ui_wh(&self, ui: usize) -> (i32, i32) {
        self.ui(ui).map_or((0, 0), |u| (u.rect.w, u.rect.h))
    }

    fn ui_label(&self, ui: usize) -> String {
        self.wins
            .iter()
            .flatten()
            .find_map(|w| {
                w.tabs
                    .pack
                    .iter()
                    .find(|b| b.ui == ui)
                    .map(|b| b.label.clone())
            })
            .unwrap_or_default()
    }

    fn ui_copy_label(&mut self, ui: usize, title: &str) {
        // ≙ `ui->copy_label` — le label interne = titre complet non
        // tronqué (sert au titre fenêtre dans switch_tab amont).
        if let Some(u) = self.ui_mut(ui) {
            u.main_label = title.to_string();
        }
    }

    fn set_window_title(&mut self, ui: usize, title: &str) {
        let tabs = self
            .wins
            .iter()
            .position(|w| w.as_ref().is_some_and(|w| w.tabs.get_btn_idx(ui) >= 0));
        if let Some(t) = tabs {
            if let Some(w) = self.win_of_tabs(t) {
                w.title = title.to_string();
            }
            self.app_ops.push_back(WinOp::SetTitle {
                tabs: t,
                title: title.to_string(),
            });
        }
    }

    fn focus_main(&mut self, ui: usize) {
        if let Some(u) = self.ui_mut(ui) {
            u.focus_main();
        }
    }

    fn focus_location(&mut self, ui: usize) {
        if let Some(u) = self.ui_mut(ui) {
            u.focus_location();
        }
    }

    fn get_location(&self, ui: usize) -> String {
        self.ui(ui).map_or_else(String::new, Ui::get_location)
    }

    fn set_location(&mut self, ui: usize, text: &str) {
        if let Some(u) = self.ui_mut(ui) {
            u.set_location(text);
        }
    }

    fn set_status(&mut self, ui: usize, msg: &str) {
        if let Some(u) = self.ui_mut(ui) {
            u.set_status(msg);
        }
    }

    fn set_page_prog(&mut self, ui: usize, nbytes: usize, cmd: i32) {
        if let Some(u) = self.ui_mut(ui) {
            u.set_page_prog(nbytes, cmd);
        }
    }

    fn set_img_prog(&mut self, ui: usize, n_img: i32, t_img: i32, cmd: i32) {
        if let Some(u) = self.ui_mut(ui) {
            u.set_img_prog(n_img, t_img, cmd);
        }
    }

    fn set_bug_prog(&mut self, ui: usize, n_bug: i32) {
        let (ok, bug) = (
            self.icons_std.imgs[IconKind::MeterOk as usize].clone(),
            self.icons_std.imgs[IconKind::MeterBug as usize].clone(),
        );
        if let (Some(ok), Some(bug)) = (ok, bug) {
            if let Some(u) = self.ui_mut(ui) {
                u.set_bug_prog(&ok, &bug, n_bug);
            }
        }
    }

    fn button_set_sens(&mut self, ui: usize, btn: UiButton, sens: bool) {
        if let Some(u) = self.ui_mut(ui) {
            u.button_set_sens(btn, sens);
        }
    }

    fn panels_toggle(&mut self, ui: usize) {
        if let Some(u) = self.ui_mut(ui) {
            u.panels_toggle();
        }
    }

    fn findbar_toggle(&mut self, ui: usize, on: bool) {
        if let Some(u) = self.ui_mut(ui) {
            u.findbar_toggle(on);
        }
    }

    // `a_Dialog_*` — modèle [`crate::dialog`] + runner injecté
    // (défaut `AutoCancel` ≙ fenêtre fermée immédiatement).
    fn dialog_input(&mut self, title: &str, msg: &str) -> Option<String> {
        let spec =
            crate::dialog::input_spec(&mut self.dialog_state, &self.prefs.borrow(), title, msg);
        match self.dialog_runner.run(&spec) {
            DialogAnswer::Input(s, idx) => {
                // verbatim : `prefs.search_url_idx = ch->value()` —
                // bool_t amont ≙ `!= 0`.
                self.prefs.borrow().search_url_idx.set(idx != 0);
                Some(s)
            }
            _ => None,
        }
    }
    fn dialog_passwd(&mut self, title: &str, msg: &str) -> Option<String> {
        let spec = crate::dialog::DialogSpec {
            kind: crate::dialog::DialogKind::Passwd,
            title: crate::dialog::default_title(&crate::dialog::DialogKind::Passwd, Some(title)),
            msg: msg.to_string(),
            alternatives: Vec::new(),
            pattern: None,
            suggested: None,
            text: String::new(),
            search_labels: Vec::new(),
            search_url_idx: 0,
            window_wh: (0, 0),
            font_factor: 0.0,
        };
        match self.dialog_runner.run(&spec) {
            DialogAnswer::Path(p) => p,
            _ => None,
        }
    }
    fn dialog_save_file(
        &mut self,
        title: &str,
        msg: Option<&str>,
        suggested: &str,
    ) -> Option<String> {
        let spec = crate::dialog::file_spec(
            crate::dialog::DialogKind::SaveFile,
            title,
            msg,
            Some(suggested),
        );
        match self.dialog_runner.run(&spec) {
            DialogAnswer::Path(p) => p,
            _ => None,
        }
    }
    fn dialog_select_file(
        &mut self,
        title: &str,
        msg: Option<&str>,
        init: Option<&str>,
    ) -> Option<String> {
        // verbatim : select ≙ même chemin que save.
        let spec =
            crate::dialog::file_spec(crate::dialog::DialogKind::SelectFile, title, msg, init);
        match self.dialog_runner.run(&spec) {
            DialogAnswer::Path(p) => p,
            _ => None,
        }
    }
    fn dialog_open_file(
        &mut self,
        title: &str,
        msg: Option<&str>,
        pattern: &str,
    ) -> Option<String> {
        let spec = crate::dialog::file_spec(
            crate::dialog::DialogKind::OpenFile,
            title,
            Some(pattern),
            msg,
        );
        match self.dialog_runner.run(&spec) {
            // verbatim : `a_Misc_escape_chars(fc_name, "% #")`.
            DialogAnswer::Path(p) => crate::dialog::open_file_escape(p.as_deref()),
            _ => None,
        }
    }
    fn dialog_text_window(&mut self, title: &str, txt: &str) {
        let spec = crate::dialog::text_window_spec(title, txt, &self.prefs.borrow());
        let _ = self.dialog_runner.run(&spec);
    }
    fn dialog_msg(&mut self, title: &str, msg: &str) {
        let spec = crate::dialog::DialogSpec {
            kind: crate::dialog::DialogKind::Msg,
            title: crate::dialog::default_title(&crate::dialog::DialogKind::Msg, Some(title)),
            msg: msg.to_string(),
            alternatives: Vec::new(),
            pattern: None,
            suggested: None,
            text: String::new(),
            search_labels: Vec::new(),
            search_url_idx: 0,
            window_wh: (0, 0),
            font_factor: 0.0,
        };
        let _ = self.dialog_runner.run(&spec);
    }
    fn dialog_choice(&mut self, title: &str, msg: &str, alts: &[&str]) -> i32 {
        match crate::dialog::choice_spec(title, msg, alts) {
            Some(spec) => match self.dialog_runner.run(&spec) {
                DialogAnswer::Choice(n) => n,
                _ => 0, // verbatim : fenêtre fermée → choice_answer = 0
            },
            None => 0, // verbatim : pas d'alternatives → 0
        }
    }

    fn history_popup(&mut self, bw: u32, list: Vec<i32>, x: i32, y: i32, dir: i32) {
        menu::history_popup(&mut self.menus, bw, x, y, dir, list);
    }
    // `a_Menu_*_popup` — verbatim menu.cc : `popup_x/y =
    // Fl::event_x/y` ≙ `self.event_xy` (posé par l'app à chaque
    /// event souris). Tous poussent dans `menus.pending` ≙
    /// `a_Timeout_add(0.0, Menu_popup_cb)`.
    fn page_popup(
        &mut self,
        bw: u32,
        url: Option<&DilloUrl>,
        has_bugs: bool,
        css_urls: &[DilloUrl],
    ) {
        let (x, y) = self.event_xy;
        menu::page_popup(
            &mut self.menus,
            &self.page_actions,
            bw,
            x,
            y,
            url,
            has_bugs,
            css_urls,
        );
    }
    fn link_popup(&mut self, bw: u32, url: &DilloUrl, page_url: Option<&DilloUrl>) {
        let (x, y) = self.event_xy;
        menu::link_popup(&mut self.menus, &self.link_actions, bw, x, y, url, page_url);
    }
    fn image_popup(
        &mut self,
        bw: u32,
        url: &DilloUrl,
        loaded_img: bool,
        page_url: Option<&DilloUrl>,
        link_url: Option<&DilloUrl>,
    ) {
        let (x, y) = self.event_xy;
        menu::image_popup(
            &mut self.menus,
            &self.link_actions,
            bw,
            x,
            y,
            url,
            loaded_img,
            page_url,
            link_url,
        );
    }
    fn form_popup(&mut self, bw: u32, url: &DilloUrl, form: usize, showing_hiddens: bool) {
        let (x, y) = self.event_xy;
        menu::form_popup(&mut self.menus, bw, x, y, url, form, showing_hiddens);
    }
    fn file_popup(&mut self, bw: u32, x: i32, y: i32) {
        menu::file_popup(&mut self.menus, &self.keys, bw, x, y);
    }
    fn bugmeter_popup(&mut self, bw: u32, url: Option<&DilloUrl>) {
        let (x, y) = self.event_xy;
        menu::bugmeter_popup(&mut self.menus, bw, x, y, url);
    }
    fn tools_popup(&mut self, bw: u32, ui: usize, x: i32, y: i32) {
        // verbatim : toggles depuis prefs + radio panel_size/setonly.
        let (size, icons) = self.panel_state(ui);
        menu::tools_popup(&mut self.menus, &self.prefs.borrow(), bw, x, y, size, icons);
    }
    fn panel_state(&self, ui: usize) -> (i32, bool) {
        self.ui(ui)
            .map_or((0, false), |u| (u.panel_size, u.small_icons))
    }
    fn panel_change(&mut self, ui: usize, size: i32, small_icons: bool) {
        let prefs = self.prefs.clone();
        if let Some(u) = self.ui_mut(ui) {
            u.change_panel(&prefs.borrow(), size, small_icons);
        }
    }

    fn old_working_dir(&self) -> Vec<u8> {
        self.old_cwd.clone()
    }
    fn control_is_waiting(&self) -> bool {
        self.control
            .as_ref()
            .is_some_and(|c| c.borrow().is_waiting())
    }
    fn control_notify_finish(&mut self, bw: u32) {
        // verbatim `a_Control_notify_finish` — la réponse `0\n`
        // est écrite au prochain `Control::poll` (writable).
        if let Some(c) = &self.control {
            let _ = c.borrow_mut().notify_finish(bw);
        }
    }
    fn add_bookmark(&mut self, bw: u32, url: &DilloUrl) {
        self.bookmarks.add(bw, &mut url.clone());
    }
    fn flush(&mut self) {
        // ≙ `Fl::flush` — redraw de toutes les fenêtres vivantes.
        self.app_ops.push_back(WinOp::RedrawAll);
    }
    fn defer_nav(&mut self, bw: u32, op: NavOp) {
        self.deferred_nav.push((bw, op));
    }
    fn defer_close_all(&mut self) {
        self.deferred_close_all = true;
    }
}

/// `NavSched` est implémenté par `Chrome` (drain via `drain_nav`).
impl NavSched for Chrome {
    fn defer(&mut self, bw: u32, op: NavOp) {
        self.deferred_nav.push((bw, op));
    }
}

// -- événements (UI::handle / CustTabs::handle / callbacks) ----------------------

/// Résultat d'un hit sur le chrome — à dispatcher par l'appelant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// Bouton d'onglet (index pack).
    Tab(usize),
    /// `CloseBtn` du strip.
    TabClose,
    /// Widget chrome.
    Wid(WId),
    /// Sous-widget du findbar (`None` = zone morte du groupe).
    Findbar(Option<FindHit>),
    /// Zone `Main` (viewport).
    Main,
    /// Rien.
    None,
}

impl Ui {
    /// Hit-test complet (strip géré par `Tabs` côté Chrome).
    #[must_use]
    pub fn hit(&self, x: i32, y: i32) -> Hit {
        if let Some(w) = self.widget_at(x, y) {
            Hit::Wid(w)
        } else if self.findbar_visible && self.findbar.contains(x, y) {
            // verbatim : dispatch interne du `Fl_Group` findbar.
            let rel = (x - self.findbar.x, y - self.findbar.y);
            Hit::Findbar(crate::findbar::hit(&self.findbar_layout(), rel))
        } else if self.main.contains(x, y) {
            Hit::Main
        } else {
            Hit::None
        }
    }

    /// Rect courant du widget `id` (cherche dans les barres ≙
    /// `ui->contains`/`widget->x/y/w/h` amont).
    #[must_use]
    pub fn rect_of(&self, id: WId) -> Option<Rect> {
        for g in [
            self.loc_bar.as_ref(),
            self.nav_bar.as_ref(),
            Some(&self.status_bar),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(i) = g.find(id) {
                return Some(g.children[i].rect);
            }
        }
        None
    }

    /// Sous-rects du findbar (coordonnées relatives au rect —
    /// verbatim constructeur `Findbar(width, height)`).
    #[must_use]
    pub fn findbar_layout(&self) -> FindbarLayout {
        Findbar::layout(self.findbar.w, self.findbar.h)
    }
}

/// `location_cb` verbatim — le texte courant ouvre l'URL ; le panel
/// temporaire se replie (`temporaryPanels` → `panels_toggle`).
pub fn location_commit(
    uicmd: &UiCmd,
    reg: &BwRegistry,
    shell: &mut Chrome,
    prefs: &Prefs,
    bw: u32,
) {
    let ui = reg.get(bw).map_or(0, |b| b.ui.get());
    let text = shell.get_location(ui);
    uicmd.open_urlstr(reg, shell, prefs, bw, text.as_bytes());
    if let Some(u) = shell.ui_mut(ui) {
        if u.panel_temporary {
            u.panel_temporary = false;
            u.panels_toggle();
        }
    }
}

/// `UI::handle` `FL_SHORTCUT` (verbatim ui.cc:794-863) — dispatch
/// `KeysCommand` vers `a_UIcmd_*`. Retourne `true` si consommé.
pub fn ui_shortcut(
    uicmd: &UiCmd,
    reg: &BwRegistry,
    shell: &mut Chrome,
    prefs: &Prefs,
    bw: u32,
    cmd: KeysCommand,
) -> bool {
    use KeysCommand as K;
    let Some(b) = reg.get(bw) else { return false };
    let ui = b.ui.get();
    match cmd {
        K::Nop | K::Invalid => {}
        K::ScreenUp
        | K::ScreenDown
        | K::ScreenLeft
        | K::ScreenRight
        | K::LineUp
        | K::LineDown
        | K::Left
        | K::Right
        | K::Top
        | K::Bottom => {
            uicmd.scroll(reg, bw, cmd);
        }
        K::Back => uicmd.back(reg, shell, prefs, bw),
        K::Forward => uicmd.forw(reg, shell, prefs, bw),
        K::Copy => uicmd.copy(reg, bw),
        K::ZoomIn => uicmd.zoom_in(reg, shell, prefs, bw),
        K::ZoomOut => uicmd.zoom_out(reg, shell, prefs, bw),
        K::ZoomReset => uicmd.zoom_reset(reg, shell, prefs, bw),
        K::Bookmarks => uicmd.book(reg, shell, prefs, bw, false),
        K::Find => uicmd.findbar_toggle(reg, shell, bw, true),
        K::WebSearch => {
            uicmd.search_dialog(reg, shell, prefs, bw);
        }
        K::Goto => uicmd.focus_location(reg, shell, bw),
        K::HidePanels => {
            /* Hide findbar if present, hide panels if not */
            if let Some(u) = shell.ui_mut(ui) {
                if u.findbar_visible {
                    uicmd.findbar_toggle(reg, shell, bw, false);
                } else {
                    uicmd.panels_toggle(reg, shell, bw);
                }
                if let Some(u) = shell.ui_mut(ui) {
                    u.panel_temporary = false;
                }
            }
        }
        K::Open => uicmd.open_file(reg, shell, prefs, bw),
        K::Home => uicmd.home(reg, shell, prefs, bw, false),
        K::Reload => uicmd.reload(reg, shell, prefs, bw),
        K::Stop => uicmd.stop(reg, shell, prefs, bw),
        K::Save => uicmd.save_page(reg, shell, bw),
        K::FileMenu => {
            // verbatim : `a_UIcmd_file_popup(bw, FileButton)` — popup
            // au coin bas-gauche du bouton File.
            if let Some(r) = shell.ui(ui).and_then(|u| u.rect_of(WId::FileMenu)) {
                uicmd.file_popup(shell, bw, r.x, r.y + r.h);
            }
        }
        K::ViewSource => {
            let url = reg
                .get(bw)
                .and_then(|b| dillo_net::history::history_get_url(b.nav.borrow().get_top_uidx()));
            if let Some(url) = url {
                uicmd.view_page_source(reg, shell, prefs, bw, &url);
            }
        }
        k if (K::FocusTab1 as i32..=K::FocusTab10 as i32).contains(&(k as i32)) => {
            let index = (k as i32) - (K::FocusTab1 as i32); /* zero-based */
            uicmd.focus_tab(reg, shell, bw, index);
        }
        _ => return false,
    }
    true
}

/// `CustInput::handle` (verbatim ui.cc:154-203) — filtre clavier de
/// la barre de location. `key` = keysym FLTK-like, `mods` =
/// shift/ctrl/alt. Retourne `true` si consommé, `false` → remonter.
pub fn cust_input_key(
    uicmd: &UiCmd,
    reg: &BwRegistry,
    shell: &mut Chrome,
    prefs: &Prefs,
    bw: u32,
    keysym: i32,
    text: Option<&str>,
    mods: i32,
) -> bool {
    use crate::keys::keysym;
    const FL_SHIFT: i32 = 1;
    const FL_CTRL: i32 = 4;
    const FL_ALT: i32 = 8;
    let modifier = mods & (FL_SHIFT | FL_CTRL | FL_ALT);
    let Some(ui_id) = reg.get(bw).map(|b| b.ui.get()) else {
        return false;
    };
    // La décision « laisser remonter » est calculée avant d'emprunter
    // `u` (verbat.) — `focus_main_area` re-emprunte `shell`.
    let forward_to_ui = (keysym == keysym::ESCAPE && modifier == 0)
        || (modifier == FL_SHIFT && (keysym == keysym::LEFT || keysym == keysym::RIGHT))
        || (modifier == FL_CTRL
            && matches!(keysym, k if k == 'h' as i32 || k == 'o' as i32
                || k == 'r' as i32 || k == keysym::HOME || k == keysym::END))
        || (modifier == 0
            && matches!(keysym, k if k == keysym::DOWN || k == keysym::UP || k == keysym::PAGE_DOWN
                || k == keysym::PAGE_UP || k == keysym::TAB));
    if forward_to_ui {
        if modifier == 0
            && matches!(keysym, k if k == keysym::DOWN || k == keysym::UP || k == keysym::PAGE_DOWN
                || k == keysym::PAGE_UP || k == keysym::TAB)
        {
            // Give up focus and honor the key
            uicmd.focus_main_area(reg, shell, bw);
        }
        return false;
    }
    let platform = Rc::clone(shell.platform_rc());
    let Some(u) = shell.ui_mut(ui_id) else {
        return false;
    };

    if keysym == keysym::ESCAPE && modifier == 0 {
        // Let the parent group handle this Esc key
        return false;
    } else if modifier == FL_SHIFT {
        if keysym == keysym::LEFT || keysym == keysym::RIGHT {
            // Let these keys get to the UI
            return false;
        }
    } else if modifier == FL_CTRL {
        if keysym == 'e' as i32 {
            let n = u.location.size();
            u.location.set_position(n, n);
            return true;
        } else if keysym == 'k' as i32 {
            let n = u.location.size();
            u.location.cut(u.location.pos, n);
            return true;
        } else if keysym == 'd' as i32 {
            let (p, n) = (u.location.pos, u.location.size());
            u.location.cut(p, (p + 1).min(n));
            return true;
        } else if keysym == 'a' as i32 || keysym == 'l' as i32 {
            // Make text selected when already focused.
            let n = u.location.size();
            u.location.set_position(n, 0);
            return true;
        } else if keysym == 'h' as i32
            || keysym == 'o' as i32
            || keysym == 'r' as i32
            || keysym == keysym::HOME
            || keysym == keysym::END
        {
            // Let these keys get to the UI
            return false;
        }
    }
    if keysym == keysym::PAGE_DOWN || keysym == keysym::PAGE_UP {
        // These do nothing of interest when FL_MULTILINE_INPUT isn't set.
        return false;
    }
    // ≙ TipWinInput::handle — édition + Enter → location_cb
    if keysym == keysym::ENTER || keysym == keysym::KP_ENTER {
        location_commit(uicmd, reg, shell, prefs, bw);
        return true;
    }
    // `Fl::copy`/`Fl::paste` ≙ clipboard plateforme (OS via arboard).
    let mut copy = {
        let platform = Rc::clone(&platform);
        move |s: &str| platform.borrow_mut().copy_selection(s.as_bytes(), 1)
    };
    let mut paste = move || {
        let t = platform.borrow().clipboard_text();
        if t.is_empty() {
            None
        } else {
            String::from_utf8(t).ok()
        }
    };
    line_input_key(
        &mut u.location,
        keysym,
        text,
        false,
        false,
        &mut copy,
        &mut paste,
    )
}

// -- dessin (≙ `Fl_Widget::draw` des widgets du chrome) ------------------------

/// `dw` Color i32 → `tiny_skia::Color` opaque.
fn skc(rgb: u32) -> dillo_platform::tiny_skia::Color {
    dillo_platform::tiny_skia::Color::from_rgba8(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
        255,
    )
}

/// Remplit un rect du pixmap (`fl_rectf`).
fn pm_fill(pm: &mut dillo_platform::tiny_skia::Pixmap, r: Rect, rgb: u32) {
    use dillo_platform::tiny_skia::{Paint, Path, PathBuilder, Transform};
    let Some(tr) =
        dillo_platform::tiny_skia::Rect::from_xywh(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
    else {
        return;
    };
    let mut b = PathBuilder::new();
    b.push_rect(tr);
    let Some(path): Option<Path> = b.finish() else {
        return;
    };
    pm.fill_path(
        &path,
        &Paint {
            shader: dillo_platform::tiny_skia::Shader::SolidColor(skc(rgb)),
            ..Paint::default()
        },
        dillo_platform::tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
}

/// `fl_rect` — bordure 1px.
fn pm_rect(pm: &mut dillo_platform::tiny_skia::Pixmap, r: Rect, rgb: u32) {
    use dillo_platform::tiny_skia::{Paint, PathBuilder, Stroke, Transform};
    let Some(tr) = dillo_platform::tiny_skia::Rect::from_xywh(
        r.x as f32 + 0.5,
        r.y as f32 + 0.5,
        r.w as f32 - 1.0,
        r.h as f32 - 1.0,
    ) else {
        return;
    };
    let mut b = PathBuilder::new();
    b.push_rect(tr);
    let Some(path) = b.finish() else { return };
    pm.stroke_path(
        &path,
        &Paint {
            shader: dillo_platform::tiny_skia::Shader::SolidColor(skc(rgb)),
            ..Paint::default()
        },
        &Stroke::default(),
        Transform::identity(),
        None,
    );
}

/// Nuance `rgb` (`Color::shade` ≙ `fl_color_average`) — `fac` >1
/// éclaircit vers blanc, <1 assombrit vers noir (convention dw).
fn shade(rgb: u32, fac: f64) -> u32 {
    let f = |v: u32| -> u32 {
        if fac >= 1.0 {
            (f64::from(v) + (255.0 - f64::from(v)) * (fac - 1.0).min(1.0)).round() as u32
        } else {
            (f64::from(v) * fac.max(0.0)).round() as u32
        }
    };
    (f((rgb >> 16) & 0xff) << 16) | (f((rgb >> 8) & 0xff) << 8) | f(rgb & 0xff)
}

/// Cadre 3D 2px style FLTK (`FL_UP_BOX` / `FL_DOWN_BOX`) : up =
/// light haut-gauche / dark bas-droite, down = inversé.
fn pm_box(pm: &mut dillo_platform::tiny_skia::Pixmap, r: Rect, bg: u32, down: bool) {
    pm_fill(pm, r, bg);
    let (lt, dk) = if down {
        (shade(bg, 0.55), shade(bg, 1.55))
    } else {
        (shade(bg, 1.55), shade(bg, 0.55))
    };
    // bord haut + gauche = lt, bas + droite = dk
    pm_fill(
        pm,
        Rect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: 1,
        },
        lt,
    );
    pm_fill(
        pm,
        Rect {
            x: r.x,
            y: r.y,
            w: 1,
            h: r.h,
        },
        lt,
    );
    pm_fill(
        pm,
        Rect {
            x: r.x,
            y: r.y + r.h - 1,
            w: r.w,
            h: 1,
        },
        dk,
    );
    pm_fill(
        pm,
        Rect {
            x: r.x + r.w - 1,
            y: r.y,
            w: 1,
            h: r.h,
        },
        dk,
    );
    // deuxième pixel (FL_UP_BOX a 2px)
    pm_fill(
        pm,
        Rect {
            x: r.x + 1,
            y: r.y + 1,
            w: r.w - 2,
            h: 1,
        },
        if down { dk } else { shade(bg, 1.15) },
    );
    pm_fill(
        pm,
        Rect {
            x: r.x + 1,
            y: r.y + 1,
            w: 1,
            h: r.h - 2,
        },
        if down { dk } else { shade(bg, 1.15) },
    );
    pm_fill(
        pm,
        Rect {
            x: r.x,
            y: r.y + r.h - 2,
            w: r.w,
            h: 1,
        },
        if down { lt } else { shade(bg, 0.75) },
    );
    pm_fill(
        pm,
        Rect {
            x: r.x + r.w - 2,
            y: r.y,
            w: 1,
            h: r.h,
        },
        if down { lt } else { shade(bg, 0.75) },
    );
}

/// Dessine du texte (`fl_draw`) — `px` taille, couleur `0xRRGGBB`,
/// centré dans `r` (align `1` = gauche).
fn pm_text(
    pm: &mut dillo_platform::tiny_skia::Pixmap,
    fs: &mut dillo_platform::cosmic_text::FontSystem,
    sw: &mut dillo_platform::cosmic_text::SwashCache,
    r: Rect,
    text: &str,
    px: i32,
    rgb: u32,
    left: bool,
) {
    use dillo_platform::cosmic_text::{Attrs, Buffer, Color as CColor, Metrics, Shaping, Wrap};
    let size = px.max(1) as f32;
    let mut buf = Buffer::new(fs, Metrics::new(size, size * 1.2));
    buf.set_wrap(fs, Wrap::None);
    buf.set_size(fs, Some(r.w.max(1) as f32), Some(r.h.max(1) as f32));
    let text = dillo_platform::platform::sanitize_shape_text(text);
    buf.set_text(fs, &text, Attrs::new(), Shaping::Advanced);
    buf.shape_until_scroll(fs, true);
    // Hauteur de ligne pour centrage vertical.
    let lh = buf
        .layout_runs()
        .map(|run| run.line_height)
        .fold(0.0f32, f32::max);
    let tw = buf
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0f32, f32::max);
    let ox = if left {
        2.0
    } else {
        ((r.w as f32 - tw) / 2.0).max(0.0)
    };
    let oy = ((r.h as f32 - lh) / 2.0).max(0.0);
    let cc = CColor::rgba(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
        255,
    );
    buf.draw(fs, sw, cc, |px_, py_, gw, gh, col| {
        let pc = col.as_rgba();
        for dy in 0..gh as i32 {
            for dx in 0..gw as i32 {
                let (x, y) = (
                    r.x + (ox + px_ as f32) as i32 + dx,
                    r.y + (oy + py_ as f32) as i32 + dy,
                );
                if x < r.x
                    || y < r.y
                    || x >= r.x + r.w
                    || y >= r.y + r.h
                    || x < 0
                    || y < 0
                    || x >= pm.width() as i32
                    || y >= pm.height() as i32
                {
                    continue;
                }
                let idx = (y as u32 * pm.width() + x as u32) as usize;
                if let Some(dst) = pm.pixels_mut().get_mut(idx) {
                    // Source-over ≙ `blend_pixel` de `SkiaView` : la
                    // couverture AA est semi-transparente — remplacer
                    // `dst` laisserait `fg*α` sans le fond, affiché fondu
                    // sur noir au present opaque.
                    let a = u16::from(pc[3]);
                    let inv = 255 - a;
                    let r = (u16::from(pc[0]) * a / 255 + u16::from(dst.red()) * inv / 255) as u8;
                    let g = (u16::from(pc[1]) * a / 255 + u16::from(dst.green()) * inv / 255) as u8;
                    let b = (u16::from(pc[2]) * a / 255 + u16::from(dst.blue()) * inv / 255) as u8;
                    if let Some(p) =
                        dillo_platform::tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, 255)
                    {
                        *dst = p;
                    }
                }
            }
        }
    });
}

/// Buffer cosmic mono-ligne identique au `pm_text` du `CustInput`
/// (mêmes métriques/wrap) — caret, sélection et hit-test de la
/// barre d'adresse mesurés sur les glyphes réels (≙ `fl_width` +
/// hit-test de `Fl_Input::handle_mouse`), pas sur une avance fixe.
pub(crate) fn loc_buf(
    fs: &mut dillo_platform::cosmic_text::FontSystem,
    text: &str,
    px: i32,
    w: i32,
) -> dillo_platform::cosmic_text::Buffer {
    use dillo_platform::cosmic_text::{Attrs, Buffer, Metrics, Shaping, Wrap};
    let size = px.max(1) as f32;
    let mut buf = Buffer::new(fs, Metrics::new(size, size * 1.2));
    buf.set_wrap(fs, Wrap::None);
    buf.set_size(fs, Some(w.max(1) as f32), Some(size * 1.2));
    let text = dillo_platform::platform::sanitize_shape_text(text);
    buf.set_text(fs, &text, Attrs::new(), Shaping::Advanced);
    buf.shape_until_scroll(fs, true);
    buf
}

/// X du caret (origine glyphes, ≙ `fl_width(text[..pos])`) pour
/// `pos` en octets dans la ligne shapée — bord gauche du glyphe
/// contenant `pos`, fin de ligne si `pos` dépasse tout.
pub(crate) fn loc_x(buf: &dillo_platform::cosmic_text::Buffer, pos: usize) -> f32 {
    if let Some(run) = buf.layout_runs().next() {
        let mut x = 0.0;
        for g in run.glyphs {
            if pos < g.end {
                return g.x;
            }
            x = g.x + g.w;
        }
        return x;
    }
    0.0
}

/// Span `(x,w)` de la sélection `[a,b)` (octets) sur les glyphes
/// réels — ≙ rectangle de surlignage FLTK.
pub(crate) fn loc_sel_span(
    buf: &dillo_platform::cosmic_text::Buffer,
    a: usize,
    b: usize,
) -> Option<(f32, f32)> {
    use dillo_platform::cosmic_text::Cursor;
    buf.layout_runs()
        .next()
        .and_then(|run| run.highlight(Cursor::new(0, a), Cursor::new(0, b)))
}

/// Index octet sous `x` (origine glyphes) — ≙ hit-test
/// `Fl_Input::handle_mouse` via `Buffer::hit`.
pub(crate) fn loc_index_at(buf: &dillo_platform::cosmic_text::Buffer, x: f32) -> usize {
    let mid = buf
        .layout_runs()
        .next()
        .map_or(0.0, |r| r.line_top + r.line_height / 2.0);
    buf.hit(x, mid)
        .map_or(if x < 0.0 { 0 } else { usize::MAX }, |c| c.index)
}

// -- overlay popup (≙ `Fl_Menu` — déviation ADR-0002 : dessiné
//    dans le back-buffer de la fenêtre au lieu d'une fenêtre
//    native, modale côté dispatch) --------------------------------

/// Métriques lignes (≙ `Fl_Menu_Item` : textsize 12, hauteur
/// textsize+10 ≈ FLTK par défaut).
const POP_FONT: i32 = 12;
/// Hauteur d'une ligne item.
const POP_ROW_H: i32 = 22;
/// Surhauteur d'un item `divider` (filet sous l'item ≙
/// `FL_MENU_DIVIDER`).
const POP_DIV_H: i32 = 7;
/// Marge latérale externe.
const POP_PAD_X: i32 = 6;
/// Colonne check/radio.
const POP_CHECK_W: i32 = 20;
/// Écart label↔shortcut.
const POP_SHORT_GAP: i32 = 16;
/// Colonne `>` sous-menu.
const POP_SUB_W: i32 = 14;

/// ≙ `fl_shortcut_label` — `Ctrl+T`, `Shift+F5`…
fn shortcut_label(sc: i32) -> String {
    use crate::keys::{mods, Keys};
    let mut s = String::new();
    if sc & mods::CTRL != 0 {
        s.push_str("Ctrl+");
    }
    if sc & mods::ALT != 0 {
        s.push_str("Alt+");
    }
    if sc & mods::SHIFT != 0 {
        s.push_str("Shift+");
    }
    if sc & mods::META != 0 {
        s.push_str("Meta+");
    }
    s.push_str(&Keys::key_name(
        sc & !(mods::SHIFT
            | mods::CTRL
            | mods::ALT
            | mods::META
            | mods::BUTTON1
            | mods::BUTTON2
            | mods::BUTTON3),
    ));
    s
}

/// Largeur de texte mesurée sur glyphes réels (≙ `fl_width` —
/// contrairement à `text_w`, approximation pour la géométrie
/// initiale des widgets).
fn glyph_w(fs: &mut dillo_platform::cosmic_text::FontSystem, s: &str, px: i32) -> i32 {
    if s.is_empty() {
        return 0;
    }
    let buf = loc_buf(fs, s, px, 10_000);
    buf.layout_runs()
        .map(|r| r.line_w)
        .fold(0.0, f32::max)
        .ceil() as i32
}

/// Géométrie du popup `items` ancré en `(x, y)` clampé dans la
/// fenêtre `(max_w, max_h)` — partagée par draw et hit-test :
/// `(outer, rows)` où `rows[i]` est la ligne de `items[i]` (le
/// filet d'un `divider` occupe la fin de sa ligne).
fn popup_geom(
    fs: &mut dillo_platform::cosmic_text::FontSystem,
    items: &[menu::MenuItem],
    x: i32,
    y: i32,
    max_w: i32,
    max_h: i32,
) -> (Rect, Vec<Rect>) {
    let mut w = 40;
    for it in items {
        let lw = glyph_w(fs, &it.label, POP_FONT);
        let sw = if it.shortcut != 0 {
            POP_SHORT_GAP + glyph_w(fs, &shortcut_label(it.shortcut), POP_FONT)
        } else {
            0
        };
        let sub = if it.submenu.is_some() { POP_SUB_W } else { 0 };
        w = w.max(POP_PAD_X + POP_CHECK_W + lw + sw + sub + POP_PAD_X);
    }
    let mut rows = Vec::with_capacity(items.len());
    let mut cy = y;
    for it in items {
        let h = POP_ROW_H + if it.divider { POP_DIV_H } else { 0 };
        rows.push(Rect { x, y: cy, w, h });
        cy += h;
    }
    // Clamp dans la fenêtre (≙ `Fl_Menu` qui reste à l'écran).
    let dx = (x + w - max_w).max(0).min(x);
    let dy = (cy - max_h).max(0).min(y);
    for r in &mut rows {
        r.x -= dx;
        r.y -= dy;
    }
    (
        Rect {
            x: x - dx,
            y: y - dy,
            w,
            h: cy - y,
        },
        rows,
    )
}

/// `(item, sub)` sous `(x, y)` — `sub` = `Some(j)` dans le
/// sous-menu ouvert `sub_open`. `None` = hors du menu (clic dehors
/// ≙ fermeture).
pub(crate) fn popup_item_at(
    fs: &mut dillo_platform::cosmic_text::FontSystem,
    pop: &menu::Popup,
    sub_open: Option<usize>,
    x: i32,
    y: i32,
    max_w: i32,
    max_h: i32,
) -> Option<(usize, Option<usize>)> {
    let (outer, rows) = popup_geom(fs, &pop.items, pop.x, pop.y, max_w, max_h);
    // Sous-menu d'abord (au-dessus du parent).
    if let Some(p) = sub_open {
        if let Some(subs) = pop.items.get(p).and_then(|i| i.submenu.as_ref()) {
            if let Some(pr) = rows.get(p) {
                let (_, srows) = popup_geom(fs, subs, outer.x + outer.w, pr.y, max_w, max_h);
                for (j, r) in srows.iter().enumerate() {
                    if r.contains(x, y) {
                        return Some((p, Some(j)));
                    }
                }
            }
        }
    }
    for (i, r) in rows.iter().enumerate() {
        // La zone divider (bas de ligne) ne sélectionne pas.
        let selectable_h = if pop.items[i].divider { POP_ROW_H } else { r.h };
        if x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + selectable_h {
            return Some((i, None));
        }
    }
    None
}

/// Dessin de l'overlay (≙ `Fl_Menu_::draw`) — fond, items,
/// coches/radios, shortcuts, `>` sous-menus, dividers, surlignage
/// de `hover`/`armed`, puis le sous-menu ouvert.
#[allow(clippy::too_many_arguments)]
fn draw_popup(
    pm: &mut dillo_platform::tiny_skia::Pixmap,
    fs: &mut dillo_platform::cosmic_text::FontSystem,
    sw: &mut dillo_platform::cosmic_text::SwashCache,
    pop: &menu::Popup,
    sub_open: Option<usize>,
    sel: Option<(usize, Option<usize>)>,
    max_w: i32,
    max_h: i32,
) {
    let (outer, rows) = popup_geom(fs, &pop.items, pop.x, pop.y, max_w, max_h);
    let bg = 0x00f0_f0f0;
    pm_fill(pm, outer, bg);
    pm_rect(pm, outer, shade(bg, 0.6));
    for (i, it) in pop.items.iter().enumerate() {
        let r = rows[i];
        let row = Rect {
            h: POP_ROW_H.min(r.h),
            ..r
        };
        let hot = sel == Some((i, None));
        if hot && it.active {
            pm_fill(pm, row, 0x002e_5a9b); // ≙ FL_SELECTION_COLOR
        }
        let fg = if !it.active {
            shade(bg, 0.45)
        } else if hot {
            0x00ff_ffff
        } else {
            0
        };
        let tx = Rect {
            x: row.x + POP_PAD_X + POP_CHECK_W,
            y: row.y,
            w: (row.w - POP_PAD_X * 2 - POP_CHECK_W - POP_SUB_W).max(1),
            h: row.h,
        };
        // check / radio
        if it.set && it.toggle {
            pm_text(
                pm,
                fs,
                sw,
                Rect {
                    x: row.x + 2,
                    w: POP_CHECK_W,
                    ..row
                },
                "\u{2713}",
                POP_FONT,
                fg,
                true,
            );
        } else if it.set && it.radio {
            pm_text(
                pm,
                fs,
                sw,
                Rect {
                    x: row.x + 2,
                    w: POP_CHECK_W,
                    ..row
                },
                "\u{25cf}",
                POP_FONT - 4,
                fg,
                true,
            );
        }
        pm_text(pm, fs, sw, tx, &it.label, POP_FONT, fg, true);
        if it.shortcut != 0 {
            let sl = shortcut_label(it.shortcut);
            let swd = glyph_w(fs, &sl, POP_FONT);
            pm_text(
                pm,
                fs,
                sw,
                Rect {
                    x: row.x + row.w - POP_PAD_X - POP_SUB_W - swd,
                    w: swd,
                    ..row
                },
                &sl,
                POP_FONT,
                fg,
                true,
            );
        }
        if it.submenu.is_some() {
            pm_text(
                pm,
                fs,
                sw,
                Rect {
                    x: row.x + row.w - POP_SUB_W,
                    w: POP_SUB_W,
                    ..row
                },
                "\u{203a}",
                POP_FONT,
                fg,
                true,
            );
        }
        if it.divider {
            let dy = r.y + POP_ROW_H + POP_DIV_H / 2;
            pm_fill(
                pm,
                Rect {
                    x: r.x + POP_PAD_X,
                    y: dy,
                    w: (r.w - POP_PAD_X * 2).max(0),
                    h: 1,
                },
                shade(bg, 0.6),
            );
        }
    }
    // Sous-menu ouvert : ancré au bord droit de la ligne parente
    // (≙ `Fl_Menu` sous-menu adjacent). Un seul niveau — les menus
    // amont n'en ouvrent jamais davantage.
    if let Some(p) = sub_open {
        if let Some(subs) = pop.items.get(p).and_then(|i| i.submenu.as_ref()) {
            if let Some(pr) = rows.get(p) {
                let sub_sel = sel.and_then(|(i, s)| if i == p { s } else { None });
                draw_sub_popup(
                    pm,
                    fs,
                    sw,
                    subs,
                    outer.x + outer.w,
                    pr.y,
                    sub_sel,
                    max_w,
                    max_h,
                );
            }
        }
    }
}

/// Dessin d'une liste de sous-menu (≙ même `Fl_Menu` à un niveau).
#[allow(clippy::too_many_arguments)]
fn draw_sub_popup(
    pm: &mut dillo_platform::tiny_skia::Pixmap,
    fs: &mut dillo_platform::cosmic_text::FontSystem,
    sw: &mut dillo_platform::cosmic_text::SwashCache,
    items: &[menu::MenuItem],
    x: i32,
    y: i32,
    sel: Option<usize>,
    max_w: i32,
    max_h: i32,
) {
    let (outer, rows) = popup_geom(fs, items, x, y, max_w, max_h);
    let bg = 0x00f0_f0f0;
    pm_fill(pm, outer, bg);
    pm_rect(pm, outer, shade(bg, 0.6));
    for (i, it) in items.iter().enumerate() {
        let row = Rect {
            h: POP_ROW_H.min(rows[i].h),
            ..rows[i]
        };
        let hot = sel == Some(i);
        if hot && it.active {
            pm_fill(pm, row, 0x002e_5a9b);
        }
        let fg = if !it.active {
            shade(bg, 0.45)
        } else if hot {
            0x00ff_ffff
        } else {
            0
        };
        if it.set && (it.toggle || it.radio) {
            pm_text(
                pm,
                fs,
                sw,
                Rect {
                    x: row.x + 2,
                    w: POP_CHECK_W,
                    ..row
                },
                if it.radio { "\u{25cf}" } else { "\u{2713}" },
                if it.radio { POP_FONT - 4 } else { POP_FONT },
                fg,
                true,
            );
        }
        pm_text(
            pm,
            fs,
            sw,
            Rect {
                x: row.x + POP_PAD_X + POP_CHECK_W,
                w: (row.w - POP_PAD_X * 2 - POP_CHECK_W).max(1),
                ..row
            },
            &it.label,
            POP_FONT,
            fg,
            true,
        );
    }
}

/// Blit icône RGBA (`icon.data` = `0xAARRGGBB`) dans `r` centré.
fn pm_icon(pm: &mut dillo_platform::tiny_skia::Pixmap, r: Rect, icn: &Icon) {
    let ox = r.x + (r.w - icn.w as i32).max(0) / 2;
    let oy = r.y + (r.h - icn.h as i32).max(0) / 2;
    for iy in 0..icn.h as i32 {
        for ix in 0..icn.w as i32 {
            let (x, y) = (ox + ix, oy + iy);
            if x < r.x
                || y < r.y
                || x >= r.x + r.w
                || y >= r.y + r.h
                || x < 0
                || y < 0
                || x >= pm.width() as i32
                || y >= pm.height() as i32
            {
                continue;
            }
            let px = icn.data[(iy * icn.w as i32 + ix) as usize];
            let a = (px >> 24) & 0xff;
            if a == 0 {
                continue;
            }
            let idx = (y as u32 * pm.width() + x as u32) as usize;
            if let Some(p) = dillo_platform::tiny_skia::PremultipliedColorU8::from_rgba(
                ((px >> 16) & 0xff) as u8,
                ((px >> 8) & 0xff) as u8,
                (px & 0xff) as u8,
                a as u8,
            ) {
                if let Some(dst) = pm.pixels_mut().get_mut(idx) {
                    *dst = p;
                }
            }
        }
    }
}

impl Ui {
    /// Fond barres (≙ couleur `UI`/`panel` — `ui_main_bg_color`).
    #[must_use]
    pub fn bar_bg(&self, prefs: &Prefs) -> u32 {
        if prefs.ui_main_bg_color != -1 {
            prefs.ui_main_bg_color as u32
        } else {
            FL_BG_DEFAULT as u32
        }
    }

    /// Dessine un [`WItem`] (`Fl_Widget::draw` : box + icône/label).
    pub fn draw_item(
        &self,
        pm: &mut dillo_platform::tiny_skia::Pixmap,
        fs: &mut dillo_platform::cosmic_text::FontSystem,
        sw: &mut dillo_platform::cosmic_text::SwashCache,
        it: &WItem,
        prefs: &Prefs,
    ) {
        if !it.visible || it.rect.w <= 0 || it.rect.h <= 0 {
            return;
        }
        let bg = self.bar_bg(prefs);
        match it.id {
            WId::Spacer => {
                pm_fill(pm, it.rect, bg);
            }
            WId::Location => {
                // CustInput : boîte input (`ui_text_bg_color`) + texte.
                let in_bg = if prefs.ui_text_bg_color != -1 {
                    prefs.ui_text_bg_color as u32
                } else {
                    0x00ff_ffff
                };
                let fg = if prefs.ui_fg_color != -1 {
                    prefs.ui_fg_color as u32
                } else {
                    0
                };
                pm_box(pm, it.rect, in_bg, true);
                let inner = Rect {
                    x: it.rect.x + 3,
                    y: it.rect.y + 2,
                    w: (it.rect.w - 6).max(1),
                    h: (it.rect.h - 4).max(1),
                };
                // Buffer shapé seulement quand le caret/sélection
                // sont visibles — positions réelles ≙
                // `fl_width`/hit-test amont.
                let buf = (self.focus == Focus::Location)
                    .then(|| loc_buf(fs, &self.location.text, self.loc_font_px, inner.w));
                // sélection (tout bleu ≙ FL_SELECTION_COLOR)
                let (a, b) = (
                    self.location.pos.min(self.location.mark),
                    self.location.pos.max(self.location.mark),
                );
                if let (Some(buf), true) = (&buf, a < b) {
                    if let Some((sx, sw_)) = loc_sel_span(buf, a, b) {
                        pm_fill(
                            pm,
                            Rect {
                                x: inner.x + 2 + sx.round() as i32,
                                y: inner.y,
                                w: sw_.ceil().max(0.0) as i32,
                                h: inner.h,
                            },
                            0x0000_8090,
                        );
                    }
                }
                pm_text(
                    pm,
                    fs,
                    sw,
                    inner,
                    &self.location.text,
                    self.loc_font_px,
                    fg,
                    true,
                );
                // curseur (caret 1px) quand focus
                if let Some(buf) = &buf {
                    let cx = inner.x + 2 + loc_x(buf, self.location.pos).round() as i32;
                    pm_fill(
                        pm,
                        Rect {
                            x: cx.min(inner.x + inner.w - 1),
                            y: inner.y,
                            w: 1,
                            h: inner.h,
                        },
                        fg,
                    );
                }
            }
            WId::StatusOut => {
                pm_fill(pm, it.rect, bg);
                pm_rect(pm, it.rect, shade(bg, 0.7));
                let fg = if prefs.ui_fg_color != -1 {
                    prefs.ui_fg_color as u32
                } else {
                    0
                };
                pm_text(pm, fs, sw, it.rect, &it.label, 8, fg, true);
            }
            WId::PageProg | WId::ImgProg | WId::BugMeter => {
                pm_fill(pm, it.rect, bg);
                pm_rect(pm, it.rect, shade(bg, 0.7));
                if let Some(icn) = &it.icon {
                    pm_icon(pm, it.rect, icn);
                }
                let fg = if prefs.ui_fg_color != -1 {
                    prefs.ui_fg_color as u32
                } else {
                    0
                };
                pm_text(
                    pm,
                    fs,
                    sw,
                    it.rect,
                    &it.label,
                    10,
                    fg,
                    it.id == WId::StatusOut,
                );
            }
            _ => {
                // Bouton : box plat, pressed → thin-down (down).
                if it.active {
                    pm_box(pm, it.rect, bg, it.down);
                } else {
                    pm_fill(pm, it.rect, bg);
                    pm_rect(pm, it.rect, shade(bg, 0.8));
                }
                let icn = if it.active {
                    it.icon.as_ref()
                } else {
                    it.deicon.as_ref().or(it.icon.as_ref())
                };
                // `FL_ALIGN_IMAGE_OVER_TEXT` (défaut FLTK, flag=0) :
                // icône au-dessus, label dans la bande basse du bouton.
                let label_h = if !it.label.is_empty() && self.lbl != 0 && icn.is_some() {
                    14
                } else {
                    0
                };
                if let Some(icn) = icn {
                    let ir = Rect {
                        h: (it.rect.h - label_h).max(1),
                        ..it.rect
                    };
                    pm_icon(pm, ir, icn);
                }
                if !it.label.is_empty() && self.lbl != 0 {
                    let tr = if label_h > 0 {
                        Rect {
                            y: it.rect.y + it.rect.h - label_h,
                            h: label_h,
                            ..it.rect
                        }
                    } else {
                        it.rect
                    };
                    let fg = if prefs.ui_fg_color != -1 {
                        prefs.ui_fg_color as u32
                    } else {
                        0
                    };
                    pm_text(pm, fs, sw, tr, &it.label, 12, fg, false);
                }
            }
        }
    }

    /// Dessine les barres de l'UI (LocBar/NavBar/StatusBar + findbar
    /// fond) — la vue du contenu est blittée par `Chrome` (main).
    pub fn draw(
        &self,
        pm: &mut dillo_platform::tiny_skia::Pixmap,
        fs: &mut dillo_platform::cosmic_text::FontSystem,
        sw: &mut dillo_platform::cosmic_text::SwashCache,
        prefs: &Prefs,
    ) {
        let bg = self.bar_bg(prefs);
        for g in [&self.loc_bar, &self.nav_bar].into_iter().flatten() {
            if g.rect.h <= 0 {
                continue;
            }
            pm_fill(pm, g.rect, bg);
            for it in &g.children {
                self.draw_item(pm, fs, sw, it, prefs);
            }
        }
        if self.findbar_visible {
            pm_fill(pm, self.findbar, bg);
            pm_rect(pm, self.findbar, shade(bg, 0.7));
        }
        if self.status_bar.rect.h > 0 {
            pm_fill(pm, self.status_bar.rect, bg);
            for it in &self.status_bar.children {
                self.draw_item(pm, fs, sw, it, prefs);
            }
        }
    }
}

impl Chrome {
    /// Dessine une fenêtre complète : strip d'onglets + chrome de
    /// l'onglet actif + blit de sa vue contenu dans `Main`
    /// (≙ `Fl_Window::draw` + `FltkViewport` présentation).
    pub fn draw_window(&mut self, tabs: usize, pm: &mut dillo_platform::tiny_skia::Pixmap) {
        // Drain des popups poussés depuis la dernière passe
        // (≙ `a_Timeout_add(0, Menu_popup_cb)`) — avant l'emprunt
        // `w`/`platform`, `&mut self` requis.
        self.popup_activate();
        let Some(w) = self.wins.get(tabs).and_then(|w| w.as_ref()) else {
            return;
        };
        let prefs = self.prefs.borrow();
        let fg_def = if prefs.ui_fg_color != -1 {
            prefs.ui_fg_color as u32
        } else {
            0
        };
        let strip_bg = if prefs.ui_main_bg_color != -1 {
            prefs.ui_main_bg_color as u32
        } else {
            FL_BG_DEFAULT as u32
        };
        let active = w.tabs.value();

        // fond strip + onglets
        let th = w.tabs.ctab_h;
        pm_fill(
            pm,
            Rect {
                x: 0,
                y: 0,
                w: w.tabs.win_w,
                h: th,
            },
            strip_bg,
        );

        // emprunts plateforme pour le texte
        let p = self.platform.borrow();
        let mut fs = p.font_system().borrow_mut();
        let mut sw = p.swash().borrow_mut();

        let act_idx = w.tabs.active;
        for i in 0..w.tabs.pack.len() {
            let b = &w.tabs.pack[i];
            if !b.visible {
                continue;
            }
            let Some(r) = w.tabs.tab_rect(i) else {
                continue;
            };
            let (bg, fg) = self.tab_colors(i == act_idx);
            pm_box(pm, r, bg, false);
            pm_text(pm, &mut fs, &mut sw, r, &b.label, 10, fg, false);
        }
        // CloseBtn (≙ `x` sur fond bouton)
        if w.tabs.close_visible {
            let r = Rect {
                x: w.tabs.win_w - w.tabs.ctl_w + 2,
                y: 0,
                w: w.tabs.btn_w,
                h: w.tabs.tab_h.min(th),
            };
            pm_box(pm, r, strip_bg, false);
            pm_text(pm, &mut fs, &mut sw, r, "x", 10, fg_def, false);
        }

        // chrome de l'onglet actif + blit contenu
        if let Some(ui_id) = active {
            if let Some(u) = self.ui(ui_id) {
                u.draw(pm, &mut fs, &mut sw, &prefs);
                // contenu : pixmap vue → `main`
                let c = u.view.borrow();
                let view_pm = c.pixmap();
                if let Some(view_pm) = view_pm.as_deref() {
                    let mr = u.main;
                    for y in 0..view_pm.height() as i32 {
                        for x in 0..view_pm.width() as i32 {
                            if x >= mr.w || y >= mr.h {
                                continue;
                            }
                            let (dx, dy) = (mr.x + x, mr.y + y);
                            if dx < 0
                                || dy < 0
                                || dx >= pm.width() as i32
                                || dy >= pm.height() as i32
                            {
                                continue;
                            }
                            let sidx = (y * view_pm.width() as i32 + x) as usize;
                            let didx = (dy * pm.width() as i32 + dx) as usize;
                            if let (Some(src), Some(dst)) =
                                (view_pm.pixels().get(sidx), pm.pixels_mut().get_mut(didx))
                            {
                                *dst = *src;
                            }
                        }
                    }
                }
            }
        }

        // Overlay popup (≙ fenêtre `Fl_Menu` modale amont) —
        // dessiné en dernier, par-dessus chrome et contenu ; le
        // drain des popups différés a eu lieu avant l'emprunt
        // plateforme (`self.popup_activate()` plus haut).
        let sel = self.popup_armed.or(self.popup_hover);
        if let Some(pop) = &self.popup {
            draw_popup(
                pm,
                &mut fs,
                &mut sw,
                pop,
                self.popup_sub,
                sel,
                pm.width() as i32,
                pm.height() as i32,
            );
        }
    }

    /// Layout d'un `ui` (≙ `bw->render_layout` côté shell).
    #[must_use]
    pub fn layout_of(&self, ui: usize) -> Option<Rc<RefCell<Layout>>> {
        self.layouts.get(&ui).cloned()
    }

    /// Drain des callbacks idle de **tous** les layouts d'onglets —
    /// ≙ `Fl::add_idle(generalIdle)` amont (une fonction par passe
    /// et par plateforme/layout). Indispensable : en mode shell la
    /// boucle ne connaît pas `w.layout` (mode standalone) — sans ce
    /// drain, `scrollIdle`/`resizeIdle` ne se résolvent jamais et
    /// `Layout::draw` diffère à l'infini (canvas blanc).
    pub fn drain_layouts(&mut self) {
        for l in self.layouts.values() {
            l.borrow_mut().drain_idles();
        }
    }

    /// Repose les rects `Ui` d'une fenêtre après resize ou bascule
    /// du strip (verbatim : `Wizard->resize` cascade `UI::resize` →
    /// `rearrange` + viewport `resize` → `layoutViewportSizeChanged`).
    pub fn layout_win(&mut self, tabs: usize, w: i32, h: i32) {
        let Some(win) = self.wins.get_mut(tabs).and_then(|w| w.as_mut()) else {
            return;
        };
        win.tabs.win_w = w;
        win.tabs.win_h = h;
        let ctab_h = win.tabs.ctab_h;
        let ids: Vec<usize> = win.tabs.pack.iter().map(|b| b.ui).collect();
        for ui in ids {
            if let Some(u) = self.ui_mut(ui) {
                u.rect = Rect {
                    x: 0,
                    y: ctab_h,
                    w,
                    h: h - ctab_h,
                };
                u.win_w = w;
                u.win_h = h - ctab_h;
                u.rearrange();
                u.set_main_rect();
                let (mw, mh) = (u.main.w, u.main.h);
                u.view.borrow_mut().queue_draw_total();
                if let Some(l) = self.layouts.get(&ui) {
                    l.borrow_mut().viewport_size_changed(mw, mh);
                }
            }
        }
    }

    /// `Tabs` d'une fenêtre (lecture — hit-test event).
    #[must_use]
    pub fn tabs(&self, tabs: usize) -> Option<&Tabs> {
        self.wins
            .get(tabs)
            .and_then(|w| w.as_ref())
            .map(|w| &w.tabs)
    }

    /// `Tabs` mutable.
    pub fn tabs_mut(&mut self, tabs: usize) -> Option<&mut Tabs> {
        self.wins
            .get_mut(tabs)
            .and_then(|w| w.as_mut())
            .map(|w| &mut w.tabs)
    }

    /// Nombre de fenêtres ouvertes.
    #[must_use]
    pub fn win_count(&self) -> usize {
        self.wins.iter().flatten().count()
    }

    /// Titre courant d'une fenêtre.
    #[must_use]
    pub fn win_title(&self, tabs: usize) -> Option<&str> {
        self.wins
            .get(tabs)
            .and_then(|w| w.as_ref())
            .map(|w| w.title.as_str())
    }

    /// Position demandée d'une fenêtre (`prefs.xpos/ypos` appliqués).
    #[must_use]
    pub fn win_pos(&self, tabs: usize) -> Option<(i32, i32)> {
        self.wins
            .get(tabs)
            .and_then(|w| w.as_ref())
            .and_then(|w| w.pos)
    }
}

// -- tests ------------------------------------------------------------------

// -- dispatch souris (verbatim callbacks de ui.cc/menu.cc) -----------------------

/// Boutons souris amont (`Fl::event_button()`).
pub const MOUSE_LEFT: i32 = 1;
/// `FL_MIDDLE_MOUSE`.
pub const MOUSE_MIDDLE: i32 = 2;
/// `FL_RIGHT_MOUSE`.
pub const MOUSE_RIGHT: i32 = 3;

/// Dispatch verbatim des callbacks de widgets du chrome —
/// `b1_cb`/`clear_cb`/`search_cb`/`help_cb`/`filemenu_cb`/
/// `bugmeter_cb` (ui.cc) + sous-widgets du findbar (findbar.cc).
///
/// `(wx, wy)` = position du clic dans l'UI ; `mouse` ≙
/// `Fl::event_button()` ; `paste` ≙ `Fl::paste` (sélection primaire
/// → texte). Retourne `true` si un callback a consommé le clic.
#[allow(clippy::too_many_arguments)]
pub fn chrome_widget(
    uicmd: &UiCmd,
    reg: &BwRegistry,
    shell: &mut Chrome,
    prefs: &RefCell<Prefs>,
    bw: u32,
    hit: Hit,
    mouse: i32,
    clicks: i32,
    wx: i32,
    wy: i32,
    paste: &mut dyn FnMut() -> Option<String>,
) -> bool {
    shell.event_xy = (wx, wy); // ≙ Fl::event_x/y pour les popups
    let Some(b) = reg.get(bw) else { return false };
    let ui = b.ui.get();
    match hit {
        Hit::Wid(wid) => {
            let rect = shell
                .ui(ui)
                .and_then(|u| u.rect_of(wid))
                .unwrap_or_default();
            let p = prefs.borrow();
            match wid {
                WId::Btn(btn) => {
                    // `b1_cb` verbatim — gauche/milieu/droite.
                    match btn {
                        UiButton::Back => match mouse {
                            MOUSE_LEFT => uicmd.back(reg, shell, &p, bw),
                            MOUSE_MIDDLE => uicmd.back_nt(reg, shell, &p, bw),
                            MOUSE_RIGHT => {
                                uicmd.back_popup(reg, shell, bw, rect.x, rect.y + rect.h);
                            }
                            _ => {}
                        },
                        UiButton::Forw => match mouse {
                            MOUSE_LEFT => uicmd.forw(reg, shell, &p, bw),
                            MOUSE_MIDDLE => uicmd.forw_nt(reg, shell, &p, bw),
                            MOUSE_RIGHT => {
                                uicmd.forw_popup(reg, shell, bw, rect.x, rect.y + rect.h);
                            }
                            _ => {}
                        },
                        UiButton::Home => match mouse {
                            MOUSE_LEFT => uicmd.home(reg, shell, &p, bw, false),
                            MOUSE_MIDDLE => uicmd.home(reg, shell, &p, bw, true),
                            _ => {}
                        },
                        UiButton::Reload => {
                            if mouse == MOUSE_LEFT {
                                uicmd.reload(reg, shell, &p, bw);
                            }
                        }
                        UiButton::Save => {
                            if mouse == MOUSE_LEFT {
                                uicmd.save_page(reg, shell, bw);
                            }
                        }
                        UiButton::Stop => {
                            if mouse == MOUSE_LEFT {
                                uicmd.stop(reg, shell, &p, bw);
                            }
                        }
                        UiButton::Book => match mouse {
                            MOUSE_LEFT => uicmd.book(reg, shell, &p, bw, false),
                            MOUSE_MIDDLE => uicmd.book(reg, shell, &p, bw, true),
                            _ => {}
                        },
                        UiButton::Tools if (mouse == MOUSE_LEFT || mouse == MOUSE_RIGHT) => {
                            uicmd.tools(reg, shell, bw, rect.x, rect.y + rect.h);
                        }
                        _ => {}
                    }
                    true
                }
                WId::FileMenu => {
                    // `filemenu_cb` — gauche ou droite.
                    if mouse == MOUSE_LEFT || mouse == MOUSE_RIGHT {
                        uicmd.file_popup(shell, bw, rect.x, rect.y + rect.h);
                    }
                    true
                }
                WId::Clear => {
                    // `clear_cb` verbatim — L: clear+focus,
                    // M: `paste_url` (FL_PASTE → set_location+open).
                    if mouse == MOUSE_LEFT {
                        uicmd.focus_location(reg, shell, bw);
                        if let Some(u) = shell.ui_mut(ui) {
                            u.set_location("");
                        }
                    } else if mouse == MOUSE_MIDDLE {
                        if let Some(t) = paste() {
                            if !t.is_empty() {
                                shell.set_location(ui, &t);
                                uicmd.open_urlstr(reg, shell, &p, bw, t.as_bytes());
                            }
                        }
                    }
                    true
                }
                WId::Search => {
                    if mouse == MOUSE_LEFT {
                        uicmd.search_dialog(reg, shell, &p, bw);
                    }
                    true
                }
                WId::Help => {
                    // `help_cb` verbatim — docdir local ou remote.
                    if mouse == MOUSE_LEFT {
                        let path = shell.docdir.join("user_help.html");
                        if path.is_file() {
                            let url = format!("file:{}", path.to_string_lossy());
                            uicmd.open_urlstr(reg, shell, &p, bw, url.as_bytes());
                        } else {
                            uicmd.open_urlstr(
                                reg,
                                shell,
                                &p,
                                bw,
                                b"https://dillo-browser.org/user_help.html",
                            );
                        }
                    }
                    true
                }
                WId::BugMeter => {
                    // `bugmeter_cb` verbatim.
                    if mouse == MOUSE_LEFT {
                        uicmd.view_page_bugs(reg, shell, bw);
                    } else if mouse == MOUSE_RIGHT {
                        uicmd.bugmeter_popup(reg, shell, bw);
                    }
                    true
                }
                WId::Location => {
                    // `Fl_Input::handle(FL_PUSH)` ≙ `take_focus` +
                    // `handle_mouse` : caret sous le clic (pas de
                    // select-all — réservé à `UI::focus_location`) ;
                    // middle = `Fl::paste` à la position.
                    let platform = Rc::clone(shell.platform_rc());
                    if let Some(u) = shell.ui_mut(ui) {
                        if u.panel_mode == UiPanelMode::Hidden {
                            u.panels_toggle();
                            u.panel_temporary = true;
                        }
                        u.focus = Focus::Location;
                        u.loc_drag = mouse == MOUSE_LEFT;
                        // Hit-test sur les glyphes réels — le texte
                        // est dessiné à `inner.x + 2` (≙ `pm_text`).
                        let p = {
                            let pl = platform.borrow();
                            let mut fs = pl.font_system().borrow_mut();
                            let buf = loc_buf(&mut fs, &u.location.text, u.loc_font_px, rect.w - 6);
                            loc_index_at(&buf, (wx - (rect.x + 3 + 2)) as f32)
                        };
                        if clicks >= 3 {
                            // Triple-clic ≙ `event_clicks()>1` :
                            // sélection totale.
                            u.location.select_all();
                        } else if clicks == 2 {
                            // Double-clic ≙ `wordboundary` : le mot.
                            let (wa, wz) = word_span(&u.location.text, p);
                            u.location.set_position(wz, wa);
                        } else {
                            u.location.set_position(p, p);
                        }
                        if mouse == MOUSE_MIDDLE {
                            if let Some(t) = paste() {
                                u.location.insert(&t);
                            }
                        }
                    }
                    true
                }
                _ => false,
            }
        }
        Hit::Findbar(fh) => {
            // verbatim `findbar.cc` callbacks.
            match fh {
                Some(FindHit::Hide) => uicmd.findbar_toggle(reg, shell, bw, false),
                Some(FindHit::Next) => {
                    if let Some(u) = shell.ui(ui) {
                        if let Some((key, cs, back)) =
                            crate::findbar::search_cmd(&u.findbar_state, false)
                        {
                            uicmd.findtext_search(reg, shell, bw, key.as_bytes(), cs, back);
                        }
                    }
                }
                Some(FindHit::Prev) => {
                    if let Some(u) = shell.ui(ui) {
                        if let Some((key, cs, back)) =
                            crate::findbar::search_cmd(&u.findbar_state, true)
                        {
                            uicmd.findtext_search(reg, shell, bw, key.as_bytes(), cs, back);
                        }
                    }
                }
                Some(FindHit::Case) => {
                    if let Some(u) = shell.ui_mut(ui) {
                        u.findbar_state.case_sens = !u.findbar_state.case_sens;
                    }
                }
                Some(FindHit::Input) => {
                    if let Some(u) = shell.ui_mut(ui) {
                        u.findbar_state.input_focused = true;
                    }
                }
                None => {}
            }
            true
        }
        _ => false,
    }
}

/// Touche quand le findbar a le focus (verbatim `MyInput::handle` +
/// `Findbar::handle` — Escape laisse remonter à l'UI, Enter =
/// next, Shift+Enter = prev via shortcut du bouton).
///
/// Retourne `true` si consommé par la barre.
pub fn findbar_key(
    uicmd: &UiCmd,
    reg: &BwRegistry,
    shell: &mut Chrome,
    bw: u32,
    key: i32,
    text: Option<&str>,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
) -> bool {
    let Some(b) = reg.get(bw) else { return false };
    let ui = b.ui.get();
    let platform = Rc::clone(shell.platform_rc());
    let Some(u) = shell.ui_mut(ui) else {
        return false;
    };

    // Enter = `next_btn` shortcut (verbatim `next_btn->shortcut(
    // FL_Enter)` + `prev_btn->shortcut(FL_SHIFT+FL_Enter)`).
    if (key == crate::keys::keysym::ENTER || key == crate::keys::keysym::KP_ENTER)
        && (!ctrl && !alt && !meta)
    {
        let backwards = shift;
        if let Some((k, cs, back)) = crate::findbar::search_cmd(&u.findbar_state, backwards) {
            uicmd.findtext_search(reg, shell, bw, k.as_bytes(), cs, back);
        }
        return true;
    }

    match crate::findbar::input_key(&mut u.findbar_state.input, key, shift, ctrl, alt, meta) {
        crate::findbar::InputKey::Pass => false,
        crate::findbar::InputKey::Consumed => true,
        crate::findbar::InputKey::Edit => {
            // `Fl_Input::handle` ≙ `line_input_key` — clipboard OS
            // (MyInput hérite Fl_Input : Ctrl+C/X/V actifs amont).
            let mut copy = {
                let platform = Rc::clone(&platform);
                move |s: &str| platform.borrow_mut().copy_selection(s.as_bytes(), 1)
            };
            let mut p = move || {
                let t = platform.borrow().clipboard_text();
                if t.is_empty() {
                    None
                } else {
                    String::from_utf8(t).ok()
                }
            };
            let mut ed = LineInput {
                text: u.findbar_state.input.buf.clone(),
                pos: u.findbar_state.input.pos,
                mark: u.findbar_state.input.pos,
            };
            let r = line_input_key(&mut ed, key, text, shift, ctrl, &mut copy, &mut p);
            u.findbar_state.input.buf = ed.text;
            u.findbar_state.input.pos = ed.pos;
            r
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::keysym;
    use dillo_net::url::url_new;

    fn platform() -> Rc<RefCell<SkiaPlatform>> {
        Rc::new(RefCell::new(SkiaPlatform::new()))
    }

    fn ui(prefs: &Prefs) -> Ui {
        Ui::new(&platform(), prefs, UI_MIN_W, UI_MIN_H, None)
    }

    // -- xpm_decode ---------------------------------------------------------

    /// `Ui::page_focus_of` ≙ `Fl::focus` unique : la ressource
    /// in-page focusée est retournée tant que `focus == Main` ;
    /// `Location`/findbar → `None` + caret retiré (≙ `FL_UNFOCUS`
    /// de l'enfant quand `Fl::focus` part au chrome) ; popup →
    /// `None` mais caret conservé (≙ grab `Fl_Menu`).
    #[test]
    fn page_focus_respects_chrome_focus() {
        use dillo_dw::platform::Platform as _;
        let prefs = Prefs::default();
        let mut u = ui(&prefs);
        let p = platform();
        let factory = p.borrow_mut().get_resource_factory().expect("factory");
        let res: Rc<dyn dillo_dw::ui::Resource> = factory.create_entry_resource(10, false, "", "");
        u.view
            .borrow_mut()
            .set_focused_resource(Some(Rc::clone(&res)));

        // Main + page focusée → ressource + sa vue (canvas du damage).
        let (r, v) = Ui::page_focus_of(&u, false).expect("page focusée");
        assert!(Rc::ptr_eq(&r, &res));
        assert!(Rc::ptr_eq(&v, &u.view));

        // Popup ouvert → touches au menu, caret conservé.
        assert!(Ui::page_focus_of(&u, true).is_none());
        assert!(u.view.borrow().focused_resource().is_some());

        // Location focusée → page défocalisée (caret retiré).
        u.focus = Focus::Location;
        assert!(Ui::page_focus_of(&u, false).is_none());
        assert!(u.view.borrow().focused_resource().is_none());
    }

    #[test]
    fn xpm_decode_ok_and_none() {
        let icn = xpm_decode(&["2 2 2 1", "a c #ff0000", "b c None", "ab", "ba"]).unwrap();
        assert_eq!((icn.w, icn.h), (2, 2));
        assert_eq!(icn.data[0], 0xffff_0000);
        assert_eq!(icn.data[1], 0x0000_0000); // None → transparent
        assert_eq!(icn.data[3], 0xffff_0000);
        assert!(xpm_decode(&["bad"]).is_none());
    }

    #[test]
    fn xpm_decode_hash_none_and_wide_channels() {
        use dillo_platform::pixmaps as px;
        // `TOOLS_XPM` amont : `c #None` + couleurs 12 hex
        // (16 bits/canal → top byte ≙ `Fl_Pixmap`).
        let icn = xpm_decode(&[
            "2 2 3 1",
            "a c #ff0000",
            "b c #None",
            "c c #696977777676",
            "ab",
            "ca",
        ])
        .unwrap();
        assert_eq!(icn.data[1], 0x0000_0000); // `#None` → transparent
        assert_eq!(icn.data[2], 0xff69_7776);
        // Ressources embarquées réelles : aucune ne doit échouer.
        for d in [
            px::HOME_XPM,
            px::LEFT_XPM,
            px::RIGHT_XPM,
            px::SAVE_XPM,
            px::RELOAD_XPM,
            px::STOP_XPM,
            px::BM_XPM,
            px::TOOLS_XPM,
            px::TOOLS_S_XPM,
            px::HELP_XPM,
            px::SEARCH_XPM,
            px::MINI_OK_XPM,
            px::MINI_BUG_XPM,
            px::NEW_S_XPM,
            px::HOME_S_XPM,
            px::LEFT_S_XPM,
            px::RIGHT_S_XPM,
            px::SAVE_S_XPM,
            px::RELOAD_S_XPM,
            px::STOP_S_XPM,
            px::BM_S_XPM,
        ] {
            assert!(xpm_decode(d).is_some());
        }
    }

    #[test]
    fn pm_icon_blits_centered_argb() {
        // `Icon.data` = `0xAARRGGBB` : rouge opaque + transparent.
        let icn = Icon {
            w: 2,
            h: 2,
            data: vec![0xffff_0000, 0, 0, 0xff00_00ff],
        };
        let mut pm = dillo_platform::tiny_skia::Pixmap::new(8, 8).unwrap();
        pm_icon(
            &mut pm,
            Rect {
                x: 0,
                y: 0,
                w: 8,
                h: 8,
            },
            &icn,
        );
        // Centré → icône 2×2 aux pixels (3,3)..(4,4).
        let px_at = |x: u32, y: u32| pm.pixel(x, y).unwrap();
        assert_eq!(
            (px_at(3, 3).red(), px_at(3, 3).alpha()),
            (255, 255),
            "R opaque"
        );
        assert_eq!(px_at(3, 4).alpha(), 0, "transparent → non écrit");
        assert_eq!(px_at(4, 4).blue(), 255, "B opaque");
    }

    #[test]
    fn pm_text_blends_aa_coverage() {
        // Le canal alpha des glyphes swash est une *couverture* :
        // src-over, pas remplacement — sinon `fg*α` sans fond se
        // présente fondu sur noir (texte de l'Entry « fondu »).
        use dillo_platform::cosmic_text::FontSystem;
        let mut pm = dillo_platform::tiny_skia::Pixmap::new(120, 24).unwrap();
        pm.fill(dillo_platform::tiny_skia::Color::from_rgba8(
            0xff, 0xff, 0xff, 0xff,
        ));
        let mut fs = FontSystem::new();
        let mut sw = dillo_platform::cosmic_text::SwashCache::new();
        pm_text(
            &mut pm,
            &mut fs,
            &mut sw,
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 24,
            },
            "URL",
            14,
            0,
            true,
        );
        // Texte noir sur blanc : chaque pixel touché doit être
        // opaque (255) avec luminance < 255 — pas de pixel
        // semi-transparent résiduel.
        for p in pm.pixels() {
            assert_eq!(p.alpha(), 255, "dst doit rester opaque");
        }
        let dark = pm
            .pixels()
            .iter()
            .any(|p| p.red() < 0x80 && p.green() < 0x80 && p.blue() < 0x80);
        assert!(dark, "des pixels de glyphe doivent être présents");
    }

    #[test]
    fn iconset_builds_all() {
        let s = IconSet::new(false, FL_BG_DEFAULT as u32);
        assert!(s.imgs[IconKind::Left as usize].is_some());
        assert!(s.imgs[IconKind::LeftIn as usize].is_some());
        assert!(s.imgs[IconKind::MeterOk as usize].is_some());
        // *In = desaturate + color_average → pixels dérivés
        let (a, b) = (
            &s.imgs[IconKind::Left as usize],
            &s.imgs[IconKind::LeftIn as usize],
        );
        if let (Some(a), Some(b)) = (a, b) {
            assert_eq!((a.w, a.h), (b.w, b.h));
            assert_ne!(a.data, b.data);
        }
    }

    // -- Group::rearrange (CustGroupHorizontal verbatim) ---------------------

    #[test]
    fn group_rearrange_resizable_gets_rest() {
        let mut g = Group {
            rect: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 20,
            },
            children: vec![
                WItem::new(WId::Clear, 0, 10, 20),
                WItem::new(WId::Location, 10, 5, 20),
                WItem::new(WId::Search, 90, 10, 20),
            ],
            resizable: Some(1),
        };
        g.rearrange();
        assert_eq!(
            g.children[0].rect,
            Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 20
            }
        );
        assert_eq!(
            g.children[1].rect,
            Rect {
                x: 10,
                y: 0,
                w: 80,
                h: 20
            }
        );
        assert_eq!(
            g.children[2].rect,
            Rect {
                x: 90,
                y: 0,
                w: 10,
                h: 20
            }
        );
    }

    #[test]
    fn group_rearrange_overflow_drops_resizable() {
        let mut g = Group {
            rect: Rect {
                x: 0,
                y: 0,
                w: 15,
                h: 20,
            },
            children: vec![
                WItem::new(WId::Clear, 0, 10, 20),
                WItem::new(WId::Location, 10, 5, 20),
                WItem::new(WId::Search, 90, 10, 20),
            ],
            resizable: Some(1),
        };
        g.rearrange();
        assert_eq!(g.children[1].rect.w, 0);
        assert_eq!(g.resizable, None); // verbatim: resizable perdu
    }

    #[test]
    fn group_invisible_skipped() {
        let mut g = Group {
            rect: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 20,
            },
            children: vec![
                WItem::new(WId::Clear, 0, 10, 20),
                WItem::new(WId::Location, 10, 5, 20),
                WItem::new(WId::Search, 90, 10, 20),
            ],
            resizable: Some(1),
        };
        g.children[0].visible = false;
        g.rearrange();
        // Clear caché : Location commence à 0 et récupère ses 10px.
        assert_eq!(
            g.children[1].rect,
            Rect {
                x: 0,
                y: 0,
                w: 90,
                h: 20
            }
        );
        assert_eq!(g.children[2].rect.x, 90);
    }

    // -- LineInput / CustInput ----------------------------------------------

    #[test]
    fn line_input_editing() {
        let mut li = LineInput::default();
        li.set_value("héllo");
        assert_eq!(li.size(), 6); // octets
        li.set_position(0, 0);
        li.cut(0, 1);
        assert_eq!(li.text, "éllo");
        // cut sur frontière invalide clampée
        li.set_value("hé");
        li.cut(0, 2); // 'h' + premier octet de 'é' → clampé à fin 'h'? non:
        assert_eq!(li.text, "é"); // cut(0,1) retirerait h ; 0..2 couvre h+\xc3
                                  // verbatim : FLTK coupe des octets — la borne UTF-8 est le garde-fou
    }

    #[test]
    fn line_input_select_and_insert() {
        let mut li = LineInput::default();
        li.set_value("abcd");
        li.select_all();
        assert_eq!((li.pos, li.mark), (4, 0));
        li.insert("xy");
        assert_eq!(li.text, "xy");
        assert_eq!(li.pos, 2);
    }

    #[test]
    fn word_span_runs() {
        // ≙ `word_end`/`word_begin` : run alnum + `_`.
        assert_eq!(word_span("foo.bar baz", 1), (0, 3));
        assert_eq!(word_span("foo.bar baz", 4), (4, 7));
        // Séparateur seul → le caractère.
        assert_eq!(word_span("foo.bar", 3), (3, 4));
        // Bord droit / hors bornes.
        assert_eq!(word_span("abc", 3), (0, 3));
        assert_eq!(word_span("", 5), (0, 0));
        // `_` compte dans le mot.
        assert_eq!(word_span("a_b", 1), (0, 3));
    }

    // -- Ui metrics (verbatim make_panel) ------------------------------------

    #[test]
    fn panel_metrics_verbatim() {
        let p = Prefs::default();
        let mut u = ui(&p);
        // P_tiny
        u.panel_size = 0;
        u.small_icons = false;
        u.panel_metrics();
        assert_eq!(
            (u.bw, u.bh, u.lh, u.nh, u.fh, u.sh, u.pw),
            (28, 28, 28, 28, 28, 20, 70)
        );
        // P_medium + small_icons
        u.panel_size = 2;
        u.small_icons = true;
        u.panel_metrics();
        assert_eq!((u.bw, u.bh, u.lh, u.mh, u.lbl), (42, 36, 22, 0, 1));
        // P_small grandes icônes
        u.panel_size = 1;
        u.small_icons = false;
        u.panel_metrics();
        assert_eq!((u.bw, u.bh, u.lh, u.lbl), (28, 28, 28, 0));
    }

    #[test]
    fn ui_constructs_tiny_single_bar() {
        let p = Prefs {
            panel_size: PanelSize::Tiny,
            ..Prefs::default()
        };
        let u = ui(&p);
        assert!(u.loc_bar.is_none()); // P_tiny : une seule barre
        let nav = u.nav_bar.as_ref().unwrap();
        // Back..Tools + File + Clear + Location + Search + Help + ImgProg + PageProg
        assert!(nav.find(WId::Btn(UiButton::Back)).is_some());
        assert!(nav.find(WId::FileMenu).is_some());
        assert!(nav.find(WId::Location).is_some());
        assert!(nav.find(WId::PageProg).is_some());
        // resizable = Location
        assert_eq!(nav.resizable, nav.find(WId::Location));
    }

    #[test]
    fn ui_medium_two_bars() {
        let p = Prefs {
            panel_size: PanelSize::Medium,
            ..Prefs::default()
        };
        let u = ui(&p);
        assert!(u.loc_bar.is_some());
        let nav = u.nav_bar.as_ref().unwrap();
        // resizable = spacer
        assert_eq!(nav.resizable, nav.find(WId::Spacer));
        assert!(nav.find(WId::PageProg).is_some());
    }

    #[test]
    fn status_bar_layout() {
        let p = Prefs::default();
        let u = ui(&p);
        assert_eq!(u.status_bar.children.len(), 2);
        assert_eq!(u.status_bar.children[1].id, WId::BugMeter);
        assert_eq!(u.status_bar.children[1].rect.w, 20);
        assert_eq!(u.status_bar.children[0].rect.w, UI_MIN_W - 20);
    }

    #[test]
    fn rearrange_main_gets_rest() {
        let p = Prefs::default();
        let mut u = ui(&p);
        u.rearrange();
        // medium : loc(lh) + nav(nh) + main + status(sh)
        assert_eq!(u.main.y, u.lh + u.nh);
        assert_eq!(u.main.h, UI_MIN_H - u.lh - u.nh - u.sh);
        assert_eq!(u.status_bar.rect.y, UI_MIN_H - u.sh);
    }

    #[test]
    fn panels_toggle_hides_and_restores() {
        let p = Prefs::default();
        let mut u = ui(&p);
        u.panels_toggle();
        assert_eq!(u.panel_mode, UiPanelMode::Hidden);
        assert_eq!(u.main.h, UI_MIN_H); // tout au main
        u.panels_toggle();
        assert_eq!(u.panel_mode, UiPanelMode::Normal);
        assert_eq!(u.main.h, UI_MIN_H - u.lh - u.nh - u.sh);
    }

    #[test]
    fn fullwindow_start_hides_panel() {
        let p = Prefs {
            fullwindow_start: true,
            ..Prefs::default()
        };
        let u = ui(&p);
        assert_eq!(u.panel_mode, UiPanelMode::Hidden);
    }

    #[test]
    fn findbar_toggle_geometry() {
        let p = Prefs::default();
        let mut u = ui(&p);
        u.findbar_toggle(true);
        assert!(u.findbar_visible);
        assert_eq!(u.findbar.h, 28);
        // main réduit de fh
        assert_eq!(u.main.h, UI_MIN_H - u.lh - u.nh - u.sh - u.fh);
        u.findbar_toggle(false);
        assert!(!u.findbar_visible);
        assert_eq!(u.findbar.w, 0); // workaround STR#2639 verbatim
    }

    #[test]
    fn location_focus_semantics() {
        let p = Prefs::default();
        let mut u = ui(&p);
        u.set_location("http://x/");
        // pas de focus → position 0 (verbatim)
        assert_eq!(u.location.pos, 0);
        u.focus_location();
        assert_eq!((u.location.pos, u.location.mark), (u.location.size(), 0));
        u.set_location("http://y/");
        // focus Location → curseur à la fin
        assert_eq!(u.location.pos, u.location.size());
    }

    #[test]
    fn focus_location_unhides_temporary_panel() {
        let p = Prefs {
            fullwindow_start: true,
            ..Prefs::default()
        };
        let mut u = ui(&p);
        u.focus_location();
        assert_eq!(u.panel_mode, UiPanelMode::Normal); // ré-affiché
        assert!(u.panel_temporary);
    }

    #[test]
    fn progress_labels_verbatim() {
        let p = Prefs::default();
        let mut u = ui(&p);
        u.set_page_prog(512 * 1024, 1);
        let i = u.nav_bar.as_ref().unwrap().find(WId::PageProg).unwrap();
        assert_eq!(
            u.nav_bar.as_ref().unwrap().children[i].label,
            "Page
512.0 KB"
        );
        u.set_page_prog(3 * 1024 * 1024, 1);
        let i = u.nav_bar.as_ref().unwrap().find(WId::PageProg).unwrap();
        assert_eq!(
            u.nav_bar.as_ref().unwrap().children[i].label,
            "Page
3.0 MB"
        );
        u.set_img_prog(2, 5, 1);
        let i = u.nav_bar.as_ref().unwrap().find(WId::ImgProg).unwrap();
        assert_eq!(
            u.nav_bar.as_ref().unwrap().children[i].label,
            "Images
2 of 5"
        );
        u.set_page_prog(0, 0);
        let i = u.nav_bar.as_ref().unwrap().find(WId::PageProg).unwrap();
        assert!(!u.nav_bar.as_ref().unwrap().children[i].active); // cmd 0
    }

    #[test]
    fn bug_meter_label_and_width() {
        let p = Prefs::default();
        let mut u = ui(&p);
        let ok = Icon {
            w: 1,
            h: 1,
            data: vec![0xff00_ff00],
        };
        let bug = Icon {
            w: 1,
            h: 1,
            data: vec![0xffff_0000],
        };
        u.set_bug_prog(&ok, &bug, 7);
        let i = u.status_bar.find(WId::BugMeter).unwrap();
        assert_eq!(u.status_bar.children[i].label, "7");
        assert_eq!(u.status_bar.children[i].rect.w, 29); // verbatim
        u.set_bug_prog(&ok, &bug, 0);
        let i = u.status_bar.find(WId::BugMeter).unwrap();
        assert_eq!(u.status_bar.children[i].label, "");
        assert_eq!(u.status_bar.children[i].rect.w, 20);
    }

    #[test]
    fn customize_hides_widgets() {
        let p = Prefs {
            show_back: false,
            show_url: false,
            show_progress_box: false,
            ..Prefs::default()
        };
        let u = ui(&p);
        let find_in = |u: &Ui, id: WId| -> bool {
            [&u.loc_bar, &u.nav_bar]
                .into_iter()
                .flatten()
                .find_map(|g| g.find(id).map(|i| g.children[i].visible))
                .unwrap()
        };
        assert!(!find_in(&u, WId::Btn(UiButton::Back)));
        assert!(find_in(&u, WId::Btn(UiButton::Forw)));
        assert!(!find_in(&u, WId::Location));
        assert!(!find_in(&u, WId::PageProg));
        assert!(!find_in(&u, WId::ImgProg));
    }

    #[test]
    fn button_sens_and_inherit() {
        let p = Prefs::default();
        let mut u = ui(&p);
        u.button_set_sens(UiButton::Back, false);
        let nav = u.nav_bar.as_ref().unwrap();
        let i = nav.find(WId::Btn(UiButton::Back)).unwrap();
        assert!(!nav.children[i].active);
        // verbatim : nouvel onglet hérite panel_size/small_icons
        let u2 = Ui::new(&platform(), &p, UI_MIN_W, UI_MIN_H, Some(&u));
        assert_eq!(u2.panel_size, u.panel_size);
        assert_eq!(u2.small_icons, u.small_icons);
    }

    // -- Tabs (CustTabs verbatim) --------------------------------------------

    #[test]
    fn tabs_lifecycle() {
        let mut t = Tabs::new(600, 200, 22);
        assert_eq!(t.ctab_h, 1);
        assert!(!t.close_visible);
        let lw = |s: &str| s.len() as i32 * 7;
        assert!(!t.add_new_tab(1, true, lw)); // 1er onglet : strip caché
        assert_eq!(t.num_tabs(), 1);
        assert!(t.add_new_tab(2, false, lw)); // 2e : strip apparaît
        assert_eq!(t.ctab_h, 22);
        assert!(t.close_visible);
        assert_eq!(t.value(), Some(1)); // focus conservé sur 1
        t.add_new_tab(3, true, lw);
        assert_eq!(t.value(), Some(3)); // focus → nouveau
    }

    #[test]
    fn tabs_switch_and_remove_focus() {
        let mut t = Tabs::new(600, 200, 22);
        let lw = |s: &str| s.len() as i32 * 7;
        t.add_new_tab(1, true, lw);
        t.add_new_tab(2, true, lw);
        t.add_new_tab(3, true, lw);
        t.switch_tab(0); // ui 1 actif en dernier (focus_num=4)
                         // supprimer l'actif (ui 1) → focus_num max restant = ui 3
        t.remove_tab(1);
        assert_eq!(t.value(), Some(3));
        assert_eq!(t.num_tabs(), 2);
        // supprimer l'actif (ui 3) → reste ui 2
        t.remove_tab(3);
        assert_eq!(t.num_tabs(), 1);
        assert_eq!(t.ctab_h, 1); // strip re-caché verbatim
        assert!(!t.close_visible);
        assert_eq!(t.value(), Some(2));
    }

    #[test]
    fn tabs_prev_next_wrap() {
        let mut t = Tabs::new(600, 200, 22);
        let lw = |s: &str| s.len() as i32 * 7;
        t.add_new_tab(1, true, lw);
        t.add_new_tab(2, false, lw);
        t.add_new_tab(3, false, lw);
        assert_eq!(t.value(), Some(1));
        assert_eq!(t.next_tab(), Some(2));
        assert_eq!(t.next_tab(), Some(3));
        assert_eq!(t.next_tab(), Some(1)); // wrap verbatim
        assert_eq!(t.prev_tab(), Some(3));
    }

    #[test]
    fn tab_label_truncation_utf8() {
        let mut t = Tabs::new(600, 200, 22);
        let lw = |s: &str| s.len() as i32 * 7;
        t.add_new_tab(1, true, lw);
        t.set_tab_label(1, "12345678901234567890", lw);
        let b = &t.pack[0];
        assert_eq!(b.label, "123456789012345...");
        // utf-8 : borne ajustée par end_of_char
        t.set_tab_label(1, "12345678901234\u{00e9}7890", lw); // é = 2 octets à l'index 14
        let b = &t.pack[0];
        assert!(b.label.starts_with("12345678901234") && b.label.ends_with("..."));
        t.set_tab_label(1, "short", lw);
        assert_eq!(t.pack[0].label, "short");
    }

    #[test]
    fn tab_hit_test() {
        let mut t = Tabs::new(600, 200, 22);
        let lw = |s: &str| s.len() as i32 * 7;
        t.add_new_tab(1, true, lw);
        t.add_new_tab(2, false, lw);
        // onglet 0 : w = label_w+14 ; onglet 1 : tab_w=50
        let w0 = t.pack[0].w;
        assert_eq!(t.tab_at(w0 / 2, 5), Some(0));
        assert_eq!(t.tab_at(w0 + 5, 5), Some(1));
        assert_eq!(t.tab_at(0, t.ctab_h + 5), None); // sous le strip
        assert!(t.close_at(t.win_w - t.ctl_w + 5, 5));
    }

    // -- Chrome / UiShell -----------------------------------------------------

    #[test]
    fn chrome_window_and_tabs() {
        let p = platform();
        let prefs = Rc::new(RefCell::new(Prefs::default()));
        let mut c = Chrome::new(p, prefs);
        let tabs = c.window_new(800, 600);
        c.window_show(tabs);
        let (ui1, _l1) = c.tab_ui_new(tabs, 0, true);
        let (ui2, _l2) = c.tab_ui_new(tabs, ui1, false);
        assert_eq!(c.tab_count(tabs), 2);
        assert_eq!(c.tabs_of(ui2), Some(tabs));
        assert!(c.is_active_tab(ui1));
        assert!(!c.is_active_tab(ui2));
        assert_eq!(c.tabs_active_ui(tabs), ui1);
        c.switch_to_tab(ui2);
        assert!(c.is_active_tab(ui2));
        c.set_location(ui2, "http://x/");
        assert_eq!(c.get_location(ui2), "http://x/");
        c.set_status(ui2, "msg");
        assert_eq!(c.ui(ui2).unwrap().status_bar.children[0].label, "msg");
        c.tab_ui_remove(ui1);
        c.tab_ui_remove(ui2);
        assert_eq!(c.win_count(), 0); // fenêtre détruite à 0 onglets (verbatim)
    }

    #[test]
    fn chrome_defer_and_close() {
        let p = platform();
        let prefs = Rc::new(RefCell::new(Prefs::default()));
        let mut c = Chrome::new(p, prefs);
        c.defer_nav(3, NavOp::Reload);
        c.defer_nav(4, NavOp::Repush);
        let ops = c.drain_nav();
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0].1, NavOp::Reload));
        c.defer_close_all();
        assert!(c.take_close_all());
        assert!(!c.take_close_all());
    }

    // -- ui_shortcut dispatch --------------------------------------------------

    #[test]
    fn shortcut_dispatch_consumes() {
        let p = platform();
        let prefs = Rc::new(RefCell::new(Prefs::default()));
        let mut c = Chrome::new(p, Rc::clone(&prefs));
        let reg = BwRegistry::new();
        let uicmd = UiCmd::new(&Prefs::default());
        let pr = prefs.borrow();
        let bw = uicmd.browser_window_new(&reg, &mut c, &pr, 800, 600, None);
        let ui_id = reg.get(bw).unwrap().ui.get();
        // Nop → consommé sans effet ; invalid bw → non consommé
        assert!(ui_shortcut(&uicmd, &reg, &mut c, &pr, bw, KeysCommand::Nop));
        assert!(!ui_shortcut(
            &uicmd,
            &reg,
            &mut c,
            &pr,
            999,
            KeysCommand::Nop
        ));
        // HidePanels → panels toggled
        let before = c.ui(ui_id).unwrap().panel_mode;
        ui_shortcut(&uicmd, &reg, &mut c, &pr, bw, KeysCommand::HidePanels);
        assert_ne!(c.ui(ui_id).unwrap().panel_mode, before);
    }

    // -- cust_input_key --------------------------------------------------------

    #[test]
    fn cust_input_key_filters() {
        const CTRL: i32 = 4;
        let p = platform();
        let prefs = Rc::new(RefCell::new(Prefs::default()));
        let mut c = Chrome::new(p, Rc::clone(&prefs));
        let reg = BwRegistry::new();
        let uicmd = UiCmd::new(&Prefs::default());
        let pr = prefs.borrow();
        let bw = uicmd.browser_window_new(&reg, &mut c, &pr, 800, 600, None);
        let ui_id = reg.get(bw).unwrap().ui.get();
        c.ui_mut(ui_id).unwrap().location.set_value("ab");
        // Ctrl+E → curseur fin (verbatim)
        assert!(cust_input_key(
            &uicmd, &reg, &mut c, &pr, bw, 'e' as i32, None, CTRL
        ));
        assert_eq!(c.ui(ui_id).unwrap().location.pos, 2);
        // Esc → remonte (non consommé)
        assert!(!cust_input_key(
            &uicmd,
            &reg,
            &mut c,
            &pr,
            bw,
            keysym::ESCAPE,
            None,
            0
        ));
        // Shift+Left → remonte
        assert!(!cust_input_key(
            &uicmd,
            &reg,
            &mut c,
            &pr,
            bw,
            keysym::LEFT,
            None,
            1
        ));
        // Down sans modif → focus main + non consommé
        c.ui_mut(ui_id).unwrap().focus = Focus::Location;
        assert!(!cust_input_key(
            &uicmd,
            &reg,
            &mut c,
            &pr,
            bw,
            keysym::DOWN,
            None,
            0
        ));
        assert_eq!(c.ui(ui_id).unwrap().focus, Focus::Main);
        // insertion texte
        assert!(cust_input_key(
            &uicmd,
            &reg,
            &mut c,
            &pr,
            bw,
            0,
            Some("z"),
            0
        ));
        assert_eq!(c.ui(ui_id).unwrap().location.text, "abz");
    }

    // -- chrome-3 : menus / dialogs / findbar / tipwin -------------------------

    fn chrome_bw() -> (Rc<RefCell<Prefs>>, Chrome, BwRegistry, UiCmd, u32, usize) {
        let prefs = Rc::new(RefCell::new(Prefs::default()));
        let mut c = Chrome::new(platform(), Rc::clone(&prefs));
        let reg = BwRegistry::new();
        let uicmd = UiCmd::new(&Prefs::default());
        let bw = {
            let pr = prefs.borrow();
            uicmd.browser_window_new(&reg, &mut c, &pr, 800, 600, None)
        };
        let ui = reg.get(bw).unwrap().ui.get();
        (prefs, c, reg, uicmd, bw, ui)
    }

    #[test]
    fn file_popup_builds_and_dispatches() {
        let (prefs, mut c, reg, uicmd, bw, ui) = chrome_bw();
        // verbatim : filemenu_cb L → popup sous le bouton File
        let r = c.ui(ui).unwrap().rect_of(WId::FileMenu).unwrap();
        uicmd.file_popup(&mut c, bw, r.x, r.y + r.h);
        let mut pop = c.take_popup().unwrap();
        assert_eq!(pop.items.len(), 6);
        assert_eq!(pop.items[0].label, "New tab");
        // "Open URL..." (idx 3) → focus location
        c.popup_dispatch(&uicmd, &reg, &mut pop, 3, None, 1, false);
        assert_eq!(c.ui(ui).unwrap().focus, Focus::Location);
        // "Exit Dillo" (idx 5) → defer_close_all (verbatim)
        uicmd.file_popup(&mut c, bw, r.x, r.y + r.h);
        let mut pop2 = c.take_popup().unwrap();
        c.popup_dispatch(&uicmd, &reg, &mut pop2, 5, None, 1, false);
        assert!(c.take_close_all());
        drop(prefs);
    }

    #[test]
    fn link_popup_dispatch_opens_nt() {
        let (prefs, mut c, reg, uicmd, bw, _ui) = chrome_bw();
        prefs.borrow_mut().focus_new_tab = false;
        let link = url_new(b"https://ex.org/l", None).unwrap();
        let page = url_new(b"https://ex.org/p", None).unwrap();
        c.event_xy = (7, 8);
        uicmd.link_popup(&mut c, bw, &link, Some(&page));
        let mut pop = c.take_popup().unwrap();
        assert_eq!((pop.x, pop.y), (7, 8));
        let count = |reg: &BwRegistry| (0..10).filter(|i| reg.get(*i).is_some()).count();
        let n0 = count(&reg);
        // "Open link in new tab" → nt créé
        c.popup_dispatch(&uicmd, &reg, &mut pop, 0, None, 1, false);
        assert_eq!(count(&reg), n0 + 1);
    }

    #[test]
    fn tools_popup_panel_change_dispatch() {
        let (prefs, mut c, reg, uicmd, bw, ui) = chrome_bw();
        c.tools_popup(bw, ui, 10, 20);
        let mut pop = c.take_popup().unwrap();
        assert_eq!(pop.items[5].submenu.as_ref().unwrap().len(), 4);
        // sous-item "tiny" (index 0) → panel_size 0
        c.popup_dispatch(&uicmd, &reg, &mut pop, 5, Some(0), 1, false);
        assert_eq!(c.ui(ui).unwrap().panel_size, 0);
        // "small icons" (sub idx 3) — garde la taille
        c.tools_popup(bw, ui, 0, 0);
        let mut pop2 = c.take_popup().unwrap();
        c.popup_dispatch(&uicmd, &reg, &mut pop2, 5, Some(3), 1, false);
        assert!(c.ui(ui).unwrap().small_icons);
        assert_eq!(c.ui(ui).unwrap().panel_size, 0);
        drop(prefs);
    }

    #[test]
    fn tools_toggle_remote_css_writes_prefs() {
        let (prefs, mut c, reg, uicmd, bw, ui) = chrome_bw();
        prefs.borrow_mut().load_stylesheets = true;
        c.tools_popup(bw, ui, 0, 0);
        let mut pop = c.take_popup().unwrap();
        c.popup_dispatch(&uicmd, &reg, &mut pop, 0, None, 1, false);
        assert!(!prefs.borrow().load_stylesheets);
        assert!(!pop.items[0].set);
    }

    #[test]
    fn bugmeter_validate_strips_fragment() {
        let (prefs, mut c, reg, uicmd, bw, _ui) = chrome_bw();
        let u0 = url_new(b"https://ex.org/page#frag", None).unwrap();
        {
            let pr = prefs.borrow();
            uicmd.open_url(&reg, &mut c, &pr, bw, Some(&u0));
            uicmd.expect_done(&reg, &mut c, &pr, bw); // ≙ capi→nav
        }
        uicmd.bugmeter_popup(&reg, &mut c, bw);
        let mut pop = c.take_popup().unwrap();
        c.popup_dispatch(&uicmd, &reg, &mut pop, 0, None, 1, false);
        {
            let pr = prefs.borrow();
            uicmd.expect_done(&reg, &mut c, &pr, bw); // ≙ réponse capi
        }
        let b = reg.get(bw).unwrap();
        let nav = b.nav.borrow();
        let mut top = dillo_net::history::history_get_url(nav.get_top_uidx()).unwrap();
        let s = String::from_utf8_lossy(top.url_str()).into_owned();
        assert!(s.contains("validator.w3.org/nu/"));
        assert!(!s.contains("%23")); // '#' tronqué verbatim
        assert!(s.contains("https%3A%2F%2Fex.org%2Fpage"));
    }

    #[test]
    fn history_popup_dispatch_jump() {
        let (prefs, mut c, reg, uicmd, bw, _ui) = chrome_bw();
        {
            let pr = prefs.borrow();
            for s in ["https://a/", "https://b/", "https://c/"] {
                let u = url_new(s.as_bytes(), None).unwrap();
                uicmd.open_url(&reg, &mut c, &pr, bw, Some(&u));
                uicmd.expect_done(&reg, &mut c, &pr, bw);
            }
        }
        uicmd.back_popup(&reg, &mut c, bw, 3, 4);
        let mut pop = c.take_popup().unwrap();
        assert_eq!(pop.items.len(), 2); // b,a (+sentinel)
                                        // item idx 1 = le plus ancien → retour à "a"
        c.popup_dispatch(&uicmd, &reg, &mut pop, 1, None, 1, false);
        let b = reg.get(bw).unwrap();
        let nav = b.nav.borrow();
        let mut top = dillo_net::history::history_get_url(nav.get_top_uidx()).unwrap();
        assert_eq!(String::from_utf8_lossy(top.url_str()), "https://a/");
    }

    #[test]
    fn form_popup_dispatches_doc_ops() {
        thread_local! {
            static CALLS: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
        }
        struct Rec;
        impl DocOps for Rec {
            fn load_images(&mut self, _d: usize, _p: Option<&DilloUrl>) {}
            fn form_submit(&mut self, doc: usize, form: usize) {
                CALLS.with(|c| c.borrow_mut().push((doc, form)));
            }
            fn form_reset(&mut self, _d: usize, _f: usize) {}
            fn form_display_hiddens(&mut self, _d: usize, _f: usize, _x: bool) {}
        }
        let (prefs, mut c, reg, uicmd, bw, _ui) = chrome_bw();
        c.set_doc_ops(Box::new(Rec));
        let page = url_new(b"https://ex.org/f", None).unwrap();
        reg.get(bw).unwrap().add_doc(11);
        reg.get(bw).unwrap().add_url(&page); // verbatim a_Bw_get_url_doc
        c.event_xy = (1, 2);
        uicmd.form_popup(&mut c, bw, &page, 42, false);
        let mut pop = c.take_popup().unwrap();
        assert_eq!(pop.items[2].label, "Show hiddens");
        c.popup_dispatch(&uicmd, &reg, &mut pop, 0, None, 1, false);
        CALLS.with(|v| assert_eq!(*v.borrow(), vec![(11, 42)]));
        drop(prefs);
    }

    #[test]
    fn chrome_widget_button_verbatim() {
        let (prefs, mut c, reg, uicmd, bw, _ui) = chrome_bw();
        let mut paste = || None;
        // Back right-click → history popup (verbatim b1_cb)
        chrome_widget(
            &uicmd,
            &reg,
            &mut c,
            prefs.as_ref(),
            bw,
            Hit::Wid(WId::Btn(UiButton::Back)),
            MOUSE_RIGHT,
            1,
            5,
            5,
            &mut paste,
        );
        assert!(c.take_popup().is_some());
        // FileMenu → file popup
        chrome_widget(
            &uicmd,
            &reg,
            &mut c,
            prefs.as_ref(),
            bw,
            Hit::Wid(WId::FileMenu),
            MOUSE_LEFT,
            1,
            5,
            5,
            &mut paste,
        );
        let pop = c.take_popup().unwrap();
        assert_eq!(pop.items[0].label, "New tab");
        // BugMeter right → bugmeter popup
        chrome_widget(
            &uicmd,
            &reg,
            &mut c,
            prefs.as_ref(),
            bw,
            Hit::Wid(WId::BugMeter),
            MOUSE_RIGHT,
            1,
            5,
            5,
            &mut paste,
        );
        let bp = c.take_popup().unwrap();
        assert_eq!(bp.items.len(), 3);
    }

    #[test]
    fn findbar_toggle_and_keys() {
        let (prefs, mut c, reg, uicmd, bw, ui) = chrome_bw();
        uicmd.findbar_toggle(&reg, &mut c, bw, true);
        assert!(c.ui(ui).unwrap().findbar_visible);
        assert!(c.ui(ui).unwrap().findbar_state.input_focused);
        // Escape → Pass (remonte à l'UI qui cache la barre)
        assert!(!findbar_key(
            &uicmd,
            &reg,
            &mut c,
            bw,
            keysym::ESCAPE,
            None,
            false,
            false,
            false,
            false,
        ));
        // texte + Enter → search dispatché (ne panique pas sans layout)
        c.ui_mut(ui).unwrap().findbar_state.input.buf = "x".into();
        assert!(findbar_key(
            &uicmd,
            &reg,
            &mut c,
            bw,
            keysym::ENTER,
            None,
            false,
            false,
            false,
            false,
        ));
        // Enter vide → consommé sans search
        c.ui_mut(ui).unwrap().findbar_state.input.buf.clear();
        assert!(findbar_key(
            &uicmd,
            &reg,
            &mut c,
            bw,
            keysym::ENTER,
            None,
            false,
            false,
            false,
            false,
        ));
        // HidePanels shortcut cache la barre (verbatim)
        let pr = prefs.borrow();
        ui_shortcut(&uicmd, &reg, &mut c, &pr, bw, KeysCommand::HidePanels);
        assert!(!c.ui(ui).unwrap().findbar_visible);
    }
}
