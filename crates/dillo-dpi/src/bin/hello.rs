//! Port of `dpi/hello.c` — plugin « Hello World » (exemple d'interop).
//!
//! Verbatim : lit le tag `auth` sur stdio (socket héritée du dpid),
//! vérifie la clé, demande confirmation par `dialog`, envoie la page
//! `start_send_page` + HTML, et affiche la sortie de `date -R`.
//!
//! POSIX uniquement (stdio = socket, ≙ `dup2` amont) — le binaire
//! existe aussi hors Unix mais quitte immédiatement.

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
fn main() {
    use dillo_dpi::dpip::{check_auth, DpIoSock, Dsh};
    use dillo_dpi::dpiutil::escape_html_str;
    use std::io::Read;

    // verbatim : STDIN_FILENO/STDOUT_FILENO — le socket accepté par
    // dpid est dup2'né sur 0/1 avant l'exec du plugin.
    let rd = DpIoSock::stdio(0, false).expect("fd0 — dup2 dpi amont");
    let wr = DpIoSock::stdio(1, false).expect("fd1 — dup2 dpi amont");
    let mut sh = Dsh::new(Box::new(rd), Box::new(wr), 2 * 1024);

    // Authentification (clé partagée ~/.dillo/dpid_comm_keys).
    let tag = sh.read_token(true);
    let ok = tag
        .as_deref()
        .is_some_and(|t| check_auth(&String::from_utf8_lossy(t)));
    if !ok {
        eprintln!("[hello dpi]: can't authenticate request");
        sh.close();
        std::process::exit(1);
    }

    // Tag de commande.
    let Some(tag2) = sh.read_token(true) else {
        sh.close();
        std::process::exit(1);
    };
    let tag2s = String::from_utf8_lossy(&tag2).into_owned();
    let cmd = dillo_net::dpip::get_attr(&tag2s, "cmd").unwrap_or_default();
    let url = dillo_net::dpip::get_attr(&tag2s, "url").unwrap_or_default();

    // ---- dialog (verbatim : exemple d'interaction utilisateur) ----
    let choice = [
        "Window was closed",
        "Yes",
        "No",
        "Could be",
        "It's OK",
        "Cancel",
    ];
    let mut choice_num: i32 = -1;
    if let Some(d_cmd) = dillo_net::dpip::build_cmd(
        "cmd=%s title=%s msg=%s alt1=%s alt2=%s alt3=%s alt4=%s alt5=%s",
        &[
            "dialog",
            "OnionBit-Browser: Hello",
            "Do you want to see the hello page?",
            choice[1],
            choice[2],
            choice[3],
            choice[4],
            choice[5],
        ],
    ) {
        let _ = sh.write_str(true, &d_cmd);
        if let Some(ans) = sh.read_token(true) {
            let anss = String::from_utf8_lossy(&ans).into_owned();
            if let Some(msg) = dillo_net::dpip::get_attr(&anss, "msg") {
                // verbatim : `*dialog_msg - '0'` — premier octet.
                choice_num = msg.bytes().next().map_or(0, |c| i32::from(c) - 48);
            } else {
                choice_num = 0;
            }
        }
    }

    // ---- réponse : start_send_page puis le HTML ----
    if let Some(d_cmd) = dillo_net::dpip::build_cmd("cmd=%s url=%s", &["start_send_page", &url]) {
        let _ = sh.write_str(false, &d_cmd);
    }

    let _ = sh.write(
        false,
        b"Content-type: text/html\n\n\
          <!DOCTYPE HTML PUBLIC '-//W3C//DTD HTML 4.01 Transitional//EN'>\n\
          <html>\n\
          <head><title>Simple dpi test page (hello.dpi)</title></head>\n\
          <body><hr><h1>Hello world!</h1><hr>\n<br><br>\n",
    );

    let answer = if choice_num < 0 {
        "There was NO dialog!".to_string()
    } else {
        choice
            .get(usize::try_from(choice_num).unwrap_or(0))
            .unwrap_or(&"There was NO dialog!")
            .to_string()
    };
    let _ = sh.write(
        false,
        format!(
            "<hr>\n\
             <table width='100%' border='1' bgcolor='burlywood'><tr><td>\n\
             <big><em>Dialog question:</em> Do you want to see the hello page?<br>\n\
             <em>Answer received:</em> <b>{}</b></big> </table>\n<hr>\n",
            answer
        )
        .as_bytes(),
    );

    // tag reçu, échappé HTML.
    let esc = escape_html_str(&tag2);
    let mut body = b"<h3>dpip tag received:</h3>\n<pre>\n".to_vec();
    body.extend_from_slice(&esc);
    body.extend_from_slice(b"</pre>\n<br><small>(<b>dpip:</b> dpi protocol)</small><br><br><br>\n");
    let _ = sh.write(false, &body);

    // verbatim : `popen("date -R")` — flux d'un programme externe.
    if !cmd.is_empty() && !url.is_empty() {
        if let Ok(mut out) = std::process::Command::new("date")
            .arg("-R")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                let mut buf = Vec::new();
                if let Some(mut so) = c.stdout.take() {
                    let _ = so.read_to_end(&mut buf);
                }
                let _ = c.wait();
                Ok(buf)
            })
        {
            let _ = sh.write_str(false, "<h3>date:</h3>\n<pre>\n");
            let _ = sh.write(false, &out);
            let _ = sh.write_str(false, "</pre>\n");
        }
    }

    let _ = sh.write_str(true, "</body></html>\n");
    sh.close();
}

/// Hors POSIX : les plugins dpi utilisent `dup2(sock,0/1)` — pas de
/// socket stdio sous Windows (verbatim : amont est POSIX-only ici).
#[cfg(not(unix))]
fn main() {
    eprintln!("[hello dpi]: socket stdio unsupported on this platform");
    std::process::exit(1);
}
