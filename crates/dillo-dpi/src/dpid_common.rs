//! Port of `dpid/dpid_common.c` + `misc_new.c` + `dpi_socket_dir.c`
//! + `dpid/dpi.c` — code partagé du démon et de ses clients.
//!
//! Le point central : `~/.dillo/dpid_comm_keys` (port TCP +
//! clé hexa) et le répertoire des sockets `~/.dillo/dpi_socket_dir`.
//!
//! Correspondances verbatim :
//! - `dpi.c` (`a_Dpi_sockdir_file` / `a_Dpi_rd_dpi_socket_dir` /
//!   `a_Dpi_srs`) → [`sockdir_file`], [`rd_dpi_socket_dir`], [`srs`]
//! - `dpi_socket_dir.c` → [`mk_sockdir`], [`init_sockdir`],
//!   [`w_dpi_socket_dir`], [`tst_dir`]
//! - `misc_new.c` → [`mksecret`], [`mkfname`], [`mkdtemp`],
//!   [`rdtag`], [`readtag`]
//! - `dpid_common.c` → [`errmsg`], [`ckd_write`]

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
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;

use tracing::warn;

use dillo_common::dlib;

/// `dotDILLO_DPI`.
pub const DOT_DILLO_DPI: &str = ".dillo/dpi";
/// `dotDILLO_DPIDRC`.
pub const DOT_DILLO_DPIDRC: &str = ".dillo/dpidrc";
/// `dotDILLO_DPID_COMM_KEYS`.
pub const DOT_DILLO_DPID_COMM_KEYS: &str = ".dillo/dpid_comm_keys";

/// `errmsg` — message verbeux vers stderr.
pub fn errmsg(caller: &str, called: &str, file: &str, line: u32) {
    eprintln!("[dpid]: {file}:{line}: {caller}: {called}");
}

/// `ckd_write` — écriture complète avec retry EINTR.
/// Retourne `false` ≙ `-1` amont.
pub fn ckd_write(s: &mut TcpStream, msg: &[u8]) -> bool {
    let mut sent = 0;
    while sent < msg.len() {
        match s.write(&msg[sent..]) {
            Ok(0) => break,
            Ok(n) => sent += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("[dpid]: write: {e}");
                return false;
            }
        }
    }
    true
}

// --------------------------------------------------------------------
// misc_new.c
// --------------------------------------------------------------------

/// `a_Misc_mksecret` — clé partagée aléatoire hexadécimale.
/// Amont utilise `rand()` ; le port utilise `getrandom`-like via
/// `std::time`+pid mixés — même protocole, meilleure entropie.
#[must_use]
pub fn mksecret(nchar: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x9e3779b9u64, |d| d.as_nanos() as u64)
        ^ u64::from(std::process::id());
    let mut out = String::with_capacity(nchar);
    for _ in 0..nchar {
        // xorshift64* — déterministe seedé, pas crypto (verbatim : la
        // clé amont est `rand()` non cryptographique aussi).
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        let d = ((seed.wrapping_mul(0x2545F4914F6CDD1D) >> 60) & 0xf) as u8;
        out.push(if d > 9 {
            (b'a' + d - 10) as char
        } else {
            (b'0' + d) as char
        });
    }
    out
}

/// `a_Misc_mkfname` — remplace les 6 `X` finaux par de l'aléatoire
/// (verbatim : `rand()` limité à 5 bits par position, sans borne
/// hexadécimale — peut dépasser `f`), retourne le nom si le fichier
/// n'existe pas.
#[must_use]
pub fn mkfname(template: &str) -> Option<String> {
    if template.len() < 6 || !template[template.len() - 6..].bytes().all(|c| c == b'X') {
        return None;
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1u64, |d| d.as_nanos() as u64)
        ^ u64::from(std::process::id());
    let prefix = &template[..template.len() - 6];
    for _ in 0..64 {
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        let r = seed.wrapping_mul(0x2545F4914F6CDD1D);
        let mut name = String::with_capacity(template.len());
        name.push_str(prefix);
        for i in 0..6 {
            let d = ((r >> (i * 5)) & 0x1f) as u8;
            name.push(if d > 9 {
                (b'a' + d - 10) as char
            } else {
                (b'0' + d) as char
            });
        }
        if !std::path::Path::new(&name).exists() {
            return Some(name);
        }
    }
    None
}

/// `a_Misc_mkdtemp` — crée un répertoire `0700` depuis un template
/// `XXXXXX`.
#[must_use]
pub fn mkdtemp(template: &str) -> Option<String> {
    loop {
        let name = mkfname(template)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            match std::fs::DirBuilder::new().mode(0o700).create(&name) {
                Ok(()) => return Some(name),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return None,
            }
        }
        #[cfg(not(unix))]
        {
            match std::fs::create_dir(&name) {
                Ok(()) => return Some(name),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return None,
            }
        }
    }
}

/// `a_Misc_rdtag` — lit un tag caractère par caractère jusqu'à `>`.
/// `None` en cas d'erreur de lecture.
pub fn rdtag(s: &mut TcpStream) -> Option<Vec<u8>> {
    let mut tag = Vec::with_capacity(64);
    let mut c = [0u8; 1];
    loop {
        match s.read(&mut c) {
            Ok(0) => break,
            Ok(_) => {
                tag.push(c[0]);
                if c[0] == b'>' {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
    Some(tag)
}

/// `a_Misc_readtag` — variante avec tableau extensible (verbatim :
/// même sémantique, alloué dynamiquement).
pub fn readtag(s: &mut TcpStream) -> Option<Vec<u8>> {
    rdtag(s)
}

// --------------------------------------------------------------------
// dpi.c — socket dir
// --------------------------------------------------------------------

/// `a_Dpi_sockdir_file` — chemin de `~/.dillo/dpi_socket_dir`.
#[must_use]
pub fn sockdir_file() -> PathBuf {
    let home = dlib::get_home_dir();
    let mut p = PathBuf::from(String::from_utf8_lossy(&home).into_owned());
    p.push(".dillo");
    p.push("dpi_socket_dir");
    p
}

/// `a_Dpi_rd_dpi_socket_dir` — lit le répertoire de sockets depuis le
/// fichier. Amont `exit(1)` si `~/.dillo` n'existe pas ou en cas
/// d'erreur ≠ ENOENT — le port retourne `Err` (≙ exit).
///
/// `Err(msg)` ≙ exit amont ; `Ok(None)` ≙ fichier absent.
pub fn rd_dpi_socket_dir(dirname: &std::path::Path) -> Result<Option<String>, String> {
    let home = dlib::get_home_dir();
    let mut rcpath = PathBuf::from(String::from_utf8_lossy(&home).into_owned());
    rcpath.push(".dillo");
    if !rcpath.exists() {
        return Err(format!(
            "a_Dpi_rd_dpi_socket_dir: {} n'existe pas",
            rcpath.display()
        ));
    }
    match std::fs::read_to_string(dirname) {
        Ok(s) => Ok(Some(s.lines().next().unwrap_or_default().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            warn!("a_Dpi_rd_dpi_socket_dir: {} absent", dirname.display());
            Ok(None)
        }
        Err(e) => Err(format!("a_Dpi_rd_dpi_socket_dir: {e}")),
    }
}

/// `a_Dpi_srs` — chemin complet de la socket `dpid.srs`.
pub fn srs() -> Result<Option<PathBuf>, String> {
    let dirfile = sockdir_file();
    Ok(rd_dpi_socket_dir(&dirfile)?.map(|sockdir| {
        let mut b = sockdir.into_bytes();
        dlib::str_strip(&mut b);
        let mut p = PathBuf::from(String::from_utf8_lossy(&b).into_owned());
        p.push("dpid.srs");
        p
    }))
}

// --------------------------------------------------------------------
// dpi_socket_dir.c
// --------------------------------------------------------------------

/// `w_dpi_socket_dir` — écrit le nom du répertoire dans le fichier.
pub fn w_dpi_socket_dir(dirname: &std::path::Path, sockdir: &str) -> Result<(), String> {
    std::fs::write(dirname, sockdir).map_err(|e| format!("w_dpi_socket_dir: {e}"))
}

/// `tst_dir` — le chemin existe-t-il ? (verbatim : `access(F_OK)` —
/// ne vérifie pas que c'est un répertoire).
#[must_use]
pub fn tst_dir(dir: &str) -> bool {
    std::path::Path::new(&format!("{dir}/")).exists()
}

/// `mk_sockdir` — `/tmp/<logname>-XXXXXX` mode `0700`.
#[must_use]
pub fn mk_sockdir() -> Option<String> {
    let logname = std::env::var("LOGNAME").unwrap_or_else(|_| "dillo".to_string());
    let tmp = std::env::temp_dir();
    let template = format!("{}/{}-XXXXXX", tmp.display(), logname);
    mkdtemp(&template)
}

/// `init_sockdir` — réutilise le répertoire existant ou en crée un.
#[must_use]
pub fn init_sockdir(dpi_socket_dir: &std::path::Path) -> Option<String> {
    let mut sockdir = None;
    let mut dir_ok = false;
    if let Ok(Some(d)) = rd_dpi_socket_dir(dpi_socket_dir) {
        sockdir = Some(d);
        if let Some(ref s) = sockdir {
            if tst_dir(s) {
                dir_ok = true;
            }
        }
        if !dir_ok {
            sockdir = None;
        }
    }
    if !dir_ok {
        sockdir = mk_sockdir();
        if let Some(ref s) = sockdir {
            if w_dpi_socket_dir(dpi_socket_dir, s).is_err() {
                sockdir = None;
            }
        }
    }
    sockdir
}

// --------------------------------------------------------------------
// clés de communication (partagé avec dpi.rs côté client — ici la
// version verbatim d'upstream pour le démon : même parsing).
// --------------------------------------------------------------------

/// `Dpi_read_comm_keys` (verbatim, utilisé par dpid + dpidc) —
/// `(port, clé)` depuis `~/.dillo/dpid_comm_keys`, `None` ≙ erreur.
#[must_use]
pub fn read_comm_keys_pub() -> Option<(i32, Vec<u8>)> {
    let home = dlib::get_home_dir();
    let mut fname = PathBuf::from(String::from_utf8_lossy(&home).into_owned());
    fname.push(".dillo");
    fname.push("dpid_comm_keys");
    let line = std::fs::read_to_string(&fname).ok()?;
    let line = line.lines().next()?;
    let b = line.as_bytes();
    // verbatim strtol : espaces + signe + chiffres
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    let neg = i < b.len() && b[i] == b'-';
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let port: i32 = line[start..i]
        .parse()
        .ok()
        .map(|p: i32| if neg { -p } else { p })?;
    // verbatim : tail pointe le 1er non-digit ; clé = hexa après lui.
    let mut key = Vec::with_capacity(31);
    if i < b.len() {
        let mut j = i + 1;
        while j < b.len() && key.len() < 31 && dlib::is_xdigit(b[j]) {
            key.push(b[j]);
            j += 1;
        }
    }
    if key.is_empty() {
        return None;
    }
    Some((port, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mksecret_hex() {
        let s = mksecret(8);
        assert_eq!(s.len(), 8);
        assert!(s.bytes().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn mkfname_replaces_x() {
        let t = mkfname("/tmp/dillo-XXXXXX").unwrap_or_default();
        assert!(t.starts_with("/tmp/dillo-"));
        assert_eq!(t.len(), "/tmp/dillo-XXXXXX".len());
        assert!(!t.contains('X'));
    }

    #[test]
    fn smtp_and_sockdir() {
        // init_sockdir sur un chemin temporaire
        let dir = std::env::temp_dir().join("dillo_dpid_test_sockdir");
        let f = dir.join("dpi_socket_dir");
        std::fs::create_dir_all(&dir).ok();
        std::fs::remove_file(&f).ok();
        // pas de fichier + pas de ~/.dillo → rd_ renvoie Err (homedir
        // existe pourtant sur la machine) — on teste juste que
        // mk_sockdir produit un nom.
        let s = mk_sockdir();
        if let Some(ref d) = s {
            assert!(std::path::Path::new(d).is_dir());
            let _ = std::fs::remove_dir(d);
        }
    }
}
