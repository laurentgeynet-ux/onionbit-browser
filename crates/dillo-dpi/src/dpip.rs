//! Port of `dpip/dpip.c` — **moitié serveur** du protocole dpip.
//!
//! (`build_cmd`/`get_attr`/`get_attr_l` vivent déjà dans
//! `dillo_net::dpip` — la moitié cliente. Ici : [`check_auth`] ≙
//! `a_Dpip_check_auth` et [`Dsh`] ≙ `Dsh` + `a_Dpip_dsh_*`, le handler
//! de socket bufferisé utilisé par dpid et les plugins.)
//!
//! Le `fd` nu amont devient le trait [`DpIo`] : [`DpIoTcp`]
//! (`TcpStream` accepté par le démon) et `DpIoSock` (`SockRef` sur
//! les descripteurs hérités 0/1 des plugins ≙ `dup2(sock,0/1)`).

// Lint verbatim : fidélité aux casts/tests C amont (voir AGENTS.md §5).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::similar_names)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::result_unit_err)]
#![allow(clippy::format_push_string)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::needless_continue)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::manual_strip)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::overly_complex_bool_expr)]
use std::io::{self, Read, Write};
use std::net::TcpStream;

use tracing::warn;

/// `DPIP_TAG_END` — fin de tag dpip (espace+quote+chevron).
const DPIP_TAG_END: &[u8] = b" '>";
/// `DPIP_MODE_SWITCH_TAG` — bascule TAG → RAW.
const DPIP_MODE_SWITCH_TAG: &[u8] = b"cmd='start_send_page' ";
/// `RBUF_SZ` — taille du buffer de lecture.
const RBUF_SZ: usize = 16 * 1024;

/// `DPIP_*` — flags du mode courant (verbatim `dpip.h`).
pub const DPIP_TAG: u8 = 1;
/// `DPIP_LAST_TAG`.
pub const DPIP_LAST_TAG: u8 = 2;
/// `DPIP_RAW`.
pub const DPIP_RAW: u8 = 4;
/// `DPIP_NONBLOCK`.
pub const DPIP_NONBLOCK: u8 = 8;

/// `DpipDshStatus` — verbatim (ordre des valeurs conservé : la
/// comparaison `status == EAGAIN` amont les distingue).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DshStatus {
    /// `DPIP_EAGAIN` (valeur amont 0).
    Eagain,
    /// `DPIP_ERROR`.
    Error,
    /// `DPIP_EOF`.
    Eof,
    /// Lecture/écriture réussie (amont laisse `status` inchangé).
    Ok,
}

/// `EAGAIN` *errno* amont — quirk verbatim : `a_Dpip_dsh_read_token2`
/// compare `dsh->status` à la constante errno `EAGAIN` (jamais à
/// `DPIP_EAGAIN`) — la boucle n'itère donc jamais. Valeurs POSIX/Win.
const EAGAIN_ERRNO: i32 = if cfg!(unix) { 11 } else { 10035 };

/// Abstraction socket duplex ≙ `fd` POSIX (`read`/`write`/`fcntl`).
/// Implémentée par [`DpIoTcp`] et `DpIoSock`.
pub trait DpIo {
    /// `fcntl(F_SETFL, O_NONBLOCK|…)` — bascule temporaire verbatim.
    fn set_nonblocking(&mut self, nb: bool);
    /// `read(fd, …)` — `WouldBlock` ≙ `EAGAIN`.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    /// `write(fd, …)`.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize>;
    /// Le descripteur est-il actuellement non-bloquant (≙ test
    /// `F_GETFL & O_NONBLOCK` de `a_Dpip_dsh_new`).
    fn is_nonblocking(&self) -> bool;
    /// `dClose(fd)` — fermeture de ce descripteur.
    fn close(&mut self);
}

/// [`DpIo`] sur une [`TcpStream`] possédée (socket acceptée par dpid).
pub struct DpIoTcp {
    s: TcpStream,
    nb: bool,
}

impl DpIoTcp {
    /// Wrap — `nb` = état non-bloquant courant du descripteur
    /// (verbatim `F_GETFL & O_NONBLOCK`).
    #[must_use]
    pub fn new(s: TcpStream, nb: bool) -> Self {
        Self { s, nb }
    }
}

impl DpIo for DpIoTcp {
    fn set_nonblocking(&mut self, nb: bool) {
        let _ = self.s.set_nonblocking(nb);
        self.nb = nb;
    }
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.s.read(buf)
    }
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.s.write(buf)
    }
    fn is_nonblocking(&self) -> bool {
        self.nb
    }
    fn close(&mut self) {
        let _ = self.s.shutdown(std::net::Shutdown::Both);
    }
}

/// [`DpIo`] sur un descripteur hérité (plugins : `dup2(sock,0/1)`).
/// Le fd est gardé brut ; `SockRef` est créé à la demande (les deux
/// handles partagent le socket comme en amont — sans ownership, donc
/// `close` est un no-op comme `dClose` ne doit pas tuer stdio d'un
/// processus).
#[cfg(unix)]
enum DpIoStd {
    /// fd 0 — `dup2`'d par le parent sur la socket dpi.
    In(std::io::Stdin),
    /// fd 1 — idem (même socket, sens écriture).
    Out(std::io::Stdout),
}

#[cfg(unix)]
pub struct DpIoSock {
    io: DpIoStd,
    nb: bool,
}

#[cfg(unix)]
impl DpIoSock {
    /// Wrap du fd `n` (typiquement 0/1 dans un plugin — ≙
    /// `a_Dpip_dsh_new(STDIN_FILENO, STDOUT_FILENO, …)` amont).
    /// Les plugins dpi sont POSIX (dup2 sur des sockets) ; pas de
    /// fallback Windows (verbatim — amont n'en a pas non plus).
    /// `None` hors descripteurs stdio — seul chemin `AsFd` safe
    /// sans `unsafe` (`forbid(unsafe_code)`).
    #[must_use]
    pub fn stdio(n: std::os::unix::io::RawFd, nb: bool) -> Option<Self> {
        let io = match n {
            0 => DpIoStd::In(std::io::stdin()),
            1 => DpIoStd::Out(std::io::stdout()),
            _ => return None,
        };
        Some(Self { io, nb })
    }

    fn as_fd(&self) -> std::os::unix::io::BorrowedFd<'_> {
        use std::os::unix::io::AsFd;
        match &self.io {
            DpIoStd::In(s) => s.as_fd(),
            DpIoStd::Out(s) => s.as_fd(),
        }
    }
}

#[cfg(unix)]
impl DpIo for DpIoSock {
    fn set_nonblocking(&mut self, nb: bool) {
        let fd = self.as_fd();
        let _ = socket2::SockRef::from(&fd).set_nonblocking(nb);
        self.nb = nb;
    }
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let fd = self.as_fd();
        let s = socket2::SockRef::from(&fd);
        let mut r: &socket2::Socket = &s;
        r.read(buf)
    }
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let fd = self.as_fd();
        let s = socket2::SockRef::from(&fd);
        let mut w: &socket2::Socket = &s;
        w.write(buf)
    }
    fn is_nonblocking(&self) -> bool {
        self.nb
    }
    fn close(&mut self) {
        // verbatim plugins : le fd reste ouvert (il appartient à
        // stdio / au parent) — `a_Dpip_dsh_close` fait dClose mais
        // le processus meurt juste après. No-op ici.
    }
}

/// `Dsh` — handler de socket dpip bufferisé.
///
/// `fd_in`/`fd_out` amont peuvent être le même descripteur ; le port
/// accepte deux impls [`DpIo`] (ou la même via `try_clone` du
/// `TcpStream` par [`Dsh::tcp`]).
pub struct Dsh<'a> {
    fd_in: Box<dyn DpIo + 'a>,
    fd_out: Box<dyn DpIo + 'a>,
    /// `wrbuf`.
    wrbuf: Vec<u8>,
    /// `rdbuf`.
    rdbuf: Vec<u8>,
    /// `flush_sz` (amont le stocke sans l'utiliser — verbatim).
    #[allow(dead_code)]
    flush_sz: usize,
    /// `mode` — `DPIP_*`.
    mode: u8,
    /// `status`.
    status: DshStatus,
}

impl<'a> Dsh<'a> {
    /// `a_Dpip_dsh_new` — crée le handler sur deux [`DpIo`].
    #[must_use]
    pub fn new(fd_in: Box<dyn DpIo + 'a>, fd_out: Box<dyn DpIo + 'a>, flush_sz: usize) -> Self {
        let mut mode = DPIP_TAG;
        if fd_in.is_nonblocking() {
            mode |= DPIP_NONBLOCK;
        }
        Self {
            fd_in,
            fd_out,
            wrbuf: Vec::with_capacity(8 * 1024),
            rdbuf: Vec::with_capacity(8 * 1024),
            flush_sz,
            mode,
            status: DshStatus::Eagain, // 0 amont ≙ DPIP_EAGAIN
        }
    }

    /// `a_Dpip_dsh_new(fd, fd, …)` — les deux extrémités d'un même
    /// socket TCP (≙ `dup` implicite : `try_clone`).
    #[must_use]
    pub fn tcp(s: TcpStream) -> Option<Self> {
        let nb = {
            // état nb non lisible sur TcpStream — amont lit F_GETFL ;
            // nos sockets acceptées sont bloquantes par défaut.
            false
        };
        let out = s.try_clone().ok()?;
        Some(Self::new(
            Box::new(DpIoTcp::new(s, nb)),
            Box::new(DpIoTcp::new(out, nb)),
            1024,
        ))
    }

    /// `Dpip_dsh_write` interne : écrit `data` (partiellement en nb).
    /// Retour verbatim : `1..=len` envoyés, `-1` eagain, `-3` erreur.
    fn raw_write(&mut self, nb: bool, data: &[u8]) -> i32 {
        let req_mode = if nb { DPIP_NONBLOCK } else { 0 };
        let toggled = (self.mode & DPIP_NONBLOCK) != req_mode;
        if toggled {
            self.fd_out.set_nonblocking(nb);
        }

        let mut sent = 0usize;
        let mut ret = -3;
        loop {
            match self.fd_out.write(&data[sent..]) {
                Ok(0) => break, // write de 0 : impossible mais on sort
                Ok(n) => {
                    sent += n;
                    if nb || sent == data.len() {
                        ret = i32::try_from(sent).unwrap_or(i32::MAX);
                        break;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.status = DshStatus::Eagain;
                    ret = -1;
                    break;
                }
                Err(e) => {
                    warn!("[Dpip_dsh_write] {}", e);
                    self.status = DshStatus::Error;
                    break;
                }
            }
        }

        if toggled {
            self.fd_out.set_nonblocking(self.mode & DPIP_NONBLOCK != 0);
        }
        ret
    }

    /// `Dpip_dsh_read` interne : `blocking` = lecture bloquante
    /// (un seul `read` si donnée), sinon drain non-bloquant.
    fn raw_read(&mut self, blocking: bool) {
        if self.status == DshStatus::Error || self.status == DshStatus::Eof {
            return;
        }
        let nb = !blocking;
        let req_mode = if nb { DPIP_NONBLOCK } else { 0 };
        let toggled = (self.mode & DPIP_NONBLOCK) != req_mode;
        if toggled {
            self.fd_in.set_nonblocking(nb);
        }

        let mut buf = vec![0u8; RBUF_SZ];
        loop {
            match self.fd_in.read(&mut buf) {
                Ok(0) => {
                    self.status = DshStatus::Eof;
                    break;
                }
                Ok(n) => {
                    self.rdbuf.extend_from_slice(&buf[..n]);
                    if blocking {
                        break;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.status = DshStatus::Eagain;
                    break;
                }
                Err(e) => {
                    warn!("[Dpip_dsh_read] {}", e);
                    self.status = DshStatus::Error;
                    break;
                }
            }
        }

        if toggled {
            self.fd_in.set_nonblocking(self.mode & DPIP_NONBLOCK != 0);
        }
        // verbatim : après une lecture bloquante, drainer sans bloquer.
        if blocking {
            self.raw_read(false);
        }
    }

    /// `a_Dpip_dsh_write` — ajoute au buffer, flush si demandé.
    /// `0` ok, `1` erreur.
    pub fn write(&mut self, flush: bool, data: &[u8]) -> i32 {
        self.wrbuf.extend_from_slice(data);
        if !flush || self.wrbuf.is_empty() {
            return 0;
        }
        let buf = std::mem::take(&mut self.wrbuf);
        let ret = self.raw_write(false, &buf);
        if usize::try_from(ret).unwrap_or(0) == buf.len() {
            return 0;
        }
        // verbatim : non écrit en entier → garder dans wrbuf
        self.wrbuf = buf;
        1
    }

    /// `a_Dpip_dsh_write_str`.
    pub fn write_str(&mut self, flush: bool, s: &str) -> i32 {
        self.write(flush, s.as_bytes())
    }

    /// `a_Dpip_dsh_tryflush` — flush non-bloquant :
    /// `0` ok/vide, `1..` envoyé, `-1` eagain, `-3` erreur.
    pub fn try_flush(&mut self) -> i32 {
        if self.wrbuf.is_empty() {
            return 0;
        }
        let buf = std::mem::take(&mut self.wrbuf);
        let st = self.raw_write(true, &buf);
        if st > 0 {
            let n = usize::try_from(st).unwrap_or(0);
            self.wrbuf = buf[n..].to_vec();
        } else {
            self.wrbuf = buf;
        }
        if self.wrbuf.is_empty() {
            0
        } else {
            st
        }
    }

    /// `a_Dpip_dsh_trywrite` — écriture non-bloquante directe ;
    /// le reste non envoyé va en `wrbuf`.
    pub fn try_write(&mut self, data: &[u8]) -> i32 {
        let st = self.raw_write(true, data);
        if st > 0 {
            let n = usize::try_from(st).unwrap_or(0);
            if n < data.len() {
                self.wrbuf.extend_from_slice(&data[n..]);
            }
        }
        st
    }

    /// `a_Dpip_dsh_read_token2` — retourne le prochain token :
    /// tag complet (jusqu'à ` '>`) en mode `TAG`, tout le buffer en
    /// mode `RAW`. `blocking` = attente de données.
    #[must_use]
    pub fn read_token(&mut self, blocking: bool) -> Option<Vec<u8>> {
        self.raw_read(false);

        if self.mode & DPIP_LAST_TAG != 0 {
            self.mode = DPIP_RAW;
        }

        if blocking {
            if self.mode & DPIP_TAG != 0 {
                // attendre tant que le tag est incomplet
                if !self.rdbuf.windows(3).any(|w| w == DPIP_TAG_END) {
                    loop {
                        self.raw_read(true);
                        if self.rdbuf.windows(3).any(|w| w == DPIP_TAG_END) {
                            break;
                        }
                        // verbatim : `status == EAGAIN` compare à l'errno
                        // EAGAIN (≠ DPIP_EAGAIN) — presque jamais vrai ;
                        // on reproduit en comparant le code errno brut.
                        if status_code(self.status) != EAGAIN_ERRNO {
                            break;
                        }
                    }
                }
            } else if self.mode & DPIP_RAW != 0 {
                while self.rdbuf.is_empty()
                    && self.status != DshStatus::Error
                    && self.status != DshStatus::Eof
                {
                    self.raw_read(true);
                }
            }
        }

        if self.mode & DPIP_TAG != 0 {
            if let Some(p) = self.rdbuf.windows(3).position(|w| w == DPIP_TAG_END) {
                let end = p + 3;
                let ret: Vec<u8> = self.rdbuf[..end].to_vec();
                self.rdbuf.drain(..end);
                if ret
                    .windows(DPIP_MODE_SWITCH_TAG.len())
                    .any(|w| w == DPIP_MODE_SWITCH_TAG)
                {
                    self.mode |= DPIP_LAST_TAG;
                }
                return Some(ret);
            }
        } else if !self.rdbuf.is_empty() {
            return Some(std::mem::take(&mut self.rdbuf));
        }
        None
    }

    /// `a_Dpip_dsh_close` — flush puis fermeture des deux extrémités.
    pub fn close(&mut self) {
        let _ = self.write(true, b"");
        self.fd_in.close();
        // upstream : `if (fd_out != fd_in)` — nôtres sont des clones,
        // fermer les deux ≙ fermer le fd une fois.
        self.fd_out.close();
    }

    /// Accesseur verbatim `dsh->status`.
    #[must_use]
    pub fn status(&self) -> DshStatus {
        self.status
    }

    /// Accesseur verbatim `dsh->mode`.
    #[must_use]
    pub fn mode(&self) -> u8 {
        self.mode
    }
}

/// Code numérique de [`DshStatus`] (les valeurs amont sont 0,1,2 —
/// `Ok` n'existe pas amont, on le mappe hors plage pour que la
/// comparaison `== EAGAIN` verbatim reste fausse).
fn status_code(s: DshStatus) -> i32 {
    match s {
        DshStatus::Eagain => 0,
        DshStatus::Error => 1,
        DshStatus::Eof => 2,
        DshStatus::Ok => -99,
    }
}

/// `a_Dpip_check_auth` — vérifie qu'un tag `auth` porte la clé
/// partagée de `~/.dillo/dpid_comm_keys` (verbatim : même parsing
/// `strtol` + queue hexadécimale que `Dpi_read_comm_keys`).
#[must_use]
pub fn check_auth(auth_tag: &str) -> bool {
    let Some(cmd) = dillo_net::dpip::get_attr(auth_tag, "cmd") else {
        return false;
    };
    if cmd != "auth" {
        return false;
    }
    let Some(msg) = dillo_net::dpip::get_attr(auth_tag, "msg") else {
        return false;
    };
    let Some((_port, key)) = crate::dpid_common::read_comm_keys_pub() else {
        warn!("[a_Dpip_check_auth] can't read dpid_comm_keys");
        return false;
    };
    msg.as_bytes() == key.as_slice()
}

/// `a_Dpip_dsh_printf` macro ≙ `format!` + [`Dsh::write`].
pub fn dsh_printf(dsh: &mut Dsh, flush: bool, s: String) -> i32 {
    dsh.write(flush, s.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// `DpIo` de test — file d'octets scriptée en lecture.
    struct MockIo {
        rd: VecDeque<u8>,
        rd_err: Option<io::ErrorKind>,
        wr: Vec<u8>,
        nb: bool,
    }

    impl DpIo for MockIo {
        fn set_nonblocking(&mut self, nb: bool) {
            self.nb = nb;
        }
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.rd.is_empty() {
                let k = self.rd_err.take().unwrap_or(io::ErrorKind::WouldBlock);
                return Err(io::Error::from(k));
            }
            let n = self.rd.len().min(buf.len());
            for b in &mut buf[..n] {
                *b = self.rd.pop_front().unwrap_or(0);
            }
            Ok(n)
        }
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.wr.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn is_nonblocking(&self) -> bool {
            self.nb
        }
        fn close(&mut self) {}
    }

    fn dsh_with(input: &[u8]) -> Dsh<'static> {
        Dsh::new(
            Box::new(MockIo {
                rd: input.iter().copied().collect(),
                rd_err: None,
                wr: vec![],
                nb: false,
            }),
            Box::new(MockIo {
                rd: VecDeque::new(),
                rd_err: None,
                wr: vec![],
                nb: false,
            }),
            1024,
        )
    }

    #[test]
    fn token_tag_then_raw() {
        // verbatim : `<cmd='start_send_page' '>` bascule en RAW.
        let mut d = dsh_with(b"<cmd='start_send_page' '>RAWDATA");
        let t = d.read_token(false);
        assert_eq!(t.as_deref(), Some(b"<cmd='start_send_page' '>" as &[u8]));
        assert_eq!(d.mode() & DPIP_LAST_TAG, DPIP_LAST_TAG);
        let t2 = d.read_token(false);
        assert_eq!(t2.as_deref(), Some(b"RAWDATA" as &[u8]));
        assert_eq!(d.mode(), DPIP_RAW);
    }

    #[test]
    fn token_tag_partial() {
        // tag incomplet en non-bloquant → None
        let mut d = dsh_with(b"<cmd='x'");
        assert_eq!(d.read_token(false), None);
    }

    #[test]
    fn write_flush() {
        let mut d = dsh_with(b"");
        assert_eq!(d.write(false, b"abc"), 0); // pas flush → bufferisé
        assert_eq!(d.try_flush(), 0); // envoyé, buffer vidé
        assert_eq!(d.write(true, b"x"), 0);
    }

    #[test]
    fn check_auth_bad_tag() {
        assert!(!check_auth("<cmd='auth' msg='zzz' '>"));
        assert!(!check_auth("<cmd='x' '>"));
    }
}
