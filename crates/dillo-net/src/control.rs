//! Port de `src/IO/control.c` + `control.h` — socket de contrôle
//! distant (« dilloc »).
//!
//! Amont : un `AF_UNIX` `~/.dillo/ctl/<pid>` non-bloquant piloté par
//! `a_IOwatch_*` (boucle FLTK). Ici la **machine d'état + le
//! protocole** sont portés verbatim sur des seams :
//!
//! - [`CtlIo`] ≙ `read`/`write`/`close` non-bloquants d'un fd
//!   (`EAGAIN` → « réessayer plus tard », EOF → 0).
//! - [`CtlOps`] ≙ les globales `a_UIcmd_*`/`a_Capi_*`/`a_Cache_*`/
//!   `getpid` — injectées par `dillo-browser` (le registre de
//!   fenêtres vit plus haut, deps descendantes).
//!
//! Le listener (`socket`/`bind`/`listen`/`accept`) est un détail de
//! transport : sous Unix il vit dans le binaire `dillo`
//! (`std::os::unix::net::UnixListener`) ; hors Unix le module reste
//! un no-op — verbatim `#else` de `ENABLE_CONTROL_SOCKET` amont.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_lines)]
#![forbid(unsafe_code)]

use dillo_common::dlib;

/// `enum control_req_stage` verbatim.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ReqStage {
    /// `REQ_READ_COMMAND`.
    ReadCommand,
    /// `REQ_READ_BODY`.
    ReadBody,
    /// `REQ_PREPARE_REPLY`.
    PrepareReply,
    /// `REQ_WRITE_REPLY` (`REQ_SWITCH_WRITE` est immédiat :
    /// l'appelant rebascule sur l'événement writable).
    WriteReply,
    /// `REQ_CLOSE`.
    Close,
}

/// `struct control_req` verbatim.
pub struct ControlReq {
    stage: ReqStage,
    cmd: Vec<u8>,
    body: Vec<u8>,
    reply: Vec<u8>,
    reply_offset: usize,
}

impl ControlReq {
    /// `Control_req_new` verbatim (Dstr vides ≙ `sized_new`).
    fn new() -> Self {
        Self {
            stage: ReqStage::ReadCommand,
            cmd: Vec::new(),
            body: Vec::new(),
            reply: Vec::new(),
            reply_offset: 0,
        }
    }
}

/// Verdict de lecture d'un octet ≙ `read(fd, &b, 1)` non-bloquant.
pub enum CtlByte {
    /// `r == 1`.
    Byte(u8),
    /// `r == 0` — fin de flux.
    Eof,
    /// `errno == EAGAIN/EWOULDBLOCK` — rien de disponible.
    Again,
    /// Autre erreur (`errno != EINTR` — l'EINTR est absorbé par le
    /// transport).
    Error,
}

/// Seam transport ≙ `read`/`write`/`close` non-bloquants amont
/// (les boucles `EINTR` sont absorbées par l'impl).
pub trait CtlIo {
    /// `read` octet par octet (verbatim `Control_req_read_cmd`).
    fn read_byte(&mut self) -> CtlByte;
    /// `Control_read_fd` : draine dans `out` ; retour verbatim
    /// `+1` = encore des données (EAGAIN), `0` = EOF, `-1` = erreur.
    fn read_to_end(&mut self, out: &mut Vec<u8>) -> i32;
    /// `Control_write_fd` : écrit depuis `*offset` ; met à jour
    /// l'offset ; `+1` = EAGAIN (reprendre plus tard), `0` = fini,
    /// `-1` = erreur.
    fn write_from(&mut self, input: &[u8], offset: &mut usize) -> i32;
}

/// Seam commandes ≙ `a_UIcmd_*`/`a_Capi_*`/`a_Cache_*`/`getpid`
/// appelées par `Control_req_prepare_reply`. `bw` = id fenêtre.
pub trait CtlOps {
    /// `a_UIcmd_get_first_active_bw` (`NULL` ≙ `None`).
    fn first_active_bw(&mut self) -> Option<u32>;
    /// `a_UIcmd_reload_all_active`.
    fn reload_all(&mut self);
    /// `a_UIcmd_has_finished`.
    fn has_finished(&mut self, bw: u32) -> bool;
    /// `a_UIcmd_open_urlstr`.
    fn open_urlstr(&mut self, bw: u32, s: &str);
    /// `a_UIcmd_get_location_text`.
    fn location_text(&mut self, bw: u32) -> String;
    /// `a_UIcmd_get_page_title` (`NULL` ≙ `None`).
    fn page_title(&mut self, bw: u32) -> Option<String>;
    /// `a_UIcmd_set_msg`.
    fn set_msg(&mut self, bw: u32, msg: &str);
    /// `a_Capi_get_buf` sur l'URL de l'onglet (`None` ≙ échec).
    fn get_buf(&mut self, url_str: &str) -> Option<Vec<u8>>;
    /// `a_Cache_get_header` (`NULL` ≙ `None`).
    fn get_header(&mut self, url_str: &str) -> Option<Vec<u8>>;
    /// `a_Cache_entry_inject` (flags `CA_*` verbatim).
    fn entry_inject(&mut self, url_str: &str, body: &[u8], flags: u32) -> bool;
    /// `a_UIcmd_repush`.
    fn repush(&mut self, bw: u32);
    /// `a_UIcmd_close_all_bw((void*)1)` — `force` ≙ non-NULL.
    fn close_all(&mut self);
    /// `a_UIcmd_by_name` — `0` = succès.
    fn by_name(&mut self, bw: u32, name: &str) -> i32;
    /// `getpid`.
    fn pid(&mut self) -> i64;
}

/// Serveur de requêtes — ≙ les globaux `bw_waiting`/
/// `bw_waiting_req` + la table fd→req de `a_IOwatch`.
#[derive(Default)]
pub struct Control {
    /// Requêtes actives : `fd-slot` → (req, transport). Le slot est
    /// l'id opaque rendu par [`Control::accept`].
    reqs: Vec<Option<(ControlReq, Box<dyn CtlIo>)>>,
    /// `static BrowserWindow *bw_waiting` + `bw_waiting_req`.
    waiting: Option<(u32, usize)>,
    /// Délai `wait` demandé par `prepare_reply` ≙
    /// `a_Timeout_add(timeout, handle_wait_timeout)` — l'appelant
    /// arme le timer réel via [`Control::take_wait_timeout`].
    pending_wait_timeout: Option<f64>,
}

impl Control {
    /// `a_Control_init` côté requêtes (le listener est câblé par
    /// l'appelant).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `Control_listen_cb` — enregistre une connexion acceptée
    /// (≙ `accept`+`O_NONBLOCK`+`a_IOwatch_add_fd(DIO_READ)`).
    /// Retourne l'id de la requête.
    pub fn accept(&mut self, io: Box<dyn CtlIo>) -> usize {
        // Slot libre sinon append (≙ fd neuf).
        for (i, slot) in self.reqs.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some((ControlReq::new(), io));
                return i;
            }
        }
        self.reqs.push(Some((ControlReq::new(), io)));
        self.reqs.len() - 1
    }

    /// `a_Control_is_waiting` verbatim.
    #[must_use]
    pub fn is_waiting(&self) -> bool {
        self.waiting.is_some()
    }

    /// `Control_req_read_cb` — passe de la machine sur événement
    /// **readable** (les `if` en cascade sont verbatim — les stades
    /// tombent en chaîne dans la même passe).
    ///
    /// Retourne `true` tant que la requête existe ; `false` ≙
    /// `REQ_CLOSE` (fd fermé, req libérée).
    pub fn on_readable(&mut self, id: usize, ops: &mut dyn CtlOps) -> bool {
        if self.stage_of(id) == Some(ReqStage::ReadCommand) {
            if let Some(Some((req, io))) = self.reqs.get_mut(id) {
                read_cmd(req, io.as_mut());
            }
        }
        if self.stage_of(id) == Some(ReqStage::ReadBody) {
            let mut next = None;
            if let Some(Some((req, io))) = self.reqs.get_mut(id) {
                match io.read_to_end(&mut req.body) {
                    0 => next = Some(ReqStage::PrepareReply),
                    r if r < 0 => next = Some(ReqStage::Close),
                    _ => {}
                }
            }
            if let Some(s) = next {
                self.set_stage(id, s);
            }
        }
        if self.stage_of(id) == Some(ReqStage::PrepareReply) {
            self.prepare_reply(id, ops);
        }
        if self.stage_of(id) == Some(ReqStage::Close) {
            self.close(id);
            return false;
        }
        // REQ_SWITCH_WRITE ≙ l'appelant bascule le watch en WRITE —
        // ici `prepare_reply`/`notify`/`timeout` mettent le stade
        // `WriteReply` directement : `on_writable` fait le reste.
        true
    }

    /// `REQ_WRITE_REPLY` — passe sur événement **writable**
    /// (verbatim `Control_req_read_cb` après `a_IOwatch_add_fd
    /// (DIO_WRITE)`).
    pub fn on_writable(&mut self, id: usize) -> bool {
        if self.stage_of(id) == Some(ReqStage::WriteReply) {
            let mut done = false;
            if let Some(Some((req, io))) = self.reqs.get_mut(id) {
                done = io.write_from(&req.reply, &mut req.reply_offset) <= 0;
            }
            if done {
                self.set_stage(id, ReqStage::Close);
            }
        }
        if self.stage_of(id) == Some(ReqStage::Close) {
            self.close(id);
            return false;
        }
        true
    }

    fn stage_of(&self, id: usize) -> Option<ReqStage> {
        self.reqs.get(id)?.as_ref().map(|(r, _)| r.stage)
    }

    fn set_stage(&mut self, id: usize, stage: ReqStage) {
        if let Some(Some((req, _))) = self.reqs.get_mut(id) {
            req.stage = stage;
        }
    }

    /// Requête en écriture (≙ stage `REQ_WRITE_REPLY`) — le shell
    /// rebascule le watch R/W dessus.
    #[must_use]
    pub fn wants_write(&self, id: usize) -> bool {
        matches!(
            self.reqs.get(id).and_then(|s| s.as_ref()),
            Some((req, _)) if req.stage == ReqStage::WriteReply
        )
    }

    /// `a_Control_notify_finish` — le `bw` a fini de charger :
    /// répond `0\n` à la requête `wait` en cours (verbatim).
    /// Retourne l'id de la requête à repasser en write, ou `None`.
    pub fn notify_finish(&mut self, bw: u32) -> Option<usize> {
        let (wait_bw, req_id) = self.waiting?;
        if wait_bw != bw {
            return None;
        }
        if let Some(Some((req, _))) = self.reqs.get_mut(req_id) {
            req.reply.extend_from_slice(b"0\n");
            req.stage = ReqStage::WriteReply;
        }
        self.waiting = None;
        Some(req_id)
    }

    /// `handle_wait_timeout` — délai `wait` écoulé : répond
    /// `1\nCurrent url not cached` (**verbatim**, message étrange
    /// conservé) et repasse en write.
    pub fn on_wait_timeout(&mut self) -> Option<usize> {
        let (_, req_id) = self.waiting?;
        if let Some(Some((req, _))) = self.reqs.get_mut(req_id) {
            req.reply.extend_from_slice(b"1\nCurrent url not cached");
            req.stage = ReqStage::WriteReply;
        }
        self.waiting = None;
        Some(req_id)
    }

    /// `REQ_CLOSE` — libère la requête (≙ `dClose` + `req_free`).
    fn close(&mut self, id: usize) {
        if let Some(slot) = self.reqs.get_mut(id) {
            *slot = None;
        }
        if self.waiting.is_some_and(|(_, r)| r == id) {
            self.waiting = None;
        }
    }

    /// `Control_req_prepare_reply` verbatim — dispatch des
    /// commandes + codes de retour (`N\n` en tête).
    fn prepare_reply(&mut self, id: usize, ops: &mut dyn CtlOps) {
        let Some((req, _)) = self.reqs.get_mut(id).and_then(|s| s.as_mut()) else {
            return;
        };
        let mut done = true;
        let cmd = String::from_utf8_lossy(&req.cmd).into_owned();
        let bw = ops.first_active_bw();
        let r: &mut Vec<u8> = &mut req.reply;

        let out = |r: &mut Vec<u8>, s: &str| r.extend_from_slice(s.as_bytes());

        if cmd == "help" {
            out(r, "0\n");
            out(r, "Commands return 0 on success or non-zero on error.\n");
            out(r, "Available commands:\n");
            out(r, " ping          Check if dillo replies correctly:\n");
            out(r, " pid           Print PID of selected dillo process\n");
            out(r, " reload        Reload the current tab\n");
            out(
                r,
                " ready         Exits with 0 if finished loading, 1 otherwise\n",
            );
            out(r, " open URL      Open the given URL in the current tab\n");
            out(r, " url           Print the url in the current tab\n");
            out(
                r,
                " title         Print the title of page in the current tab\n",
            );
            out(r, " status [MSG]  Set the status bar to MSG\n");
            out(r, " dump          Print the content of the current tab\n");
            out(
                r,
                " hdump         Print the HTTP headers of the current tab\n",
            );
            out(
                r,
                " load          Replace the content in the current tab by stdin\n",
            );
            out(
                r,
                " rawload       Replace the HTTP headers and content of the current\n",
            );
            out(r, "               tab by stdin\n");
            out(r, " quit          Close dillo\n");
            out(
                r,
                " wait [T]      Wait until the current tab has finished loading\n",
            );
            out(
                r,
                "               at most T seconds (default 60.0). Wait forever with\n",
            );
            out(r, "               T set to 0.\n");
        } else if cmd == "ping" {
            out(r, "0\npong\n");
        } else if cmd == "pid" {
            out(r, &format!("0\n{}\n", ops.pid()));
        } else if cmd == "reload" {
            ops.reload_all();
            out(r, "0\n");
        } else if cmd == "ready" {
            let finished = bw.is_some_and(|b| ops.has_finished(b));
            out(r, if finished { "0\n" } else { "1\n" });
        } else if let Some(u) = cmd.strip_prefix("open ") {
            if let Some(b) = bw {
                ops.open_urlstr(b, u);
            }
            out(r, "0\n");
        } else if cmd == "url" {
            let t = bw.map_or_else(String::new, |b| ops.location_text(b));
            out(r, &format!("0\n{t}\n"));
        } else if cmd == "title" {
            match bw.and_then(|b| ops.page_title(b)) {
                Some(t) => out(r, &format!("0\n{t}\n")),
                None => out(r, "1\n"),
            }
        } else if cmd == "status" || cmd.starts_with("status ") {
            if let Some(b) = bw {
                if cmd.as_bytes().get(6) == Some(&b' ') {
                    ops.set_msg(b, &cmd[7..]);
                } else {
                    ops.set_msg(b, "");
                }
            }
            out(r, "0\n");
        } else if cmd == "dump" {
            let url = bw.map_or_else(String::new, |b| ops.location_text(b));
            out(r, "0\n");
            if let Some(buf) = ops.get_buf(&url) {
                r.extend_from_slice(&buf);
            }
        } else if cmd == "hdump" {
            let url = bw.map_or_else(String::new, |b| ops.location_text(b));
            match ops.get_header(&url) {
                None => out(r, "1\nCurrent url not cached"),
                Some(h) => {
                    out(r, "0\n");
                    r.extend_from_slice(&h);
                }
            }
        } else if cmd == "load" {
            let url = bw.map_or_else(String::new, |b| ops.location_text(b));
            // `load` ≙ en-têtes synthétiques (`CA_GotHeader |
            // CA_GotLength` amont — constantes de `cache`, pas de
            // valeurs locales).
            if ops.entry_inject(
                &url,
                &req.body,
                crate::cache::ca_flags::GOT_HEADER | crate::cache::ca_flags::GOT_LENGTH,
            ) {
                if let Some(b) = bw {
                    ops.repush(b);
                }
                out(r, "0\n");
            } else {
                out(r, "1\ncannot get current url");
            }
        } else if cmd == "rawload" {
            let url = bw.map_or_else(String::new, |b| ops.location_text(b));
            if ops.entry_inject(&url, &req.body, 0) {
                if let Some(b) = bw {
                    ops.repush(b);
                }
                out(r, "0\n");
            } else {
                out(r, "1\ncannot get current url");
            }
        } else if cmd == "quit" {
            ops.close_all();
            out(r, "0\n");
        } else if let Some(name) = cmd.strip_prefix("cmd ") {
            let ret = bw.map_or(1, |b| ops.by_name(b, name));
            out(r, if ret == 0 { "0\n" } else { "1\n" });
        } else if cmd == "wait" || cmd.starts_with("wait ") {
            // `atof(&cmd[5])` verbatim — préfixe numérique (`5abc`→5),
            // 60.0 par défaut, ≤0 ou sans préfixe = infini.
            let timeout: f64 = if cmd.as_bytes().get(4) == Some(&b' ') {
                dlib::strtod(&cmd.as_bytes()[5..]).0
            } else {
                60.0
            };
            if bw.is_some_and(|b| ops.has_finished(b)) {
                out(r, "0\n");
            } else if self.waiting.is_some() {
                out(r, "1\nalready waiting\n");
            } else {
                // verbatim : reply différée, stade non avancé.
                if let Some(b) = bw {
                    self.waiting = Some((b, id));
                }
                done = false;
                // Le timeout (>0) est armé par l'appelant via
                // `wait_timeout_pending` — ≙ `a_Timeout_add`.
                self.pending_wait_timeout = if timeout > 0.0 { Some(timeout) } else { None };
            }
        } else {
            out(r, &format!("1\nunknown command: {cmd}\n"));
        }

        if done {
            let Some((req, _)) = self.reqs.get_mut(id).and_then(|s| s.as_mut()) else {
                return;
            };
            req.stage = ReqStage::WriteReply;
        }
    }

    /// Délai `wait` à armer après `on_readable` (≙
    /// `a_Timeout_add(timeout, handle_wait_timeout)` — `None` =
    /// attente infinie ou pas de wait).
    #[must_use]
    pub fn take_wait_timeout(&mut self) -> Option<f64> {
        self.pending_wait_timeout.take()
    }

    /// Passe de boucle ≙ les callbacks `a_IOwatch` amont : chaque
    /// requête vivante est offerte à [`Control::on_readable`] (les
    /// transports non-bloquants rendent `EAGAIN`/`Again` inoffensif),
    /// puis à [`Control::on_writable`] si `WriteReply`. Appelée à
    /// chaque tick du shell (pas de watch fd natif — cf. docs/PORTING).
    pub fn poll(&mut self, ops: &mut dyn CtlOps) {
        let ids: Vec<usize> = self
            .reqs
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_some())
            .map(|(i, _)| i)
            .collect();
        for id in ids {
            if self.on_readable(id, ops) && self.wants_write(id) {
                self.on_writable(id);
            }
        }
    }
}

/// `Control_req_read_cmd` verbatim — octet par octet jusqu'à `\n` ;
/// ligne vide/erreur → `REQ_CLOSE` ; `load`/`rawload` →
/// `REQ_READ_BODY`, sinon `REQ_PREPARE_REPLY`.
fn read_cmd(req: &mut ControlReq, io: &mut dyn CtlIo) {
    loop {
        match io.read_byte() {
            CtlByte::Byte(b) => {
                req.cmd.push(b);
                if b == b'\n' {
                    break;
                }
            }
            CtlByte::Eof => break,
            CtlByte::Again => return,
            CtlByte::Error => {
                req.stage = ReqStage::Close;
                return;
            }
        }
    }

    if req.cmd.is_empty() {
        // verbatim : connexion réinitialisée / vide → close.
        req.stage = ReqStage::Close;
        return;
    }

    // strip `\n`/`\r` (verbatim — premier des deux rencontré).
    if let Some(p) = req.cmd.iter().position(|&b| b == b'\n' || b == b'\r') {
        req.cmd.truncate(p);
    }

    let load = req.cmd == b"load" || req.cmd == b"rawload";
    req.stage = if load {
        ReqStage::ReadBody
    } else {
        ReqStage::PrepareReply
    };
}

/// Transport `AF_UNIX` ≙ la partie `socket`/`fcntl`/`accept` de
/// `a_Control_init`/`Control_listen_cb` — présent seulement sous
/// Unix (verbatim `#else` de `ENABLE_CONTROL_SOCKET`).
#[cfg(unix)]
pub mod unix {
    use std::fs;
    use std::io::{self, Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};

    use super::{CtlByte, CtlIo};

    /// [`CtlIo`] sur un `UnixStream` non-bloquant (≙ `O_NONBLOCK` +
    /// `FIONBIO` explicites de `Control_listen_cb` — les boucles
    /// `EINTR` sont absorbées par l'impl).
    pub struct UnixCtlIo {
        s: UnixStream,
    }

    impl UnixCtlIo {
        /// Enveloppe un stream accepté.
        #[must_use]
        pub fn new(s: UnixStream) -> Self {
            Self { s }
        }
    }

    impl CtlIo for UnixCtlIo {
        /// `read(fd, &b, 1)` verbatim — `EAGAIN` → [`CtlByte::Again`].
        fn read_byte(&mut self) -> CtlByte {
            let mut b = [0u8; 1];
            loop {
                match self.s.read(&mut b) {
                    Ok(1) => return CtlByte::Byte(b[0]),
                    Ok(_) => return CtlByte::Eof,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        return CtlByte::Again;
                    }
                    Err(_) => return CtlByte::Error,
                }
            }
        }

        /// `Control_read_fd` verbatim — draine jusqu'à `EAGAIN`
        /// (`+1`), EOF (`0`) ou erreur (`-1`).
        fn read_to_end(&mut self, out: &mut Vec<u8>) -> i32 {
            let mut buf = [0u8; 8192];
            loop {
                match self.s.read(&mut buf) {
                    Ok(0) => return 0,
                    Ok(n) => out.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return 1,
                    Err(_) => return -1,
                }
            }
        }

        /// `Control_write_fd` verbatim — `+1` = `EAGAIN`,
        /// `0` = tout écrit, `-1` = erreur.
        fn write_from(&mut self, input: &[u8], offset: &mut usize) -> i32 {
            loop {
                if *offset >= input.len() {
                    return 0;
                }
                match self.s.write(&input[*offset..]) {
                    Ok(0) => return -1,
                    Ok(n) => *offset += n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return 1,
                    Err(_) => return -1,
                }
            }
        }
    }

    /// Listener `~/.dillo/ctl/<pid>` ≙ `a_Control_init`. Le fichier
    /// socket est supprimé au `Drop` (≙ `a_Control_free` : `unlink`).
    pub struct CtlListener {
        l: UnixListener,
        path: PathBuf,
    }

    impl CtlListener {
        /// `a_Control_init` verbatim : `mkdir(0700)` idempotent sur
        /// `~/.dillo/ctl`, bind sur `~/.dillo/ctl/<pid>`,
        /// `listen(32)`, non-bloquant (`O_NONBLOCK`+`FIONBIO`).
        ///
        /// # Errors
        /// `Err` ≙ le `return -1` amont (MSG + socket désactivé).
        pub fn init(dillo_dir: &Path) -> io::Result<Self> {
            let ctldir = dillo_dir.join("ctl");
            fs::create_dir(&ctldir).or_else(|e| {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(e)
                }
            })?;
            // verbatim `mkdir(ctldir, 0700)` — seul l'utilisateur
            // peut piloter le navigateur à distance.
            fs::set_permissions(&ctldir, fs::Permissions::from_mode(0o700))?;
            let path = ctldir.join(std::process::id().to_string());
            let l = UnixListener::bind(&path)?;
            l.set_nonblocking(true)?;
            Ok(Self { l, path })
        }

        /// `Control_listen_cb` — `accept` non-bloquant ; `Ok(None)`
        /// ≙ `EAGAIN`/`EWOULDBLOCK` amont.
        ///
        /// # Errors
        /// `Err` sur erreur d'accept (amont : `MSG_ERR` + `exit`).
        pub fn accept(&self) -> io::Result<Option<UnixCtlIo>> {
            match self.l.accept() {
                Ok((s, _)) => {
                    // verbatim : O_NONBLOCK explicitement sur le
                    // socket accepté (pas hérité sous Linux).
                    s.set_nonblocking(true)?;
                    Ok(Some(UnixCtlIo { s }))
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
                Err(e) => Err(e),
            }
        }
    }

    impl Drop for CtlListener {
        /// `a_Control_free` — `unlink(control_path)`.
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// Transport scripté — octets entrants en file, sortants
    /// capturés.
    struct FakeIo {
        incoming: VecDeque<u8>,
        written: Rc<RefCell<Vec<u8>>>,
        drained: bool,
    }

    impl FakeIo {
        fn new(input: &[u8]) -> (Self, Rc<RefCell<Vec<u8>>>) {
            let w = Rc::new(RefCell::new(Vec::new()));
            (
                Self {
                    incoming: input.iter().copied().collect(),
                    written: Rc::clone(&w),
                    drained: false,
                },
                w,
            )
        }
    }

    impl CtlIo for FakeIo {
        fn read_byte(&mut self) -> CtlByte {
            match self.incoming.pop_front() {
                Some(b) => CtlByte::Byte(b),
                None => CtlByte::Eof,
            }
        }
        fn read_to_end(&mut self, out: &mut Vec<u8>) -> i32 {
            if self.incoming.is_empty() {
                if self.drained {
                    0
                } else {
                    self.drained = true;
                    0
                }
            } else {
                out.extend(self.incoming.drain(..));
                0
            }
        }
        fn write_from(&mut self, input: &[u8], offset: &mut usize) -> i32 {
            self.written
                .borrow_mut()
                .extend_from_slice(&input[*offset..]);
            *offset = input.len();
            0
        }
    }

    /// Ops scriptés — `bw` 7 actif, replies enregistrées.
    struct FakeOps {
        bw: Option<u32>,
        finished: bool,
        msgs: Vec<String>,
        opened: Vec<String>,
        injected: Vec<(String, u32)>,
        closed: bool,
        by_name_ret: i32,
    }

    impl Default for FakeOps {
        fn default() -> Self {
            Self {
                bw: Some(7),
                finished: false,
                msgs: Vec::new(),
                opened: Vec::new(),
                injected: Vec::new(),
                closed: false,
                by_name_ret: 0,
            }
        }
    }

    impl CtlOps for FakeOps {
        fn first_active_bw(&mut self) -> Option<u32> {
            self.bw
        }
        fn reload_all(&mut self) {}
        fn has_finished(&mut self, _bw: u32) -> bool {
            self.finished
        }
        fn open_urlstr(&mut self, _bw: u32, s: &str) {
            self.opened.push(s.to_string());
        }
        fn location_text(&mut self, _bw: u32) -> String {
            "https://cur/".into()
        }
        fn page_title(&mut self, _bw: u32) -> Option<String> {
            Some("T".into())
        }
        fn set_msg(&mut self, _bw: u32, msg: &str) {
            self.msgs.push(msg.to_string());
        }
        fn get_buf(&mut self, _u: &str) -> Option<Vec<u8>> {
            Some(b"BODY".to_vec())
        }
        fn get_header(&mut self, _u: &str) -> Option<Vec<u8>> {
            Some(b"H: v\n".to_vec())
        }
        fn entry_inject(&mut self, u: &str, _b: &[u8], flags: u32) -> bool {
            self.injected.push((u.to_string(), flags));
            true
        }
        fn repush(&mut self, _bw: u32) {}
        fn close_all(&mut self) {
            self.closed = true;
        }
        fn by_name(&mut self, _bw: u32, _n: &str) -> i32 {
            self.by_name_ret
        }
        fn pid(&mut self) -> i64 {
            4242
        }
    }

    fn run(cmd: &[u8], ops: &mut FakeOps) -> Vec<u8> {
        let mut c = Control::new();
        let (io, w) = FakeIo::new(cmd);
        let id = c.accept(Box::new(io));
        while c.on_readable(id, ops) && !c.wants_write(id) {}
        while c.on_writable(id) {}
        let out = w.borrow().clone();
        out
    }

    #[test]
    fn ping_pong() {
        let mut ops = FakeOps::default();
        assert_eq!(run(b"ping\n", &mut ops), b"0\npong\n");
    }

    #[test]
    fn pid_and_url_title() {
        let mut ops = FakeOps::default();
        assert_eq!(run(b"pid\n", &mut ops), b"0\n4242\n");
        assert_eq!(run(b"url\n", &mut ops), b"0\nhttps://cur/\n");
        assert_eq!(run(b"title\n", &mut ops), b"0\nT\n");
    }

    #[test]
    fn unknown_and_status() {
        let mut ops = FakeOps::default();
        assert_eq!(run(b"zzz\n", &mut ops), b"1\nunknown command: zzz\n");
        assert_eq!(run(b"status hello\n", &mut ops), b"0\n");
        assert_eq!(ops.msgs, ["hello"]);
        // `status` nu → msg vide (verbatim cmd[6] != ' ').
        run(b"status\n", &mut ops);
        assert_eq!(ops.msgs[1], "");
    }

    #[test]
    fn open_and_cmd_dispatch() {
        let mut ops = FakeOps::default();
        assert_eq!(run(b"open https://x/\n", &mut ops), b"0\n");
        assert_eq!(ops.opened, ["https://x/"]);
        assert_eq!(run(b"cmd goto-selection-end\n", &mut ops), b"0\n");
        ops.by_name_ret = -1;
        assert_eq!(run(b"cmd bogus\n", &mut ops), b"1\n");
    }

    #[test]
    fn dump_hdump_and_load() {
        let mut ops = FakeOps::default();
        assert_eq!(run(b"dump\n", &mut ops), b"0\nBODY");
        assert_eq!(run(b"hdump\n", &mut ops), b"0\nH: v\n");
        // load : cmd + corps jusqu'à EOF, flags verbatim.
        assert_eq!(run(b"load\n<html>hi", &mut ops), b"0\n");
        assert_eq!(
            ops.injected,
            [(
                "https://cur/".to_string(),
                // `CA_GotHeader|CA_GotLength` amont (0x1|0x4) —
                // régression A01-D6 : le module local portait 0x10/0x20.
                crate::cache::ca_flags::GOT_HEADER | crate::cache::ca_flags::GOT_LENGTH
            )]
        );
        assert_eq!(ops.injected[0].1, 0x5);
        // rawload : flags 0 verbatim.
        assert_eq!(run(b"rawload\nHTTP/1.0 200\n\nx", &mut ops), b"0\n");
        assert_eq!(ops.injected[1].1, 0);
    }

    #[test]
    fn quit_closes_all() {
        let mut ops = FakeOps::default();
        assert_eq!(run(b"quit\n", &mut ops), b"0\n");
        assert!(ops.closed);
    }

    #[test]
    fn wait_deferred_then_notify() {
        let mut c = Control::new();
        let mut ops = FakeOps::default();
        let (io, w) = FakeIo::new(b"wait\n");
        let id = c.accept(Box::new(io));
        // readable : wait → en attente (réponse différée, verbatim).
        c.on_readable(id, &mut ops);
        assert!(c.is_waiting());
        assert_eq!(c.take_wait_timeout(), Some(60.0));
        assert!(w.borrow().is_empty());
        // notify_finish autre bw → pas de réponse.
        assert_eq!(c.notify_finish(9), None);
        // notify_finish bon bw → "0\n" + write.
        assert_eq!(c.notify_finish(7), Some(id));
        while c.on_writable(id) {}
        assert_eq!(*w.borrow(), b"0\n");
    }

    #[test]
    fn wait_timeout_replies_error() {
        let mut c = Control::new();
        let mut ops = FakeOps::default();
        let (io, w) = FakeIo::new(b"wait 5\n");
        let id = c.accept(Box::new(io));
        c.on_readable(id, &mut ops);
        assert_eq!(c.take_wait_timeout(), Some(5.0));
        // verbatim : « 1\nCurrent url not cached » (message étrange
        // amont conservé).
        assert_eq!(c.on_wait_timeout(), Some(id));
        while c.on_writable(id) {}
        assert_eq!(*w.borrow(), b"1\nCurrent url not cached");
    }

    #[test]
    fn wait_finished_immediate_and_double_wait() {
        let mut c = Control::new();
        let mut ops = FakeOps {
            finished: true,
            ..FakeOps::default()
        };
        let (io, w) = FakeIo::new(b"wait\n");
        let id = c.accept(Box::new(io));
        c.on_readable(id, &mut ops);
        assert!(!c.is_waiting());
        while c.on_writable(id) {}
        assert_eq!(*w.borrow(), b"0\n");

        // Double wait : « already waiting » verbatim.
        ops.finished = false;
        let (io1, _w1) = FakeIo::new(b"wait\n");
        let (io2, w2) = FakeIo::new(b"wait\n");
        let id1 = c.accept(Box::new(io1));
        let id2 = c.accept(Box::new(io2));
        c.on_readable(id1, &mut ops);
        c.on_readable(id2, &mut ops);
        while c.on_writable(id2) {}
        assert_eq!(*w2.borrow(), b"1\nalready waiting\n");
    }

    /// A01-D11 : `atof(&cmd[5])` amont consomme le préfixe numérique —
    /// `wait 5abc` → 5 s, pas attente infinie.
    #[test]
    fn wait_timeout_atof_prefix() {
        let mut c = Control::new();
        let mut ops = FakeOps::default();
        let (io, _w) = FakeIo::new(b"wait 5abc\n");
        let id = c.accept(Box::new(io));
        c.on_readable(id, &mut ops);
        assert_eq!(c.take_wait_timeout(), Some(5.0));
        // Sans préfixe numérique → 0 → infini (verbatim) — `Control`
        // frais (un seul `wait` en vol par instance amont).
        let mut c2 = Control::new();
        let (io2, _w2) = FakeIo::new(b"wait zzz\n");
        let id2 = c2.accept(Box::new(io2));
        c2.on_readable(id2, &mut ops);
        assert_eq!(c2.take_wait_timeout(), None);
    }

    #[test]
    fn empty_line_closes() {
        let mut c = Control::new();
        let mut ops = FakeOps::default();
        let (io, _w) = FakeIo::new(b"");
        let id = c.accept(Box::new(io));
        // EOF sans cmd → close (verbatim err||len==0).
        assert!(!c.on_readable(id, &mut ops));
    }
}
