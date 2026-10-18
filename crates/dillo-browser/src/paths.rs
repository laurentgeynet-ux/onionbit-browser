//! Port de `src/paths.cc` + `paths.hh`.
//!
//! `Paths` amont = 3 statiques + un tampon `oldWorkingDir` :
//! - `init` : mémorise le cwd, `chdir("/tmp")` (Dillo travaille
//!   depuis un répertoire non monté), crée `~/.dillo` (0700) si
//!   absent.
//! - `get_prefs_fp` : ouvre `~/.dillo/<rc>` sinon
//!   `DILLO_SYSCONF/<rc>` (défaut compilé amont → ici variable
//!   d'env `DILLO_SYSCONF` puis `/etc/dillo/` ≙
//!   `--sysconfdir` des builds classiques).
//!
//! Windows : `chdir` devient `std::env::set_current_dir` vers le
//! répertoire temporaire (`%TEMP%` ≙ `/tmp`) — déviation
//! documentée (amont est POSIX-first, ADR-0002 §« Windows »).

#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::io;
use std::path::PathBuf;

/// `PATHS_RC_PREFS`.
pub const RC_PREFS: &str = "dillorc";
/// `PATHS_RC_KEYS`.
pub const RC_KEYS: &str = "keysrc";
/// `PATHS_RC_DOMAIN`.
pub const RC_DOMAIN: &str = "domainrc";
/// `PATHS_HSTS_PRELOAD`.
pub const HSTS_PRELOAD: &str = "hsts_preload";

thread_local! {
    /// `static char *oldWorkingDir` — cwd de lancement (peut être
    /// libéré par `paths_free`, verbatim `dFree`).
    static OLD_WORKING_DIR: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

/// Répertoire système de secours ≙ macro `DILLO_SYSCONF`
/// (configure `--sysconfdir`) : variable d'env `DILLO_SYSCONF`
/// si définie, sinon `/etc/dillo/` (valeur classique amont).
#[must_use]
pub fn sysconfdir() -> PathBuf {
    match std::env::var_os("DILLO_SYSCONF") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => PathBuf::from("/etc/dillo/"),
    }
}

/// `~/.dillo` — chemin du répertoire utilisateur (verbatim
/// `dStrconcat(dGethomedir(), "/.dillo")`).
#[must_use]
pub fn dillo_dir() -> PathBuf {
    let home = dillo_common::dlib::get_home_dir();
    PathBuf::from(String::from_utf8_lossy(&home).into_owned()).join(".dillo")
}

/// `Paths::init` — mémorise le cwd courant (≙ `dGetcwd`), bascule
/// sur le répertoire temporaire (`/tmp` POSIX ≙ `%TEMP%`), crée
/// `~/.dillo` si absent (mode 0700 ≙ `mkdir(path, 0700)` — les
/// bits Unix ne s'appliquent que sous `cfg(unix)`).
pub fn init() {
    let cwd = std::env::current_dir().unwrap_or_default();
    OLD_WORKING_DIR.with(|d| *d.borrow_mut() = Some(cwd.into_os_string().into_encoded_bytes()));

    // verbatim : « Dillo works from an unmounted directory (/tmp) ».
    let tmp = std::env::temp_dir();
    if std::env::set_current_dir(&tmp).is_err() {
        tracing::warn!(
            "paths: Error changing directory to {}: {}",
            tmp.display(),
            io::Error::last_os_error()
        );
    }

    let path = dillo_dir();
    match std::fs::metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            tracing::info!("paths: Creating directory '{}/'", path.display());
            if let Err(e) = create_private_dir(&path) {
                tracing::warn!("paths: Error creating directory {}: {}", path.display(), e);
            }
        }
        Err(e) => {
            tracing::warn!("OnionBit-Browser: error reading {}: {}", path.display(), e);
        }
        Ok(_) => {}
    }
}

#[cfg(unix)]
fn create_private_dir(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .or_else(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                Ok(())
            } else {
                Err(e)
            }
        })
}

#[cfg(not(unix))]
fn create_private_dir(path: &std::path::Path) -> io::Result<()> {
    // Windows : pas de bits POSIX — répertoire du profil
    // utilisateur déjà privé ≙ 0700 effectif.
    std::fs::create_dir(path).or_else(|e| {
        if e.kind() == io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(e)
        }
    })
}

/// `Paths::getOldWorkingDir` — cwd mémorisé par [`init`]
/// (`NULL` amont ≙ `None` si `init` n'a pas tourné).
#[must_use]
pub fn old_working_dir() -> Option<Vec<u8>> {
    OLD_WORKING_DIR.with(|d| d.borrow().clone())
}

/// `Paths::free` — libère le tampon (verbatim `dFree`).
pub fn free() {
    OLD_WORKING_DIR.with(|d| *d.borrow_mut() = None);
}

/// `Paths::getPrefsFP` — `~/.dillo/<rc>` sinon `sysconfdir()/<rc>` ;
/// `None` ≙ `NULL` (les MSG amont deviennent `warn!`/`info!`).
#[must_use]
pub fn prefs_fp(rc_file: &str) -> Option<io::BufReader<std::fs::File>> {
    let user = dillo_dir().join(rc_file);
    match std::fs::File::open(&user) {
        Ok(f) => Some(io::BufReader::new(f)),
        Err(e) => {
            tracing::warn!("paths: Cannot open file '{}': {}", user.display(), e);
            let sys = sysconfdir().join(rc_file);
            match std::fs::File::open(&sys) {
                Ok(f) => {
                    tracing::info!("paths: Using {}", sys.display());
                    Some(io::BufReader::new(f))
                }
                Err(e) => {
                    tracing::warn!("paths: Cannot open file '{}': {}", sys.display(), e);
                    tracing::warn!("paths: Using internal defaults...");
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dillo_dir_under_home() {
        let d = dillo_dir();
        assert!(d.ends_with(".dillo") || d.ends_with(".dillo/"));
    }

    #[test]
    fn sysconfdir_env_override() {
        // verbatim : fallback /etc/dillo sans env (non destructif —
        // on vérifie juste la branche par défaut si env absente).
        if std::env::var_os("DILLO_SYSCONF").is_none() {
            assert_eq!(sysconfdir(), PathBuf::from("/etc/dillo/"));
        }
    }

    #[test]
    fn old_working_dir_roundtrip() {
        let cwd = std::env::current_dir().unwrap_or_default();
        // init() change le cwd — sauvegarde/restore autour.
        init();
        assert_eq!(
            old_working_dir().as_deref(),
            Some(cwd.as_os_str().as_encoded_bytes())
        );
        free();
        assert!(old_working_dir().is_none());
        let _ = std::env::set_current_dir(&cwd);
    }

    #[test]
    fn prefs_fp_none_for_missing() {
        // Un nom improbable → double échec → None (verbatim).
        assert!(prefs_fp("__dillorc_inexistant_42__").is_none());
    }
}
