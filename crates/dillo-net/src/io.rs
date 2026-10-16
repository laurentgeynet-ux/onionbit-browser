//! Port of `src/IO/IO.c` + `src/IO/iowatch.cc` + `src/IO/IO.h` — moteur
//! d'I/O événementiel de Dillo.
//!
//! Mapping (ADR-0003) :
//! - `ValidIOs` (klist globale) → le registre par clé `IoEngine::ios`
//!   (statiques amont → instance).
//! - `a_IOwatch_add_fd` (`Fl::add_fd`) → une tâche tokio par `IoData`
//!   (`readable`/`writable().await` = readiness, `try_read`/`try_write`
//!   = `read(2)`/`write(2)` non-bloquants). `O_NONBLOCK|FD_CLOEXEC` est
//!   implicite (tokio/mio).
//! - Callbacks d'activité → `IoEvent` posté en `mpsc` vers le thread
//!   principal : les opérations CCC restent synchrones et réentrantes
//!   sur un seul thread, comme amont (TODO amont sur la réentrance).
//! - `dClose(fd)` → drop du [`tokio::net::TcpStream`] (le BUG amont de
//!   double-close en Abort disparaît avec l'ownership).
//! - `a_Tls_connection(fd)` → futur variant `IoStream::Tls` (`tls.c`
//!   non porté — le point de dispatch est matérialisé).
//!
//! `io_ccc` reproduit `a_IO_ccc` verbatim (branches 1B/1F/2B/2F).

use std::collections::HashMap;
use std::io;

use dillo_common::dlib::Dstr;
use tokio::io::Interest;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, trace, warn};

use crate::chain::{
    Chain, ChainData, DataBuf, LinkId, BCK, FWD, OP_ABORT, OP_END, OP_SEND, OP_START,
};

/// `IORead | IOWrite` (op de l'`IOData_t`).
pub const IO_READ: i32 = 0;
pub const IO_WRITE: i32 = 1;

/// `IOBufLen`.
pub const IO_BUFLEN: usize = 8192;

/// `IO_StopRd`/`IO_StopWr`/`IO_StopRdWr` (shutdown).
pub const IO_STOP_RD: i32 = 1;
pub const IO_STOP_WR: i32 = 2;
pub const IO_STOP_RDWR: i32 = 3;

/// Clé primaire (`io->Key`, `a_Klist_insert`).
pub type IoKey = i32;

/// Flux I/O — `Plain` = socket nu ; `Tls` = connexion rustls partagée
/// (`Arc<TlsConn>` ≙ le `io->FD`/`conn` partagé lecteur+écrivain amont,
/// cf. `a_Tls_connection`).
#[derive(Debug)]
pub enum IoStream {
    /// Socket TCP nu.
    Plain(TcpStream),
    /// Connexion TLS (rustls) partagée entre les IO reader/writer.
    Tls(std::sync::Arc<crate::tls::TlsConn>),
}

impl IoStream {
    /// `read(fd, buf, len)` non-bloquant (≙ EAGAIN → `WouldBlock`).
    fn try_read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.try_read(buf),
            Self::Tls(t) => t.try_read(buf),
        }
    }

    /// `write(fd, buf, len)` non-bloquant.
    fn try_write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.try_write(buf),
            Self::Tls(t) => t.try_write(buf),
        }
    }

    /// readiness du fd (≙ retour de `select`/watch).
    async fn ready(&self, interest: Interest) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.ready(interest).await.map(|_| ()),
            Self::Tls(t) => t.sock.ready(interest).await.map(|_| ()),
        }
    }

    /// `dup(fd)` : deuxième handle sur la même socket — amont partage
    /// le *même* `io->FD` entre lecteur et écrivain. `Plain` passe par
    /// `socket2` + `from_std` dans le contexte runtime ; `Tls` partage
    /// l'`Arc` (la connexion rustls reste unique, comme `conn` amont).
    fn try_clone(&self, rt: &tokio::runtime::Handle) -> Option<Self> {
        match self {
            Self::Plain(s) => {
                let sock = socket2::SockRef::from(s).try_clone().ok()?;
                let std_s: std::net::TcpStream = sock.into();
                std_s.set_nonblocking(true).ok()?;
                let _guard = rt.enter();
                tokio::net::TcpStream::from_std(std_s).ok().map(Self::Plain)
            }
            Self::Tls(t) => Some(Self::Tls(t.clone())),
        }
    }

    /// `shutdown(fd, SHUT_RDWR)` — atteint tous les détenteurs de clones
    /// (fidèle à `dClose` : fermer le côté écriture tue aussi la lecture).
    /// `Tls` envoie `close_notify` d'abord (`a_Tls_close_by_fd`).
    fn shutdown_both(&self) -> io::Result<()> {
        match self {
            Self::Plain(s) => socket2::SockRef::from(s).shutdown(std::net::Shutdown::Both),
            Self::Tls(t) => t.shutdown_both(),
        }
    }
}

/// `IOData_t`.
struct IoData {
    /// `Key` — clé klist.
    key: IoKey,
    /// `Op` — `IO_READ`/`IO_WRITE`.
    op: i32,
    /// `FD` — identifiant de la table `socks` (-1 avant l'op `"FD"`).
    sock_id: i32,
    /// `FD` — flux (≙ descripteur ; `None` avant l'op `"FD"`).
    stream: Option<IoStream>,
    /// `Status` — errno non nul sur échec.
    status: i32,
    /// `Buf` — buffer interne (read : octets lus ; write : à écrire).
    buf: Vec<u8>,
    /// `Info` — lien CCC propriétaire.
    info: LinkId,
    /// tâche reactor (remplace le watch FLTK).
    task: Option<JoinHandle<()>>,
    /// Canal vers la tâche writer (`io->Buf` amont) — `Data`/`Swap`.
    wr_tx: Option<mpsc::UnboundedSender<IoMsg>>,
    /// Canal de contrôle vers la tâche reader (swap TLS).
    ctl_tx: Option<mpsc::UnboundedSender<IoMsg>>,
}

/// Message vers les tâches I/O armées — `Data` ≙ `dStr_append_l(io->Buf)`
/// (writer), `Swap` ≙ re-résolution `a_Tls_connection(fd)` (upgrade TLS
/// sur tunnel CONNECT — reader comme writer).
enum IoMsg {
    Data(Vec<u8>),
    Swap(IoStream),
}

/// Événement reactor → thread principal (callbacks `IO_fd_*_cb` amont).
pub enum IoEvent {
    /// Données lues (≙ `IO_read` ayant rempli `io->Buf`).
    ReadData { key: IoKey, bytes: Vec<u8> },
    /// `read == 0` (≙ `St == 0` → `OpEnd`).
    Eof { key: IoKey },
    /// Erreur lecture (≙ `io->Status` → `OpAbort` branche 2).
    ReadErr { key: IoKey, status: i32 },
    /// Buffer d'écriture vide (upstream : watch retirée, rien au CCC).
    Written { key: IoKey },
    /// Erreur écriture (≙ `OpAbort` branche 1).
    WriteErr { key: IoKey, status: i32 },
}

/// `IoEngine` — `ValidIOs` + reactor (iowatch). Enregistré comme module
/// `Chain` : `chain.set_module(IoEngine::new(rt))`.
pub struct IoEngine {
    ios: HashMap<IoKey, IoData>,
    /// Table des descripteurs ouverts — `int fd` amont ≙ `sock_id`.
    /// Le producteur insère le stream et passe `ChainData::Int(id)` avec
    /// le tag `"FD"` ; chaque consommateur obtient un *clone* (≙ `dup`),
    /// le numéro restant valide jusqu'à `dClose` (`IO_STOP_RDWR`).
    socks: HashMap<i32, IoStream>,
    next_sock: i32,
    next_key: IoKey,
    rt: tokio::runtime::Handle,
    event_tx: mpsc::UnboundedSender<IoEvent>,
    event_rx: mpsc::UnboundedReceiver<IoEvent>,
}

impl std::fmt::Debug for IoEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IoEngine")
            .field("ios", &self.ios.len())
            .finish_non_exhaustive()
    }
}

impl IoEngine {
    /// `a_IOwatch_add_fd` global : construit le moteur sur le runtime.
    #[must_use]
    pub fn new(rt: tokio::runtime::Handle) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            ios: HashMap::new(),
            socks: HashMap::new(),
            next_sock: 1,
            next_key: 1,
            rt,
            event_tx,
            event_rx,
        }
    }

    /// Canal d'événements (les producteurs externes le clonent si besoin).
    #[must_use]
    pub fn event_sender(&self) -> mpsc::UnboundedSender<IoEvent> {
        self.event_tx.clone()
    }

    /// `socket()` côté producteur : enregistre un stream, retourne son
    /// « fd » (transmis via `ChainData::Int` + tag `"FD"`).
    pub fn insert_socket(&mut self, s: IoStream) -> i32 {
        let id = self.next_sock;
        self.next_sock += 1;
        self.socks.insert(id, s);
        id
    }

    /// `fd` → clone du stream (l'entrée reste — un fd peut équiper
    /// plusieurs `IOData`, comme le `io->FD` partagé amont).
    fn clone_socket(&self, id: i32) -> Option<IoStream> {
        self.socks.get(&id).and_then(|s| s.try_clone(&self.rt))
    }

    /// Retire l'entrée du « fd » sans le fermer — préalable au wrap
    /// TLS (`a_Tls_connect` re-prend possession du descripteur amont).
    pub fn take_socket(&mut self, id: i32) -> Option<IoStream> {
        self.socks.remove(&id)
    }

    /// `dClose(fd)` explicite : l'entrée disparaît et le socket meurt
    /// pour tous les clones (lecteur+écrivain partagés).
    pub fn remove_socket(&mut self, id: i32) {
        if let Some(s) = self.socks.remove(&id) {
            let _ = s.shutdown_both();
        }
    }

    /// `a_Tls_close_by_fd` (hors `connecting`) : `close_notify` envoyé,
    /// le fd reste ouvert — le `dClose` relève du teardown IO amont.
    pub fn close_notify(&mut self, id: i32) {
        if let Some(IoStream::Tls(t)) = self.socks.get(&id) {
            t.close_notify();
        }
    }

    /// `a_Tls_connection(fd)` : remplace le stream d'un « fd » (wrap
    /// TLS après tunnel CONNECT) et pousse le nouveau handle aux ios
    /// déjà armés — ≙ re-lookup par-fd à chaque `read`/`write` amont.
    pub fn swap_socket(&mut self, id: i32, s: IoStream) {
        self.socks.insert(id, s);
        let keys: Vec<IoKey> = self
            .ios
            .iter()
            .filter(|(_, io)| io.sock_id == id)
            .map(|(k, _)| *k)
            .collect();
        for key in keys {
            let fresh = self.clone_socket(id);
            let Some(io) = self.ios.get_mut(&key) else {
                continue;
            };
            if let Some(tx) = &io.ctl_tx {
                if let Some(s) = fresh {
                    let _ = tx.send(IoMsg::Swap(s));
                }
            } else if let Some(tx) = &io.wr_tx {
                if let Some(s) = fresh {
                    let _ = tx.send(IoMsg::Swap(s));
                }
            } else {
                io.stream = fresh;
            }
        }
    }

    /// `IO_new` + `IO_ins` — crée et enregistre un `IoData`.
    fn io_new(&mut self, op: i32, info: LinkId) -> IoKey {
        let key = self.next_key;
        self.next_key += 1;
        trace!(key, op, "IO_ins");
        self.ios.insert(
            key,
            IoData {
                key,
                op,
                sock_id: -1,
                stream: None,
                wr_tx: None,
                ctl_tx: None,
                status: 0,
                buf: Vec::with_capacity(IO_BUFLEN),
                info,
                task: None,
            },
        );
        key
    }

    /// `IO_del` + `IO_free` — retire du registre (le drop libère).
    fn io_free(&mut self, key: IoKey) -> Option<IoData> {
        let io = self.ios.remove(&key);
        debug_assert!(io.as_ref().is_none_or(|io| io.key == key));
        io
    }

    /// `IO_get`.
    fn io_get(&mut self, key: IoKey) -> Option<&mut IoData> {
        self.ios.get_mut(&key)
    }

    /// `IO_submit` — spawn de la tâche reactor sur `io->FD`.
    /// Upstream exige `FD >= 0` (sinon `MSG_ERR` + abandon silencieux).
    /// Le handle pris est re-cloné depuis `socks` (≙ `io->FD` re-résolu
    /// au moment de l'armement : un upgrade TLS intermédiaire est couvert).
    fn submit(&mut self, key: IoKey) {
        let Some(io) = self.ios.get_mut(&key) else {
            return;
        };
        if io.task.is_some() {
            return; // watch déjà armée
        }
        let (op, sock_id, stored, staged) = (
            io.op,
            io.sock_id,
            io.stream.take(),
            std::mem::take(&mut io.buf),
        );
        let stream = self.clone_socket(sock_id).or(stored);
        if stream.is_none() {
            error!("IO_submit: FD not initialized");
            if let Some(io) = self.ios.get_mut(&key) {
                io.stream = stream;
            }
            return;
        }
        let tx = self.event_tx.clone();
        let Some(io) = self.ios.get_mut(&key) else {
            return;
        };
        io.task = Some(if op == IO_READ {
            let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
            io.ctl_tx = Some(ctl_tx);
            self.rt.spawn(read_task(key, stream, tx, ctl_rx))
        } else {
            let (wr_tx, wr_rx) = mpsc::unbounded_channel();
            io.wr_tx = Some(wr_tx);
            self.rt.spawn(write_task(key, stream, tx, wr_rx, staged))
        });
    }

    /// `IO_close_fd` — retrait watch (abort tâche) + `IO_del` +
    /// `dClose` **uniquement** si `IO_StopRdWr`. `StopRd`/`StopWr` seuls
    /// laissent le fd ouvert, comme amont : le shutdown est fait via la
    /// table `socks` pour atteindre aussi les clones partis en tâche.
    fn close_fd(&mut self, key: IoKey, close_code: i32) {
        trace!(key, close_code, "IO_close_fd");
        let Some(io) = self.ios.get_mut(&key) else {
            return;
        };
        if let Some(t) = io.task.take() {
            t.abort();
        }
        io.wr_tx = None;
        io.ctl_tx = None;
        if close_code == IO_STOP_RDWR {
            // `dClose(fd)` : le numéro devient invalide et le socket
            // meurt pour tous les clones (lecteur+écrivain partagés).
            if let Some(s) = self.socks.remove(&io.sock_id) {
                let _ = s.shutdown_both();
            }
            io.stream = None;
        }
        let _ = self.io_free(key);
    }

    /// File d'écriture du canal 1B (`dStr_append_l(io->Buf, dbuf)` amont :
    /// ici, `io->Buf` vit dans la tâche ; `wr_tx` joue le rôle du buffer).
    fn write_data(&mut self, key: IoKey, bytes: Vec<u8>) {
        if let Some(io) = self.ios.get_mut(&key) {
            if let Some(tx) = &io.wr_tx {
                let _ = tx.send(IoMsg::Data(bytes));
            } else {
                // FD pas encore fixé ou tâche morte : stage dans io->Buf
                io.buf.extend_from_slice(&bytes);
            }
        }
    }

    /// Vide le canal d'événements (pompe du reactor vers le CCC —
    /// séparée du dispatch pour pouvoir emprunter `Chain` ensuite).
    fn drain_events(&mut self) -> Vec<IoEvent> {
        let mut v = Vec::new();
        while let Ok(ev) = self.event_rx.try_recv() {
            v.push(ev);
        }
        v
    }
}

/// Pompe les événements reactor → ops CCC (thread principal).
/// Équivalent du dispatch `IO_fd_read_cb`/`IO_fd_write_cb` : le check
/// `IO_get` avant chaque dispatch reproduit la garde réentrance amont
/// (un `OpSend` peut avoir aborté la chaîne — `io_get` → `None`).
pub fn poll_io(chain: &mut Chain) {
    let events = chain
        .module_mut::<IoEngine>()
        .map_or_else(Vec::new, IoEngine::drain_events);
    for ev in events {
        match ev {
            IoEvent::ReadData { key, bytes } => {
                let send = {
                    let Some(eng) = chain.module_mut::<IoEngine>() else {
                        return;
                    };
                    eng.io_get(key).map(|io| {
                        io.buf.extend_from_slice(&bytes);
                        io.info
                    })
                };
                if let Some(info) = send {
                    io_ccc(
                        chain,
                        OP_SEND,
                        2,
                        FWD,
                        info,
                        ChainData::Int(key),
                        ChainData::None,
                    );
                }
            }
            IoEvent::Eof { key } => {
                let info = {
                    let Some(eng) = chain.module_mut::<IoEngine>() else {
                        return;
                    };
                    eng.io_get(key).map(|io| io.info)
                };
                if let Some(info) = info {
                    io_ccc(
                        chain,
                        OP_END,
                        2,
                        FWD,
                        info,
                        ChainData::Int(key),
                        ChainData::None,
                    );
                }
            }
            IoEvent::ReadErr { key, status } => {
                let info = {
                    let Some(eng) = chain.module_mut::<IoEngine>() else {
                        return;
                    };
                    eng.io_get(key).map(|io| {
                        io.status = status;
                        io.info
                    })
                };
                if let Some(info) = info {
                    io_ccc(
                        chain,
                        OP_ABORT,
                        2,
                        FWD,
                        info,
                        ChainData::Int(key),
                        ChainData::None,
                    );
                }
            }
            IoEvent::Written { key: _ } => {
                // upstream : watch retirée, rien remonté au CCC
            }
            IoEvent::WriteErr { key, status } => {
                let info = {
                    let Some(eng) = chain.module_mut::<IoEngine>() else {
                        return;
                    };
                    eng.io_get(key).map(|io| {
                        io.status = status;
                        io.info
                    })
                };
                if let Some(info) = info {
                    io_ccc(
                        chain,
                        OP_ABORT,
                        1,
                        FWD,
                        info,
                        ChainData::None,
                        ChainData::None,
                    );
                }
            }
        }
    }
}

/// `IO_read` + `IO_fd_read_cb` — boucle `read()` jusqu'à `EAGAIN`
/// *avant* d'attendre la readiness (ordre amont) — couvre aussi le
/// plaintext TLS déjà bufferisé dans la connexion rustls.
/// `IoMsg::Swap` ≙ `a_Tls_connection(io->FD)` re-résolu : le handle est
/// remplacé en vol (upgrade après tunnel CONNECT).
/// Poste `ReadData` par morceau, `Eof`/`ReadErr`.
async fn read_task(
    key: IoKey,
    mut stream: Option<IoStream>,
    tx: mpsc::UnboundedSender<IoEvent>,
    mut ctl_rx: mpsc::UnboundedReceiver<IoMsg>,
) {
    let Some(s) = stream.as_mut() else {
        return;
    };
    let mut out = Vec::with_capacity(IO_BUFLEN);
    let mut buf = [0u8; IO_BUFLEN];
    let mut ctl_open = true;
    loop {
        // `IO_read` : drain jusqu'à EAGAIN/EOF/erreur
        enum Stop {
            Eof,
            Err(i32),
        }
        let mut stop = None;
        loop {
            match s.try_read(&mut buf) {
                Ok(0) => {
                    stop = Some(Stop::Eof);
                    break;
                }
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    stop = Some(Stop::Err(e.raw_os_error().unwrap_or(-1)));
                    break;
                }
            }
        }
        if !out.is_empty() {
            let _ = tx.send(IoEvent::ReadData {
                key,
                bytes: std::mem::take(&mut out),
            });
        }
        match stop {
            Some(Stop::Eof) => {
                let _ = tx.send(IoEvent::Eof { key });
                return;
            }
            Some(Stop::Err(status)) => {
                let _ = tx.send(IoEvent::ReadErr { key, status });
                return;
            }
            None => {}
        }
        // EAGAIN → ret=TRUE amont (watch conservée) : attendre l'activité
        // — ou un swap de stream (upgrade TLS).
        if ctl_open {
            tokio::select! {
                r = s.ready(Interest::READABLE) => {
                    if r.is_err() {
                        let _ = tx.send(IoEvent::ReadErr { key, status: -1 });
                        return;
                    }
                }
                m = ctl_rx.recv() => {
                    match m {
                        Some(IoMsg::Swap(ns)) => *s = ns,
                        Some(IoMsg::Data(_)) => {}
                        None => ctl_open = false,
                    }
                }
            }
        } else if s.ready(Interest::READABLE).await.is_err() {
            let _ = tx.send(IoEvent::ReadErr { key, status: -1 });
            return;
        }
    }
}

/// `IO_write` + `IO_fd_write_cb` — buffer → socket jusqu'à drain ou
/// EAGAIN (watch conservée) ; `WriteErr` sur erreur fatale.
async fn write_task(
    key: IoKey,
    mut stream: Option<IoStream>,
    tx: mpsc::UnboundedSender<IoEvent>,
    mut wr_rx: mpsc::UnboundedReceiver<IoMsg>,
    staged: Vec<u8>,
) {
    let Some(s) = stream.as_mut() else {
        return;
    };
    let mut buf: Vec<u8> = staged;
    let mut wr_open = true;
    'outer: loop {
        // drainer les données en attente
        if wr_open {
            loop {
                match wr_rx.try_recv() {
                    Ok(IoMsg::Data(b)) => buf.extend_from_slice(&b),
                    Ok(IoMsg::Swap(ns)) => *s = ns,
                    Err(_) => break,
                }
            }
        }
        if buf.is_empty() {
            if !wr_open {
                break 'outer;
            }
            match wr_rx.recv().await {
                Some(IoMsg::Data(b)) => {
                    buf.extend_from_slice(&b);
                    continue;
                }
                Some(IoMsg::Swap(ns)) => {
                    *s = ns;
                    continue;
                }
                None => {
                    wr_open = false;
                    continue;
                }
            }
        }
        loop {
            // attendre writable — ou un message (données supplémentaires
            // ou swap TLS ≙ `a_Tls_connection` re-résolu).
            if wr_open {
                tokio::select! {
                    r = s.ready(Interest::WRITABLE) => {
                        if r.is_err() {
                            let _ = tx.send(IoEvent::WriteErr { key, status: -1 });
                            break 'outer;
                        }
                    }
                    m = wr_rx.recv() => {
                        match m {
                            Some(IoMsg::Data(b)) => buf.extend_from_slice(&b),
                            Some(IoMsg::Swap(ns)) => *s = ns,
                            None => wr_open = false,
                        }
                        continue 'outer;
                    }
                }
            } else if s.ready(Interest::WRITABLE).await.is_err() {
                let _ = tx.send(IoEvent::WriteErr { key, status: -1 });
                break 'outer;
            }
            match s.try_write(&buf) {
                Ok(n) if n < buf.len() => {
                    // `dStr_erase(io->Buf, 0, St)`
                    buf.drain(..n);
                    // upstream boucle immédiatement ; la readiness peut
                    // avoir été consommée → tour suivant ré-attend
                }
                Ok(_) => {
                    buf.clear();
                    let _ = tx.send(IoEvent::Written { key });
                    continue 'outer;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    let _ = tx.send(IoEvent::WriteErr {
                        key,
                        status: e.raw_os_error().unwrap_or(-1),
                    });
                    break 'outer;
                }
            }
        }
    }
}

/// `a_IO_ccc` — dispatcher verbatim (branches 1B/1F/2B/2F).
/// `info` = lien côté IO ; `Data1 = IOData_t*` amont devient
/// `ChainData::Int(io_key)` (registre `IoEngine`).
// les lints de longueur/valeur reflètent la forme du dispatcher amont
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub fn io_ccc(
    chain: &mut Chain,
    op: i32,
    branch: i32,
    dir: i32,
    info: LinkId,
    d1: ChainData,
    d2: ChainData,
) {
    if !chain.check("a_IO_ccc", op, branch, dir, info) {
        return;
    }
    match (branch, dir) {
        (1, BCK) => {
            // Write data
            match op {
                OP_START => {
                    let Some(eng) = chain.module_mut::<IoEngine>() else {
                        return;
                    };
                    let key = eng.io_new(IO_WRITE, info);
                    chain.set_local_key(info, key);
                }
                OP_SEND => {
                    let Some(&key) = chain.local_key::<IoKey>(info) else {
                        return;
                    };
                    if d2.is_tag("FD") {
                        // `io->FD = *(int*)Data1` — sock_id → clone (≙ fd)
                        if let ChainData::Int(sock_id) = d1 {
                            let Some(eng) = chain.module_mut::<IoEngine>() else {
                                return;
                            };
                            let s = eng.clone_socket(sock_id);
                            if let Some(io) = eng.io_get(key) {
                                io.sock_id = sock_id;
                                io.stream = s;
                            }
                        }
                    } else if let ChainData::Buf(dbuf) = d1 {
                        let Some(eng) = chain.module_mut::<IoEngine>() else {
                            return;
                        };
                        let size = usize::try_from(dbuf.size).unwrap_or(dbuf.buf.len());
                        eng.write_data(key, dbuf.buf[..size.min(dbuf.buf.len())].to_vec());
                        eng.submit(key);
                    }
                }
                OP_END | OP_ABORT => {
                    let Some(&key) = chain.local_key::<IoKey>(info) else {
                        return;
                    };
                    let Some(eng) = chain.module_mut::<IoEngine>() else {
                        return;
                    };
                    if let Some(io) = eng.io_get(key) {
                        if !io.buf.is_empty() {
                            let n = io
                                .buf
                                .iter()
                                .position(|&b| b == b'\n')
                                .unwrap_or(2048)
                                .min(io.buf.len());
                            debug!(
                                "IO_write, closing with pending data not sent: {:?}",
                                Dstr::new(&io.buf).printable(n)
                            );
                        }
                    }
                    eng.close_fd(
                        key,
                        if op == OP_END {
                            IO_STOP_WR
                        } else {
                            IO_STOP_RDWR
                        },
                    );
                    eng.io_free(key);
                    chain.free_link(info);
                }
                _ => warn!("Unused CCC IO 1B"),
            }
        }
        (1, FWD) => {
            // Write-data status
            if op == OP_ABORT {
                let Some(&key) = chain.local_key::<IoKey>(info) else {
                    return;
                };
                let Some(eng) = chain.module_mut::<IoEngine>() else {
                    return;
                };
                eng.close_fd(key, IO_STOP_RDWR);
                eng.io_free(key);
                chain.fcb(OP_ABORT, info, ChainData::None, ChainData::None);
                chain.free_link(info);
            } else {
                warn!("Unused CCC IO 1F");
            }
        }
        (2, BCK) => {
            // Reader side
            match op {
                OP_START => {
                    let Some(eng) = chain.module_mut::<IoEngine>() else {
                        return;
                    };
                    let key = eng.io_new(IO_READ, info);
                    chain.set_local_key(info, key);
                }
                OP_SEND => {
                    if d2.is_tag("FD") {
                        let Some(&key) = chain.local_key::<IoKey>(info) else {
                            return;
                        };
                        if let ChainData::Int(sock_id) = d1 {
                            let Some(eng) = chain.module_mut::<IoEngine>() else {
                                return;
                            };
                            let s = eng.clone_socket(sock_id);
                            if let Some(io) = eng.io_get(key) {
                                io.sock_id = sock_id;
                                io.stream = s;
                            }
                            eng.submit(key);
                        }
                    }
                }
                OP_END | OP_ABORT => {
                    let Some(&key) = chain.local_key::<IoKey>(info) else {
                        return;
                    };
                    let Some(eng) = chain.module_mut::<IoEngine>() else {
                        return;
                    };
                    eng.close_fd(
                        key,
                        if op == OP_END {
                            IO_STOP_RD
                        } else {
                            IO_STOP_RDWR
                        },
                    );
                    eng.io_free(key);
                    chain.free_link(info);
                }
                _ => warn!("Unused CCC IO 2B"),
            }
        }
        (2, FWD) => {
            // Send read-data — `Data1 = io`
            let ChainData::Int(key) = d1 else {
                return;
            };
            match op {
                OP_SEND => {
                    let send = {
                        let Some(eng) = chain.module_mut::<IoEngine>() else {
                            return;
                        };
                        eng.io_get(key).map(|io| {
                            let len = i32::try_from(io.buf.len()).unwrap_or(i32::MAX);
                            let dbuf = DataBuf::new(std::mem::take(&mut io.buf), len, 0);
                            (dbuf, io.info)
                        })
                    };
                    if let Some((dbuf, info2)) = send {
                        chain.fcb(
                            OP_SEND,
                            info2,
                            ChainData::Buf(Box::new(dbuf)),
                            ChainData::None,
                        );
                    }
                }
                OP_END | OP_ABORT => {
                    let info2 = {
                        let Some(eng) = chain.module_mut::<IoEngine>() else {
                            return;
                        };
                        eng.io_get(key).map(|io| io.info)
                    };
                    if let Some(i2) = info2 {
                        chain.fcb(op, i2, ChainData::None, ChainData::None);
                    }
                    let Some(eng) = chain.module_mut::<IoEngine>() else {
                        return;
                    };
                    // `IO_StopRd` fuirait les FD (note amont) → StopRdWr
                    eng.close_fd(key, IO_STOP_RDWR);
                    eng.io_free(key);
                    chain.free_link(info);
                }
                _ => warn!("Unused CCC IO 2F"),
            }
        }
        _ => {}
    }
}

/// `a_IO_ccc` en `ChainCb` prête à enregistrer.
#[must_use]
pub fn io_ccb() -> crate::chain::ChainCb {
    std::rc::Rc::new(|ch, op, branch, dir, info, d1, d2| {
        io_ccc(ch, op, branch, dir, info, d1, d2);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    fn engine() -> (Chain, IoKey) {
        let mut ch = Chain::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // garde le runtime vivant dans le module
        ch.set_module(IoEngine::new(rt.handle().clone()));
        ch.set_module(rt);
        (ch, 0)
    }

    #[test]
    fn branch1_start_sets_local_key() {
        let (mut ch, _) = engine();
        let info = ch.new_link();
        io_ccc(
            &mut ch,
            OP_START,
            1,
            BCK,
            info,
            ChainData::None,
            ChainData::None,
        );
        assert!(ch.local_key::<IoKey>(info).is_some());
    }

    #[test]
    fn branch2_send_stages_and_flows() {
        let (mut ch, _) = engine();
        // un collecteur FWD pour observer ce que 2F fcb émet
        let got = Rc::new(RefCell::new(Vec::new()));
        let g2 = got.clone();
        let info = ch.new_link();
        ch.link_mut(info).unwrap().fcb = Some(Rc::new(move |_ch, op, _b, _d, _i, d1, _d2| {
            if let ChainData::Buf(dbuf) = d1 {
                g2.borrow_mut().push(dbuf.buf.clone());
            }
            let _ = op;
        }));
        io_ccc(
            &mut ch,
            OP_START,
            2,
            BCK,
            info,
            ChainData::None,
            ChainData::None,
        );
        let key = *ch.local_key::<IoKey>(info).unwrap();
        // injecte un événement ReadData comme le ferait la tâche
        {
            let eng = ch.module_mut::<IoEngine>().unwrap();
            eng.io_get(key).unwrap().buf = b"hello".to_vec();
        }
        io_ccc(
            &mut ch,
            OP_SEND,
            2,
            FWD,
            info,
            ChainData::Int(key),
            ChainData::None,
        );
        assert_eq!(*got.borrow(), vec![b"hello".to_vec()]);
    }

    #[test]
    fn branch2_end_frees_link() {
        let (mut ch, _) = engine();
        let info = ch.new_link();
        io_ccc(
            &mut ch,
            OP_START,
            2,
            BCK,
            info,
            ChainData::None,
            ChainData::None,
        );
        let key = *ch.local_key::<IoKey>(info).unwrap();
        io_ccc(
            &mut ch,
            OP_END,
            2,
            FWD,
            info,
            ChainData::Int(key),
            ChainData::None,
        );
        assert!(ch.link(info).is_none());
    }

    #[test]
    fn abort_blocks_further_calls() {
        let (mut ch, _) = engine();
        let info = ch.new_link();
        // flag Ended manuellement → check() refuse
        ch.link_mut(info).unwrap().flags = CCC_ENDED | CCC_ABORTED;
        io_ccc(
            &mut ch,
            OP_START,
            1,
            BCK,
            info,
            ChainData::None,
            ChainData::None,
        );
        assert!(ch.local_key::<IoKey>(info).is_none());
    }

    /// Régression fidélité : lecteur + écrivain partagent le même `io->FD`
    /// (HTTP) ; `IO_close_fd(…, IO_StopWr)` ne doit PAS tuer la socket —
    /// seul `IO_StopRdWr` appelle `dClose` amont. Le lecteur doit donc
    /// encore recevoir des données après la fermeture de l'écrivain.
    #[test]
    fn shared_fd_survives_writer_close() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut ch = Chain::new();
        ch.set_module(IoEngine::new(rt.handle().clone()));

        // collecteur des DataBuf émis par le côté lecture (2F OpSend)
        let got = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
        let g2 = got.clone();

        let (server, mut client) = rt.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let accept =
                tokio::spawn(async move { listener.accept().await.map(|(s, _)| s).unwrap() });
            let client = TcpStream::connect(addr).await.unwrap();
            let server = accept.await.unwrap();
            (IoStream::Plain(server), client)
        });

        let sock_id = ch.module_mut::<IoEngine>().unwrap().insert_socket(server);

        // lecteur : lien branch 2, callback FWD = collecteur
        let rinfo = ch.new_link();
        ch.link_mut(rinfo).unwrap().fcb = Some(Rc::new(move |_c, _o, _b, _d, _i, d1, _d2| {
            if let ChainData::Buf(dbuf) = d1 {
                g2.borrow_mut().push(dbuf.buf);
            }
        }));
        io_ccc(
            &mut ch,
            OP_START,
            2,
            BCK,
            rinfo,
            ChainData::None,
            ChainData::None,
        );
        io_ccc(
            &mut ch,
            OP_SEND,
            2,
            BCK,
            rinfo,
            ChainData::Int(sock_id),
            "FD".into(),
        );

        // écrivain sur le MÊME fd (la socket est clonée comme `dup`)
        let winfo = ch.new_link();
        io_ccc(
            &mut ch,
            OP_START,
            1,
            BCK,
            winfo,
            ChainData::None,
            ChainData::None,
        );
        io_ccc(
            &mut ch,
            OP_SEND,
            1,
            BCK,
            winfo,
            ChainData::Int(sock_id),
            "FD".into(),
        );
        // OpEnd écrivain → IO_StopWr : watch retirée, fd RESTE ouvert
        io_ccc(
            &mut ch,
            OP_END,
            1,
            BCK,
            winfo,
            ChainData::None,
            ChainData::None,
        );

        rt.block_on(async {
            client.write_all(b"pong").await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        });
        poll_io(&mut ch);
        assert_eq!(*got.borrow(), vec![b"pong".to_vec()]);

        // OpEnd lecteur → IO_StopRdWr : là, `dClose` tue la socket
        let rkey = *ch.local_key::<IoKey>(rinfo).unwrap();
        io_ccc(
            &mut ch,
            OP_END,
            2,
            FWD,
            rinfo,
            ChainData::Int(rkey),
            ChainData::None,
        );
        assert!(ch.link(rinfo).is_none());
    }
}
