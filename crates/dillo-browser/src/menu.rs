//! Port of `src/menu.cc` + `src/menu.hh` — menus popup du navigateur.
//!
//! Les `Fl_Menu_Item` deviennent des [`MenuItem`] (label, shortcut,
//! flags verbatim `FL_MENU_*`, callback → [`MenuCmd`]). Les globaux
//! amont (`popup_bw`, `popup_url`, `popup_form`, `history_*`,
//! `current_page_url`) vivent dans [`MenuCtx`]. `a_Timeout_add(0,
//! Menu_popup_cb)` ≙ la file [`MenuCtx::pending`] — l'app présente le
//! popup puis rappelle [`dispatch`] avec l'item choisi (verbatim :
//! les events du clic ayant ouvert le menu sont partis — même
//! sémantique différée).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::field_reassign_with_default)]
#![forbid(unsafe_code)]

use std::collections::VecDeque;

use std::cell::RefCell;

use dillo_common::dlib;
use dillo_net::history;
use dillo_net::url::DilloUrl;

use crate::actions::{actions_run, Action};
use crate::bw::BwRegistry;
use crate::keys::Keys;
use crate::prefs::Prefs;
use crate::uicmd::{UiCmd, UiShell};

/// `MenuCmd` — un variant par callback statique amont
/// (`Menu_*_cb`/`filemenu_cb`).
#[derive(Clone, Debug)]
pub enum MenuCmd {
    /// `Menu_nop_cb`.
    Nop,
    /// `filemenu_cb` — `"nw"|"nt"|"of"|"ou"|"cw"|"ed"`.
    File(&'static str),
    /// `Menu_open_url_cb`.
    OpenUrl,
    /// `Menu_open_url_nw_cb`.
    OpenUrlNw,
    /// `Menu_open_url_nt_cb`.
    OpenUrlNt,
    /// `Menu_add_bookmark_cb`.
    AddBookmark,
    /// `Menu_copy_urlstr_cb` — `a_UIcmd_copy_urlstr(.., 2)`.
    CopyUrl,
    /// `Menu_save_link_cb`.
    SaveLink,
    /// `Menu_save_page_cb`.
    SavePage,
    /// `Menu_view_page_source_cb`.
    ViewSource,
    /// `Menu_view_page_bugs_cb`.
    ViewBugs,
    /// `Menu_find_text_cb`.
    FindText,
    /// `Menu_load_images_cb` (user_data = page_url).
    LoadImages,
    /// `Menu_form_submit_cb`.
    FormSubmit,
    /// `Menu_form_reset_cb`.
    FormReset,
    /// `Menu_form_hiddens_cb` (user_data = `&hiddens_visible`).
    FormHiddens,
    /// `Menu_stylesheet_cb` (bouton 1 open / 2 nt|nw).
    Stylesheet,
    /// `Menu_bugmeter_validate_*_cb` — `true` = Nu (HTML5).
    BugmeterValidate(bool),
    /// `Menu_bugmeter_about_cb`.
    BugmeterAbout,
    /// `Menu_history_cb` (user_data = index 1-based).
    History(usize),
    /// `Menu_remote_css_cb` (toggle).
    RemoteCss,
    /// `Menu_embedded_css_cb` (toggle).
    EmbeddedCss,
    /// `Menu_force_https_cb` (toggle).
    ForceHttps,
    /// `Menu_imgload_toggle_cb` (toggle).
    ImgloadToggle,
    /// `Menu_bgimg_load_toggle_cb` (toggle).
    BgimgToggle,
    /// `Menu_panel_change_cb` — 0/1/2 = taille, 10 = small icons.
    PanelChange(i32),
    /// `Menu_open_page_action_cb` — action page (env `url`).
    PageAction(usize),
    /// `Menu_open_url_action_cb` — action lien (env `url`,`origin`).
    LinkAction(usize),
}

/// `user_data` amont — pointeur vers url/page_url/form/flag.
#[derive(Clone, Debug, Default)]
pub enum MenuData {
    /// `NULL`.
    #[default]
    None,
    /// `popup_url` (implicite).
    PopupUrl,
    /// `popup_page_url` (image menu).
    PageUrl,
    /// `popup_link_url`.
    LinkUrl,
    /// `&hiddens_visible`.
    HiddensFlag,
    /// `history_list[i-1]`.
    HistoryIdx(usize),
    /// `stylesheets[i]` — url de la i-ème feuille CSS.
    Stylesheet(usize),
    /// Action custom (`link_menu_item`).
    ActionItem(usize),
}

/// `Fl_Menu_Item` (verbatim) — `text`, `shortcut_`, `callback_`,
/// `user_data_`, `flags` (`FL_MENU_DIVIDER`/`TOGGLE`/`RADIO`/`VALUE`,
/// `FL_SUBMENU`/`FL_SUBMENU_POINTER`, `FL_MENU_INACTIVE`).
#[derive(Clone)]
pub struct MenuItem {
    /// `text`.
    pub label: String,
    /// `shortcut_` — keysym+mods FLTK (`Keys::shortcut`).
    pub shortcut: i32,
    /// `callback_`.
    pub cmd: MenuCmd,
    /// `user_data_`.
    pub data: MenuData,
    /// `FL_MENU_DIVIDER`.
    pub divider: bool,
    /// `FL_MENU_TOGGLE`.
    pub toggle: bool,
    /// `FL_MENU_RADIO`.
    pub radio: bool,
    /// `FL_MENU_VALUE` (coché).
    pub set: bool,
    /// `activate()/deactivate()`.
    pub active: bool,
    /// `FL_SUBMENU`/`FL_SUBMENU_POINTER` — sous-menu.
    pub submenu: Option<Vec<MenuItem>>,
}

impl MenuItem {
    fn it(label: &str, cmd: MenuCmd, data: MenuData) -> Self {
        MenuItem {
            label: label.to_string(),
            shortcut: 0,
            cmd,
            data,
            divider: false,
            toggle: false,
            radio: false,
            set: false,
            active: true,
            submenu: None,
        }
    }
}

/// Popup affiché (≙ `(Fl_Menu_Item*)->popup(x, y, title, picked)`).
/// `picked` = index de l'item choisi (`None` avant présentation).
pub struct Popup {
    /// `Menu_popup_data_t::title`.
    pub title: &'static str,
    /// Items du menu (`\0`-terminated amont → `Vec`).
    pub items: Vec<MenuItem>,
    /// `popup_x`/`popup_y`.
    pub x: i32,
    /// `y`.
    pub y: i32,
    /// `Menu_popup_data_t::picked` (présélection amont — jamais posée).
    pub picked: Option<usize>,
}

/// Globaux amont (`popup_*`, `history_*`, `current_page_url`) +
/// file de popups différés (`Menu_popup_cb`/`Menu_simple_popup_cb`
/// ≙ `a_Timeout_add(0,…)`).
#[derive(Default)]
pub struct MenuCtx {
    /// `popup_bw`.
    pub bw: Option<u32>,
    /// `popup_url`.
    pub url: Option<DilloUrl>,
    /// `popup_page_url`/`popup_link_url` (image menu).
    pub page_url: Option<DilloUrl>,
    /// `popup_link_url`.
    pub link_url: Option<DilloUrl>,
    /// `popup_form`.
    pub form: usize,
    /// `history_direction` (-1 back / 1 forward).
    pub history_dir: i32,
    /// `history_list`.
    pub history_list: Vec<i32>,
    /// `current_page_url` (pour `Menu_open_page_action_cb`).
    pub current_page_url: Option<DilloUrl>,
    /// URLs des feuilles du sous-menu "View stylesheets" — verbatim
    /// `stylesheets[j].user_data = a_Url_dup(url)`.
    pub css_urls: Vec<DilloUrl>,
    /// `hiddens_visible` (form menu).
    pub hiddens_visible: bool,
    /// File de popups en attente (≙ timeouts 0).
    pub pending: VecDeque<Popup>,
}

impl MenuCtx {
    /// `Menu_*_popup` amont → push différé. L'appelant draine via
    /// [`MenuCtx::take_popup`] (≙ `a_Timeout_add(0.0, Menu_popup_cb)`).
    pub fn push(&mut self, p: Popup) {
        self.pending.push_back(p);
    }

    /// `a_Timeout` dispatch — retire le prochain popup.
    pub fn take_popup(&mut self) -> Option<Popup> {
        self.pending.pop_front()
    }
}

/// `a_Menu_file_popup` (verbatim) — shortcuts via `Keys::getShortcut`.
#[must_use]
pub fn file_menu(keys: &Keys) -> Vec<MenuItem> {
    use crate::keys::KeysCommand as K;
    let sc = |c: K| keys.shortcut(c);
    let mut v = Vec::new();
    let mut it = MenuItem::it("New tab", MenuCmd::File("nt"), MenuData::None);
    it.shortcut = sc(K::NewTab);
    v.push(it);
    let mut it = MenuItem::it("New window", MenuCmd::File("nw"), MenuData::None);
    it.shortcut = sc(K::NewWindow);
    it.divider = true;
    v.push(it);
    let mut it = MenuItem::it("Open file...", MenuCmd::File("of"), MenuData::None);
    it.shortcut = sc(K::Open);
    v.push(it);
    let mut it = MenuItem::it("Open URL...", MenuCmd::File("ou"), MenuData::None);
    it.shortcut = sc(K::Goto);
    v.push(it);
    let mut it = MenuItem::it("Close", MenuCmd::File("cw"), MenuData::None);
    it.shortcut = sc(K::CloseTab);
    it.divider = true;
    v.push(it);
    let mut it = MenuItem::it("Exit Dillo", MenuCmd::File("ed"), MenuData::None);
    it.shortcut = sc(K::CloseAll);
    v.push(it);
    v
}

/// `a_Menu_file_popup` verbatim — `popup_x/y` = coin bas-gauche du
/// widget (`wid->x()`, `wid->y()+wid->h()` — résolu par l'appelant).
pub fn file_popup(ctx: &mut MenuCtx, keys: &Keys, bw: u32, x: i32, y: i32) {
    ctx.bw = Some(bw);
    ctx.url = None;
    ctx.push(Popup {
        title: "",
        items: file_menu(keys),
        x,
        y,
        picked: None,
    });
}

/// `link_menu_` verbatim (+ `get_link_menu` : items statiques +
/// actions dynamiques).
#[must_use]
pub fn link_menu_static() -> Vec<MenuItem> {
    let mut v = Vec::new();
    v.push(MenuItem::it(
        "Open link in new tab",
        MenuCmd::OpenUrlNt,
        MenuData::None,
    ));
    let mut it = MenuItem::it(
        "Open link in new window",
        MenuCmd::OpenUrlNw,
        MenuData::None,
    );
    it.divider = true;
    v.push(it);
    v.push(MenuItem::it(
        "Bookmark this link",
        MenuCmd::AddBookmark,
        MenuData::None,
    ));
    let mut it = MenuItem::it("Copy link location", MenuCmd::CopyUrl, MenuData::None);
    it.divider = true;
    v.push(it);
    let mut it = MenuItem::it("Save link as...", MenuCmd::SaveLink, MenuData::None);
    it.divider = true;
    v.push(it);
    v
}

/// `get_link_menu` (verbatim) — statiques + `a_Actions_link_get`.
#[must_use]
pub fn link_menu(link_actions: &[Action]) -> Vec<MenuItem> {
    let mut v = link_menu_static();
    for (i, a) in link_actions.iter().enumerate() {
        let mut it = MenuItem::it(&a.label, MenuCmd::LinkAction(i), MenuData::ActionItem(i));
        it.divider = false;
        v.push(it);
        let _ = a;
    }
    v
}

/// `Menu_set_link_menu_user_data` (verbatim) — les items statiques
/// prennent `url` ; les `LinkAction` gardent leur `mitem` (url+origin
/// stockés dans le ctx — `popup_url`/`page_url` sont déjà posés).
fn set_link_menu_user_data(items: &mut [MenuItem]) {
    for it in items.iter_mut() {
        if !matches!(it.cmd, MenuCmd::LinkAction(_)) {
            it.data = MenuData::PopupUrl;
        }
    }
}

/// `a_Menu_link_popup` (verbatim).
pub fn link_popup(
    ctx: &mut MenuCtx,
    link_actions: &[Action],
    bw: u32,
    x: i32,
    y: i32,
    url: &DilloUrl,
    page_url: Option<&DilloUrl>,
) {
    ctx.bw = Some(bw);
    ctx.url = Some(url.clone());
    ctx.page_url = page_url.cloned();
    let mut items = link_menu(link_actions);
    set_link_menu_user_data(&mut items);
    ctx.push(Popup {
        title: "Link menu",
        items,
        x,
        y,
        picked: None,
    });
}

/// `page_menu_` verbatim (statique ; actions dynamiques ajoutées par
/// [`page_menu_items`] — `stylesheets` sous-menu rempli à chaque popup).
fn page_menu_static() -> Vec<MenuItem> {
    let mut v = Vec::new();
    v.push(MenuItem::it(
        "View page source",
        MenuCmd::ViewSource,
        MenuData::PopupUrl,
    ));
    let mut it = MenuItem::it("View page bugs", MenuCmd::ViewBugs, MenuData::None);
    it.active = false;
    v.push(it);
    // "View stylesheets" — submenu pointer
    let mut ss = MenuItem::it("View stylesheets", MenuCmd::Nop, MenuData::None);
    ss.submenu = Some(Vec::new());
    ss.divider = true;
    ss.active = false;
    v.push(ss);
    let mut it = MenuItem::it(
        "Bookmark this page",
        MenuCmd::AddBookmark,
        MenuData::PopupUrl,
    );
    it.divider = true;
    v.push(it);
    v.push(MenuItem::it("Find text", MenuCmd::FindText, MenuData::None));
    let mut it = MenuItem::it("Save page as...", MenuCmd::SavePage, MenuData::None);
    it.divider = true;
    v.push(it);
    v
}

/// `get_page_menu` (verbatim) — statiques + `a_Actions_page_get`.
#[must_use]
pub fn page_menu_items(page_actions: &[Action]) -> Vec<MenuItem> {
    let mut v = page_menu_static();
    for (i, a) in page_actions.iter().enumerate() {
        v.push(MenuItem::it(
            &a.label,
            MenuCmd::PageAction(i),
            MenuData::ActionItem(i),
        ));
    }
    v
}

/// `a_Menu_page_popup` (verbatim — activation items + sous-menu
/// stylesheets reconstruit à chaque appel avec labels tronqués
/// 30+`...`+40).
pub fn page_popup(
    ctx: &mut MenuCtx,
    page_actions: &[Action],
    bw: u32,
    x: i32,
    y: i32,
    url: Option<&DilloUrl>,
    has_bugs: bool,
    css_urls: &[DilloUrl],
) {
    ctx.bw = Some(bw);
    ctx.url = url.cloned();
    ctx.current_page_url = url.cloned();
    ctx.css_urls = css_urls.to_vec();
    let mut pm = page_menu_items(page_actions);

    pm[1].active = has_bugs;
    // source désactivée pour dpi:/vsource/*
    let is_vsource = url.is_some_and(|u| {
        dlib::str_ascii_casecmp(u.scheme.as_deref().unwrap_or(b""), b"dpi") == 0
            && u.path.as_deref().unwrap_or(b"").starts_with(b"/vsource/")
    });
    if is_vsource {
        pm[0].active = false;
    } else {
        pm[0].active = true;
        pm[0].data = MenuData::PopupUrl;
    }

    if css_urls.is_empty() {
        pm[2].submenu = Some(Vec::new());
        pm[2].active = false;
    } else {
        const HEAD: usize = 30;
        const TAIL: usize = 40;
        let mut sheets = Vec::new();
        for u in css_urls {
            let url_str = String::from_utf8_lossy(&url_str(u)).into_owned();
            let label = if url_str.len() > HEAD + TAIL + 3 {
                format!(
                    "{}...{}",
                    &url_str[..HEAD],
                    &url_str[url_str.len() - TAIL..]
                )
            } else {
                url_str
            };
            // verbatim : `user_data = a_Url_dup(url)` — lié par index
            // dans `ctx.css_urls`.
            let it = MenuItem::it(
                &label,
                MenuCmd::Stylesheet,
                MenuData::Stylesheet(sheets.len()),
            );
            sheets.push(it);
        }
        pm[2].submenu = Some(sheets);
        pm[2].active = true;
    }
    pm[3].data = MenuData::PopupUrl;
    ctx.push(Popup {
        title: "Page menu",
        items: pm,
        x,
        y,
        picked: None,
    });
}

/// `a_Menu_image_popup` (verbatim — pm[] + activations).
pub fn image_popup(
    ctx: &mut MenuCtx,
    link_actions: &[Action],
    bw: u32,
    x: i32,
    y: i32,
    url: &DilloUrl,
    loaded_img: bool,
    page_url: Option<&DilloUrl>,
    link_url: Option<&DilloUrl>,
) {
    ctx.bw = Some(bw);
    ctx.url = Some(url.clone());
    ctx.page_url = page_url.cloned();
    ctx.link_url = link_url.cloned();

    let mut pm = vec![
        MenuItem::it("Isolate image", MenuCmd::OpenUrl, MenuData::PopupUrl),
        MenuItem::it(
            "Open image in new tab",
            MenuCmd::OpenUrlNt,
            MenuData::PopupUrl,
        ),
        {
            let mut i = MenuItem::it(
                "Open image in new window",
                MenuCmd::OpenUrlNw,
                MenuData::PopupUrl,
            );
            i.divider = true;
            i
        },
        MenuItem::it("Load image", MenuCmd::LoadImages, MenuData::PageUrl),
        MenuItem::it(
            "Bookmark this image",
            MenuCmd::AddBookmark,
            MenuData::PopupUrl,
        ),
        {
            let mut i = MenuItem::it("Copy image location", MenuCmd::CopyUrl, MenuData::PopupUrl);
            i.divider = true;
            i
        },
        {
            let mut i = MenuItem::it("Save image as...", MenuCmd::SaveLink, MenuData::PopupUrl);
            i.divider = true;
            i
        },
        {
            let mut i = MenuItem::it("Link menu", MenuCmd::Nop, MenuData::None);
            i.submenu = Some(link_menu(link_actions));
            i
        },
    ];

    pm[3].active = !loaded_img;
    if pm[3].active {
        pm[3].data = MenuData::PageUrl;
    }
    if link_url.is_some() {
        pm[7].active = true;
        if let Some(sub) = pm[7].submenu.as_mut() {
            set_link_menu_user_data(sub);
        }
    } else {
        pm[7].active = false;
    }
    ctx.push(Popup {
        title: "Image menu",
        items: pm,
        x,
        y,
        picked: None,
    });
}

/// `a_Menu_form_popup` (verbatim — label hiddens dynamique).
pub fn form_popup(
    ctx: &mut MenuCtx,
    bw: u32,
    x: i32,
    y: i32,
    page_url: &DilloUrl,
    form: usize,
    showing_hiddens: bool,
) {
    ctx.bw = Some(bw);
    ctx.url = Some(page_url.clone());
    ctx.form = form;
    ctx.hiddens_visible = showing_hiddens;
    let pm = vec![
        MenuItem::it("Submit form", MenuCmd::FormSubmit, MenuData::None),
        MenuItem::it("Reset form", MenuCmd::FormReset, MenuData::None),
        MenuItem::it(
            if showing_hiddens {
                "Hide hiddens"
            } else {
                "Show hiddens"
            },
            MenuCmd::FormHiddens,
            MenuData::HiddensFlag,
        ),
    ];
    ctx.push(Popup {
        title: "Form menu",
        items: pm,
        x,
        y,
        picked: None,
    });
}

/// `a_Menu_bugmeter_popup` (verbatim).
pub fn bugmeter_popup(ctx: &mut MenuCtx, bw: u32, x: i32, y: i32, url: Option<&DilloUrl>) {
    ctx.bw = Some(bw);
    ctx.url = url.cloned();
    let pm = vec![
        MenuItem::it(
            "Validate URL with W3C Nu validator (HTML5 only)",
            MenuCmd::BugmeterValidate(true),
            MenuData::None,
        ),
        {
            let mut i = MenuItem::it(
                "Validate URL with W3C validator (HTML 4.01 and older)",
                MenuCmd::BugmeterValidate(false),
                MenuData::None,
            );
            i.divider = true;
            i
        },
        MenuItem::it("About bug meter", MenuCmd::BugmeterAbout, MenuData::None),
    ];
    ctx.push(Popup {
        title: "",
        items: pm,
        x,
        y,
        picked: None,
    });
}

/// `a_Menu_history_popup` (verbatim — items = titres `history_list`,
/// `direction` ∈ {-1,1}, user_data = index 1-based). `list` ≙
/// `a_UIcmd_get_history(bw, direction)` calculé par l'appelant.
pub fn history_popup(ctx: &mut MenuCtx, bw: u32, x: i32, y: i32, direction: i32, list: Vec<i32>) {
    ctx.bw = Some(bw);
    ctx.history_dir = direction;
    ctx.history_list = list;
    let mut pm = Vec::new();
    // verbatim : `for (i=0; history_list[i] != -1; i++)` — la liste
    // est terminée par `-1` (inclus dans `get_history`).
    for (i, &idx) in ctx
        .history_list
        .iter()
        .take_while(|&&i| i != -1)
        .enumerate()
    {
        let title = history::history_get_title(idx, true).unwrap_or_default();
        pm.push(MenuItem::it(
            &title,
            MenuCmd::History(i + 1),
            MenuData::HistoryIdx(i + 1),
        ));
    }
    ctx.push(Popup {
        title: "",
        items: pm,
        x,
        y,
        picked: None,
    });
}

/// `a_Menu_tools_popup` (verbatim — toggles depuis prefs, radio
/// panel_size, `small icons` toggle).
pub fn tools_popup(
    ctx: &mut MenuCtx,
    prefs: &Prefs,
    bw: u32,
    x: i32,
    y: i32,
    cur_panelsize: i32,
    cur_smallicons: bool,
) {
    ctx.bw = Some(bw);
    ctx.url = None;
    let mut pm = vec![
        {
            let mut i = MenuItem::it("Use remote CSS", MenuCmd::RemoteCss, MenuData::None);
            i.toggle = true;
            i.set = prefs.load_stylesheets;
            i
        },
        {
            let mut i = MenuItem::it("Use embedded CSS", MenuCmd::EmbeddedCss, MenuData::None);
            i.toggle = true;
            i.divider = true;
            i.set = prefs.parse_embedded_css;
            i
        },
        {
            let mut i = MenuItem::it("Load images", MenuCmd::ImgloadToggle, MenuData::None);
            i.toggle = true;
            i.set = prefs.load_images;
            i
        },
        {
            let mut i = MenuItem::it(
                "Load background images",
                MenuCmd::BgimgToggle,
                MenuData::None,
            );
            i.toggle = true;
            i.divider = true;
            i.set = prefs.load_background_images;
            i
        },
        {
            let mut i = MenuItem::it("Force HTTPS", MenuCmd::ForceHttps, MenuData::None);
            i.toggle = true;
            i.divider = true;
            i.set = prefs.http_force_https;
            i
        },
        {
            // "Panel size" + submenu radio 0/1/2 + toggle small icons
            let mut sub = vec![
                {
                    let mut i = MenuItem::it("tiny", MenuCmd::PanelChange(0), MenuData::None);
                    i.radio = true;
                    i.set = cur_panelsize == 0;
                    i
                },
                {
                    let mut i = MenuItem::it("small", MenuCmd::PanelChange(1), MenuData::None);
                    i.radio = true;
                    i.set = cur_panelsize == 1;
                    i
                },
                {
                    let mut i = MenuItem::it("medium", MenuCmd::PanelChange(2), MenuData::None);
                    i.radio = true;
                    i.divider = true;
                    i.set = cur_panelsize == 2;
                    i
                },
                {
                    let mut i =
                        MenuItem::it("small icons", MenuCmd::PanelChange(10), MenuData::None);
                    i.toggle = true;
                    i.set = cur_smallicons;
                    i
                },
            ];
            let mut it = MenuItem::it("Panel size", MenuCmd::Nop, MenuData::None);
            it.submenu = Some(std::mem::take(&mut sub));
            it
        },
    ];
    ctx.push(Popup {
        title: "",
        items: std::mem::take(&mut pm),
        x,
        y,
        picked: None,
    });
}

// -- dispatch (callbacks verbatim) ----------------------------------------------

/// Contexte d'appel d'une action custom (`link_menu_item` amont :
/// `url`+`origin`+`action`).
pub struct ActionEnv<'a> {
    /// `url` (≙ `URL_STR(url)`).
    pub url: &'a [u8],
    /// `origin` (≙ `URL_STR(origin)` — page ou lien source).
    pub origin: &'a [u8],
}

/// `Menu_open_url_action_cb` verbatim — `setenv("url")` +
/// `setenv("origin")` + `a_Actions_run`. (Pas de unset amont ici.)
fn run_link_action(action: &Action, url: &DilloUrl, origin: &DilloUrl) {
    // SAFETY-free : std::env::set_var est safe en Rust (le risque
    // MT est géré par le modèle single-thread du port).
    std::env::set_var("url", String::from_utf8_lossy(&url_str(url)).as_ref());
    std::env::set_var("origin", String::from_utf8_lossy(&url_str(origin)).as_ref());
    let _ = actions_run(action);
}

/// `Menu_open_page_action_cb` verbatim — `setenv("url")` + run +
/// `unsetenv("url")`.
fn run_page_action(action: &Action, url: Option<&DilloUrl>) {
    if let Some(u) = url {
        std::env::set_var("url", String::from_utf8_lossy(&url_str(u)).as_ref());
    }
    let _ = actions_run(action);
    std::env::remove_var("url");
}

/// `URL_STR(url)` sur une url immuable (`url_str` rebuilde le cache).
fn url_str(u: &DilloUrl) -> Vec<u8> {
    let mut c = u.clone();
    c.url_str().to_vec()
}

/// `Menu_bugmeter_validate` verbatim — `popup_url` non-dpi, fragment
/// tronqué, url hex-encodée dans le gabarit validator.
fn bugmeter_validate(
    uicmd: &UiCmd,
    reg: &BwRegistry,
    shell: &mut dyn UiShell,
    prefs: &Prefs,
    ctx: &MenuCtx,
    validator_url: &str,
) {
    if let Some(url) = &ctx.url {
        if dlib::str_ascii_casecmp(url.scheme.as_deref().unwrap_or(b""), b"dpi") != 0 {
            let popup_str = String::from_utf8_lossy(&url_str(url)).into_owned();
            let no_fragment = match popup_str.rfind('#') {
                Some(p) => &popup_str[..p],
                None => &popup_str,
            };
            let encoded =
                String::from_utf8_lossy(&dillo_net::url::encode_hex_str(no_fragment.as_bytes()))
                    .into_owned();
            let target = validator_url.replacen("%s", &encoded, 1);
            let Some(bw) = ctx.bw else { return };
            uicmd.open_urlstr(reg, shell, prefs, bw, target.as_bytes());
        }
    }
}

/// `Menu_popup_cb`/`do_callback` — exécute la callback de l'item
/// choisi. `mouse_button`/`shift` ≙ `Fl::event_button()`/`event_state
/// (FL_SHIFT)` au moment du clic (verbatim `Menu_*_cb`).
///
/// `prefs` ≙ global amont mutable : les toggles (`prefs.x = v`) vont
/// via `borrow_mut()` (verbatim `FL_MENU_VALUE`), les lectures via
/// `borrow()` — jamais simultanés (single thread ≙ amont).
#[allow(clippy::too_many_arguments)]
pub fn dispatch(
    ctx: &mut MenuCtx,
    uicmd: &UiCmd,
    reg: &BwRegistry,
    shell: &mut dyn UiShell,
    prefs: &RefCell<Prefs>,
    item: &mut MenuItem,
    mouse_button: i32,
    shift: bool,
    page_actions: &[Action],
    link_actions: &[Action],
    doc_ops: &mut dyn DocOps,
) {
    if !item.active {
        return; // verbatim : items inactifs ne dispatch pas
    }
    let Some(bw) = ctx.bw else { return };
    let url_of = |data: &MenuData| -> Option<DilloUrl> {
        match data {
            MenuData::PageUrl => ctx.page_url.clone(),
            MenuData::LinkUrl => ctx.link_url.clone(),
            MenuData::Stylesheet(i) => ctx.css_urls.get(*i).cloned(),
            _ => ctx.url.clone(),
        }
    };
    match &item.cmd {
        MenuCmd::Nop => {}
        MenuCmd::File(code) => match *code {
            "nw" => uicmd.open_url_nw(reg, shell, &prefs.borrow(), bw, None),
            "nt" => uicmd.open_url_nt(reg, shell, &prefs.borrow(), bw, None, 1),
            "of" => uicmd.open_file(reg, shell, &prefs.borrow(), bw),
            "ou" => uicmd.focus_location(reg, shell, bw),
            // `a_Timeout_add(0.0, a_UIcmd_close_bw, bw)` verbatim
            "cw" => shell.defer_nav(bw, crate::nav::NavOp::CloseBw),
            "ed" => shell.defer_close_all(),
            _ => {}
        },
        MenuCmd::OpenUrl => {
            if let Some(u) = url_of(&item.data) {
                uicmd.open_url(reg, shell, &prefs.borrow(), bw, Some(&u));
            }
        }
        MenuCmd::OpenUrlNw => {
            if let Some(u) = url_of(&item.data) {
                uicmd.open_url_nw(reg, shell, &prefs.borrow(), bw, Some(&u));
            }
        }
        MenuCmd::OpenUrlNt => {
            let mut focus = i32::from(prefs.borrow().focus_new_tab);
            if shift {
                focus = i32::from(focus == 0);
            }
            if let Some(u) = url_of(&item.data) {
                uicmd.open_url_nt(reg, shell, &prefs.borrow(), bw, Some(&u), focus);
            }
        }
        MenuCmd::AddBookmark => {
            if let Some(u) = url_of(&item.data) {
                uicmd.add_bookmark(shell, bw, &u);
            }
        }
        MenuCmd::CopyUrl => {
            if let Some(u) = url_of(&item.data) {
                uicmd.copy_urlstr(reg, bw, &url_str(&u), 2);
            }
        }
        MenuCmd::SaveLink => {
            if let Some(u) = url_of(&item.data) {
                uicmd.save_link(reg, shell, bw, &u, None);
            }
        }
        MenuCmd::SavePage => uicmd.save_page(reg, shell, bw),
        MenuCmd::ViewSource => {
            if let Some(u) = url_of(&item.data) {
                uicmd.view_page_source(reg, shell, &prefs.borrow(), bw, &u);
            }
        }
        MenuCmd::ViewBugs => uicmd.view_page_bugs(reg, shell, bw),
        MenuCmd::FindText => uicmd.findbar_toggle(reg, shell, bw, true),
        MenuCmd::LoadImages => {
            // doc = a_Bw_get_url_doc(popup_bw, page_url)
            if let Some(doc) = reg
                .get(bw)
                .and_then(|b| ctx.page_url.clone().and_then(|u| b.get_url_doc(&u)))
            {
                doc_ops.load_images(doc, ctx.url.as_ref());
            }
        }
        MenuCmd::FormSubmit => {
            if let Some(doc) = reg
                .get(bw)
                .and_then(|b| ctx.url.as_ref().and_then(|u| b.get_url_doc(u)))
            {
                doc_ops.form_submit(doc, ctx.form);
            }
        }
        MenuCmd::FormReset => {
            if let Some(doc) = reg
                .get(bw)
                .and_then(|b| ctx.url.as_ref().and_then(|u| b.get_url_doc(u)))
            {
                doc_ops.form_reset(doc, ctx.form);
            }
        }
        MenuCmd::FormHiddens => {
            if let Some(doc) = reg
                .get(bw)
                .and_then(|b| ctx.url.as_ref().and_then(|u| b.get_url_doc(u)))
            {
                doc_ops.form_display_hiddens(doc, ctx.form, !ctx.hiddens_visible);
            }
        }
        MenuCmd::Stylesheet => {
            if mouse_button == 1 {
                if let Some(u) = url_of(&item.data) {
                    uicmd.open_url(reg, shell, &prefs.borrow(), bw, Some(&u));
                }
            } else if mouse_button == 2 {
                let p = prefs.borrow();
                if p.middle_click_opens_new_tab {
                    let mut focus = i32::from(p.focus_new_tab);
                    if shift {
                        focus = i32::from(focus == 0);
                    }
                    drop(p);
                    if let Some(u) = url_of(&item.data) {
                        uicmd.open_url_nt(reg, shell, &prefs.borrow(), bw, Some(&u), focus);
                    }
                } else {
                    drop(p);
                    if let Some(u) = url_of(&item.data) {
                        uicmd.open_url_nw(reg, shell, &prefs.borrow(), bw, Some(&u));
                    }
                }
            }
        }
        MenuCmd::BugmeterValidate(nu) => {
            let tpl = if *nu {
                "https://validator.w3.org/nu/?useragent=Validator.nu%2FLV+https%3A%2F%2Fvalidator.w3.org%2Fservices&acceptlanguage=&doc=%s"
            } else {
                "https://validator.w3.org/check?uri=%s&charset=%28detect+automatically%29&doctype=Inline&group=0&user-agent=W3C_Validator%2F1.3+"
            };
            bugmeter_validate(uicmd, reg, shell, &prefs.borrow(), ctx, tpl);
        }
        MenuCmd::BugmeterAbout => {
            uicmd.open_urlstr(
                reg,
                shell,
                &prefs.borrow(),
                bw,
                b"https://dillo-browser.org/old/help/bug_meter.html",
            );
        }
        MenuCmd::History(i) => {
            let i = *i;
            if mouse_button == 1 {
                uicmd.nav_jump(
                    reg,
                    shell,
                    &prefs.borrow(),
                    bw,
                    ctx.history_dir * i as i32,
                    false,
                );
            } else if mouse_button == 2 {
                if let Some(&idx) = ctx.history_list.get(i.saturating_sub(1)) {
                    if let Some(url) = history::history_get_url(idx) {
                        let p = prefs.borrow();
                        if p.middle_click_opens_new_tab {
                            let mut focus = i32::from(p.focus_new_tab);
                            if shift {
                                focus = i32::from(focus == 0);
                            }
                            drop(p);
                            uicmd.open_url_nt(reg, shell, &prefs.borrow(), bw, Some(&url), focus);
                        } else {
                            drop(p);
                            uicmd.open_url_nw(reg, shell, &prefs.borrow(), bw, Some(&url));
                        }
                    }
                }
            }
        }
        MenuCmd::RemoteCss => {
            item.set = !item.set;
            prefs.borrow_mut().load_stylesheets = item.set;
            uicmd.repush(reg, shell, &prefs.borrow(), bw);
        }
        MenuCmd::EmbeddedCss => {
            item.set = !item.set;
            prefs.borrow_mut().parse_embedded_css = item.set;
            uicmd.repush(reg, shell, &prefs.borrow(), bw);
        }
        MenuCmd::ForceHttps => {
            item.set = !item.set;
            prefs.borrow_mut().http_force_https = item.set;
            uicmd.repush(reg, shell, &prefs.borrow(), bw);
        }
        MenuCmd::ImgloadToggle => {
            item.set = !item.set;
            prefs.borrow_mut().load_images = item.set;
            if item.set {
                if let Some(doc) = reg.get(bw).and_then(|b| b.get_current_doc()) {
                    doc_ops.load_images(doc, None);
                }
            }
        }
        MenuCmd::BgimgToggle => {
            item.set = !item.set;
            prefs.borrow_mut().load_background_images = item.set;
            uicmd.repush(reg, shell, &prefs.borrow(), bw);
        }
        MenuCmd::PanelChange(code) => {
            // verbatim `Menu_panel_change_cb` : 10 → toggle small
            // icons (taille conservée), sinon taille (icons conservés).
            if let Some(b) = reg.get(bw) {
                let ui = b.ui.get();
                let (size, icons) = shell.panel_state(ui);
                if *code == 10 {
                    shell.panel_change(ui, size, !icons);
                } else {
                    shell.panel_change(ui, *code, icons);
                }
            }
        }
        MenuCmd::PageAction(i) => {
            if let Some(a) = page_actions.get(*i) {
                run_page_action(a, ctx.current_page_url.as_ref());
            }
        }
        MenuCmd::LinkAction(i) => {
            if let (Some(a), Some(u), Some(o)) =
                (link_actions.get(*i), ctx.url.clone(), ctx.page_url.clone())
            {
                run_link_action(a, &u, &o);
            }
        }
    }
}

/// Seam vers les `a_Html_*` amont — le doc `usize` (≙ `void *doc`
/// de `a_Bw_get_url_doc`) est résolu par l'appelant vers le
/// `DilloHtml` vivant. Implémenté par l'app (chrome-4) ; mock en
/// test.
pub trait DocOps {
    /// `a_Html_load_images(doc, pattern)` — `None` ≙ `NULL`.
    fn load_images(&mut self, doc: usize, pattern: Option<&DilloUrl>);
    /// `a_Html_form_submit(doc, form)`.
    fn form_submit(&mut self, doc: usize, form: usize);
    /// `a_Html_form_reset(doc, form)`.
    fn form_reset(&mut self, doc: usize, form: usize);
    /// `a_Html_form_display_hiddens(doc, form, display)`.
    fn form_display_hiddens(&mut self, doc: usize, form: usize, display: bool);
}

/// `DocOps` no-op (popups inertes tant que l'app n'a pas injecté
/// le registre de docs — verbatim amont : `doc == NULL` → rien).
pub struct NullDocOps;

impl DocOps for NullDocOps {
    fn load_images(&mut self, _doc: usize, _pattern: Option<&DilloUrl>) {}
    fn form_submit(&mut self, _doc: usize, _form: usize) {}
    fn form_reset(&mut self, _doc: usize, _form: usize) {}
    fn form_display_hiddens(&mut self, _doc: usize, _form: usize, _display: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use dillo_net::url::url_new;

    fn u(s: &str) -> DilloUrl {
        url_new(s.as_bytes(), None).unwrap()
    }

    #[test]
    fn file_menu_items_verbatim() {
        // 6 items amont, dividers après "New window" et "Close".
        let keys = Keys::new();
        let m = file_menu(&keys);
        assert_eq!(m.len(), 6);
        assert_eq!(m[0].label, "New tab");
        assert_eq!(m[1].label, "New window");
        assert!(m[1].divider && m[4].divider && !m[0].divider);
        assert_eq!(m[2].label, "Open file...");
        assert_eq!(m[3].label, "Open URL...");
        assert_eq!(m[4].label, "Close");
        assert_eq!(m[5].label, "Exit Dillo");
        // shortcuts présents (défauts keys.cc)
        assert_ne!(m[0].shortcut, 0);
    }

    #[test]
    fn link_menu_static_and_actions() {
        let m = link_menu_static();
        assert_eq!(m.len(), 5);
        assert_eq!(m[0].label, "Open link in new tab");
        assert!(m[1].divider && m[3].divider && m[4].divider);
        // actions dynamiques ajoutées à la suite (verbatim)
        let acts = vec![Action {
            label: "Wget".into(),
            cmd: "wget $url".into(),
        }];
        let m2 = link_menu(&acts);
        assert_eq!(m2.len(), 6);
        assert_eq!(m2[5].label, "Wget");
        assert!(matches!(m2[5].cmd, MenuCmd::LinkAction(0)));
    }

    #[test]
    fn page_popup_activation_and_stylesheets() {
        let mut ctx = MenuCtx::default();
        let url = u("https://example.org/page#frag");
        page_popup(&mut ctx, &[], 3, 10, 20, Some(&url), true, &[]);
        let pop = ctx.take_popup().unwrap();
        assert_eq!(pop.title, "Page menu");
        assert_eq!(pop.items.len(), 6);
        assert!(pop.items[1].active); // has_bugs → activate
        assert!(!pop.items[2].active); // pas de stylesheets → deactivate
        assert_eq!((pop.x, pop.y), (10, 20));
        assert_eq!(
            ctx.url.as_ref().map(url_str).unwrap(),
            b"https://example.org/page#frag"
        );

        // dpi:/vsource/* désactive "View page source" (verbatim)
        let mut ctx2 = MenuCtx::default();
        let dpi = u("dpi:/vsource/x");
        page_popup(&mut ctx2, &[], 3, 0, 0, Some(&dpi), false, &[]);
        let pop2 = ctx2.take_popup().unwrap();
        assert!(!pop2.items[0].active);
        assert!(!pop2.items[1].active); // !has_bugs → deactivate

        // sous-menu stylesheets : labels tronqués 30+...+40 (verbatim)
        let long = u("https://cdn.example.org/styles/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.css");
        let mut ctx3 = MenuCtx::default();
        page_popup(&mut ctx3, &[], 3, 0, 0, Some(&url), true, &[long]);
        let pop3 = ctx3.take_popup().unwrap();
        let sheets = pop3.items[2].submenu.as_ref().unwrap();
        assert_eq!(sheets.len(), 1);
        let lbl = &sheets[0].label;
        assert!(lbl.contains("...") && lbl.len() == 30 + 3 + 40);
        assert!(pop3.items[2].active);
        assert_eq!(ctx3.css_urls.len(), 1);
    }

    #[test]
    fn image_popup_verbatim_states() {
        let mut ctx = MenuCtx::default();
        let img = u("https://ex.org/i.png");
        let page = u("https://ex.org/p");
        let link = u("https://ex.org/l");
        // image chargée → "Load image" désactivé ; lien présent → submenu actif
        image_popup(&mut ctx, &[], 1, 5, 6, &img, true, Some(&page), Some(&link));
        let pop = ctx.take_popup().unwrap();
        assert_eq!(pop.items.len(), 8);
        assert!(!pop.items[3].active); // Load image
        assert!(pop.items[7].active); // Link menu
        assert!(pop.items[7].submenu.is_some());
        // non chargée, pas de lien
        image_popup(&mut ctx, &[], 1, 0, 0, &img, false, Some(&page), None);
        let pop2 = ctx.take_popup().unwrap();
        assert!(pop2.items[3].active);
        assert!(!pop2.items[7].active);
    }

    #[test]
    fn form_and_bugmeter_popups() {
        let mut ctx = MenuCtx::default();
        let page = u("https://ex.org/f");
        form_popup(&mut ctx, 2, 0, 0, &page, 7, true);
        let pop = ctx.take_popup().unwrap();
        assert_eq!(pop.items[2].label, "Hide hiddens"); // verbatim
        assert_eq!(ctx.form, 7);
        form_popup(&mut ctx, 2, 0, 0, &page, 7, false);
        assert_eq!(ctx.take_popup().unwrap().items[2].label, "Show hiddens");

        bugmeter_popup(&mut ctx, 2, 1, 2, Some(&page));
        let bp = ctx.take_popup().unwrap();
        assert_eq!(bp.items.len(), 3);
        assert!(bp.items[1].divider);
        assert!(matches!(bp.items[0].cmd, MenuCmd::BugmeterValidate(true)));
        assert!(matches!(bp.items[1].cmd, MenuCmd::BugmeterValidate(false)));
    }

    #[test]
    fn history_popup_stops_at_minus_one() {
        let mut ctx = MenuCtx::default();
        history_popup(&mut ctx, 9, 1, 2, -1, vec![3, 5, -1, 99]);
        let pop = ctx.take_popup().unwrap();
        assert_eq!(pop.items.len(), 2); // sentinel -1 termine la liste
        assert_eq!(ctx.history_dir, -1);
        assert!(matches!(pop.items[1].cmd, MenuCmd::History(2)));
    }

    #[test]
    fn tools_popup_toggles_and_radios() {
        let mut prefs = Prefs::default();
        prefs.load_stylesheets = true;
        prefs.parse_embedded_css = false;
        prefs.load_images = true;
        prefs.load_background_images = false;
        prefs.http_force_https = false;
        let mut ctx = MenuCtx::default();
        tools_popup(&mut ctx, &prefs, 4, 30, 40, 2, true);
        let pop = ctx.take_popup().unwrap();
        assert_eq!(pop.items.len(), 6);
        assert!(pop.items[0].set && pop.items[0].toggle); // remote CSS
        assert!(!pop.items[1].set); // embedded CSS
        assert!(pop.items[2].set && pop.items[4].divider);
        let sub = pop.items[5].submenu.as_ref().unwrap();
        assert_eq!(sub.len(), 4);
        assert!(sub[2].set && !sub[0].set); // radio = medium
        assert!(sub[3].set); // small icons
        assert_eq!((pop.x, pop.y), (30, 40));
    }
}
