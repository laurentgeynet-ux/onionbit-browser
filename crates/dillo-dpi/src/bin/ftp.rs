//! Port of `dpi/ftp.c` — plugin filtre `ftp:`.
//!
//! Délègue le transfert à `wget -t1 -O- <url>` (verbatim), sniffe le
//! type MIME sur les 256 premiers octets, envoie `start_send_page` +
//! en-tête + données. `application/octet-stream` → wget tué
//! (verbatim : Dillo ouvre alors le dialogue de téléchargement).

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
/// `a_Misc_get_content_type_from_data2` verbatim — retourne
/// `(type, douteux)` : `douteux=false` ≙ `st=0` amont.
fn content_type_from_data(data: &[u8]) -> (&'static str, bool) {
    let mut i = 0;
    while i < data.len() && (data[i] as char).is_whitespace() {
        i += 1;
    }
    let rest = &data[i..];
    let starts = |pat: &str| {
        rest.len() >= pat.len() && rest[..pat.len()].eq_ignore_ascii_case(pat.as_bytes())
    };
    if starts("<html")
        || starts("<head")
        || starts("<title")
        || starts("<!doctype html")
        || starts("<!-- HTML listing")
    {
        ("text/html", false)
    } else if data.len() >= 4 && &data[..4] == b"GIF8" {
        ("image/gif", false)
    } else if data.len() >= 4 && &data[..4] == b"\x89PNG" {
        ("image/png", false)
    } else if data.len() >= 2 && &data[..2] == b"\xff\xd8" {
        ("image/jpeg", false)
    } else {
        // verbatim : « >10 % » du caractères hors [32..126]/isspace
        // sur l'échantillon (≤256) → binaire.
        let sample = &data[..data.len().min(256)];
        let non_ascii = sample
            .iter()
            .filter(|&&c| !(32..=126).contains(&c) && !(c as char).is_whitespace())
            .count();
        let ty = if non_ascii > sample.len() / 10 {
            "application/octet-stream"
        } else {
            "text/plain"
        };
        // `st = 0` seulement si échantillon complet (verbatim).
        (ty, sample.len() != 256)
    }
}

/// `make_wget_argv` verbatim : `wget -t1 -O- <url-escapée>` avec
/// `Filter_smtp_hack` anti-relais SMTP.
fn make_wget_argv(url: &str) -> Vec<String> {
    // verbatim `Escape_uri_str(url, "'")` + `Filter_smtp_hack`.
    let esc = dillo_dpi::dpiutil::escape_uri_str(url.as_bytes(), Some(b"'"));
    let esc = String::from_utf8_lossy(&dillo_dpi::dpiutil::filter_smtp_hack(&esc)).into_owned();
    vec!["wget".into(), "-t1".into(), "-O-".into(), esc]
}

/// `try_ftp_transfer` verbatim — retourne le nombre d'octets, `-1` si
/// fichier inexistant (0 octet), `-2` si aborté (octet-stream).
#[cfg(unix)]
fn try_ftp_transfer(sh: &mut dillo_dpi::dpip::Dsh<'_>, url: &str) -> i64 {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    const MIN_SZ: usize = 256;
    const READ_SZ: usize = 16 * 1024;

    let argv = make_wget_argv(url);
    // verbatim fork + dup2(pipe,1) + execvp — via Command stdout pipé.
    let mut child = match Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return 0, // verbatim pipe() fail → 0
    };
    let mut stdout = match child.stdout.take() {
        Some(s) => s,
        None => return 0,
    };

    let mut nb: i64 = 0;
    let mut mime_type = "application/octet-stream";
    let mut dbuf: Vec<u8> = Vec::with_capacity(READ_SZ);
    let mut has_mime_type = false;
    let mut has_html_header = false;
    let mut aborted = false;
    let mut no_such_file = false;
    let mut offer_download = false;
    let mut buf = [0u8; READ_SZ];

    loop {
        let n = match stdout.read(&mut buf) {
            Ok(n) => n as i64,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => -1,
        };
        if n > 0 {
            dbuf.extend_from_slice(&buf[..n as usize]);
            if !has_mime_type && dbuf.len() < MIN_SZ {
                continue;
            }
        } else if n < 0 {
            break;
        }
        if !has_mime_type {
            if dbuf.is_empty() {
                // verbatim : transfert vide ≙ fichier inexistant.
                no_such_file = true;
                break;
            }
            let (mt, _doubt) = content_type_from_data(&dbuf);
            mime_type = mt;
            has_mime_type = true;
            if mt == "application/octet-stream" {
                // verbatim kill(ch_pid, SIGTERM) + offer_download.
                let _ = unsafe_kill(&mut child);
                offer_download = true;
                aborted = true;
            }
        }
        if offer_download || (!aborted && !has_html_header && !dbuf.is_empty()) {
            if let Some(d) = dillo_net::dpip::build_cmd("cmd=%s url=%s", &["start_send_page", url])
            {
                let _ = sh.write_str(true, &d);
            }
            let _ = sh.write_str(false, "Content-type: ");
            let _ = sh.write_str(false, mime_type);
            let _ = sh.write_str(true, "\r\n\r\n");
            has_html_header = true;
        }
        if !aborted && !dbuf.is_empty() {
            let _ = sh.write(true, &dbuf);
            nb += dbuf.len() as i64;
            dbuf.clear();
        }
        if n <= 0 || aborted {
            break;
        }
    }
    let _ = child.wait();
    if no_such_file {
        -1
    } else if aborted {
        -2
    } else {
        nb
    }
}

/// `kill(pid, SIGTERM)` verbatim — `Child::kill` envoie SIGKILL ;
/// SIGTERM via `kill` n'est pas exposé safe → on se contente du kill
/// std (effet observable identique : wget s'arrête).
#[cfg(unix)]
fn unsafe_kill(child: &mut std::process::Child) -> std::io::Result<()> {
    child.kill()
}

#[cfg(unix)]
fn run() -> i32 {
    use dillo_dpi::dpip::{check_auth, DpIoSock, Dsh};

    // verbatim `chdir("/tmp")` (wget écrit des fichiers temporaires).
    let _ = std::env::set_current_dir("/tmp");

    let rd = DpIoSock::stdio(0, false).expect("fd0 — dup2 dpi amont");
    let wr = DpIoSock::stdio(1, false).expect("fd1 — dup2 dpi amont");
    let mut sh = Dsh::new(Box::new(rd), Box::new(wr), 8 * 1024);

    let args: Vec<String> = std::env::args().collect();
    let tag = if args.len() == 2 {
        // verbatim : `argc == 2` → tag dpip passé en ligne de commande
        // (debug du plugin sans dpid).
        args[1].clone().into_bytes()
    } else {
        let Some(t) = sh.read_token(true) else {
            sh.close();
            return 1;
        };
        if !check_auth(&String::from_utf8_lossy(&t)) {
            eprintln!("[ftp dpi]: can't authenticate request");
            sh.close();
            return 1;
        }
        let Some(t2) = sh.read_token(true) else {
            sh.close();
            return 1;
        };
        t2
    };
    let tag = String::from_utf8_lossy(&tag).into_owned();
    let url = match (
        dillo_net::dpip::get_attr(&tag, "cmd"),
        dillo_net::dpip::get_attr(&tag, "url"),
    ) {
        (Some(_), Some(u)) => u,
        _ => return 1,
    };

    let mut st = try_ftp_transfer(&mut sh, &url);
    if st == -1 {
        // verbatim : fichier absent ou symlink vers répertoire —
        // réessaie avec '/' final si possible.
        if let Some(p) = url.rfind('/') {
            if p + 1 < url.len() && p > 0 && url.as_bytes()[p - 1] != b'/' {
                let url2 = format!("{url}/");
                st = try_ftp_transfer(&mut sh, &url2);
            }
        }
    }
    if st == -1 {
        let err_msg = "404 Not Found\nNo such file or directory";
        if let Some(d) = dillo_net::dpip::build_cmd("cmd=%s url=%s", &["start_send_page", &url]) {
            let _ = sh.write_str(false, &d);
        }
        let _ = sh.write_str(
            true,
            &format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n\
                 Content-Length: {}\r\n\r\n{err_msg}",
                err_msg.len()
            ),
        );
    }
    sh.close();
    0
}

#[cfg(unix)]
fn main() {
    std::process::exit(run());
}

#[cfg(not(unix))]
fn main() {
    eprintln!("[ftp dpi]: wget/pipe socket stdio unsupported on this platform");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff2() {
        assert_eq!(content_type_from_data(b"<html>x").0, "text/html");
        assert_eq!(content_type_from_data(b"GIF8xy").0, "image/gif");
        // échantillon < 256 → douteux verbatim.
        let (_, doubt) = content_type_from_data(b"abc");
        assert!(doubt);
        let full = vec![b'a'; 256];
        assert_eq!(content_type_from_data(&full), ("text/plain", false));
        let bin = vec![0u8; 256];
        assert_eq!(content_type_from_data(&bin).0, "application/octet-stream");
    }

    #[test]
    fn argv() {
        let a = make_wget_argv("ftp://h/p f");
        assert_eq!(&a[..3], &["wget", "-t1", "-O-"]);
        assert!(a[3].starts_with("ftp://h/p"));
    }
}
