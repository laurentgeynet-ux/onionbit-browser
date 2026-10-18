//! Port of `dpi/cookies.c` `main`/`srv_parse_tok` — démon serveur
//! `cookies.dpi`.
//!
//! Verbatim : `stdin` = socket d'écoute ; **séquentiel** (amont sert
//! un client à la fois, `accept` bloquant). Commandes :
//! `set_cookie`/`get_cookie` → `*_answer`, `DpiBye` → exit(0).
//! `atexit(cleanup)` ≙ sauvegarde `cookies.txt` à la sortie.
//!
//! Le moteur est [`dillo_dpi::cookies::CookieJar`] ; l'I/O fichier
//! (`Cookies_fopen`, lock POSIX) est gérée ici.

#![cfg_attr(not(unix), allow(dead_code))]
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
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};

/// `srv_parse_tok` verbatim — retourne 1=exit, 0=auth ok, 2=close.
fn srv_parse_tok(
    sh: &mut dillo_dpi::dpip::Dsh<'_>,
    authed: &mut bool,
    jar: &mut dillo_dpi::cookies::CookieJar,
    buf: &str,
) -> i32 {
    let Some(cmd) = dillo_net::dpip::get_attr(buf, "cmd") else {
        return 1; // abort verbatim
    };
    if !*authed {
        // verbatim : premier tag = auth (`a_Dpip_check_auth`).
        if dillo_dpi::dpip::check_auth(buf) {
            *authed = true;
            return 0;
        }
        return 1;
    }
    match cmd.as_str() {
        "DpiBye" => 3, // verbatim exit(0) — remonté à main
        "set_cookie" => {
            let cookie = dillo_net::dpip::get_attr(buf, "cookie").unwrap_or_default();
            let host = dillo_net::dpip::get_attr(buf, "host").unwrap_or_default();
            let path = dillo_net::dpip::get_attr(buf, "path").unwrap_or_default();
            let date = dillo_net::dpip::get_attr(buf, "date");
            let st = jar.set(&cookie, &host, &path, date.as_deref());
            if let Some(ans) = dillo_net::dpip::build_cmd(
                "cmd=%s msg=%s",
                &["set_cookie_answer", if st == 0 { "ok" } else { "not set" }],
            ) {
                let _ = sh.write_str(true, &ans);
            }
            2
        }
        "get_cookie" => {
            let scheme = dillo_net::dpip::get_attr(buf, "scheme").unwrap_or_default();
            let host = dillo_net::dpip::get_attr(buf, "host").unwrap_or_default();
            let path = dillo_net::dpip::get_attr(buf, "path").unwrap_or_default();
            let cookie = jar.get(&host, &path, &scheme);
            // verbatim : write_str ≠ 0 → ret 1 → exit(1) (dpid
            // relancera le démon à la prochaine requête).
            if let Some(ans) =
                dillo_net::dpip::build_cmd("cmd=%s cookie=%s", &["get_cookie_answer", &cookie])
            {
                if sh.write_str(true, &ans) != 0 {
                    return 1;
                }
            }
            2
        }
        _ => 1, // verbatim : cmd non reconnu → exit(1)
    }
}

/// `Cookies_fopen` verbatim : ouvre ou crée le fichier avec
/// `init_str` si absent.
fn cookies_fopen(path: &std::path::Path, init: Option<&str>) -> Option<std::fs::File> {
    use std::io::Write;
    if let Ok(f) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
    {
        return Some(f);
    }
    // verbatim `open(O_WRONLY|O_CREAT|O_TRUNC, S_IRUSR|S_IWUSR)` +
    // `FD_CLOEXEC` (défaut Rust sur unix).
    let mut oo = std::fs::OpenOptions::new();
    oo.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        oo.mode(0o600);
    }
    let mut f = oo.open(path).ok()?;
    if let Some(init) = init {
        let _ = f.write_all(init.as_bytes());
    }
    drop(f);
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .ok()
}

#[cfg(unix)]
fn run() -> i32 {
    use dillo_dpi::dpip::{DpIoTcp, Dsh};
    use std::io::Read;

    // verbatim : signaux → exit (atexit → save). Flag consulté dans
    // la boucle ; la sauvegarde se fait avant exit.
    let quit = std::sync::Arc::new(AtomicBool::new(false));
    for sig in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGTERM,
    ] {
        let _ = signal_hook::flag::register(sig, quit.clone());
    }

    let home = dillo_common::dlib::get_home_dir();
    let home = String::from_utf8_lossy(&home).into_owned();
    let mut jar = dillo_dpi::cookies::CookieJar::new();

    // `Cookie_control_init` verbatim : lit ~/.dillo/cookiesrc
    // (créé « DEFAULT DENY\n » si absent) ; disabled si tout DENY.
    let rc_path = std::path::PathBuf::from(&home).join(".dillo/cookiesrc");
    let mut rc_content = String::new();
    let enabled = match cookies_fopen(&rc_path, Some("DEFAULT DENY\n")) {
        Some(mut f) => {
            let _ = f.read_to_string(&mut rc_content);
            jar.control_init(&rc_content)
        }
        None => 2,
    };
    // verbatim `Cookies_init` : `Cookie_control_init() != 0`
    // (DENY-only ou pas de cookiesrc) → `disabled` → `exit(1)`
    // **avant** d'ouvrir/créer cookies.txt.
    if enabled != 0 {
        eprintln!("[cookies dpi]: disabled (cookiesrc DENY-only or unreadable)");
        return 1;
    }

    // `Cookies_init` verbatim : cookies.txt (créé avec l'en-tête),
    // lock POSIX ≙ `fcntl(F_SETLK)` — std n'a pas de lock safe ;
    // déviation documentée : on ouvre sans lock (le risque est deux
    // instances cookies.dpi — dpid n'en lance qu'une).
    let txt_path = std::path::PathBuf::from(&home).join(".dillo/cookies.txt");
    let mut txt = cookies_fopen(
        &txt_path,
        Some(
            "# HTTP Cookie File\n# This is a generated file!  Do not edit.\n\
              # [domain  subdomains  path  secure  expiry_time  name  value]\n\n",
        ),
    );
    // verbatim : échec d'ouverture → disabled → exit(1).
    let Some(txt_file) = txt.as_mut() else {
        eprintln!("[cookies dpi]: can't open ~/.dillo/cookies.txt; disabling cookies");
        return 1;
    };
    jar.disabled = false;
    let mut contents = String::new();
    let _ = txt_file.read_to_string(&mut contents);
    jar.load(&contents);

    // verbatim : accept bloquant + clients séquentiels.
    let stdin_io = std::io::stdin();
    let stdin = socket2::SockRef::from(&stdin_io);
    loop {
        if quit.load(Ordering::Relaxed) {
            break;
        }
        let Ok((sock, _)) = stdin.accept() else {
            eprintln!("[cookies dpi]: accept failed");
            return 1;
        };
        let stream: std::net::TcpStream = sock.into();
        let Ok(rd) = stream.try_clone() else { continue };
        let mut sh = Dsh::new(
            Box::new(DpIoTcp::new(rd, false)),
            Box::new(DpIoTcp::new(stream, false)),
            8 * 1024,
        );
        let mut authed = false;
        // verbatim : EOF → exit(1) — mais amont boucle sur
        // code=1 → exit(1). Ici : client suivant si EOF sans
        // erreur, sinon sortie.
        while let Some(tag) = sh.read_token(true) {
            let tag = String::from_utf8_lossy(&tag).into_owned();
            let code = srv_parse_tok(&mut sh, &mut authed, &mut jar, &tag);
            if code == 1 {
                // verbatim exit(1) → atexit cleanup.
                save(&mut jar, txt.as_mut());
                return 1;
            }
            if code == 3 {
                // DpiBye → exit(0) verbatim.
                save(&mut jar, txt.as_mut());
                return 0;
            }
            if code == 2 {
                break;
            }
        }
        sh.close();
    }
    // verbatim atexit(cleanup) → Cookies_save_and_free.
    save(&mut jar, txt.as_mut());
    0
}

/// `Cookies_save_and_free` — réécrit `cookies.txt` (truncate +
/// en-tête + lignes).
#[cfg(unix)]
fn save(jar: &mut dillo_dpi::cookies::CookieJar, f: Option<&mut std::fs::File>) {
    use std::io::{Seek, SeekFrom, Write};
    let Some(f) = f else { return };
    let out = jar.save();
    let _ = f.set_len(0);
    let _ = f.seek(SeekFrom::Start(0));
    let _ = f.write_all(out.as_bytes());
}

#[cfg(unix)]
fn main() {
    std::process::exit(run());
}

#[cfg(not(unix))]
fn main() {
    eprintln!("[cookies dpi]: socket stdio unsupported on this platform");
    std::process::exit(1);
}
