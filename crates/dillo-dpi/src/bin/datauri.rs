//! Port of `dpi/datauri.c` — plugin filtre pour les URLs `data:`.
//!
//! Filtre (`.filter.dpi` amont) : lit `cmd`+`url` après auth, décode
//! le corps (`;base64` ou `%XX`), répond `start_send_page` +
//! `Content-type:` puis les octets — ou une page d'erreur HTML.

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
// ---------------------------------------------------------------
// fonctions verbatim (copiées du plugin C — il n'y a pas de lib
// partagée amont ; le binaire est autosuffisant).
// ---------------------------------------------------------------

/// `b64strip_illegal_chars` — retire tout ce qui n'est pas
/// alnum/`+/=`/ASCII.
fn b64strip_illegal_chars(s: &mut Vec<u8>) {
    s.retain(|&c| c.is_ascii() && (c.is_ascii_alphanumeric() || b"+/=".contains(&c)));
}

/// `b64decode` — décodage in-place verbatim (machine à phases 0..3,
/// la table à 256 entrées de l'amont).
fn b64decode(s: &[u8]) -> Vec<u8> {
    // verbatim strict : la table amont met -1 pour '=' aussi — donc
    // '=' est ignoré comme tout caractère illégal (strip + skip).
    let table = |c: u8| -> i32 {
        match c {
            b'+' => 62,
            b'/' => 63,
            b'0'..=b'9' => i32::from(c - b'0') + 52,
            b'A'..=b'Z' => i32::from(c - b'A'),
            b'a'..=b'z' => i32::from(c - b'a') + 26,
            _ => -1,
        }
    };
    let mut out = Vec::with_capacity(s.len());
    let mut dlast = 0i32;
    let mut phase = 0;
    for &cur in s {
        let d = table(cur);
        if d != -1 {
            match phase {
                0 => phase = 1,
                1 => {
                    out.push(((dlast << 2) | ((d & 0x30) >> 4)) as u8);
                    phase = 2;
                }
                2 => {
                    out.push((((dlast & 0xf) << 4) | ((d & 0x3c) >> 2)) as u8);
                    phase = 3;
                }
                _ => {
                    out.push((((dlast & 0x03) << 6) | d) as u8);
                    phase = 0;
                }
            }
            dlast = d;
        }
    }
    out
}

/// `Url_decode_hex_octet` — `-1` si pas deux hexadécimaux.
fn url_decode_hex_octet(s: &[u8]) -> i32 {
    if s.len() >= 2 && s[0].is_ascii_hexdigit() && s[1].is_ascii_hexdigit() {
        let hi = hex_val(s[0]);
        let lo = hex_val(s[1]);
        return i32::from(hi) * 16 + i32::from(lo);
    }
    -1
}

fn hex_val(c: u8) -> u8 {
    if c.is_ascii_digit() {
        c - b'0'
    } else {
        c.to_ascii_uppercase() - b'A' + 10
    }
}

/// `a_Url_decode_hex_str` — `%XX` → octet, reste inchangé.
fn url_decode_hex_str(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' && i + 2 < s.len() {
            let val = url_decode_hex_octet(&s[i + 1..]);
            if val >= 0 {
                out.push(val as u8);
                i += 3;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

/// `datauri_get_mime` — extrait le MIME avant `,`, retire `;base64`,
/// défaut `text/plain;charset=US-ASCII`, `charset…` → `text/plain;…`.
fn datauri_get_mime(url: &str) -> Option<String> {
    if !url[..url.len().min(5)].eq_ignore_ascii_case("data:") {
        return None;
    }
    let rest = &url[5..];
    let comma = rest.find(',');
    // verbatim `p - url < 256` — index absolu → relatif + 5.
    let mut mime = comma
        .filter(|p| *p + 5 < 256)
        .map(|p| rest[..p].to_string());
    let len_ok = mime.is_some();
    if let Some(m) = mime.as_mut() {
        if m.len() >= 7 && m[m.len() - 7..].eq_ignore_ascii_case(";base64") {
            m.truncate(m.len() - 7);
        }
    }
    if !len_ok {
        return None;
    }
    let m = mime.unwrap_or_default();
    Some(if m.is_empty() {
        "text/plain;charset=US-ASCII".to_string()
    } else if m.len() >= 7 && m[..7].eq_ignore_ascii_case("charset") {
        format!("text/plain;{m}")
    } else {
        m
    })
}

/// `datauri_get_data` — `;base64` avant la virgule ≙ `p - url >= 12`
/// et `p-7 == ";base64"`, sinon décodage `%XX`.
fn datauri_get_data(url: &str) -> Vec<u8> {
    let Some(p) = url.find(',') else {
        return Vec::new();
    };
    let is_base64 = p >= 12 && url[p - 7..p].eq_ignore_ascii_case(";base64");
    let data = &url[p + 1..];
    if is_base64 {
        let mut raw = dillo_dpi::dpiutil::unescape_uri_str(data.as_bytes());
        b64strip_illegal_chars(&mut raw);
        b64decode(&raw)
    } else {
        url_decode_hex_str(data.as_bytes())
    }
}

// ---------------------------------------------------------------
// partie processus — POSIX (stdio = socket héritée de dpid).
// ---------------------------------------------------------------

#[cfg(unix)]
fn run() -> i32 {
    use dillo_dpi::dpip::{check_auth, DpIoSock, Dsh};

    let rd = DpIoSock::stdio(0, false).expect("fd0 — dup2 dpi amont");
    let wr = DpIoSock::stdio(1, false).expect("fd1 — dup2 dpi amont");
    let mut sh = Dsh::new(Box::new(rd), Box::new(wr), 8 * 1024);
    // verbatim : `chdir("/tmp")` (silencieux si échec).
    let _ = std::env::set_current_dir("/tmp");

    let tag = sh.read_token(true);
    let ok = tag
        .as_deref()
        .is_some_and(|t| check_auth(&String::from_utf8_lossy(t)));
    if !ok {
        sh.close();
        return 1;
    }

    let Some(tag2) = sh.read_token(true) else {
        sh.close();
        return 1;
    };
    let tag2s = String::from_utf8_lossy(&tag2).into_owned();
    let cmd = dillo_net::dpip::get_attr(&tag2s, "cmd");
    let url = dillo_net::dpip::get_attr(&tag2s, "url");
    let (Some(_cmd), Some(url)) = (cmd, url) else {
        sh.close();
        return 1;
    };

    let mime = datauri_get_mime(&url);
    let data = datauri_get_data(&url);

    if let Some(mt) = mime {
        // `send_decoded_data` verbatim.
        if let Some(d_cmd) = dillo_net::dpip::build_cmd("cmd=%s url=%s", &["start_send_page", &url])
        {
            let _ = sh.write_str(true, &d_cmd);
        }
        let _ = sh.write_str(false, "Content-type: ");
        let _ = sh.write_str(false, &mt);
        let _ = sh.write_str(true, "\n\n");
        let _ = sh.write(false, &data);
    } else {
        // `send_failure_message` verbatim.
        if let Some(d_cmd) = dillo_net::dpip::build_cmd("cmd=%s url=%s", &["start_send_page", &url])
        {
            let _ = sh.write_str(true, &d_cmd);
        }
        let _ = sh.write_str(false, "Content-type: text/html");
        let _ = sh.write_str(true, "\n\n");
        let msg = "<!DOCTYPE HTML PUBLIC '-//W3C//DTD HTML 4.01 Transitional//EN'>\n\
                   <html><body>\n\
                   <hr><h1>Datauri dpi</h1><hr>\n\
                   <p><b>Can't parse datauri:</b><br>\n";
        let _ = sh.write_str(false, msg);
        let dbg = format!(
            "mime_type: {}<br>data size: {}<br>data: {}<br>",
            "null",
            data.len(),
            String::from_utf8_lossy(&data)
        );
        let _ = sh.write_str(false, &dbg);
        let _ = sh.write_str(false, "</body></html>");
    }
    sh.close();
    0
}

#[cfg(unix)]
fn main() {
    std::process::exit(run());
}

/// Hors POSIX : pas de socket sur stdio (verbatim POSIX-only).
#[cfg(not(unix))]
fn main() {
    eprintln!("[datauri dpi]: socket stdio unsupported on this platform");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_default() {
        assert_eq!(
            datauri_get_mime("data:,"),
            Some("text/plain;charset=US-ASCII".into())
        );
        assert_eq!(
            datauri_get_mime("data:text/html,<b>x</b>"),
            Some("text/html".into())
        );
        assert_eq!(
            datauri_get_mime("data:;base64,QUJD"),
            Some("text/plain;charset=US-ASCII".into())
        );
        assert_eq!(
            datauri_get_mime("data:charset=utf-8,x"),
            Some("text/plain;charset=utf-8".into())
        );
        assert_eq!(datauri_get_mime("http://x,y"), None);
    }

    #[test]
    fn data_plain() {
        assert_eq!(datauri_get_data("data:,hello%20world"), b"hello world");
    }

    #[test]
    fn data_base64() {
        // "data:;base64," = 14 > 12 ✓ ; "QUJD" → "ABC"
        assert_eq!(datauri_get_data("data:;base64,QUJD"), b"ABC");
        // text/plain + base64
        assert_eq!(datauri_get_data("data:text/plain;base64,QUJD"), b"ABC");
    }

    #[test]
    fn hex_octet() {
        assert_eq!(url_decode_hex_octet(b"41"), 0x41);
        assert_eq!(url_decode_hex_octet(b"4g"), -1);
        assert_eq!(url_decode_hex_octet(b"4"), -1);
    }
}
