//! Port of `dpi/vsource.c` — plugin « view source ».
//!
//! Après `start_send_page` du client, dillo renvoie la page cachée
//! (`data_size` octets) que le plugin retransmet coloriée en HTML
//! (numéros de ligne + échappement `&<`).

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
const DOCTYPE: &str = "<!DOCTYPE HTML PUBLIC '-//W3C//DTD HTML 4.01 Transitional//EN'>";

/// `send_plain_text` (verbatim — non sélectionné par défaut amont).
#[allow(dead_code)]
fn send_plain_text(sh: &mut dillo_dpi::dpip::Dsh, data_size: usize) {
    let _ = sh.write_str(false, "Content-type: text/plain\n\n");
    let mut bytes_read = 0;
    while bytes_read < data_size {
        let Some(tok) = sh.read_token(true) else {
            break;
        };
        bytes_read += tok.len();
        let _ = sh.write(true, &tok);
    }
}

/// `send_numbered_text` (verbatim — variante non utilisée amont).
#[allow(dead_code)]
fn send_numbered_text(sh: &mut dillo_dpi::dpip::Dsh, data_size: usize) {
    let _ = sh.write_str(false, "Content-type: text/plain\n\n");
    let mut bytes_read = 0;
    let mut line = 1u32;
    while bytes_read < data_size {
        let Some(tok) = sh.read_token(true) else {
            break;
        };
        bytes_read += tok.len();
        let mut q = 0usize;
        while q < tok.len() {
            let _ = sh.write_str(false, &format!("{line:2}: "));
            if let Some(rel) = tok[q..].iter().position(|&c| c == b'\r' || c == b'\n') {
                let mut p = q + rel;
                let mut end = p + 1;
                if tok[p] == b'\r' && tok.get(p + 1) == Some(&b'\n') {
                    end = p + 2;
                }
                let _ = sh.write(false, &tok[q..end]);
                line += 1;
                p = end - 1;
                q = p + 1;
            } else {
                let _ = sh.write(true, &tok[q..]);
                break;
            }
        }
    }
}

/// `send_html_text` — verbatim : tableau `td.num`/`td.src`, ancres
/// `L<n>`, échappe `<`/`&` et sauts de ligne CRLF.
fn send_html_text(sh: &mut dillo_dpi::dpip::Dsh, url: &str, data_size: usize) {
    let url = if url[..url.len().min(4)].eq_ignore_ascii_case("dpi:")
        && url.len() >= 14
        && url[4..14] == *"/vsource/:"
    {
        &url[14..]
    } else {
        url
    };
    let _ = sh.write_str(false, "Content-type: text/html\n\n");
    let _ = sh.write_str(false, DOCTYPE);
    let head = format!(
        "\n<html><head>\n\
         <title>Source for {url}</title>\n\
         <style type=\"text/css\">\n  body {{\n    white-space: pre-wrap;\n\
         \x20   font-family: monospace;\n    margin: 0;\n    width: 100%;\n  }}\n\
         \x20 table {{ border:0 }}\n  td.num {{\n    padding-top: 1px;\n\
         \x20   padding-bottom: 1px;\n    padding-left: 0.5em;\n\
         \x20   padding-right: 0.5em;\n    text-align: right;\n\
         \x20   border-right: 1px solid #999999;\n    background-color: #c6c6c6;\n  }}\
         \x20 td.src {{ padding-left:0.25em; }}\n  a {{ color: black; text-decoration:none; }}\n\
         </style>\n</head>\n<body id=\"dillo_vs\">\n\
         <table cellspacing='0' cellpadding='0'>\n"
    );
    let _ = sh.write_str(false, &head);

    let mut bytes_read = 0;
    let mut old_line = 0u32;
    let mut line = 1u32;
    while bytes_read < data_size {
        let Some(tok) = sh.read_token(true) else {
            break;
        };
        bytes_read += tok.len();
        let mut q = 0usize;
        while q < tok.len() {
            if line > old_line {
                let ls = format!(
                    "<tr><td class='num' id='L{line}'><a href='#L{line}'>{line}</a><td class='src'>"
                );
                let _ = sh.write_str(false, &ls);
                old_line = line;
            }
            let p = tok[q..]
                .iter()
                .position(|&c| matches!(c, b'\r' | b'\n' | b'<' | b'&'))
                .map(|r| q + r);
            if let Some(p) = p {
                if tok[p] == b'\r' || tok[p] == b'\n' {
                    let mut end = p + 1;
                    if tok[p] == b'\r' && tok.get(p + 1) == Some(&b'\n') {
                        end = p + 2;
                    }
                    let _ = sh.write(false, &tok[q..end]);
                    line += 1;
                    q = end;
                } else {
                    let _ = sh.write(false, &tok[q..p]);
                    let _ = sh.write_str(false, if tok[p] == b'<' { "&lt;" } else { "&amp;" });
                    q = p + 1;
                }
            } else {
                let _ = sh.write(true, &tok[q..]);
                break;
            }
        }
    }
    let _ = sh.write_str(true, "</table></body></html>");
}

#[cfg(unix)]
fn run() -> i32 {
    use dillo_dpi::dpip::{check_auth, DpIoSock, Dsh};

    let rd = DpIoSock::stdio(0, false).expect("fd0 — dup2 dpi amont");
    let wr = DpIoSock::stdio(1, false).expect("fd1 — dup2 dpi amont");
    let mut sh = Dsh::new(Box::new(rd), Box::new(wr), 2 * 1024);

    let tag = sh.read_token(true);
    let ok = tag
        .as_deref()
        .is_some_and(|t| check_auth(&String::from_utf8_lossy(t)));
    if !ok {
        eprintln!("[vsource dpi]: can't authenticate request");
        sh.close();
        return 1;
    }

    let Some(tag2) = sh.read_token(true) else {
        sh.close();
        return 1;
    };
    let tag2s = String::from_utf8_lossy(&tag2).into_owned();
    let url = dillo_net::dpip::get_attr(&tag2s, "url").unwrap_or_default();

    // verbatim : `start_send_page` sans flush intermédiaire.
    if let Some(d) = dillo_net::dpip::build_cmd("cmd=%s url=%s", &["start_send_page", &url]) {
        let _ = sh.write_str(false, &d);
    }

    // Réponse de dillo : `start_send_page` + `data_size`.
    let Some(tag3) = sh.read_token(true) else {
        sh.close();
        return 0;
    };
    let tag3s = String::from_utf8_lossy(&tag3).into_owned();
    if let Some(cmd2) = dillo_net::dpip::get_attr(&tag3s, "cmd") {
        if cmd2 == "start_send_page" {
            if let Some(sz) = dillo_net::dpip::get_attr(&tag3s, "data_size") {
                let data_size = sz.parse::<usize>().unwrap_or(0);
                send_html_text(&mut sh, &url, data_size);
            }
        } else if cmd2 == "DpiError" {
            let _ = sh.write_str(false, "Content-type: text/plain\n\n");
            let _ = sh.write_str(true, "[vsource dpi]: ERROR: Page not cached.\n");
        }
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
    eprintln!("[vsource dpi]: socket stdio unsupported on this platform");
    std::process::exit(1);
}
