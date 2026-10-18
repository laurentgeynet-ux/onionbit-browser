//! Port de `src/cache.c`/`cache.h` — cache de requêtes, file de clients,
//! parsing de réponse HTTP et dispatch vers les viewers.
//!
//! Amont : trois listes globales (`CachedURLs` triée, `ClientQueue`,
//! `DelayedQueue`) mutées en réentrant depuis les callbacks. Ici : un
//! registre thread-local [`Cache`] derrière seams injectées
//! (`BrowserOps`/`UiOps`/`NavOps`/`DicacheOps`/`WebDispatch`) — `dillo-net`
//! ne connaît ni navigateur ni layout.
//!
//! Réentrance : les callbacks client amont peuvent rappeler le cache
//! (ex. `a_Capi_set_content_type` depuis un viewer). Le client concerné
//! est **détaché** de la file le temps de son callback — le `RefCell` est
//! relâché — puis réinséré ou supprimé selon le flot amont.

use std::cell::RefCell;
use std::rc::Rc;

use dillo_common::dlib;
use dillo_common::misc;

use crate::about;
use crate::auth::{auth_do_auth, AuthDialog};
use crate::cookies::{cookies_set, DpiSend};
use crate::decode::{Decode, DecodeTransfer};
use crate::domain::domain_permit;
use crate::hsts::hsts_set;
use crate::http::{HttpWeb, WEB_DOWNLOAD, WEB_ROOT_URL};
use crate::io::{IO_ABORT, IO_CLOSE, IO_READ};
use crate::timeout::{timeout_add, timeout_remove};
use crate::url::{flags as url_flags, host_type, url_new, DilloUrl, URL_HOST_NAME};

/// `CA_Send` — mise à jour normale.
pub const CA_SEND: i32 = 0;
/// `CA_Close` — clôture réussie.
pub const CA_CLOSE: i32 = 1;
/// `CA_Abort` — avortement.
pub const CA_ABORT: i32 = 2;

/// Flags `CA_*` (cache.h).
pub mod ca_flags {
    /// `CA_GotHeader`.
    pub const GOT_HEADER: u32 = 0x1;
    /// `CA_GotContentType`.
    pub const GOT_CONTENT_TYPE: u32 = 0x2;
    /// `CA_GotLength`.
    pub const GOT_LENGTH: u32 = 0x4;
    /// `CA_InProgress`.
    pub const IN_PROGRESS: u32 = 0x8;
    /// `CA_Redirect`.
    pub const REDIRECT: u32 = 0x10;
    /// `CA_ForceRedirect`.
    pub const FORCE_REDIRECT: u32 = 0x20;
    /// `CA_TempRedirect`.
    pub const TEMP_REDIRECT: u32 = 0x40;
    /// `CA_NotFound`.
    pub const NOT_FOUND: u32 = 0x80;
    /// `CA_Aborted`.
    pub const ABORTED: u32 = 0x100;
    /// `CA_MsgErased`.
    pub const MSG_ERASED: u32 = 0x200;
    /// `CA_RedirectLoop`.
    pub const REDIRECT_LOOP: u32 = 0x400;
    /// `CA_InternalUrl`.
    pub const INTERNAL_URL: u32 = 0x800;
    /// `CA_HugeFile`.
    pub const HUGE_FILE: u32 = 0x1000;
    /// `CA_IsEmpty`.
    pub const IS_EMPTY: u32 = 0x2000;
    /// `CA_KeepAlive`.
    pub const KEEP_ALIVE: u32 = 0x4000;
}
use ca_flags as ca;

/// `MAX_INIT_BUF` — taille initiale max du buffer auto-croissant.
const MAX_INIT_BUF: usize = 1024 * 1024;
/// `HUGE_FILESIZE` — au-delà, proposition de téléchargement.
const HUGE_FILESIZE: usize = 15 * 1024 * 1024;

/// `CA_Callback_t` — `(Op, Client)` amont ; le client est détaché de la
/// file pendant l'appel (il peut être réinséré ou dépilé ensuite).
pub type CaCb = Box<dyn FnMut(i32, &mut CacheClient)>;

/// `CacheClient_t` — un consommateur d'entrée de cache.
pub struct CacheClient {
    /// `Key` — clé primaire.
    pub key: i32,
    /// `Url` — l'entrée est retrouvée par `url_cmp` (amont : pointeur
    /// partagé vers `entry->Url`, unique par construction du cache).
    pub url: DilloUrl,
    /// `Version` — version dicache (0 si inutilisée).
    pub version: i32,
    /// `Buf`/`BufSize` — snapshot rempli avant `CA_Send` (amont :
    /// pointeur *live* dans `entry->Data` — équivalent, le callback lit
    /// de façon synchrone).
    pub buf: Vec<u8>,
    /// `Callback`.
    pub callback: Option<CaCb>,
    /// `Web` — partagé `Rc` avec le transport (≙ `DilloWeb*`
    /// amont) ; `a_Web_free` ≙ `invalidate` au dequeue.
    pub web: Rc<HttpWeb>,
}

/// `CacheEntry_t`.
struct CacheEntry {
    /// `Url` — clé primaire (`a_Url_dup` amont).
    url: DilloUrl,
    /// `TypeDet` — MIME détecté depuis les données.
    type_det: Option<Vec<u8>>,
    /// `TypeHdr` — MIME de l'en-tête HTTP.
    type_hdr: Option<Vec<u8>>,
    /// `TypeMeta` — MIME d'un META http-equiv.
    type_meta: Option<Vec<u8>>,
    /// `TypeNorm` — MIME normalisé.
    type_norm: Option<Vec<u8>>,
    /// `ContentDisposition`.
    content_disposition: Option<Vec<u8>>,
    /// `Header` — en-tête brut (CR/NUL strippés, lignes dépliées).
    header: Vec<u8>,
    /// `Location` — cible de redirection.
    location: Option<DilloUrl>,
    /// `Auth` — défis `WWW-Authenticate` (Dlist<char*> amont).
    auth: Option<Vec<String>>,
    /// `Data` — données décodées (transfer+content).
    data: Vec<u8>,
    /// `UTF8Data` — données après traduction charset (lazy/refcounted).
    utf8_data: Option<Vec<u8>>,
    /// `DataRefcount`.
    data_refcount: i32,
    /// `TransferDecoder` — chunked…
    transfer_decoder: Option<DecodeTransfer>,
    /// `ContentDecoder` — gzip/deflate/brotli.
    content_decoder: Option<Decode>,
    /// `CharsetDecoder` — vers UTF-8.
    charset_decoder: Option<Decode>,
    /// `ExpectedSize` — `Content-Length` (0 = inconnu).
    expected_size: usize,
    /// `TransferSize` — octets de corps reçus (bruts).
    transfer_size: usize,
    /// `Flags` — `CA_*`.
    flags: u32,
    /// `Hits`.
    hits: i32,
}

impl CacheEntry {
    /// `Cache_entry_init` — valeurs sûres.
    fn new(url: &DilloUrl) -> Self {
        Self {
            url: url.clone(),
            type_det: None,
            type_hdr: None,
            type_meta: None,
            type_norm: None,
            content_disposition: None,
            header: Vec::new(),
            location: None,
            auth: None,
            data: Vec::with_capacity(8 * 1024),
            utf8_data: None,
            data_refcount: 0,
            transfer_decoder: None,
            content_decoder: None,
            charset_decoder: None,
            expected_size: 0,
            transfer_size: 0,
            flags: ca::IS_EMPTY | ca::IN_PROGRESS | ca::KEEP_ALIVE,
            hits: 0,
        }
    }

    /// `Cache_current_content_type` — Norm > Meta > Hdr > Det.
    fn current_content_type(&self) -> Option<&[u8]> {
        self.type_norm
            .as_deref()
            .or(self.type_meta.as_deref())
            .or(self.type_hdr.as_deref())
            .or(self.type_det.as_deref())
    }

    /// `Cache_data` — `UTF8Data` si traduction active, sinon Data.
    fn data(&self) -> &[u8] {
        self.utf8_data.as_deref().unwrap_or(&self.data)
    }
}

/* ------------------------- seams injectées ------------------------- */

/// Seam `BrowserWindow` (`bw->…` + `a_Bw_*`).
pub trait BrowserOps {
    /// `a_Bw_remove_client`.
    fn remove_client(&mut self, bw: u32, key: i32);
    /// `a_Bw_close_client`.
    fn close_client(&mut self, bw: u32, key: i32);
    /// Lecture de `bw->redirect_level`.
    fn redirect_level(&mut self, bw: u32) -> i32;
    /// Écriture de `bw->redirect_level`.
    fn set_redirect_level(&mut self, bw: u32, level: i32);
}

/// Seam `a_UIcmd_*` utilisée par cache.c.
pub trait UiOps {
    /// `a_UIcmd_set_msg`.
    fn set_msg(&mut self, bw: u32, msg: &str);
    /// `a_UIcmd_set_page_prog` — `cmd` ≙ 3ᵉ arg amont (1 = réception, 0 = fin).
    fn set_page_prog(&mut self, bw: u32, bytes: usize, cmd: i32);
    /// `a_UIcmd_save_link` (offre de téléchargement).
    fn save_link(&mut self, bw: u32, url: &DilloUrl, filename: Option<&str>);
}

/// Seam `nav.c`.
pub trait NavOps {
    /// `a_Nav_push`.
    fn push(&mut self, bw: u32, url: DilloUrl, requester: &DilloUrl);
    /// `a_Nav_reload`.
    fn reload(&mut self, bw: u32);
    /// `a_Nav_cancel_expect_if_eq`.
    fn cancel_expect_if_eq(&mut self, bw: u32, url: &DilloUrl);
    /// `a_Nav_stack_size` (≙ hauteur de la pile — `dpi_verify_request`).
    fn stack_size(&mut self, bw: u32) -> i32;
    /// `a_History_get_url(NAV_TOP_UIDX(bw))` — URL en haut de pile.
    fn top_url(&mut self, bw: u32) -> Option<DilloUrl>;
}

/// Seam `dicache.c` (phase images — no-op par défaut).
pub trait DicacheOps {
    /// `a_Dicache_invalidate_entry`.
    fn invalidate_entry(&mut self, url: &DilloUrl);
    /// `a_Dicache_get_entry` — existe ?
    fn get_entry(&mut self, url: &DilloUrl, version: i32) -> bool;
    /// `a_Dicache_unref`.
    fn unref(&mut self, url: &DilloUrl, version: i32);
    /// `a_Dicache_cleanup`.
    fn cleanup(&mut self);
    /// `a_Dicache_stats` (page `about:dicache`).
    fn stats(&mut self) -> Option<String>;
}

/// `a_Tls_certificate_is_clean` — seam : le statut cert vit dans le
/// `TlsEngine` du CCC (pas de global Rust) ; injectée par le glue.
pub type CertCleanFn = dyn FnMut(&mut DilloUrl) -> bool;

/// Impl muet par défaut de [`BrowserOps`].
#[derive(Default)]
pub struct NullBrowserOps;
impl BrowserOps for NullBrowserOps {
    fn remove_client(&mut self, _bw: u32, _key: i32) {}
    fn close_client(&mut self, _bw: u32, _key: i32) {}
    fn redirect_level(&mut self, _bw: u32) -> i32 {
        0
    }
    fn set_redirect_level(&mut self, _bw: u32, _level: i32) {}
}

/// Impl muet par défaut de [`UiOps`].
#[derive(Default)]
pub struct NullUiOps;
impl UiOps for NullUiOps {
    fn set_msg(&mut self, _bw: u32, _msg: &str) {}
    fn set_page_prog(&mut self, _bw: u32, _bytes: usize, _cmd: i32) {}
    fn save_link(&mut self, _bw: u32, _url: &DilloUrl, _filename: Option<&str>) {}
}

/// Impl muet par défaut de [`NavOps`].
#[derive(Default)]
pub struct NullNavOps;
impl NavOps for NullNavOps {
    fn push(&mut self, _bw: u32, _url: DilloUrl, _requester: &DilloUrl) {}
    fn reload(&mut self, _bw: u32) {}
    fn cancel_expect_if_eq(&mut self, _bw: u32, _url: &DilloUrl) {}
    fn stack_size(&mut self, _bw: u32) -> i32 {
        0
    }
    fn top_url(&mut self, _bw: u32) -> Option<DilloUrl> {
        None
    }
}

/// Impl muet par défaut de [`DicacheOps`].
#[derive(Default)]
pub struct NullDicacheOps;
impl DicacheOps for NullDicacheOps {
    fn invalidate_entry(&mut self, _url: &DilloUrl) {}
    fn get_entry(&mut self, _url: &DilloUrl, _version: i32) -> bool {
        false
    }
    fn unref(&mut self, _url: &DilloUrl, _version: i32) {}
    fn cleanup(&mut self) {}
    fn stats(&mut self) -> Option<String> {
        None
    }
}

/// Sous-ensemble de `prefs` lu par `cache.c`.
pub struct CachePrefs {
    /// `prefs.trace_http` — log des en-têtes bruts.
    pub trace_http: bool,
    /// `prefs.http_strict_transport_security`.
    pub http_strict_transport_security: bool,
}

/// État global ≙ les `static` de cache.c.
pub struct Cache {
    /// `CachedURLs` — triée par `a_Url_cmp` (`dList_insert_sorted`).
    entries: Vec<CacheEntry>,
    /// `ClientQueue`.
    clients: Vec<CacheClient>,
    /// `DelayedQueue` — les entrées y sont désignées par URL (amont :
    /// pointeur faible ; la recherche se refait au drain).
    delayed: Vec<DilloUrl>,
    /// `DelayedQueueIdleId != 0`.
    delayed_idle: bool,
    /// `busy` statique de `Cache_auth_entry`.
    auth_busy: bool,
    /// `Busy` statique de `Cache_process_queue` (garde, log amont).
    busy: bool,
    /// `ClientKey` statique.
    client_key: i32,
    /// Seam navigateur (fenêtres/clients).
    bw: Box<dyn BrowserOps>,
    /// Seam barre d'état / progression / sauvegarde.
    ui: Box<dyn UiOps>,
    /// Seam navigation.
    nav: Box<dyn NavOps>,
    /// Seam dicache.
    dicache: Box<dyn DicacheOps>,
    /// Seam `a_Web_dispatch_by_type` (partie navigateur).
    dispatch: Option<Box<dyn crate::web::WebDispatch>>,
    /// Transport DPI injecté pour `a_Cookies_set`.
    dpi_send: Option<Box<DpiSend<'static>>>,
    /// Boîte d'auth injectée (`Cache_auth_callback`).
    auth_dialog: Option<Box<AuthDialog<'static>>>,
    /// `a_Tls_certificate_is_clean` injecté (HSTS).
    cert_clean: Option<Box<CertCleanFn>>,
    /// `prefs` lu par cache.c.
    prefs: CachePrefs,
    /// Sink pour `MSG`/`MSG_HTTP` (diagnostic, injectable).
    log: Box<dyn FnMut(&str)>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::new());
}

/// `URL_SCHEME(url)` ≙ `dStrAsciiCasecmp(scheme, x) == 0`.
fn scheme_is(url: &DilloUrl, scheme: &[u8]) -> bool {
    url.scheme
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case(scheme))
}

/// `URL_STR(url)` sur un `&DilloUrl` (amont matérialise `url_string`).
fn url_str(url: &DilloUrl) -> Vec<u8> {
    let mut u = url.clone();
    u.url_str().to_vec()
}

fn null_client_cb(op: i32, client: &mut CacheClient) {
    // `Cache_null_client` — ignore tout sauf `CA_CLOSE` (root →
    // `a_Bw_close_client` — bouton stop inerte).
    if op == CA_CLOSE && client.web.flags & WEB_ROOT_URL != 0 {
        let (bw, key) = (client.web.bw, client.key);
        CACHE.with(|c| c.borrow_mut().bw.close_client(bw, key));
    }
}

fn boxed_null_client() -> CaCb {
    Box::new(null_client_cb)
}

impl Cache {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            clients: Vec::new(),
            delayed: Vec::new(),
            delayed_idle: false,
            auth_busy: false,
            busy: false,
            client_key: 0,
            bw: Box::new(NullBrowserOps),
            ui: Box::new(NullUiOps),
            nav: Box::new(NullNavOps),
            dicache: Box::new(NullDicacheOps),
            dispatch: None,
            dpi_send: None,
            auth_dialog: None,
            cert_clean: None,
            prefs: CachePrefs {
                trace_http: false,
                http_strict_transport_security: true,
            },
            log: Box::new(|_| {}),
        }
    }

    /* -------------------- helpers internes (entrées) -------------------- */

    fn url_order(a: &DilloUrl, b: &DilloUrl) -> std::cmp::Ordering {
        a.url_cmp(b).cmp(&0)
    }

    /// `Cache_entry_search` — `dList_find_sorted` ≙ recherche binaire.
    fn entry_pos(&self, url: &DilloUrl) -> Option<usize> {
        self.entries
            .binary_search_by(|e| Self::url_order(&e.url, url))
            .ok()
    }

    /// `Cache_entry_search_with_redirect` — suit ≤3 redirections, stoppe
    /// sur `CA_RedirectLoop` ou cible sans redirect.
    fn entry_pos_with_redirect(&self, url: &DilloUrl) -> Option<usize> {
        let mut url = url.clone();
        let mut i = 0;
        loop {
            let pos = self.entry_pos(&url)?;
            let entry = &self.entries[pos];
            if entry.flags & ca::REDIRECT_LOOP != 0 || i == 3 {
                return Some(pos);
            }
            if entry.flags & ca::REDIRECT != 0 {
                if let Some(loc) = &entry.location {
                    url = loc.clone();
                    i += 1;
                    continue;
                }
            }
            return Some(pos);
        }
    }

    /// `Cache_entry_add` — warn+remplace si présent, insère trié.
    fn entry_add(&mut self, url: &DilloUrl) -> usize {
        if let Some(pos) = self.entry_pos(url) {
            (self.log)("Cache_entry_add, leaking an entry.");
            self.entries.remove(pos);
        }
        let pos = self
            .entries
            .binary_search_by(|e| Self::url_order(&e.url, url))
            .unwrap_or_else(|p| p);
        self.entries.insert(pos, CacheEntry::new(url));
        pos
    }

    /// `Cache_client_enqueue` — retourne la clé primaire.
    fn client_enqueue(&mut self, url: &DilloUrl, web: Rc<HttpWeb>, call: Option<CaCb>) -> i32 {
        // `ClientKey++` avec wrap à 1 sur i32::MAX — verbatim amont.
        self.client_key = if self.client_key < i32::MAX {
            self.client_key + 1
        } else {
            1
        };
        let key = self.client_key;
        self.clients.push(CacheClient {
            key,
            url: url.clone(),
            version: 0,
            buf: Vec::new(),
            callback: call,
            web,
        });
        key
    }

    /// `Cache_client_dequeue` — retire + `a_Web_free` (invalidate + drop).
    fn client_dequeue(&mut self, idx: usize) {
        if idx < self.clients.len() {
            let cl = self.clients.remove(idx);
            crate::web::web_free(&cl.web);
        }
    }

    fn client_pos_by_key(&self, key: i32) -> Option<usize> {
        self.clients.iter().position(|c| c.key == key)
    }

    /// `Cache_ref_data` — rebuild `UTF8Data` si charset actif.
    fn ref_data(&mut self, pos: usize) {
        let e = &mut self.entries[pos];
        e.data_refcount += 1;
        if e.charset_decoder.is_some() && (e.utf8_data.is_none() || e.data_refcount == 1) {
            let data = std::mem::take(&mut e.data);
            if let Some(dec) = &mut e.charset_decoder {
                e.utf8_data = Some(dec.process(&data));
            }
            e.data = data;
        }
    }

    /// `Cache_unref_data`.
    fn unref_data(&mut self, pos: usize) {
        let e = &mut self.entries[pos];
        e.data_refcount -= 1;
        if e.charset_decoder.is_some() {
            if e.data_refcount == 0 {
                e.utf8_data = None;
            } else if e.data_refcount < 0 {
                e.data_refcount = 0;
            }
        }
    }

    /// `Cache_entry_remove` — retire entrée + clients + delayed + dicache.
    /// `CA_InternalUrl` n'est jamais retiré.
    fn entry_remove(&mut self, url: &DilloUrl) {
        let Some(pos) = self.entry_pos(url) else {
            return;
        };
        if self.entries[pos].flags & ca::INTERNAL_URL != 0 {
            return;
        }
        // Retire tous les clients de l'entrée (≙ boucle
        // `a_Cache_stop_client` amont).
        let mut i = 0;
        while i < self.clients.len() {
            if self.clients[i].url.url_cmp(url) == 0 {
                let key = self.clients[i].key;
                self.stop_client(key);
            } else {
                i += 1;
            }
        }
        self.delayed.retain(|u| u.url_cmp(url) != 0);
        let entry_url = self.entries[pos].url.clone();
        self.dicache.invalidate_entry(&entry_url);
        self.entries.remove(pos);
    }

    /// `Cache_internal_url` — `about:cache` / `about:dicache` injectés.
    fn internal_url(&mut self, pos: usize) {
        let path = self.entries[pos].url.path.clone();
        let s = match path.as_deref() {
            Some(b"cache") => Some(self.stats_html()),
            Some(b"dicache") => self.dicache.stats(),
            _ => None,
        };
        if let Some(s) = s {
            let url = self.entries[pos].url.clone();
            // Injecte les données complètes (chemin GotLength).
            if let Some(p) = self.entry_pos(&url) {
                let e = &mut self.entries[p];
                e.flags = ca::GOT_HEADER | ca::GOT_LENGTH;
                if !s.is_empty() {
                    e.flags &= !ca::IS_EMPTY;
                }
                e.data = s.into_bytes();
                e.expected_size = e.data.len();
                e.transfer_size = e.data.len();
            }
        }
    }

    /// `Cache_stats` — page HTML `about:cache` (même gabarit qu'amont).
    #[allow(clippy::cast_precision_loss)] // compteurs < 2^52 comme amont
    fn stats_html(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::from(
            "<!DOCTYPE HTML>\n<html>\n<head><title>Dillo Cache</title></head>\n<body>\n",
        );
        let _ = writeln!(s, "<h1>Cached URLs ({})</h1>", self.entries.len());
        s.push_str("<table>\n<tr>\n<th>Hits</th>\n<th>Size</th>\n<th>URL</th>\n</tr>\n");
        let mut total_kb = 0.0f64;
        for e in &self.entries {
            let size_kb = e.data().len() as f64 / 1024.0;
            let url_bytes = url_str(&e.url);
            let url_str = String::from_utf8_lossy(&url_bytes);
            s.push_str("<tr>\n");
            let _ = writeln!(s, "<td style='text-align:right'>{}</td>", e.hits);
            let _ = writeln!(s, "<td style='text-align:right'>{size_kb:.2} KiB</td>");
            // `dStr_shorten(s, url, 60)` amont : moitiés + `...`
            // (byte-based, pas `chars().take`).
            let mut ds = dillo_common::dlib::Dstr::new(b"");
            ds.shorten(&url_bytes, 60);
            let short = String::from_utf8_lossy(&ds.into_vec()).into_owned();
            let _ = writeln!(s, "<td><a href='{url_str}'>{short}</a></td>");
            s.push_str("</tr>\n");
            total_kb += size_kb;
        }
        s.push_str("</table>\n");
        let _ = writeln!(s, "<p>Total cached: {:.2} MiB</p>", total_kb / 1024.0);
        s.push_str("</body>\n</html>\n");
        s
    }

    /// `a_Cache_stop_client` — retire client (queue + delayed + dicache).
    fn stop_client(&mut self, key: i32) {
        if let Some(pos) = self.client_pos_by_key(key) {
            let url = self.clients[pos].url.clone();
            let version = self.clients[pos].version;
            if self.dicache.get_entry(&url, version) {
                self.dicache.unref(&url, version);
            }
            // DelayedQueue : le même client peut y figurer via son entrée.
            self.delayed.retain(|u| u.url_cmp(&url) != 0);
            self.client_dequeue(pos);
        }
    }

    /// `a_Cache_client_get_if_unique` — retourne `(url, BufSize)` du
    /// client si c'est le seul client de son entrée.
    fn client_get_if_unique(&self, key: i32) -> Option<(DilloUrl, usize)> {
        let pos = self.client_pos_by_key(key)?;
        let url = self.clients[pos].url.clone();
        let n = self
            .clients
            .iter()
            .filter(|c| c.url.url_cmp(&url) == 0)
            .count();
        (n == 1).then(|| (url, self.clients[pos].buf.len()))
    }
}

/* ------------------- parsing d'en-tête ------------------- */

/// `Cache_parse_field` — extrait un champ `nom:` de l'en-tête
/// (CR-strippé, une ligne par champ). ASCII-ci, trim espace/tab.
fn parse_field(header: &[u8], fieldname: &[u8]) -> Option<Vec<u8>> {
    let mut i = 0usize;
    while i < header.len() {
        let mut j = 0usize;
        while j < fieldname.len()
            && i + j < header.len()
            && fieldname[j].eq_ignore_ascii_case(&header[i + j])
        {
            j += 1;
        }
        if j == fieldname.len() && i + j < header.len() && header[i + j] == b':' {
            i += j + 1;
            while i < header.len() && (header[i] == b' ' || header[i] == b'\t') {
                i += 1;
            }
            let start = i;
            let mut end = i;
            while end < header.len() && header[end] != b'\n' {
                end += 1;
            }
            let mut stop = end;
            while stop > start && (header[stop - 1] == b' ' || header[stop - 1] == b'\t') {
                stop -= 1;
            }
            return Some(header[start..stop].to_vec());
        }
        // Saute à la ligne suivante.
        {
            let off = header[i..].iter().position(|&b| b == b'\n')?;
            i += off + 1;
        }
    }
    None
}

/// `Cache_parse_multiple_fields` — toutes les occurrences du champ.
fn parse_multiple_fields(header: &[u8], fieldname: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut fields = Vec::new();
    let mut i = 0usize;
    while i < header.len() {
        let mut j = 0usize;
        while j < fieldname.len()
            && i + j < header.len()
            && fieldname[j].eq_ignore_ascii_case(&header[i + j])
        {
            j += 1;
        }
        if j == fieldname.len() && i + j < header.len() && header[i + j] == b':' {
            i += j + 1;
            while i < header.len() && (header[i] == b' ' || header[i] == b'\t') {
                i += 1;
            }
            let start = i;
            let mut end = i;
            while end < header.len() && header[end] != b'\n' {
                end += 1;
            }
            let mut stop = end;
            while stop > start && (header[stop - 1] == b' ' || header[stop - 1] == b'\t') {
                stop -= 1;
            }
            fields.push(header[start..stop].to_vec());
            i = end;
        }
        match header[i..].iter().position(|&b| b == b'\n') {
            Some(off) => i += off + 1,
            None => break,
        }
    }
    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

/// `Cache_get_header` — accumulateur : strippe `\r`/NUL, déplie les
/// continuations (espace/tab en début de ligne), complet après `\n\n`.
/// Retourne les octets consommés de `buf` (0 = en-tête incomplet).
fn get_header(e: &mut CacheEntry, buf: &[u8]) -> usize {
    let hdr = &mut e.header;
    let mut n = usize::from(hdr.last() == Some(&b'\n'));
    let mut i = 0usize;
    while i < buf.len() && n < 2 {
        let b = buf[i];
        if b == b'\r' || b == 0 {
            i += 1;
            continue;
        }
        if n == 1 && (b == b' ' || b == b'\t') {
            hdr.pop(); // unfold : retire le '\n' précédent
        }
        n = if b == b'\n' { n + 1 } else { 0 };
        hdr.push(b);
        i += 1;
    }
    if n == 2 {
        e.flags |= ca::GOT_HEADER;
        i
    } else {
        0
    }
}

/// `strtol(Length, NULL, 10)` — parse décimal avec tolérance amont
/// (espaces initiaux, signe, préfixe numérique).
fn parse_i64(s: &[u8]) -> i64 {
    let mut i = 0;
    while i < s.len() && dlib::is_space(s[i]) {
        i += 1;
    }
    let mut neg = false;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        neg = s[i] == b'-';
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
    if neg {
        -v
    } else {
        v
    }
}

/// `Cache_parse_header` — 100-continue, redirects, auth, cookies, HSTS,
/// décodeurs, taille attendue, Content-Type, Content-Disposition.
/// Termine par `Cache_ref_data`.
#[allow(clippy::too_many_lines)] // port direct de la machine à états amont
fn parse_header(c: &mut Cache, pos: usize) {
    let server1point0 = c.entries[pos].header.starts_with(b"HTTP/1.0");
    let url = c.entries[pos].url.clone();

    // 100 Continue → en-tête informatif, on repart à zéro.
    {
        let h = &c.entries[pos].header;
        if h.len() > 12 && h.get(9..12) == Some(b"100") {
            (c.log)("An actual 100 Continue header!");
            let e = &mut c.entries[pos];
            e.flags &= !ca::GOT_HEADER;
            e.header = Vec::new();
            return;
        }
    }

    if c.entries[pos].header.len() > 12 {
        // `header[9..11]` amont = code de statut ; copie locale (l'entrée
        // est mutée ensuite).
        let h9_11: [u8; 3] = c.entries[pos]
            .header
            .get(9..12)
            .and_then(|s| <[u8; 3]>::try_from(s).ok())
            .unwrap_or([0; 3]);
        if h9_11.starts_with(b"30") && parse_field(&c.entries[pos].header, b"Location").is_some() {
            // 30x : redirection.
            let location_str = parse_field(&c.entries[pos].header, b"Location").unwrap_or_default();
            let base = url_str(&url);
            let location = url_new(&location_str, Some(&base));
            let blocked = match &location {
                Some(loc) => {
                    let mut src = url.clone();
                    let mut dst = loc.clone();
                    !domain_permit(&mut src, &mut dst)
                        || (loc.flags & (url_flags::POST | url_flags::GET) != 0
                            && scheme_is(loc, b"dpi")
                            && !scheme_is(&url, b"dpi"))
                }
                None => true,
            };
            // `entry->Location` posé AVANT le test domainrc/dpi amont :
            // une 30x bloquée garde `Location` sans `CA_Redirect` →
            // `Cache_provide_redirection_blocked_page` peut servir la
            // page « Dillo blocked a redirection ».
            c.entries[pos].location = location;
            if blocked {
                (c.log)("Redirection not followed (domainrc/dpi)");
            } else {
                let e = &mut c.entries[pos];
                e.flags |= ca::REDIRECT;
                match h9_11[2] {
                    b'1' => e.flags |= ca::FORCE_REDIRECT,
                    b'2' => e.flags |= ca::TEMP_REDIRECT,
                    _ => {}
                }
            }
        } else if &h9_11 == b"401" {
            let fields = parse_multiple_fields(&c.entries[pos].header, b"WWW-Authenticate");
            c.entries[pos].auth = fields.map(|v| {
                v.iter()
                    .map(|f| String::from_utf8_lossy(f).into_owned())
                    .collect()
            });
        } else if &h9_11 == b"404" {
            c.entries[pos].flags |= ca::NOT_FOUND;
        }
    }

    {
        let header = c.entries[pos].header.clone();
        if let Some(warnings) = parse_multiple_fields(&header, b"Warning") {
            for w in warnings {
                (c.log)(&format!("HTTP Warning: {}", String::from_utf8_lossy(&w)));
            }
        }
    }

    if server1point0 {
        c.entries[pos].flags &= !ca::KEEP_ALIVE;
    }
    let connection = parse_field(&c.entries[pos].header, b"Connection");
    if let Some(connection) = connection {
        if connection.eq_ignore_ascii_case(b"close") {
            c.entries[pos].flags &= !ca::KEEP_ALIVE;
        } else if server1point0 && connection.eq_ignore_ascii_case(b"keep-alive") {
            c.entries[pos].flags |= ca::KEEP_ALIVE;
        }
    }

    // HSTS (prefs + https + nom d'hôte — pas d'IP).
    let https_name = scheme_is(&url, b"https") && {
        let mut u = url.clone();
        u.hostname().is_some_and(|h| host_type(h) == URL_HOST_NAME)
    };
    if c.prefs.http_strict_transport_security && https_name {
        if let Some(hsts) = parse_field(&c.entries[pos].header, b"Strict-Transport-Security") {
            let mut u = url.clone();
            let clean = c.cert_clean.as_deref_mut().is_some_and(|f| f(&mut u));
            hsts_set(&String::from_utf8_lossy(&hsts), &mut u, clean);
        }
    }

    // Transfer-Encoding → décodeur (`NULL` amont ≙ `None`).
    let encoding = parse_field(&c.entries[pos].header, b"Transfer-Encoding");
    let enc_s = encoding
        .as_deref()
        .map(|e| String::from_utf8_lossy(e).into_owned());
    c.entries[pos].transfer_decoder = DecodeTransfer::new(enc_s.as_deref());

    if let Some(length) = parse_field(&c.entries[pos].header, b"Content-Length") {
        if let Some(enc) = &encoding {
            // TE présent : Content-Length ignoré ; erreur si non-identity.
            if !enc.eq_ignore_ascii_case(b"identity") {
                (c.log)("Content-Length and non-identity Transfer-Encoding headers both present.");
            }
        } else {
            let e = &mut c.entries[pos];
            e.flags |= ca::GOT_LENGTH;
            e.expected_size = usize::try_from(parse_i64(&length).max(0)).unwrap_or(0);
        }
    }

    // Set-Cookie — premier client root/requester autorisé seulement.
    if let Some(cookies) = parse_multiple_fields(&c.entries[pos].header, b"Set-Cookie") {
        for ci in 0..c.clients.len() {
            if c.clients[ci].url.url_cmp(&url) != 0 {
                continue;
            }
            let requester_none = c.clients[ci].web.requester.is_none();
            let safe_redirect = c.entries[pos].flags & ca::REDIRECT != 0
                && c.clients[ci].web.flags & WEB_ROOT_URL != 0;
            let same_org = match &c.clients[ci].web.requester {
                None => true,
                Some(req) => {
                    let mut e_url = url.clone();
                    let mut r = req.clone();
                    e_url.same_organization(&mut r)
                }
            };
            if requester_none || safe_redirect || same_org {
                let date = parse_field(&c.entries[pos].header, b"Date");
                let cookie_strs: Vec<String> = cookies
                    .iter()
                    .map(|ck| String::from_utf8_lossy(ck).into_owned())
                    .collect();
                let mut url_m = url.clone();
                let mut send = c.dpi_send.take();
                let mut noop = |_cmd: &str| -> Option<String> { None };
                let transport: &mut DpiSend = send.as_deref_mut().unwrap_or(&mut noop);
                let date_s = date
                    .as_deref()
                    .map(|d| String::from_utf8_lossy(d).into_owned());
                cookies_set(&cookie_strs, &mut url_m, date_s.as_deref(), transport);
                if send.is_some() {
                    c.dpi_send = send;
                }
                break; // amont : stop après le premier client *autorisé*
            }
            // client tiers non autorisé → le scan continue amont
        }
    }

    // Content-Encoding → décodeur contenu.
    let cenc = parse_field(&c.entries[pos].header, b"Content-Encoding");
    c.entries[pos].content_decoder = Decode::content_init(
        cenc.as_deref()
            .map(|e| String::from_utf8_lossy(e).into_owned())
            .as_deref(),
    );

    if c.entries[pos].expected_size > 0 {
        let e = &mut c.entries[pos];
        if e.expected_size > HUGE_FILESIZE {
            e.flags |= ca::HUGE_FILE;
        }
        // Capacité initiale bornée (évite les réallocs + SEGFAULT amont
        // sur fichiers énormes — Vec croît dynamiquement ensuite).
        e.data = Vec::with_capacity(e.expected_size.min(MAX_INIT_BUF));
    }

    // Content-Type (non fiable — vérifié contre les données réelles dans
    // process_queue avant GOT_CONTENT_TYPE).
    if let Some(ty) = parse_field(&c.entries[pos].header, b"Content-Type") {
        set_content_type(c, &url, &ty, "http");
    }

    c.entries[pos].content_disposition =
        parse_field(&c.entries[pos].header, b"Content-Disposition");
    c.ref_data(pos);
}

/// `a_Cache_set_content_type` — `from` ∈ `{"http","meta"}`.
/// Retourne le type courant (≙ `curr` amont).
fn set_content_type(c: &mut Cache, url: &DilloUrl, ctype: &[u8], from: &str) -> Option<Vec<u8>> {
    let pos = c.entry_pos(url)?;
    let curr = c.entries[pos].current_content_type().map(<[u8]>::to_vec);
    let already = c.entries[pos].type_meta.is_some()
        || (from.starts_with('h') && c.entries[pos].type_hdr.is_some());
    if already {
        return curr;
    }
    if from.starts_with('h') {
        c.entries[pos].type_hdr = Some(ctype.to_vec());
    } else {
        c.entries[pos].type_meta = Some(ctype.to_vec());
    }
    if !misc::content_type_cmp(curr.as_deref(), Some(ctype)) {
        // Le nouveau type diffère du courant → normalisation éventuelle.
        let parsed = misc::parse_content_type(Some(ctype));
        let major_empty = parsed.major.as_deref().is_none_or(<[u8]>::is_empty);
        let minor_empty = parsed.minor.as_deref().is_none_or(<[u8]>::is_empty);
        if from.starts_with('m') && parsed.charset.is_some() && major_empty && minor_empty {
            // META ne donne que le charset → préfixe le type détecté.
            let mut norm = c.entries[pos].type_det.clone().unwrap_or_default();
            norm.extend_from_slice(ctype);
            c.entries[pos].type_norm = Some(norm);
        } else if from.starts_with('m') && dlib::strn_ascii_casecmp(ctype, b"text/xhtml", 10) == 0 {
            // workaround doxygen "text/xhtml".
            c.entries[pos].type_norm = Some(match &parsed.charset {
                Some(cs) => {
                    let mut v = b"application/xhtml+xml; charset=".to_vec();
                    v.extend_from_slice(cs);
                    v
                }
                None => b"application/xhtml+xml".to_vec(),
            });
        }
        if let Some(cs) = &parsed.charset {
            let cs = String::from_utf8_lossy(cs).into_owned();
            let e = &mut c.entries[pos];
            e.charset_decoder = Decode::charset_init(Some(&cs));
            e.utf8_data = None;
            return e.current_content_type().map(<[u8]>::to_vec);
        }
    }
    curr
}

/* ------------------------- API publique ------------------------- */

/// `a_Cache_init` — vide les files et injecte `about:splash`.
pub fn cache_init() {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.entries.clear();
        c.clients.clear();
        c.delayed.clear();
        c.delayed_idle = false;
        c.auth_busy = false;
        c.busy = false;
    });
    if let Some(u) = url_new(b"about:splash", None) {
        cache_entry_inject(
            &u,
            about::ABOUT_SPLASH.as_bytes(),
            ca::GOT_HEADER | ca::GOT_LENGTH | ca::INTERNAL_URL,
        );
    }
}

/// Injecte les seams navigateur (appelé par `dillo-browser` au boot).
pub fn cache_set_seams(
    bw: Box<dyn BrowserOps>,
    ui: Box<dyn UiOps>,
    nav: Box<dyn NavOps>,
    dicache: Box<dyn DicacheOps>,
    dispatch: Box<dyn crate::web::WebDispatch>,
) {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.bw = bw;
        c.ui = ui;
        c.nav = nav;
        c.dicache = dicache;
        c.dispatch = Some(dispatch);
    });
}

/// Injecte le transport DPI pour `a_Cookies_set` (phase DPI).
pub fn cache_set_dpi_send(send: Box<DpiSend<'static>>) {
    CACHE.with(|c| c.borrow_mut().dpi_send = Some(send));
}

/// Injecte la boîte d'authentification (`a_UIcmd` dialog amont).
pub fn cache_set_auth_dialog(dialog: Box<AuthDialog<'static>>) {
    CACHE.with(|c| c.borrow_mut().auth_dialog = Some(dialog));
}

/// Injecte `a_Tls_certificate_is_clean` (HSTS).
pub fn cache_set_cert_clean(f: Box<CertCleanFn>) {
    CACHE.with(|c| c.borrow_mut().cert_clean = Some(f));
}

/// Injecte le sink de logs `MSG`/`MSG_HTTP`.
pub fn cache_set_log(log: Box<dyn FnMut(&str)>) {
    CACHE.with(|c| c.borrow_mut().log = log);
}

/// `a_UIcmd_set_msg` — accès cross-module à la seam UI pour les
/// callbacks cache détenus ailleurs (`Nav_save_cb` amont appelle
/// `a_UIcmd_set_msg` directement ; ici la seam vit dans `CACHE`).
pub fn cache_ui_msg(bw: u32, msg: &str) {
    CACHE.with(|c| c.borrow_mut().ui.set_msg(bw, msg));
}

/// `a_Bw_close_client` — accès cross-module à la seam navigateur
/// (`a_Dicache_close`/`a_Dicache_callback` amont appellent
/// `a_Bw_close_client` directement).
pub fn cache_close_client(bw: u32, key: i32) {
    CACHE.with(|c| c.borrow_mut().bw.close_client(bw, key));
}

/// `prefs` lu par cache.c.
pub fn cache_set_prefs(trace_http: bool, hsts: bool) {
    CACHE.with(|c| {
        c.borrow_mut().prefs = CachePrefs {
            trace_http,
            http_strict_transport_security: hsts,
        }
    });
}

/// `a_Cache_open_url` — hit → client + delayed queue ; miss → entrée +
/// interne `about:*` éventuel. Retourne la clé client.
pub fn cache_open_url(web: Rc<HttpWeb>, call: Option<CaCb>) -> i32 {
    let url = web.url.clone();
    let internal = scheme_is(&url, b"about");
    let (key, hit_url) = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if internal {
            c.entry_remove(&url);
        }
        if url.flags & url_flags::E2E_QUERY != 0 {
            c.entry_remove(&url);
        }
        if let Some(pos) = c.entry_pos(&url) {
            let entry_url = c.entries[pos].url.clone();
            let key = c.client_enqueue(&entry_url, web, call);
            (key, Some((entry_url, true)))
        } else {
            let pos = c.entry_add(&url);
            let entry_url = c.entries[pos].url.clone();
            if internal {
                c.internal_url(pos);
            }
            let key = c.client_enqueue(&entry_url, web, call);
            // miss interne : queue processée mais `Hits` n'augmente
            // que sur hit amont (`cache.c` branche `entry` trouvée).
            (
                key,
                if internal {
                    Some((entry_url, false))
                } else {
                    None
                },
            )
        }
    });
    if let Some((u, hit)) = hit_url {
        delayed_process_queue(&u);
        if hit {
            CACHE.with(|c| {
                let mut c = c.borrow_mut();
                if let Some(p) = c.entry_pos(&u) {
                    c.entries[p].hits += 1;
                }
            });
        }
    }
    key
}

/// `a_Cache_get_flags`.
#[must_use]
pub fn cache_get_flags(url: &DilloUrl) -> u32 {
    CACHE.with(|c| {
        c.borrow()
            .entry_pos(url)
            .map_or(0, |p| c.borrow().entries[p].flags)
    })
}

/// `a_Cache_get_flags_with_redirection`.
#[must_use]
pub fn cache_get_flags_with_redirection(url: &DilloUrl) -> u32 {
    CACHE.with(|c| {
        c.borrow()
            .entry_pos_with_redirect(url)
            .map_or(0, |p| c.borrow().entries[p].flags)
    })
}

/// `a_Cache_get_content_type` — type courant en suivant les redirections.
#[must_use]
pub fn cache_get_content_type(url: &DilloUrl) -> Option<Vec<u8>> {
    CACHE.with(|c| {
        let c = c.borrow();
        c.entry_pos_with_redirect(url)
            .and_then(|p| c.entries[p].current_content_type().map(<[u8]>::to_vec))
    })
}

/// `a_Cache_set_content_type` — `from` ∈ `{"http","meta"}`.
#[must_use]
pub fn cache_set_content_type(url: &DilloUrl, ctype: &[u8], from: &str) -> Option<Vec<u8>> {
    CACHE.with(|c| set_content_type(&mut c.borrow_mut(), url, ctype, from))
}

/// `a_Cache_get_buf` — snapshot des données (UTF-8 si traduites).
/// `DataRefcount` incrémenté comme amont — appairer avec
/// [`cache_unref_buf`].
#[must_use]
pub fn cache_get_buf(url: &DilloUrl) -> Option<Vec<u8>> {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let p = c.entry_pos_with_redirect(url)?;
        c.ref_data(p);
        Some(c.entries[p].data().to_vec())
    })
}

/// `a_Cache_get_header` — en-tête brut (suivant les redirections).
#[must_use]
pub fn cache_get_header(url: &DilloUrl) -> Option<Vec<u8>> {
    CACHE.with(|c| {
        let c = c.borrow();
        c.entry_pos_with_redirect(url)
            .map(|p| c.entries[p].header.clone())
    })
}

/// `a_Cache_unref_buf`.
pub fn cache_unref_buf(url: &DilloUrl) {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(p) = c.entry_pos_with_redirect(url) {
            c.unref_data(p);
        }
    });
}

/// `a_Cache_entry_inject` — `flags & CA_GotLength` : injection directe ;
/// sinon entrée fraîche puis `process_dbuf` pour parser.
pub fn cache_entry_inject(url: &DilloUrl, buf: &[u8], flags: u32) {
    let needs_process = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.entry_pos(url).is_none() {
            c.entry_add(url);
        }
        if flags & ca::GOT_LENGTH != 0 {
            if let Some(p) = c.entry_pos(url) {
                let e = &mut c.entries[p];
                e.flags = flags;
                if !buf.is_empty() {
                    e.flags &= !ca::IS_EMPTY;
                }
                e.data.clear();
                e.data.extend_from_slice(buf);
                e.data.shrink_to_fit();
                e.expected_size = e.data.len();
                e.transfer_size = e.data.len();
            }
            false
        } else {
            if let Some(p) = c.entry_pos(url) {
                c.entries[p] = CacheEntry::new(url);
            }
            true
        }
    });
    if needs_process {
        process_dbuf(IO_READ, buf, url);
    }
}

/// `a_Cache_entry_remove_by_url`.
pub fn cache_entry_remove_by_url(url: &DilloUrl) {
    CACHE.with(|c| c.borrow_mut().entry_remove(url));
}

/// `a_Cache_download_enabled` — schemes http/https/ftp.
#[must_use]
pub fn cache_download_enabled(url: &DilloUrl) -> bool {
    scheme_is(url, b"http") || scheme_is(url, b"https") || scheme_is(url, b"ftp")
}

/// `a_Cache_client_get_if_unique` — `(url, BufSize)` si la clé est le
/// seul client de son entrée (amont rend le `CacheClient_t*`).
#[must_use]
pub fn client_get_if_unique(key: i32) -> Option<(DilloUrl, usize)> {
    CACHE.with(|c| c.borrow().client_get_if_unique(key))
}

/// `a_Cache_stop_client`.
pub fn cache_stop_client(key: i32) {
    CACHE.with(|c| c.borrow_mut().stop_client(key));
}

/// `a_Cache_freeall` — purge clients + entrées (exit).
pub fn cache_freeall() {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        while !c.clients.is_empty() {
            let last = c.clients.len() - 1;
            c.client_dequeue(last);
        }
        c.entries.clear();
        c.delayed.clear();
        c.delayed_idle = false;
        c.auth_busy = false;
        c.busy = false;
    });
}

/// Nombre d'entrées (tests/diagnostic).
#[must_use]
pub fn cache_entry_count() -> usize {
    CACHE.with(|c| c.borrow().entries.len())
}

/// Nombre de clients en file (tests/diagnostic).
#[must_use]
pub fn cache_client_count() -> usize {
    CACHE.with(|c| c.borrow().clients.len())
}

/* --------------------- cœur : flux de données --------------------- */

/// `Cache_finish_msg` — fin de corps : logs d'incohérences, clear
/// `CA_InProgress`, libère les décodeurs, shrink, `process_queue`.
fn finish_msg(url: &DilloUrl) {
    CACHE.with(|cell| {
        {
            let mut c = cell.borrow_mut();
            let Some(pos) = c.entry_pos(url) else { return };
            if c.entries[pos].flags & ca::IN_PROGRESS == 0 {
                return;
            }
            let (expected, transfer, no_hdr, len_mismatch) = {
                let e = &c.entries[pos];
                (
                    e.expected_size,
                    e.transfer_size,
                    e.type_hdr.is_none(),
                    e.flags & ca::GOT_LENGTH != 0 && e.expected_size != e.transfer_size,
                )
            };
            if (expected > 0 || transfer > 0) && no_hdr {
                (c.log)("Message with a body lacked Content-Type header.");
            }
            if len_mismatch {
                (c.log)("Content-Length does NOT match message body");
            }
            let e = &mut c.entries[pos];
            e.flags &= !ca::IN_PROGRESS;
            e.transfer_decoder = None;
            e.content_decoder = None;
            e.data.shrink_to_fit();
        }
        if process_queue(url) {
            let mut c = cell.borrow_mut();
            if let Some(p) = c.entry_pos(url) {
                if c.entries[p].flags & ca::GOT_HEADER != 0 {
                    c.unref_data(p);
                }
            }
        }
    });
}

/// `a_Cache_process_dbuf` — `Op` ∈ {`IO_READ`, `IO_CLOSE`, `IO_ABORT`}.
/// `IO_READ` : accumulation d'en-tête puis décodage chaîné vers `Data`/
/// `UTF8Data`. Retourne `true` si le corps est complet (`done` amont).
/// Beaucoup d'appelants amont ignorent le retour (`a_Capi_ccc` abort).
#[allow(clippy::too_many_lines, clippy::must_use_candidate)]
pub fn process_dbuf(op: i32, buf: &[u8], url: &DilloUrl) -> bool {
    let mut done = false;
    CACHE.with(|cell| {
        if op == IO_READ {
            // Boucle en-tête (100-continue vide et recommence).
            let mut offset = 0usize;
            loop {
                let got = cell
                    .borrow()
                    .entry_pos(url)
                    .is_some_and(|p| cell.borrow().entries[p].flags & ca::GOT_HEADER != 0);
                if got {
                    break;
                }
                let (len, alive) = {
                    let mut c = cell.borrow_mut();
                    match c.entry_pos(url) {
                        Some(p) => (
                            get_header(&mut c.entries[p], &buf[offset.min(buf.len())..]),
                            true,
                        ),
                        None => (0, false),
                    }
                };
                if !alive {
                    return;
                }
                if len == 0 {
                    break;
                }
                offset += len;
                {
                    let mut c = cell.borrow_mut();
                    match c.entry_pos(url) {
                        Some(p) => parse_header(&mut c, p),
                        None => return,
                    }
                }
            }

            let has_header = cell
                .borrow()
                .entry_pos(url)
                .is_some_and(|p| cell.borrow().entries[p].flags & ca::GOT_HEADER != 0);
            if !has_header {
                return;
            }
            {
                let mut c = cell.borrow_mut();
                let Some(p) = c.entry_pos(url) else { return };
                let e = &mut c.entries[p];
                let raw = &buf[offset.min(buf.len())..];
                e.transfer_size += raw.len();
                let mut data = raw.to_vec();
                if let Some(dec) = &mut e.transfer_decoder {
                    data = dec.process(&data);
                    done = dec.finished();
                }
                if let Some(dec) = &mut e.content_decoder {
                    data = dec.process(&data);
                }
                e.data.extend_from_slice(&data);
                if e.charset_decoder.is_some() && e.utf8_data.is_some() {
                    let out = e
                        .charset_decoder
                        .as_mut()
                        .map(|d| d.process(&data))
                        .unwrap_or_default();
                    if let Some(u) = &mut e.utf8_data {
                        u.extend_from_slice(&out);
                    }
                }
                if !e.data.is_empty() {
                    e.flags &= !ca::IS_EMPTY;
                }
                if e.flags & ca::GOT_LENGTH != 0 && e.transfer_size >= e.expected_size {
                    done = true;
                }
                if e.flags & ca::KEEP_ALIVE == 0 {
                    done = false; // IOClose terminera
                }
            }
            if process_queue(url) && done {
                finish_msg(url);
            }
        } else if op == IO_CLOSE {
            finish_msg(url);
        } else if op == IO_ABORT {
            let empty;
            {
                let mut c = cell.borrow_mut();
                let Some(p) = c.entry_pos(url) else { return };
                c.entries[p].flags |= ca::ABORTED;
                empty = c.entries[p].data.is_empty();
                if !empty {
                    (c.log)("Premature close");
                }
            }
            if empty {
                // Pas de données : retire les clients de cette entrée
                // (a_Bw_remove_client amont).
                let mut c = cell.borrow_mut();
                let mut i = 0;
                while i < c.clients.len() {
                    if c.clients[i].url.url_cmp(url) == 0 {
                        let (bw, key) = (c.clients[i].web.bw, c.clients[i].key);
                        c.bw.remove_client(bw, key);
                        c.client_dequeue(i);
                    } else {
                        i += 1;
                    }
                }
            } else {
                finish_msg(url);
            }
        }
    });
    done
}

/// `Cache_redirect` — redirections 30x (page racine → `a_Nav_push`).
fn cache_redirect(c: &mut Cache, pos: usize, web_flags: u32, bw: u32) {
    let entry_url = c.entries[pos].url.clone();

    if entry_url.flags & url_flags::SPAM_SAFE != 0 {
        c.ui.set_msg(bw, "WARNING: local URL with redirection.  Aborting.");
        return;
    }
    if c.bw.redirect_level(bw) >= 5 {
        c.entries[pos].flags |= ca::REDIRECT_LOOP;
    }
    if c.entries[pos].flags & ca::REDIRECT_LOOP != 0 {
        c.ui.set_msg(bw, "ERROR: redirect loop");
        c.bw.set_redirect_level(bw, 0);
        return;
    }
    let e = &c.entries[pos];
    if e.flags & ca::REDIRECT != 0
        && e.location.is_some()
        && (e.flags & (ca::FORCE_REDIRECT | ca::TEMP_REDIRECT) != 0
            || e.data.is_empty()
            || e.data.len() < 1024)
        && web_flags & WEB_ROOT_URL != 0
    {
        let loc = e.location.clone();
        let base = url_str(&entry_url);
        if let Some(loc) = loc {
            let loc_s = url_str(&loc);
            if let Some(mut new_url) = url_new(&loc_s, Some(&base)) {
                if e.flags & ca::TEMP_REDIRECT != 0 {
                    new_url.flags |= url_flags::E2E_QUERY;
                }
                c.nav.push(bw, new_url, &entry_url);
            }
        }
    }
    // Sous-entité (image) : amont ne fait que logger.
}

/// `Cache_auth_entry`/`Cache_auth_callback` — dialog injecté via timeout
/// 0.0 (≙ `a_Timeout_add(0.0,…)` + flag `busy`).
fn auth_entry(url: &DilloUrl, challenges: Vec<String>, bw: u32) {
    let go = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.auth_busy {
            (c.log)("Cache_auth_entry: caught busy!");
            false
        } else {
            c.auth_busy = true;
            true
        }
    });
    if !go {
        return;
    }
    let url = url.clone();
    timeout_add(0.0, move || {
        let ok = CACHE.with(|c| {
            let mut c = c.borrow_mut();
            let mut url_m = url.clone();
            let mut dialog = c.auth_dialog.take();
            let mut noop = |_: &str, _: &str| -> Option<(String, String)> { None };
            let d: &mut AuthDialog = dialog.as_deref_mut().unwrap_or(&mut noop);
            let r = auth_do_auth(&challenges, &mut url_m, d);
            if dialog.is_some() {
                c.auth_dialog = dialog;
            }
            r
        });
        if ok {
            CACHE.with(|c| c.borrow_mut().nav.reload(bw));
        }
        CACHE.with(|c| c.borrow_mut().auth_busy = false);
        timeout_remove(); // no-op amont, porté
    });
}

/// `Cache_savelink_cb` ≙ `a_UIcmd_save_link` différé par timeout 0.0.
fn savelink_later(bw: u32, url: DilloUrl, filename: Option<String>) {
    timeout_add(0.0, move || {
        CACHE.with(|c| c.borrow_mut().ui.save_link(bw, &url, filename.as_deref()));
    });
}

/// `Cache_provide_redirection_blocked_page` — page d'erreur inline
/// servie au client (dispatch text/html forcé amont).
fn provide_redirection_blocked_page(cell: &RefCell<Cache>, cl: &mut CacheClient) {
    let (url_s, loc_s) = {
        let c = cell.borrow();
        match c.entry_pos(&cl.url) {
            Some(p) => {
                let u = String::from_utf8_lossy(&url_str(&c.entries[p].url)).into_owned();
                let l = c.entries[p]
                    .location
                    .as_ref()
                    .map(|loc| String::from_utf8_lossy(&url_str(loc)).into_owned())
                    .unwrap_or_default();
                (u, l)
            }
            None => return,
        }
    };
    // `a_Web_dispatch_by_type("text/html", …)` — force le viewer HTML.
    let mut d = cell.borrow_mut().dispatch.take();
    let cb = crate::web::dispatch_by_type(
        "text/html",
        &cl.web,
        d.as_deref_mut().unwrap_or(&mut NullDispatch),
    );
    {
        let mut c = cell.borrow_mut();
        if d.is_some() {
            c.dispatch = d;
        }
    }
    if let Some(cb) = cb {
        cl.callback = Some(cb);
    }
    cl.buf = format!(
        "<!doctype html><html><body>Dillo blocked a redirection from \
         <a href=\"{url_s}\">{url_s}</a> to <a href=\"{loc_s}\">{loc_s}</a> \
         based on your domainrc settings.</body></html>"
    )
    .into_bytes();
    if let Some(mut cb) = cl.callback.take() {
        cb(CA_SEND, cl);
        if cl.callback.is_none() {
            cl.callback = Some(cb);
        }
    }
    cl.buf.clear();
}

/// Dispatch muet quand la seam n'est pas injectée.
struct NullDispatch;
impl crate::web::WebDispatch for NullDispatch {
    fn dispatch(&mut self, _mime: &str, _web: &Rc<HttpWeb>) -> Option<CaCb> {
        None
    }
}

/// `Cache_process_queue` — détection de type, dispatch viewer, envoi
/// `CA_Send`, clôtures `CA_CLOSE`, redirects, aborts, auth, download.
/// Retourne `true` si l'entrée existe encore après traitement
/// (amont : pointeur non-NULL).
fn process_queue(url: &DilloUrl) -> bool {
    CACHE.with(|cell| {
        {
            let mut c = cell.borrow_mut();
            if c.busy {
                (c.log)("FATAL!: >>>> Cache_process_queue Caught busy!!! <<<<");
            }
            c.busy = true;
        }
        let r = process_queue_inner(cell, url);
        cell.borrow_mut().busy = false;
        r
    })
}

#[allow(clippy::too_many_lines)] // port direct de la boucle amont
fn process_queue_inner(cell: &RefCell<Cache>, url: &DilloUrl) -> bool {
    // Phase 0 : garde + détection du type depuis les données.
    // `TypeMismatch` amont : local de fonction posé ici (une fois, à
    // la détection) — pas recalculé par client ni par passe.
    let mut type_mismatch = false;
    {
        let mut c = cell.borrow_mut();
        let Some(pos) = c.entry_pos(url) else {
            return false;
        };
        if c.entries[pos].flags & ca::GOT_HEADER == 0 {
            return true;
        }
        if c.entries[pos].flags & ca::GOT_CONTENT_TYPE == 0 {
            let data = c.entries[pos].data.clone();
            let (ty, st) = misc::get_content_type_from_data(&data);
            if st == 0 || c.entries[pos].flags & ca::IN_PROGRESS == 0 {
                type_mismatch =
                    misc::content_type_check(c.entries[pos].type_hdr.as_deref(), ty.as_bytes()) < 0;
                if type_mismatch {
                    let h = c.entries[pos].type_hdr.clone().unwrap_or_default();
                    (c.log)(&format!(
                        "Content-Type '{}' doesn't match the real data.",
                        String::from_utf8_lossy(&h)
                    ));
                }
                let e = &mut c.entries[pos];
                e.type_det = Some(ty.as_bytes().to_vec());
                e.flags |= ca::GOT_CONTENT_TYPE;
            } else {
                return true; // attendre plus de données
            }
        }
    }

    // Boucle clients — amont itère `ClientQueue` avec fixups `--i`.
    // Ici : le client est détaché (remove) puis réinséré ou supprimé.
    let mut i = 0usize;
    let mut last_bw = 0u32;
    let mut abort_entry = false;
    let mut offer_download = false;
    let mut dfilename: Option<Vec<u8>> = None;
    loop {
        let idx = {
            let c = cell.borrow();
            let mut found = None;
            for (n, cl) in c.clients.iter().enumerate().skip(i) {
                if cl.url.url_cmp(url) == 0 {
                    found = Some(n);
                    break;
                }
            }
            match found {
                Some(n) => n,
                None => break,
            }
        };
        // Détache le client (callback/dispatch peuvent ré-entrer).
        // `i = idx` : sans réinsertion, le client suivant a glissé à
        // `idx` — amont compense le shift par `--i` (sauter `idx+1`
        // laisserait un client non servi sur toute entrée à ≥2
        // clients). La branche réinsérée reprend à `idx + 1`.
        let mut cl = cell.borrow_mut().clients.remove(idx);
        i = idx;
        last_bw = cl.web.bw;

        // Pré-logique (root/non-root, msgs, redirect_level, dispatch).
        let mut dequeued = false;
        {
            let mut c = cell.borrow_mut();
            let Some(pos) = c.entry_pos(url) else {
                // Entrée disparue (abort synchrone) : amont continuerait
                // sur un pointeur mort — ici on dépile proprement.
                crate::web::web_free(&cl.web);
                return false;
            };
            let e_flags = c.entries[pos].flags;
            let web_flags = cl.web.flags;
            let bw = cl.web.bw;
            let mut dtype: Option<Vec<u8>> = None;

            if web_flags & WEB_ROOT_URL != 0 {
                if let Some(cd) = c.entries[pos].content_disposition.clone() {
                    let (t, f) = misc::parse_content_disposition(Some(&cd));
                    dtype = t;
                    if dfilename.is_none() {
                        dfilename = f;
                    }
                }
                if e_flags & ca::MSG_ERASED == 0 {
                    c.ui.set_msg(bw, "");
                    c.entries[pos].flags |= ca::MSG_ERASED;
                }
                // `set_msg` root-only amont ; `AbortEntry` s'applique à
                // tout client sans callback via `type_mismatch` partagé.
                if type_mismatch {
                    let h = c.entries[pos].type_hdr.clone().unwrap_or_default();
                    c.ui.set_msg(
                        bw,
                        &format!(
                            "HTTP warning: Content-Type '{}' doesn't match the real data.",
                            String::from_utf8_lossy(&h)
                        ),
                    );
                    offer_download = true;
                }
                if e_flags & ca::REDIRECT != 0 {
                    if cl.callback.is_none() {
                        cl.callback = Some(boxed_null_client());
                        let lvl = c.bw.redirect_level(bw);
                        c.bw.set_redirect_level(bw, lvl + 1);
                    }
                } else {
                    c.bw.set_redirect_level(bw, 0);
                }
                if e_flags & ca::HUGE_FILE != 0 {
                    let mb = c.entries[pos].expected_size / (1024 * 1024);
                    c.ui.set_msg(bw, &format!("Huge file! ({mb} MB)"));
                    abort_entry = true;
                    offer_download = true;
                }
            } else if e_flags & (ca::REDIRECT | ca::NOT_FOUND) != 0 {
                cl.callback = Some(boxed_null_client());
            }

            // Choix du viewer si pas de callback.
            if cl.callback.is_none() {
                let has_location_no_redirect =
                    c.entries[pos].location.is_some() && c.entries[pos].flags & ca::REDIRECT == 0;
                if !has_location_no_redirect {
                    if type_mismatch {
                        abort_entry = true;
                    } else {
                        let curr = c.entries[pos]
                            .current_content_type()
                            .map(<[u8]>::to_vec)
                            .unwrap_or_default();
                        let mime = String::from_utf8_lossy(&curr).into_owned();
                        // Dispatch hors borrow (la seam peut ré-entrer).
                        let mut d = c.dispatch.take();
                        drop(c);
                        let cb = crate::web::dispatch_by_type(
                            &mime,
                            &cl.web,
                            d.as_deref_mut().unwrap_or(&mut NullDispatch),
                        );
                        c = cell.borrow_mut();
                        if d.is_some() {
                            c.dispatch = d;
                        }
                        match cb {
                            None => {
                                if web_flags & WEB_ROOT_URL != 0 {
                                    (c.log)(&format!("Content-Type '{mime}' not viewable."));
                                    abort_entry = true;
                                    offer_download = true;
                                }
                                // Non-root : reste null_client (entrée se
                                // remplit sans viewer — amont ne coupe pas).
                            }
                            Some(cb) => {
                                cl.callback = Some(cb);
                                if dtype
                                    .as_deref()
                                    .is_some_and(|d| dlib::strn_ascii_casecmp(d, b"inline", 6) != 0)
                                {
                                    abort_entry = true;
                                    offer_download = true;
                                }
                            }
                        }
                    }
                    if abort_entry {
                        if web_flags & WEB_ROOT_URL != 0 {
                            c.nav.cancel_expect_if_eq(bw, &cl.url.clone());
                        }
                        c.bw.remove_client(bw, cl.key);
                        crate::web::web_free(&cl.web);
                        dequeued = true;
                    }
                }
            }
        }
        if dequeued {
            continue;
        }

        // Envoi CA_Send — client détaché, aucun borrow tenu.
        let (data_len_total, finished) = {
            let c = cell.borrow();
            let p = c.entry_pos(url);
            let buf = match p {
                Some(p) => {
                    if cl.web.flags & WEB_DOWNLOAD != 0 {
                        c.entries[p].data.clone()
                    } else {
                        c.entries[p].data().to_vec()
                    }
                }
                None => Vec::new(),
            };
            cl.buf = buf;
            (
                p.map_or(0, |p| c.entries[p].data.len()),
                p.is_none_or(|p| c.entries[p].flags & ca::IN_PROGRESS == 0),
            )
        };
        let sent = !cl.buf.is_empty();
        if sent {
            if let Some(mut cb) = cl.callback.take() {
                cb(CA_SEND, &mut cl);
                if cl.callback.is_none() {
                    cl.callback = Some(cb);
                }
            }
            if cl.web.flags & WEB_ROOT_URL != 0 {
                cell.borrow_mut()
                    .ui
                    .set_page_prog(cl.web.bw, data_len_total, 1);
            }
        }

        if finished {
            // `CA_Close` : page bloquée éventuelle, close, redirect.
            let (is_redirect, blocked_redirect) = {
                let c = cell.borrow();
                match c.entry_pos(url) {
                    Some(p) => (
                        c.entries[p].flags & ca::REDIRECT != 0,
                        cl.web.flags & WEB_ROOT_URL != 0
                            && c.entries[p].location.is_some()
                            && c.entries[p].flags & ca::REDIRECT == 0,
                    ),
                    None => (false, false),
                }
            };
            if blocked_redirect {
                provide_redirection_blocked_page(cell, &mut cl);
            }
            if let Some(mut cb) = cl.callback.take() {
                cb(CA_CLOSE, &mut cl);
                if cl.callback.is_none() {
                    cl.callback = Some(cb);
                }
            }
            {
                let mut c = cell.borrow_mut();
                if cl.web.flags & WEB_ROOT_URL != 0 {
                    let aborted = c
                        .entry_pos(url)
                        .is_some_and(|p| c.entries[p].flags & ca::ABORTED != 0);
                    if aborted {
                        c.ui.set_msg(
                            cl.web.bw,
                            "ERROR: Connection closed early, read not complete.",
                        );
                    }
                    c.ui.set_page_prog(cl.web.bw, 0, 0);
                }
            }
            let (web_flags, bw) = (cl.web.flags, cl.web.bw);
            crate::web::web_free(&cl.web);
            // Un seul appel redirect amont (après le dequeue).
            if is_redirect {
                let mut c = cell.borrow_mut();
                if let Some(p) = c.entry_pos(url) {
                    cache_redirect(&mut c, p, web_flags, bw);
                }
            }
        } else {
            // Réinsère le client vivant à sa place — le scan reprend
            // après lui (≙ `++i` amont après réinsertion).
            let mut c = cell.borrow_mut();
            let at = idx.min(c.clients.len());
            c.clients.insert(at, cl);
            i = idx + 1;
        }
    }

    // AbortEntry → coupe la connexion (≙ a_Capi_conn_abort_by_url) puis
    // offre de téléchargement différée.
    if abort_entry {
        crate::capi::conn_abort_by_url(url);
        if offer_download {
            {
                let mut c = cell.borrow_mut();
                c.entry_remove(url);
            }
            if cache_download_enabled(url) {
                let fname = dfilename
                    .as_ref()
                    .map(|f| String::from_utf8_lossy(f).into_owned());
                savelink_later(last_bw, url.clone(), fname);
            }
        }
        return false;
    }

    // Auth en attente (401) → dialog via timeout. `Cache_auth_entry`
    // amont teste `busy` AVANT de consommer `entry->Auth` : si occupé,
    // le défi reste posé sur l'entrée (rejouable) — pas de `take`.
    let auth = {
        let mut c = cell.borrow_mut();
        match c.entry_pos(url) {
            Some(p)
                if !c.auth_busy
                    && c.entries[p].auth.is_some()
                    && c.entries[p].flags & ca::IN_PROGRESS == 0 =>
            {
                c.entries[p].auth.take()
            }
            _ => None,
        }
    };
    if let Some(challenges) = auth {
        auth_entry(url, challenges, last_bw);
    } else {
        let warn = CACHE.with(|c| {
            let c = c.borrow();
            c.auth_busy
                && c.entry_pos(url)
                    .is_some_and(|p| c.entries[p].auth.is_some())
        });
        if warn {
            CACHE.with(|c| (c.borrow_mut().log)("Cache_auth_entry: caught busy!"));
        }
    }

    // `a_Dicache_cleanup` quand la file est vide.
    if cell.borrow().clients.is_empty() {
        cell.borrow_mut().dicache.cleanup();
    }
    cell.borrow().entry_pos(url).is_some()
}

/// `Cache_delayed_process_queue(_callback)` — drain différé par timeout.
fn delayed_process_queue(url: &DilloUrl) {
    let armed = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if !c.delayed.iter().any(|u| u.url_cmp(url) == 0) {
            c.delayed.push(url.clone());
        }
        if c.delayed_idle {
            false
        } else {
            c.delayed_idle = true;
            true
        }
    });
    if armed {
        timeout_add(0.0, || {
            loop {
                let next = CACHE.with(|c| c.borrow().delayed.first().cloned());
                let Some(url) = next else { break };
                CACHE.with(|c| {
                    let mut c = c.borrow_mut();
                    if let Some(p) = c.entry_pos(&url) {
                        c.ref_data(p);
                    }
                });
                if process_queue(&url) {
                    CACHE.with(|c| {
                        let mut c = c.borrow_mut();
                        if let Some(p) = c.entry_pos(&url) {
                            c.unref_data(p);
                            c.delayed.retain(|u| u.url_cmp(&url) != 0);
                        }
                    });
                } else {
                    CACHE.with(|c| {
                        c.borrow_mut().delayed.retain(|u| u.url_cmp(&url) != 0);
                    });
                }
            }
            CACHE.with(|c| c.borrow_mut().delayed_idle = false);
            timeout_remove(); // no-op amont, porté
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn u(s: &str) -> DilloUrl {
        url_new(s.as_bytes(), None).unwrap_or_else(|| panic!("bad url: {s}"))
    }

    fn web(url: &DilloUrl, flags: u32) -> Rc<HttpWeb> {
        Rc::new(HttpWeb::new(url.clone(), None, flags))
    }

    fn reset() {
        cache_init();
        crate::timeout::timeout_freeall();
    }

    /// Réponse HTTP complète en une fois — ≙ `IORead` amont.
    fn resp(head: &str, body: &str) -> Vec<u8> {
        let mut v = head.replace('\n', "\r\n").into_bytes();
        v.extend_from_slice(b"\r\n\r\n");
        v.extend_from_slice(body.as_bytes());
        v
    }

    #[test]
    fn open_creates_entry_and_client_key() {
        reset();
        let url = u("http://example.com/");
        let key = cache_open_url(web(&url, WEB_ROOT_URL), None);
        assert!(key > 0);
        let flags = cache_get_flags(&url);
        assert!(flags & ca::IN_PROGRESS != 0);
        assert!(flags & ca::IS_EMPTY != 0);
        assert_eq!(cache_client_count(), 1);
    }

    #[test]
    fn dbuf_header_then_body_sends_to_client() {
        reset();
        let url = u("http://example.com/p");
        let events = Rc::new(RefCell::new(Vec::<(i32, Vec<u8>)>::new()));
        let ev = events.clone();
        let cb: CaCb = Box::new(move |op, cl| {
            ev.borrow_mut().push((op, cl.buf.clone()));
        });
        cache_open_url(web(&url, WEB_ROOT_URL), Some(cb));
        // HTTP/1.1 : keep-alive par défaut → `done` à Content-Length.
        let done = process_dbuf(
            IO_READ,
            &resp(
                "HTTP/1.1 200 OK\nContent-Type: text/html\nContent-Length: 5",
                "hello",
            ),
            &url,
        );
        assert!(done, "Content-Length atteinte → done");
        // `process_queue` tourne avant ET après `finish_msg` :
        // CA_Send (InProgress) puis CA_Send + CA_Close.
        let evs = events.borrow();
        assert!(evs.len() >= 2, "{evs:?}");
        for (op, buf) in evs.iter().take(evs.len() - 1) {
            assert_eq!(*op, CA_SEND, "{evs:?}");
            assert_eq!(buf, b"hello");
        }
        assert_eq!(evs.last().map(|e| e.0), Some(CA_CLOSE), "{evs:?}");
        let flags = cache_get_flags(&url);
        assert!(flags & ca::GOT_HEADER != 0);
        assert_eq!(flags & ca::IN_PROGRESS, 0);
        assert_eq!(
            cache_get_content_type(&url).as_deref(),
            Some(b"text/html".as_slice())
        );
    }

    #[test]
    fn dbuf_split_header_completes_on_second_read() {
        reset();
        let url = u("http://example.com/");
        // Callback fourni : sans viewer (`NullDispatch`), un ROOT sans
        // callback est aborté comme amont (`AbortEntry`).
        let cb: CaCb = Box::new(|_, _| {});
        cache_open_url(web(&url, WEB_ROOT_URL), Some(cb));
        let b1 = b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r";
        assert!(!process_dbuf(IO_READ, b1, &url));
        assert_eq!(cache_get_flags(&url) & ca::GOT_HEADER, 0);
        let b2 = b"\n\r\nabc";
        assert!(process_dbuf(IO_READ, b2, &url));
        assert!(cache_get_flags(&url) & ca::GOT_HEADER != 0);
    }

    /// A01-D7 : `Strict-Transport-Security` honoré quand la seam
    /// `cert_clean` est câblée (≙ `a_Tls_certificate_is_clean` amont,
    /// `hsts.c:208`) — ignorée sinon.
    #[test]
    fn hsts_header_registers_policy_only_when_cert_clean() {
        reset();
        crate::hsts::hsts_freeall();
        crate::hsts::hsts_init(true, None);
        cache_set_prefs(false, true);
        // Seam absente → clean=false → STS ignoré (comme amont sur
        // connexion non TLS-clean).
        let url = u("https://sts.example/");
        cache_open_url(web(&url, WEB_ROOT_URL), Some(Box::new(|_, _| {})));
        process_dbuf(
            IO_READ,
            &resp(
                "HTTP/1.1 200 OK\nStrict-Transport-Security: max-age=100\nContent-Length: 1",
                "x",
            ),
            &url,
        );
        assert!(!crate::hsts::hsts_require_https("sts.example"));
        // Seam câblée → politique enregistrée.
        cache_set_cert_clean(Box::new(|_| true));
        let url2 = u("https://sts2.example/");
        cache_open_url(web(&url2, WEB_ROOT_URL), Some(Box::new(|_, _| {})));
        process_dbuf(
            IO_READ,
            &resp(
                "HTTP/1.1 200 OK\nStrict-Transport-Security: max-age=100\nContent-Length: 1",
                "x",
            ),
            &url2,
        );
        assert!(crate::hsts::hsts_require_https("sts2.example"));
        CACHE.with(|c| c.borrow_mut().cert_clean = None);
    }

    #[test]
    fn folded_header_is_unfolded_and_crs_stripped() {
        reset();
        let url = u("http://example.com/");
        cache_open_url(web(&url, WEB_ROOT_URL), None);
        let raw = b"HTTP/1.0 200 OK\r\nX-Long: par\r\n tial\r\nContent-Length: 0\r\n\r\n";
        process_dbuf(IO_READ, raw, &url);
        let hdr = cache_get_header(&url).unwrap_or_default();
        let s = String::from_utf8_lossy(&hdr);
        // Quirk amont : le `\n` est supprimé mais l'espace de
        // continuation reste → « par tial » (pas « partial »).
        assert!(s.contains("X-Long: par tial"), "{s}");
        assert!(!s.contains('\r'), "{s}");
    }

    #[test]
    fn informational_100_resets_header() {
        reset();
        let url = u("http://example.com/");
        cache_open_url(web(&url, WEB_ROOT_URL), None);
        let mut raw = b"HTTP/1.0 100 Continue\r\n\r\n".to_vec();
        raw.extend_from_slice(b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nhi");
        process_dbuf(IO_READ, &raw, &url);
        let hdr = cache_get_header(&url).unwrap_or_default();
        let s = String::from_utf8_lossy(&hdr);
        assert!(s.starts_with("HTTP/1.0 200"), "{s}");
        assert_eq!(cache_get_buf(&url).as_deref(), Some(b"hi".as_slice()));
    }

    #[test]
    fn redirect_30x_sets_location_and_flags() {
        reset();
        let url = u("http://example.com/old");
        cache_open_url(web(&url, WEB_ROOT_URL), None);
        let raw =
            b"HTTP/1.0 302 Found\r\nLocation: http://example.com/new\r\nContent-Length: 0\r\n\r\n";
        process_dbuf(IO_READ, raw, &url);
        process_dbuf(IO_CLOSE, &[], &url);
        let flags = cache_get_flags(&url);
        assert!(flags & ca::REDIRECT != 0);
        assert!(flags & ca::TEMP_REDIRECT != 0);
        assert_eq!(flags & ca::FORCE_REDIRECT, 0);
    }

    #[test]
    fn redirect_chain_followed_with_limit() {
        reset();
        // a → b → c → d : la recherche avec redirect suit ≤ 3 sauts.
        for (from, to) in [
            ("http://x/a", "http://x/b"),
            ("http://x/b", "http://x/c"),
            ("http://x/c", "http://x/d"),
        ] {
            let from = u(from);
            cache_open_url(web(&from, WEB_ROOT_URL), None);
            let raw = format!("HTTP/1.0 301 Moved\r\nLocation: {to}\r\n\r\n");
            process_dbuf(IO_READ, raw.as_bytes(), &from);
            process_dbuf(IO_CLOSE, &[], &from);
        }
        let d = u("http://x/d");
        cache_open_url(web(&d, WEB_ROOT_URL), None);
        process_dbuf(
            IO_READ,
            &resp("HTTP/1.0 200 OK\nContent-Length: 1", "x"),
            &d,
        );
        let a = u("http://x/a");
        // Les 3 sauts mènent à d — flags de d, pas de a.
        let flags = cache_get_flags_with_redirection(&a);
        assert!(flags & ca::REDIRECT == 0, "arrivée sur d");
        assert_eq!(flags & ca::IS_EMPTY, 0);
    }

    #[test]
    fn not_found_sets_flag() {
        reset();
        let url = u("http://example.com/nope");
        cache_open_url(web(&url, WEB_ROOT_URL), None);
        process_dbuf(
            IO_READ,
            &resp("HTTP/1.0 404 Not Found\nContent-Length: 0", ""),
            &url,
        );
        assert!(cache_get_flags(&url) & ca::NOT_FOUND != 0);
    }

    #[test]
    fn abort_marks_entry_and_removes_empty_clients() {
        reset();
        let url = u("http://example.com/");
        let key = cache_open_url(web(&url, WEB_ROOT_URL), None);
        process_dbuf(IO_ABORT, &[], &url);
        assert!(cache_get_flags(&url) & ca::ABORTED != 0);
        // Entrée vide → clients retirés (a_Bw_remove_client ≙ déqueue).
        assert_eq!(cache_client_count(), 0, "key {key} must be gone");
    }

    #[test]
    fn internal_about_cache_is_injected() {
        reset();
        let url = u("about:cache");
        let key = cache_open_url(web(&url, WEB_ROOT_URL), None);
        assert!(key > 0);
        let flags = cache_get_flags(&url);
        assert!(flags & ca::GOT_HEADER != 0);
        let buf = cache_get_buf(&url).unwrap_or_default();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("Cached URLs"), "{s}");
    }

    #[test]
    fn about_splash_is_preloaded() {
        reset();
        let url = u("about:splash");
        let flags = cache_get_flags(&url);
        assert!(flags & ca::INTERNAL_URL != 0);
        assert!(flags & ca::GOT_LENGTH != 0);
        let buf = cache_get_buf(&url).unwrap_or_default();
        assert!(buf.windows(5).any(|w| w == b"Dillo"));
    }

    #[test]
    fn hit_open_returns_same_entry_and_delayed_send() {
        reset();
        let url = u("http://example.com/");
        cache_entry_inject(&url, b"cached", ca::GOT_HEADER | ca::GOT_LENGTH);
        let events = Rc::new(RefCell::new(Vec::new()));
        let ev = events.clone();
        let cb: CaCb = Box::new(move |op, cl| {
            ev.borrow_mut().push((op, cl.buf.clone()));
        });
        let key = cache_open_url(web(&url, WEB_ROOT_URL), Some(cb));
        assert!(key > 0);
        // DelayedQueue : le send arrive après poll_timeouts.
        crate::timeout::poll_timeouts();
        let evs = events.borrow();
        assert!(
            evs.iter()
                .any(|(op, buf)| *op == CA_SEND && buf == b"cached"),
            "{evs:?}"
        );
    }

    #[test]
    fn stop_client_dequeues() {
        reset();
        let url = u("http://example.com/");
        let key = cache_open_url(web(&url, WEB_ROOT_URL), None);
        assert_eq!(cache_client_count(), 1);
        cache_stop_client(key);
        assert_eq!(cache_client_count(), 0);
    }

    #[test]
    fn unique_client_detected() {
        reset();
        let url = u("http://example.com/");
        let k1 = cache_open_url(web(&url, WEB_ROOT_URL), None);
        assert!(client_get_if_unique(k1).is_some());
        let k2 = cache_open_url(web(&url, WEB_ROOT_URL), None);
        assert!(client_get_if_unique(k1).is_none());
        assert!(client_get_if_unique(k2).is_none());
    }

    #[test]
    fn remove_entry_purges_clients_and_delayed() {
        reset();
        let url = u("http://example.com/");
        cache_open_url(web(&url, WEB_ROOT_URL), None);
        cache_entry_remove_by_url(&url);
        assert_eq!(cache_get_flags(&url), 0);
        assert_eq!(cache_client_count(), 0);
    }

    /// A11-D1 : une 30x bloquée par domainrc garde `Location` sans
    /// `CA_Redirect` → la page « Dillo blocked a redirection » est
    /// servie au client (amont pose `entry->Location` avant le test).
    #[test]
    fn blocked_redirect_keeps_location_and_serves_block_page() {
        reset();
        crate::domain::domain_parse("default deny\n");
        let url = u("http://a.b/old");
        let events = Rc::new(RefCell::new(Vec::<(i32, Vec<u8>)>::new()));
        let ev = events.clone();
        let cb: CaCb = Box::new(move |op, cl| {
            ev.borrow_mut().push((op, cl.buf.clone()));
        });
        cache_open_url(web(&url, WEB_ROOT_URL), Some(cb));
        let raw = b"HTTP/1.0 302 Found\r\nLocation: http://c.d/new\r\nContent-Length: 0\r\n\r\n";
        process_dbuf(IO_READ, raw, &url);
        process_dbuf(IO_CLOSE, &[], &url);
        crate::domain::domain_freeall();
        assert_eq!(cache_get_flags(&url) & ca::REDIRECT, 0);
        let evs = events.borrow();
        let blocked = evs.iter().any(|(op, buf)| {
            *op == CA_SEND && String::from_utf8_lossy(buf).contains("Dillo blocked a redirection")
        });
        assert!(blocked, "{evs:?}");
    }

    /// A11-D2 : quand un client est déqueued (`AbortEntry`), le client
    /// suivant de la même entrée glisse à son index — il doit quand
    /// même être servi (amont compense par `--i`).
    #[test]
    fn process_queue_serves_client_after_dequeued_sibling() {
        reset();
        let url = u("http://example.com/p");
        // Client A : root sans callback → aborté à la détection de type
        // (NullDispatch → « not viewable » → AbortEntry → dequeued).
        cache_open_url(web(&url, WEB_ROOT_URL), None);
        let events = Rc::new(RefCell::new(Vec::<(i32, Vec<u8>)>::new()));
        let ev = events.clone();
        let cb: CaCb = Box::new(move |op, cl| {
            ev.borrow_mut().push((op, cl.buf.clone()));
        });
        // Client B : même entrée, callback posé.
        cache_open_url(web(&url, WEB_ROOT_URL), Some(cb));
        process_dbuf(
            IO_READ,
            &resp(
                "HTTP/1.1 200 OK\nContent-Type: text/html\nContent-Length: 5",
                "hello",
            ),
            &url,
        );
        let evs = events.borrow();
        assert!(
            evs.iter().any(|(op, _)| *op == CA_CLOSE),
            "client B sauté après le dequeue de A — {evs:?}"
        );
    }

    /// A11-D3 : `Set-Cookie` — un client tiers non autorisé n'arrête
    /// pas le scan ; le premier client autorisé pose les cookies.
    #[test]
    fn set_cookie_scans_past_unauthorized_client() {
        reset();
        crate::cookies::cookies_freeall();
        assert_eq!(crate::cookies::cookies_init(Some("DEFAULT ACCEPT\n")), 0);
        let sent = Rc::new(RefCell::new(Vec::<String>::new()));
        let s2 = sent.clone();
        cache_set_dpi_send(Box::new(move |cmd: &str| {
            s2.borrow_mut().push(cmd.to_string());
            None
        }));
        let url = u("http://a.b/p");
        // Client 1 : requester cross-org, non-root → non autorisé.
        let req = u("http://x.y/");
        cache_open_url(Rc::new(HttpWeb::new(url.clone(), Some(req), 0)), None);
        // Client 2 : requester None → autorisé.
        cache_open_url(web(&url, WEB_ROOT_URL), None);
        process_dbuf(
            IO_READ,
            &resp(
                "HTTP/1.0 200 OK\nContent-Type: text/html\nSet-Cookie: k=v\nContent-Length: 1",
                "x",
            ),
            &url,
        );
        crate::cookies::cookies_freeall();
        let cmds = sent.borrow();
        assert!(
            cmds.iter().any(|c| c.contains("set_cookie")),
            "cookie jamais posé — {cmds:?}"
        );
    }

    /// A11-D5 : `TypeMismatch` → `AbortEntry` vaut pour tout client
    /// sans callback, pas seulement les roots. Observable : l'entrée
    /// est rapportée morte → `finish_msg` sauté → `IN_PROGRESS` resté.
    #[test]
    fn non_root_type_mismatch_aborts_entry() {
        reset();
        let url = u("http://example.com/m");
        cache_open_url(web(&url, 0), None);
        // Déclaré image/png mais le sniffer voit du HTML.
        process_dbuf(
            IO_READ,
            &resp(
                "HTTP/1.1 200 OK\nContent-Type: image/png\nContent-Length: 15",
                "<html><body></",
            ),
            &url,
        );
        let flags = cache_get_flags(&url);
        assert!(
            flags & ca::IN_PROGRESS != 0,
            "entrée non abortée — flags={flags:#x}"
        );
    }

    /// A11-D6 : `hits` n'augmente que sur un vrai hit — l'ouverture
    /// interne `about:cache`/`about:dicache` (miss + injection) ne
    /// compte pas (amont `Hits++` branche hit seule).
    #[test]
    fn internal_url_open_does_not_count_as_hit() {
        reset();
        let url = u("about:cache");
        cache_open_url(web(&url, WEB_ROOT_URL), None);
        let buf = cache_get_buf(&url).unwrap_or_default();
        let s = String::from_utf8_lossy(&buf);
        // La page liste l'entrée about:cache elle-même avec Hits=0.
        assert!(s.contains("right'>0</td>"), "{s}");
        assert!(!s.contains("right'>1</td>"), "{s}");
    }

    /// A11-D7 : `auth_busy` → `entry->Auth` n'est PAS consommé (amont
    // ne vide le défi qu'en armant le timeout — le défi reste rejouable).
    #[test]
    fn auth_challenge_survives_busy_auth_entry() {
        reset();
        let url = u("http://example.com/");
        cache_open_url(web(&url, WEB_ROOT_URL), None);
        // Dialog déjà en vol → busy.
        CACHE.with(|c| c.borrow_mut().auth_busy = true);
        process_dbuf(
            IO_READ,
            &resp(
                "HTTP/1.0 401 Auth\nWWW-Authenticate: Basic realm=\"x\"\nContent-Length: 1",
                "x",
            ),
            &url,
        );
        let kept = CACHE.with(|c| {
            c.borrow()
                .entry_pos(&url)
                .is_some_and(|p| c.borrow().entries[p].auth.is_some())
        });
        assert!(kept, "défi perdu alors que le dialog était busy");
    }

    /// A11-D14 : `about:cache` tronque les URLs par `dStr_shorten`
    /// (moitiés + `...`), pas un préfixe de 60 chars.
    #[test]
    fn about_cache_shortens_long_url_middle() {
        reset();
        let long = format!("http://example.com/{}", "a".repeat(80));
        let url = u(&long);
        cache_entry_inject(&url, b"x", ca::GOT_HEADER | ca::GOT_LENGTH);
        let about = u("about:cache");
        cache_open_url(web(&about, WEB_ROOT_URL), None);
        let buf = cache_get_buf(&about).unwrap_or_default();
        let s = String::from_utf8_lossy(&buf);
        // dStr_shorten(60) : 28 premiers (`http://example.com/` + 9
        // `a`) + "..." + 29 derniers — un préfixe `take(60)` n'aurait
        // pas de "..." ni de queue.
        assert!(s.contains("http://example.com/aaaaaaaaa..."), "{s}");
        assert!(s.contains(&format!("...{}</a>", "a".repeat(29))), "{s}");
    }
}
