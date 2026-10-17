//! Port of `src/IO/http.c` — transport HTTP : DNS → connect TCP →
//! (tunnel `CONNECT` proxy) → (TLS) → envoi de la requête ; réponses
//! pipelinées vers le module amont (cache), réutilisation keep-alive.
//!
//! Mapping :
//! - `SocketData_t` → `SocketData` ; `ValidSocks` (Klist) →
//!   `HashMap<skey, SocketData>` dans [`HttpEngine`]. Les sockets
//!   `QUEUED` puis libérées restent zombies `TO_BE_FREED` dans la map
//!   jusqu'au balayage de file (≙ `dFree` différé amont).
//! - `Server_t` → `Server` ; `servers` (Dlist) → `HashMap<(host-ci,
//!   port, https)>` (comparaison ascii-ci amont ≙ clé lowercase).
//! - `fd_map` → `HashMap<sock_id, skey>` — même rôle : retrouver la
//!   socket par « fd » au `connect_done`/branche `"FD"`.
//! - `a_Http_ccc` → [`http_ccc`] verbatim (1B query, 1F status,
//!   2F réponses/`send_page_2eof`, 2B `"FD"`/`"reply_complete"`).
//! - `connect()` non-bloquant + `Http_connect_socket_cb` (watch WRITE,
//!   `getsockopt(SO_ERROR)`, retry par adresse) → `connect_task` async
//!   itérant `addr_list` ; résultat via `HttpEvent` → [`poll_http`].
//!   `addr_list_idx` disparaît : l'itération vit dans la tâche (même
//!   séquence d'essais séquentiels — le champ ne sert qu'à savoir
//!   quelle IP a été atteinte).
//! - `DilloWeb` → [`HttpWeb`] : le sous-ensemble que `http.c` touche
//!   (`url`, `requester`, `flags` `WEB_*`, `a_Web_valid`, `MSG_BW`).
//!   Le vrai `web.cc` (phase browser) le fournira.
//! - `a_Cookies_get_query`/`a_Auth_get_auth_str`/`prefs` → injectés
//!   ([`HttpEngine::set_cookies_fn`]/[`HttpEngine::set_auth_fn`]/
//!   [`HttpConfig`]) : deps descendantes (net ne voit pas browser).
//!   Défauts = build `!HAVE_COOKIES` amont (`dStrdup("")`/`NULL`).
//! - TLS : `a_Tls_connect_ready` dans la file, `a_Tls_connect` après
//!   TCP ou tunnel `CONNECT` (swap du stream dans `IoEngine` ≙
//!   `a_Tls_connection(fd)` par-fd amont), `TlsEvent` →
//!   `Tls_check_certificate` puis `a_Http_connect_done`.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::rc::Rc;

use dillo_common::dlib::{self, Dstr};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::chain::{
    Chain, ChainCb, ChainData, DataBuf, LinkId, BCK, FWD, OP_ABORT, OP_END, OP_SEND, OP_START,
};
use crate::dns::dns_resolve;
use crate::io::{io_ccb, IoEngine, IoStream};
use crate::tls::{
    TlsEngine, TlsEvent, CERT_STATUS_RECEIVING, CERT_STATUS_USER_ACCEPTED, TLS_CONNECT_NEVER,
    TLS_CONNECT_READY,
};
use crate::url::{flags as url_flags, url_new, DilloUrl};

pub use crate::web::{WEB_DOWNLOAD, WEB_IMAGE, WEB_ROOT_URL, WEB_STYLESHEET};

const HTTP_SOCKET_USE_PROXY: u32 = 0x1;
const HTTP_SOCKET_QUEUED: u32 = 0x2;
const HTTP_SOCKET_TO_BE_FREED: u32 = 0x4;
const HTTP_SOCKET_TLS: u32 = 0x8;
const HTTP_SOCKET_IOWATCH_ACTIVE: u32 = 0x10;

/// `MsgSink` — sink `a_UIcmd_set_msg` (status bar / tests).
type MsgSink = std::cell::RefCell<Box<dyn FnMut(&str)>>;

/// `DilloWeb` (web.hh) — la requête vue par `http.c`, `cache.c`, `capi.c`
/// et `web.cc`. `valid` ≙ `a_Web_valid` (le web peut mourir entre
/// l'enqueue et la connexion — abort navigateur) ; `msg` ≙ `MSG_BW` →
/// `a_UIcmd_set_msg` injecté (status bar).
static NEXT_WEB_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// `DilloWeb` (web.hh) — la requête vue par `http.c`, `cache.c`, `capi.c`
/// et `web.cc`. `valid` ≙ `a_Web_valid` (le web peut mourir entre
/// l'enqueue et la connexion — abort navigateur) ; `msg` ≙ `MSG_BW` →
/// `a_UIcmd_set_msg` injecté (status bar). Partagé en `Rc` entre transport
/// et cache (≙ `DilloWeb*` amont) ; `valid` via `Cell` → invalidation
/// visible de tous les détenteurs.
pub struct HttpWeb {
    /// Identité du web (≙ adresse dans `ValidWebs` amont — la seam de
    /// dispatch navigateur peut l'utiliser comme clé).
    pub id: usize,
    /// `web->url` — URL demandée (jamais modifiée par le transport).
    pub url: DilloUrl,
    /// `web->requester` — URL à l'origine de la requête (cookies).
    /// `None` = action utilisateur (≙ `NULL` amont).
    pub requester: Option<DilloUrl>,
    /// `web->bw` — fenêtre navigateur (handle opaque, résolu côté
    /// `dillo-browser` via les seams `BrowserOps`/`UiOps`/`NavOps`).
    pub bw: u32,
    /// `web->flags` — `WEB_*` (module [`crate::web`]).
    pub flags: u32,
    /// `web->filename` — destination téléchargement (`WEB_Download`).
    pub filename: Option<String>,
    /// `web->stream` — fichier ouvert par `capi` pour `WEB_Download`
    /// (≙ `web->stream` amont ; `RefCell` = partage via `Rc` main-
    /// thread).
    pub stream: std::cell::RefCell<Option<std::fs::File>>,
    /// `web->SavedBytes` — octets déjà écrits sur disque (muté à
    /// travers le `Rc` partagé, comme `Web->SavedBytes` amont).
    pub saved_bytes: std::cell::Cell<usize>,
    valid: std::cell::Cell<bool>,
    msg_sink: MsgSink,
}

impl std::fmt::Debug for HttpWeb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpWeb")
            .field("flags", &self.flags)
            .field("valid", &self.valid.get())
            .finish_non_exhaustive()
    }
}

impl HttpWeb {
    /// Crée un web valide, sink de messages muet par défaut.
    #[must_use]
    pub fn new(url: DilloUrl, requester: Option<DilloUrl>, flags: u32) -> Self {
        Self {
            id: NEXT_WEB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            url,
            requester,
            bw: 0,
            flags,
            filename: None,
            stream: std::cell::RefCell::new(None),
            saved_bytes: std::cell::Cell::new(0),
            valid: std::cell::Cell::new(true),
            msg_sink: std::cell::RefCell::new(Box::new(|_| {})),
        }
    }

    /// Injecte le sink `a_UIcmd_set_msg` (status bar / tests).
    pub fn set_msg_sink(&mut self, sink: Box<dyn FnMut(&str)>) {
        *self.msg_sink.borrow_mut() = sink;
    }

    /// `a_Web_valid` — le web n'a pas été invalidé (abort).
    #[must_use]
    pub fn valid(&self) -> bool {
        self.valid.get()
    }

    /// Invalide le web (≙ retrait de `webs` amont).
    pub fn invalidate(&self) {
        self.valid.set(false);
    }

    /// `MSG_BW(web, root, fmt)` — émis seulement si `valid` et
    /// (`root == 0` ou `WEB_RootUrl`), comme la macro amont.
    pub fn msg(&self, root: i32, s: &str) {
        if self.valid() && (root == 0 || self.flags & WEB_ROOT_URL != 0) {
            (self.msg_sink.borrow_mut())(s);
        }
    }
}

/// `SocketData_t` — une socket HTTP (connectée ou en file).
struct SocketData {
    /// `SockFD` — `sock_id` dans `IoEngine` (−1 avant connexion).
    sock_id: i32,
    /// `flags` `HTTP_SOCKET_*`.
    flags: u32,
    /// `web` — référence client (pas propriétaire, comme amont).
    web: Option<Rc<HttpWeb>>,
    /// `url` — `a_Url_dup(web->url)`.
    url: Option<DilloUrl>,
    /// `addr_list` — réponse DNS (`Dlist<DilloHost>` amont).
    addr_list: Vec<IpAddr>,
    /// `Info` — lien CCC 1B (ops asynchrones).
    info: Option<LinkId>,
    /// `connected_to` — host serveur pour la limite par-serveur.
    connected_to: Option<String>,
    /// `connect_port`.
    connect_port: u16,
    /// `https_proxy_reply` — accumulateur réponse CONNECT (`Some`
    /// tant que le tunnel n'est pas établi).
    https_proxy_reply: Option<Dstr>,
    /// Tâche `connect()` en vol ≙ `HTTP_SOCKET_IOWATCH_ACTIVE`.
    connect_abort: Option<tokio::task::AbortHandle>,
}

/// `Server_t` — état par (host, port, https) : limite de connexions
/// et file d'attente.
struct Server {
    /// `host` (lowercase — comparaison `dStrAsciiCasecmp` amont).
    host: String,
    /// `port` — porté pour la forme `Server_t` ; la clé `servers` le
    /// contient déjà (la lecture se fait via la clé).
    #[allow(dead_code)]
    port: u16,
    /// `https` — idem (présent dans la clé).
    #[allow(dead_code)]
    https: bool,
    /// `active_conns`.
    active_conns: i32,
    /// `running_the_queue` — garde de réentrance.
    running_the_queue: i32,
    /// `queue` — skeys en attente (`Dlist<SocketData_t*>` amont).
    queue: Vec<i32>,
}

/// Configuration ≙ les champs `prefs` lus par `http.c`. Construite
/// par l'appelant depuis `dillo_browser::prefs::Prefs` (deps
/// descendantes — net ne connaît pas browser).
pub struct HttpConfig {
    /// `prefs.http_language` → en-tête `Accept-Language` (vide sinon).
    pub http_language: Option<String>,
    /// `prefs.http_proxy` — l'env `http_proxy` prime ([`HttpEngine::new`]).
    pub http_proxy: Option<DilloUrl>,
    /// `prefs.http_proxyuser` — utilisé par `set_proxy_passwd`.
    pub http_proxyuser: Option<String>,
    /// `prefs.no_proxy` — liste à espaces, suffix-match ascii-ci.
    pub no_proxy: Option<String>,
    /// `prefs.http_max_conns` — limite par serveur.
    pub http_max_conns: i32,
    /// `prefs.http_persistent_conns` → `Connection: keep-alive|close`.
    pub http_persistent_conns: bool,
    /// `prefs.http_user_agent`.
    pub http_user_agent: String,
    /// `prefs.http_referer` — `"host"`|`"path"`|autre (masqué).
    pub http_referer: String,
    /// `prefs.trace_http` — log de la requête complète.
    pub trace_http: bool,
    /// `ENABLE_BROTLI` — ajoute `, br` à `Accept-Encoding`.
    pub brotli: bool,
}

impl Default for HttpConfig {
    /// Défauts = `dillorc` amont (sans proxy, sans language).
    fn default() -> Self {
        Self {
            http_language: None,
            http_proxy: None,
            http_proxyuser: None,
            no_proxy: None,
            http_max_conns: 6,
            http_persistent_conns: true,
            http_user_agent: "Dillo/3.3.0-rc1".to_string(),
            http_referer: "host".to_string(),
            trace_http: false,
            brotli: true,
        }
    }
}

/// `CookiesGetQuery_t` amont (`a_Cookies_get_query`).
type CookiesFn = Rc<dyn Fn(&mut DilloUrl, Option<&mut DilloUrl>, bool) -> String>;
/// `AuthGetAuthStr_t` amont (`a_Auth_get_auth_str`).
type AuthFn = Rc<dyn Fn(&mut DilloUrl, &[u8]) -> Option<String>>;

/// Événement `connect()` → thread principal (≙ `Http_connect_socket_cb`).
enum HttpEvent {
    /// `connect()` réussi — `getsockopt(SO_ERROR)==0` amont.
    Connected { skey: i32, stream: TcpStream },
    /// Toutes les adresses épuisées ≙ `connect_done(fd, FALSE)`.
    ConnectFailed { skey: i32 },
}

/// `HttpEngine` — `ValidSocks` + `servers` + `fd_map` + globales proxy.
/// Module `Chain` ; toute la logique est single-thread CCC.
pub struct HttpEngine {
    rt: tokio::runtime::Handle,
    /// `ValidSocks` (Klist) — skey → `SocketData` (zombies
    /// `TO_BE_FREED` en file ≙ `dFree` différé amont).
    socks: HashMap<i32, SocketData>,
    next_skey: i32,
    /// `servers` (Dlist) — clé (host-ci, port, https).
    servers: HashMap<(String, u16, bool), Server>,
    /// `fd_map` — `sock_id` → skey (`a_Http_connect_done`/`"FD"`).
    fd_map: HashMap<i32, i32>,
    /// `HTTP_Proxy` — env `http_proxy` puis prefs.
    proxy: Option<DilloUrl>,
    /// `HTTP_Proxy_Auth_base64` — `set_proxy_passwd` seulement (le
    /// chargement dillorc est désactivé amont — risque sécurité).
    proxy_auth_b64: Option<String>,
    /// `HTTP_Language_hdr` — pré-formaté à l'init.
    language_hdr: String,
    /// `prefs.http_proxyuser` (pour `set_proxy_passwd`).
    proxy_user: Option<String>,
    /// `prefs.no_proxy`.
    no_proxy: Option<String>,
    /// `prefs.http_max_conns`.
    max_conns: i32,
    /// `prefs.http_persistent_conns`.
    persistent_conns: bool,
    /// `prefs.http_user_agent`.
    user_agent: String,
    /// `prefs.http_referer`.
    referer_pref: String,
    /// `prefs.trace_http`.
    trace_http: bool,
    /// `Accept-Encoding` complet (`gzip, deflate` + `, br` si brotli).
    accept_encoding: &'static str,
    /// `a_Cookies_get_query` injecté (défaut = `!HAVE_COOKIES` : "").
    /// `&mut` ≙ `DilloUrl*` amont (lazy parse) — sur un clone du
    /// `requester` (le web vit dans un `Rc`, pas mutable ici).
    cookies_fn: CookiesFn,
    /// `a_Auth_get_auth_str` injecté (défaut amont `NULL` → "").
    auth_fn: AuthFn,
    /// Pompe `connect_task` → [`poll_http`].
    event_tx: mpsc::UnboundedSender<HttpEvent>,
    event_rx: mpsc::UnboundedReceiver<HttpEvent>,
}

impl std::fmt::Debug for HttpEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpEngine")
            .field("socks", &self.socks.len())
            .field("servers", &self.servers.len())
            .field("fd_map", &self.fd_map.len())
            .finish_non_exhaustive()
    }
}

impl HttpEngine {
    /// `a_Http_init` — proxy : env `http_proxy` > prefs ; en-tête
    /// `Accept-Language` pré-formaté.
    #[must_use]
    pub fn new(rt: tokio::runtime::Handle, cfg: HttpConfig) -> Self {
        let language_hdr = cfg
            .http_language
            .map(|l| format!("Accept-Language: {l}\r\n"))
            .unwrap_or_default();
        let proxy = std::env::var("http_proxy")
            .ok()
            .filter(|s| !s.is_empty())
            .and_then(|s| url_new(s.as_bytes(), None))
            .or(cfg.http_proxy);
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            rt,
            socks: HashMap::new(),
            next_skey: 0,
            servers: HashMap::new(),
            fd_map: HashMap::new(),
            proxy,
            proxy_auth_b64: None,
            language_hdr,
            proxy_user: cfg.http_proxyuser,
            no_proxy: cfg.no_proxy,
            max_conns: cfg.http_max_conns,
            persistent_conns: cfg.http_persistent_conns,
            user_agent: cfg.http_user_agent,
            referer_pref: cfg.http_referer,
            trace_http: cfg.trace_http,
            accept_encoding: if cfg.brotli {
                "gzip, deflate, br"
            } else {
                "gzip, deflate"
            },
            cookies_fn: Rc::new(|_, _, _| String::new()),
            auth_fn: Rc::new(|_, _| None),
            event_tx,
            event_rx,
        }
    }

    /// Injecte `a_Cookies_get_query` (au port de `cookies.c`).
    pub fn set_cookies_fn(&mut self, f: CookiesFn) {
        self.cookies_fn = f;
    }

    /// Injecte `a_Auth_get_auth_str` (au port de `auth.c`).
    pub fn set_auth_fn(&mut self, f: AuthFn) {
        self.auth_fn = f;
    }

    /// `a_Http_proxy_auth` — auth proxy déjà configurée ?
    #[must_use]
    pub fn proxy_auth(&self) -> bool {
        self.proxy_auth_b64.is_some()
    }

    /// `a_Http_set_proxy_passwd` — `user:passwd` → base64.
    pub fn set_proxy_passwd(&mut self, passwd: &str) {
        let user = self.proxy_user.clone().unwrap_or_default();
        let b64 = dillo_common::misc::encode_base64(format!("{user}:{passwd}").as_bytes());
        self.proxy_auth_b64 = Some(String::from_utf8_lossy(&b64).into_owned());
    }

    /// `Http_sock_new` — `SocketData` zéro + skey.
    fn sock_new(&mut self) -> i32 {
        let skey = self.next_skey;
        self.next_skey += 1;
        self.socks.insert(
            skey,
            SocketData {
                sock_id: -1,
                flags: 0,
                web: None,
                url: None,
                addr_list: Vec::new(),
                info: None,
                connected_to: None,
                connect_port: 0,
                https_proxy_reply: None,
                connect_abort: None,
            },
        );
        skey
    }

    /// `a_Klist_get_data` — les zombies `TO_BE_FREED` sont invisibles
    /// (≙ retirés de `ValidSocks` amont).
    fn sd(&self, skey: i32) -> Option<&SocketData> {
        self.socks
            .get(&skey)
            .filter(|s| s.flags & HTTP_SOCKET_TO_BE_FREED == 0)
    }

    /// `Http_server_get` — clé (host-ci, port, https) ; crée si absent.
    fn server_get(&mut self, host: &str, port: u16, https: bool) -> &mut Server {
        let key = (host.to_ascii_lowercase(), port, https);
        self.servers.entry(key).or_insert_with(|| Server {
            host: host.to_ascii_lowercase(),
            port,
            https,
            active_conns: 0,
            running_the_queue: 0,
            queue: Vec::new(),
        })
    }

    /// Vide le canal `connect_task` → [`poll_http`].
    fn drain_events(&mut self) -> Vec<HttpEvent> {
        let mut v = Vec::new();
        while let Ok(ev) = self.event_rx.try_recv() {
            v.push(ev);
        }
        v
    }
}

/// `Http_must_use_proxy` — proxy configuré ET host pas couvert par
/// `no_proxy` (tokens `dStrsep(" ")` — **les tokens vides matchent**,
/// quirk amont : `strlen(tok)==0` → suffixe trivialement égal).
fn must_use_proxy(eng: &HttpEngine, hostname: &[u8]) -> bool {
    if eng.proxy.is_none() {
        return false;
    }
    if let Some(np) = &eng.no_proxy {
        let host_len = hostname.len();
        for tok in np.split(' ') {
            let tok = tok.as_bytes();
            if host_len >= tok.len()
                && dlib::str_ascii_casecmp(&hostname[host_len - tok.len()..], tok) == 0
            {
                return false;
            }
        }
    }
    true
}

/// `Http_get_referer` — `"host"` → `scheme://authority/` ; `"path"` →
/// +path ; autre → "".
fn get_referer(url: &DilloUrl, pref: &str) -> String {
    let scheme = String::from_utf8_lossy(url.scheme.as_deref().unwrap_or(b""));
    let auth = String::from_utf8_lossy(url.authority.as_deref().unwrap_or(b""));
    match pref {
        "host" => format!("Referer: {scheme}://{auth}/\r\n"),
        "path" => {
            let path = url
                .path
                .as_deref()
                .map_or("/".to_string(), |p| String::from_utf8_lossy(p).into_owned());
            format!("Referer: {scheme}://{auth}{path}\r\n")
        }
        _ => String::new(),
    }
}

/// `Http_make_content_type` — `multipart/form-data` : boundary =
/// `data[2..'\r')` (les `--` initiaux sautés, verbatim) ; `data`
/// vide/≤2 → boundary arbitraire `0`. Sinon `x-www-form-urlencoded`.
fn make_content_type(url: &DilloUrl) -> Vec<u8> {
    if url.flags & url_flags::MULTIPART_ENC != 0 {
        let mut dstr = Vec::from(&b"multipart/form-data; boundary=\""[..]);
        if url.data.len() > 2 {
            let start = &url.data[2..];
            if let Some(eol) = start.iter().position(|&b| b == b'\r') {
                dstr.extend_from_slice(&start[..eol]);
            }
        } else {
            dstr.push(b'0');
        }
        dstr.push(b'"');
        dstr
    } else {
        b"application/x-www-form-urlencoded".to_vec()
    }
}

/// `Http_make_query_str` — chaîne de requête verbatim (ordre des
/// en-têtes et valeurs identiques à l'amont).
// verbatim amont (formats GET/POST entiers)
#[allow(clippy::too_many_lines)]
fn make_query_str(eng: &HttpEngine, web: &HttpWeb, use_proxy: bool, use_tls: bool) -> Vec<u8> {
    let mut url = web.url.clone();
    let accept_hdr: &str = if web.flags & WEB_IMAGE != 0 {
        "image/png,image/*;q=0.8,*/*;q=0.5"
    } else if web.flags & WEB_STYLESHEET != 0 {
        "text/css,*/*;q=0.1"
    } else {
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
    };
    let connection_hdr: &str = if eng.persistent_conns {
        "keep-alive"
    } else {
        "close"
    };

    let mut request_uri: Vec<u8> = Vec::new();
    let mut proxy_auth = String::new();
    if use_proxy && !use_tls {
        // URL absolue ; '/' forcé si ni path ni query ; '#' tronqué.
        request_uri.extend_from_slice(url.str_());
        if url.path.is_none() && url.query.is_none() {
            request_uri.push(b'/');
        }
        if let Some(hash) = request_uri.iter().rposition(|&b| b == b'#') {
            request_uri.truncate(hash);
        }
        if let Some(b64) = &eng.proxy_auth_b64 {
            proxy_auth = format!("Proxy-Authorization: Basic {b64}\r\n");
        }
    } else {
        request_uri.extend_from_slice(url.path.as_deref().unwrap_or(b"/"));
        if url.query.is_some() {
            request_uri.push(b'?');
            request_uri.extend_from_slice(url.query.as_deref().unwrap_or(b""));
        }
    }

    let mut requester = web.requester.clone();
    let cookies = (eng.cookies_fn)(&mut url, requester.as_mut(), {
        web.flags & WEB_ROOT_URL != 0
    });
    let auth = (eng.auth_fn)(&mut url, &request_uri).unwrap_or_default();
    let referer = get_referer(&url, &eng.referer_pref);
    let host = String::from_utf8_lossy(url.authority.as_deref().unwrap_or(b""));
    let uri = String::from_utf8_lossy(&request_uri);

    let mut query = if url.flags & url_flags::POST != 0 {
        let content_type = String::from_utf8_lossy(&make_content_type(&url)).into_owned();
        format!(
            "POST {uri} HTTP/1.1\r\n\
             Host: {host}\r\n\
             User-Agent: {ua}\r\n\
             Accept: {accept}\r\n\
             {lang}Accept-Encoding: {ae}\r\n\
             {auth}DNT: 1\r\n\
             {pauth}{referer}Connection: {conn}\r\n\
             Content-Type: {ct}\r\n\
             Content-Length: {len}\r\n\
             {cookies}\r\n",
            uri = uri,
            host = host,
            ua = eng.user_agent,
            accept = accept_hdr,
            lang = eng.language_hdr,
            ae = eng.accept_encoding,
            auth = auth,
            pauth = proxy_auth,
            referer = referer,
            conn = connection_hdr,
            ct = content_type,
            len = url.data.len(),
            cookies = cookies,
        )
        .into_bytes()
    } else {
        let cc = if url.flags & url_flags::E2E_QUERY != 0 {
            "Pragma: no-cache\r\nCache-Control: no-cache\r\n"
        } else {
            ""
        };
        format!(
            "GET {uri} HTTP/1.1\r\n\
             Host: {host}\r\n\
             User-Agent: {ua}\r\n\
             Accept: {accept}\r\n\
             {lang}Accept-Encoding: {ae}\r\n\
             {auth}DNT: 1\r\n\
             {pauth}{referer}Connection: {conn}\r\n\
             {cc}{cookies}\r\n",
            uri = uri,
            host = host,
            ua = eng.user_agent,
            accept = accept_hdr,
            lang = eng.language_hdr,
            ae = eng.accept_encoding,
            auth = auth,
            pauth = proxy_auth,
            referer = referer,
            conn = connection_hdr,
            cc = cc,
            cookies = cookies,
        )
        .into_bytes()
    };
    if url.flags & url_flags::POST != 0 {
        query.extend_from_slice(&url.data);
    }
    query
}

/// `Http_get_connect_str` — requête `CONNECT host:443` pour tunnel
/// HTTPS via proxy (ajoute `:443` si l'authority ne finit pas par un
/// chiffre, verbatim).
fn get_connect_str(eng: &HttpEngine, url: &DilloUrl) -> Vec<u8> {
    debug_assert!({
        // `dReturn_val_if_fail(Http_must_use_proxy(URL_HOST))` amont
        let mut u = url.clone();
        must_use_proxy(eng, u.hostname().unwrap_or(b""))
    });
    let mut auth = url.authority.clone().unwrap_or_default();
    if auth.last().is_some_and(|c| !c.is_ascii_digit()) || auth.is_empty() {
        auth.extend_from_slice(b":443");
    }
    let auth = String::from_utf8_lossy(&auth);
    let proxy_auth = eng.proxy_auth_b64.as_ref().map_or(String::new(), |b64| {
        format!("Proxy-Authorization: Basic {b64}\r\n")
    });
    format!("CONNECT {auth} HTTP/1.1\r\nHost: {auth}\r\n{proxy_auth}\r\n").into_bytes()
}

/// `Http_socket_enqueue` — non-images devant le premier `WEB_Image`
/// valide de la file (priorité pages/styles, verbatim).
fn socket_enqueue(chain: &mut Chain, srv_key: &(String, u16, bool), skey: i32) {
    let Some(eng) = chain.module_mut::<HttpEngine>() else {
        return;
    };
    if let Some(sd) = eng.socks.get_mut(&skey) {
        debug_assert_eq!(sd.flags & HTTP_SOCKET_QUEUED, 0);
        sd.flags |= HTTP_SOCKET_QUEUED;
    }
    let is_image = eng
        .sd(skey)
        .and_then(|s| s.web.as_ref())
        .is_some_and(|w| w.flags & WEB_IMAGE != 0);
    let Some(srv) = eng.servers.get_mut(srv_key) else {
        return;
    };
    if !is_image {
        for i in 0..srv.queue.len() {
            // amont voit aussi les zombies TBF (leur `web` survit) —
            // `socks` direct, pas `sd()` qui les filtre.
            let other_image = eng
                .socks
                .get(&srv.queue[i])
                .and_then(|s| s.web.as_ref())
                .is_some_and(|w| w.valid() && w.flags & WEB_IMAGE != 0);
            if other_image {
                srv.queue.insert(i, skey);
                return;
            }
        }
    }
    srv.queue.push(skey);
}

/// `Http_socket_activate` — sort de file + `active_conns++` +
/// `connected_to = srv->host`.
fn socket_activate(chain: &mut Chain, srv_key: &(String, u16, bool), skey: i32) {
    let Some(eng) = chain.module_mut::<HttpEngine>() else {
        return;
    };
    let host = {
        let Some(srv) = eng.servers.get_mut(srv_key) else {
            return;
        };
        srv.queue.retain(|&k| k != skey);
        srv.active_conns += 1;
        srv.host.clone()
    };
    if let Some(sd) = eng.socks.get_mut(&skey) {
        sd.flags &= !HTTP_SOCKET_QUEUED;
        sd.connected_to = Some(host);
    }
}

/// `Http_socket_reuse_compatible` — même serveur ; TLS+proxy exige
/// le même host:port cible (verbatim).
fn reuse_compatible(old: &SocketData, new: &SocketData) -> bool {
    if !new.web.as_ref().is_some_and(|w| w.valid()) {
        return false;
    }
    if old.flags & HTTP_SOCKET_TLS == 0 || old.flags & HTTP_SOCKET_USE_PROXY == 0 {
        return true;
    }
    match (old.url.as_ref(), new.url.as_ref()) {
        (Some(ou), Some(nu)) => {
            let (mut ou, mut nu) = (ou.clone(), nu.clone());
            ou.port() == nu.port()
                && ou
                    .hostname()
                    .zip(nu.hostname())
                    .is_some_and(|(a, b)| dlib::str_ascii_casecmp(a, b) == 0)
        }
        _ => false,
    }
}

/// `Http_socket_free` — verbatim : IOWATCH → abort (≙ remove watch +
/// `dClose` du stream en vol) ; `QUEUED` → zombie `TO_BE_FREED`
/// (balayé par la file) ; sinon `fd_map` −, `reset_server_state`,
/// `a_Tls_close_by_fd`, `active_conns--`, relance de la file.
fn socket_free(chain: &mut Chain, skey: i32) {
    #[allow(clippy::large_enum_variant)]
    enum Tail {
        /// `QUEUED` → zombie (rien d'autre à faire).
        Zombie,
        /// sd complète à libérer.
        Free {
            url: Option<DilloUrl>,
            sock_id: i32,
            connected: Option<(String, u16, bool)>,
        },
    }
    let tail = {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(sd) = eng.socks.get_mut(&skey) else {
            return;
        };
        if sd.flags & HTTP_SOCKET_IOWATCH_ACTIVE != 0 {
            sd.flags &= !HTTP_SOCKET_IOWATCH_ACTIVE;
            if let Some(h) = sd.connect_abort.take() {
                h.abort(); // ≙ a_IOwatch_remove_fd + dClose
            }
        }
        sd.https_proxy_reply = None;
        if sd.flags & HTTP_SOCKET_QUEUED != 0 {
            sd.flags |= HTTP_SOCKET_TO_BE_FREED;
            sd.url = None;
            Tail::Zombie
        } else {
            if sd.sock_id != -1 {
                eng.fd_map.remove(&sd.sock_id);
            }
            let connected = sd
                .connected_to
                .clone()
                .map(|h| (h, sd.connect_port, sd.flags & HTTP_SOCKET_TLS != 0));
            Tail::Free {
                url: sd.url.take(),
                sock_id: sd.sock_id,
                connected,
            }
        }
    };
    let Tail::Free {
        url,
        sock_id,
        connected,
    } = tail
    else {
        return;
    };
    // `a_Tls_reset_server_state(S->url)`
    if let Some(mut u) = url {
        if let Some(tls) = chain.module_mut::<TlsEngine>() {
            tls.reset_server_state(&mut u);
        }
    }
    if let Some((host, port, https)) = connected {
        // `a_Tls_close_by_fd(S->SockFD)`
        {
            let connecting = chain
                .module_mut::<TlsEngine>()
                .and_then(|tls| tls.close_by_fd(sock_id));
            match connecting {
                Some(true) => {
                    if let Some(eng) = chain.module_mut::<IoEngine>() {
                        eng.remove_socket(sock_id);
                    }
                }
                Some(false) => {
                    if let Some(eng) = chain.module_mut::<IoEngine>() {
                        eng.close_notify(sock_id);
                    }
                }
                None => {}
            }
        }
        let srv_key = (host, port, https);
        {
            let Some(eng) = chain.module_mut::<HttpEngine>() else {
                return;
            };
            let srv = eng.server_get(&srv_key.0, srv_key.1, srv_key.2);
            srv.active_conns -= 1;
        }
        connect_queued_sockets(chain, &srv_key);
    }
    // `a_Url_free(S->url); dFree(S)` — retrait de ValidSocks
    if let Some(eng) = chain.module_mut::<HttpEngine>() {
        eng.socks.remove(&skey);
    }
}

/// `Http_server_remove` — vide la file (zombies → `dFree`) + retrait.
fn server_remove(chain: &mut Chain, srv_key: &(String, u16, bool)) {
    let Some(eng) = chain.module_mut::<HttpEngine>() else {
        return;
    };
    let Some(srv) = eng.servers.remove(srv_key) else {
        return;
    };
    for skey in srv.queue {
        if let Some(sd) = eng.socks.get_mut(&skey) {
            sd.flags &= !HTTP_SOCKET_QUEUED;
            if sd.flags & HTTP_SOCKET_TO_BE_FREED != 0 {
                eng.socks.remove(&skey); // dFree amont
            }
        }
    }
}

/// `Http_connect_queued_sockets` — verbatim : balayage zombies,
/// `TLS_CONNECT_*` par socket TLS, activation jusqu'à
/// `http_max_conns`, garde `running_the_queue`, retrait du serveur
/// vide.
// verbatim amont (boucle unique avec index réexaminé)
#[allow(clippy::too_many_lines)]
fn connect_queued_sockets(chain: &mut Chain, srv_key: &(String, u16, bool)) {
    {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(srv) = eng.servers.get_mut(srv_key) else {
            return;
        };
        srv.running_the_queue += 1;
    }
    let mut i = 0usize;
    loop {
        enum Step {
            /// file épuisée ou `active_conns` plein.
            Stop,
            /// zombie — retiré de la file + drop (i inchangé).
            Sweeped,
            /// `NEVER` ou web invalide → `Http_socket_free` (i++).
            Free(i32),
            /// `READY` → activate + connect (i inchangé — l'entrée sort).
            Activate(i32, LinkId),
            /// socket TLS → `connect_ready` évalué hors emprunt eng.
            CheckTls(i32, LinkId),
        }
        let step = {
            let Some(eng) = chain.module_mut::<HttpEngine>() else {
                return;
            };
            let Some(srv) = eng.servers.get(srv_key) else {
                return;
            };
            if i >= srv.queue.len() || srv.active_conns >= eng.max_conns {
                Step::Stop
            } else {
                let skey = srv.queue[i];
                match eng.socks.get(&skey) {
                    None => Step::Sweeped, // sd disparue → balayer
                    Some(sd) if sd.flags & HTTP_SOCKET_TO_BE_FREED != 0 => Step::Sweeped,
                    Some(sd) => {
                        let web_ok = sd.web.as_ref().is_some_and(|w| w.valid());
                        if !web_ok {
                            Step::Free(skey)
                        } else if sd.flags & HTTP_SOCKET_TLS != 0 {
                            match sd.info {
                                Some(info) => Step::CheckTls(skey, info),
                                None => Step::Free(skey),
                            }
                        } else {
                            match sd.info {
                                Some(info) => Step::Activate(skey, info),
                                None => Step::Free(skey),
                            }
                        }
                    }
                }
            }
        };
        match step {
            Step::Stop => break,
            Step::Sweeped => {
                let Some(eng) = chain.module_mut::<HttpEngine>() else {
                    return;
                };
                if let Some(srv) = eng.servers.get_mut(srv_key) {
                    if i < srv.queue.len() {
                        let skey = srv.queue.remove(i);
                        eng.socks.remove(&skey); // dFree zombie
                    }
                }
            }
            Step::Free(skey) => {
                socket_free(chain, skey);
                i += 1;
            }
            Step::Activate(skey, info) => {
                socket_activate(chain, srv_key, skey);
                connect_socket(chain, info);
            }
            Step::CheckTls(skey, info) => {
                // évalué en phase 2 (voir ci-dessous)
                let ready = {
                    let url = chain
                        .module_mut::<HttpEngine>()
                        .and_then(|eng| eng.socks.get(&skey).and_then(|s| s.url.clone()));
                    match url {
                        Some(mut u) => chain
                            .module_mut::<TlsEngine>()
                            .map_or(TLS_CONNECT_READY, |t| t.connect_ready(&mut u)),
                        None => TLS_CONNECT_READY,
                    }
                };
                if ready == TLS_CONNECT_NEVER {
                    socket_free(chain, skey);
                    i += 1;
                } else if ready == TLS_CONNECT_READY {
                    socket_activate(chain, srv_key, skey);
                    connect_socket(chain, info);
                } else {
                    i += 1; // TLS_CONNECT_NOT_YET
                }
            }
        }
    }
    // `if (--running_the_queue == 0 && active_conns == 0) remove`
    let remove = {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(srv) = eng.servers.get_mut(srv_key) else {
            return;
        };
        srv.running_the_queue -= 1;
        srv.running_the_queue == 0 && srv.active_conns == 0
    };
    if remove {
        server_remove(chain, srv_key);
    }
}

/// `connect()` + `Http_connect_socket_cb` — itère `addr_list`
/// séquentiellement (≙ `socket`+`connect`+`EINPROGRESS`+watch+
/// `SO_ERROR`, retry sur l'IP suivante à chaque échec).
async fn connect_task(
    skey: i32,
    addrs: Vec<IpAddr>,
    port: u16,
    tx: mpsc::UnboundedSender<HttpEvent>,
) {
    for ip in addrs {
        match TcpStream::connect(SocketAddr::new(ip, port)).await {
            Ok(stream) => {
                let _ = tx.send(HttpEvent::Connected { skey, stream });
                return;
            }
            Err(e) => {
                debug!(%ip, error = %e, "Http_connect_socket connect ERROR — next IP");
            }
        }
    }
    let _ = tx.send(HttpEvent::ConnectFailed { skey });
}

/// `Http_connect_socket` — arme la tâche de connexion sur l'adresse
/// courante (≙ `socket()`+`NONBLOCK`+`connect()` ; la tâche rejoue
/// les retries `addr_list_idx++` en interne).
fn connect_socket(chain: &mut Chain, info: LinkId) {
    let work = {
        let Some(skey) = chain.local_key::<i32>(info).copied() else {
            return;
        };
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(sd) = eng.socks.get_mut(&skey) else {
            return;
        };
        if sd.addr_list.is_empty() {
            // "ran out of IP addrs" → connect_done(fd, FALSE) — ici le
            // skey est connu directement (fd_map jamais rempli).
            None
        } else {
            sd.flags |= HTTP_SOCKET_IOWATCH_ACTIVE;
            Some((
                skey,
                sd.addr_list.clone(),
                sd.connect_port,
                eng.event_tx.clone(),
            ))
        }
    };
    let Some((skey, addrs, port, tx)) = work else {
        connect_failed(chain, info);
        return;
    };
    let rt = {
        let eng = chain.module_mut::<HttpEngine>().unwrap();
        eng.rt.clone()
    };
    let h = rt.spawn(connect_task(skey, addrs, port, tx));
    if let Some(eng) = chain.module_mut::<HttpEngine>() {
        if let Some(sd) = eng.socks.get_mut(&skey) {
            sd.connect_abort = Some(h.abort_handle());
        }
    }
}

/// `Http_get_connect_str` guard — `must_use_proxy` vérifié par
/// l'appelant (assert amont).
fn connect_tls(chain: &mut Chain, info: LinkId) {
    let work = {
        let Some(skey) = chain.local_key::<i32>(info).copied() else {
            return;
        };
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(sd) = eng.socks.get_mut(&skey) else {
            return;
        };
        (skey, sd.sock_id, sd.flags, sd.url.clone(), sd.web.clone())
    };
    let (skey, sock_id, flags, url, web) = work;
    if flags & HTTP_SOCKET_USE_PROXY != 0 {
        // tunnel CONNECT — la requête part en clair, la réponse est
        // accumulée dans `https_proxy_reply` (branche 2F).
        let Some(u) = url.as_ref() else { return };
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let connect_str = get_connect_str(eng, u);
        if let Some(w) = &web {
            w.msg(1, "Tunnel secure connection through proxy...");
        }
        chain.bfcb(
            OP_SEND,
            info,
            ChainData::Int(sock_id),
            ChainData::Str("FD".to_string()),
        );
        if let Some(eng) = chain.module_mut::<HttpEngine>() {
            if let Some(sd) = eng.socks.get_mut(&skey) {
                sd.https_proxy_reply = Some(Dstr::new(&[]));
            }
        }
        let len = i32::try_from(connect_str.len()).unwrap_or(i32::MAX);
        chain.bcb(
            OP_SEND,
            info,
            ChainData::Buf(Box::new(DataBuf::new(connect_str, len, 0))),
            ChainData::None,
        );
    } else {
        if let Some(w) = &web {
            w.msg(1, "Secure connection negotiation...");
        }
        tls_wrap(chain, skey);
    }
}

/// `a_Tls_connect(fd, url)` — `take_socket` → `TlsEngine::connect` →
/// `swap_socket` dans `IoEngine` (≙ même fd re-wrappé ; les ios armés
/// reçoivent `IoMsg::Swap`) → `begin_handshake`.
/// `Err` ≙ `success = FALSE` amont → `reset_server_state` +
/// `connect_done(FALSE)`.
fn tls_wrap(chain: &mut Chain, skey: i32) {
    let (sock_id, url) = {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(sd) = eng.socks.get(&skey) else {
            return;
        };
        (sd.sock_id, sd.url.clone())
    };
    let Some(mut url) = url else { return };
    let plain = chain
        .module_mut::<IoEngine>()
        .and_then(|e| e.take_socket(sock_id));
    let Some(stream) = plain else {
        connect_failed_sock(chain, skey, sock_id);
        return;
    };
    let res = chain
        .module_mut::<TlsEngine>()
        .map(|t| t.connect(sock_id, stream, &mut url));
    if let Some(Ok(tls_stream)) = res {
        let IoStream::Tls(arc) = &tls_stream else {
            return;
        };
        let arc = arc.clone();
        if let Some(eng) = chain.module_mut::<IoEngine>() {
            eng.swap_socket(sock_id, tls_stream);
        }
        if let Some(tls) = chain.module_mut::<TlsEngine>() {
            tls.begin_handshake(sock_id, &arc);
        }
    } else {
        // `success = FALSE` → reset_server_state + connect_done(FALSE)
        if let Some(tls) = chain.module_mut::<TlsEngine>() {
            tls.reset_server_state(&mut url);
        }
        connect_done(chain, sock_id, false);
    }
}

/// Chemin d'échec `connect_done(fd, FALSE)` quand le `fd` n'a jamais
/// été enregistré dans `fd_map` (≙ `dClose` impossible → skey direct).
fn connect_failed_sock(chain: &mut Chain, skey: i32, sock_id: i32) {
    let work = {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        eng.sd(skey).map(|sd| (sd.info, sd.web.clone()))
    };
    let Some((info, web)) = work else { return };
    if let Some(w) = &web {
        if w.valid() {
            w.msg(1, "Could not establish connection.");
        }
    }
    if sock_id != -1 {
        if let Some(e) = chain.module_mut::<IoEngine>() {
            e.remove_socket(sock_id);
        }
    }
    socket_free(chain, skey);
    if let Some(info) = info {
        chain.bfcb(
            OP_ABORT,
            info,
            ChainData::None,
            ChainData::Str("Both".to_string()),
        );
        chain.free_link(info);
    }
}

/// `Http_connect_socket ran out of IP addrs` → `connect_done(fd,FALSE)`
/// — skey connu via `info->LocalKey` (pas d'entrée `fd_map`).
fn connect_failed(chain: &mut Chain, info: LinkId) {
    let Some(skey) = chain.local_key::<i32>(info).copied() else {
        return;
    };
    connect_failed_sock(chain, skey, -1);
}

/// `a_Http_connect_done` — `fd_map[fd]` → skey ; succès+web valide →
/// `"FD"` + `send_query` ; sinon `dClose` + free + `OpAbort Both`.
fn connect_done(chain: &mut Chain, sock_id: i32, success: bool) {
    let work = {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(&skey) = eng.fd_map.get(&sock_id) else {
            debug!(sock_id, "connect_done: no fme/sd");
            return;
        };
        eng.sd(skey).map(|sd| (skey, sd.info, sd.web.clone()))
    };
    let Some((skey, info, web)) = work else {
        return;
    };
    let valid_web = web.as_ref().is_some_and(|w| w.valid());
    if success && valid_web {
        if let Some(info) = info {
            chain.bfcb(
                OP_SEND,
                info,
                ChainData::Int(sock_id),
                ChainData::Str("FD".to_string()),
            );
        }
        send_query(chain, skey);
    } else {
        if valid_web {
            if let Some(w) = &web {
                w.msg(1, "Could not establish connection.");
            }
        }
        debug!(sock_id, "fd is done and failed");
        if let Some(e) = chain.module_mut::<IoEngine>() {
            e.remove_socket(sock_id); // dClose(fd) amont
        }
        socket_free(chain, skey);
        if let Some(info) = info {
            chain.bfcb(
                OP_ABORT,
                info,
                ChainData::None,
                ChainData::Str("Both".to_string()),
            );
            chain.free_link(info);
        }
    }
}

/// `Http_send_query` — construit la requête et la pousse `bcb` (IO 1B).
fn send_query(chain: &mut Chain, skey: i32) {
    let work = {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(sd) = eng.sd(skey) else { return };
        let Some(web) = sd.web.clone() else { return };
        let use_proxy = sd.flags & HTTP_SOCKET_USE_PROXY != 0;
        let query = make_query_str(eng, &web, use_proxy, sd.flags & HTTP_SOCKET_TLS != 0);
        (sd.info, web, query, eng.trace_http, use_proxy)
    };
    let (info, web, query, trace, use_proxy) = work;
    web.msg(
        1,
        if use_proxy {
            "Sending query through proxy..."
        } else {
            "Sending query..."
        },
    );
    if trace {
        debug!(query = %String::from_utf8_lossy(&query), ">>> sending HTTP");
    }
    if let Some(info) = info {
        let len = i32::try_from(query.len()).unwrap_or(i32::MAX);
        chain.bcb(
            OP_SEND,
            info,
            ChainData::Buf(Box::new(DataBuf::new(query, len, 0))),
            ChainData::None,
        );
    }
}

/// `a_Tls_connect` post-`2xx` CONNECT — wrap + handshake sur le fd
/// du tunnel (≙ `a_Tls_connect(sd->SockFD, sd->url)` amont).
fn tunnel_tls(chain: &mut Chain, skey: i32) {
    tls_wrap(chain, skey);
}

/// `Http_dns_cb` — succès → `addr_list` + `server_get` + enqueue +
/// `connect_queued_sockets` ; échec/web invalide → cleanup + `OpAbort`.
fn http_dns_cb(chain: &mut Chain, status: i32, addr_list: Option<Vec<IpAddr>>, skey: i32) {
    enum Path {
        Enqueue((String, u16, bool)),
        Abort(LinkId),
        None,
    }
    let path = {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(sd) = eng.socks.get_mut(&skey) else {
            return;
        };
        let use_proxy = sd.flags & HTTP_SOCKET_USE_PROXY != 0;
        let mut host_url = if use_proxy {
            eng.proxy.clone()
        } else {
            sd.url.clone()
        };
        let host = host_url
            .as_mut()
            .and_then(|u| {
                u.hostname()
                    .map(|h| String::from_utf8_lossy(h).into_owned())
            })
            .unwrap_or_default();
        let web_ok = sd.web.as_ref().is_some_and(|w| w.valid());
        if web_ok && status == 0 && addr_list.is_some() {
            sd.addr_list = addr_list.unwrap_or_default();
            let srv_key = (host, sd.connect_port, sd.flags & HTTP_SOCKET_TLS != 0);
            Path::Enqueue(srv_key)
        } else {
            if web_ok {
                if let Some(w) = &sd.web {
                    w.msg(0, &format!("ERROR: DNS can't resolve {host}"));
                }
            }
            match sd.info {
                Some(info) => Path::Abort(info),
                None => Path::None,
            }
        }
    };
    match path {
        Path::Enqueue(srv_key) => {
            {
                let Some(eng) = chain.module_mut::<HttpEngine>() else {
                    return;
                };
                eng.server_get(&srv_key.0, srv_key.1, srv_key.2);
            }
            socket_enqueue(chain, &srv_key, skey);
            connect_queued_sockets(chain, &srv_key);
        }
        Path::Abort(info) => {
            socket_free(chain, skey);
            chain.bfcb(
                OP_ABORT,
                info,
                ChainData::None,
                ChainData::Str("Both".to_string()),
            );
            chain.free_link(info);
        }
        Path::None => {
            socket_free(chain, skey);
        }
    }
}

/// `Http_get` — enregistre `web`, décide proxy/TLS, lance le DNS
/// (≙ `a_Dns_resolve(hostname, Http_dns_cb, SKey)`).
fn http_get(chain: &mut Chain, info: LinkId, d1: ChainData) {
    let work = {
        let Some(skey) = chain.local_key::<i32>(info).copied() else {
            return;
        };
        let ChainData::Owned(any) = d1 else {
            return;
        };
        let Ok(web) = any.downcast::<HttpWeb>() else {
            return;
        };
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        // décision proxy + cible calculées AVANT l'emprunt mutable
        let mut target_url = web.url.clone();
        let use_proxy = {
            let mut h = target_url.clone();
            let host = h.hostname().map(<[u8]>::to_vec).unwrap_or_default();
            must_use_proxy(eng, &host)
        };
        if use_proxy {
            if let Some(p) = &eng.proxy {
                target_url = p.clone();
            }
        }
        let mut t2 = target_url.clone();
        let hostname = t2
            .hostname()
            .map(|h| String::from_utf8_lossy(h).into_owned());
        let port = t2.port();
        let scheme_is_https = web
            .url
            .scheme
            .as_deref()
            .is_some_and(|s| dlib::str_ascii_casecmp(s, b"https") == 0);
        let Some(sd) = eng.socks.get_mut(&skey) else {
            return;
        };
        sd.web = Some(web.clone());
        sd.info = Some(info);
        if use_proxy {
            sd.flags |= HTTP_SOCKET_USE_PROXY;
        }
        sd.connect_port = u16::try_from(port).unwrap_or(0);
        sd.url = Some(web.url.clone());
        if scheme_is_https {
            sd.flags |= HTTP_SOCKET_TLS;
        }
        (skey, web, hostname)
    };
    let (skey, web, hostname) = work;
    let Some(hostname) = hostname else { return };
    web.msg(1, &format!("DNS resolving {hostname}"));
    dns_resolve(
        chain,
        &hostname,
        Box::new(move |ch, status, addrs| http_dns_cb(ch, status, addrs, skey)),
    );
}

/// `Http_socket_reuse` — keep-alive : un socket en file reprend le fd
/// (≙ `fd` transféré, `active_conns` déplacé, `connect_done(TRUE)`).
fn socket_reuse(chain: &mut Chain, skey: i32) {
    enum Reuse {
        /// Pas de connexion/serveur ou aucun compatible → free + dClose.
        Drop(i32),
        /// `new_skey` reprend `old_sock` (fd + compteur serveur).
        Transfer {
            srv_key: (String, u16, bool),
            old_sock: i32,
            new_skey: i32,
        },
    }
    let decision = {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(old) = eng.sd(skey) else { return };
        let Some(host) = old.connected_to.clone() else {
            return;
        };
        let srv_key = (host, old.connect_port, old.flags & HTTP_SOCKET_TLS != 0);
        let old_sock = old.sock_id;
        let mut pick: Option<i32> = None;
        if let Some(srv) = eng.servers.get(&srv_key) {
            for &cand in &srv.queue {
                let Some(new) = eng.socks.get(&cand) else {
                    continue;
                };
                if new.flags & HTTP_SOCKET_TO_BE_FREED == 0 && reuse_compatible(old, new) {
                    pick = Some(cand);
                    break;
                }
            }
        }
        if let Some(new_skey) = pick {
            Reuse::Transfer {
                srv_key,
                old_sock,
                new_skey,
            }
        } else {
            Reuse::Drop(old_sock)
        }
    };
    let Reuse::Transfer {
        srv_key,
        old_sock,
        new_skey,
    } = decision
    else {
        let Reuse::Drop(fd) = decision else {
            return;
        };
        drop_fd_and_free(chain, skey, fd);
        return;
    };
    {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        if let Some(sd) = eng.socks.get_mut(&new_skey) {
            sd.sock_id = old_sock;
        }
        if let Some(sd) = eng.socks.get_mut(&skey) {
            sd.connected_to = None;
        }
        if let Some(srv) = eng.servers.get_mut(&srv_key) {
            srv.active_conns -= 1;
        }
    }
    socket_free(chain, skey); // fd_map[old_sock] retiré ici (amont)
    socket_activate(chain, &srv_key, new_skey);
    {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        eng.fd_map.insert(old_sock, new_skey); // fd_map_add_entry(new)
    }
    debug!(fd = old_sock, "Reusing fd");
    connect_done(chain, old_sock, true);
}

/// Fin du `Http_socket_reuse` amont : `Http_socket_free` puis
/// `dClose(old_fd)` (ordre verbatim — libérer avant de fermer).
fn drop_fd_and_free(chain: &mut Chain, skey: i32, sock_id: i32) {
    socket_free(chain, skey);
    if sock_id != -1 {
        if let Some(e) = chain.module_mut::<IoEngine>() {
            e.remove_socket(sock_id);
        }
    }
}

/// Réponse `Connected` — `IOWATCH` off, `fd_map` rempli, puis
/// `connect_tls` ou `connect_done(TRUE)`.
fn on_connected(chain: &mut Chain, skey: i32, stream: TcpStream) {
    let (info, is_tls) = {
        let sock_id = chain
            .module_mut::<IoEngine>()
            .map_or(-1, |e| e.insert_socket(IoStream::Plain(stream)));
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(sd) = eng.socks.get_mut(&skey) else {
            // socket libérée pendant le connect → drop le stream (dClose)
            if let Some(e) = chain.module_mut::<IoEngine>() {
                e.remove_socket(sock_id);
            }
            return;
        };
        sd.flags &= !HTTP_SOCKET_IOWATCH_ACTIVE;
        sd.connect_abort = None;
        sd.sock_id = sock_id;
        eng.fd_map.insert(sock_id, skey); // Http_fd_map_add_entry
        (sd.info, sd.flags & HTTP_SOCKET_TLS != 0)
    };
    if is_tls {
        if let Some(info) = info {
            connect_tls(chain, info);
        }
    } else {
        let sock_id = chain
            .module_mut::<HttpEngine>()
            .and_then(|eng| eng.sd(skey).map(|s| s.sock_id))
            .unwrap_or(-1);
        connect_done(chain, sock_id, true);
    }
}

/// `TlsEvent::Handshook` — `connecting = FALSE`, log cipher si
/// `RECEIVING`, `Tls_check_certificate` (`USER_ACCEPTED` court-circuite
/// l'examen, verbatim), puis `connect_done`.
fn on_handshook(chain: &mut Chain, sock_id: i32, has_cert: bool, cipher: Option<&str>) {
    {
        if let Some(tls) = chain.module_mut::<TlsEngine>() {
            tls.mark_established(sock_id);
        }
    }
    let url = {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(&skey) = eng.fd_map.get(&sock_id) else {
            debug!(sock_id, "handshook: no fme");
            return;
        };
        eng.sd(skey).and_then(|s| s.url.clone())
    };
    let Some(mut url) = url else {
        connect_done(chain, sock_id, false);
        return;
    };
    let status = {
        let Some(tls) = chain.module_mut::<TlsEngine>() else {
            return;
        };
        tls.cert_status(&mut url)
    };
    // ≙ `cert_status == RECEIVING` → log "<authority> <cipher>"
    if status == CERT_STATUS_RECEIVING {
        let auth = url
            .authority
            .as_deref()
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .unwrap_or_default();
        debug!(host = %auth, cipher = ?cipher, "TLS established");
    }
    let accepted = {
        let Some(tls) = chain.module_mut::<TlsEngine>() else {
            return;
        };
        status == CERT_STATUS_USER_ACCEPTED || tls.after_handshake(&mut url, has_cert)
    };
    if accepted {
        connect_done(chain, sock_id, true);
    } else {
        // `Tls_close_by_key` (connecting déjà false → close_notify) +
        // `connect_done(fd, FALSE)` → dClose + free + abort
        {
            let connecting = chain
                .module_mut::<TlsEngine>()
                .and_then(|t| t.close_by_fd(sock_id));
            if let Some(eng) = chain.module_mut::<IoEngine>() {
                match connecting {
                    Some(true) => eng.remove_socket(sock_id),
                    Some(false) => eng.close_notify(sock_id),
                    None => {}
                }
            }
        }
        connect_done(chain, sock_id, false);
    }
}

/// `TlsEvent::Failed` ≙ `Tls_handshake` `failed` → `close_by_key` +
/// `connect_done(FALSE)` (même ordre amont : connecting false d'abord).
fn on_tls_failed(chain: &mut Chain, sock_id: i32) {
    {
        if let Some(tls) = chain.module_mut::<TlsEngine>() {
            tls.mark_established(sock_id);
        }
        let connecting = chain
            .module_mut::<TlsEngine>()
            .and_then(|t| t.close_by_fd(sock_id));
        if let Some(eng) = chain.module_mut::<IoEngine>() {
            match connecting {
                Some(true) => eng.remove_socket(sock_id),
                Some(false) => eng.close_notify(sock_id),
                None => {}
            }
        }
    }
    connect_done(chain, sock_id, false);
}

/// Pompe `poll_http` — événements `connect_task` + handshake TLS
/// (≙ `IOwatch` callbacks `Http_connect_socket_cb`/`Tls_handshake_cb`).
pub fn poll_http(chain: &mut Chain) {
    let events = chain
        .module_mut::<HttpEngine>()
        .map_or_else(Vec::new, HttpEngine::drain_events);
    for ev in events {
        match ev {
            HttpEvent::Connected { skey, stream } => on_connected(chain, skey, stream),
            // ≙ `connect_done(fd, FALSE)` — la tâche est morte ;
            // `socket_free` retire `IOWATCH_ACTIVE` et nettoie.
            HttpEvent::ConnectFailed { skey } => {
                connect_failed_sock(chain, skey, -1);
            }
        }
    }
    let tls_events = chain
        .module_mut::<TlsEngine>()
        .map_or_else(Vec::new, TlsEngine::drain_events);
    for ev in tls_events {
        match ev {
            TlsEvent::Handshook {
                sock_id,
                has_cert,
                cipher,
            } => on_handshook(chain, sock_id, has_cert, cipher.as_deref()),
            TlsEvent::Failed { sock_id } => on_tls_failed(chain, sock_id),
        }
    }
}

/// `a_Http_ccc` — dispatcher verbatim (1B query / 1F status /
/// 2F réponses / 2B `"FD"`+`"reply_complete"`).
// forme dispatcher amont
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub fn http_ccc(
    chain: &mut Chain,
    op: i32,
    branch: i32,
    dir: i32,
    info: LinkId,
    d1: ChainData,
    d2: ChainData,
) {
    if !chain.check("a_Http_ccc", op, branch, dir, info) {
        return;
    }
    let skey = chain.local_key::<i32>(info).copied().unwrap_or(-1);
    match (branch, dir) {
        (1, BCK) => match op {
            OP_START => {
                // Data1 = Web
                let skey = {
                    let Some(eng) = chain.module_mut::<HttpEngine>() else {
                        return;
                    };
                    eng.sock_new()
                };
                chain.set_local_key(info, skey);
                chain.link_new(info, http_ccb(), BCK, io_ccb(), 1, 1);
                chain.bcb(OP_START, info, ChainData::None, ChainData::None);
                http_get(chain, info, d1);
            }
            OP_END => {
                chain.bcb(OP_END, info, ChainData::None, ChainData::None);
                chain.free_link(info);
            }
            OP_ABORT => {
                socket_free(chain, skey);
                chain.bcb(OP_ABORT, info, ChainData::None, ChainData::None);
                chain.free_link(info);
            }
            _ => warn!("Unused CCC HTTP 1B"),
        },
        (1, FWD) => {
            if op == OP_ABORT {
                if let Some(eng) = chain.module_mut::<HttpEngine>() {
                    if let Some(sd) = eng.sd(skey) {
                        let mut u = sd.url.clone();
                        if let (Some(w), Some(u)) = (&sd.web, &mut u) {
                            let s = String::from_utf8_lossy(u.str_()).into_owned();
                            w.msg(1, &format!("Can't get {s}"));
                        }
                    }
                }
                socket_free(chain, skey);
                chain.fcb(
                    OP_ABORT,
                    info,
                    ChainData::None,
                    ChainData::Str("Both".to_string()),
                );
                chain.free_link(info);
            } else {
                warn!("Unused CCC HTTP 1F");
            }
        }
        (2, FWD) => match op {
            OP_SEND => {
                let has_reply = chain
                    .module_mut::<HttpEngine>()
                    .and_then(|e| e.sd(skey))
                    .is_some_and(|sd| sd.https_proxy_reply.is_some());
                if has_reply {
                    proxy_reply_recv(chain, info, skey, &d1);
                } else {
                    // Data1 = dbuf → upstream (cache)
                    chain.fcb(
                        OP_SEND,
                        info,
                        d1,
                        ChainData::Str("send_page_2eof".to_string()),
                    );
                }
            }
            OP_END => {
                let has_reply = chain
                    .module_mut::<HttpEngine>()
                    .and_then(|e| e.sd(skey))
                    .is_some_and(|sd| sd.https_proxy_reply.is_some());
                if has_reply {
                    debug!("CONNECT through proxy failed. Full reply not received");
                    socket_free(chain, skey);
                    chain.bfcb(
                        OP_ABORT,
                        info,
                        ChainData::None,
                        ChainData::Str("Both".to_string()),
                    );
                } else {
                    socket_free(chain, skey);
                    chain.fcb(OP_END, info, ChainData::None, ChainData::None);
                }
                chain.free_link(info);
            }
            OP_ABORT => {
                socket_free(chain, skey);
                chain.fcb(
                    OP_ABORT,
                    info,
                    ChainData::None,
                    ChainData::Str("Both".to_string()),
                );
                chain.free_link(info);
            }
            _ => warn!("Unused CCC HTTP 2F"),
        },
        (2, BCK) => match op {
            OP_START => {
                chain.link_new(info, http_ccb(), BCK, io_ccb(), 2, 2);
                chain.bcb(OP_START, info, ChainData::None, ChainData::None);
            }
            OP_SEND => {
                if d2.is_tag("FD") {
                    // fd → skey via fd_map, puis relais vers IO 2B
                    if let ChainData::Int(fd) = &d1 {
                        let sk = chain
                            .module_mut::<HttpEngine>()
                            .and_then(|e| e.fd_map.get(fd).copied());
                        if let Some(sk) = sk {
                            chain.set_local_key(info, sk);
                        }
                        chain.bcb(OP_SEND, info, d1, d2);
                    }
                } else if d2.is_tag("reply_complete") {
                    chain.bfcb(OP_END, info, ChainData::None, ChainData::None);
                    socket_reuse(chain, skey);
                    chain.free_link(info);
                }
            }
            OP_ABORT => {
                socket_free(chain, skey);
                chain.bcb(OP_ABORT, info, ChainData::None, ChainData::None);
                chain.free_link(info);
            }
            _ => warn!("Unused CCC HTTP 2B"),
        },
        _ => {}
    }
}

/// `a_Http_ccc` en `ChainCb` prête à enregistrer.
#[must_use]
pub fn http_ccb() -> ChainCb {
    Rc::new(|ch, op, branch, dir, info, d1, d2| {
        http_ccc(ch, op, branch, dir, info, d1, d2);
    })
}

/// 2F/`OpSend` avec `https_proxy_reply` — accumule la réponse CONNECT
/// jusqu'à `"\r\n\r\n"`, vérifie le `2` de statut à l'index 9
/// (verbatim), puis `a_Tls_connect` ou abort.
fn proxy_reply_recv(chain: &mut Chain, info: LinkId, skey: i32, d1: &ChainData) {
    enum Outcome {
        Ok,
        Fail,
        Wait,
    }
    let outcome = {
        let Some(eng) = chain.module_mut::<HttpEngine>() else {
            return;
        };
        let Some(sd) = eng.socks.get_mut(&skey) else {
            return;
        };
        let Some(reply) = sd.https_proxy_reply.as_mut() else {
            return;
        };
        if let ChainData::Buf(dbuf) = d1 {
            // `dStr_append_l(reply, dbuf->Buf, dbuf->Size)` amont —
            // `Size` borne (peut différer de `buf.len`).
            let n = usize::try_from(dbuf.size).unwrap_or(0).min(dbuf.buf.len());
            reply.append(&dbuf.buf[..n]);
        }
        let bytes = reply.as_bytes();
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            // "HTTP/1.1 200 …" — statut à l'index 9 (verbatim len>=12)
            if bytes.len() >= 12 && bytes[9] == b'2' {
                debug!("CONNECT through proxy succeeded");
                Outcome::Ok
            } else {
                Outcome::Fail
            }
        } else {
            Outcome::Wait
        }
    };
    match outcome {
        Outcome::Wait => {}
        Outcome::Ok => {
            {
                let Some(eng) = chain.module_mut::<HttpEngine>() else {
                    return;
                };
                if let Some(sd) = eng.socks.get_mut(&skey) {
                    sd.https_proxy_reply = None;
                    if let Some(w) = &sd.web {
                        w.msg(1, "Secure connection negotiation...");
                    }
                }
            }
            tunnel_tls(chain, skey);
        }
        Outcome::Fail => {
            {
                let Some(eng) = chain.module_mut::<HttpEngine>() else {
                    return;
                };
                if let Some(sd) = eng.socks.get_mut(&skey) {
                    if let Some(w) = &sd.web {
                        let mut u = w.url.clone();
                        let h = u
                            .hostname()
                            .map(|x| String::from_utf8_lossy(x).into_owned())
                            .unwrap_or_default();
                        w.msg(1, &format!("Can't connect through proxy to {h}"));
                    }
                }
            }
            debug!("CONNECT through proxy failed");
            socket_free(chain, skey);
            chain.bfcb(
                OP_ABORT,
                info,
                ChainData::None,
                ChainData::Str("Both".to_string()),
            );
            chain.free_link(info);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::Dns;
    use crate::url::url_new;

    fn eng(rt: &tokio::runtime::Handle) -> HttpEngine {
        HttpEngine::new(rt.clone(), HttpConfig::default())
    }

    fn mk_url(s: &str) -> DilloUrl {
        url_new(s.as_bytes(), None).expect("url parse")
    }

    /// Chain + `HttpEngine` (DNS sync, pas d'`Io`/`Tls` sauf besoin).
    fn chain_with_eng(rt: &tokio::runtime::Handle) -> Chain {
        let mut ch = Chain::new();
        ch.set_module(eng(rt));
        ch
    }

    #[test]
    fn must_use_proxy_suffix_and_empty_token_quirk() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let mut e = eng(rt.handle());
        e.proxy = Some(mk_url("http://proxy:3128/"));
        assert!(must_use_proxy(&e, b"www.example.com"));

        e.no_proxy = Some("example.com".to_string());
        assert!(!must_use_proxy(&e, b"www.example.com")); // suffixe
        assert!(!must_use_proxy(&e, b"example.com")); // exact
                                                      // quirk amont : PAS de frontière '.' — "notexample.com" finit
                                                      // littéralement par "example.com" → matche aussi (verbatim).
        assert!(!must_use_proxy(&e, b"www.notexample.com"));
        assert!(must_use_proxy(&e, b"example.com.evil.net")); // pas suffixe

        // quirk amont : token VIDE (double espace) matche tout —
        // `dStrsep` ne saute pas les délimiteurs consécutifs.
        e.no_proxy = Some("example.com  foo.net".to_string());
        assert!(!must_use_proxy(&e, b"anything.what"));

        // pas de proxy → toujours false
        e.no_proxy = None;
        e.proxy = None;
        assert!(!must_use_proxy(&e, b"www.example.com"));
    }

    #[test]
    fn make_query_str_get_golden() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let e = eng(rt.handle());
        let web = HttpWeb::new(
            mk_url("http://example.com/dir/page?q=1#frag"),
            None,
            WEB_ROOT_URL,
        );
        let q = make_query_str(&e, &web, false, false);
        let s = String::from_utf8(q).expect("utf8");
        let expected = concat!(
            "GET /dir/page?q=1 HTTP/1.1\r\n",
            "Host: example.com\r\n",
            "User-Agent: Dillo/3.3.0-rc1\r\n",
            "Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8\r\n",
            "Accept-Encoding: gzip, deflate, br\r\n",
            "DNT: 1\r\n",
            "Referer: http://example.com/\r\n",
            "Connection: keep-alive\r\n",
            "\r\n"
        );
        assert_eq!(s, expected);
    }

    #[test]
    fn make_query_str_accepts_and_referer() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let cfg = HttpConfig {
            http_referer: "path".to_string(),
            ..HttpConfig::default()
        };
        let e = HttpEngine::new(rt.handle().clone(), cfg);

        let web = HttpWeb::new(mk_url("http://a.b/p/x.png"), None, WEB_IMAGE);
        let s = String::from_utf8(make_query_str(&e, &web, false, false)).expect("u");
        assert!(s.contains("Accept: image/png,image/*;q=0.8,*/*;q=0.5\r\n"));
        assert!(s.contains("Referer: http://a.b/p/x.png\r\n"));

        let web = HttpWeb::new(mk_url("http://a.b/s.css"), None, WEB_STYLESHEET);
        let s = String::from_utf8(make_query_str(&e, &web, false, false)).expect("u");
        assert!(s.contains("Accept: text/css,*/*;q=0.1\r\n"));
    }

    #[test]
    fn make_query_str_proxy_absolute_uri() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let mut e = eng(rt.handle());
        e.proxy = Some(mk_url("http://proxy:3128/"));
        e.set_proxy_passwd("pw");
        let web = HttpWeb::new(mk_url("http://example.com/"), None, WEB_ROOT_URL);
        let s = String::from_utf8(make_query_str(&e, &web, true, false)).expect("u");
        assert!(s.starts_with("GET http://example.com/ HTTP/1.1\r\n"));
        assert!(s.contains("Proxy-Authorization: Basic"));
    }

    #[test]
    fn make_query_str_post_and_content_type() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let e = eng(rt.handle());
        let mut u = mk_url("http://example.com/post");
        u.flags |= url_flags::POST;
        u.data = b"a=1&b=2".to_vec();
        let web = HttpWeb::new(u, None, 0);
        let s = make_query_str(&e, &web, false, false);
        let txt = String::from_utf8_lossy(&s);
        assert!(txt.starts_with("POST /post HTTP/1.1\r\n"));
        assert!(txt.contains("Content-Type: application/x-www-form-urlencoded\r\n"));
        assert!(txt.contains("Content-Length: 7\r\n"));
        assert!(s.ends_with(b"\r\na=1&b=2"));

        // multipart : boundary extrait de `data[2..'\r')` (verbatim)
        let mut u = mk_url("http://example.com/up");
        u.flags |= url_flags::POST | url_flags::MULTIPART_ENC;
        u.data = b"--BOUNDXYZ\r\nrest".to_vec();
        let web = HttpWeb::new(u, None, 0);
        let txt = String::from_utf8_lossy(&make_query_str(&e, &web, false, false)).into_owned();
        assert!(txt.contains("Content-Type: multipart/form-data; boundary=\"BOUNDXYZ\"\r\n"));
    }

    #[test]
    fn get_connect_str_golden() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let mut e = eng(rt.handle());
        e.proxy = Some(mk_url("http://proxy:3128/"));
        // authority sans port → ":443" ajouté (verbatim)
        let s = String::from_utf8(get_connect_str(&e, &mk_url("https://ex.com/a"))).expect("u");
        assert_eq!(s, "CONNECT ex.com:443 HTTP/1.1\r\nHost: ex.com:443\r\n\r\n");
        // avec port explicite
        let s =
            String::from_utf8(get_connect_str(&e, &mk_url("https://ex.com:8443/a"))).expect("u");
        assert_eq!(
            s,
            "CONNECT ex.com:8443 HTTP/1.1\r\nHost: ex.com:8443\r\n\r\n"
        );
    }

    /// Fabrique un `SocketData` minimal pour `skey` (web attaché) et
    /// laisse le flag `QUEUED` NON posé (`socket_enqueue` l'assert).
    fn mk_sock(ch: &mut Chain, skey: i32, web_flags: u32, valid: bool) {
        let url = mk_url("http://h.local/");
        let web = Rc::new(HttpWeb::new(url, None, web_flags));
        if !valid {
            web.invalidate();
        }
        let eng = ch.module_mut::<HttpEngine>().expect("eng");
        let sd = eng.socks.get_mut(&skey).expect("sd");
        sd.web = Some(web);
    }

    #[test]
    fn enqueue_prioritizes_pages_over_images() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let mut ch = chain_with_eng(rt.handle());
        let srv_key = ("h.local".to_string(), 80u16, false);
        {
            let e = ch.module_mut::<HttpEngine>().expect("e");
            e.server_get("h.local", 80, false);
        }
        let mut keys = Vec::new();
        for _ in 0..4 {
            keys.push(ch.module_mut::<HttpEngine>().expect("e").sock_new());
        }
        let [i1, p1, i2, p2] = [keys[0], keys[1], keys[2], keys[3]];
        // file : image, page, image, page → les non-images passent
        // devant le premier WEB_Image valide (verbatim)
        for (k, f) in [
            (i1, WEB_IMAGE),
            (p1, WEB_ROOT_URL),
            (i2, WEB_IMAGE),
            (p2, 0u32),
        ] {
            mk_sock(&mut ch, k, f, true);
            socket_enqueue(&mut ch, &srv_key, k);
        }
        let e = ch.module_mut::<HttpEngine>().expect("e");
        let q = &e.servers[&srv_key].queue;
        assert_eq!(q, &vec![p1, p2, i1, i2]);
        for &k in q {
            assert!(e.socks[&k].flags & HTTP_SOCKET_QUEUED != 0);
        }
    }

    #[test]
    fn socket_free_queued_makes_zombie_then_sweep() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let mut ch = chain_with_eng(rt.handle());
        let srv_key = ("h.local".to_string(), 80u16, false);
        {
            let e = ch.module_mut::<HttpEngine>().expect("e");
            e.server_get("h.local", 80, false);
        }
        let k = ch.module_mut::<HttpEngine>().expect("e").sock_new();
        mk_sock(&mut ch, k, WEB_IMAGE, true);
        socket_enqueue(&mut ch, &srv_key, k);
        socket_free(&mut ch, k);
        {
            let e = ch.module_mut::<HttpEngine>().expect("e");
            let sd = e.socks.get(&k).expect("zombie reste");
            assert!(sd.flags & HTTP_SOCKET_TO_BE_FREED != 0);
            assert!(sd.url.is_none());
            assert!(e.sd(k).is_none(), "zombie invisible via sd()");
        }
        // balayage par la file → zombie retiré + serveur supprimé
        connect_queued_sockets(&mut ch, &srv_key);
        let e = ch.module_mut::<HttpEngine>().expect("e");
        assert!(!e.socks.contains_key(&k));
        assert!(!e.servers.contains_key(&srv_key));
    }

    #[test]
    fn reuse_compatible_rules() {
        let old = SocketData {
            sock_id: 7,
            flags: 0,
            web: None,
            url: Some(mk_url("http://a.b/")),
            addr_list: vec![],
            info: None,
            connected_to: Some("a.b".into()),
            connect_port: 80,
            https_proxy_reply: None,
            connect_abort: None,
        };
        let new = SocketData {
            sock_id: -1,
            flags: HTTP_SOCKET_QUEUED,
            web: Some(Rc::new(HttpWeb::new(mk_url("http://c.d/"), None, 0))),
            url: Some(mk_url("http://c.d/")),
            addr_list: vec![],
            info: None,
            connected_to: None,
            connect_port: 80,
            https_proxy_reply: None,
            connect_abort: None,
        };
        // old non-TLS → toujours compatible (web valide)
        assert!(reuse_compatible(&old, &new));
        // web invalide → false
        new.web.as_ref().expect("w").invalidate();
        assert!(!reuse_compatible(&old, &new));
    }

    /// Test wire complet : listener loopback, la requête GET doit
    /// arriver octet pour octet (DNS sync + connect tokio + io tasks).
    #[tokio::test(flavor = "current_thread")]
    async fn wire_end_to_end_get() {
        use crate::dns::poll_dns;
        use crate::io::{poll_io, IoEngine};
        use tokio::io::AsyncReadExt;
        use tokio::net::TcpListener;

        let rt = tokio::runtime::Handle::current();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            let Ok((mut s, _)) = listener.accept().await else {
                return;
            };
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => got.extend_from_slice(&buf[..n]),
                }
            }
            let _ = tx.send(got);
        });

        let mut ch = Chain::new();
        ch.set_module(Dns::new_sync());
        ch.set_module(IoEngine::new(rt.clone()));
        ch.set_module(eng(&rt));
        let url = mk_url(&format!("http://127.0.0.1:{port}/dir/x?a=b"));
        let web = Rc::new(HttpWeb::new(url, None, WEB_ROOT_URL));
        let info = ch.new_link();
        http_ccc(
            &mut ch,
            OP_START,
            1,
            BCK,
            info,
            ChainData::Owned(web as Rc<dyn std::any::Any>),
            ChainData::None,
        );
        let mut got = None;
        for _ in 0..200 {
            poll_dns(&mut ch);
            poll_http(&mut ch);
            poll_io(&mut ch);
            if let Ok(g) = rx.try_recv() {
                got = Some(g);
                break;
            }
            tokio::task::yield_now().await;
        }
        let got = got.expect("request must arrive");
        let s = String::from_utf8_lossy(&got);
        let expected_prefix = format!("GET /dir/x?a=b HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n");
        assert!(s.starts_with(&expected_prefix), "got: {s:?}");
        assert!(s.contains("User-Agent: Dillo/3.3.0-rc1\r\n"));
        assert!(s.contains("Connection: keep-alive\r\n"));
    }

    /// File + limite : `active_conns` ne dépasse pas `http_max_conns`,
    /// les extras restent `QUEUED`.
    #[tokio::test(flavor = "current_thread")]
    async fn queue_respects_max_conns() {
        use crate::dns::poll_dns;
        use crate::io::{poll_io, IoEngine};
        use tokio::net::TcpListener;

        let rt = tokio::runtime::Handle::current();
        let l1 = TcpListener::bind("127.0.0.1:0").await.expect("l1");
        let p1 = l1.local_addr().expect("a").port();
        // accepte en arrière-plan, garde la conn ouverte
        tokio::spawn(async move {
            let mut acc = Vec::new();
            for _ in 0..4 {
                if let Ok((s, _)) = l1.accept().await {
                    acc.push(s);
                }
            }
        });

        let mut ch = Chain::new();
        ch.set_module(Dns::new_sync());
        ch.set_module(IoEngine::new(rt.clone()));
        {
            let mut e = eng(&rt);
            e.max_conns = 1;
            ch.set_module(e);
        }
        // 3 requêtes sur le même serveur → 1 active, 2 en file
        for i in 0..3 {
            let url = mk_url(&format!("http://127.0.0.1:{p1}/r{i}"));
            let web = Rc::new(HttpWeb::new(url, None, WEB_ROOT_URL));
            let info = ch.new_link();
            http_ccc(
                &mut ch,
                OP_START,
                1,
                BCK,
                info,
                ChainData::Owned(web as Rc<dyn std::any::Any>),
                ChainData::None,
            );
        }
        for _ in 0..60 {
            poll_dns(&mut ch);
            poll_http(&mut ch);
            poll_io(&mut ch);
            tokio::task::yield_now().await;
        }
        let e = ch.module_mut::<HttpEngine>().expect("e");
        let srv = e
            .servers
            .get(&("127.0.0.1".to_string(), p1, false))
            .expect("srv");
        assert_eq!(srv.active_conns, 1);
        assert_eq!(srv.queue.len(), 2);
    }
}
