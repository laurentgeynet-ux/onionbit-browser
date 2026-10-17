//! Port de `src/capi.c`/`capi.h` — broker cache↔CCC : dispatch des
//! requêtes par schéma (http/dpi/about/download) et gestion des
//! connexions actives.
//!
//! `capi_conn_t` amont → `CapiConn` partagé en `Rc<RefCell<…>>` (≙ le
//! `Ref` amont : un clone par branche CCC + un dans le registre). La
//! `Chain` de la connexion vit dans le même `RefCell` ; les callbacks
//! CCC peuvent ré-entrer (`conn_abort_by_url` depuis `process_queue`) —
//! un abort pendant une opération en vol est **différé** et drainé à la
//! sortie du CCC de plus haut niveau (`pending_abort`), là où amont
//! appelle `a_Capi_ccc` en direct.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::cache::{
    self, cache_download_enabled, cache_entry_remove_by_url, cache_get_buf, cache_get_flags,
    cache_get_flags_with_redirection, cache_open_url, cache_stop_client, process_dbuf, CaCb,
    NavOps, UiOps,
};
use crate::chain::{
    Chain, ChainCb, ChainData, DataBuf, LinkId, BCK, FWD, OP_ABORT, OP_END, OP_SEND, OP_START,
};
use crate::domain::domain_permit;
use crate::dpip;
use crate::http::{http_ccb, HttpWeb, WEB_DOWNLOAD, WEB_ROOT_URL};
use crate::io::{IO_ABORT, IO_CLOSE, IO_READ};
use crate::url::{flags as url_flags, DilloUrl};

/// `CAPI_IsCached`.
pub const CAPI_IS_CACHED: u32 = 0x1;
/// `CAPI_IsEmpty`.
pub const CAPI_IS_EMPTY: u32 = 0x2;
/// `CAPI_InProgress`.
pub const CAPI_IN_PROGRESS: u32 = 0x4;
/// `CAPI_Aborted`.
pub const CAPI_ABORTED: u32 = 0x8;
/// `CAPI_Completed`.
pub const CAPI_COMPLETED: u32 = 0x10;

const PENDING: i32 = 1;
const ABORTED: i32 = 4;

/// `capi_conn_t` — une connexion (http ou serveur dpi).
struct CapiConn {
    /// `url` — copie locale de `web->url` (`NULL` possible amont).
    url: Option<DilloUrl>,
    /// `bw`.
    bw: u32,
    /// `server` — `"http"`/`"https"` ou nom du serveur dpi.
    server: String,
    /// `datastr` — commande initiale en attente (dpi PENDING).
    datastr: Option<String>,
    /// `SockFD`.
    sock_fd: i32,
    /// `Flags` — `PENDING`/`ABORTED`.
    flags: i32,
    /// `InfoSend` — lien CCC branche 1.
    info_send: Option<LinkId>,
    /// `InfoRecv` — lien CCC branche 2.
    info_recv: Option<LinkId>,
    /// `Ref` ≙ compte de `Rc` — gardé pour la sémantique
    /// `CapiConns` (membre tant qu'une branche vit).
    refs: i32,
    /// Arène CCC de la connexion (les deux branches y vivent).
    chain: Rc<RefCell<Chain>>,
    /// Profondeur d'appels CCC en cours (drain `pending_abort`).
    depth: Cell<usize>,
    /// Abort demandé pendant une opération en vol.
    pending_abort: Cell<bool>,
}

impl CapiConn {
    /// `Capi_conn_new` — `datastr`/`server` dupliqués, `PENDING` pour les
    /// serveurs non-http (attente du démarrage de dpid amont).
    fn new(url: Option<&DilloUrl>, bw: u32, server: &str, datastr: &str) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            url: url.cloned(),
            bw,
            server: server.to_string(),
            datastr: Some(datastr.to_string()),
            sock_fd: -1,
            flags: if server == "http" { 0 } else { PENDING },
            info_send: None,
            info_recv: None,
            refs: 0,
            chain: Rc::new(RefCell::new(Chain::new())),
            depth: Cell::new(0),
            pending_abort: Cell::new(false),
        }))
    }
}

/// Seam `a_Dpiapi_dialog` + `a_Bookmarks_chat_add` (phase DPI — muet par
/// défaut).
pub trait DpiOps {
    /// `a_Dpiapi_dialog` — message d'un serveur dpi affiché en dialog.
    fn dialog(&mut self, bw: u32, server: &str, msg: &str);
    /// `a_Bookmarks_chat_add`.
    fn chat_add(&mut self, msg: &str);
}

/// Impl muette de [`DpiOps`].
#[derive(Default)]
pub struct NullDpiOps;
impl DpiOps for NullDpiOps {
    fn dialog(&mut self, _bw: u32, _server: &str, _msg: &str) {}
    fn chat_add(&mut self, _msg: &str) {}
}

/// `prefs` lus par `capi.c`.
pub struct CapiPrefs {
    /// `prefs.http_user_agent` (commande `downloads` dpi).
    pub http_user_agent: String,
}

/// État global ≙ les `static` de capi.c.
pub struct Capi {
    /// `CapiConns` — connexions actives (≙ `Ref ≥ 1` amont).
    conns: Vec<Rc<RefCell<CapiConn>>>,
    /// `CapiVsUrl` — dernière URL « view source ».
    vs_url: Option<DilloUrl>,
    /// `a_Dpi_ccc` injecté (phase DPI — `None` = pas de serveur dpi :
    /// les requêtes dpi sont refusées proprement).
    dpi_ccc: Option<ChainCb>,
    /// Installateur des moteurs de chaîne (`IoEngine`/`HttpEngine`/
    /// `Dns`/`TlsEngine`) ≙ les `a_*_init` amont — injecté par le
    /// binaire (`a_Capi_init` côté process, runtime tokio global).
    engines: Option<EngineFactory>,
    /// Seams navigateur (mêmes traits que `cache`).
    ui: Box<dyn UiOps>,
    /// Seam nav.
    nav: Box<dyn NavOps>,
    /// Seam `dpiapi`/bookmarks.
    dpi: Box<dyn DpiOps>,
    /// `prefs` lus par capi.c.
    prefs: CapiPrefs,
    /// Sink `MSG`.
    log: Box<dyn FnMut(&str)>,
}

thread_local! {
    static CAPI: RefCell<Capi> = RefCell::new(Capi::new());
}

impl Capi {
    fn new() -> Self {
        Self {
            conns: Vec::new(),
            vs_url: None,
            dpi_ccc: None,
            engines: None,
            ui: Box::new(cache::NullUiOps),
            nav: Box::new(cache::NullNavOps),
            dpi: Box::new(NullDpiOps),
            prefs: CapiPrefs {
                http_user_agent: "Dillo/3.3.0-rc1".to_string(),
            },
            log: Box::new(|_| {}),
        }
    }

    /// `Capi_conn_valid` — présent dans `CapiConns` ?
    fn conn_valid(&self, conn: &Rc<RefCell<CapiConn>>) -> bool {
        self.conns.iter().any(|c| Rc::ptr_eq(c, conn))
    }

    /// `Capi_conn_find` — recherche par `server`.
    fn conn_find(&self, server: &str) -> Option<Rc<RefCell<CapiConn>>> {
        self.conns
            .iter()
            .find(|c| c.borrow().server == server)
            .cloned()
    }
}

/// `Capi_conn_ref` — ajoute au registre à la première référence.
fn conn_ref(conn: &Rc<RefCell<CapiConn>>) {
    let mut c = conn.borrow_mut();
    c.refs += 1;
    if c.refs == 1 {
        drop(c);
        CAPI.with(|capi| capi.borrow_mut().conns.push(conn.clone()));
    }
}

/// `Capi_conn_unref` — retire du registre à zéro référence.
fn conn_unref(conn: &Rc<RefCell<CapiConn>>) {
    let mut c = conn.borrow_mut();
    c.refs -= 1;
    if c.refs == 0 {
        drop(c);
        CAPI.with(|capi| {
            capi.borrow_mut().conns.retain(|x| !Rc::ptr_eq(x, conn));
        });
    }
}

/// Injecte les seams navigateur (mêmes objets que `cache_set_seams`).
pub fn capi_set_seams(ui: Box<dyn UiOps>, nav: Box<dyn NavOps>, dpi: Box<dyn DpiOps>) {
    CAPI.with(|c| {
        let mut c = c.borrow_mut();
        c.ui = ui;
        c.nav = nav;
        c.dpi = dpi;
    });
}

/// Factory de moteurs réseau — installe `IoEngine`/`Dns`/
/// `HttpEngine`/`TlsEngine` dans la `Chain` d'une connexion.
pub type EngineFactory = Rc<dyn Fn(&mut Chain)>;

/// Injecte `a_Dpi_ccc` (phase DPI).
pub fn capi_set_dpi_ccc(cb: ChainCb) {
    CAPI.with(|c| c.borrow_mut().dpi_ccc = Some(cb));
}

/// `a_Http_init`+`a_IO_init`+`a_Dns_init`+`a_Tls_init` par connexion —
/// le binaire injecte la closure qui installe les moteurs
/// (`IoEngine`/`HttpEngine`/`Dns`/`TlsEngine`) dans la `Chain` de
/// chaque nouvelle `CapiConn` http(s) (≙ les `a_*_init` amont,
/// regroupés car les moteurs partagent le même runtime tokio).
pub fn capi_set_engine_factory(f: EngineFactory) {
    CAPI.with(|c| c.borrow_mut().engines = Some(f));
}

/// Nombre de connexions HTTP(S) vivantes (≙ taille de `CapiConns`
/// amont) — lu par la boucle pour décider `WaitUntil` vs `Wait`.
#[must_use]
pub fn capi_conns_active() -> usize {
    CAPI.with(|c| c.borrow().conns.len())
}

/// Passe de pompe réseau ≙ les callbacks `a_IOwatch` + timers
/// HTTP/DNS de la boucle FLTK amont : sur la chaîne de chaque
/// connexion vivante, draine les événements du reactor
/// ([`crate::io::poll_io`]), les réponses DNS
/// ([`crate::dns::poll_dns`]) et la machine à états HTTP
/// ([`crate::http::poll_http`]). À appeler à chaque tick de la
/// boucle d'événements (shell). Retourne le nombre de connexions
/// en cours — l'appelant en déduit l'intervalle de réveil
/// (`ControlFlow::WaitUntil`).
#[must_use]
pub fn capi_poll() -> usize {
    let conns = CAPI.with(|c| c.borrow().conns.clone());
    for conn in &conns {
        let chain = conn.borrow().chain.clone();
        // `try_borrow_mut` ≙ garde « chaîne occupée » amont
        // (`Cache_process_queue` busy) — une chaîne en CCC plein
        // est reportée à la passe suivante.
        let Ok(mut ch) = chain.try_borrow_mut() else {
            continue;
        };
        crate::io::poll_io(&mut ch);
        crate::dns::poll_dns(&mut ch);
        crate::http::poll_http(&mut ch);
    }
    conns.len()
}

/// Injecte `prefs.http_user_agent`.
pub fn capi_set_user_agent(ua: &str) {
    CAPI.with(|c| {
        c.borrow_mut().prefs.http_user_agent = ua.to_string();
    });
}

/// Injecte le sink `MSG`.
pub fn capi_set_log(log: Box<dyn FnMut(&str)>) {
    CAPI.with(|c| c.borrow_mut().log = log);
}

/// `a_Capi_set_vsource_url`.
pub fn capi_set_vsource_url(url: &DilloUrl) {
    CAPI.with(|c| c.borrow_mut().vs_url = Some(url.clone()));
}

/* --------------------------- le CCC capi --------------------------- */

fn capi_ccc_fn(
    chain: &mut Chain,
    op: i32,
    branch: i32,
    dir: i32,
    info: LinkId,
    d1: ChainData,
    d2: ChainData,
) {
    // Profondeur pour drainer `pending_abort` à la sortie top-level.
    let conn_rc = chain.local_key::<Rc<RefCell<CapiConn>>>(info).cloned();
    if let Some(cr) = &conn_rc {
        let d = cr.borrow().depth.get() + 1;
        cr.borrow().depth.set(d);
    }
    capi_ccc_body(chain, op, branch, dir, info, d1, d2);
    if let Some(cr) = &conn_rc {
        let d = cr.borrow().depth.get().saturating_sub(1);
        cr.borrow().depth.set(d);
        if d == 0 && cr.borrow().pending_abort.get() {
            cr.borrow_mut().pending_abort.set(false);
            conn_abort_inner(chain, cr);
        }
    }
}

/// `a_Capi_conn_abort_by_url` (partie CCC) — `OpAbort` sur les deux
/// branches, dans l'ordre amont (1B puis 2B).
fn conn_abort_inner(chain: &mut Chain, conn: &Rc<RefCell<CapiConn>>) {
    let (is, ir) = {
        let c = conn.borrow();
        (c.info_send, c.info_recv)
    };
    if let Some(i) = is {
        capi_ccc_fn(chain, OP_ABORT, 1, BCK, i, ChainData::None, ChainData::None);
    }
    if let Some(i) = ir {
        capi_ccc_fn(chain, OP_ABORT, 2, BCK, i, ChainData::None, ChainData::None);
    }
}

/// `a_Capi_conn_abort_by_url` — abort synchrone si la `Chain` de la
/// connexion est libre, sinon différé (`pending_abort`, drainé à la
/// sortie du CCC — amont appelle en direct mais le pointer-modèle le
/// permet ; ici le `RefCell` impose le report).
pub fn conn_abort_by_url(url: &DilloUrl) {
    let targets: Vec<Rc<RefCell<CapiConn>>> = CAPI.with(|c| {
        c.borrow()
            .conns
            .iter()
            .filter(|conn| {
                conn.borrow()
                    .url
                    .as_ref()
                    .is_some_and(|u| u.url_cmp(url) == 0)
            })
            .cloned()
            .collect()
    });
    for conn in targets {
        conn.borrow_mut().flags |= ABORTED;
        abort_or_defer(&conn);
    }
}

/// Abort immédiat si la chaîne est libre, sinon `pending_abort` est
/// drainé à la sortie du CCC de plus haut niveau.
fn abort_or_defer(conn: &Rc<RefCell<CapiConn>>) {
    let chain = conn.borrow().chain.clone();
    if chain.try_borrow_mut().is_ok() {
        let mut ch = chain.borrow_mut();
        conn_abort_inner(&mut ch, conn);
    } else {
        conn.borrow().pending_abort.set(true);
    }
}

/// `Capi_conn_resume` — renvoie les commandes en attente de dpid.
fn conn_resume() {
    let conns = CAPI.with(|c| c.borrow().conns.clone());
    for conn in conns {
        let (datastr, info_send, chain) = {
            let mut c = conn.borrow_mut();
            if c.flags & PENDING == 0 {
                continue;
            }
            c.flags &= !PENDING;
            (c.datastr.clone(), c.info_send, c.chain.clone())
        };
        if let (Some(dstr), Some(i)) = (datastr, info_send) {
            let mut ch = chain.borrow_mut();
            let dbuf = DataBuf::new(dstr.into_bytes(), 0, 0);
            // `dbuf->Size` amont = strlen → len des octets.
            let mut dbuf = dbuf;
            dbuf.size = i32::try_from(dbuf.buf.len()).unwrap_or(i32::MAX);
            capi_ccc_fn(
                &mut ch,
                OP_SEND,
                1,
                BCK,
                i,
                ChainData::Buf(Box::new(dbuf)),
                ChainData::None,
            );
        }
    }
}

/// `a_Capi_ccc` — corps (dispatch `a_Chain_check` ≙ structurelle).
#[allow(clippy::too_many_lines)]
fn capi_ccc_body(
    chain: &mut Chain,
    op: i32,
    branch: i32,
    dir: i32,
    info: LinkId,
    d1: ChainData,
    d2: ChainData,
) {
    match (branch, dir, op) {
        // ---------------- branche 1 — émission (BCK) ----------------
        (1, BCK, OP_START) => {
            // Data1 = conn ; Data2 = {HttpWeb | nom serveur}
            let ChainData::Owned(any) = d1 else { return };
            let Ok(conn) = any.downcast::<RefCell<CapiConn>>() else {
                return;
            };
            conn_ref(&conn);
            chain.set_local_key(info, conn.clone());
            conn.borrow_mut().info_send = Some(info);
            let server = conn.borrow().server.clone();
            let next = if server == "http" || server == "https" {
                Some(http_ccb())
            } else {
                CAPI.with(|c| c.borrow().dpi_ccc.clone())
            };
            if let Some(next_cb) = next {
                chain.link_new(info, capi_ccc(), BCK, next_cb, 1, 1);
                chain.bcb(OP_START, info, d2, ChainData::None);
            } else {
                CAPI.with(|c| {
                    (c.borrow_mut().log)("capi: no dpi CCC available (phase DPI)");
                });
                // Amont : `a_Dpi_ccc` répondrait `OpAbort`+"DpidERROR"
                // (serveur indisponible) — même issue ici.
                capi_ccc_fn(
                    chain,
                    OP_ABORT,
                    1,
                    FWD,
                    info,
                    ChainData::None,
                    ChainData::Str("DpidERROR".to_string()),
                );
            }
        }
        (1, BCK, OP_SEND) => {
            chain.bcb(OP_SEND, info, d1, ChainData::None);
        }
        (1, BCK, op @ (OP_END | OP_ABORT)) => {
            let conn = chain.local_key::<Rc<RefCell<CapiConn>>>(info).cloned();
            if let Some(cr) = conn {
                cr.borrow_mut().info_send = None;
                chain.bcb(op, info, ChainData::None, ChainData::None);
                conn_unref(&cr);
            }
            chain.free_link(info);
        }
        // ------------- branche 1 — statut (FWD) -------------
        (1, FWD, OP_SEND) => {
            match d2.as_str() {
                Some("FD") => {
                    // Relais du descripteur vers la branche réception.
                    let (conn, fd) = {
                        let c = chain.local_key::<Rc<RefCell<CapiConn>>>(info).cloned();
                        let fd = match d1 {
                            ChainData::Int(fd) => fd,
                            _ => -1,
                        };
                        (c, fd)
                    };
                    if let Some(cr) = conn {
                        cr.borrow_mut().sock_fd = fd;
                        let ir = cr.borrow().info_recv;
                        if let Some(i) = ir {
                            capi_ccc_fn(
                                chain,
                                OP_SEND,
                                2,
                                BCK,
                                i,
                                ChainData::Int(fd),
                                ChainData::Str("FD".to_string()),
                            );
                        }
                    }
                }
                Some("DpidOK") => conn_resume(),
                _ => {
                    if matches!(d2, ChainData::None) {
                        CAPI.with(|c| (c.borrow_mut().log)("Capi.c: Opsend [1F] Data2 = NULL"));
                    }
                }
            }
        }
        (1, FWD, OP_ABORT) => {
            let conn = chain.local_key::<Rc<RefCell<CapiConn>>>(info).cloned();
            if let Some(cr) = conn {
                cr.borrow_mut().info_send = None;
                let (url, bw, recv) = {
                    let c = cr.borrow();
                    (c.url.clone(), c.bw, c.info_recv)
                };
                if let Some(u) = &url {
                    process_dbuf(IO_ABORT, &[], u);
                }
                match d2.as_str() {
                    Some("DpidERROR") => {
                        let scheme = url
                            .as_ref()
                            .and_then(|u| u.scheme.clone())
                            .unwrap_or_default();
                        CAPI.with(|c| {
                            c.borrow_mut().ui.set_msg(
                                bw,
                                &format!(
                                    "ERROR: can't start dpid daemon (URL scheme = '{}')!",
                                    String::from_utf8_lossy(&scheme)
                                ),
                            );
                        });
                    }
                    Some("Both") => {
                        if let Some(i) = recv {
                            capi_ccc_fn(
                                chain,
                                OP_ABORT,
                                2,
                                BCK,
                                i,
                                ChainData::None,
                                ChainData::None,
                            );
                        }
                    }
                    _ => {}
                }
                CAPI.with(|c| {
                    let mut c = c.borrow_mut();
                    if let Some(u) = &url {
                        c.nav.cancel_expect_if_eq(bw, u);
                    }
                });
                conn_unref(&cr);
            }
            chain.free_link(info);
        }
        // ---------------- branche 2 — réception (BCK) ----------------
        (2, BCK, OP_START) => {
            let ChainData::Owned(any) = d1 else { return };
            let Ok(conn) = any.downcast::<RefCell<CapiConn>>() else {
                return;
            };
            conn_ref(&conn);
            chain.set_local_key(info, conn.clone());
            conn.borrow_mut().info_recv = Some(info);
            let server = conn.borrow().server.clone();
            let next = if server == "http" {
                Some(http_ccb())
            } else {
                CAPI.with(|c| c.borrow().dpi_ccc.clone())
            };
            if let Some(next_cb) = next {
                chain.link_new(info, capi_ccc(), BCK, next_cb, 2, 2);
                chain.bcb(OP_START, info, ChainData::None, d2);
            } else {
                CAPI.with(|c| (c.borrow_mut().log)("capi: no dpi CCC available"));
                capi_ccc_fn(
                    chain,
                    OP_ABORT,
                    2,
                    FWD,
                    info,
                    ChainData::None,
                    ChainData::None,
                );
            }
        }
        (2, BCK, OP_SEND) => {
            if d2.is_tag("FD") {
                chain.bcb(OP_SEND, info, d1, d2);
            }
        }
        (2, BCK, OP_ABORT) => {
            let conn = chain.local_key::<Rc<RefCell<CapiConn>>>(info).cloned();
            if let Some(cr) = conn {
                cr.borrow_mut().info_recv = None;
                let url = cr.borrow().url.clone();
                chain.bcb(OP_ABORT, info, ChainData::None, ChainData::None);
                if let Some(u) = &url {
                    cache_entry_remove_by_url(u);
                }
                conn_unref(&cr);
            }
            chain.free_link(info);
        }
        // ------------- branche 2 — écoute serveur (FWD) -------------
        (2, FWD, OP_SEND) => {
            let conn = chain.local_key::<Rc<RefCell<CapiConn>>>(info).cloned();
            let Some(cr) = conn else { return };
            match d2.as_str() {
                Some("send_page_2eof") => {
                    let url = cr.borrow().url.clone();
                    if let (Some(u), ChainData::Buf(dbuf)) = (url, d1) {
                        let n = usize::try_from(dbuf.size.max(0)).unwrap_or(0);
                        let data = &dbuf.buf[..n.min(dbuf.buf.len())];
                        let finished = process_dbuf(IO_READ, data, &u);
                        let go = finished
                            && {
                                let c = cr.borrow();
                                c.info_recv.is_some()
                            }
                            && CAPI.with(|capi| capi.borrow().conn_valid(&cr));
                        if go {
                            chain.bcb(
                                OP_SEND,
                                info,
                                ChainData::None,
                                ChainData::Str("reply_complete".to_string()),
                            );
                        }
                    }
                }
                Some("send_status_message" | "chat") => {
                    if let ChainData::Str(msg) = &d1 {
                        let (bw, msg) = (cr.borrow().bw, msg.clone());
                        CAPI.with(|c| c.borrow_mut().ui.set_msg(bw, &msg));
                        if d2.is_tag("chat") {
                            CAPI.with(|c| c.borrow_mut().dpi.chat_add(&msg));
                        }
                    }
                }
                Some("dialog") => {
                    if let ChainData::Str(msg) = &d1 {
                        let (bw, server, msg) =
                            (cr.borrow().bw, cr.borrow().server.clone(), msg.clone());
                        CAPI.with(|c| c.borrow_mut().dpi.dialog(bw, &server, &msg));
                    }
                }
                Some("reload_request") => {
                    let bw = cr.borrow().bw;
                    CAPI.with(|c| c.borrow_mut().nav.reload(bw));
                }
                // `"start_send_page"` : no-op amont (le cache est prêt).
                _ => {}
            }
        }
        (2, FWD, OP_END) => {
            let conn = chain.local_key::<Rc<RefCell<CapiConn>>>(info).cloned();
            if let Some(cr) = conn {
                cr.borrow_mut().info_recv = None;
                let (url, send) = {
                    let c = cr.borrow();
                    (c.url.clone(), c.info_send)
                };
                if let Some(u) = &url {
                    process_dbuf(IO_CLOSE, &[], u);
                }
                if let Some(i) = send {
                    // Propague OpEnd vers la branche émission.
                    capi_ccc_fn(chain, OP_END, 1, BCK, i, ChainData::None, ChainData::None);
                }
                conn_unref(&cr);
            }
            chain.free_link(info);
        }
        (2, FWD, OP_ABORT) => {
            let conn = chain.local_key::<Rc<RefCell<CapiConn>>>(info).cloned();
            if let Some(cr) = conn {
                cr.borrow_mut().info_recv = None;
                let (url, bw, send) = {
                    let c = cr.borrow();
                    (c.url.clone(), c.bw, c.info_send)
                };
                if let Some(u) = &url {
                    process_dbuf(IO_ABORT, &[], u);
                }
                if d2.is_tag("Both") {
                    if let Some(i) = send {
                        capi_ccc_fn(chain, OP_ABORT, 1, BCK, i, ChainData::None, ChainData::None);
                    }
                }
                CAPI.with(|c| {
                    let mut c = c.borrow_mut();
                    if let Some(u) = &url {
                        c.nav.cancel_expect_if_eq(bw, u);
                    }
                });
                conn_unref(&cr);
            }
            chain.free_link(info);
        }
        _ => {
            CAPI.with(|c| (c.borrow_mut().log)("Unused CCC Capi branch"));
        }
    }
}

/// `a_Capi_ccc` — callback CCC enregistré.
#[must_use]
pub fn capi_ccc() -> ChainCb {
    Rc::new(capi_ccc_fn)
}

/* --------------------------- open_url --------------------------- */

/// `Capi_url_uses_dpi` — `Some(server)` si l'URL relève d'un serveur dpi.
fn url_uses_dpi(url: &DilloUrl) -> Option<String> {
    let s = url_str(url);
    if s.starts_with(b"http:") || s.starts_with(b"https:") || s.starts_with(b"about:") {
        return None;
    }
    if s.starts_with(b"dpi:/") {
        let rest = &s[5..];
        let mut server = match rest.iter().position(|&b| b == b'/') {
            Some(p) => String::from_utf8_lossy(&rest[..p]).into_owned(),
            None => "?".to_string(),
        };
        if server == "bm" {
            server = "bookmarks".to_string();
        }
        return Some(server);
    }
    s.iter()
        .position(|&b| b == b':')
        .map(|p| format!("proto.{}", String::from_utf8_lossy(&s[..p])))
}

fn url_str(url: &DilloUrl) -> Vec<u8> {
    let mut u = url.clone();
    u.url_str().to_vec()
}

/// `a_Capi_dpi_verify_request` — GET|POST `dpi:` seulement depuis une
/// page générée par dpi (sauf `/vsource/` GET).
#[must_use]
pub fn capi_dpi_verify_request(bw: u32, url: &DilloUrl) -> bool {
    let mut allow = false;
    if url
        .scheme
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case(b"dpi"))
    {
        let get_only_vsource = url.flags & url_flags::POST == 0
            && url
                .path
                .as_deref()
                .is_some_and(|p| p.starts_with(b"/vsource/"));
        if url.flags & (url_flags::POST | url_flags::GET) == 0 || get_only_vsource {
            allow = true;
        } else {
            // GET&POST seulement depuis des pages dpi.
            let (size, referer) = CAPI.with(|c| {
                let mut c = c.borrow_mut();
                let sz = c.nav.stack_size(bw);
                let r = if sz > 0 { c.nav.top_url(bw) } else { None };
                (sz, r)
            });
            if size > 0 {
                if let Some(r) = referer {
                    if r.scheme
                        .as_deref()
                        .is_some_and(|s| s.eq_ignore_ascii_case(b"dpi"))
                    {
                        allow = true;
                    }
                }
            }
        }
    } else {
        allow = true;
    }
    if !allow {
        CAPI.with(|c| {
            (c.borrow_mut().log)("a_Capi_dpi_verify_request: Permission Denied!");
        });
    }
    allow
}

/// `Capi_dpi_build_cmd` — `downloads` → cmd complète, sinon `open_url`.
fn dpi_build_cmd(web: &HttpWeb, server: &str, user_agent: &str) -> Option<String> {
    let url_s = String::from_utf8_lossy(&url_str(&web.url)).into_owned();
    if server == "downloads" {
        dpip::build_cmd(
            "cmd=%s url=%s destination=%s user-agent=%s",
            &[
                "download",
                &url_s,
                web.filename.as_deref().unwrap_or(""),
                user_agent,
            ],
        )
    } else {
        dpip::build_cmd("cmd=%s url=%s", &["open_url", &url_s])
    }
}

/// `Capi_dpi_send_source` — envoie la source de la page au dpi vsource.
fn dpi_send_source(bw: u32, url: &DilloUrl) {
    // Il faut « scheme:…:… » (deux ':') amont.
    let s = url_str(url);
    let ok = s
        .iter()
        .position(|&b| b == b':')
        .is_some_and(|p| s.iter().skip(p + 1).any(|&b| b == b':'));
    if !ok {
        return;
    }
    let vs = CAPI.with(|c| c.borrow().vs_url.clone());
    if let Some(vs_url) = vs {
        if let Some(buf) = cache_get_buf(&vs_url) {
            let size_str = buf.len().to_string();
            let u_s = String::from_utf8_lossy(&s).into_owned();
            if let Some(cmd) = dpip::build_cmd(
                "cmd=%s url=%s data_size=%s",
                &["start_send_page", &u_s, &size_str],
            ) {
                capi_dpi_send_cmd(None, bw, &cmd, "vsource", 0);
            }
            capi_dpi_send_data(Some(url), bw, &buf, "vsource", 0);
            return;
        }
    }
    if let Some(cmd) = dpip::build_cmd("cmd=%s msg=%s", &["DpiError", "Page is NOT cached"]) {
        capi_dpi_send_cmd(None, bw, &cmd, "vsource", 0);
    }
}

/// `Capi_request_permitted` — bloque le mixed-content (sauf `data:`)
/// et consulte `domainrc` (sauf cache hit).
fn request_permitted(web: &HttpWeb) -> bool {
    let Some(requester) = &web.requester else {
        return true; // action utilisateur
    };
    if web.flags & !WEB_ROOT_URL != 0
        && requester
            .scheme
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case(b"https"))
    {
        // Mixed content : https → non-https/data bloqué.
        let s = &web.url.scheme;
        let insecure = !s
            .as_deref()
            .is_some_and(|x| x.eq_ignore_ascii_case(b"https"))
            && !s
                .as_deref()
                .is_some_and(|x| x.eq_ignore_ascii_case(b"data"));
        if insecure {
            CAPI.with(|c| (c.borrow_mut().log)("capi: Blocked mixed content"));
            return false;
        }
    }
    if cache_get_flags(&web.url) & to_capi_cached() != 0 {
        return true;
    }
    let mut req = requester.clone();
    let mut dst = web.url.clone();
    domain_permit(&mut req, &mut dst)
}

const fn to_capi_cached() -> u32 {
    CAPI_IS_CACHED
}

/// `Capi_map_cache_flags`.
#[must_use]
pub fn map_cache_flags(flags: u32) -> u32 {
    if flags == 0 {
        return 0;
    }
    let mut status = CAPI_IS_CACHED;
    if flags & cache::ca_flags::IS_EMPTY != 0 {
        status |= CAPI_IS_EMPTY;
    }
    if flags & cache::ca_flags::IN_PROGRESS != 0 {
        status |= CAPI_IN_PROGRESS;
    } else {
        status |= CAPI_COMPLETED;
    }
    status
}

/// `a_Capi_get_flags`.
#[must_use]
pub fn capi_get_flags(url: &DilloUrl) -> u32 {
    map_cache_flags(cache_get_flags(url))
}

/// `a_Capi_get_flags_with_redirection`.
#[must_use]
pub fn capi_get_flags_with_redirection(url: &DilloUrl) -> u32 {
    map_cache_flags(cache_get_flags_with_redirection(url))
}

/// `a_Capi_init` — registre + `a_Cache_init`.
pub fn capi_init() {
    CAPI.with(|c| {
        let mut c = c.borrow_mut();
        c.conns.clear();
        c.vs_url = None;
    });
    cache::cache_init();
}

/// `a_Capi_get_buf`.
#[must_use]
pub fn capi_get_buf(url: &DilloUrl) -> Option<Vec<u8>> {
    cache_get_buf(url)
}

/// `a_Capi_unref_buf`.
pub fn capi_unref_buf(url: &DilloUrl) {
    cache::cache_unref_buf(url);
}

/// `a_Capi_get_content_type`.
#[must_use]
pub fn capi_get_content_type(url: &DilloUrl) -> Option<Vec<u8>> {
    cache::cache_get_content_type(url)
}

/// `a_Capi_set_content_type`.
#[must_use]
pub fn capi_set_content_type(url: &DilloUrl, ctype: &[u8], from: &str) -> Option<Vec<u8>> {
    cache::cache_set_content_type(url, ctype, from)
}

/// `a_Capi_open_url` — dispatch par schéma : `WEB_Download`, dpi,
/// http/https (CCC) ou `about`. Retourne la clé client (0 = avorté).
#[allow(clippy::too_many_lines)] // port direct du dispatch amont
pub fn capi_open_url(mut web: Rc<HttpWeb>, call: Option<CaCb>) -> i32 {
    let mut use_cache = false;
    let mut conn: Option<Rc<RefCell<CapiConn>>> = None;

    if request_permitted(&web) {
        let reload = capi_get_flags(&web.url) & CAPI_IS_CACHED == 0
            || web.url.flags & url_flags::E2E_QUERY != 0;

        if web.flags & WEB_DOWNLOAD != 0 {
            if capi_get_flags_with_redirection(&web.url) & CAPI_IS_CACHED != 0 {
                if let Some(fname) = &web.filename {
                    // `web->stream = fopen(filename, "w")` amont.
                    match std::fs::File::create(fname) {
                        Ok(f) => {
                            *web.stream.borrow_mut() = Some(f);
                            use_cache = true;
                        }
                        Err(e) => {
                            CAPI.with(|c| {
                                (c.borrow_mut().log)(&format!(
                                    "Cannot open \"{fname}\" for writing: {e}."
                                ));
                            });
                        }
                    }
                }
            } else if cache_download_enabled(&web.url) {
                let cmd = {
                    let ua = CAPI.with(|c| c.borrow().prefs.http_user_agent.clone());
                    dpi_build_cmd(&web, "downloads", &ua)
                };
                if let Some(cmd) = cmd {
                    capi_dpi_send_cmd(Some(&web.url), web.bw, &cmd, "downloads", 1);
                }
            } else {
                CAPI.with(|c| {
                    (c.borrow_mut().log)(
                        "Ignoring download request: not in cache and not downloadable.",
                    );
                });
            }
        } else if let Some(server) = url_uses_dpi(&web.url) {
            // Requête dpi.
            if capi_dpi_verify_request(web.bw, &web.url) {
                let mut reload = reload;
                if web
                    .url
                    .scheme
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(b"dpi"))
                    && server != "vsource"
                {
                    // `dpi:/` hors vsource : toujours reload
                    // (`a_Url_set_flags(E2EQuery)` amont — mutation de
                    // web->url ; ici web est `Rc` partagé → mutation
                    // uniquement si exclusif, sinon la décision reload
                    // couvre déjà le même effet).
                    if let Some(w) = Rc::get_mut(&mut web) {
                        w.url.flags |= url_flags::E2E_QUERY;
                    }
                    reload = true;
                }
                if reload {
                    conn_abort_by_url(&web.url);
                    if let Some(cmd) = dpi_build_cmd(&web, &server, &{
                        CAPI.with(|c| c.borrow().prefs.http_user_agent.clone())
                    }) {
                        capi_dpi_send_cmd(Some(&web.url), web.bw, &cmd, &server, 1);
                    }
                    if server == "vsource" {
                        dpi_send_source(web.bw, &web.url);
                    }
                }
                use_cache = true;
            }
        } else if web
            .url
            .scheme
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case(b"http"))
            || web
                .url
                .scheme
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(b"https"))
        {
            if reload {
                conn_abort_by_url(&web.url);
                let cr = CapiConn::new(Some(&web.url), web.bw, "http", "none");
                let chain = cr.borrow().chain.clone();
                // Branche réception d'abord (le DNS peut répondre tout
                // de suite — course amont).
                let mut ch = chain.borrow_mut();
                // `a_*_init` amont (globaux) → ici les moteurs sont
                // par-connexion : installés à la création de la chaîne.
                if let Some(f) = CAPI.with(|c| c.borrow().engines.clone()) {
                    f(&mut ch);
                }
                let l2 = ch.new_link();
                capi_ccc_fn(
                    &mut ch,
                    OP_START,
                    2,
                    BCK,
                    l2,
                    ChainData::Owned(cr.clone()),
                    ChainData::Str("http".to_string()),
                );
                let l1 = ch.new_link();
                capi_ccc_fn(
                    &mut ch,
                    OP_START,
                    1,
                    BCK,
                    l1,
                    ChainData::Owned(cr.clone()),
                    ChainData::Owned(web.clone()),
                );
                conn = Some(cr);
            }
            use_cache = true;
        } else if web
            .url
            .scheme
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case(b"about"))
        {
            use_cache = true;
        }
    }

    if use_cache {
        let valid = match &conn {
            None => true,
            Some(cr) => CAPI.with(|c| c.borrow().conn_valid(cr)),
        };
        if valid {
            return cache_open_url(web, call);
        }
        return 0;
    }
    crate::web::web_free(&web);
    0
}

/// `a_Capi_dpi_send_data` — `flags & 1` : nouvelle connexion ; sinon
/// réutilise celle du `server` (retour ignoré amont aussi).
#[allow(clippy::must_use_candidate)]
pub fn capi_dpi_send_data(
    url: Option<&DilloUrl>,
    bw: u32,
    data: &[u8],
    server: &str,
    flags: i32,
) -> i32 {
    if flags & 1 != 0 {
        let datastr = String::from_utf8_lossy(data).into_owned();
        let conn = CapiConn::new(url, bw, server, &datastr);
        let chain = conn.borrow().chain.clone();
        let mut ch = chain.borrow_mut();
        let l2 = ch.new_link();
        capi_ccc_fn(
            &mut ch,
            OP_START,
            2,
            BCK,
            l2,
            ChainData::Owned(conn.clone()),
            ChainData::Str(server.to_string()),
        );
        let l1 = ch.new_link();
        capi_ccc_fn(
            &mut ch,
            OP_START,
            1,
            BCK,
            l1,
            ChainData::Owned(conn.clone()),
            ChainData::Str(server.to_string()),
        );
    } else {
        let found = CAPI.with(|c| c.borrow().conn_find(server));
        match found {
            Some(conn) => {
                let i = conn.borrow().info_send;
                if let Some(i) = i {
                    let chain = conn.borrow().chain.clone();
                    let mut ch = chain.borrow_mut();
                    let mut dbuf = DataBuf::new(data.to_vec(), 0, 0);
                    dbuf.size = i32::try_from(data.len()).unwrap_or(i32::MAX);
                    capi_ccc_fn(
                        &mut ch,
                        OP_SEND,
                        1,
                        BCK,
                        i,
                        ChainData::Buf(Box::new(dbuf)),
                        ChainData::None,
                    );
                } else {
                    CAPI.with(|c| {
                        (c.borrow_mut().log)(
                            "ERROR: [a_Capi_dpi_send_data] No open connection found",
                        );
                    });
                }
            }
            None => {
                CAPI.with(|c| {
                    (c.borrow_mut().log)("ERROR: [a_Capi_dpi_send_data] No open connection found");
                });
            }
        }
    }
    0
}

/// `a_Capi_dpi_send_cmd`.
#[allow(clippy::must_use_candidate)]
pub fn capi_dpi_send_cmd(
    url: Option<&DilloUrl>,
    bw: u32,
    cmd: &str,
    server: &str,
    flags: i32,
) -> i32 {
    capi_dpi_send_data(url, bw, cmd.as_bytes(), server, flags)
}

/// `a_Capi_stop_client` — `force` : abort aussi la CCC si dernier client.
pub fn capi_stop_client(key: i32, force: bool) {
    if let Some((url, buf_size)) = cache::client_get_if_unique(key) {
        if force || buf_size == 0 {
            conn_abort_by_url(&url);
        }
    }
    cache_stop_client(key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url::url_new;

    fn u(s: &str) -> DilloUrl {
        url_new(s.as_bytes(), None).unwrap_or_else(|| panic!("bad url: {s}"))
    }

    fn reset() {
        capi_init();
        crate::timeout::timeout_freeall();
    }

    #[test]
    fn map_flags_translates() {
        assert_eq!(map_cache_flags(0), 0);
        let f = map_cache_flags(cache::ca_flags::IS_EMPTY | cache::ca_flags::IN_PROGRESS);
        assert!(f & CAPI_IS_CACHED != 0);
        assert!(f & CAPI_IS_EMPTY != 0);
        assert!(f & CAPI_IN_PROGRESS != 0);
        assert_eq!(f & CAPI_COMPLETED, 0);
        let f2 = map_cache_flags(cache::ca_flags::GOT_HEADER);
        assert!(f2 & CAPI_COMPLETED != 0);
        assert_eq!(f2 & CAPI_IN_PROGRESS, 0);
    }

    #[test]
    fn url_uses_dpi_dispatch() {
        assert_eq!(url_uses_dpi(&u("http://x/")), None);
        assert_eq!(url_uses_dpi(&u("https://x/")), None);
        assert_eq!(url_uses_dpi(&u("about:splash")), None);
        // Amont : nom = texte jusqu'au premier '/' après « dpi:/ » ;
        // sans '/' → « ? ».
        assert_eq!(
            url_uses_dpi(&u("dpi:/bookmarks/x")),
            Some("bookmarks".to_string())
        );
        assert_eq!(url_uses_dpi(&u("dpi:/bm/x")), Some("bookmarks".to_string()));
        assert_eq!(url_uses_dpi(&u("dpi:/ftp/dir")), Some("ftp".to_string()));
        assert_eq!(url_uses_dpi(&u("dpi:/bookmarks")), Some("?".to_string()));
        assert_eq!(url_uses_dpi(&u("dpi:/")), Some("?".to_string()));
        assert_eq!(url_uses_dpi(&u("ftp://x/")), Some("proto.ftp".to_string()));
        assert_eq!(
            url_uses_dpi(&u("file:/tmp/x")),
            Some("proto.file".to_string())
        );
    }

    #[test]
    fn dpi_verify_request_rules() {
        reset();
        // Non-dpi : toujours permis.
        assert!(capi_dpi_verify_request(0, &u("http://x/")));
        // dpi sans GET|POST : permis.
        assert!(capi_dpi_verify_request(0, &u("dpi:/bookmarks")));
        // dpi GET /vsource/ : permis.
        let mut vs = u("dpi:/vsource/");
        vs.flags |= url_flags::GET;
        assert!(capi_dpi_verify_request(0, &vs));
        // dpi GET hors vsource : refusé (pile nav vide — pas de referer dpi).
        let mut g = u("dpi:/bm");
        g.flags |= url_flags::GET;
        assert!(!capi_dpi_verify_request(0, &g));
    }

    #[test]
    fn about_url_uses_cache_without_conn() {
        reset();
        let url = u("about:splash");
        let web = Rc::new(HttpWeb::new(url.clone(), None, WEB_ROOT_URL));
        let key = capi_open_url(web, None);
        assert!(key > 0);
        assert!(capi_get_flags(&url) & CAPI_IS_CACHED != 0);
        // `about:` passe par le cache directement — pas de connexion CCC.
        assert!(CAPI.with(|c| c.borrow().conns.is_empty()));
    }

    #[test]
    fn dpi_send_data_registers_and_sends_pending() {
        reset();
        // Sans `dpi_ccc` injecté : la branche 1 avorte la connexion à
        // OP_START — le conn ne reste pas enregistré (fidèle à
        // « server non démarré » amont, ici « seam absente »).
        let url = u("dpi:/hello");
        capi_dpi_send_cmd(Some(&url), 0, "cmd=hi", "hello", 1);
        assert!(CAPI.with(|c| c.borrow().conns.is_empty()));
    }

    #[test]
    fn conn_abort_by_url_marks_aborted() {
        reset();
        // Insère un conn factice http (branches non armées : abort no-op
        // comme amont quand `InfoSend/InfoRecv` sont NULL).
        let url = u("http://example.com/");
        let cr = CapiConn::new(Some(&url), 0, "http", "none");
        conn_ref(&cr);
        conn_abort_by_url(&url);
        assert!(cr.borrow().flags & ABORTED != 0);
        conn_unref(&cr);
        assert!(CAPI.with(|c| c.borrow().conns.is_empty()));
    }

    #[test]
    fn stop_client_delegates_to_cache() {
        reset();
        let url = u("http://example.com/");
        cache::cache_entry_inject(
            &url,
            b"x",
            cache::ca_flags::GOT_HEADER | cache::ca_flags::GOT_LENGTH,
        );
        let web = Rc::new(HttpWeb::new(url.clone(), None, WEB_ROOT_URL));
        let key = capi_open_url(web, None);
        assert!(key > 0);
        capi_stop_client(key, false);
        assert_eq!(cache::cache_client_count(), 0);
    }

    #[test]
    fn vsource_url_is_stored() {
        reset();
        let url = u("http://example.com/");
        capi_set_vsource_url(&url);
        assert!(CAPI.with(|c| c.borrow().vs_url.is_some()));
    }
}
