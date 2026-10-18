//! Port of `src/prefs.c` + `src/prefs.h` + `src/prefsparser.cc` —
//! `DilloPrefs`, défauts `a_Prefs_init` verbatim, parseur dillorc.
//!
//! Différence structurelle : pas de global — le `Prefs` est owned
//! par le navigateur et passé en paramètre (le `extern DilloPrefs
//! prefs` amont devient un champ du shell browser). Mêmes défauts,
//! mêmes types, même tolérance de parsing.

use std::collections::HashSet;
use std::io::BufRead;

use dillo_common::{colors, dlib, misc};
use dillo_net::url::{self, DilloUrl};

pub const PREFS_GEOMETRY_DEFAULT_WIDTH: i32 = 780;
pub const PREFS_GEOMETRY_DEFAULT_HEIGHT: i32 = 580;
pub const PREFS_GEOMETRY_DEFAULT_XPOS: i32 = -9999;
pub const PREFS_GEOMETRY_DEFAULT_YPOS: i32 = -9999;

/// `enum { P_tiny, P_small, P_medium }` amont.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PanelSize {
    Tiny = 0,
    Small,
    #[default]
    Medium,
}

/// `DilloPrefs` — champs verbatim (ordre amont). Les ~40 bools sont
/// la forme du réglage amont.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Prefs {
    pub width: i32,
    pub height: i32,
    pub xpos: i32,
    pub ypos: i32,
    pub http_language: Option<Vec<u8>>,
    pub http_max_conns: i32,
    pub http_proxy: Option<DilloUrl>,
    pub http_proxyuser: Option<Vec<u8>>,
    /// Extension `OnionBit` (absente amont) : mot de passe proxy lu dans
    /// le dillorc au lieu du dialog `a_UIcmd_get_passwd` — nécessaire
    /// pour un spawn headless (daemon → enfant sans interaction).
    pub http_proxypass: Option<Vec<u8>>,
    pub http_referer: Option<Vec<u8>>,
    pub http_user_agent: Option<Vec<u8>>,
    pub no_proxy: Option<Vec<u8>>,
    pub start_page: Option<DilloUrl>,
    pub home: Option<DilloUrl>,
    pub new_tab_page: Option<DilloUrl>,
    pub allow_white_bg: bool,
    pub white_bg_replacement: i32,
    pub bg_color: i32,
    pub ui_button_highlight_color: i32,
    pub ui_fg_color: i32,
    pub ui_main_bg_color: i32,
    pub ui_selection_color: i32,
    pub ui_tab_active_bg_color: i32,
    pub ui_tab_active_fg_color: i32,
    pub ui_tab_bg_color: i32,
    pub ui_tab_fg_color: i32,
    pub ui_tab_height: i32,
    pub ui_text_bg_color: i32,
    pub contrast_visited_color: bool,
    pub show_tooltip: bool,
    pub show_ui_tooltip: bool,
    pub theme: Option<Vec<u8>>,
    pub panel_size: PanelSize,
    pub small_icons: bool,
    pub limit_text_width: bool,
    pub adjust_min_width: bool,
    pub adjust_table_min_width: bool,
    pub focus_new_tab: bool,
    pub font_factor: f64,
    pub zoom_factor: f64,
    pub font_max_size: i32,
    pub font_min_size: i32,
    pub scroll_step: i32,
    pub scroll_page_overlap: i32,
    pub scrollbar_on_left: bool,
    pub scrollbar_page_mode: bool,
    pub show_back: bool,
    pub show_forw: bool,
    pub show_home: bool,
    pub show_reload: bool,
    pub show_save: bool,
    pub show_stop: bool,
    pub show_bookmarks: bool,
    pub show_tools: bool,
    pub show_filemenu: bool,
    pub show_clear_url: bool,
    pub show_url: bool,
    pub show_search: bool,
    pub show_help: bool,
    pub show_progress_box: bool,
    pub show_quit_dialog: bool,
    pub fullwindow_start: bool,
    pub load_images: bool,
    pub ignore_image_formats: Option<Vec<u8>>,
    pub mark_unloaded_images: bool,
    pub load_background_images: bool,
    pub load_stylesheets: bool,
    pub parse_embedded_css: bool,
    pub http_persistent_conns: bool,
    pub http_strict_transport_security: bool,
    pub http_force_https: bool,
    pub buffered_drawing: i32,
    pub font_serif: Option<Vec<u8>>,
    pub font_sans_serif: Option<Vec<u8>>,
    pub font_cursive: Option<Vec<u8>>,
    pub font_fantasy: Option<Vec<u8>>,
    pub font_monospace: Option<Vec<u8>>,
    pub enterpress_forces_submit: bool,
    pub middle_click_opens_new_tab: bool,
    pub right_click_closes_tab: bool,
    pub scroll_switches_tabs: bool,
    pub scroll_switches_tabs_reverse: bool,
    /// `bool_t` amont — mutation pendant `open_urlstr` (verbatim) →
    /// `Cell` pour rester accessible via `&Prefs`.
    pub search_url_idx: std::cell::Cell<bool>,
    pub search_urls: Vec<Vec<u8>>,
    pub save_dir: Option<Vec<u8>>,
    pub show_msg: bool,
    pub show_extra_warnings: bool,
    pub trace_http: bool,
    pub middle_click_drags_page: bool,
    pub penalty_hyphen: i32,
    pub penalty_hyphen_2: i32,
    pub penalty_em_dash_left: i32,
    pub penalty_em_dash_right: i32,
    pub penalty_em_dash_right_2: i32,
    pub stretchability_factor: i32,
    pub link_actions: Vec<Vec<u8>>,
    pub page_actions: Vec<Vec<u8>>,
}

const PREFS_START_PAGE: &[u8] = b"about:splash";
const PREFS_HOME: &[u8] = b"https://dillo-browser.org/";
const PREFS_NEW_TAB_PAGE: &[u8] = b"about:blank";
const PREFS_FONT_SERIF: &[u8] = b"DejaVu Serif";
const PREFS_FONT_SANS_SERIF: &[u8] = b"DejaVu Sans";
const PREFS_FONT_CURSIVE: &[u8] = b"DejaVu Sans";
const PREFS_FONT_FANTASY: &[u8] = b"DejaVu Sans";
const PREFS_FONT_MONOSPACE: &[u8] = b"DejaVu Sans Mono";
const PREFS_SEARCH_URL: &[u8] = b"dd http://duckduckgo.com/lite/?kp=-1&kd=-1&q=%s";
const PREFS_NO_PROXY: &[u8] = b"localhost 127.0.0.1";
const PREFS_SAVE_DIR: &[u8] = b"/tmp/";
const PREFS_HTTP_REFERER: &[u8] = b"host";
const PREFS_HTTP_USER_AGENT: &[u8] = b"Dillo/3.3.0-rc1"; // "Dillo/" VERSION amont
const PREFS_THEME: &[u8] = b"none";

impl Default for Prefs {
    /// `a_Prefs_init` — tous les défauts verbatim.
    fn default() -> Self {
        Self {
            allow_white_bg: true,
            white_bg_replacement: 0x00e0_e0a3,
            bg_color: 0x00dc_d1ba,
            buffered_drawing: 1,
            contrast_visited_color: true,
            enterpress_forces_submit: false,
            focus_new_tab: false,
            font_cursive: Some(PREFS_FONT_CURSIVE.to_vec()),
            font_factor: 1.0,
            zoom_factor: 1.0,
            font_max_size: 100,
            font_min_size: 6,
            font_fantasy: Some(PREFS_FONT_FANTASY.to_vec()),
            font_monospace: Some(PREFS_FONT_MONOSPACE.to_vec()),
            font_sans_serif: Some(PREFS_FONT_SANS_SERIF.to_vec()),
            font_serif: Some(PREFS_FONT_SERIF.to_vec()),
            fullwindow_start: false,
            width: PREFS_GEOMETRY_DEFAULT_WIDTH,
            height: PREFS_GEOMETRY_DEFAULT_HEIGHT,
            xpos: PREFS_GEOMETRY_DEFAULT_XPOS,
            ypos: PREFS_GEOMETRY_DEFAULT_YPOS,
            home: url::url_new(PREFS_HOME, None),
            http_language: None,
            http_proxy: None,
            http_max_conns: 6,
            http_persistent_conns: true,
            http_proxyuser: None,
            http_proxypass: None,
            http_referer: Some(PREFS_HTTP_REFERER.to_vec()),
            http_strict_transport_security: true,
            http_force_https: false,
            http_user_agent: Some(PREFS_HTTP_USER_AGENT.to_vec()),
            limit_text_width: false,
            adjust_min_width: true,
            adjust_table_min_width: true,
            load_images: true,
            ignore_image_formats: None,
            mark_unloaded_images: false,
            load_background_images: false,
            load_stylesheets: true,
            middle_click_drags_page: true,
            middle_click_opens_new_tab: true,
            right_click_closes_tab: true,
            scroll_switches_tabs: true,
            scroll_switches_tabs_reverse: false,
            no_proxy: Some(PREFS_NO_PROXY.to_vec()),
            link_actions: Vec::with_capacity(16),
            page_actions: Vec::with_capacity(16),
            panel_size: PanelSize::Medium,
            parse_embedded_css: true,
            save_dir: Some(PREFS_SAVE_DIR.to_vec()),
            scroll_step: 100,
            scroll_page_overlap: 50,
            search_urls: vec![PREFS_SEARCH_URL.to_vec()],
            search_url_idx: std::cell::Cell::new(false),
            scrollbar_on_left: false,
            scrollbar_page_mode: false,
            show_back: true,
            show_bookmarks: true,
            show_clear_url: true,
            show_extra_warnings: false,
            show_filemenu: true,
            show_forw: true,
            show_help: true,
            show_home: true,
            show_msg: true,
            show_progress_box: true,
            show_quit_dialog: false,
            show_reload: true,
            show_save: true,
            show_url: true,
            show_search: true,
            show_stop: true,
            show_tools: true,
            show_tooltip: true,
            show_ui_tooltip: true,
            small_icons: false,
            trace_http: false,
            start_page: url::url_new(PREFS_START_PAGE, None),
            new_tab_page: url::url_new(PREFS_NEW_TAB_PAGE, None),
            theme: Some(PREFS_THEME.to_vec()),
            ui_button_highlight_color: -1,
            ui_fg_color: -1,
            ui_main_bg_color: -1,
            ui_selection_color: -1,
            ui_tab_active_bg_color: -1,
            ui_tab_bg_color: -1,
            ui_tab_active_fg_color: -1,
            ui_tab_height: 20,
            ui_tab_fg_color: -1,
            ui_text_bg_color: -1,
            penalty_hyphen: 100,
            penalty_hyphen_2: 800,
            penalty_em_dash_left: 800,
            penalty_em_dash_right: 100,
            penalty_em_dash_right_2: 800,
            stretchability_factor: 100,
        }
    }
}

/// `strtod(…, NULL)` minimal : préfixe float valide, 0.0 sinon.
fn strtod(s: &[u8]) -> f64 {
    let mut end = 0;
    while end < s.len() && dlib::is_space(s[end]) {
        end += 1;
    }
    let start = end;
    if matches!(s.get(end), Some(b'+' | b'-')) {
        end += 1;
    }
    let mut digits = false;
    while end < s.len() && s[end].is_ascii_digit() {
        end += 1;
        digits = true;
    }
    if end < s.len() && s[end] == b'.' {
        end += 1;
        while end < s.len() && s[end].is_ascii_digit() {
            end += 1;
            digits = true;
        }
    }
    if digits && end < s.len() && matches!(s[end], b'e' | b'E') {
        let save = end;
        end += 1;
        if matches!(s.get(end), Some(b'+' | b'-')) {
            end += 1;
        }
        let exp_start = end;
        while end < s.len() && s[end].is_ascii_digit() {
            end += 1;
        }
        if end == exp_start {
            end = save; // 'e' sans chiffres → pas d'exposant
        }
    }
    if end == start {
        return 0.0;
    }
    std::str::from_utf8(&s[..end])
        .ok()
        .and_then(|t| t.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// `strtol(…, 10)` minimal (0 si pas de chiffres).
fn strtol(s: &[u8]) -> i32 {
    let mut i = 0;
    while i < s.len() && dlib::is_space(s[i]) {
        i += 1;
    }
    let neg = matches!(s.get(i), Some(b'-'));
    if matches!(s.get(i), Some(b'+' | b'-')) {
        i += 1;
    }
    let mut v: i64 = 0;
    let mut any = false;
    while i < s.len() && s[i].is_ascii_digit() {
        v = v.saturating_mul(10).saturating_add(i64::from(s[i] - b'0'));
        i += 1;
        any = true;
    }
    if !any {
        return 0;
    }
    let v = if neg { -v } else { v };
    i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(i32::MAX)
}

/// `PREFS_BOOL` : `yes`/`true` ci-insensibles.
fn parse_bool(v: &[u8]) -> bool {
    dlib::str_ascii_casecmp(v, b"yes") == 0 || dlib::str_ascii_casecmp(v, b"true") == 0
}

impl Prefs {
    /// `PrefsParser::parse` — lit le dillorc ligne à ligne.
    /// Erreurs de syntaxe loguées (comme amont), jamais fatales.
    ///
    /// # Errors
    /// Propage les erreurs `io` du reader.
    pub fn parse_dillorc(&mut self, reader: &mut impl BufRead) -> std::io::Result<()> {
        // `seen` : les clés STRINGS vident leur défaut au premier
        // occurrence (node->count == 0 amont)
        let mut strings_seen: HashSet<&'static str> = HashSet::new();
        let mut line_number = 1usize;
        while let Some(line) = dlib::getline(reader)? {
            match dlib::parse_rc_line(&line) {
                dlib::RcLine::Pair { name, value } => {
                    tracing::debug!(
                        name = ?String::from_utf8_lossy(&name),
                        value = ?String::from_utf8_lossy(&value),
                        "prefsparser"
                    );
                    self.parse_option(&name, &value, &mut strings_seen);
                }
                dlib::RcLine::Error => {
                    tracing::error!(
                        line_number,
                        line = ?String::from_utf8_lossy(&line),
                        "prefsparser: syntax error in dillorc"
                    );
                }
                dlib::RcLine::Skip => {}
            }
            line_number += 1;
        }
        Ok(())
    }

    /// `parseOption` — dispatch par nom (symbol table amont → match).
    /// Inconnu → warn + ignoré.
    #[allow(clippy::too_many_lines)] // miroir plat de la symbol table
    fn parse_option(
        &mut self,
        name: &[u8],
        value: &[u8],
        strings_seen: &mut HashSet<&'static str>,
    ) {
        // helper bool/color/string local
        let color = |cur: i32| -> i32 {
            let (c, st) = colors::color_parse(value, cur);
            if st == 1 {
                tracing::warn!(value = ?String::from_utf8_lossy(value), "prefs: color not recognized");
            }
            c
        };
        match name {
            b"allow_white_bg" => self.allow_white_bg = parse_bool(value),
            b"white_bg_replacement" => self.white_bg_replacement = color(self.white_bg_replacement),
            b"bg_color" => self.bg_color = color(self.bg_color),
            b"buffered_drawing" => self.buffered_drawing = strtol(value),
            b"contrast_visited_color" => self.contrast_visited_color = parse_bool(value),
            b"enterpress_forces_submit" => self.enterpress_forces_submit = parse_bool(value),
            b"focus_new_tab" => self.focus_new_tab = parse_bool(value),
            b"font_cursive" => self.font_cursive = Some(value.to_vec()),
            b"font_factor" => self.font_factor = strtod(value),
            b"font_fantasy" => self.font_fantasy = Some(value.to_vec()),
            b"font_max_size" => self.font_max_size = strtol(value),
            b"font_min_size" => self.font_min_size = strtol(value),
            b"font_monospace" => self.font_monospace = Some(value.to_vec()),
            b"font_sans_serif" => self.font_sans_serif = Some(value.to_vec()),
            b"font_serif" => self.font_serif = Some(value.to_vec()),
            b"fullwindow_start" => self.fullwindow_start = parse_bool(value),
            b"geometry" => {
                if let Some((x, y, w, h)) = misc::parse_geometry(value) {
                    if let Some(x) = x {
                        self.xpos = x;
                    }
                    if let Some(y) = y {
                        self.ypos = y;
                    }
                    self.width = w;
                    self.height = h;
                }
            }
            b"home" => self.home = url::url_new(value, None),
            b"new_tab_page" => self.new_tab_page = url::url_new(value, None),
            b"http_language" => self.http_language = Some(value.to_vec()),
            b"http_max_conns" => self.http_max_conns = strtol(value),
            b"http_persistent_conns" => self.http_persistent_conns = parse_bool(value),
            b"http_proxy" => self.http_proxy = url::url_new(value, None),
            b"http_proxyuser" => self.http_proxyuser = Some(value.to_vec()),
            b"http_proxypass" => self.http_proxypass = Some(value.to_vec()),
            b"http_referer" => self.http_referer = Some(value.to_vec()),
            b"http_strict_transport_security" => {
                self.http_strict_transport_security = parse_bool(value);
            }
            b"http_force_https" => self.http_force_https = parse_bool(value),
            b"http_user_agent" => self.http_user_agent = Some(value.to_vec()),
            b"limit_text_width" => self.limit_text_width = parse_bool(value),
            b"adjust_min_width" => self.adjust_min_width = parse_bool(value),
            b"adjust_table_min_width" => self.adjust_table_min_width = parse_bool(value),
            b"load_images" => self.load_images = parse_bool(value),
            b"mark_unloaded_images" => self.mark_unloaded_images = parse_bool(value),
            b"ignore_image_formats" => self.ignore_image_formats = Some(value.to_vec()),
            b"load_background_images" => self.load_background_images = parse_bool(value),
            b"load_stylesheets" => self.load_stylesheets = parse_bool(value),
            b"middle_click_drags_page" => self.middle_click_drags_page = parse_bool(value),
            b"middle_click_opens_new_tab" => self.middle_click_opens_new_tab = parse_bool(value),
            b"right_click_closes_tab" => self.right_click_closes_tab = parse_bool(value),
            b"scroll_switches_tabs" => self.scroll_switches_tabs = parse_bool(value),
            b"scroll_switches_tabs_reverse" => {
                self.scroll_switches_tabs_reverse = parse_bool(value);
            }
            b"no_proxy" => self.no_proxy = Some(value.to_vec()),
            b"panel_size" => {
                self.panel_size = if dlib::str_ascii_casecmp(value, b"tiny") == 0 {
                    PanelSize::Tiny
                } else if dlib::str_ascii_casecmp(value, b"small") == 0 {
                    PanelSize::Small
                } else {
                    PanelSize::Medium
                };
            }
            b"parse_embedded_css" => self.parse_embedded_css = parse_bool(value),
            b"save_dir" => self.save_dir = Some(value.to_vec()),
            b"scroll_step" => self.scroll_step = strtol(value),
            b"scroll_page_overlap" => self.scroll_page_overlap = strtol(value),
            b"scrollbar_on_left" => self.scrollbar_on_left = parse_bool(value),
            b"scrollbar_page_mode" => self.scrollbar_page_mode = parse_bool(value),
            b"search_url" => {
                if strings_seen.insert("search_url") {
                    self.search_urls.clear(); // override du défaut
                }
                self.search_urls.push(value.to_vec());
            }
            b"show_back" => self.show_back = parse_bool(value),
            b"show_bookmarks" => self.show_bookmarks = parse_bool(value),
            b"show_clear_url" => self.show_clear_url = parse_bool(value),
            b"show_extra_warnings" => self.show_extra_warnings = parse_bool(value),
            b"show_filemenu" => self.show_filemenu = parse_bool(value),
            b"show_forw" => self.show_forw = parse_bool(value),
            b"show_help" => self.show_help = parse_bool(value),
            b"show_home" => self.show_home = parse_bool(value),
            b"show_msg" => self.show_msg = parse_bool(value),
            b"show_progress_box" => self.show_progress_box = parse_bool(value),
            b"show_quit_dialog" => self.show_quit_dialog = parse_bool(value),
            b"show_reload" => self.show_reload = parse_bool(value),
            b"show_save" => self.show_save = parse_bool(value),
            b"show_url" => self.show_url = parse_bool(value),
            b"show_search" => self.show_search = parse_bool(value),
            b"show_stop" => self.show_stop = parse_bool(value),
            b"show_tools" => self.show_tools = parse_bool(value),
            b"show_tooltip" => self.show_tooltip = parse_bool(value),
            b"show_ui_tooltip" => self.show_ui_tooltip = parse_bool(value),
            b"trace_http" => self.trace_http = parse_bool(value),
            b"small_icons" => self.small_icons = parse_bool(value),
            b"start_page" => self.start_page = url::url_new(value, None),
            b"theme" => self.theme = Some(value.to_vec()),
            b"ui_button_highlight_color" => {
                self.ui_button_highlight_color = color(self.ui_button_highlight_color);
            }
            b"ui_fg_color" => self.ui_fg_color = color(self.ui_fg_color),
            b"ui_main_bg_color" => self.ui_main_bg_color = color(self.ui_main_bg_color),
            b"ui_selection_color" => self.ui_selection_color = color(self.ui_selection_color),
            b"ui_tab_active_bg_color" => {
                self.ui_tab_active_bg_color = color(self.ui_tab_active_bg_color);
            }
            b"ui_tab_bg_color" => self.ui_tab_bg_color = color(self.ui_tab_bg_color),
            b"ui_tab_active_fg_color" => {
                self.ui_tab_active_fg_color = color(self.ui_tab_active_fg_color);
            }
            b"ui_tab_fg_color" => self.ui_tab_fg_color = color(self.ui_tab_fg_color),
            b"ui_tab_height" => self.ui_tab_height = strtol(value),
            b"ui_text_bg_color" => self.ui_text_bg_color = color(self.ui_text_bg_color),
            b"penalty_hyphen" => self.penalty_hyphen = fraction_100(value),
            b"penalty_hyphen_2" => self.penalty_hyphen_2 = fraction_100(value),
            b"penalty_em_dash_left" => self.penalty_em_dash_left = fraction_100(value),
            b"penalty_em_dash_right" => self.penalty_em_dash_right = fraction_100(value),
            b"penalty_em_dash_right_2" => self.penalty_em_dash_right_2 = fraction_100(value),
            b"stretchability_factor" => self.stretchability_factor = fraction_100(value),
            b"link_action" => {
                if strings_seen.insert("link_action") {
                    self.link_actions.clear();
                }
                self.link_actions.push(value.to_vec());
            }
            b"page_action" => {
                if strings_seen.insert("page_action") {
                    self.page_actions.clear();
                }
                self.page_actions.push(value.to_vec());
            }
            b"zoom_factor" => self.zoom_factor = strtod(value),
            _ => {
                tracing::warn!(name = ?String::from_utf8_lossy(name), "prefs: not a recognized token");
            }
        }
    }
}

/// `PREFS_FRACTION_100` — `strtod × 100` → int (±`HUGE_VAL` → `INT_MIN/MAX`).
#[allow(clippy::cast_possible_truncation)]
fn fraction_100(v: &[u8]) -> i32 {
    let d = strtod(v);
    if d == f64::INFINITY {
        i32::MAX
    } else if d == f64::NEG_INFINITY {
        i32::MIN
    } else {
        (100.0 * d) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn defaults_verbatim() {
        let mut p = Prefs::default();
        assert_eq!(p.width, 780);
        assert_eq!(p.height, 580);
        assert!(p.load_images && !p.load_background_images);
        assert_eq!(p.http_max_conns, 6);
        assert_eq!(p.panel_size, PanelSize::Medium);
        assert_eq!(p.penalty_hyphen_2, 800);
        assert_eq!(p.search_urls.len(), 1);
        assert_eq!(
            p.home.as_mut().map(DilloUrl::url_str),
            Some(b"https://dillo-browser.org/".as_slice())
        );
        assert_eq!(p.save_dir.as_deref(), Some(b"/tmp/".as_slice()));
        assert_eq!(p.ui_tab_height, 20);
        assert_eq!(p.ui_fg_color, -1);
    }

    #[test]
    fn parse_file() {
        let rc: &[u8] = b"# comment\n\
            bg_color = #ff0000\n\
            scroll_step=50\n\
            load_images = no\n\
            geometry=800x600+10+20\n\
            search_url = g https://google.com/q=%s\n\
            search_url = dd https://ddg.com/?q=%s\n\
            panel_size=small\n\
            penalty_hyphen = 1.5\n\
            font_factor = 1.2\n\
            http_proxyuser = onionbit\n\
            http_proxypass = tok3n-s3ssion\n\
            unknown_key = yes\n";
        let mut p = Prefs::default();
        p.parse_dillorc(&mut Cursor::new(rc)).unwrap();
        assert_eq!(p.bg_color, 0xff_0000);
        assert_eq!(p.scroll_step, 50);
        assert!(!p.load_images);
        assert_eq!((p.width, p.height, p.xpos, p.ypos), (800, 600, 10, 20));
        // premier search_url a vidé le défaut, deux entrées
        assert_eq!(p.search_urls.len(), 2);
        assert_eq!(p.search_urls[0], b"g https://google.com/q=%s");
        assert_eq!(p.panel_size, PanelSize::Small);
        assert_eq!(p.penalty_hyphen, 150); // 1.5 × 100
        assert!((p.font_factor - 1.2).abs() < f64::EPSILON);
        // extension OnionBit : passwd proxy lu depuis le dillorc.
        assert_eq!(p.http_proxyuser.as_deref(), Some(b"onionbit".as_slice()));
        assert_eq!(
            p.http_proxypass.as_deref(),
            Some(b"tok3n-s3ssion".as_slice())
        );
    }

    #[test]
    fn strtod_prefix() {
        assert!((strtod(b"1.5") - 1.5).abs() < f64::EPSILON);
        assert!((strtod(b"-2.25e2") - -225.0).abs() < f64::EPSILON);
        assert_eq!(strtod(b"xyz"), 0.0);
        assert!((strtod(b"3.5junk") - 3.5).abs() < f64::EPSILON);
        assert_eq!(strtod(b"1e"), 1.0); // 'e' sans chiffres → ignoré
    }
}
