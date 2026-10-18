//! Port of `src/web.cc` (`a_Web_dispatch_by_type` partie navigateur),
//! `src/html.cc` (`a_Html_text`, `Html_callback`, `Html_load_image`,
//! partie réseau d'`a_Html_load_stylesheet`, `Html_css_load_callback`),
//! `src/plain.cc` (`a_Plain_text`, `Plain_callback`),
//! `src/styleengine.cc` (`startBackgroundImageLoad`) et
//! `src/IO/mime.c` `a_Mime_init` (viewers texte).
//!
//! C'est le chaînon entre le cache/capi (dillo-net) et la fenêtre :
//! amont ces appels atteignent `BrowserWindow*`/`UI*` globaux — ici
//! ils passent par [`DocSink`] (per-bw) et une file d'ops [`GlueOp`]
//! drainée par `Glue::tick` ([`crate::glue::Glue`]).
//!
//! **Modèle single-writer** (AGENTS.md §10) : les seams réseau ne
//! touchent jamais le `Chrome`/`UiCmd` — ils émettent des ops dans la
//! file partagée, exécutées au tick suivant sur le thread UI. Les
//! lectures `bw` directes (cellules) restent synchrones, verbatim amont
//! où `bw->…` est un accès champ.
//!
//! Déviations documentées (`docs/PORTING.md`) :
//! - les ops UI sont exécutées au **tick suivant** au lieu de
//!   synchronement mid-callback (≙ `a_Timeout_add(0)` que nav.c utilise
//!   déjà pour `repush`/`redirection0`) ;
//! - `StyleImageDeletionReceiver` non porté : le client cache d'une
//!   background-image vit jusqu'au `a_Bw_cleanup` (le style survit à
//!   l'image en pratique — cas rare noté).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::large_enum_variant,
    clippy::too_many_lines
)]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use dillo_css::parser::NullTagIndex;
use dillo_css::styleengine::{StyleEngine, StyleEngineSink};
use dillo_dw::imgrenderer::ImgRenderer;
use dillo_dw::style::{self, Color};
use dillo_dw::WidgetId;
use dillo_html::common::ImageHandle;
use dillo_html::html::DilloHtml;
use dillo_html::plain::DilloPlain;
use dillo_html::sink::{HtmlPrefs, HtmlSink};
use dillo_net::cache::{BrowserOps, CaCb, CacheClient, NavOps, UiOps};
use dillo_net::capi::{self, DpiOps};
use dillo_net::http::HttpWeb;
use dillo_net::mime::{self, MimeViewer};
use dillo_net::url::DilloUrl;
use dillo_net::web::{self, WebDispatch, WEB_IMAGE, WEB_ROOT_URL, WEB_STYLESHEET};

use crate::bw::{BrowserWindow, BwRegistry, BwUi};
use crate::dialog::{
    choice_spec, file_spec, AutoCancelRunner, DialogAnswer, DialogKind, DialogRunner,
};

use crate::prefs::Prefs;

/// File d'ops navigateur différées — ≙ les `a_UIcmd_*` que les
/// callbacks réseau invoquent directement amont. Drainée par le glue
/// sur le thread UI.
pub type IoQueue = Rc<RefCell<VecDeque<GlueOp>>>;

/// Crée la file partagée.
#[must_use]
pub fn io_queue() -> IoQueue {
    Rc::new(RefCell::new(VecDeque::new()))
}

/// Opération différée vers `UiCmd`/`Chrome` (≙ `a_UIcmd_*` amont,
/// jouée au tick — même sémantique de report que les callbacks
/// `a_Timeout_add(0)` de `nav.c`).
pub enum GlueOp {
    /// `a_UIcmd_set_msg`.
    SetMsg { bw: u32, msg: String },
    /// `a_UIcmd_set_page_title` (label ≙ titre nu). `url`+`hist` :
    /// `a_History_set_title_by_url` — fait **au drain** car amont
    /// tout est synchrone et `expect_done` (`add_url`, headers)
    /// précède toujours `<title>` (body) ; un appel eager au parse
    /// inverserait cet ordre. `hist` ≙ uniquement le chemin
    /// `<title>` (html.cc:1727), pas le basename du dispatch.
    SetPageTitle {
        bw: u32,
        url: DilloUrl,
        title: Option<String>,
        hist: bool,
    },
    /// `a_UIcmd_set_bug_prog`.
    SetBugProg { bw: u32, n: i32 },
    /// `a_UIcmd_set_buttons_sens` (relit le snapshot nav).
    SetButtonsSens { bw: u32 },
    /// `a_UIcmd_set_location_text`.
    SetLocationText { bw: u32, text: Vec<u8> },
    /// `a_UIcmd_set_page_prog`.
    SetPageProg { bw: u32, bytes: usize, cmd: i32 },
    /// `a_UIcmd_set_img_prog`.
    SetImgProg {
        bw: u32,
        got: i32,
        total: i32,
        cmd: i32,
    },
    /// `a_UIcmd_finish_loading`.
    FinishLoading { bw: u32 },
    /// `a_UIcmd_open_url`.
    OpenUrl { bw: u32, url: DilloUrl },
    /// `a_UIcmd_open_urlstr` (commandes `open`/`url` de dilloc).
    OpenUrlStr { bw: u32, url: String },
    /// `a_UIcmd_open_url_nw`.
    OpenUrlNw { bw: u32, url: DilloUrl },
    /// `a_UIcmd_open_url_nt` (`offset` ≙ `focus_new_tab`).
    OpenUrlNt { bw: u32, url: DilloUrl, focus: bool },
    /// `a_UIcmd_redirection0` (meta refresh).
    Redirection0 { bw: u32, url: DilloUrl },
    /// `a_UIcmd_repush`.
    Repush { bw: u32 },
    /// `a_UIcmd_page_popup`.
    PagePopup {
        bw: u32,
        has_bugs: bool,
        css_urls: Vec<DilloUrl>,
    },
    /// `a_UIcmd_link_popup`.
    LinkPopup {
        bw: u32,
        url: DilloUrl,
        page_url: Option<DilloUrl>,
    },
    /// `a_UIcmd_image_popup`.
    ImagePopup {
        bw: u32,
        url: DilloUrl,
        loaded: bool,
        page_url: DilloUrl,
        link: Option<DilloUrl>,
    },
    /// `a_UIcmd_form_popup`.
    FormPopup {
        bw: u32,
        page_url: DilloUrl,
        form: usize,
        showing_hiddens: bool,
    },
    /// `a_UIcmd_bugmeter_popup`.
    BugmeterPopup { bw: u32, url: Option<DilloUrl> },
    /// `a_UIcmd_save_link`.
    SaveLink {
        bw: u32,
        url: DilloUrl,
        filename: Option<Vec<u8>>,
    },
    /// `a_UIcmd_stop`.
    Stop { bw: u32 },
    /// `a_Nav_push` (depuis un seam capi — redirection HTTP).
    NavPush {
        bw: u32,
        url: DilloUrl,
        requester: Option<DilloUrl>,
    },
    /// `a_Nav_reload` (seam capi).
    NavReload { bw: u32 },
    /// `a_Nav_cancel_expect_if_eq` (seam capi).
    NavCancelExpectIfEq { bw: u32, url: DilloUrl },
    /// `a_Nav_expect_done` (dispatch racine verbatim).
    ExpectDone { bw: u32 },
    /// `a_Dpiapi_dialog`.
    DpiDialog {
        bw: u32,
        server: String,
        msg: String,
    },
    /// `a_Bookmarks_chat_add`.
    ChatAdd { msg: String },
    /// `a_UIcmd_reload_all_active` (ctl `reload`).
    ReloadAll,
    /// `a_UIcmd_close_all_bw(force)` (ctl `quit`).
    CloseAll,
    /// `a_UIcmd_by_name` (ctl `cmd`).
    ByName { bw: u32, name: String },
}

impl std::fmt::Debug for GlueOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::SetMsg { .. } => "SetMsg",
            Self::SetPageTitle { .. } => "SetPageTitle",
            Self::SetBugProg { .. } => "SetBugProg",
            Self::SetButtonsSens { .. } => "SetButtonsSens",
            Self::SetLocationText { .. } => "SetLocationText",
            Self::SetPageProg { .. } => "SetPageProg",
            Self::SetImgProg { .. } => "SetImgProg",
            Self::FinishLoading { .. } => "FinishLoading",
            Self::OpenUrl { .. } => "OpenUrl",
            Self::OpenUrlStr { .. } => "OpenUrlStr",
            Self::OpenUrlNw { .. } => "OpenUrlNw",
            Self::OpenUrlNt { .. } => "OpenUrlNt",
            Self::Redirection0 { .. } => "Redirection0",
            Self::Repush { .. } => "Repush",
            Self::PagePopup { .. } => "PagePopup",
            Self::LinkPopup { .. } => "LinkPopup",
            Self::ImagePopup { .. } => "ImagePopup",
            Self::FormPopup { .. } => "FormPopup",
            Self::BugmeterPopup { .. } => "BugmeterPopup",
            Self::SaveLink { .. } => "SaveLink",
            Self::Stop { .. } => "Stop",
            Self::NavPush { .. } => "NavPush",
            Self::NavReload { .. } => "NavReload",
            Self::NavCancelExpectIfEq { .. } => "NavCancelExpectIfEq",
            Self::ExpectDone { .. } => "ExpectDone",
            Self::DpiDialog { .. } => "DpiDialog",
            Self::ChatAdd { .. } => "ChatAdd",
            Self::ReloadAll => "ReloadAll",
            Self::CloseAll => "CloseAll",
            Self::ByName { .. } => "ByName",
        };
        f.write_str(name)
    }
}

// =====================================================================
// Seams capi/cache — lectures directes (cellules), écritures via file.
// =====================================================================

/// `a_Bw_*` côté cache (`BrowserOps`) — le registre est partagé `Rc`.
pub struct DocBrowserOps {
    /// Registre partagé (≙ `bws` global).
    reg: Rc<BwRegistry>,
    /// File d'ops UI.
    queue: IoQueue,
}

impl DocBrowserOps {
    /// Construit les seams navigateur.
    #[must_use]
    pub fn new(reg: Rc<BwRegistry>, queue: IoQueue) -> Self {
        Self { reg, queue }
    }
}

/// `BwUi` minimal émettant dans la file (≙ `a_UIcmd_*` appelés par
/// `a_Bw_close_client`/`a_Bw_add_client` pendant un callback cache).
pub struct QueuedUi {
    /// File d'ops.
    queue: IoQueue,
}

impl BwUi for QueuedUi {
    fn set_img_prog(&mut self, bw: u32, got: i32, total: i32, force: bool) {
        self.queue.borrow_mut().push_back(GlueOp::SetImgProg {
            bw,
            got,
            total,
            cmd: i32::from(force),
        });
    }
    fn set_buttons_sens(&mut self, bw: u32) {
        self.queue
            .borrow_mut()
            .push_back(GlueOp::SetButtonsSens { bw });
    }
    fn finish_loading(&mut self, bw: u32) {
        self.queue
            .borrow_mut()
            .push_back(GlueOp::FinishLoading { bw });
    }
}

impl BrowserOps for DocBrowserOps {
    /// `a_Bw_remove_client` — cellules directes (verbatim).
    fn remove_client(&mut self, bw: u32, key: i32) {
        if let Some(b) = self.reg.get(bw) {
            b.remove_client(key);
        }
    }
    /// `a_Bw_close_client`.
    fn close_client(&mut self, bw: u32, key: i32) {
        if let Some(b) = self.reg.get(bw) {
            let mut ui = QueuedUi {
                queue: self.queue.clone(),
            };
            b.close_client(bw, key, &mut ui);
        }
    }
    /// `bw->redirect_level`.
    fn redirect_level(&mut self, bw: u32) -> i32 {
        self.reg.get(bw).map_or(0, |b| b.redirect_level.get())
    }
    /// `bw->redirect_level = level`.
    fn set_redirect_level(&mut self, bw: u32, level: i32) {
        if let Some(b) = self.reg.get(bw) {
            b.redirect_level.set(level);
        }
    }
}

/// `a_UIcmd_set_msg`/`set_page_prog`/`save_link` côté capi — tous
/// différés (le shell n'est pas joignable depuis la pompe réseau).
pub struct DocUiOps {
    queue: IoQueue,
}

impl DocUiOps {
    /// Construit le seam.
    #[must_use]
    pub fn new(queue: IoQueue) -> Self {
        Self { queue }
    }
}

impl UiOps for DocUiOps {
    fn set_msg(&mut self, bw: u32, msg: &str) {
        self.queue.borrow_mut().push_back(GlueOp::SetMsg {
            bw,
            msg: msg.to_string(),
        });
    }
    fn set_page_prog(&mut self, bw: u32, bytes: usize, cmd: i32) {
        self.queue
            .borrow_mut()
            .push_back(GlueOp::SetPageProg { bw, bytes, cmd });
    }
    fn save_link(&mut self, bw: u32, url: &DilloUrl, filename: Option<&str>) {
        self.queue.borrow_mut().push_back(GlueOp::SaveLink {
            bw,
            url: url.clone(),
            filename: filename.map(|f| f.as_bytes().to_vec()),
        });
    }
}

/// `a_Nav_*` côté capi — les lectures (`stack_size`/`top_url`) passent
/// par les snapshots maintenus à chaque `set_buttons_sens` (verbatim :
/// `bw->nav` est lisible librement amont ; ici `nav.borrow_mut()` peut
/// être détenu par `push` → `open_url` → `capi` → ce seam).
pub struct DocNavOps {
    reg: Rc<BwRegistry>,
    queue: IoQueue,
}

impl DocNavOps {
    /// Construit le seam.
    #[must_use]
    pub fn new(reg: Rc<BwRegistry>, queue: IoQueue) -> Self {
        Self { reg, queue }
    }
}

impl NavOps for DocNavOps {
    /// `a_Nav_push` — différée au tick (verbatim : pendant un
    /// callback cache, amont empile immédiatement ; la file assure
    /// la même sémantique à un tour de boucle près — noté PORTING).
    fn push(&mut self, bw: u32, url: DilloUrl, requester: &DilloUrl) {
        self.queue.borrow_mut().push_back(GlueOp::NavPush {
            bw,
            url,
            requester: Some(requester.clone()),
        });
    }
    /// `a_Nav_reload`.
    fn reload(&mut self, bw: u32) {
        self.queue.borrow_mut().push_back(GlueOp::NavReload { bw });
    }
    /// `a_Nav_cancel_expect_if_eq`.
    fn cancel_expect_if_eq(&mut self, bw: u32, url: &DilloUrl) {
        self.queue
            .borrow_mut()
            .push_back(GlueOp::NavCancelExpectIfEq {
                bw,
                url: url.clone(),
            });
    }
    /// `a_Nav_stack_size` — snapshot (cf. [`BrowserWindow::nav_size_snap`]).
    fn stack_size(&mut self, bw: u32) -> i32 {
        self.reg.get(bw).map_or(0, |b| b.nav_size_snap.get())
    }
    /// `a_Nav_top_url` — snapshot.
    fn top_url(&mut self, bw: u32) -> Option<DilloUrl> {
        self.reg
            .get(bw)
            .and_then(|b| b.nav_top_snap.borrow().clone())
    }
}

/// `a_Dpiapi_*`/bookmarks côté capi.
pub struct DocDpiOps {
    queue: IoQueue,
}

impl DocDpiOps {
    /// Construit le seam.
    #[must_use]
    pub fn new(queue: IoQueue) -> Self {
        Self { queue }
    }
}

impl DpiOps for DocDpiOps {
    fn dialog(&mut self, bw: u32, server: &str, msg: &str) {
        self.queue.borrow_mut().push_back(GlueOp::DpiDialog {
            bw,
            server: server.to_string(),
            msg: msg.to_string(),
        });
    }
    fn chat_add(&mut self, msg: &str) {
        self.queue.borrow_mut().push_back(GlueOp::ChatAdd {
            msg: msg.to_string(),
        });
    }
}

// =====================================================================
// DocSink — `a_UIcmd_*`/`a_Bw_*`/`a_Dialog_*`/`a_Misc_file2dstr` pour
// un document (verbatim html_common.hh `sink` : BrowserWindow*).
// =====================================================================

/// Sink document ↔ bw — un par `HtmlSink` requis (page racine).
/// `Rc<RefCell<DocSink>>` ≙ `BrowserWindow*` + globals amont.
pub struct DocSink {
    /// `bw` verbatim (référence directe — cellules intérieures).
    b: Rc<BrowserWindow>,
    /// `bw_id` (≙ `Web->bw`).
    bw: u32,
    /// File d'ops UI.
    queue: IoQueue,
    /// Runner de dialogs modaux (≙ `a_Dialog_*`/`fl_*` amont —
    /// `RefCell` : `&mut self` du sink n'atteint pas le runner
    /// partagé entre sinks).
    dialogs: Rc<RefCell<dyn DialogRunner>>,
    /// Documents enregistrés (`a_Bw_add_doc`/`remove_doc` — amont
    /// identifie par pointeur `DilloHtml*` ; ici ids locaux).
    docs: RefCell<Vec<usize>>,
    /// Compteur d'ids docs (≙ unicité des pointeurs amont).
    next_doc: Cell<usize>,
}

impl DocSink {
    /// Crée le sink pour `bw_id` (`b` résolu via `reg`).
    #[must_use]
    pub fn new(
        bw: u32,
        reg: &Rc<BwRegistry>,
        queue: IoQueue,
        dialogs: Rc<RefCell<dyn DialogRunner>>,
    ) -> Self {
        Self {
            b: reg.get(bw).unwrap_or_else(|| Rc::new(BrowserWindow::new())),
            bw,
            queue,
            dialogs,
            docs: RefCell::new(Vec::new()),
            next_doc: Cell::new(0),
        }
    }
}

impl DocSink {
    /// Runner dialog avec garde réentrance (un dialog modal pendant
    /// un callback document ≙ boucle `Fl::wait()` amont — si le
    /// runner est déjà emprunté, la réponse est « annulé » comme
    /// [`AutoCancelRunner`]).
    fn run_dialog(&self, spec: &crate::dialog::DialogSpec) -> DialogAnswer {
        match self.dialogs.try_borrow_mut() {
            Ok(mut d) => d.run(spec),
            Err(_) => AutoCancelRunner.run(spec),
        }
    }
}

impl HtmlSink for DocSink {
    // -- bugs / messages ----------------------------------------------------

    /// `bugMessage` — verbatim (`bw->page_bugs` + compteur + prog).
    fn bug_message(&mut self, msg: &str) {
        {
            let mut pb = self.b.page_bugs.borrow_mut();
            if !pb.is_empty() {
                pb.push('\n');
            }
            pb.push_str(msg);
        }
        let n = self.b.num_page_bugs.get() + 1;
        self.b.num_page_bugs.set(n);
        self.queue
            .borrow_mut()
            .push_back(GlueOp::SetBugProg { bw: self.bw, n });
    }

    /// `a_UIcmd_set_msg`.
    fn set_msg(&mut self, msg: &[u8]) {
        self.queue.borrow_mut().push_back(GlueOp::SetMsg {
            bw: self.bw,
            msg: String::from_utf8_lossy(msg).into_owned(),
        });
    }

    /// `a_UIcmd_set_page_title` + `a_History_set_title_by_url`
    /// (l'appel history est fait au drain — ordre de file ≙
    /// synchronicité amont, cf. `GlueOp::SetPageTitle`).
    fn set_page_title(&mut self, url: &DilloUrl, title: &[u8]) {
        let t = String::from_utf8_lossy(title).into_owned();
        self.queue.borrow_mut().push_back(GlueOp::SetPageTitle {
            bw: self.bw,
            url: url.clone(),
            title: Some(t),
            hist: true,
        });
    }

    // -- navigation ----------------------------------------------------------

    fn open_url(&mut self, url: &DilloUrl) {
        self.queue.borrow_mut().push_back(GlueOp::OpenUrl {
            bw: self.bw,
            url: url.clone(),
        });
    }
    fn open_url_nw(&mut self, url: &DilloUrl) {
        self.queue.borrow_mut().push_back(GlueOp::OpenUrlNw {
            bw: self.bw,
            url: url.clone(),
        });
    }
    fn open_url_nt(&mut self, url: &DilloUrl, focus: bool) {
        self.queue.borrow_mut().push_back(GlueOp::OpenUrlNt {
            bw: self.bw,
            url: url.clone(),
            focus,
        });
    }
    fn redirection0(&mut self, url: DilloUrl) {
        self.queue
            .borrow_mut()
            .push_back(GlueOp::Redirection0 { bw: self.bw, url });
    }
    fn repush(&mut self) {
        self.queue
            .borrow_mut()
            .push_back(GlueOp::Repush { bw: self.bw });
    }

    // -- menus contextuels -----------------------------------------------------

    fn page_popup(&mut self, has_bugs: bool, css_urls: &[DilloUrl]) {
        self.queue.borrow_mut().push_back(GlueOp::PagePopup {
            bw: self.bw,
            has_bugs,
            css_urls: css_urls.to_vec(),
        });
    }
    fn link_popup(&mut self, url: &DilloUrl, page_url: &DilloUrl) {
        self.queue.borrow_mut().push_back(GlueOp::LinkPopup {
            bw: self.bw,
            url: url.clone(),
            page_url: Some(page_url.clone()),
        });
    }
    fn image_popup(
        &mut self,
        url: &DilloUrl,
        loaded: bool,
        page_url: &DilloUrl,
        link: Option<&DilloUrl>,
    ) {
        self.queue.borrow_mut().push_back(GlueOp::ImagePopup {
            bw: self.bw,
            url: url.clone(),
            loaded,
            page_url: page_url.clone(),
            link: link.cloned(),
        });
    }
    fn form_popup(&mut self, page_url: &DilloUrl, form_id: usize, showing_hiddens: bool) {
        self.queue.borrow_mut().push_back(GlueOp::FormPopup {
            bw: self.bw,
            page_url: page_url.clone(),
            form: form_id,
            showing_hiddens,
        });
    }

    // -- état bw / clients -----------------------------------------------------

    fn bw_id(&self) -> u32 {
        self.bw
    }
    fn expecting(&self) -> bool {
        self.b.expecting()
    }
    /// `a_Bw_close_client` — verbatim (`RootClients`/`ImageClients` +
    /// progressions via seam UI file).
    fn close_client(&mut self, key: i32) {
        let mut ui = QueuedUi {
            queue: self.queue.clone(),
        };
        self.b.close_client(self.bw, key, &mut ui);
    }
    /// `a_Bw_add_doc` — id local ≙ `DilloHtml*` amont.
    fn add_doc(&mut self) {
        let id = self.next_doc.get();
        self.next_doc.set(id + 1);
        self.b.add_doc(id);
        self.docs.borrow_mut().push(id);
    }
    /// `a_Bw_remove_doc` — retire le doc du sink (le widget delete
    /// callback du document courant — un doc vivant par sink).
    fn remove_doc(&mut self) {
        if let Some(id) = self.docs.borrow_mut().pop() {
            self.b.remove_doc(id);
        }
    }
    fn num_pending_style_sheets(&self) -> i32 {
        self.b.num_pending_style_sheets.get()
    }
    fn set_num_pending_style_sheets(&mut self, n: i32) {
        self.b.num_pending_style_sheets.set(n);
    }
    fn zoom(&self) -> f64 {
        f64::from(self.b.zoom.get())
    }

    // -- chargements ------------------------------------------------------------

    /// `Html_load_image` (html.cc:2202) — verbatim :
    /// `a_Web_new` + `Web->Image = image` (le `DilloImage` dicache
    /// construit sur le renderer du widget ≙ `image->dw` amont) +
    /// `a_Image_ref` + `WEB_Image` + `a_Capi_open_url` +
    /// `a_Bw_add_client`/`add_url`.
    fn load_image(
        &mut self,
        url: &DilloUrl,
        requester: Option<&DilloUrl>,
        image: &ImageHandle,
    ) -> bool {
        let (rndr, dpi, bg, fg) = {
            let im = image.borrow();
            let mut lay = im.layout.borrow_mut();
            let rndr: Rc<RefCell<dyn ImgRenderer>> = lay.image_img_renderer(im.widget);
            let dpi = lay.dpi_x() as f32;
            (rndr, dpi, im.bg_color, im.fg_color)
        };
        let di = dillo_images::image::image_new(0, rndr, bg, fg, dpi);
        let mut w = web::web_new(url.clone(), requester.cloned());
        w.bw = self.bw;
        *w.image.borrow_mut() = Some(di);
        w.flags |= WEB_IMAGE;
        let key = capi::capi_open_url(Rc::new(w), None);
        if key != 0 {
            let mut ui = QueuedUi {
                queue: self.queue.clone(),
            };
            self.b.add_client(self.bw, key, false, &mut ui);
            self.b.add_url(url);
        }
        key != 0
    }

    /// `a_Html_load_stylesheet` partie réseau (html.cc:3290) +
    /// `Html_css_load_callback` (html.cc:3247) verbatim — `--` compteur
    /// à EOF, `a_UIcmd_repush` quand tout est arrivé.
    fn load_stylesheet(&mut self, url: &DilloUrl, requester: &DilloUrl) -> bool {
        let mut w = web::web_new(url.clone(), Some(requester.clone()));
        w.bw = self.bw;
        w.flags |= WEB_STYLESHEET;
        let b2 = Rc::clone(&self.b);
        let queue = self.queue.clone();
        let bw = self.bw;
        let cb: CaCb = Box::new(move |op, _cl| {
            if op != 0 {
                // `Html_css_load_callback` verbatim.
                let n = b2.num_pending_style_sheets.get() - 1;
                b2.num_pending_style_sheets.set(n);
                if n == 0 {
                    queue.borrow_mut().push_back(GlueOp::Repush { bw });
                }
            }
        });
        let key = capi::capi_open_url(Rc::new(w), Some(cb));
        if key != 0 {
            let mut ui = QueuedUi {
                queue: self.queue.clone(),
            };
            self.b.add_client(self.bw, key, false, &mut ui);
            self.b.add_url(url);
        }
        key != 0
    }

    // -- formulaires / dialogs --------------------------------------------------

    /// `a_UIcmd_select_file` (`fl_file_chooser` SELECTSINGLE).
    fn select_file(&mut self) -> Option<String> {
        let spec = file_spec(DialogKind::SelectFile, "Dillo: Select a File", None, None);
        match self.run_dialog(&spec) {
            DialogAnswer::Path(p) => p,
            _ => None,
        }
    }

    /// `a_Dialog_choice` — retourne l'indice (1-based, 0 ≙ annulé).
    fn dialog_choice(&mut self, title: &str, msg: &str, buttons: &[&str]) -> i32 {
        let Some(spec) = choice_spec(title, msg, buttons) else {
            return 0;
        };
        match self.run_dialog(&spec) {
            DialogAnswer::Choice(i) => i,
            _ => 0,
        }
    }

    /// `a_Misc_file2dstr`.
    fn file_to_string(&mut self, path: &str) -> Option<Vec<u8>> {
        dillo_common::misc::file_to_vec(std::path::Path::new(path)).ok()
    }

    fn as_style_engine_sink(&mut self) -> &mut dyn StyleEngineSink {
        self
    }
}

impl StyleEngineSink for DocSink {
    /// `startBackgroundImageLoad` (styleengine.cc:762) — verbatim :
    /// `a_Image_new(layout, image->getMainImgRenderer(), 0xffffff, 0)`,
    /// `a_Web_new(requester=page_url)`, `WEB_Image`,
    /// `a_Capi_open_url` et `a_Bw_add_client`/`add_url`.
    ///
    /// Déviation notée : `StyleImageDeletionReceiver` non porté — le
    /// client vit jusqu'au `a_Bw_cleanup` (verbatim amont le stoppe
    /// aussi à la destruction du style ; cas rare d'un style détruit
    /// en cours de fetch).
    fn start_background_image_load(
        &mut self,
        img_url: &DilloUrl,
        requester: Option<&DilloUrl>,
        image: &Rc<RefCell<style::StyleImage>>,
    ) {
        let rndr = image.borrow().get_main_img_renderer();
        let dpi = self
            .b
            .layout
            .borrow()
            .as_ref()
            .map_or(96.0, |l| l.borrow_mut().dpi_x() as f32);
        let di = dillo_images::image::image_new(0, rndr, 0x00ff_ffff, 0, dpi);
        let mut w = web::web_new(img_url.clone(), requester.cloned());
        w.bw = self.bw;
        *w.image.borrow_mut() = Some(di);
        w.flags |= WEB_IMAGE;
        let key = capi::capi_open_url(Rc::new(w), None);
        if key != 0 {
            let mut ui = QueuedUi {
                queue: self.queue.clone(),
            };
            self.b.add_client(self.bw, key, false, &mut ui);
            self.b.add_url(img_url);
        }
    }
}

// =====================================================================
// DocDispatch — `a_Web_dispatch_by_type` partie navigateur.
// =====================================================================

/// Contexte partagé capturé par les `MimeViewer` enregistrés
/// (≙ les globaux `prefs`/`bws` amont).
pub struct DocCtx {
    /// Registre partagé.
    reg: Rc<BwRegistry>,
    /// File d'ops UI.
    queue: IoQueue,
    /// Prefs partagées (`prefs.bg_color` verbatim).
    prefs: Rc<RefCell<Prefs>>,
    /// `HtmlPrefs` gelé à l'init (≙ `prefs` global lu par html/plain).
    hprefs: Rc<HtmlPrefs>,
    /// `&'static` ≙ `prefs` global amont (`Box::leak` — fuite
    /// unique à l'init, le global amont ne meurt jamais non plus).
    hprefs_ref: &'static HtmlPrefs,
    /// Runner de dialogs partagé avec les sinks.
    dialogs: Rc<RefCell<dyn DialogRunner>>,
}

impl DocCtx {
    /// Crée le contexte.
    #[must_use]
    pub fn new(
        reg: Rc<BwRegistry>,
        queue: IoQueue,
        prefs: Rc<RefCell<Prefs>>,
        hprefs: Rc<HtmlPrefs>,
        dialogs: Rc<RefCell<dyn DialogRunner>>,
    ) -> Rc<Self> {
        let hprefs_ref = Box::leak(Box::new((*hprefs).clone()));
        Rc::new(Self {
            reg,
            queue,
            prefs,
            hprefs,
            hprefs_ref,
            dialogs,
        })
    }

    /// `Rc<DocSink>` pour `web.bw` (verbatim : les viewers amont
    /// reçoivent `web->bw`).
    fn sink_for(&self, web: &Rc<HttpWeb>) -> Rc<RefCell<DocSink>> {
        Rc::new(RefCell::new(DocSink::new(
            web.bw,
            &self.reg,
            self.queue.clone(),
            self.dialogs.clone(),
        )))
    }
}

/// `a_Html_text` (html.cc:211) + `Html_callback` (html.cc:4325) —
/// verbatim : `*Data = html`, `*Call = Html_callback`, retour `dw`.
fn html_viewer(ctx: Rc<DocCtx>) -> MimeViewer {
    Rc::new(move |mime: &str, web: &Rc<HttpWeb>| {
        let sink = ctx.sink_for(web);
        let layout = ctx.reg.get(web.bw)?.layout.borrow().clone()?;
        let content_type = mime.as_bytes().to_vec();
        let html = Rc::new(RefCell::new(DilloHtml::new(
            layout,
            sink as Rc<RefCell<dyn HtmlSink>>,
            &web.url,
            &content_type,
            ctx.hprefs_ref,
            ctx.hprefs.clone(),
        )));
        let dw = html.borrow().dw()?;
        Some(mime::ViewerOutcome {
            call: Box::new(move |op, cl: &mut CacheClient| {
                // `Html_callback` verbatim — `Client->Buf` cumulatif.
                html.borrow_mut().write(&Rc::new(cl.buf.clone()), op != 0);
                if op != 0 {
                    html.borrow_mut().finish_parsing(cl.key);
                }
            }),
            view: Some(Box::new(dw)),
        })
    })
}

/// `a_Plain_text` (plain.cc:89) + `Plain_callback` (plain.cc:218).
fn plain_viewer(ctx: Rc<DocCtx>) -> MimeViewer {
    Rc::new(move |mime: &str, web: &Rc<HttpWeb>| {
        let _ = mime;
        let sink = ctx.sink_for(web);
        let layout = ctx.reg.get(web.bw)?.layout.borrow().clone()?;
        let plain = Rc::new(RefCell::new(DilloPlain::new(
            sink as Rc<RefCell<dyn HtmlSink>>,
            layout,
            &ctx.hprefs,
        )));
        let dw = plain.borrow().dw;
        Some(mime::ViewerOutcome {
            call: Box::new(move |op, cl: &mut CacheClient| {
                // `Plain_callback` verbatim.
                plain.borrow_mut().write(&cl.buf, op != 0);
            }),
            view: Some(Box::new(dw)),
        })
    })
}

/// `a_Image_new_with_dw` (image.cc:60) — factory dicache : crée le
/// widget `dw::Image` (`layout.image_new(alt, mark_unloaded)` ≙
/// `new dw::Image(alt_text, prefs.mark_unloaded_images)`) et rend
/// son `ImgRenderer` + la dpi du layout (≙ `a_Image_new` amont —
/// le `DilloImage` est construit par `dicache_image`).
pub struct DocImageFactory {
    /// Contexte (reg + prefs).
    ctx: Rc<DocCtx>,
}

impl DocImageFactory {
    /// Crée la factory.
    #[must_use]
    pub fn new(ctx: Rc<DocCtx>) -> Self {
        Self { ctx }
    }
}

/// Renderer poubelle (≙ `NULL` amont quand `bw` est mort — dicache
/// écrit dans le vide sans paniquer).
struct NullDocRenderer;
impl ImgRenderer for NullDocRenderer {
    fn set_buffer(&mut self, _buffer: &Rc<dyn dillo_dw::imgbuf::Imgbuf>, _resize: bool) {}
    fn draw_row(&mut self, _row: i32) {}
    fn finish(&mut self) {}
    fn fatal(&mut self) {}
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Renderer nul pour un `bw` mort (≙ `NULL` amont).
fn null_doc_renderer() -> (Rc<RefCell<dyn ImgRenderer>>, f32) {
    (Rc::new(RefCell::new(NullDocRenderer)), 96.0)
}

impl dillo_images::dicache::ImageFactory for DocImageFactory {
    fn new_image(&mut self, web: &HttpWeb) -> (Rc<RefCell<dyn ImgRenderer>>, f32) {
        let mark = self.ctx.prefs.borrow().mark_unloaded_images;
        let Some(b) = self.ctx.reg.get(web.bw) else {
            return null_doc_renderer();
        };
        let Some(layout) = b.layout.borrow().clone() else {
            return null_doc_renderer();
        };
        let mut lay = layout.borrow_mut();
        let w = lay.image_new(None, mark);
        let dpi = lay.dpi_x() as f32;
        (lay.image_img_renderer(w), dpi)
    }
}

/// `a_Mime_init` — viewers texte (les `image/*` sont enregistrés par
/// `dillo_images::dicache::register_image_viewers`).
pub fn register_doc_viewers(ctx: &Rc<DocCtx>) {
    // Verbatim `a_Mime_init` (IO/mime.c:97) — types texte.
    mime::mime_add_minor_type("text/html", html_viewer(ctx.clone()));
    mime::mime_add_minor_type("application/xhtml+xml", html_viewer(ctx.clone()));
    mime::mime_add_minor_type("application/xml", plain_viewer(ctx.clone()));
    mime::mime_add_minor_type("application/rss+xml", plain_viewer(ctx.clone()));
    mime::mime_add_minor_type("application/atom+xml", plain_viewer(ctx.clone()));
    mime::mime_add_minor_type("application/json", plain_viewer(ctx.clone()));
    mime::mime_add_major_type("text", plain_viewer(ctx.clone()));
}

/// Dispatch MIME → viewer, verbatim `a_Web_dispatch_by_type` pour la
/// partie navigateur (le filtre non-root `image/*` vit dans
/// [`dillo_net::web::dispatch_by_type`]).
pub struct DocDispatch {
    /// Contexte partagé avec les `MimeViewer`.
    ctx: Rc<DocCtx>,
}

impl DocDispatch {
    /// Crée le dispatch navigateur.
    #[must_use]
    pub fn new(ctx: Rc<DocCtx>) -> Self {
        Self { ctx }
    }
}

impl WebDispatch for DocDispatch {
    /// `a_Web_dispatch_by_type` — verbatim web.cc:50-115 :
    /// root → fond/style/titre/location/progs/`expect_done` +
    /// `setWidget` ; enfant → viewer direct.
    fn dispatch(&mut self, mime: &str, web: &Rc<HttpWeb>) -> Option<CaCb> {
        let b = self.ctx.reg.get(web.bw)?;
        let layout = b.layout.borrow().clone()?;
        let viewer = mime::mime_get_viewer(mime)?;

        if web.flags & WEB_ROOT_URL != 0 {
            // `-- RootUrl` — verbatim setup.
            let prefs_bg = self.ctx.prefs.borrow().bg_color;
            let (bgc, style) = {
                let mut lay = layout.borrow_mut();
                let bg = Color::new(prefs_bg);
                lay.set_bg_color(Some(bg));
                lay.set_bg_image(
                    None,
                    style::BackgroundRepeat::Repeat,
                    style::BackgroundAttachment::Scroll,
                    style::create_per_length(0.0),
                    style::create_per_length(0.0),
                );
                // `StyleEngine (layout, web->url, web->url, bw->zoom)`
                // + `startElement("body", bw)` + `style(bw)`.
                let mut ti = NullTagIndex;
                let zoom = f64::from(b.zoom.get());
                let mut se = StyleEngine::new(
                    &mut lay,
                    Some(web.url.clone()),
                    Some(web.url.clone()),
                    zoom,
                    &self.ctx.hprefs.style,
                    &mut ti,
                );
                se.start_element(*dillo_html::tags::I_BODY, &mut lay, None);
                (bg.get_color(), se.word_style(&mut lay, None))
            };
            web.bg_color.set(bgc);

            // `dw = viewer(Type, Web, Call, Data)` — verbatim.
            let outcome = viewer(mime, web)?;
            let view = outcome.view?;
            let dw = view.downcast::<WidgetId>().ok().map(|w| *w)?;
            {
                let mut lay = layout.borrow_mut();
                if let Some(st) = &style {
                    lay.widget_set_style(dw, st);
                }
                lay.set_widget(dw);
            }

            // Titre = basename (`strrchr('/')`), puis location/progs.
            let mut u = web.url.clone();
            let s = u.url_str().to_vec();
            let base = s.iter().rposition(|&c| c == b'/').map_or(0, |p| p + 1);
            let title = String::from_utf8_lossy(&s[base..]).into_owned();
            let mut q = self.ctx.queue.borrow_mut();
            q.push_back(GlueOp::SetPageTitle {
                bw: web.bw,
                url: u,
                title: Some(title),
                hist: false,
            });
            q.push_back(GlueOp::SetLocationText {
                bw: web.bw,
                text: s,
            });
            q.push_back(GlueOp::SetPageProg {
                bw: web.bw,
                bytes: 0,
                cmd: 2,
            });
            q.push_back(GlueOp::SetImgProg {
                bw: web.bw,
                got: 0,
                total: 0,
                cmd: 2,
            });
            q.push_back(GlueOp::SetBugProg { bw: web.bw, n: 0 });
            q.push_back(GlueOp::ExpectDone { bw: web.bw });
            Some(outcome.call)
        } else {
            // Enfant non-racine : `image/*` seulement (le filtre
            // `dispatch_by_type` a déjà rejeté le reste).
            let outcome = viewer(mime, web)?;
            Some(outcome.call)
        }
    }
}
