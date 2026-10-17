//! Port of `dpi/dpiutil.c` — utilitaires partagés des plugins dpi.
//!
//! Fonctions pures verbatim : échappement URI (`%XX`), entités HTML
//! et le filtre anti-relais SMTP des URLs `ftp://`.

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
const UNSAFE_CHARS: &[u8] = b"&<>\"'";
const UNSAFE_REP: [&[u8]; 5] = [b"&amp;", b"&lt;", b"&gt;", b"&quot;", b"&#39;"];

/// `Escape_uri_str` — échappe les caractères de `esc_set`
/// (défaut `%#:' `) plus tous les contrôles (`<= 0x1F`, `0x7F`) en
/// `%XX` hexadécimal majuscule.
#[must_use]
pub fn escape_uri_str(s: &[u8], esc_set: Option<&[u8]>) -> Vec<u8> {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let esc = esc_set.unwrap_or(b"%#:' ");
    let mut out = Vec::with_capacity(s.len());
    for &c in s {
        if c <= 0x1F || c == 0x7F || esc.contains(&c) {
            out.push(b'%');
            out.push(HEX[usize::from(c >> 4) & 15]);
            out.push(HEX[usize::from(c) & 15]);
        } else {
            out.push(c);
        }
    }
    out
}

/// `Unescape_uri_str` — inverse : `%XX` → octet (hexa requis des deux
/// côtés ; `%` non suivi d'hexa est conservé — verbatim).
#[must_use]
pub fn unescape_uri_str(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%'
            && i + 2 < s.len()
            && dillo_common::dlib::is_xdigit(s[i + 1])
            && dillo_common::dlib::is_xdigit(s[i + 2])
        {
            let hi = hex_val(s[i + 1]);
            let lo = hex_val(s[i + 2]);
            out.push(hi * 16 + lo);
            i += 3;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

fn hex_val(c: u8) -> u8 {
    if c.is_ascii_digit() {
        c - b'0'
    } else {
        c.to_ascii_uppercase() - b'A' + 10
    }
}

/// `Escape_html_str` — `&<>"'` → entités.
#[must_use]
pub fn escape_html_str(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for &c in s {
        match UNSAFE_CHARS.iter().position(|&u| u == c) {
            Some(k) => out.extend_from_slice(UNSAFE_REP[k]),
            None => out.push(c),
        }
    }
    out
}

/// `Unescape_html_str` — inverse de [`escape_html_str`], comparaison
/// insensible à la casse (`dStrnAsciiCasecmp` verbatim).
#[must_use]
pub fn unescape_html_str(s: &[u8]) -> Vec<u8> {
    if !s.contains(&b'&') {
        return s.to_vec();
    }
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'&' {
            let mut matched = false;
            for k in 0..5 {
                let rep = UNSAFE_REP[k];
                if s.len() - i >= rep.len()
                    && s[i..i + rep.len()]
                        .iter()
                        .zip(rep.iter())
                        .all(|(a, b)| a.to_ascii_lowercase() == *b)
                {
                    out.push(UNSAFE_CHARS[k]);
                    i += rep.len();
                    matched = true;
                    break;
                }
            }
            if !matched {
                out.push(s[i]);
                i += 1;
            }
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// `Filter_smtp_hack` — retire `\n`, `\r`, `%0A`, `%0D` (casse libre)
/// de la partie autorité d'une URL `ftp://` (anti-relais SMTP).
#[must_use]
pub fn filter_smtp_hack(url: &[u8]) -> Vec<u8> {
    let mut u = url.to_vec();
    if u.len() > 6 {
        // commence après "ftp://"
        let mut i = 6;
        while i < u.len() && u[i] != b'/' {
            let c = u[i];
            if c == b'\n' || c == b'\r' {
                u.remove(i);
                // verbatim : --i (réexamine la même position)
                if i > 6 {
                    i -= 1;
                } else {
                    // i == 6 : reste sur place (≙ --i puis ++i)
                }
            } else if c == b'%'
                && i + 2 < u.len()
                && u[i + 1] == b'0'
                && matches!(u[i + 2].to_ascii_lowercase(), b'a' | b'd')
            {
                u.remove(i);
                u.remove(i);
                u.remove(i);
                if i > 6 {
                    i -= 1;
                }
            } else {
                i += 1;
            }
        }
    }
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_uri() {
        assert_eq!(escape_uri_str(b"a b%:", None), b"a%20b%25%3A");
        assert_eq!(escape_uri_str(b"x\ty\x7f", None), b"x%09y%7F");
        assert_eq!(escape_uri_str(b"a b", Some(b"-")), b"a b");
    }

    #[test]
    fn unescape_uri() {
        assert_eq!(unescape_uri_str(b"a%20b%3A"), b"a b:");
        assert_eq!(unescape_uri_str(b"100%"), b"100%");
        assert_eq!(unescape_uri_str(b"%zz"), b"%zz");
        assert_eq!(unescape_uri_str(b"%2f"), b"/");
    }

    #[test]
    fn escape_html() {
        assert_eq!(
            escape_html_str(b"<a href=\"x\">&'"),
            b"&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
        assert_eq!(unescape_html_str(b"&LT;&amp;"), b"<&");
        // pas d'entité → copie
        assert_eq!(unescape_html_str(b"plain"), b"plain");
    }

    #[test]
    fn smtp_hack() {
        assert_eq!(
            filter_smtp_hack(b"ftp://u%0Aser:pw@host/dir"),
            b"ftp://user:pw@host/dir"
        );
        assert_eq!(filter_smtp_hack(b"ftp://u\nser@h/d"), b"ftp://user@h/d");
        // %0D%0A casse mixte
        assert_eq!(filter_smtp_hack(b"ftp://a%0db@h/"), b"ftp://ab@h/");
        // protégé : '/' termine la zone autorité
        assert_eq!(filter_smtp_hack(b"ftp://h/%0A"), b"ftp://h/%0A");
    }
}
