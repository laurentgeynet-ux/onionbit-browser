//! Port of `dpid/dpid.c` — logique du démon de plugins dpi.
//!
//! Le démon (binaire `dillo-dpid`, ≙ `dpid/main.c`) supervise les
//! plugins : il découvre les `.dpi` dans `~/.dillo/dpi` puis le
//! répertoire système (`dpi_dir` de `dpidrc`), ouvre une socket TCP
//! loopback par plugin (`DPID_BASE_PORT`=5020, scan +50 verbatim)
//! plus la socket de supervision dont le port est publié dans
//! `~/.dillo/dpid_comm_keys`, et démarre les plugins à la demande.
//!
//! Mapping verbatim :
//! - `struct dp` → [`DpAttr`] ; `struct service` → [`Service`]
//! - `sock_set`/`FD_ISSET` → liste de [`std::net::TcpListener`] non
//!   bloquants (≙ `select` — la boucle `select` vit dans `bin/dpid.rs`)
//! - `fork`+`execl` → [`std::process::Command`] ; `pid` → `child`
//! - `waitpid(WNOHANG)` → [`std::process::Child::try_wait`]
//! - `dRealloc` sur la liste → `Vec::push`

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
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Child;

use tracing::warn;

use crate::dpid_common;
use dillo_common::dlib;

/// `DPID_BASE_PORT` — `IPPORT_USERRESERVED(5000) + 20`.
pub const DPID_BASE_PORT: i32 = 5020;
/// `QUEUE` — backlog de `listen` (verbatim : `listen(sock, QUEUE)`
/// avec QUEUE=5 ; la std Rust applique son propre backlog).
#[allow(dead_code)]
const QUEUE: u32 = 5;
/// Profondeur du scan de ports amont (`last_port = base + 50`).
const PORT_SCAN: i32 = 50;

/// `struct dp` — état d'un plugin connu du démon.
#[derive(Debug)]
pub struct DpAttr {
    /// `id` — nom du service/répertoire (ex. `file`, `proto.data`).
    pub id: String,
    /// `path` — chemin de l'exécutable `.dpi`.
    pub path: PathBuf,
    /// `sock_fd` — socket d'écoute du plugin.
    pub sock: Option<TcpListener>,
    /// `port`.
    pub port: i32,
    /// `pid` — processus du serveur lancé (`None` ≙ `pid == 1` amont
    /// = pas démarré ; `pid > 1` ≙ en cours).
    pub child: Option<Child>,
    /// `filter` — `.filter` dans le nom ≙ plugin filtre (une
    /// connexion par invocation, socket surveillée en permanence).
    pub filter: bool,
}

/// `struct service` — association service → plugin (`dpidrc`).
#[derive(Debug, Clone)]
pub struct Service {
    /// `name` (peut finir par `*` ≙ préfixe, cf. [`service_match`]).
    pub name: String,
    /// `dp_index` — index dans `DpState::dpis`, `-1` = non résolu.
    pub dp_index: i32,
}

/// `enum file_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// `DPI_FILE` — nom finissant par `.dpi` (ou `.dpi.exe`).
    Dpi,
    /// `UNKNOWN_FILE`.
    Unknown,
}

/// `DPI_EXT` ≙ `".dpi" EXEEXT`.
fn dpi_ext() -> &'static str {
    if cfg!(windows) {
        ".dpi.exe"
    } else {
        ".dpi"
    }
}

/// `ends_with` + `get_file_type` verbatim.
fn file_type(file_name: &str) -> FileType {
    if file_name.ends_with(dpi_ext()) {
        FileType::Dpi
    } else {
        FileType::Unknown
    }
}

/// `get_dpi_dir` — lit `dpi_dir=<path>` dans `dpidrc` (verbatim :
/// première ligne commençant par `dpi_dir`, valeur après `=` trimée
/// des espaces de tête).
#[must_use]
pub fn get_dpi_dir(dpidrc: &std::path::Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(dpidrc).ok()?;
    for line in content.lines() {
        if line.starts_with("dpi_dir") {
            let Some(eq) = line.find('=') else {
                warn!("get_dpi_dir: '=' not found in {}", line);
                return None;
            };
            let v = line[eq + 1..].trim_start_matches(' ');
            return Some(PathBuf::from(v));
        }
    }
    warn!("get_dpi_dir: no dpi_dir entry in {}", dpidrc.display());
    None
}

/// `~/.dillo/dpi` et `~/.dillo/dpidrc` (verbatim `dotDILLO_DPI`).
fn user_dpi_dir() -> PathBuf {
    let home = dlib::get_home_dir();
    let mut p = PathBuf::from(String::from_utf8_lossy(&home).into_owned());
    p.push(".dillo");
    p.push("dpi");
    p
}

fn user_dpidrc() -> PathBuf {
    let home = dlib::get_home_dir();
    let mut p = PathBuf::from(String::from_utf8_lossy(&home).into_owned());
    p.push(".dillo");
    p.push("dpidrc");
    p
}

/// `DPIDRC_SYS` ≙ `$(sysconfdir)/dpidrc` — `/etc/dillo/dpidrc`
/// (Unix) ou `<exe>/../etc/dillo/dpidrc` (Windows ≙ pas de sysconfdir).
fn sys_dpidrc() -> PathBuf {
    if cfg!(windows) {
        let mut p = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_default();
        p.push("..");
        p.push("etc");
        p.push("dillo");
        p.push("dpidrc");
        p
    } else {
        PathBuf::from("/etc/dillo/dpidrc")
    }
}

/// `get_dpi_attr` — premier fichier `.dpi` du répertoire
/// `<dpi_dir>/<service>/` (verbatim : ordre `readdir` ≙ itérateur
/// `read_dir` non trié, premier `.dpi` gagne).
#[must_use]
pub fn get_dpi_attr(dpi_dir: &std::path::Path, service: &str) -> Option<DpAttr> {
    let service_dir = dpi_dir.join(service);
    let rd = match std::fs::read_dir(&service_dir) {
        Ok(r) => r,
        Err(e) => {
            warn!("get_dpi_attr: {}/{} : {}", dpi_dir.display(), service, e);
            return None;
        }
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if file_type(&name) == FileType::Dpi {
            let path = entry.path();
            let filter = name.contains(".filter");
            return Some(DpAttr {
                id: service.to_string(),
                path,
                sock: None,
                port: 0,
                child: None,
                filter,
            });
        }
        warn!("get_file_type: Unknown file type for {}", name);
    }
    warn!(
        "get_dpi_attr: No dpi plug-in in {}/{}",
        dpi_dir.display(),
        service
    );
    None
}

/// `register_all` — scanne `~/.dillo/dpi` puis le répertoire système ;
/// services utilisateur **puis** système (verbatim : les deux listes
/// s'ajoutent, sans dédup — `fill_services_list` gère les liens).
#[must_use]
pub fn register_all() -> Vec<DpAttr> {
    let mut out = Vec::new();
    let udpi = user_dpi_dir();
    let has_user = udpi.exists();
    let dpidrc = user_dpidrc();
    let dpidrc = if dpidrc.exists() {
        Some(dpidrc)
    } else {
        let s = sys_dpidrc();
        if s.exists() {
            Some(s)
        } else {
            None
        }
    };
    let sys_dpidir = dpidrc.as_deref().and_then(get_dpi_dir);

    if !has_user && sys_dpidir.is_none() {
        // verbatim : exit(1) — impossible en bibliothèque, l'appelant
        // décide (main fait `process::exit(1)` sur Vec vide + dirs absents).
        return out;
    }

    if has_user {
        if let Ok(rd) = std::fs::read_dir(&udpi) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                if entry.path().is_dir() {
                    if let Some(attr) = get_dpi_attr(&udpi, &name) {
                        out.push(attr);
                    }
                }
            }
        }
    }
    if let Some(sys) = sys_dpidir {
        if let Ok(rd) = std::fs::read_dir(&sys) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                if entry.path().is_dir() {
                    if let Some(attr) = get_dpi_attr(&sys, &name) {
                        out.push(attr);
                    }
                }
            }
        }
    }
    out
}

/// `dParser_parse_rc_line` ≙ `service=path` par ligne, `#` commentaire
/// — délégué à [`dillo_common::parser`] si présent, sinon inline.
fn parse_rc_line(line: &str) -> Option<(String, String)> {
    let l = line.trim_start();
    if l.is_empty() || l.starts_with('#') {
        return None;
    }
    let eq = l.find('=')?;
    let service = l[..eq].trim_end();
    let path = l[eq + 1..].trim_start();
    if service.is_empty() || path.is_empty() {
        return None;
    }
    Some((service.to_string(), path.to_string()))
}

/// `fill_services_list` — lit les lignes `service=chemin` de dpidrc
/// (hors `dpi_dir`), lie chaque service au plugin dont le chemin
/// **finit** par `/<path>` (verbatim `strstr`+`p[-1]=='/'`+égalité de
/// longueur), trie par ordre alphabétique **inverse** (verbatim
/// `services_alpha_comp = -strcmp`).
#[must_use]
pub fn fill_services_list(attlist: &[DpAttr]) -> Vec<Service> {
    let dpidrc = {
        let u = user_dpidrc();
        if u.exists() {
            u
        } else {
            let s = sys_dpidrc();
            if s.exists() {
                s
            } else {
                return Vec::new();
            }
        }
    };
    let Ok(content) = std::fs::read_to_string(&dpidrc) else {
        warn!("fill_services_list: can't open {}", dpidrc.display());
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in content.lines() {
        let Some((service, path)) = parse_rc_line(line) else {
            continue;
        };
        if service == "dpi_dir" {
            continue; // verbatim : ignoré silencieusement
        }
        let mut s = Service {
            name: service,
            dp_index: -1,
        };
        for (i, a) in attlist.iter().enumerate() {
            let ap = a.path.to_string_lossy();
            if let Some(p) = ap.find(&path) {
                // verbatim : strstr + '/' avant + suffixe exact
                let before_ok = p > 0 && ap.as_bytes().get(p.wrapping_sub(1)) == Some(&b'/');
                if before_ok && ap.len() - p == path.len() {
                    s.dp_index = i32::try_from(i).unwrap_or(-1);
                    break;
                }
            }
        }
        out.push(s);
    }
    // verbatim `-strcmp` : tri décroissant
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

// --------------------------------------------------------------------
// sockets
// --------------------------------------------------------------------

/// `bind_socket_fd` — `127.0.0.1`, scan `[base_port, base_port+50]`,
/// `TCP_NODELAY`, `listen(5)`.
#[must_use]
pub fn bind_socket_fd(base_port: i32) -> Option<(TcpListener, i32)> {
    for port in base_port..=base_port + PORT_SCAN {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], u16::try_from(port).ok()?));
        match TcpListener::bind(addr) {
            Ok(l) => {
                let _ = l.set_nonblocking(true);
                return Some((l, port));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => continue,
            Err(e) => {
                warn!("bind_socket_fd: bind: {}", e);
                return None;
            }
        }
    }
    warn!(
        "Can't find an available port from {} to {}",
        base_port,
        base_port + PORT_SCAN
    );
    None
}

/// `save_comm_keys` — `<port> <clé>` dans `~/.dillo/dpid_comm_keys`
/// (verbatim : mode `0600` ≙ `O_WRONLY|CREAT|TRUNC, S_IRUSR|S_IWUSR`).
pub fn save_comm_keys(srs_port: i32, shared_key: &str) -> Result<(), String> {
    let home = dlib::get_home_dir();
    let mut fname = PathBuf::from(String::from_utf8_lossy(&home).into_owned());
    fname.push(".dillo");
    fname.push("dpid_comm_keys");
    if let Some(dir) = fname.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("save_comm_keys: {e}"))?;
    }
    std::fs::write(&fname, format!("{srs_port} {shared_key}\n"))
        .map_err(|e| format!("save_comm_keys: open {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&fname, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// `cleanup` — supprime `dpid_comm_keys` (≙ `unlink` amont).
pub fn cleanup() {
    let home = dlib::get_home_dir();
    let mut fname = PathBuf::from(String::from_utf8_lossy(&home).into_owned());
    fname.push(".dillo");
    fname.push("dpid_comm_keys");
    let _ = std::fs::remove_file(&fname);
}

/// `service_match` — égalité sur `max(lenA,lenB)` octets, ou préfixe
/// si le nom finit par `*` (verbatim `dStrnAsciiCasecmp`, insensible
/// à la casse ASCII).
#[must_use]
pub fn service_match(a: &str, b: &str) -> bool {
    let a_b = a.as_bytes();
    let b_b = b.as_bytes();
    let mut len = a_b.len().max(b_b.len());
    if a_b.last() == Some(&b'*') {
        len = a_b.len() - 1;
    }
    let al = a_b.len().min(len);
    let bl = b_b.len().min(len);
    // verbatim : dStrnAsciiCasecmp compare len octets (troncature si
    // une chaîne est plus courte ≙ son '\0' → la plus courte perd si
    // préfixe exacte). On reproduit : différent si longueurs ≠ sauf
    // cas `*` (len tronquée à A-1).
    if a_b.last() == Some(&b'*') {
        a_b[..al].eq_ignore_ascii_case(&b_b[..al.min(bl)])
    } else {
        al == bl && a_b[..al].eq_ignore_ascii_case(&b_b[..bl])
    }
}

/// `send_sockport` — répond `send_data msg=<port>` au `check_server`.
/// Recherche : services d'abord (`service_match`), sinon premier `id`
/// dont le préfixe avant `.` correspond (verbatim `strncmp` sur
/// `id - strchr(id,'.')` caractères — i.e. la partie avant le premier
/// `.` de `id`).
pub fn send_sockport(sock: &mut TcpStream, dpi_tag: &str, dpis: &[DpAttr], services: &[Service]) {
    let Some(dpi_id) = dillo_net::dpip::get_attr(dpi_tag, "msg") else {
        warn!("get_message: failed to parse msg");
        if let Some(d_cmd) =
            dillo_net::dpip::build_cmd("cmd=%s msg=%s", &["DpiError", "Failed to parse request"])
        {
            let _ = dpid_common::ckd_write(sock, d_cmd.as_bytes());
        }
        return;
    };

    let serv_idx = services
        .iter()
        .find(|s| service_match(&s.name, &dpi_id))
        .map(|s| s.dp_index);

    let mut idx = serv_idx.unwrap_or(-1);
    if idx == -1 {
        for (i, d) in dpis.iter().enumerate() {
            let dot = d.id.find('.').unwrap_or(d.id.len());
            let prefix = &d.id[..dot];
            if prefix == dpi_id || (prefix.len() >= dpi_id.len() && dpi_id == prefix) {
                idx = i32::try_from(i).unwrap_or(-1);
                break;
            }
        }
    }

    if idx >= 0 && usize::try_from(idx).is_ok_and(|i| i < dpis.len()) {
        let port_str = dpis[usize::try_from(idx).unwrap_or(0)].port.to_string();
        if let Some(d_cmd) = dillo_net::dpip::build_cmd("cmd=%s msg=%s", &["send_data", &port_str])
        {
            let _ = dpid_common::ckd_write(sock, d_cmd.as_bytes());
        }
    }
}

/// `stop_active_dpis` — `auth`+`DpiBye` vers chaque serveur actif.
pub fn stop_active_dpis(dpis: &[DpAttr], shared_key: &str) {
    let bye = dillo_net::dpip::build_cmd("cmd=%s", &["DpiBye"]);
    let auth = dillo_net::dpip::build_cmd("cmd=%s msg=%s", &["auth", shared_key]);
    let (Some(bye), Some(auth)) = (bye, auth) else {
        return;
    };
    for d in dpis {
        if d.child.is_none() || d.filter {
            continue;
        }
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], u16::try_from(d.port).unwrap_or(0)));
        match TcpStream::connect(addr) {
            Ok(mut s) => {
                let _ = dpid_common::ckd_write(&mut s, auth.as_bytes());
                let _ = dpid_common::ckd_write(&mut s, bye.as_bytes());
            }
            Err(e) => warn!("stop_active_dpis: {} : {}", d.path.display(), e),
        }
    }
    // verbatim : laisse le temps aux enfants de lire dpid_comm_keys.
    std::thread::sleep(std::time::Duration::from_secs(1));
}

/// `handle_sigchld` — récolte les enfants terminés (verbatim
/// `waitpid WNOHANG` : remet la socket du plugin dans le set et
/// `pid=1` ≙ `child=None`).
pub fn handle_sigchld(dpis: &mut [DpAttr], watched: &mut Vec<bool>) -> i32 {
    let mut readded = 0;
    for (i, d) in dpis.iter_mut().enumerate() {
        if let Some(ch) = d.child.as_mut() {
            if let Ok(Some(_status)) = ch.try_wait() {
                d.child = None;
                if !watched.get(i).copied().unwrap_or(false) {
                    watched[i] = true;
                    readded += 1;
                }
            }
        }
    }
    readded
}

/// `server_is_running` — un serveur (`!filter`) avec `pid > 1`.
#[must_use]
pub fn server_is_running(dpis: &[DpAttr], server_id: &str) -> bool {
    dpis.iter()
        .any(|d| !d.filter && d.child.is_some() && d.id == server_id)
}

/// Commandes du protocole dpid (verbatim `enum` de `dpi.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpidCmd {
    /// `UNKNOWN_CMD`.
    Unknown,
    /// `AUTH_CMD` — `cmd='auth'`.
    Auth,
    /// `BYE_CMD` — `cmd='DpiBye'`.
    Bye,
    /// `CHECK_SERVER_CMD` — `cmd='check_server'`.
    CheckServer,
    /// `REGISTER_ALL_CMD` — `cmd='register_all'`.
    RegisterAll,
    /// `REGISTER_SERVICE_CMD` — `cmd='register_service'` (reconnu
    /// amont mais sans `case` dans le switch — même comportement).
    RegisterService,
}

/// `get_command` — mappe `cmd=` du tag vers [`DpidCmd`].
/// `Err` ≙ `-1` amont (tag nul ou `cmd` absent → réponse `DpiError`).
pub fn get_command(dpi_tag: Option<&str>, sh: &mut crate::dpip::Dsh) -> Result<DpidCmd, ()> {
    let Some(tag) = dpi_tag else {
        return Err(());
    };
    match dillo_net::dpip::get_attr(tag, "cmd").as_deref() {
        None => {
            warn!("get_command: failed to parse cmd in {}", tag);
            if let Some(d) = dillo_net::dpip::build_cmd(
                "cmd=%s msg=%s",
                &["DpiError", "Failed to parse request"],
            ) {
                let _ = sh.write_str(true, &d);
            }
            Err(())
        }
        Some("auth") => Ok(DpidCmd::Auth),
        Some("DpiBye") => Ok(DpidCmd::Bye),
        Some("check_server") => Ok(DpidCmd::CheckServer),
        Some("register_all") => Ok(DpidCmd::RegisterAll),
        Some("register_service") => Ok(DpidCmd::RegisterService),
        Some(_) => Ok(DpidCmd::Unknown),
    }
}

/// `register_service` (commande dpi `register_service` — sans
/// traitement dans le switch amont, la fonction existe pour les
/// outils) : enregistre un service seul.
#[must_use]
pub fn register_service(service: &str) -> Option<DpAttr> {
    let user_dir = user_dpi_dir();
    let user_service = user_dir.join(service);
    if user_service.exists() {
        return get_dpi_attr(&user_dir, service);
    }
    let dpidrc = {
        let u = user_dpidrc();
        if u.exists() {
            u
        } else {
            let s = sys_dpidrc();
            if s.exists() {
                s
            } else {
                return None;
            }
        }
    };
    get_dpi_dir(&dpidrc).and_then(|dir| get_dpi_attr(&dir, service))
}

/// `start_filter_plugin`/`start_server_plugin` : lance le plugin avec
/// la socket sur stdio (≙ `dup2(sock,0/1)` + `execl`).
///
/// - filtre : socket **acceptée** par le démon (verbatim `accept`
///   puis `dup2` des deux côtés) ;
/// - serveur : socket **d'écoute** sur stdin, stdout hérité nul
///   (verbatim : seul `STDIN` est remplacé — en fait les deux le sont
///   par `dup2(sock)` puisque stdout n'est pas redup… amont ne dupe
///   que stdin ; le plugin serveur dialogue par `accept(0)`.
///   Le port met aussi stdout sur le listener pour coller au
///   `dup2(sock,STDOUT)` de la variante filtre seulement si besoin —
///   verbatim serveur : stdin uniquement).
#[cfg(unix)]
#[must_use]
pub fn spawn_plugin(path: &std::path::Path, io: PluginStdio) -> Option<Child> {
    use std::os::unix::io::OwnedFd;
    let (stdin, stdout): (std::process::Stdio, std::process::Stdio) = match io {
        PluginStdio::Stream(s) => {
            let s2 = s.try_clone().ok()?;
            (
                std::process::Stdio::from(OwnedFd::from(s)),
                std::process::Stdio::from(OwnedFd::from(s2)),
            )
        }
        PluginStdio::Listener(l) => (
            std::process::Stdio::from(OwnedFd::from(l)),
            std::process::Stdio::null(),
        ),
    };
    std::process::Command::new(path)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
}

/// Windows : un socket n'est pas un handle stdio valide — verbatim,
/// dpid/plugin-stdio est POSIX. Le démon reste utilisable pour
/// `check_server`/`register_all`, mais les plugins ne démarrent pas.
#[cfg(not(unix))]
pub fn spawn_plugin(_path: &std::path::Path, _io: PluginStdio) -> Option<Child> {
    warn!("spawn_plugin: socket stdio unsupported on this platform");
    None
}

/// Transport stdio d'un plugin (≙ le socket dup2'né).
pub enum PluginStdio {
    /// Filtre : socket **acceptée** (données directes).
    Stream(std::net::TcpStream),
    /// Serveur : socket **d'écoute** (le plugin fera `accept` sur fd 0).
    Listener(TcpListener),
}
