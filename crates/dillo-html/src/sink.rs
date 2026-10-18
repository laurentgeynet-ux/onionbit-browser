//! Seams navigateur pour le parseur HTML — `BrowserWindow*` +
//! `a_UIcmd_*` + `a_Bw_*` + `a_Dialog_choice` + `a_Misc_file2dstr`
//! + chargements réseau (`a_Web_new`/`a_Capi_open_url`/`a_Bw_add_*`).
//!
//! Le cache lui-même (`a_Capi_get_flags*`, `a_Capi_get_buf`,
//! `a_Capi_set_content_type`, `a_Capi_dpi_verify_request`) est
//! appelé directement via `dillo_net::capi` — verbatim.
//!
//! `prefs.*` consommés par `html.cc`/`table.cc`/`form.cc`/`plain.cc`
//! sont regroupés dans [`HtmlPrefs`].
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

use crate::common::ImageHandle;
use dillo_css::styleengine::StyleEnginePrefs;
use dillo_net::url::DilloUrl;

/// `prefs.*` — verbatim des accès dans les fichiers du périmètre.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // verbatim `Prefs` amont.
pub struct HtmlPrefs {
    /// `prefs.limit_text_width`.
    pub limit_text_width: bool,
    /// `prefs.show_tooltip`.
    pub show_tooltip: bool,
    /// `prefs.show_extra_warnings`.
    pub show_extra_warnings: bool,
    /// `prefs.parse_embedded_css`.
    pub parse_embedded_css: bool,
    /// `prefs.load_stylesheets`.
    pub load_stylesheets: bool,
    /// `prefs.load_images`.
    pub load_images: bool,
    /// `prefs.mark_unloaded_images`.
    pub mark_unloaded_images: bool,
    /// `prefs.contrast_visited_color`.
    pub contrast_visited_color: bool,
    /// `prefs.middle_click_opens_new_tab`.
    pub middle_click_opens_new_tab: bool,
    /// `prefs.focus_new_tab`.
    pub focus_new_tab: bool,
    /// `prefs.enterpress_forces_submit`.
    pub enterpress_forces_submit: bool,
    /// `prefs.buffered_drawing` (≙ `prefs` dw — double-buffering).
    pub buffered_drawing: i32,
    /// Préférences du `StyleEngine` (partagées, durée `'p`).
    pub style: StyleEnginePrefs,
}

/// `BrowserWindow*` + `a_UIcmd_*` + `a_Bw_*` + dialogs.
/// Implémenté par `dillo-browser` (≙ `bw.cc`/`uicmd.cc`).
pub trait HtmlSink {
    // -- bugs / messages --------------------------------------------------

    /// `bugMessage` : append `bw->page_bugs` (préfixe
    /// `"HTML warning: line %d, "` fait par l'appelant) +
    /// `a_UIcmd_set_bug_prog(bw, ++bw->num_page_bugs)`.
    fn bug_message(&mut self, msg: &str);
    /// `a_UIcmd_set_msg`.
    fn set_msg(&mut self, msg: &[u8]);
    /// `a_UIcmd_set_page_title` + `a_History_set_title_by_url`.
    fn set_page_title(&mut self, url: &DilloUrl, title: &[u8]);

    // -- navigation ---------------------------------------------------------

    /// `a_UIcmd_open_url`.
    fn open_url(&mut self, url: &DilloUrl);
    /// `a_UIcmd_open_url_nw`.
    fn open_url_nw(&mut self, url: &DilloUrl);
    /// `a_UIcmd_open_url_nt`.
    fn open_url_nt(&mut self, url: &DilloUrl, focus: bool);
    /// `a_UIcmd_redirection0` (meta refresh).
    fn redirection0(&mut self, url: DilloUrl);
    /// `a_UIcmd_repush` — re-rendu quand les feuilles sont chargées.
    fn repush(&mut self);

    /// Feedback « visited » du lien cliqué — ≙ la maj synchrone
    /// `DwTextblock::changeLinkColor` dans `HTML_link_clicked`
    /// (html.cc). **Doit être différée** : `clicked` tourne sous
    /// un `borrow_mut` du `Layout` (emission de signal pendant
    /// `button_event`), le sink route vers `GlueOp` — un emprunt
    /// direct paniquerait (double-borrow `RefCell`).
    /// `widget` ≙ le widget cliqué (la maj remonte à son
    /// textblock parent), `color` ≙ `prefs.contrast_visited_color`
    /// résolue (`visited_color`).
    fn link_visited(&mut self, _widget: Option<usize>, _link: i32, _color: i32) {}

    // -- menus contextuels --------------------------------------------------

    /// `a_UIcmd_page_popup`.
    fn page_popup(&mut self, has_bugs: bool, css_urls: &[DilloUrl]);
    /// `a_UIcmd_link_popup`.
    fn link_popup(&mut self, url: &DilloUrl, page_url: &DilloUrl);
    /// `a_UIcmd_image_popup`.
    fn image_popup(
        &mut self,
        url: &DilloUrl,
        loaded: bool,
        page_url: &DilloUrl,
        link: Option<&DilloUrl>,
    );
    /// `a_UIcmd_form_popup`.
    fn form_popup(&mut self, page_url: &DilloUrl, form_id: usize, showing_hiddens: bool);

    // -- état bw / clients --------------------------------------------------

    /// Identité du `BrowserWindow*` amont (≙ `u32` des seams
    /// `capi`/`nav` — `a_Capi_dpi_verify_request`, referer stack).
    fn bw_id(&self) -> u32;
    /// `a_Bw_expecting`.
    fn expecting(&self) -> bool;
    /// `a_Bw_close_client`.
    fn close_client(&mut self, key: i32);
    /// `a_Bw_add_doc` — enregistre le document html dans le bw.
    fn add_doc(&mut self);
    /// `a_Bw_remove_doc`.
    fn remove_doc(&mut self);
    /// `bw->NumPendingStyleSheets`.
    fn num_pending_style_sheets(&self) -> i32;
    /// `++/--bw->NumPendingStyleSheets` (set).
    fn set_num_pending_style_sheets(&mut self, n: i32);
    /// `bw->zoom`.
    fn zoom(&self) -> f64;

    // -- chargements --------------------------------------------------------

    /// `Html_load_image` (html.cc:2202) : `a_Web_new` +
    /// `Web->Image = image` + `a_Capi_open_url` + `a_Bw_add_client` +
    /// `a_Bw_add_url`. Retour `ClientKey != 0` ; le sink garde une
    /// référence (`Rc`) sur l'image ≙ web-owned.
    fn load_image(
        &mut self,
        url: &DilloUrl,
        requester: Option<&DilloUrl>,
        image: &ImageHandle,
    ) -> bool;

    /// Partie réseau de `a_Html_load_stylesheet` (html.cc:3290) :
    /// `a_Web_new(requester=page_url)` + `WEB_Stylesheet` +
    /// `a_Capi_open_url(Html_css_load_callback)` + `a_Bw_add_*`.
    /// Le compteur `NumPendingStyleSheets` est géré par l'appelant.
    fn load_stylesheet(&mut self, url: &DilloUrl, requester: &DilloUrl) -> bool;

    // -- formulaires / dialogs ----------------------------------------------

    /// `a_UIcmd_select_file` — `Some(path)` si choisi.
    fn select_file(&mut self) -> Option<String>;
    /// `a_Dialog_choice` — retourne l'indice du bouton (1-based,
    /// 0 ≙ annulation — verbatim `choice != 1`).
    fn dialog_choice(&mut self, title: &str, msg: &str, buttons: &[&str]) -> i32;
    /// `a_Misc_file2dstr` (input `type=file` — lecture du fichier).
    fn file_to_string(&mut self, path: &str) -> Option<Vec<u8>>;

    /// Upcast vers `StyleEngineSink` — le même `bw` amont sert aux
    /// deux callbacks (`style(bw)` / `CssParser` image links).
    fn as_style_engine_sink(&mut self) -> &mut dyn dillo_css::styleengine::StyleEngineSink;
}
