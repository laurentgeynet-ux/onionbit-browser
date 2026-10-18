//! Port of `src/uicmd.cc` + `uicmd.hh` — commandes déclenchées depuis
//! l'UI (onglets, fenêtres, URL, zoom, save, source, historique,
//! findtext, sensibilité des boutons).
//!
//! Seam [`UiShell`] ≙ `UI*`/`CustTabs`/`Fl_Window` + `a_Dialog_*` +
//! `a_Menu_*` + `a_Control_*` + `Paths` : implémentée par le chrome
//! natif (`ui.rs`). Les handles `ui`/`tabs`/`wid`/`form` sont des
//! `usize` opaques ≙ pointeurs amont ; `bw` = `bw_id` (index
//! [`BwRegistry`] ≙ `BrowserWindow*`).
//!
//! État module amont porté dans [`UiCmd`] : `save_dir` (static) et
//! `post_id` (static dans `view_page_source`).
//!
//! La dispatch `nav.c` → `a_UIcmd_*` passe par `NavSeams`
//! ([`NavIo`](crate::nav::NavIo)+[`NavSched`]
//! liant `reg`+`shell` le temps de l'appel —
//! verbatim, amont les deux sont globaux).

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dillo_common::dlib;
use dillo_common::misc;
use dillo_common::utf8::end_of_char;
use dillo_dw::layout::FindtextResult;
use dillo_dw::types::{HPosition, ScrollCommand, VPosition};
use dillo_net::history;
use dillo_net::url::{self, url_new, DilloUrl};

use crate::bw::{BwRegistry, BW_FORCE, BW_IMG, BW_ROOT};
use crate::keys::{Keys, KeysCommand};
use crate::nav::{NavOp, NavSched, NavUi};
use crate::prefs::Prefs;

/// `UIButton` (verbatim `ui.hh`) — index des boutons du panneau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum UiButton {
    /// `UI_BACK`.
    Back = 0,
    /// `UI_FORW`.
    Forw,
    /// `UI_HOME`.
    Home,
    /// `UI_RELOAD`.
    Reload,
    /// `UI_SAVE`.
    Save,
    /// `UI_STOP`.
    Stop,
    /// `UI_BOOK`.
    Book,
    /// `UI_TOOLS`.
    Tools,
    /// `UI_CLEAR`.
    Clear,
    /// `UI_SEARCH`.
    Search,
}

/// `FL_LEFT_MOUSE` (verbatim — `Fl::event_button()`).
pub const BUTTON_LEFT: i32 = 1;
/// `FL_MIDDLE_MOUSE`.
pub const BUTTON_MIDDLE: i32 = 2;
/// `FL_RIGHT_MOUSE`.
pub const BUTTON_RIGHT: i32 = 3;

/// Seam chrome ≙ `UI`/`CustTabs`/`Fl_Window` + `a_Dialog_*` +
/// `a_Menu_*` + `a_Control_*`/`Paths`/`a_Timeout_add`.
///
/// Tous les objets sont identifiés par handles `usize` ≙ pointeurs
/// amont : `ui` = `UI*` (un par onglet), `tabs` = `CustTabs*` (un par
/// fenêtre), `wid` = widget FLTK quelconque, `form` = `DilloHtmlForm*`.
pub trait UiShell {
    // ---- fenêtres / onglets (≙ CustTabs + Fl_Window) ---------------
    /// `new Fl_Window` + `new CustTabs` + push `Tabgroup`. Retourne le
    /// handle `tabs` du groupe créé (≙ `CustTabs*`).
    fn window_new(&mut self, w: i32, h: i32) -> usize;
    /// `win->end()` + `win->show()` + `win->callback(win_cb)` —
    /// appelé après le premier `tab_ui_new` (verbatim
    /// `a_UIcmd_browser_window_new`).
    fn window_show(&mut self, tabs: usize);
    /// `tabs->window()->position(x, y)` (prefs `xpos`/`ypos` ≥ 0).
    fn window_position(&mut self, tabs: usize, x: i32, y: i32);
    /// `CustTabs::add_new_tab` + corps widget de `UIcmd_tab_new` :
    /// `new UI`, `Layout` + viewport attaché (scroll step, barres,
    /// options verbatim depuis `prefs` — côté impl). Retourne
    /// `(ui_handle, layout)` ; `old_ui` = 0 si aucun.
    fn tab_ui_new(
        &mut self,
        tabs: usize,
        old_ui: usize,
        focus: bool,
    ) -> (usize, Rc<RefCell<dillo_dw::layout::Layout>>);
    /// `CustTabs::remove_tab` + retrait du `Tabgroup` et
    /// `delete tabs->window()` quand `num_tabs() == 0` (verbatim
    /// `a_UIcmd_close_bw`).
    fn tab_ui_remove(&mut self, ui: usize);
    /// `tabs->num_tabs()` pour le groupe `tabs`.
    fn tab_count(&self, tabs: usize) -> i32;
    /// `ui->tabs()` — groupe propriétaire de `ui` (`None` ≙ `NULL`).
    fn tabs_of(&self, ui: usize) -> Option<usize>;
    /// `tabs->switch_tab(index)` (0-based).
    fn switch_tab(&mut self, tabs: usize, index: i32);
    /// `tabs->switch_tab(btn)` — vers l'onglet dont le handle est `ui`.
    fn switch_to_tab(&mut self, ui: usize);
    /// `tabs->prev_tab()` / `next_tab()` (wrap-around verbatim).
    fn prev_tab(&mut self, tabs: usize);
    /// `tabs->next_tab()`.
    fn next_tab(&mut self, tabs: usize);
    /// `tabs->set_tab_label(ui, label)` — la troncature
    /// (15 chars, borne UTF-8, `...`) est verbatim côté impl.
    fn set_tab_label(&mut self, ui: usize, label: &str);
    /// `ui == ui->tabs()->wizard()->value()` (onglet actif de sa
    /// fenêtre ≙ `get_bw_by_widget(...) == bw` amont).
    fn is_active_tab(&self, ui: usize) -> bool;
    /// `tabs->wizard()->value()` — ui active du groupe.
    fn tabs_active_ui(&self, tabs: usize) -> usize;
    /// ui active de chaque `Tabgroup` (≙ boucle sur `tabgroups`).
    fn active_ui_handles(&self) -> Vec<usize>;
    /// `ui->contains(wid)` — `wid` appartient à cette `UI`.
    fn ui_contains(&self, ui: usize, wid: usize) -> bool;
    /// `ui->w()`/`ui->h()`.
    fn ui_wh(&self, ui: usize) -> (i32, i32);
    /// `ui->label()`.
    fn ui_label(&self, ui: usize) -> String;
    /// `ui->copy_label(title)` (label interne de l'onglet —
    /// `a_UIcmd_set_page_title` amont).
    fn ui_copy_label(&mut self, ui: usize, title: &str);
    /// `UIcmd_set_window_labels` ≙ `win->copy_label` + `label(c,c)`
    /// (titre fenêtre + icône).
    fn set_window_title(&mut self, ui: usize, title: &str);
    /// `ui->focus_main()` (zone rendue).
    fn focus_main(&mut self, ui: usize);
    /// `ui->focus_location()`.
    fn focus_location(&mut self, ui: usize);
    /// `ui->get_location()`.
    fn get_location(&self, ui: usize) -> String;
    /// `ui->set_location(text)`.
    fn set_location(&mut self, ui: usize, text: &str);
    /// `ui->set_status(str)`.
    fn set_status(&mut self, ui: usize, msg: &str);
    /// `ui->set_page_prog(nbytes, cmd)` — cmd : 0 désactive,
    /// 1 update, 2 clear (verbatim uicmd.hh doc).
    fn set_page_prog(&mut self, ui: usize, nbytes: usize, cmd: i32);
    /// `ui->set_img_prog(n_img, t_img, cmd)`.
    fn set_img_prog(&mut self, ui: usize, n_img: i32, t_img: i32, cmd: i32);
    /// `ui->set_bug_prog(n_bug)`.
    fn set_bug_prog(&mut self, ui: usize, n_bug: i32);
    /// `ui->button_set_sens(btn, sens)`.
    fn button_set_sens(&mut self, ui: usize, btn: UiButton, sens: bool);
    /// `ui->panels_toggle()`.
    fn panels_toggle(&mut self, ui: usize);
    /// `ui->get_panelsize()`/`ui->get_smallicons()` — état courant
    /// (menu.cc `Menu_panel_change_cb`/`a_Menu_tools_popup`).
    fn panel_state(&self, ui: usize) -> (i32, bool);
    /// `ui->change_panel(size, small_icons)` (menu.cc).
    fn panel_change(&mut self, ui: usize, size: i32, small_icons: bool);
    /// `ui->findbar_toggle(on)`.
    fn findbar_toggle(&mut self, ui: usize, on: bool);

    // ---- a_Dialog_* -------------------------------------------------
    /// `a_Dialog_choice` — retourne le choix **1-based** (`0` = fenêtre
    /// fermée ≙ `choice_answer = 0` amont).
    fn dialog_choice(&mut self, title: &str, msg: &str, alts: &[&str]) -> i32;
    /// `a_Dialog_input`.
    fn dialog_input(&mut self, title: &str, msg: &str) -> Option<String>;
    /// `a_Dialog_passwd`.
    fn dialog_passwd(&mut self, title: &str, msg: &str) -> Option<String>;
    /// `a_Dialog_save_file`.
    fn dialog_save_file(
        &mut self,
        title: &str,
        msg: Option<&str>,
        suggested: &str,
    ) -> Option<String>;
    /// `a_Dialog_select_file`.
    fn dialog_select_file(
        &mut self,
        title: &str,
        msg: Option<&str>,
        init: Option<&str>,
    ) -> Option<String>;
    /// `a_Dialog_open_file`.
    fn dialog_open_file(&mut self, title: &str, msg: Option<&str>, pattern: &str)
        -> Option<String>;
    /// `a_Dialog_text_window`.
    fn dialog_text_window(&mut self, title: &str, txt: &str);
    /// `a_Dialog_msg`.
    fn dialog_msg(&mut self, title: &str, msg: &str);

    // ---- a_Menu_* ---------------------------------------------------
    /// `a_Menu_history_popup` — `list` = `a_UIcmd_get_history(bw,dir)`
    /// calculé par l'appelant (le registre `bw` vit hors du shell) ;
    /// `dir` = -1 back / +1 forward.
    fn history_popup(&mut self, bw: u32, list: Vec<i32>, x: i32, y: i32, dir: i32);
    /// `a_Menu_page_popup`.
    fn page_popup(
        &mut self,
        bw: u32,
        url: Option<&DilloUrl>,
        has_bugs: bool,
        css_urls: &[DilloUrl],
    );
    /// `a_Menu_link_popup`.
    fn link_popup(&mut self, bw: u32, url: &DilloUrl, page_url: Option<&DilloUrl>);
    /// `a_Menu_image_popup`.
    fn image_popup(
        &mut self,
        bw: u32,
        url: &DilloUrl,
        loaded_img: bool,
        page_url: Option<&DilloUrl>,
        link_url: Option<&DilloUrl>,
    );
    /// `a_Menu_form_popup` — `form` ≙ `DilloHtmlForm*` opaque.
    fn form_popup(&mut self, bw: u32, url: &DilloUrl, form: usize, showing_hiddens: bool);
    /// `a_Menu_file_popup` — `(x,y)` ≙ `wid->x(), wid->y()+wid->h()`
    /// (l'appelant résout le rect du widget).
    fn file_popup(&mut self, bw: u32, x: i32, y: i32);
    /// `a_Menu_bugmeter_popup`.
    fn bugmeter_popup(&mut self, bw: u32, url: Option<&DilloUrl>);
    /// `a_Menu_tools_popup` — `ui` ≙ `bw->ui` (état du panel lu au
    /// build des toggles/radios).
    fn tools_popup(&mut self, bw: u32, ui: usize, x: i32, y: i32);

    // ---- misc seams --------------------------------------------------
    /// `Paths::getOldWorkingDir()` — répertoire de lancement (base des
    /// `file:` relatifs amont).
    fn old_working_dir(&self) -> Vec<u8>;
    /// `a_Control_is_waiting` (dilloc `--`: un client attend la fin).
    fn control_is_waiting(&self) -> bool;
    /// `a_Control_notify_finish`.
    fn control_notify_finish(&mut self, bw: u32);
    /// `Fl::flush()` — présentation après drain des idles.
    fn flush(&mut self);
    /// `a_Timeout_add(0.0, …)` pour les opérations de navigation
    /// différées ([`NavOp`]) — verbatim nav.c.
    fn defer_nav(&mut self, bw: u32, op: NavOp);
    /// `a_Timeout_add(0.0, a_UIcmd_close_all_bw, NULL)`
    /// (`CustTabs::handle` `KEYS_CLOSE_ALL`).
    fn defer_close_all(&mut self);
    /// `a_Bookmarks_add(bw, url)` (bookmark.c — seam : le store vit
    /// côté shell).
    fn add_bookmark(&mut self, bw: u32, url: &DilloUrl);
}

/// Dispatch `nav.c` → `a_UIcmd_*` (verbatim : les `a_UIcmd_*` amont
/// retrouvent `bw` et `UI*` globaux — ici [`NavSeams`] lie le registre
/// et le shell pour la durée d'un appel [`crate::nav::Nav`]).
pub(crate) struct NavSeams<'a> {
    pub reg: &'a BwRegistry,
    pub shell: &'a mut dyn UiShell,
    pub prefs: &'a Prefs,
    pub uicmd: &'a UiCmd,
}

impl crate::nav::NavBw for NavSeams<'_> {
    fn expecting(&self, bw: u32) -> bool {
        self.reg.get(bw).is_some_and(|b| b.expecting())
    }
    fn cancel_expect(&self, bw: u32) {
        if let Some(b) = self.reg.get(bw) {
            b.cancel_expect();
        }
    }
    fn expected_url(&self, bw: u32) -> Option<DilloUrl> {
        self.reg.get(bw).and_then(|b| b.expected_url())
    }
    fn expect(&self, bw: u32, url: &DilloUrl) {
        if let Some(b) = self.reg.get(bw) {
            b.expect(url);
        }
    }
    fn stop_clients(&self, bw: u32, which: i32) {
        if let Some(b) = self.reg.get(bw) {
            b.stop_clients(which);
        }
    }
    fn cleanup(&self, bw: u32) {
        if let Some(b) = self.reg.get(bw) {
            b.cleanup();
        }
    }
    fn add_client(&self, bw: u32, key: i32, root: bool) {
        crate::nav::NavBw::add_client(self.reg, bw, key, root);
    }
    fn add_url(&self, bw: u32, url: &DilloUrl) {
        if let Some(b) = self.reg.get(bw) {
            b.add_url(url);
        }
    }
    fn bg_color(&self, bw: u32) -> i32 {
        self.reg.get(bw).map_or(0x00ff_ffff, |b| b.bg_color.get())
    }
}

impl NavUi for NavSeams<'_> {
    fn set_msg(&mut self, bw: u32, msg: &str) {
        uicmd_set_msg(self.reg, self.shell, bw, msg);
    }
    fn set_location_text(&mut self, bw: u32, text: &[u8]) {
        uicmd_set_location_text(self.reg, self.shell, bw, text);
    }
    fn set_buttons_sens(&mut self, bw: u32, ptr: i32, size: i32, top: Option<DilloUrl>) {
        uicmd_set_buttons_sens_impl(self.reg, self.shell, bw, ptr, size, top);
    }
    fn get_scroll_xy(&mut self, bw: u32) -> (i32, i32) {
        uicmd_get_scroll_xy(self.reg, bw)
    }
    fn set_scroll_xy(&mut self, bw: u32, x: i32, y: i32) {
        uicmd_set_scroll_xy(self.reg, bw, x, y);
    }
    fn set_scroll_by_fragment(&mut self, bw: u32, fragment: &[u8]) {
        uicmd_set_scroll_by_fragment(self.reg, bw, fragment);
    }
    fn finish_loading(&mut self, bw: u32) {
        uicmd_finish_loading(self.reg, self.shell, bw);
    }
    fn open_url_nt(&mut self, bw: u32, url: &DilloUrl, offset: i32) {
        self.uicmd
            .open_url_nt(self.reg, self.shell, self.prefs, bw, Some(url), offset);
    }
    fn open_url_nw(&mut self, bw: u32, url: &DilloUrl) {
        self.uicmd
            .open_url_nw(self.reg, self.shell, self.prefs, bw, Some(url));
    }
    fn dialog_choice(&mut self, title: &str, msg: &str, alt1: &str, alt2: &str, alt3: &str) -> i32 {
        self.shell.dialog_choice(title, msg, &[alt1, alt2, alt3])
    }
}

impl NavSched for NavSeams<'_> {
    fn defer(&mut self, bw: u32, op: NavOp) {
        self.shell.defer_nav(bw, op);
    }
}

/// `uicmd.cc` — état module (`save_dir`, `post_id` statiques amont).
pub struct UiCmd {
    /// `static const char *save_dir` — initialisé par [`UiCmd::new`]
    /// (≙ `a_UIcmd_init`), `/` final garanti.
    save_dir: Vec<u8>,
    /// `static int post_id` dans `a_UIcmd_view_page_source` (verbatim).
    post_id: Cell<i32>,
}

impl UiCmd {
    /// `a_UIcmd_init` (verbatim) — `save_dir` depuis `prefs.save_dir`
    /// avec `/` final.
    #[must_use]
    pub fn new(prefs: &Prefs) -> Self {
        let mut save_dir = Vec::new();
        if let Some(dir) = &prefs.save_dir {
            if !dir.is_empty() {
                save_dir.clone_from(dir);
                // assert a trailing '/'
                if save_dir.last() != Some(&b'/') {
                    save_dir.push(b'/');
                }
            }
        }
        UiCmd {
            save_dir,
            post_id: Cell::new(0),
        }
    }

    /// `a_UIcmd_browser_window_new` (verbatim — `xid`/Xembed non porté :
    /// cf. `docs/PORTING.md`, xembed.cc est `won't port`).
    ///
    /// `ww <= 0 || wh <= 0` → géométrie `prefs.width`/`height`.
    /// Retourne le `bw_id` créé.
    pub fn browser_window_new(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        mut ww: i32,
        mut wh: i32,
        old_bw: Option<u32>,
    ) -> u32 {
        let old_ui = old_bw.and_then(|b| reg.get(b)).map_or(0, |b| b.ui.get());

        if ww <= 0 || wh <= 0 {
            // Set default geometry from dillorc.
            ww = prefs.width;
            wh = prefs.height;
        }

        let tabs = shell.window_new(ww, wh);
        let focus = true;
        let new_bw = self.tab_new(reg, shell, prefs, tabs, old_ui, focus);
        shell.window_show(tabs);

        if old_bw.is_none() {
            if let Some(b) = reg.get(new_bw) {
                b.zoom.set(prefs.zoom_factor as f32);
            }
            if prefs.xpos >= 0 && prefs.ypos >= 0 {
                // position the first window according to preferences
                shell.window_position(tabs, prefs.xpos, prefs.ypos);
            }
        }
        new_bw
    }

    /// `UIcmd_tab_new` (verbatim) — nouvel onglet dans le groupe
    /// `tabs` : `UI` + `Layout` côté shell ([`UiShell::tab_ui_new`]),
    /// `a_Bw_new` + `bw->ui`/`bw->render_layout` + titre fenêtre +
    /// `findbar_toggle(0)` (workaround amont).
    #[allow(clippy::unused_self)]
    fn tab_new(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        tabs: usize,
        old_ui: usize,
        focus: bool,
    ) -> u32 {
        let (new_ui, layout) = shell.tab_ui_new(tabs, old_ui, focus);

        // verbatim : bgColor/bgImage du layout depuis prefs.
        {
            let mut lg = layout.borrow_mut();
            lg.set_bg_color(Some(dillo_dw::style::Color::new(prefs.bg_color)));
            lg.set_bg_image(
                None,
                dillo_dw::style::BackgroundRepeat::Repeat,
                dillo_dw::style::BackgroundAttachment::Scroll,
                dillo_dw::style::create_per_length(0.0),
                dillo_dw::style::create_per_length(0.0),
            );
        }

        // Now, create a new browser window structure
        let new_bw = reg.add(super::bw::BrowserWindow::new());
        if let Some(b) = reg.get(new_bw) {
            b.ui.set(new_ui);
            *b.layout.borrow_mut() = Some(layout);
        }

        // Clear the window title
        if focus {
            let label = shell.ui_label(new_ui);
            shell.set_window_title(new_ui, &label);
        }

        // WORKAROUND: see findbar_toggle()
        shell.findbar_toggle(new_ui, false);
        new_bw
    }

    /// `a_UIcmd_close_bw` (verbatim) — stop clients, détruit le
    /// layout, retire l'onglet (+ fenêtre si dernier), `a_Bw_free`.
    pub fn close_bw(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        b.stop_clients(BW_ROOT | BW_IMG | BW_FORCE);
        *b.layout.borrow_mut() = None; // ≙ delete(layout)
        let ui = b.ui.get();
        shell.tab_ui_remove(ui); // ≙ remove_tab + window delete si vide
        reg.free(bw);
    }

    /// `a_UIcmd_close_all_bw` (verbatim) — `force` ≙ pointeur non NULL
    /// amont (jamais de dialogue).
    pub fn close_all_bw(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        force: bool,
    ) {
        let mut choice = 1;
        if !force && prefs.show_quit_dialog && reg.len() > 1 {
            choice = shell.dialog_choice(
                "OnionBit-Browser: Quit?",
                "More than one open tab or window.",
                &["Quit", "Cancel"],
            );
        }
        if force || choice == 1 {
            while let Some(b) = reg.get(0) {
                let _ = b;
                self.close_bw(reg, shell, 0);
            }
        }
    }

    /// `UIcmd_find_search_str` (verbatim) — `str` commence par le
    /// préfixe d'un nom de moteur + espace → URL de recherche.
    fn find_search_str(prefs: &Prefs, s: &[u8]) -> Option<Vec<u8>> {
        let len = s.iter().position(|&c| c == b' ').unwrap_or(s.len());
        if len > 0 && len < s.len() {
            /* we found a ' ' in str, check whether the first part of str
             * is a prefix of a search_url label */
            for (p, search) in prefs.search_urls.iter().enumerate() {
                if dlib::strn_ascii_casecmp(s, search, len) == 0 {
                    /* Are we partially matching another entry? */
                    if search.get(len).copied().unwrap_or(0) != b' ' {
                        continue;
                    }
                    // verbatim : `prefs.search_url_idx` est `bool_t`
                    // amont (index tronqué à 0/1 — quirk préservé).
                    prefs.search_url_idx.set(p != 0);
                    return Some(Self::make_search_str(prefs, &s[len + 1..]));
                }
            }
        }
        None
    }

    /// `a_UIcmd_open_urlstr` (verbatim) — filtre `search_prefix`,
    /// strip délimiteurs, normalisation `file:`.
    pub fn open_urlstr(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        urlstr: &[u8],
    ) {
        let search = Self::find_search_str(prefs, urlstr);
        let urlstr: &[u8] = search.as_deref().unwrap_or(urlstr);
        if !urlstr.is_empty() {
            /* Filter URL string */
            let new_urlstr = url::string_strip_delimiters(urlstr);

            let url = if new_urlstr.len() >= 5
                && dlib::strn_ascii_casecmp(&new_urlstr, b"file:", 5) == 0
            {
                /* file URI */
                let ch = new_urlstr.get(5).copied().unwrap_or(0);
                if ch == 0 || ch == b'.' {
                    url_new(&shell.old_working_dir(), Some(b"file:"))
                } else {
                    url_new(&new_urlstr, Some(b"file:"))
                }
            } else {
                /* common case */
                url_new(&new_urlstr, None)
            };

            if let Some(u) = url {
                self.open_url(reg, shell, prefs, bw, Some(&u));
            }
        }
    }

    /// `a_UIcmd_open_url` (verbatim) — push + focus main ; `None` ≙
    /// `NULL` (bw vierge → focus location + boutons).
    pub fn open_url(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        url: Option<&DilloUrl>,
    ) {
        let Some(b) = reg.get(bw) else { return };
        if let Some(url) = url {
            {
                let mut seams = NavSeams {
                    reg,
                    shell,
                    prefs,
                    uicmd: self,
                };
                b.nav.borrow_mut().push(bw, url, None, &mut seams);
            }
            shell.focus_main(b.ui.get());
        } else {
            // Used to start a bw with a blank screen
            shell.focus_location(b.ui.get());
            Self::set_buttons_sens(reg, shell, bw);
        }
    }

    /// `UIcmd_open_url_nbw` (verbatim) — `new_tab_page` si pas d'URL.
    fn open_url_nbw(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        new_bw: u32,
        url: Option<&DilloUrl>,
    ) {
        let mut url = url.cloned();
        if url.is_none() {
            if let Some(ntp) = &prefs.new_tab_page {
                if ntp.clone().url_str() != b"about:blank" {
                    url = Some(ntp.clone());
                }
            }
        }

        /* When opening a new BrowserWindow (tab or real window) we focus
         * Location if we don't yet have an URL, main otherwise. */
        let Some(b) = reg.get(new_bw) else { return };
        if let Some(u) = &url {
            {
                let mut seams = NavSeams {
                    reg,
                    shell,
                    prefs,
                    uicmd: self,
                };
                b.nav.borrow_mut().push(new_bw, u, None, &mut seams);
            }
            let text = u.clone().url_str().to_vec();
            uicmd_set_location_text(reg, shell, new_bw, &text);
            shell.focus_main(b.ui.get());
        } else {
            shell.focus_location(b.ui.get());
            Self::set_buttons_sens(reg, shell, new_bw);
        }
    }

    /// `a_UIcmd_open_url_nw` (verbatim — `prefs` ≙ globals amont) —
    /// même géométrie + même zoom.
    pub fn open_url_nw(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        url: Option<&DilloUrl>,
    ) {
        let (w, h) = self.get_wh(reg, shell, bw);
        let new_bw = self.browser_window_new(reg, shell, prefs, w, h, Some(bw));

        /* Preserve same zoom factor in new window */
        let zoom = reg.get(bw).map_or(1.0, |b| b.zoom.get());
        if let Some(nb) = reg.get(new_bw) {
            nb.zoom.set(zoom);
        }
        self.open_url_nbw(reg, shell, prefs, new_bw, url);
    }

    /// `a_UIcmd_open_url_nt` (verbatim — `prefs` ≙ globals amont) —
    /// nouvel onglet de la fenêtre de `bw` ; `focus` non nul ≙ verbatim
    /// (`back_nt` passe `-1`).
    pub fn open_url_nt(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        url: Option<&DilloUrl>,
        focus: i32,
    ) {
        let Some(b) = reg.get(bw) else { return };
        let ui = b.ui.get();
        let old_ui = ui;
        let Some(tabs) = shell.tabs_of(ui) else {
            return;
        };
        let new_bw = self.tab_new(reg, shell, prefs, tabs, old_ui, focus != 0);
        /* Preserve same zoom factor in new tab */
        let zoom = b.zoom.get();
        if let Some(nb) = reg.get(new_bw) {
            nb.zoom.set(zoom);
        }
        self.open_url_nbw(reg, shell, prefs, new_bw, url);
    }

    /// `a_Nav_expect_done` — appelé par capi quand la réponse racine
    /// arrive (verbatim — wrapper public ≙ l'entrée `nav.h`).
    pub fn expect_done(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        let mut seams = NavSeams {
            reg,
            shell,
            prefs,
            uicmd: self,
        };
        b.nav.borrow_mut().expect_done(bw, &mut seams);
    }

    /// `a_UIcmd_back` — verbatim `a_Nav_back`.
    pub fn back(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        let mut seams = NavSeams {
            reg,
            shell,
            prefs,
            uicmd: self,
        };
        b.nav.borrow_mut().back(bw, &mut seams);
    }

    /// `a_UIcmd_back_nt` — verbatim `a_Nav_back_nt`.
    pub fn back_nt(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        let mut seams = NavSeams {
            reg,
            shell,
            prefs,
            uicmd: self,
        };
        b.nav.borrow_mut().back_nt(bw, &mut seams);
    }

    /// `a_UIcmd_back_popup` — verbatim (`dir = -1` ; `get_history`
    /// fait ici ≙ `a_Menu_history_popup` amont).
    pub fn back_popup(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32, x: i32, y: i32) {
        let list = self.get_history(reg, bw, -1);
        shell.history_popup(bw, list, x, y, -1);
    }

    /// `a_UIcmd_forw` — verbatim `a_Nav_forw`.
    pub fn forw(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        let mut seams = NavSeams {
            reg,
            shell,
            prefs,
            uicmd: self,
        };
        b.nav.borrow_mut().forw(bw, &mut seams);
    }

    /// `a_UIcmd_forw_nt` — verbatim `a_Nav_forw_nt`.
    pub fn forw_nt(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        let mut seams = NavSeams {
            reg,
            shell,
            prefs,
            uicmd: self,
        };
        b.nav.borrow_mut().forw_nt(bw, &mut seams);
    }

    /// `a_UIcmd_forw_popup` — verbatim (`dir = +1`).
    pub fn forw_popup(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32, x: i32, y: i32) {
        let list = self.get_history(reg, bw, 1);
        shell.history_popup(bw, list, x, y, 1);
    }

    /// `a_UIcmd_home` (verbatim — `nt` ≙ nouvel onglet).
    pub fn home(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        nt: bool,
    ) {
        let home = prefs.home.clone();
        if nt {
            self.open_url_nt(reg, shell, prefs, bw, home.as_ref(), 1);
        } else {
            self.open_url(reg, shell, prefs, bw, home.as_ref());
        }
    }

    /// `a_UIcmd_reload` — verbatim `a_Nav_reload` (différé).
    pub fn reload(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        let mut seams = NavSeams {
            reg,
            shell,
            prefs,
            uicmd: self,
        };
        b.nav.borrow_mut().reload(bw, &mut seams);
    }

    /// `a_UIcmd_get_first_active_bw` (verbatim) — premier `bw` de
    /// l'onglet actif de chaque `Tabgroup`.
    pub fn first_active_bw(reg: &BwRegistry, shell: &mut dyn UiShell) -> Option<u32> {
        for ui in shell.active_ui_handles() {
            if let Some(bw) = Self::get_bw_by_widget(reg, shell, ui) {
                return Some(bw);
            }
        }
        None
    }

    /// `a_UIcmd_reload_all_active` (verbatim).
    pub fn reload_all_active(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs) {
        for ui in shell.active_ui_handles() {
            if let Some(bw) = Self::get_bw_by_widget(reg, shell, ui) {
                self.reload(reg, shell, prefs, bw);
            }
        }
    }

    /// `a_UIcmd_repush` — verbatim `a_Nav_repush` (différé).
    pub fn repush(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        let mut seams = NavSeams {
            reg,
            shell,
            prefs,
            uicmd: self,
        };
        b.nav.borrow_mut().repush(bw, &mut seams);
    }

    /// `a_UIcmd_redirection0` — verbatim `a_Nav_redirection0`.
    pub fn redirection0(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        url: &DilloUrl,
    ) {
        let Some(b) = reg.get(bw) else { return };
        let mut seams = NavSeams {
            reg,
            shell,
            prefs,
            uicmd: self,
        };
        b.nav.borrow_mut().redirection0(bw, url, &mut seams);
    }

    /// Corps des opérations différées ([`NavOp`] via
    /// [`UiShell::defer_nav`]) — appelé par le shell quand le timeout
    /// 0 expire (≙ `Nav_*_callback` amont).
    pub fn nav_op_now(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        op: NavOp,
    ) {
        // `CloseBw` ≙ `a_Timeout_add(0.0, a_UIcmd_close_bw, bw)` —
        // n'emprunte pas `nav` (close_bw reprend `b` via `reg`).
        if matches!(op, NavOp::CloseBw) {
            self.close_bw(reg, shell, bw);
            return;
        }
        let Some(b) = reg.get(bw) else { return };
        let mut seams = NavSeams {
            reg,
            shell,
            prefs,
            uicmd: self,
        };
        let mut nav = b.nav.borrow_mut();
        match op {
            NavOp::Repush => nav.repush_now(bw, &mut seams),
            NavOp::Reload => nav.reload_now(bw, &mut seams),
            NavOp::Redirection0 => nav.redirection0_now(bw, &mut seams),
            NavOp::CloseBw => {}
        }
    }

    /// `a_UIcmd_copy` — verbatim `layout->copyCurrentSelection(1)`.
    pub fn copy(&self, reg: &BwRegistry, bw: u32) {
        if let Some(b) = reg.get(bw) {
            if let Some(l) = b.layout.borrow().as_ref() {
                l.borrow_mut().copy_current_selection(1);
            }
        }
    }

    /// `a_UIcmd_zoom_in` (verbatim — `+0.10`, borne 10.0, msg, repush).
    pub fn zoom_in(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        let mut z = b.zoom.get() + 0.10;
        if z > 10.0 {
            z = 10.0;
        }
        b.zoom.set(z);
        self.set_msg(
            reg,
            shell,
            bw,
            &format!("Zoom set to {:.0}%", f64::from(z) * 100.0),
        );
        self.repush(reg, shell, prefs, bw);
    }

    /// `a_UIcmd_zoom_out` (verbatim — `-0.10`, borne 0.10).
    pub fn zoom_out(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        let mut z = b.zoom.get() - 0.10;
        if z < 0.10 {
            z = 0.10;
        }
        b.zoom.set(z);
        self.set_msg(
            reg,
            shell,
            bw,
            &format!("Zoom set to {:.0}%", f64::from(z) * 100.0),
        );
        self.repush(reg, shell, prefs, bw);
    }

    /// `a_UIcmd_zoom_reset` (verbatim — `1.0`).
    pub fn zoom_reset(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        b.zoom.set(1.0);
        self.set_msg(reg, shell, bw, "Zoom set to 100%");
        self.repush(reg, shell, prefs, bw);
    }

    /// `UIcmd_make_save_filename` (verbatim) — dernier composant du
    /// path + query tronqués à 64, `save_dir` préfixé, `%20`/espaces
    /// → `_`.
    fn make_save_filename(&self, url: &DilloUrl) -> Vec<u8> {
        const MAX_LEN: usize = 64;
        let dir = &self.save_dir;

        /* get the last component of the path */
        let path = url.path.as_deref().unwrap_or(b"");
        let comp = match path.iter().rposition(|&c| c == b'/') {
            Some(i) => &path[i + 1..],
            None => path,
        };
        /* truncate the path if necessary */
        let path = if comp.len() > MAX_LEN {
            &comp[..MAX_LEN]
        } else {
            comp
        };

        /* is there a query? */
        let query = url.query.as_deref().unwrap_or(b"");
        let query = if query.len() > MAX_LEN {
            &query[..MAX_LEN]
        } else {
            query
        };
        let mut name = dir.clone();
        name.extend_from_slice(path);
        if !query.is_empty() {
            name.push(b'?');
            name.extend_from_slice(query);
        }

        /* Replace %20 and ' ' with '_' */
        let mut n2 = 0usize;
        let mut n1 = 0usize;
        while n1 < name.len() {
            let c = name[n1];
            let out = if c == b' ' {
                b'_'
            } else if c == b'%'
                && name.get(n1 + 1) == Some(&b'2')
                && name.get(n1 + 2) == Some(&b'0')
            {
                n1 += 2;
                b'_'
            } else {
                c
            };
            name[n2] = out;
            n1 += 1;
            n2 += 1;
        }
        name.truncate(n2);
        name
    }

    /// `UIcmd_save_file_check` (verbatim) — `stat` + dialogue
    /// Abort/Continue/Rename → 0/1 abort, 2 continue, autre rename.
    #[allow(clippy::unused_self)]
    fn save_file_check(&self, shell: &mut dyn UiShell, name: &str) -> i32 {
        if let Ok(md) = std::fs::metadata(name) {
            let msg = format!(
                "The file: {} ({} Bytes) already exists. What do we do?",
                name,
                md.len()
            );
            shell.dialog_choice(
                "OnionBit-Browser Save: File exists!",
                &msg,
                &["Abort", "Continue", "Rename"],
            )
        } else {
            2 /* assume the file does not exist, so Continue */
        }
    }

    /// `UIcmd_save` (verbatim) — dialogue en boucle jusqu'à
    /// Abort/Continue (Rename reprompte).
    fn save(
        &self,
        _reg: &BwRegistry,
        shell: &mut dyn UiShell,
        bw: u32,
        url: &DilloUrl,
        filename: Option<Vec<u8>>,
        title: &str,
    ) {
        let mut suggested = filename.unwrap_or_else(|| self.make_save_filename(url));

        loop {
            let suggested_str = String::from_utf8_lossy(&suggested);
            let name = shell.dialog_save_file(title, None, &suggested_str);
            let Some(name) = name else { return }; /* no name, so Abort */
            match self.save_file_check(shell, &name) {
                0 | 1 => return, /* Abort */
                2 => {
                    /* Continue */
                    tracing::info!("UIcmd_save: {name}");
                    crate::nav::nav_save_url(bw, url, &name);
                    return;
                }
                _ => {} /* Rename — prompt again */
            }
            suggested = name.into_bytes();
        }
    }

    /// `a_UIcmd_save` (verbatim) — sauve l'URL du haut de pile.
    pub fn save_page(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        let url = history::history_get_url(b.nav.borrow().get_top_uidx());
        if let Some(url) = url {
            self.save(reg, shell, bw, &url, None, "Save Page as File");
        }
    }

    /// `a_UIcmd_save_link` (verbatim).
    pub fn save_link(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        bw: u32,
        url: &DilloUrl,
        filename: Option<Vec<u8>>,
    ) {
        self.save(
            reg,
            shell,
            bw,
            url,
            filename,
            "OnionBit-Browser: Save Link as File",
        );
    }

    /// `a_UIcmd_select_file` (verbatim).
    pub fn select_file(&self, shell: &mut dyn UiShell) -> Option<String> {
        shell.dialog_select_file("OnionBit-Browser: Select a File", None, None)
    }

    /// `a_UIcmd_stop` (verbatim) — annule l'expect + stop clients +
    /// boutons.
    pub fn stop(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        tracing::debug!("a_UIcmd_stop()");
        let Some(b) = reg.get(bw) else { return };
        {
            let mut seams = NavSeams {
                reg,
                shell,
                prefs,
                uicmd: self,
            };
            b.nav.borrow_mut().cancel_expect(bw, &mut seams);
        }
        b.stop_clients(BW_ROOT | BW_IMG | BW_FORCE);
        Self::set_buttons_sens(reg, shell, bw);
    }

    /// `a_UIcmd_tools` (verbatim — `ui` ≙ `bw->ui`, lu par le shell
    /// pour l'état des toggles du menu).
    pub fn tools(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32, x: i32, y: i32) {
        let ui = reg.get(bw).map_or(0, |b| b.ui.get());
        shell.tools_popup(bw, ui, x, y);
    }

    /// `a_UIcmd_open_file` (verbatim) — dialogue → `file:` URL.
    pub fn open_file(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        if let Some(name) = shell.dialog_open_file("OnionBit-Browser: Open File", None, "") {
            let url = url_new(name.as_bytes(), Some(b"file:"));
            self.open_url(reg, shell, prefs, bw, url.as_ref());
        }
    }

    /// `UIcmd_make_search_str` (verbatim) — `%s` → keywords
    /// url-encodés, `%%` → `%`, sinon warning ; index quirk
    /// `search_url_idx` (`bool` amont ≙ 0/1) préservé.
    fn make_search_str(prefs: &Prefs, s: &[u8]) -> Vec<u8> {
        let keys = url::encode_hex_str(s);
        let idx = usize::from(prefs.search_url_idx.get());
        let Some(src) = prefs.search_urls.get(idx) else {
            return Vec::new();
        };

        let mut ds = Vec::with_capacity(128);
        /* parse search_url into label and url */
        if let Some((_label, u)) = misc::parse_search_url(src) {
            let mut i = 0usize;
            while i < u.len() {
                let c = u[i];
                if c == b'%' {
                    i += 1;
                    match u.get(i).copied() {
                        Some(b's') => ds.extend_from_slice(&keys),
                        Some(b'%') => ds.push(b'%'),
                        None => {
                            tracing::warn!("search_url ends with '%'");
                            // verbatim `c--` : le `c++` de boucle sort.
                            break;
                        }
                        Some(other) => {
                            tracing::warn!("illegal specifier '%{}' in search_url", other as char);
                        }
                    }
                } else {
                    ds.push(c);
                }
                i += 1;
            }
        }
        ds
    }

    /// `a_UIcmd_search_dialog` (verbatim).
    pub fn search_dialog(&self, reg: &BwRegistry, shell: &mut dyn UiShell, prefs: &Prefs, bw: u32) {
        if let Some(query) = shell.dialog_input("OnionBit-Browser: Search", "Search the Web:") {
            let url_str = Self::make_search_str(prefs, query.as_bytes());
            self.open_urlstr(reg, shell, prefs, bw, &url_str);
        }
    }

    /// `a_UIcmd_get_passwd` (verbatim).
    pub fn get_passwd(&self, shell: &mut dyn UiShell, user: &str) -> Option<String> {
        let msg = format!("Password for user \"{user}\"");
        shell.dialog_passwd("OnionBit-Browser: Password", &msg)
    }

    /// `a_UIcmd_book` (verbatim — `dpi:/bm/`).
    pub fn book(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        nt: bool,
    ) {
        let url = url_new(b"dpi:/bm/", None);
        if nt {
            self.open_url_nt(reg, shell, prefs, bw, url.as_ref(), 1);
        } else {
            self.open_url(reg, shell, prefs, bw, url.as_ref());
        }
    }

    /// `a_UIcmd_add_bookmark` (verbatim).
    pub fn add_bookmark(&self, shell: &mut dyn UiShell, bw: u32, url: &DilloUrl) {
        shell.add_bookmark(bw, url);
    }

    /// `a_UIcmd_page_popup` (verbatim).
    pub fn page_popup(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        bw: u32,
        has_bugs: bool,
        css_urls: &[DilloUrl],
    ) {
        let url = reg
            .get(bw)
            .and_then(|b| history::history_get_url(b.nav.borrow().get_top_uidx()));
        shell.page_popup(bw, url.as_ref(), has_bugs, css_urls);
    }

    /// `a_UIcmd_link_popup` (verbatim).
    pub fn link_popup(
        &self,
        shell: &mut dyn UiShell,
        bw: u32,
        url: &DilloUrl,
        page_url: Option<&DilloUrl>,
    ) {
        shell.link_popup(bw, url, page_url);
    }

    /// `a_UIcmd_image_popup` (verbatim).
    pub fn image_popup(
        &self,
        shell: &mut dyn UiShell,
        bw: u32,
        url: &DilloUrl,
        loaded_img: bool,
        page_url: Option<&DilloUrl>,
        link_url: Option<&DilloUrl>,
    ) {
        shell.image_popup(bw, url, loaded_img, page_url, link_url);
    }

    /// `a_UIcmd_form_popup` (verbatim — `form` opaque ≙
    /// `DilloHtmlForm*`).
    pub fn form_popup(
        &self,
        shell: &mut dyn UiShell,
        bw: u32,
        url: &DilloUrl,
        form: usize,
        showing_hiddens: bool,
    ) {
        shell.form_popup(bw, url, form, showing_hiddens);
    }

    /// `a_UIcmd_file_popup` (verbatim — l'appelant passe le coin
    /// bas-gauche du widget `wid` ≙ `wid->x(), wid->y()+wid->h()`).
    pub fn file_popup(&self, shell: &mut dyn UiShell, bw: u32, x: i32, y: i32) {
        shell.file_popup(bw, x, y);
    }

    /// `a_UIcmd_copy_urlstr` (verbatim) — `destination` : 0 primary,
    /// 1 clipboard, 2 both.
    pub fn copy_urlstr(&self, reg: &BwRegistry, bw: u32, urlstr: &[u8], destination: i32) {
        if let Some(b) = reg.get(bw) {
            if let Some(l) = b.layout.borrow().as_ref() {
                l.borrow_mut().copy_selection(urlstr, destination);
            }
        }
    }

    /// `a_UIcmd_view_page_source` (verbatim) — `dpi:/vsource/` +
    /// suffixe `_NNNN` pour les URL POST (`post_id` static).
    pub fn view_page_source(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        url: &DilloUrl,
    ) {
        let content_type = crate::nav::nav_get_content_type(url);
        let major = misc::parse_content_type(content_type.as_deref()).major;

        let is_image = major
            .as_deref()
            .is_some_and(|m| dlib::str_ascii_casecmp(m, b"image") == 0);
        if !is_image {
            if let Some(_buf) = crate::nav::nav_get_buf(url) {
                crate::nav::nav_set_vsource_url(url);
                let mut dstr_url = b"dpi:/vsource/:".to_vec();
                dstr_url.extend_from_slice(url.clone().url_str());
                if url.flags & url::URL_POST != 0 {
                    /* append a custom string to differentiate POST URLs */
                    let id = self.post_id.get();
                    self.post_id.set(if id < 9999 { id + 1 } else { 0 });
                    dstr_url.extend_from_slice(format!("_{:04}", self.post_id.get()).as_bytes());
                }
                let vs_url = url_new(&dstr_url, None);
                self.open_url_nt(reg, shell, prefs, bw, vs_url.as_ref(), 1);
                crate::nav::nav_unref_buf(url);
            }
        }
    }

    /// `a_UIcmd_view_page_bugs` (verbatim).
    pub fn view_page_bugs(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
        let Some(b) = reg.get(bw) else { return };
        if b.num_page_bugs.get() > 0 {
            let bugs = b.page_bugs.borrow().clone();
            shell.dialog_text_window("OnionBit-Browser: Detected HTML errors", &bugs);
        } else {
            shell.dialog_msg(
                "OnionBit-Browser: Good HTML!",
                "No HTML errors found while parsing!",
            );
        }
    }

    /// `a_UIcmd_bugmeter_popup` (verbatim).
    pub fn bugmeter_popup(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
        let url = reg
            .get(bw)
            .and_then(|b| history::history_get_url(b.nav.borrow().get_top_uidx()));
        shell.bugmeter_popup(bw, url.as_ref());
    }

    /// `a_UIcmd_get_history` (verbatim) — liste d'index historique
    /// terminée par `-1` (`direction` ±1).
    #[must_use]
    pub fn get_history(&self, reg: &BwRegistry, bw: u32, direction: i32) -> Vec<i32> {
        let Some(b) = reg.get(bw) else {
            return vec![-1];
        };
        let nav = b.nav.borrow();

        // Count the number of URLs
        let mut n = 0;
        let mut i = nav.stack_ptr() + direction;
        while i >= 0 && i < nav.stack_size() {
            n += 1;
            i += direction;
        }

        // Fill the list (-1 sentinel ≙ hlist[j] = -1)
        let mut hlist = Vec::with_capacity(n as usize + 1);
        let mut i = nav.stack_ptr() + direction;
        while i >= 0 && i < nav.stack_size() {
            hlist.push(nav.get_uidx(i));
            i += direction;
        }
        hlist.push(-1);
        hlist
    }

    /// `a_UIcmd_nav_jump` — verbatim `a_Nav_jump`.
    pub fn nav_jump(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        offset: i32,
        new_bw: bool,
    ) {
        let Some(b) = reg.get(bw) else { return };
        let mut seams = NavSeams {
            reg,
            shell,
            prefs,
            uicmd: self,
        };
        b.nav.borrow_mut().jump(bw, offset, new_bw, &mut seams);
    }

    // -- UI binding functions ----------------------------------------

    /// `a_UIcmd_get_wh` (verbatim) — taille de la `UI` de `bw`.
    #[must_use]
    pub fn get_wh(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) -> (i32, i32) {
        reg.get(bw).map_or((0, 0), |b| shell.ui_wh(b.ui.get()))
    }

    /// `a_UIcmd_get_location_text` (verbatim).
    #[must_use]
    pub fn get_location_text(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) -> String {
        reg.get(bw)
            .map(|b| shell.get_location(b.ui.get()))
            .unwrap_or_default()
    }

    /// `a_UIcmd_set_location_text` (verbatim).
    pub fn set_location_text(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        bw: u32,
        text: &[u8],
    ) {
        uicmd_set_location_text(reg, shell, bw, text);
    }

    /// `a_UIcmd_set_page_prog` (verbatim — cmd 0/1/2).
    pub fn set_page_prog(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        bw: u32,
        nbytes: usize,
        cmd: i32,
    ) {
        if let Some(b) = reg.get(bw) {
            shell.set_page_prog(b.ui.get(), nbytes, cmd);
        }
    }

    /// `a_UIcmd_set_img_prog` (verbatim).
    pub fn set_img_prog(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        bw: u32,
        n_img: i32,
        t_img: i32,
        cmd: i32,
    ) {
        if let Some(b) = reg.get(bw) {
            shell.set_img_prog(b.ui.get(), n_img, t_img, cmd);
        }
    }

    /// `a_UIcmd_set_bug_prog` (verbatim).
    pub fn set_bug_prog(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32, n_bug: i32) {
        if let Some(b) = reg.get(bw) {
            shell.set_bug_prog(b.ui.get(), n_bug);
        }
    }

    /// `a_UIcmd_set_page_title` (verbatim) — titre `OnionBit-Browser: %s`
    /// tronqué à 128 avec `...` sur borne UTF-8, label onglet +
    /// titre fenêtre si onglet actif.
    pub fn set_page_title(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        bw: u32,
        label: Option<&str>,
    ) {
        const SIZE: usize = 128;
        let label_str = label.unwrap_or("");
        let mut title = format!("OnionBit-Browser: {label_str}").into_bytes();
        if title.len() >= SIZE {
            let i = (SIZE - 4).min(1 + end_of_char(&title, SIZE - 8));
            title.truncate(i);
            title.extend_from_slice(b"...");
        }
        let Some(b) = reg.get(bw) else { return };
        let ui = b.ui.get();
        let title_str = String::from_utf8_lossy(&title);
        shell.ui_copy_label(ui, &title_str);
        shell.set_tab_label(ui, label_str);
        if shell.is_active_tab(ui) {
            // This is the focused bw, set window title
            shell.set_window_title(ui, &title_str);
        }
    }

    /// `a_UIcmd_get_page_title` (verbatim) — titre d'historique de
    /// l'URL de la location bar (`None` si inconnu).
    #[must_use]
    pub fn get_page_title(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        bw: u32,
    ) -> Option<String> {
        let text = self.get_location_text(reg, shell, bw);
        let mut url = url_new(text.as_bytes(), None);
        url.as_mut()
            .and_then(|u| history::history_get_title_by_url(u, false))
    }

    /// `a_UIcmd_set_msg` (verbatim — message déjà formaté par
    /// l'appelant ≙ `dStr_vsprintf`).
    pub fn set_msg(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32, msg: &str) {
        uicmd_set_msg(reg, shell, bw, msg);
    }

    /// `a_UIcmd_set_buttons_sens` (verbatim) — Stop actif si clients,
    /// Back si `stack_ptr > 0`, Forw si `stack_ptr < size-1` et pas
    /// d'expect.
    pub fn set_buttons_sens(reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
        uicmd_set_buttons_sens(reg, shell, bw);
    }

    /// `a_UIcmd_has_finished` (verbatim).
    #[must_use]
    pub fn has_finished(reg: &BwRegistry, bw: u32) -> bool {
        let Some(b) = reg.get(bw) else { return true };
        if !b.image_clients.borrow().is_empty() || !b.root_clients.borrow().is_empty() {
            return false;
        }
        !b.expecting()
    }

    /// `a_UIcmd_finish_loading` (verbatim) — si un client dilloc
    /// attend et tout est fini : derniers passes de layout
    /// (`drain_idles` ≙ `generalIdle`), flush, notification.
    pub fn finish_loading(reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
        uicmd_finish_loading(reg, shell, bw);
    }

    /// `a_UIcmd_panels_toggle` (verbatim).
    pub fn panels_toggle(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
        if let Some(b) = reg.get(bw) {
            shell.panels_toggle(b.ui.get());
        }
    }

    /// `a_UIcmd_findtext_search` (verbatim) — `RESTART`/`NOT_FOUND` →
    /// message de statut, sinon efface.
    pub fn findtext_search(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        bw: u32,
        key: &[u8],
        case_sens: bool,
        backward: bool,
    ) {
        let Some(b) = reg.get(bw) else { return };
        let r = b
            .layout
            .borrow()
            .as_ref()
            .map(|l| l.borrow_mut().search(key, case_sens, backward));
        match r {
            Some(FindtextResult::Restart) => self.set_msg(
                reg,
                shell,
                bw,
                if backward {
                    "Top reached; restarting from the bottom."
                } else {
                    "Bottom reached; restarting from the top."
                },
            ),
            Some(FindtextResult::NotFound) => {
                self.set_msg(
                    reg,
                    shell,
                    bw,
                    &format!("\"{}\" not found.", String::from_utf8_lossy(key)),
                );
            }
            _ => self.set_msg(reg, shell, bw, ""),
        }
    }

    /// `a_UIcmd_findtext_reset` (verbatim).
    pub fn findtext_reset(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
        if let Some(b) = reg.get(bw) {
            if let Some(l) = b.layout.borrow().as_ref() {
                l.borrow_mut().reset_search();
            }
        }
        self.set_msg(reg, shell, bw, "");
    }

    /// `a_UIcmd_findbar_toggle` (verbatim).
    pub fn findbar_toggle(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32, on: bool) {
        if let Some(b) = reg.get(bw) {
            shell.findbar_toggle(b.ui.get(), on);
        }
    }

    /// `a_UIcmd_focus_main_area` (verbatim).
    pub fn focus_main_area(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
        if let Some(b) = reg.get(bw) {
            shell.focus_main(b.ui.get());
        }
    }

    /// `a_UIcmd_focus_location` (verbatim).
    pub fn focus_location(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
        if let Some(b) = reg.get(bw) {
            shell.focus_location(b.ui.get());
        }
    }

    /// `a_UIcmd_focus_tab` (verbatim) — focus l'onglet `index` de la
    /// fenêtre de `bw`.
    pub fn focus_tab(&self, reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32, index: i32) {
        if let Some(b) = reg.get(bw) {
            if let Some(tabs) = shell.tabs_of(b.ui.get()) {
                shell.switch_tab(tabs, index);
            }
        }
    }

    /// `a_UIcmd_scroll` (verbatim) — map `KeysCommand` →
    /// `ScrollCommand` puis `layout->scroll`.
    pub fn scroll(&self, reg: &BwRegistry, bw: u32, cmd: KeysCommand) {
        const MAP: &[(KeysCommand, ScrollCommand)] = &[
            (KeysCommand::ScreenUp, ScrollCommand::ScreenUp),
            (KeysCommand::ScreenDown, ScrollCommand::ScreenDown),
            (KeysCommand::ScreenLeft, ScrollCommand::ScreenLeft),
            (KeysCommand::ScreenRight, ScrollCommand::ScreenRight),
            (KeysCommand::LineUp, ScrollCommand::LineUp),
            (KeysCommand::LineDown, ScrollCommand::LineDown),
            (KeysCommand::Left, ScrollCommand::Left),
            (KeysCommand::Right, ScrollCommand::Right),
            (KeysCommand::Top, ScrollCommand::Top),
            (KeysCommand::Bottom, ScrollCommand::Bottom),
        ];
        let Some(b) = reg.get(bw) else { return };
        for (k, d) in MAP {
            if cmd == *k {
                if let Some(l) = b.layout.borrow().as_ref() {
                    l.borrow_mut().scroll(*d);
                }
                break;
            }
        }
    }

    /// `a_UIcmd_get_bw_by_widget` (verbatim) — itère le registre et
    /// teste `ui->contains(wid)` (≙ handle : `wid` = un `ui` compte).
    #[must_use]
    pub fn get_bw_by_widget(reg: &BwRegistry, shell: &mut dyn UiShell, wid: usize) -> Option<u32> {
        for i in 0..reg.len() {
            let id = u32::try_from(i).unwrap_or(u32::MAX);
            if let Some(b) = reg.get(id) {
                if shell.ui_contains(b.ui.get(), wid) {
                    return Some(id);
                }
            }
        }
        None
    }

    /// `win_cb` (verbatim) — callback fermeture fenêtre : dialogue si
    /// `show_quit_dialog` + plusieurs onglets, puis ferme chaque
    /// onglet actif (le garde `Escape` est géré côté dispatch,
    /// verbatim `Fl::event_key() == FL_Escape`).
    pub fn window_close(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        tabs: usize,
    ) {
        let mut choice = 1;
        let mut ntabs = shell.tab_count(tabs);

        if prefs.show_quit_dialog && ntabs > 1 {
            choice = shell.dialog_choice(
                "OnionBit-Browser: Close window?",
                "Window contains more than one tab.",
                &["Close", "Cancel"],
            );
        }
        if choice == 1 {
            while ntabs > 0 {
                ntabs -= 1;
                let act = shell.tabs_active_ui(tabs);
                let Some(bw) = Self::get_bw_by_widget(reg, shell, act) else {
                    break;
                };
                self.close_bw(reg, shell, bw);
            }
        }
    }

    /// `tab_btn_cb` (verbatim) — clic gauche → switch ;
    /// droit/gauche du milieu selon `prefs.right_click_closes_tab`
    /// → close.
    pub fn tab_btn(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        ui: usize,
        button: i32,
    ) {
        if button == BUTTON_LEFT {
            shell.switch_to_tab(ui);
        } else if (button == BUTTON_RIGHT && prefs.right_click_closes_tab)
            || (button == BUTTON_MIDDLE && !prefs.right_click_closes_tab)
        {
            if let Some(bw) = Self::get_bw_by_widget(reg, shell, ui) {
                self.close_bw(reg, shell, bw);
            }
        }
    }

    /// `close_tab_btn_cb` (verbatim) — bouton `X` : ferme l'onglet
    /// actif (bouton gauche seul).
    pub fn close_tab_btn(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        tabs: usize,
        button: i32,
    ) {
        if button == BUTTON_LEFT {
            let act = shell.tabs_active_ui(tabs);
            if let Some(bw) = Self::get_bw_by_widget(reg, shell, act) {
                self.close_bw(reg, shell, bw);
            }
        }
    }

    /// `CustTabs::handle` `FL_SHORTCUT` (verbatim) — dispatch
    /// `KeysCommand` → actions onglet/fenêtre. `key_cmd` est calculé
    /// par l'appelant ([`Keys::key_cmd`]). Retourne `ret` amont.
    pub fn tab_shortcut(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        cmd: KeysCommand,
    ) -> bool {
        let Some(b) = reg.get(bw) else { return false };
        let ui = b.ui.get();
        match cmd {
            KeysCommand::NewTab => {
                self.open_url_nt(reg, shell, prefs, bw, None, 1);
                true
            }
            KeysCommand::CloseTab => {
                self.close_bw(reg, shell, bw);
                true
            }
            KeysCommand::LeftTab => {
                if let Some(t) = shell.tabs_of(ui) {
                    shell.prev_tab(t);
                }
                true
            }
            KeysCommand::RightTab => {
                if let Some(t) = shell.tabs_of(ui) {
                    shell.next_tab(t);
                }
                true
            }
            KeysCommand::NewWindow => {
                self.open_url_nw(reg, shell, prefs, bw, None);
                true
            }
            KeysCommand::CloseAll => {
                shell.defer_close_all();
                true
            }
            _ => false,
        }
    }

    /// `CustTabs::handle` `FL_MOUSEWHEEL` (verbatim) — la molette
    /// change d'onglet si `prefs.scroll_switches_tabs`.
    pub fn tab_wheel(
        &self,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        ui: usize,
        dx: i32,
        dy: i32,
    ) -> bool {
        if !prefs.scroll_switches_tabs {
            return false;
        }
        /* Move to the next or previous tab using the mouse wheel */
        let mut dir = if dy != 0 { dy } else { dx };
        if prefs.scroll_switches_tabs_reverse {
            dir = -dir;
        }
        let Some(tabs) = shell.tabs_of(ui) else {
            return false;
        };
        if dir > 0 {
            shell.next_tab(tabs);
        } else if dir < 0 {
            shell.prev_tab(tabs);
        }
        true
    }

    /// `a_UIcmd_by_name` (verbatim) — dispatch par nom de commande
    /// (`Keys::getCmdCode`) ; `-1` si inconnue.
    pub fn by_name(
        &self,
        reg: &BwRegistry,
        shell: &mut dyn UiShell,
        prefs: &Prefs,
        bw: u32,
        cmd_name: &str,
    ) -> i32 {
        let cmd = Keys::cmd_code(cmd_name);
        if cmd == KeysCommand::Invalid {
            return -1;
        }
        let Some(b) = reg.get(bw) else { return -1 };
        let ui = b.ui.get();
        match cmd {
            KeysCommand::NewTab => self.open_url_nt(reg, shell, prefs, bw, None, 1),
            KeysCommand::CloseTab => self.close_bw(reg, shell, bw),
            KeysCommand::LeftTab => {
                if let Some(t) = shell.tabs_of(ui) {
                    shell.prev_tab(t);
                }
            }
            KeysCommand::RightTab => {
                if let Some(t) = shell.tabs_of(ui) {
                    shell.next_tab(t);
                }
            }
            KeysCommand::NewWindow => self.open_url_nw(reg, shell, prefs, bw, None),
            _ => return -1, /* Unknown command or not implemented */
        }
        0
    }
}

// -- fonctions internes (leaves UiShell, partagées par NavSeams) -------

/// `a_UIcmd_set_msg` — leaf `ui->set_status`.
fn uicmd_set_msg(reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32, msg: &str) {
    if let Some(b) = reg.get(bw) {
        shell.set_status(b.ui.get(), msg);
    }
}

/// `a_UIcmd_set_location_text` — leaf `ui->set_location`.
fn uicmd_set_location_text(reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32, text: &[u8]) {
    if let Some(b) = reg.get(bw) {
        shell.set_location(b.ui.get(), &String::from_utf8_lossy(text));
    }
}

/// `a_UIcmd_set_buttons_sens` (verbatim corps).
fn uicmd_set_buttons_sens(reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
    let Some(b) = reg.get(bw) else { return };
    let (ptr, size, top) = {
        let nav = b.nav.borrow();
        (nav.stack_ptr(), nav.stack_size(), nav.top_url())
    };
    uicmd_set_buttons_sens_impl(reg, shell, bw, ptr, size, top);
}

/// Corps de `uicmd_set_buttons_sens` — `ptr`/`size` passés en snapshot
/// pour être appelable depuis `Nav` (qui détient déjà l'emprunt).
fn uicmd_set_buttons_sens_impl(
    reg: &BwRegistry,
    shell: &mut dyn UiShell,
    bw: u32,
    ptr: i32,
    size: i32,
    top: Option<DilloUrl>,
) {
    let Some(b) = reg.get(bw) else { return };
    // Snapshots pour les seams capi (verbatim : lecture directe de
    // `bw->nav` — impossible ici pendant `nav.borrow_mut()`).
    b.nav_size_snap.set(size);
    *b.nav_top_snap.borrow_mut() = top;
    let ui = b.ui.get();
    // Stop
    let sens = !b.image_clients.borrow().is_empty() || !b.root_clients.borrow().is_empty();
    shell.button_set_sens(ui, UiButton::Stop, sens);
    // Back
    let expecting = b.expecting();
    shell.button_set_sens(ui, UiButton::Back, ptr > 0);
    // Forward
    shell.button_set_sens(ui, UiButton::Forw, ptr < size - 1 && !expecting);
}

/// `a_UIcmd_get_scroll_xy` — leaf `layout->getScrollPos*`.
fn uicmd_get_scroll_xy(reg: &BwRegistry, bw: u32) -> (i32, i32) {
    reg.get(bw)
        .and_then(|b| {
            b.layout.borrow().as_ref().map(|l| {
                let lg = l.borrow();
                (lg.get_scroll_pos_x(), lg.get_scroll_pos_y())
            })
        })
        .unwrap_or((0, 0))
}

/// `a_UIcmd_set_scroll_xy` — verbatim `scrollTo(HPOS_LEFT, VPOS_TOP)`.
fn uicmd_set_scroll_xy(reg: &BwRegistry, bw: u32, x: i32, y: i32) {
    if let Some(b) = reg.get(bw) {
        if let Some(l) = b.layout.borrow().as_ref() {
            l.borrow_mut()
                .scroll_to(HPosition::Left, VPosition::Top, x, y, 0, 0);
        }
    }
}

/// `a_UIcmd_set_scroll_by_fragment` — verbatim `layout->setAnchor`.
fn uicmd_set_scroll_by_fragment(reg: &BwRegistry, bw: u32, fragment: &[u8]) {
    if let Some(b) = reg.get(bw) {
        if let Some(l) = b.layout.borrow().as_ref() {
            l.borrow_mut().set_anchor(Some(fragment));
        }
    }
}

/// `a_UIcmd_finish_loading` (verbatim corps) — drain idles + flush +
/// notify si un client dilloc attend.
fn uicmd_finish_loading(reg: &BwRegistry, shell: &mut dyn UiShell, bw: u32) {
    if shell.control_is_waiting() && UiCmd::has_finished(reg, bw) {
        /* perform any last layout passes (generalIdle) then flush */
        if let Some(b) = reg.get(bw) {
            if let Some(l) = b.layout.borrow().as_ref() {
                l.borrow_mut().drain_idles();
            }
        }
        shell.flush();
        /* Now we are ready to notify any client */
        shell.control_notify_finish(bw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `UiShell` factice — enregistre les appels, handles `ui` =
    /// `bw_id + 1`, `tabs` = 100 + fenêtre.
    struct FakeShell {
        status: Vec<(usize, String)>,
        location: Vec<(usize, String)>,
        sens: Vec<(usize, UiButton, bool)>,
        labels: Vec<String>,
        tab_labels: Vec<(usize, String)>,
        win_titles: Vec<(usize, String)>,
        focus: Vec<(usize, &'static str)>,
        dialogs: Vec<String>,
        deferred: Vec<(u32, NavOp)>,
        close_all: u32,
        progs: Vec<String>,
        next_ui: usize,
        tabs_map: Vec<(usize, usize)>, // ui -> tabs
    }

    impl FakeShell {
        fn new() -> Self {
            FakeShell {
                status: vec![],
                location: vec![],
                sens: vec![],
                labels: vec![],
                tab_labels: vec![],
                win_titles: vec![],
                focus: vec![],
                dialogs: vec![],
                deferred: vec![],
                close_all: 0,
                progs: vec![],
                next_ui: 1,
                tabs_map: vec![],
            }
        }
    }

    impl UiShell for FakeShell {
        fn window_new(&mut self, _w: i32, _h: i32) -> usize {
            100
        }
        fn window_show(&mut self, _t: usize) {}
        fn window_position(&mut self, _t: usize, _x: i32, _y: i32) {}
        fn tab_ui_new(
            &mut self,
            tabs: usize,
            _old: usize,
            _focus: bool,
        ) -> (usize, Rc<RefCell<dillo_dw::layout::Layout>>) {
            let ui = self.next_ui;
            self.next_ui += 1;
            self.tabs_map.push((ui, tabs));
            (
                ui,
                Rc::new(RefCell::new(dillo_dw::layout::Layout::new(
                    Rc::new(RefCell::new(NullPlatform)),
                    false,
                ))),
            )
        }
        fn tab_ui_remove(&mut self, ui: usize) {
            self.tabs_map.retain(|(u, _)| *u != ui);
        }
        fn tab_count(&self, tabs: usize) -> i32 {
            i32::try_from(self.tabs_map.iter().filter(|(_, t)| *t == tabs).count())
                .unwrap_or_default()
        }
        fn tabs_of(&self, ui: usize) -> Option<usize> {
            self.tabs_map
                .iter()
                .find(|(u, _)| *u == ui)
                .map(|(_, t)| *t)
        }
        fn switch_tab(&mut self, _t: usize, _i: i32) {}
        fn switch_to_tab(&mut self, _ui: usize) {}
        fn prev_tab(&mut self, _t: usize) {}
        fn next_tab(&mut self, _t: usize) {}
        fn set_tab_label(&mut self, ui: usize, label: &str) {
            self.tab_labels.push((ui, label.to_string()));
        }
        fn is_active_tab(&self, _ui: usize) -> bool {
            true
        }
        fn tabs_active_ui(&self, tabs: usize) -> usize {
            self.tabs_map
                .iter()
                .find(|(_, t)| *t == tabs)
                .map_or(0, |(u, _)| *u)
        }
        fn active_ui_handles(&self) -> Vec<usize> {
            vec![1]
        }
        fn ui_contains(&self, ui: usize, wid: usize) -> bool {
            ui == wid
        }
        fn ui_wh(&self, _ui: usize) -> (i32, i32) {
            (800, 600)
        }
        fn ui_label(&self, _ui: usize) -> String {
            "OnionBit-Browser:".to_string()
        }
        fn ui_copy_label(&mut self, _ui: usize, title: &str) {
            self.labels.push(title.to_string());
        }
        fn set_window_title(&mut self, ui: usize, t: &str) {
            self.win_titles.push((ui, t.to_string()));
        }
        fn focus_main(&mut self, ui: usize) {
            self.focus.push((ui, "main"));
        }
        fn focus_location(&mut self, ui: usize) {
            self.focus.push((ui, "location"));
        }
        fn get_location(&self, _ui: usize) -> String {
            String::new()
        }
        fn set_location(&mut self, ui: usize, text: &str) {
            self.location.push((ui, text.to_string()));
        }
        fn set_status(&mut self, ui: usize, msg: &str) {
            self.status.push((ui, msg.to_string()));
        }
        fn set_page_prog(&mut self, ui: usize, n: usize, cmd: i32) {
            self.progs.push(format!("page {ui} {n} {cmd}"));
        }
        fn set_img_prog(&mut self, ui: usize, n: i32, t: i32, cmd: i32) {
            self.progs.push(format!("img {ui} {n} {t} {cmd}"));
        }
        fn set_bug_prog(&mut self, ui: usize, n: i32) {
            self.progs.push(format!("bug {ui} {n}"));
        }
        fn button_set_sens(&mut self, ui: usize, btn: UiButton, sens: bool) {
            self.sens.push((ui, btn, sens));
        }
        fn panels_toggle(&mut self, _ui: usize) {}
        fn panel_state(&self, _ui: usize) -> (i32, bool) {
            (2, false)
        }
        fn panel_change(&mut self, _ui: usize, _s: i32, _i: bool) {}
        fn findbar_toggle(&mut self, _ui: usize, _on: bool) {}
        fn dialog_choice(&mut self, t: &str, _m: &str, _a: &[&str]) -> i32 {
            self.dialogs.push(t.to_string());
            1
        }
        fn dialog_input(&mut self, _t: &str, _m: &str) -> Option<String> {
            None
        }
        fn dialog_passwd(&mut self, _t: &str, _m: &str) -> Option<String> {
            None
        }
        fn dialog_save_file(&mut self, _t: &str, _m: Option<&str>, _s: &str) -> Option<String> {
            None
        }
        fn dialog_select_file(
            &mut self,
            _t: &str,
            _m: Option<&str>,
            _i: Option<&str>,
        ) -> Option<String> {
            None
        }
        fn dialog_open_file(&mut self, _t: &str, _m: Option<&str>, _p: &str) -> Option<String> {
            None
        }
        fn dialog_text_window(&mut self, t: &str, _x: &str) {
            self.dialogs.push(t.to_string());
        }
        fn dialog_msg(&mut self, t: &str, _m: &str) {
            self.dialogs.push(t.to_string());
        }
        fn history_popup(&mut self, _bw: u32, _l: Vec<i32>, _x: i32, _y: i32, _d: i32) {}
        fn page_popup(&mut self, _bw: u32, _u: Option<&DilloUrl>, _b: bool, _c: &[DilloUrl]) {}
        fn link_popup(&mut self, _bw: u32, _u: &DilloUrl, _p: Option<&DilloUrl>) {}
        fn image_popup(
            &mut self,
            _bw: u32,
            _u: &DilloUrl,
            _l: bool,
            _p: Option<&DilloUrl>,
            _l2: Option<&DilloUrl>,
        ) {
        }
        fn form_popup(&mut self, _bw: u32, _u: &DilloUrl, _f: usize, _s: bool) {}
        fn file_popup(&mut self, _bw: u32, _x: i32, _y: i32) {}
        fn bugmeter_popup(&mut self, _bw: u32, _u: Option<&DilloUrl>) {}
        fn tools_popup(&mut self, _bw: u32, _ui: usize, _x: i32, _y: i32) {}
        fn old_working_dir(&self) -> Vec<u8> {
            b"/tmp/".to_vec()
        }
        fn control_is_waiting(&self) -> bool {
            false
        }
        fn control_notify_finish(&mut self, _bw: u32) {}
        fn flush(&mut self) {}
        fn defer_nav(&mut self, bw: u32, op: NavOp) {
            self.deferred.push((bw, op));
        }
        fn defer_close_all(&mut self) {
            self.close_all += 1;
        }
        fn add_bookmark(&mut self, _bw: u32, _u: &DilloUrl) {}
    }

    /// `Platform` factice (≙ `MockPlatform` des tests layout).
    #[derive(Default)]
    struct NullPlatform;

    impl dillo_dw::platform::Platform for NullPlatform {
        fn set_layout(&mut self, _l: usize) {}
        fn attach_view(&mut self, _v: &mut dyn dillo_dw::view::View) {}
        fn detach_view(&mut self, _v: &mut dyn dillo_dw::view::View) {}
        fn text_width(&mut self, _f: &dillo_dw::style::Font, _t: &[u8]) -> i32 {
            0
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
        fn add_idle(&mut self, _l: usize, _cb: dillo_dw::platform::IdleCallback) -> i32 {
            0
        }
        fn remove_idle(&mut self, _id: i32) {}
        fn take_pending_idles(&mut self, _l: usize) -> Vec<dillo_dw::platform::IdleCallback> {
            Vec::new()
        }
        fn create_font(
            &mut self,
            attrs: &dillo_dw::style::FontAttrs,
            _te: bool,
        ) -> Rc<dillo_dw::style::Font> {
            Rc::new(dillo_dw::style::Font {
                attrs: attrs.clone(),
                ascent: 10,
                descent: 2,
                space_width: 4,
                x_height: 5,
                zero_width: 5,
            })
        }
        fn font_exists(&mut self, _n: &[u8]) -> bool {
            true
        }
        fn create_color(&mut self, c: i32) -> dillo_dw::style::Color {
            dillo_dw::style::Color::new(c)
        }
        fn create_tooltip(&mut self, t: &[u8]) -> Rc<dillo_dw::style::Tooltip> {
            dillo_dw::style::Tooltip::create(t, None)
        }
        fn cancel_tooltip(&mut self) {}
        fn create_imgbuf(
            &mut self,
            _t: dillo_dw::imgbuf::ImgbufType,
            _w: i32,
            _h: i32,
            _g: f64,
        ) -> Rc<dyn dillo_dw::imgbuf::Imgbuf> {
            unreachable!("unused in these tests")
        }
        fn copy_selection(&mut self, _t: &[u8], _d: i32) {}
        fn get_resource_factory(&mut self) -> Option<Rc<dyn dillo_dw::ui::ResourceFactory>> {
            None
        }
    }

    fn fixture() -> (BwRegistry, FakeShell, u32) {
        let reg = BwRegistry::new();
        let mut shell = FakeShell::new();
        let uicmd = UiCmd::new(&Prefs::default());
        let bw = uicmd.browser_window_new(&reg, &mut shell, &Prefs::default(), 0, 0, None);
        (reg, shell, bw)
    }

    /// `a_UIcmd_browser_window_new` verbatim — défauts prefs,
    /// `bw->ui`/`layout` posés, `zoom = zoom_factor`.
    #[test]
    fn browser_window_new_defaults() {
        let (reg, shell, bw) = fixture();
        let b = reg.get(bw).unwrap();
        assert_eq!(b.ui.get(), 1);
        assert!(b.layout.borrow().is_some());
        assert_eq!(b.nav.borrow().stack_ptr(), -1);
        drop(shell);
    }

    /// `open_url` avec `None` ≙ NULL : focus location + sens boutons.
    #[test]
    fn open_url_none_focuses_location() {
        let (reg, mut shell, bw) = fixture();
        let uicmd = UiCmd::new(&Prefs::default());
        uicmd.open_url(&reg, &mut shell, &Prefs::default(), bw, None);
        assert!(shell.focus.contains(&(1, "location")));
        assert!(shell.sens.iter().any(|(_, b, _)| *b == UiButton::Stop));
    }

    /// `set_buttons_sens` verbatim — Stop off sans clients, Back off
    /// à `stack_ptr = -1`, Forw off.
    #[test]
    fn buttons_sens_empty() {
        let (reg, mut shell, bw) = fixture();
        UiCmd::set_buttons_sens(&reg, &mut shell, bw);
        assert_eq!(
            shell.sens,
            vec![
                (1, UiButton::Stop, false),
                (1, UiButton::Back, false),
                (1, UiButton::Forw, false),
            ]
        );
    }

    /// `zoom_in`/`zoom_out`/`zoom_reset` verbatim (bornes + msg +
    /// repush différé).
    #[test]
    fn zoom_bounds() {
        let (reg, mut shell, bw) = fixture();
        let uicmd = UiCmd::new(&Prefs::default());
        uicmd.zoom_in(&reg, &mut shell, &Prefs::default(), bw);
        assert_eq!(reg.get(bw).unwrap().zoom.get(), 1.1);
        assert!(shell.status.iter().any(|(_, m)| m == "Zoom set to 110%"));
        for _ in 0..200 {
            uicmd.zoom_in(&reg, &mut shell, &Prefs::default(), bw);
        }
        assert_eq!(reg.get(bw).unwrap().zoom.get(), 10.0);
        for _ in 0..200 {
            uicmd.zoom_out(&reg, &mut shell, &Prefs::default(), bw);
        }
        assert!((reg.get(bw).unwrap().zoom.get() - 0.10).abs() < 1e-5);
        uicmd.zoom_reset(&reg, &mut shell, &Prefs::default(), bw);
        assert_eq!(reg.get(bw).unwrap().zoom.get(), 1.0);
        // repush différé ≙ a_Timeout_add.
        assert!(shell
            .deferred
            .iter()
            .any(|(_, op)| matches!(op, NavOp::Repush)));
    }

    /// `find_search_str` + `make_search_str` verbatim : `%s`
    /// remplacé, `%%` échappé, préfixe label reconnu, quirk
    /// `search_url_idx` (bool ≙ 0/1).
    #[test]
    fn search_str_verbatim() {
        let prefs = Prefs {
            search_urls: vec![
                b"ddg https://duckduckgo.com/?q=%s".to_vec(),
                b"wikipedia https://en.wikipedia.org/wiki/%s".to_vec(),
            ],
            ..Default::default()
        };
        // Préfixe exact "ddg " → idx 0, %s substitué url-encodé.
        let r = UiCmd::find_search_str(&prefs, b"ddg hello world");
        assert_eq!(
            r.as_deref(),
            Some(b"https://duckduckgo.com/?q=hello+world".as_slice())
        );
        // idx 1 → quirk bool : search_url_idx = true → index 1.
        let r = UiCmd::find_search_str(&prefs, b"wikipedia rust lang");
        assert_eq!(
            r.as_deref(),
            Some(b"https://en.wikipedia.org/wiki/rust+lang".as_slice())
        );
        // Pas d'espace / pas de préfixe → None.
        assert!(UiCmd::find_search_str(&prefs, b"noSpace").is_none());
        assert!(UiCmd::find_search_str(&prefs, b"foo bar").is_none());
    }

    /// `make_save_filename` verbatim — dernier composant, query,
    /// `%20`/` ` → `_`, `save_dir` préfixé.
    #[test]
    fn save_filename() {
        let prefs = Prefs {
            save_dir: Some(b"/tmp".to_vec()), // '/' ajouté verbatim
            ..Default::default()
        };
        let uicmd = UiCmd::new(&prefs);
        let url = url_new(b"http://h/dir/sub/page%20one.html?a=b c", None).unwrap();
        // verbatim : dernier comp. + `?query`, `save_dir` préfixé,
        // `%20`/espace → `_`.
        assert_eq!(
            uicmd.make_save_filename(&url),
            b"/tmp/page_one.html?a=b_c".to_vec()
        );
        // troncature à 64.
        let long = format!("http://h/{}", "x".repeat(200));
        let url2 = url_new(long.as_bytes(), None).unwrap();
        assert_eq!(
            uicmd.make_save_filename(&url2),
            b"/tmp/"
                .iter()
                .copied()
                .chain("x".repeat(64).bytes())
                .collect::<Vec<u8>>()
        );
    }

    /// `open_urlstr` verbatim — entrée vide → no-op ; préfixe
    /// moteur → URL de recherche poussée (location ≙
    /// `a_UIcmd_set_location_text` via `nav.push`).
    #[test]
    fn open_urlstr_search_prefix() {
        let (reg, mut shell, bw) = fixture();
        let uicmd = UiCmd::new(&Prefs::default());
        let prefs = Prefs {
            search_urls: vec![b"ddg https://duckduckgo.com/?q=%s".to_vec()],
            ..Default::default()
        };
        uicmd.open_urlstr(&reg, &mut shell, &prefs, bw, b"   ");
        uicmd.open_urlstr(&reg, &mut shell, &prefs, bw, b"ddg hello");
        // nav.push a posé la location = URL de recherche (verbatim).
        assert_eq!(
            shell.location.last().map(|(_, s)| s.as_str()),
            Some("https://duckduckgo.com/?q=hello")
        );
    }

    /// `set_page_title` verbatim — troncature `OnionBit-Browser: %s` + `...`,
    /// label onglet (label brut) + titre fenêtre.
    #[test]
    fn page_title_truncation() {
        let (reg, mut shell, bw) = fixture();
        let uicmd = UiCmd::new(&Prefs::default());
        uicmd.set_page_title(&reg, &mut shell, bw, Some("Titre"));
        assert_eq!(shell.labels.last().unwrap(), "OnionBit-Browser: Titre");
        assert_eq!(shell.tab_labels.last().unwrap().1, "Titre");
        assert_eq!(
            shell.win_titles.last().unwrap().1,
            "OnionBit-Browser: Titre"
        );

        let long = "x".repeat(300);
        uicmd.set_page_title(&reg, &mut shell, bw, Some(&long));
        let t = shell.labels.last().unwrap();
        assert!(t.len() <= 128);
        assert!(t.ends_with("..."));
    }

    /// `view_page_bugs` verbatim — 0 bug → dialog msg, >0 → text
    /// window.
    #[test]
    fn page_bugs_dialog() {
        let (reg, mut shell, bw) = fixture();
        let uicmd = UiCmd::new(&Prefs::default());
        uicmd.view_page_bugs(&reg, &mut shell, bw);
        assert_eq!(
            shell.dialogs.last().unwrap(),
            "OnionBit-Browser: Good HTML!"
        );
        reg.get(bw).unwrap().num_page_bugs.set(2);
        *reg.get(bw).unwrap().page_bugs.borrow_mut() = "bad tag".to_string();
        uicmd.view_page_bugs(&reg, &mut shell, bw);
        assert_eq!(
            shell.dialogs.last().unwrap(),
            "OnionBit-Browser: Detected HTML errors"
        );
    }

    /// `get_history` verbatim — pile vide → `[-1]`.
    #[test]
    fn history_empty() {
        let (reg, _shell, bw) = fixture();
        let uicmd = UiCmd::new(&Prefs::default());
        assert_eq!(uicmd.get_history(&reg, bw, -1), vec![-1]);
        assert_eq!(uicmd.get_history(&reg, bw, 1), vec![-1]);
    }

    /// `by_name` verbatim — `-1` inconnu, dispatch LeftTab/RightTab.
    #[test]
    fn by_name_dispatch() {
        let (reg, mut shell, bw) = fixture();
        let uicmd = UiCmd::new(&Prefs::default());
        assert_eq!(
            uicmd.by_name(&reg, &mut shell, &Prefs::default(), bw, "nosuch"),
            -1
        );
        assert_eq!(
            uicmd.by_name(&reg, &mut shell, &Prefs::default(), bw, "left-tab"),
            0
        );
        assert_eq!(
            uicmd.by_name(&reg, &mut shell, &Prefs::default(), bw, "stop"),
            -1
        );
    }

    /// `has_finished` verbatim — vide → true, client root → false,
    /// expect → false.
    #[test]
    fn has_finished_verbatim() {
        let (reg, _s, bw) = fixture();
        assert!(UiCmd::has_finished(&reg, bw));
        reg.get(bw).unwrap().root_clients.borrow_mut().push(3);
        assert!(!UiCmd::has_finished(&reg, bw));
        reg.get(bw).unwrap().root_clients.borrow_mut().clear();
        reg.get(bw)
            .unwrap()
            .expect(&url_new(b"http://x/", None).unwrap());
        assert!(!UiCmd::has_finished(&reg, bw));
    }
}
