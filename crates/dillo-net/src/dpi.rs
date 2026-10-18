//! Port of `src/IO/dpi.c` — client DPI (plugins Dillo).
//!
//! Les plugins dpi (bookmarks, cookies, file, datauri, downloads…)
//! sont des programmes externes pilotés par `dpid` ; ce module est le
//! client côté navigateur :
//!
//! - [`DpiConn`] ≙ `dpi_conn_t` — tokeniseur du flux serveur
//!   (tags `<...'>` dpip + données brutes après `start_send_page`).
//! - [`dpi_ccb`] ≙ `a_Dpi_ccc` — callback de chaîne à 4 branches
//!   (1 = canal de commande vers le serveur, 2 = données reçues).
//! - [`send_blocking_cmd`] ≙ `a_Dpi_send_blocking_cmd` — requête
//!   synchrone (cookies, auth…) : connexion, écriture, lecture.
//!
//! Transport : TCP loopback `127.0.0.1` (verbatim — le démon écoute en
//! inet, pas en `AF_UNIX`). Toutes les connexions d'établissement sont
//! **bloquantes** comme en amont (`Dpi_blocking_*`) — le démon est
//! local, la latence est négligeable ; la donnée transite ensuite par
//! l'[`IoEngine`] comme tout autre flux.
//!
//! Différences d'implémentation (comportement identique) :
//!
//! - `fork`/`pipe`/`execl` du lancement de dpid →
//!   [`std::process::Command`] : l'échec de `spawn` ≙ l'« ERROR » que
//!   l'enfant écrivait dans le pipe amont.
//! - `DILLO_BINDIR` (chemin de build amont) → répertoire de
//!   l'exécutable courant (`current_exe`), puis `PATH`.
//! - `ValidConns` (`Klist` de pointeurs) → drapeau `valid` sur
//!   [`DpiConn`] : même garde de réentrance (un `parse_token` peut
//!   aborter la chaîne et libérer la connexion pendant la boucle), la
//!   durée de vie étant assurée par `Rc`.
//! - `SharedKey[32]` amont écrivait sans borne ; le port borne à 31
//!   octets (sécurité — la clé dpid fait 20 hexadécimaux).

use std::cell::{Cell, RefCell};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::rc::Rc;
use std::time::Duration;

use tracing::{debug, warn};

use crate::chain::{
    Chain, ChainCb, ChainData, DataBuf, LinkId, BCK, FWD, OP_ABORT, OP_END, OP_SEND, OP_START,
};
use crate::dpip;
use crate::io::{io_ccb, IoEngine};
use dillo_common::dlib;

/// `dpi_conn_t` — état d'une connexion à un serveur dpi (branche 2).
#[derive(Debug)]
pub struct DpiConn {
    /// `InTag` — à l'intérieur d'un tag `<…'>`.
    in_tag: bool,
    /// `Send2EOF` — plus de tags : tout le reste est de la donnée brute
    /// (≙ réponse `start_send_page`, ou serveur `http` via dpi).
    send2eof: bool,
    /// `DataTotalSize` (champ amont jamais écrit — conservé verbatim).
    #[allow(dead_code)]
    data_total_size: i32,
    /// `DataRecvSize` (idem).
    #[allow(dead_code)]
    data_recv_size: i32,
    /// `Buf` — tampon cumulatif de réception.
    buf: Vec<u8>,
    /// `BufIdx` — curseur de lecture dans `buf`.
    buf_idx: usize,
    /// `TokIdx` — début du token courant.
    tok_idx: usize,
    /// `TokSize`.
    tok_size: usize,
    /// `TokIsTag` (posé comme en amont ; non relu dans le port).
    #[allow(dead_code)]
    tok_is_tag: bool,
    /// `InfoRecv` — lien de la branche qui remonte vers le client
    /// (branche 1 ≙ capi/cookies…).
    info_recv: LinkId,
    /// `ValidConns` amont : présence dans la liste ≙ `valid == true`
    /// (invalidé par [`conn_free`] pendant un callback réentrant).
    valid: Cell<bool>,
}

thread_local! {
    /// `static char SharedKey[32]` — contenu lu de
    /// `~/.dillo/dpid_comm_keys` (hexadécimal).
    static SHARED_KEY: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// `static int starting` de `Dpi_check_dpid` — compteur de
    /// tentatives de démarrage du démon.
    static DPID_STARTING: Cell<i32> = const { Cell::new(0) };
}

/// `a_Dpi_init` — vide en amont.
pub fn init() {}

/// `Dpi_conn_new` — état initial du tokeniseur.
fn conn_new(info_recv: LinkId) -> Rc<RefCell<DpiConn>> {
    Rc::new(RefCell::new(DpiConn {
        in_tag: false,
        send2eof: false,
        data_total_size: 0,
        data_recv_size: 0,
        buf: Vec::with_capacity(8 * 1024),
        buf_idx: 0,
        tok_idx: 0,
        tok_size: 0,
        tok_is_tag: false,
        info_recv,
        valid: Cell::new(true),
    }))
}

/// `Dpi_conn_free` — le `Rc` gère la mémoire ; il ne reste que la
/// garde de validité (retrait de `ValidConns` amont).
fn conn_free(conn: &Rc<RefCell<DpiConn>>) {
    conn.borrow().valid.set(false);
}

/// `Dpi_conn_valid` — la connexion survit-elle à un callback réentrant ?
fn conn_valid(conn: &Rc<RefCell<DpiConn>>) -> bool {
    conn.borrow().valid.get()
}

/// `Dpi_get_token` — découpe le flux en tokens : `0` = token prêt,
/// `-1` = données insuffisantes.
///
/// Deux cas : `Send2EOF` (tout le reste = un token de données) ou
/// recherche de tag `<… '>` (EOT = `'` précédé d'un espace avant `>`).
fn get_token(conn: &mut DpiConn) -> i32 {
    if conn.buf_idx == conn.buf.len() {
        conn.buf.clear();
        conn.buf_idx = 0;
        return -1;
    }

    if conn.send2eof {
        conn.tok_idx = conn.buf_idx;
        conn.tok_size = conn.buf.len() - conn.buf_idx;
        conn.buf_idx = conn.buf.len();
        return 0;
    }

    if !conn.in_tag {
        // recherche du début de tag
        while conn.buf_idx < conn.buf.len() && conn.buf[conn.buf_idx] != b'<' {
            conn.buf_idx += 1;
        }
        if conn.buf_idx < conn.buf.len() {
            conn.in_tag = true;
            conn.tok_idx = conn.buf_idx;
        } else {
            warn!("[Dpi_get_token] Can't find token start");
        }
    }

    let mut resp = -1;
    if conn.in_tag {
        // recherche de la fin de tag (EOT = " '>")
        let mut i = conn.buf_idx;
        while i < conn.buf.len() {
            if conn.buf[i] == b'>' && i >= 2 && conn.buf[i - 1] == b'\'' && conn.buf[i - 2] == b' '
            {
                break;
            }
            i += 1;
        }
        conn.buf_idx = i;

        if conn.buf_idx < conn.buf.len() {
            conn.tok_is_tag = true;
            conn.tok_size = conn.buf_idx - conn.tok_idx + 1;
            conn.buf_idx += 1;
            conn.in_tag = false;
            resp = 0;
        }
    }
    resp
}

/// `Dpi_parse_token` — dispatch du tag dpip vers `info_recv`.
///
/// `conn` est emprunté par tranches courtes : les `fcb` peuvent
/// réentrer (abort → `conn_free`), un `borrow_mut` tenu pendant un
/// callback paniquerait.
fn parse_token(chain: &mut Chain, conn: &Rc<RefCell<DpiConn>>) {
    let (tok, send2eof, info_recv) = {
        let c = conn.borrow();
        (
            c.buf[c.tok_idx..c.tok_idx + c.tok_size].to_vec(),
            c.send2eof,
            c.info_recv,
        )
    };

    if send2eof {
        // réception de données d'une page (chaines `send_page_2eof`)
        // ≙ `a_Chain_dbuf_new(Tok, TokSize, 0)`.
        let size = i32::try_from(tok.len()).unwrap_or(i32::MAX);
        chain.fcb(
            OP_SEND,
            info_recv,
            ChainData::Buf(Box::new(DataBuf::new(tok, size, 0))),
            ChainData::Str("send_page_2eof".to_string()),
        );
        return;
    }

    let Some(cmd_b) = dpip::get_attr_l(&tok, b"cmd") else {
        return;
    };
    let cmd = String::from_utf8_lossy(&cmd_b).into_owned();
    let get = |name: &[u8]| -> Option<String> {
        dpip::get_attr_l(&tok, name).map(|v| String::from_utf8_lossy(&v).into_owned())
    };
    let tag = String::from_utf8_lossy(&tok).into_owned();
    debug!("Dpi_parse_token: {{{}}}", tag);

    match cmd.as_str() {
        "send_status_message" | "chat" => {
            if let Some(msg) = get(b"msg") {
                chain.fcb(OP_SEND, info_recv, ChainData::Str(msg), ChainData::Str(cmd));
            }
        }
        "dialog" => {
            // Le tag dpip complet est envoyé (verbatim).
            chain.fcb(OP_SEND, info_recv, ChainData::Str(tag), ChainData::Str(cmd));
        }
        "start_send_page" => {
            conn.borrow_mut().send2eof = true;
            if let Some(url) = get(b"url") {
                chain.fcb(OP_SEND, info_recv, ChainData::Str(url), ChainData::Str(cmd));
            }
            // TODO amont : attr "send_mode" jamais lu ici non plus.
        }
        "reload_request" => {
            if let Some(url) = get(b"url") {
                chain.fcb(OP_SEND, info_recv, ChainData::Str(url), ChainData::Str(cmd));
            }
        }
        _ => {}
    }
}

// --------------------------------------------------------------------

/// `Dpi_blocking_write` — écriture complète avec retry sur `EINTR`.
/// `true` = succès.
fn blocking_write(s: &mut TcpStream, msg: &[u8]) -> bool {
    let mut sent = 0;
    while sent < msg.len() {
        match s.write(&msg[sent..]) {
            Ok(0) => break,
            Ok(n) => sent += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => {
                warn!("[Dpi_blocking_write] {}", e);
                break;
            }
        }
    }
    sent == msg.len()
}

/// `Dpi_blocking_read` — lit tout ce qui arrive (réponses courtes, le
/// serveur écrit tout d'un coup). `None` = erreur et aucune donnée.
/// La condition de boucle amont (`st == buf_sz`) est conservée : une
/// lecture courte termine la lecture, comme `read()` POSIX.
fn blocking_read(s: &mut TcpStream) -> Option<Vec<u8>> {
    const BUF_SZ: usize = 8 * 1024;
    let mut buf = vec![0u8; BUF_SZ];
    let mut out = Vec::with_capacity(BUF_SZ);
    loop {
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if n != BUF_SZ {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => {
                warn!("[Dpi_blocking_read] {}", e);
                break;
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

// --------------------------------------------------------------------

/// `Dpi_process_dbuf` — accumule et découpe les tokens reçus.
/// Garde `conn_valid` verbatim : `parse_token` peut aborter la chaîne.
fn process_dbuf(chain: &mut Chain, op: i32, dbuf: &[u8], conn: &Rc<RefCell<DpiConn>>) {
    if op == crate::io::IO_READ {
        conn.borrow_mut().buf.extend_from_slice(dbuf);
        // 'conn' doit être revalidé : parse_token PEUT aborter.
        loop {
            if !conn_valid(conn) {
                break;
            }
            let has_tok = {
                let mut c = conn.borrow_mut();
                get_token(&mut c) != -1
            };
            if !has_tok {
                break;
            }
            parse_token(chain, conn);
        }
    }
    // IOClose : non utilisé en amont.
}

// --------------------------------------------------------------------
// dpid : démarrage, clés, découverte de port
// --------------------------------------------------------------------

/// `Dpi_start_dpid` — lance le démon si possible.
/// `false` ≙ `1` amont (erreur), `true` ≙ `0` (démarré).
///
/// Ordre verbatim : `~/.dillo/dpid`, `<bindir>/dpid` (≙ répertoire de
/// l'exécutable courant), `dpid` dans le `PATH`. Sur Windows le suffixe
/// `.exe` est essayé en second pour les chemins explicites.
fn start_dpid() -> bool {
    let home = dlib::get_home_dir();
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    {
        let mut p = std::path::PathBuf::from(String::from_utf8_lossy(&home).into_owned());
        p.push(".dillo");
        p.push("dpid");
        candidates.push(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("dpid"));
        }
    }
    candidates.push(std::path::PathBuf::from("dpid"));

    for cand in &candidates {
        let spawned = std::process::Command::new(cand)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        let ok = match spawned {
            Ok(_) => true,
            Err(_) if cfg!(windows) && cand.extension().is_none() => {
                // Portabilité : `execl` trouve le fichier exact ; Windows
                // exige l'extension pour un chemin explicite.
                std::process::Command::new(cand.with_extension("exe"))
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .is_ok()
            }
            Err(_) => false,
        };
        if ok {
            return true;
        }
    }
    warn!("Dpi_start_dpid: can't start dpid");
    false
}

/// `Dpi_read_comm_keys` — lit `~/.dillo/dpid_comm_keys` :
/// `<port> <clé-hexa>` (`strtol` puis les hexadécimaux après le
/// premier caractère de queue — verbatim).
/// Retourne `(port, clé)` ou `None`.
fn read_comm_keys() -> Option<(i32, Vec<u8>)> {
    let home = dlib::get_home_dir();
    let mut fname = std::path::PathBuf::from(String::from_utf8_lossy(&home).into_owned());
    fname.push(".dillo");
    fname.push("dpid_comm_keys");
    let line = std::fs::read_to_string(&fname)
        .ok()
        .and_then(|s| s.lines().next().map(str::to_owned))?;
    let b = line.as_bytes();
    // `strtol` : saute les espaces, signe optionnel, chiffres.
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let port: i32 = line[..i.max(start)].trim().parse().unwrap_or(-1);
    // `tail` pointe sur le premier non-chiffre ; la boucle amont lit
    // `tail[i+1]` tant qu'hexa — i.e. la clé commence après le séparateur.
    let mut key = Vec::with_capacity(31);
    if i < b.len() {
        let mut j = i + 1;
        while j < b.len() && key.len() < 31 && dlib::is_xdigit(b[j]) {
            key.push(b[j]);
            j += 1;
        }
    }
    if port <= 0 || key.is_empty() {
        warn!(
            "[Dpi_read_comm_keys] empty/invalid file: {}",
            fname.display()
        );
        return None;
    }
    SHARED_KEY.with(|k| k.borrow_mut().clone_from(&key));
    Some((port, key))
}

/// `Dpi_make_socket_fd` + `connect` — socket TCP `127.0.0.1:port` en
/// `TCP_NODELAY` (verbatim « avoid delays »).
fn connect_loopback(port: i32) -> Option<TcpStream> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], u16::try_from(port).ok()?));
    let s = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .or_else(|_| TcpStream::connect(addr))
        .ok()?;
    let _ = s.set_nodelay(true);
    Some(s)
}

/// `Dpi_check_dpid_ids` — test de connexion au port de supervision.
fn check_dpid_ids() -> bool {
    let Some((port, _)) = read_comm_keys() else {
        return false;
    };
    let Some(s) = connect_loopback(port) else {
        debug!("Dpi_check_dpid_ids: connect failed");
        return false;
    };
    drop(s);
    true
}

/// `Dpi_check_dpid` — `0` OK, `1` démarrage en cours, `2` erreur.
fn check_dpid(num_tries: i32) -> i32 {
    if check_dpid_ids() {
        DPID_STARTING.with(|s| s.set(0));
        return 0;
    }
    let starting = DPID_STARTING.with(std::cell::Cell::get);
    let ret = if starting == 0 {
        if start_dpid() {
            DPID_STARTING.with(|s| s.set(1));
            1
        } else {
            2
        }
    } else if starting + 1 < num_tries {
        DPID_STARTING.with(|s| s.set(starting + 1));
        1
    } else {
        DPID_STARTING.with(|s| s.set(0));
        2
    };
    debug!(
        "Dpi_check_dpid: {}",
        if ret == 0 {
            "OK"
        } else if ret == 1 {
            "EAGAIN"
        } else {
            "ERROR"
        }
    );
    ret
}

/// Extension `OnionBit` — `no_dpi` (dillorc) : `blocking_start_dpid`
/// échoue immédiatement, donc aucun `dpid` n'est spawné et aucune
/// socket dpip n'est ouverte. Sans ce court-circuit, l'absence de
/// dpid coûte 12 essais × 250 ms **par appel** (3 s par requête via
/// `send_blocking_cmd("cookies")`) — et un `dpid` étranger trouvé
/// dans le `PATH` pourrait être détourné.
static DPI_DISABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Active la coupure dpi (appelé une fois au démarrage, pref
/// `no_dpi`). Idempotent — les appels dpi rendent `None`/`DpidERROR`.
pub fn set_dpi_disabled(disabled: bool) {
    DPI_DISABLED.store(disabled, std::sync::atomic::Ordering::Relaxed);
}

/// `Dpi_blocking_start_dpid` — `0` OK / `2` erreur (12 essais × 250 ms
/// ≙ 3 s — la boucle UI est bloquée comme en amont).
fn blocking_start_dpid() -> i32 {
    const N_TRIES: i32 = 12;
    if DPI_DISABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return 2;
    }
    let mut try_n = 0;
    loop {
        let cst = check_dpid(N_TRIES);
        if cst != 1 {
            return cst;
        }
        try_n += 1;
        warn!("Dpi_blocking_start_dpid: try {}", try_n);
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// `Dpi_get_server_port` — interroge dpid (`check_server`) pour le port
/// du serveur `server_name`. `-1` en cas d'erreur.
fn get_server_port(server_name: &str) -> i32 {
    if server_name.is_empty() {
        return -1;
    }
    debug!("Dpi_get_server_port: server_name = [{}]", server_name);

    let Some((dpid_port, _)) = read_comm_keys() else {
        return -1;
    };
    let Some(mut sock) = connect_loopback(dpid_port) else {
        warn!("Dpi_get_server_port: connect failed");
        return -1;
    };
    let Some(request) = dpip::build_cmd("cmd=%s msg=%s", &["check_server", server_name]) else {
        return -1;
    };
    if !blocking_write(&mut sock, request.as_bytes()) {
        warn!("Dpi_get_server_port: write failed");
        return -1;
    }
    let Some(rply) = blocking_read(&mut sock) else {
        warn!("Dpi_get_server_port: can't read server port from dpid.");
        return -1;
    };
    let rply = String::from_utf8_lossy(&rply).into_owned();
    let Some(cmd) = dpip::get_attr(&rply, "cmd") else {
        return -1;
    };
    if cmd != "send_data" {
        return -1;
    }
    let Some(port_str) = dpip::get_attr(&rply, "msg") else {
        return -1;
    };
    debug!("Dpi_get_server_port: rply={} port_str={}", rply, port_str);
    port_str.parse().unwrap_or(-1)
}

/// `Dpi_connect_socket` — connexion authentifiée au serveur `server_name`
/// (demande du port à dpid, puis envoi de la clé partagée).
fn connect_socket(server_name: &str) -> Option<TcpStream> {
    let dpi_port = get_server_port(server_name);
    if dpi_port == -1 {
        debug!(
            "Dpi_connect_socket: can't get port number for {}",
            server_name
        );
        return None;
    }
    debug!(
        "Dpi_connect_socket: server={} port={}",
        server_name, dpi_port
    );

    let mut sock = connect_loopback(dpi_port)?;
    let key = SHARED_KEY.with(|k| k.borrow().clone());
    let key_str = String::from_utf8_lossy(&key).into_owned();
    let Some(cmd) = dpip::build_cmd("cmd=%s msg=%s", &["auth", &key_str]) else {
        warn!("[Dpi_connect_socket] Can't make auth message.");
        return None;
    };
    if !blocking_write(&mut sock, cmd.as_bytes()) {
        warn!("[Dpi_connect_socket] Can't send auth message.");
        return None;
    }
    Some(sock)
}

// --------------------------------------------------------------------

/// `a_Dpi_ccc` — callback de chaîne du module dpi.
///
/// - branche 1 (commande) : `OpStart` démarre dpid au besoin, connecte
///   le serveur et publie le « fd » + `DpidOK` ; `OpSend`/`OpEnd`/
///   `OpAbort` relaient vers la couche IO.
/// - branche 2 (réception) : `OpSend` alimente le tokeniseur,
///   `OpEnd`/`OpAbort` libèrent.
#[allow(clippy::too_many_lines)] // verbatim : 4 branches CCC amont
fn dpi_ccc(
    chain: &mut Chain,
    op: i32,
    branch: i32,
    dir: i32,
    info: LinkId,
    d1: ChainData,
    d2: ChainData,
) {
    match (branch, dir, op) {
        // ------- branche 1 BCK : commandes vers le serveur -------
        (1, BCK, OP_START) => {
            let server = d1.as_str().map(str::to_owned);
            let st = blocking_start_dpid();
            if st == 0 {
                if let Some(sock) = server.as_deref().and_then(connect_socket) {
                    let sock_id = chain
                        .module_mut::<IoEngine>()
                        .map_or(-1, |e| e.insert_std_socket(sock));
                    if sock_id == -1 {
                        chain.fcb(OP_ABORT, info, ChainData::None, ChainData::None);
                        chain.free_link(info);
                        return;
                    }
                    chain.set_local_key(info, sock_id);
                    chain.link_new(info, dpi_ccb(), BCK, io_ccb(), 1, 1);
                    chain.bcb(OP_START, info, ChainData::None, ChainData::None);
                    // Publier le fd vers le serveur et le client.
                    chain.bcb(
                        OP_SEND,
                        info,
                        ChainData::Int(sock_id),
                        ChainData::Str("FD".to_string()),
                    );
                    chain.fcb(
                        OP_SEND,
                        info,
                        ChainData::Int(sock_id),
                        ChainData::Str("FD".to_string()),
                    );
                    chain.fcb(
                        OP_SEND,
                        info,
                        ChainData::None,
                        ChainData::Str("DpidOK".to_string()),
                    );
                } else {
                    // ≙ `a_Dpi_ccc(OpAbort, 1, FWD, …)`
                    dpi_ccc(
                        chain,
                        OP_ABORT,
                        1,
                        FWD,
                        info,
                        ChainData::None,
                        ChainData::None,
                    );
                }
            } else {
                warn!("dpi.c: can't start dpi daemon");
                dpi_ccc(
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
        (1, BCK, OP_END | OP_ABORT) => {
            chain.bcb(op, info, ChainData::None, ChainData::None);
            chain.free_link(info);
        }
        // ------- branche 1 FWD : statut remontant -------
        (1, FWD, OP_ABORT) => {
            chain.fcb(OP_ABORT, info, ChainData::None, d2);
            chain.free_link(info);
        }
        // ------- branche 2 FWD : données reçues du serveur -------
        (2, FWD, OP_SEND) => {
            // `Data1 = dbuf`
            let conn = chain.local_key::<Rc<RefCell<DpiConn>>>(info).cloned();
            if let (Some(c), ChainData::Buf(dbuf)) = (conn, d1) {
                let n = usize::try_from(dbuf.size.max(0)).unwrap_or(0);
                let data = &dbuf.buf[..n.min(dbuf.buf.len())];
                if dbuf.code == 0 && !data.is_empty() {
                    process_dbuf(chain, crate::io::IO_READ, data, &c);
                }
            }
        }
        (2, FWD, OP_ABORT | OP_END) => {
            if op == OP_ABORT {
                warn!("Dpi server aborted the connection");
            }
            let conn = chain.local_key::<Rc<RefCell<DpiConn>>>(info).cloned();
            chain.fcb(op, info, ChainData::None, ChainData::None);
            if let Some(c) = conn {
                conn_free(&c);
            }
            chain.free_link(info);
        }
        // ------- branche 2 BCK : contrôle de la réception -------
        (2, BCK, OP_START) => {
            let conn = conn_new(info);
            // Hack verbatim : HTTP à travers le framework DPI.
            if d2.as_str() == Some("http") {
                conn.borrow_mut().send2eof = true;
            }
            chain.set_local_key(info, conn);
            chain.link_new(info, dpi_ccb(), BCK, io_ccb(), 2, 2);
            chain.bcb(OP_START, info, ChainData::None, ChainData::None);
        }
        (2, BCK, OP_SEND) => {
            if d2.is_tag("FD") {
                chain.bcb(OP_SEND, info, d1, d2);
            }
        }
        (2, BCK, OP_ABORT) => {
            chain.bcb(OP_ABORT, info, ChainData::None, ChainData::None);
            let conn = chain.local_key::<Rc<RefCell<DpiConn>>>(info).cloned();
            if let Some(c) = conn {
                conn_free(&c);
            }
            chain.free_link(info);
        }
        _ => warn!("Unused CCC DPI"),
    }
}

/// `a_Dpi_ccc` prête à enregistrer.
#[must_use]
pub fn dpi_ccb() -> ChainCb {
    Rc::new(dpi_ccc)
}

/// `a_Dpi_dillo_exit` — no-op amont (désactivé : servirait à expirer
/// les cookies de session).
pub fn dillo_exit() {}

/// `a_Dpi_send_blocking_cmd` — commande synchrone vers un serveur dpi.
/// Retourne le tag de réponse dpip, `None` en cas d'erreur.
#[must_use]
pub fn send_blocking_cmd(server_name: &str, cmd: &str) -> Option<String> {
    if blocking_start_dpid() != 0 {
        return None;
    }
    let mut sock = connect_socket(server_name)?;
    if !blocking_write(&mut sock, cmd.as_bytes()) {
        warn!("[a_Dpi_send_blocking_cmd] Can't send message.");
        return None;
    }
    let rply = blocking_read(&mut sock)?;
    Some(String::from_utf8_lossy(&rply).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(buf: &[u8]) -> DpiConn {
        let c = conn_new(0);
        let mut b = c.borrow_mut();
        b.buf.extend_from_slice(buf);
        DpiConn {
            in_tag: b.in_tag,
            send2eof: b.send2eof,
            data_total_size: 0,
            data_recv_size: 0,
            buf: b.buf.clone(),
            buf_idx: b.buf_idx,
            tok_idx: b.tok_idx,
            tok_size: b.tok_size,
            tok_is_tag: b.tok_is_tag,
            info_recv: 0,
            valid: Cell::new(true),
        }
    }

    #[test]
    fn token_tag_split() {
        // Un tag complet puis début du suivant (EOT = ` '>` verbatim).
        let mut c = conn(b"<a='x' '>reste");
        assert_eq!(get_token(&mut c), 0);
        assert_eq!(&c.buf[c.tok_idx..c.tok_idx + c.tok_size], b"<a='x' '>");
        // Plus assez de données → -1, curseur sur "reste" non-'<'.
        assert_eq!(get_token(&mut c), -1);
    }

    #[test]
    fn token_send2eof_raw() {
        let mut c = conn(b"raw<html>data");
        c.send2eof = true;
        assert_eq!(get_token(&mut c), 0);
        assert_eq!(c.tok_size, b"raw<html>data".len());
        assert_eq!(get_token(&mut c), -1); // buf vidé
    }

    #[test]
    fn token_tag_partial_waits() {
        let mut c = conn(b"<a='x'");
        assert_eq!(get_token(&mut c), -1); // EOT incomplet
        c.buf.extend_from_slice(b" '>");
        assert_eq!(get_token(&mut c), 0);
        assert_eq!(&c.buf[c.tok_idx..c.tok_idx + c.tok_size], b"<a='x' '>");
    }

    #[test]
    fn comm_keys_parse() {
        // "44000 a1b2" → port 44000, clé après le séparateur (verbatim :
        // tail[i+1] saute le premier caractère de queue).
        let tmp = std::env::temp_dir().join("dpi_comm_keys_test");
        std::fs::create_dir_all(&tmp).ok();
        let f = tmp.join("dpid_comm_keys");
        std::fs::write(&f, "44000 a1b2c3\n").ok();
        // parse inline (read_comm_keys utilise le homedir) — on teste
        // la logique de découpe ici directement.
        let line = "44000 a1b2c3";
        let b = line.as_bytes();
        let mut i = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let port: i32 = line[..i].parse().unwrap_or(-1);
        assert_eq!(port, 44000);
        let mut key = Vec::new();
        let mut j = i + 1;
        while j < b.len() && dlib::is_xdigit(b[j]) {
            key.push(b[j]);
            j += 1;
        }
        assert_eq!(key, b"a1b2c3");
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn conn_validity_flag() {
        let c = conn_new(7);
        assert!(conn_valid(&c));
        conn_free(&c);
        assert!(!conn_valid(&c));
        assert_eq!(c.borrow().info_recv, 7);
    }

    #[test]
    fn no_dpi_bloque_avant_toute_socket() {
        // Extension OnionBit : `no_dpi` court-circuite
        // `blocking_start_dpid` — `send_blocking_cmd` rend `None`
        // instantanément (pas de retry 12×250 ms, aucun `dpid`
        // spawné, aucune socket dpip ouverte).
        set_dpi_disabled(true);
        let t = std::time::Instant::now();
        assert!(send_blocking_cmd("cookies", "cmd=<x>").is_none());
        assert!(t.elapsed() < std::time::Duration::from_millis(100));
        set_dpi_disabled(false);
    }
}
