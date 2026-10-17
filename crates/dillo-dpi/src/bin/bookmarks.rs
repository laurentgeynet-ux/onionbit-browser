//! Port of `dpi/bookmarks.c` `main`/`Bmsrv_parse_token` — serveur
//! `bookmarks.dpi` (`dpi:/bm/`).
//!
//! Verbatim : `stdin` = socket d'écoute ; accept séquentiel, un
//! client à la fois. Commandes : `chat` (poignée de main verbatim),
//! `add_bookmark`, `open_url` (`dpi:/bm/`→listing, `…/modify`→forms,
//! `…/modify?…`→ops). `DpiBye` → exit(0).
//!
//! Le store est [`dillo_dpi::bookmarks::BmStore`] ; l'import de
//! l'ancien `bookmarks.html` (`Bms_check_import` = grep/sed amont)
//! est fait ici en Rust natif (même extraction, sans shell).

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
use dillo_dpi::bookmarks::{self, BmStore};
use dillo_dpi::dpip::Dsh;
#[cfg(unix)]
use dillo_dpi::dpip::{check_auth, DpIoTcp};

/// `Header` verbatim.
const HEADER: &str = "Content-type: text/html\n\n";

/// `Bmsrv_parse_token` verbatim — retourne 0 = continue, 1 = erreur
/// fatale, 2 = fin de connexion (break), 3 = `DpiBye` → exit(0).
fn parse_token(sh: &mut Dsh<'_>, st: &mut BmStore, buf: &str) -> i32 {
    let Some(cmd) = dillo_net::dpip::get_attr(buf, "cmd") else {
        return 0;
    };
    // verbatim : « chat » hand-shake (msg → réponse par initiale).
    if cmd == "chat" {
        if let Some(msg) = dillo_net::dpip::get_attr(buf, "msg") {
            let reply = match msg.chars().next() {
                Some('H') => "Hi browser",
                Some('I') => "Is it worth?",
                Some('S') => "Ok, send it",
                _ => "",
            };
            if !reply.is_empty() {
                if let Some(d) = dillo_net::dpip::build_cmd("cmd=%s msg=%s", &["chat", reply]) {
                    let _ = sh.write_str(true, &d);
                }
            }
        }
        return 0;
    }
    // verbatim `Bms_cond_load` — sync si le fichier a changé.
    st.cond_load();
    match cmd.as_str() {
        "DpiBye" => 3,
        "add_bookmark" => {
            let url = dillo_net::dpip::get_attr(buf, "url").unwrap_or_default();
            let mut title = dillo_net::dpip::get_attr(buf, "title").unwrap_or_default();
            if title.is_empty() {
                title = "(Untitled)".into();
            }
            if !url.is_empty() {
                st.add_bm(&url, &title);
            }
            if let Some(d) = dillo_net::dpip::build_cmd(
                "cmd=%s msg=%s",
                &["send_status_message", "Added bookmark!"],
            ) {
                let _ = sh.write_str(true, &d);
            }
            2
        }
        "open_url" => {
            let url = dillo_net::dpip::get_attr(buf, "url").unwrap_or_default();
            if url.len() >= 4 && url[..4].eq_ignore_ascii_case("dpi:") {
                let rest = &url[4..];
                if rest == "/bm/modify" {
                    return send_modify_answer(sh, st, &url);
                } else if rest.starts_with("/bm/modify?") {
                    return process_modify_request(sh, st, &url);
                }
            }
            if let Some(d) = dillo_net::dpip::build_cmd("cmd=%s url=%s", &["start_send_page", &url])
            {
                let _ = sh.write_str(true, &d);
            }
            let _ = sh.write_str(true, HEADER);
            let page = bookmarks::bm_page(st);
            if sh.write_str(true, &page) < 0 {
                let _ = sh.write_str(
                    true,
                    "<!DOCTYPE html>\n<HTML><body id='dillo_bm'> \
                     Error on the bookmarks server...      </body></html>",
                );
            }
            2
        }
        _ => 0,
    }
}

/// `Bmsrv_send_modify_answer` verbatim : `start_send_page` + header
/// + page selon `MODIFY_PAGE_NUM`.
fn send_modify_answer(sh: &mut Dsh<'_>, st: &mut BmStore, url: &str) -> i32 {
    if let Some(d) = dillo_net::dpip::build_cmd("cmd=%s url=%s", &["start_send_page", url]) {
        if sh.write_str(true, &d) != 0 {
            return 1;
        }
    }
    if sh.write_str(false, HEADER) != 0 {
        return 1;
    }
    let body = match st.modify_page_num {
        2 => {
            st.modify_page_num = 1;
            bookmarks::add_section_page()
        }
        3 => {
            st.modify_page_num = 1;
            st.send_modify_update(None).unwrap_or_default()
        }
        4 => {
            st.modify_page_num = 1;
            bookmarks::add_url_page()
        }
        _ => bookmarks::modify_page(st),
    };
    let _ = sh.write_str(true, &body);
    2
}

/// `Bmsrv_send_reload_request` verbatim.
fn send_reload(sh: &mut Dsh<'_>, url: &str) -> i32 {
    if let Some(d) = dillo_net::dpip::build_cmd("cmd=%s url=%s", &["reload_request", url]) {
        let _ = sh.write_str(true, &d);
    }
    1
}

/// `Bmsrv_process_modify_request` verbatim — dispatch `operation=`.
fn process_modify_request(sh: &mut Dsh<'_>, st: &mut BmStore, url: &str) -> i32 {
    if let Err(msg) = st.check_modify_request(url) {
        if let Some(d) = dillo_net::dpip::build_cmd("cmd=%s msg=%s", &["send_status_message", msg])
        {
            let _ = sh.write_str(true, &d);
        }
        return 2;
    }
    if url.contains("operation=delete&") {
        st.modify_delete(url);
        return send_reload(sh, "dpi:/bm/modify");
    }
    if url.contains("operation=move&") {
        if st.modify_move(url) == 1 {
            return 1;
        }
        return send_reload(sh, "dpi:/bm/modify");
    }
    if url.contains("operation=modify&") {
        st.send_modify_update(Some(url));
        st.modify_page_num = 3;
        return send_reload(sh, "dpi:/bm/modify");
    }
    if url.contains("operation=modify2&") {
        st.modify_update(url);
        return send_reload(sh, "dpi:/bm/modify");
    }
    if url.contains("operation=add_sec&") {
        st.modify_page_num = 2;
        return send_reload(sh, "dpi:/bm/modify");
    }
    if url.contains("operation=add_section&") {
        if st.modify_add_section(url) == 1 {
            return 1;
        }
        return send_reload(sh, "dpi:/bm/modify");
    }
    if url.contains("operation=add_url&") {
        st.modify_page_num = 4;
        st.save_target_section(url);
        return send_reload(sh, "dpi:/bm/modify");
    }
    if url.contains("operation=add_url2&") {
        if st.modify_add_url(url) == 1 {
            return 1;
        }
        return send_reload(sh, "dpi:/bm/modify");
    }
    2
}

/// `Bms_check_import` verbatim : si `bm.txt` absent et l'ancien
/// `~/.dillo/bookmarks.html` existe → import `s0 url title` des
/// `<li><A HREF="…">…<` (amont le fait via grep+sed ; ici en Rust
/// natif — même extraction).
#[cfg(unix)]
fn check_import(st: &BmStore) {
    if st.bm_file.exists() {
        return;
    }
    let home = dillo_common::dlib::get_home_dir();
    let home = String::from_utf8_lossy(&home).into_owned();
    let old = std::path::Path::new(&home).join(".dillo/bookmarks.html");
    let Ok(old_txt) = std::fs::read_to_string(&old) else {
        return;
    };
    let mut out = String::from(":s0: Unclassified\n");
    for line in old_txt.lines() {
        let Some(p) = line.to_ascii_uppercase().find("<A HREF=\"") else {
            continue;
        };
        let rest = &line[p + 9..];
        let Some(q) = rest.find('"') else { continue };
        let url = &rest[..q];
        let rest = &rest[q..];
        let Some(gt) = rest.find('>') else { continue };
        let title = &rest[gt + 1..];
        let title = title.find('<').map_or(title, |e| &title[..e]);
        out.push_str(&format!("s0 {url} {title}\n"));
    }
    let _ = std::fs::write(&st.bm_file, out);
}

#[cfg(unix)]
fn run() -> i32 {
    let quit = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    for sig in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGTERM,
    ] {
        let _ = signal_hook::flag::register(sig, quit.clone());
    }
    // verbatim : SIGPIPE ignoré (write sur socket morte → EPIPE).
    let _ = signal_hook::flag::register(
        signal_hook::consts::SIGPIPE,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );

    let home = dillo_common::dlib::get_home_dir();
    let home = String::from_utf8_lossy(&home).into_owned();
    let mut st = BmStore::new(std::path::Path::new(&home).join(".dillo/bm.txt"));
    check_import(&st);
    st.cond_load();

    let stdin = socket2::SockRef::from(&std::io::stdin());
    loop {
        if quit.load(std::sync::atomic::Ordering::Relaxed) {
            return 0;
        }
        let Ok((sock, _)) = stdin.accept() else {
            eprintln!("[bookmarks dpi]: accept failed");
            return 1;
        };
        let stream: std::net::TcpStream = sock.into();
        let Ok(rd) = stream.try_clone() else { continue };
        let mut sh = Dsh::new(
            Box::new(DpIoTcp::new(rd, false)),
            Box::new(DpIoTcp::new(stream, false)),
            8 * 1024,
        );
        // verbatim : auth obligatoire — échec → exit(1).
        let ok = sh
            .read_token(true)
            .map(|t| check_auth(&String::from_utf8_lossy(&t)))
            .unwrap_or(false);
        if !ok {
            eprintln!("[bookmarks dpi]: can't authenticate request");
            sh.close();
            return 1;
        }
        loop {
            let Some(tag) = sh.read_token(true) else {
                break;
            };
            let tag = String::from_utf8_lossy(&tag).into_owned();
            let code = parse_token(&mut sh, &mut st, &tag);
            if code == 3 {
                return 0; // DpiBye → exit(0) verbatim
            }
            if code != 0 {
                break; // verbatim : socket non opérative → client suivant
            }
        }
        sh.close();
    }
}

#[cfg(unix)]
fn main() {
    std::process::exit(run());
}

#[cfg(not(unix))]
fn main() {
    eprintln!("[bookmarks dpi]: socket stdio unsupported on this platform");
    std::process::exit(1);
}
