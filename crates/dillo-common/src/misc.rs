//! Port of `src/misc.c` + `src/misc.h` (dont le
//! `a_Misc_parse_content_disposition` inline). Helpers transverses :
//! escaping, expand-tabs, sniffing de Content-Type, base64, parsing
//! de `geometry`, `search_url` et `Content-Disposition`.

use crate::dlib;
use crate::utf8;

/// `a_Misc_escape_chars` — échappe en `%XX` les octets `<= 0x1F`,
/// `0x7F` ou présents dans `esc_set`. Hex uppercase comme amont.
#[must_use]
pub fn escape_chars(s: &[u8], esc_set: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = Vec::with_capacity(s.len().max(64));
    for &b in s {
        if b <= 0x1F || b == 0x7F || esc_set.contains(&b) {
            out.push(b'%');
            out.push(HEX[usize::from(b >> 4)]);
            out.push(HEX[usize::from(b & 15)]);
        } else {
            out.push(b);
        }
    }
    out
}

const TAB_SIZE: usize = 8;
const COMBINING_CHAR_SPACE: usize = 32;

/// `a_Misc_expand_tabs` — consomme `input` en avançant le curseur,
/// écrit dans `buf` (tabs → espaces au prochain taquet de 8, char =
/// +1 colonne). Suit l'allocation amont : garde `TAB_SIZE+32` octets
/// de marge, puis absorbe les diacritiques combinants en queue.
/// Retourne les octets écrits.
pub fn expand_tabs(input: &mut &[u8], buf: &mut [u8]) -> usize {
    let mut written = 0usize;
    let mut pos = 0usize;
    let buflen = buf.len();

    while !input.is_empty() && written + TAB_SIZE + COMBINING_CHAR_SPACE < buflen {
        let (code, char_len) = utf8::decode(input, 0, input.len());

        if code == i32::from(b'\t') {
            let old_pos = pos;
            pos += TAB_SIZE - (pos % TAB_SIZE);
            for _ in old_pos..pos {
                buf[written] = b' ';
                written += 1;
            }
        } else {
            debug_assert!(char_len <= 4);
            let n = char_len.min(input.len());
            buf[written..written + n].copy_from_slice(&input[..n]);
            written += n;
            pos += 1;
        }
        *input = &input[char_len.min(input.len())..];
    }

    // diacritiques combinants en fin de mot (marge de 4 octets amont)
    while !input.is_empty() && written + 4 < buflen {
        let (code, char_len) = utf8::decode(input, 0, input.len());
        if !utf8::combining_char(code) {
            break;
        }
        debug_assert!(char_len <= 4);
        let n = char_len.min(input.len());
        buf[written..written + n].copy_from_slice(&input[..n]);
        written += n;
        *input = &input[n..];
    }

    written
}

// ---- Content-Type (table + sniffing + compare) -------------------

/// Types MIME que Dillo sait gérer (`MimeTypes`, verbatim ; le second
/// champ est la longueur de préfixe utilisée par `content_type_check`).
const MIME_TYPES: &[(&str, usize)] = &[
    ("application/octet-stream", 24),
    ("application/xhtml+xml", 21),
    ("text/html", 9),
    ("text/plain", 10),
    ("image/gif", 9),
    ("image/png", 9),
    ("image/jpeg", 10),
];

const DT_OCTET_STREAM: usize = 0;
const DT_TEXT_HTML: usize = 2;
const DT_TEXT_PLAIN: usize = 3;
const DT_IMAGE_GIF: usize = 4;
const DT_IMAGE_PNG: usize = 5;
const DT_IMAGE_JPG: usize = 6;

fn ascii_ci_starts_with(hay: &[u8], needle: &str) -> bool {
    dlib::strn_ascii_casecmp(hay, needle.as_bytes(), needle.len()) == 0
}

/// `a_Misc_get_content_type_from_data` — retourne
/// `(mime, statut)` ; statut 0 sûr, 1 doute, 2 manque de données.
/// Port verbatim y compris la réaffectation de `Size`.
#[must_use]
pub fn get_content_type_from_data(data: &[u8]) -> (&'static str, i32) {
    let p = data;
    let mut st = 1; // « doubt » par défaut
    let mut ty = DT_OCTET_STREAM;

    // HTML : sauter les espaces puis tester les signatures
    let i = p.iter().take_while(|&&b| dlib::is_space(b)).count();
    let rest = &p[i.min(p.len())..];
    if ascii_ci_starts_with(rest, "<html")
        || ascii_ci_starts_with(rest, "<head")
        || ascii_ci_starts_with(rest, "<title")
        || ascii_ci_starts_with(rest, "<!doctype html")
        || ascii_ci_starts_with(rest, "<!-- HTML")
    {
        ty = DT_TEXT_HTML;
        st = 0;
    } else if p.len() >= 4 && &p[..4] == b"GIF8" {
        ty = DT_IMAGE_GIF;
        st = 0;
    } else if p.len() >= 4 && &p[..4] == b"\x89PNG" {
        ty = DT_IMAGE_PNG;
        st = 0;
    } else if p.len() >= 2 && &p[..2] == b"\xff\xd8" {
        ty = DT_IMAGE_JPG;
        st = 0;
    } else {
        // heuristique text/plain — verbatim amont
        let mut non_ascii = 0usize;
        let mut non_ascii_text = 0usize;
        let mut bin_chars = 0usize;
        let mut size = p.len().min(256);
        for &b in &p[..size] {
            if b < 32 && !dlib::is_space(b) {
                bin_chars += 1;
            }
            if b > 126 {
                non_ascii += 1;
            }
            if b > 190 {
                non_ascii_text += 1;
            }
        }
        if bin_chars == 0 && (non_ascii - non_ascii_text) <= size / 10 {
            ty = DT_TEXT_PLAIN;
        } else if size > 0 {
            // check UTF-8 : étendre jusqu'à la fin du dernier char
            size = utf8::end_of_char(p, size - 1) + 1;
            if utf8::test(&p[..size.min(p.len())]) > 0 {
                ty = DT_TEXT_PLAIN;
            }
        }
        if size >= 256 {
            st = 0;
        }
    }

    (MIME_TYPES[ty].0, st)
}

const TSPECIALS_SPACE: &[u8] = b"()<>@,;:\\\"/[]?= ";

fn is_token_char(c: u8) -> bool {
    c != 0 && dlib::is_ascii(c) && !dlib::is_cntrl(c) && !TSPECIALS_SPACE.contains(&c)
}

fn token_len(s: &[u8]) -> usize {
    s.iter().take_while(|&&b| is_token_char(b)).count()
}

/// Résultat de `parse_content_type` (les out-params amont).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ContentType {
    /// type majeur (`"text"`, `"image"`…)
    pub major: Option<Vec<u8>>,
    /// sous-type (`"html"`, `"png"`…)
    pub minor: Option<Vec<u8>>,
    /// charset (`"utf-8"`…) — `text/*` et `xhtml+xml` seulement
    pub charset: Option<Vec<u8>>,
}

/// `a_Misc_parse_content_type` — `"text/html; charset=utf-8"` →
/// `(major, minor, charset)`. Charset seulement pour `text/*` ou
/// `application/xhtml+xml` (RFC 2046 / 3236, verbatim).
#[must_use]
pub fn parse_content_type(ty: Option<&[u8]>) -> ContentType {
    const TERMINATORS: &[u8] = b" ;\t";
    const KEY: &[u8] = b"charset";
    let Some(str_) = ty else {
        return ContentType::default();
    };

    let n = token_len(str_);
    let major = Some(str_[..n].to_vec());
    let rest = &str_[n..];

    let mut minor = None;
    let mut after = rest;
    if rest.first() == Some(&b'/') {
        let s = &rest[1..];
        let m = token_len(s);
        minor = Some(s[..m].to_vec());
        after = &s[m..];
    }

    let mut charset = None;
    if !after.is_empty()
        && (dlib::strn_ascii_casecmp(str_, b"text/", 5) == 0
            || dlib::strn_ascii_casecmp(str_, b"application/xhtml+xml", 21) == 0)
    {
        if let Some(s_idx) = dlib::stri_ascii_str(after, KEY) {
            if s_idx == 0 || TERMINATORS.contains(&after[s_idx - 1]) {
                let mut s = &after[s_idx + KEY.len()..];
                while s.first().is_some_and(|&c| c == b' ' || c == b'\t') {
                    s = &s[1..];
                }
                if s.first() == Some(&b'=') {
                    s = &s[1..];
                    while s.first().is_some_and(|&c| c == b' ' || c == b'\t') {
                        s = &s[1..];
                    }
                    let len = s
                        .iter()
                        .position(|&c| TERMINATORS.contains(&c))
                        .unwrap_or(s.len());
                    if len > 0 {
                        let (mut cs, mut clen) = (&s[..len], len);
                        if cs.first() == Some(&b'"') && cs[len - 1] == b'"' && len > 1 {
                            cs = &cs[1..];
                            clen = len - 2;
                        }
                        charset = Some(cs[..clen].to_vec());
                    }
                }
            }
        }
    }
    ContentType {
        major,
        minor,
        charset,
    }
}

/// `a_Misc_content_type_cmp` — `true` si équivalents (maj/min égal
/// ci-insensible + charset égal ou l'un absent et l'autre UTF-8).
#[must_use]
pub fn content_type_cmp(ct1: Option<&[u8]>, ct2: Option<&[u8]>) -> bool {
    let empty = |ct: Option<&[u8]>| ct.is_none_or(<[u8]>::is_empty);
    if empty(ct1) && empty(ct2) {
        return true;
    }
    if empty(ct1) || empty(ct2) {
        return false;
    }
    let t1 = parse_content_type(ct1);
    let t2 = parse_content_type(ct2);

    let ci_eq = |a: &Option<Vec<u8>>, b: &Option<Vec<u8>>| -> bool {
        match (a, b) {
            (Some(x), Some(y)) => dlib::str_ascii_casecmp(x, y) == 0,
            _ => false,
        }
    };
    let charset_ok = match (&t1.charset, &t2.charset) {
        (None, None) => true,
        (Some(a), Some(b)) => dlib::str_ascii_casecmp(a, b) == 0,
        (None, Some(b)) | (Some(b), None) => dlib::str_ascii_casecmp(b, b"UTF-8") == 0,
    };
    ci_eq(&t1.major, &t2.major) && ci_eq(&t1.minor, &t2.minor) && charset_ok
}

/// `a_Misc_content_type_check` — compare le type serveur au type
/// détecté. `0` = match, `-1` = mismatch.
///
/// **Quirk amont (porté fidèlement)** : `-1` est en pratique
/// inatteignable — la branche qui conserve `-1` exige `et=text/*` +
/// `det=application/*` (elif2 faux) puis `et==xhtml` ou `det==text/html`
/// (elif3 faux), deux conditions mutuellement impossibles. Le check de
/// sécurité décrit dans les commentaires amont ne tire donc jamais.
#[must_use]
pub fn content_type_check(entry_type: Option<&[u8]>, detected_type: &[u8]) -> i32 {
    tracing::debug!(
        srv = ?entry_type.map(String::from_utf8_lossy),
        det = ?String::from_utf8_lossy(detected_type),
        "Type check"
    );
    let Some(et) = entry_type else {
        return 0; // pas de type serveur → pas de mismatch
    };

    // premier type connu dont `et` partage le préfixe (index 1.. — on
    // saute octet-stream comme amont qui commence sa boucle à 1)
    let idx = MIME_TYPES[1..]
        .iter()
        .position(|&(name, len)| dlib::strn_ascii_casecmp(et, name.as_bytes(), len) == 0)
        .map(|i| i + 1);

    let st = match idx {
        None => 0, // type inconnu → pas de mismatch
        Some(i) => {
            let (name, len) = MIME_TYPES[i];
            // les 3 branches retournent 0 : miroir délibéré des elifs
            // amont — la dernière branche (-1) est inatteignable.
            #[allow(clippy::if_same_then_else)]
            if dlib::strn_ascii_casecmp(et, b"image/", 6) == 0
                && dlib::strn_ascii_casecmp(detected_type, name.as_bytes(), len) == 0
            {
                0 // image avec match exact
            } else if dlib::strn_ascii_casecmp(et, b"text/", 5) != 0
                || dlib::strn_ascii_casecmp(detected_type, b"application/", 12) != 0
            {
                0 // pas un « application » servi en texte
            } else if dlib::strn_ascii_casecmp(et, b"application/xhtml+xml", 21) != 0
                || dlib::strn_ascii_casecmp(detected_type, b"text/html", 9) != 0
            {
                0 // version XML de HTML
            } else {
                -1
            }
        }
    };
    tracing::debug!(
        result = if st == 0 { "MATCH" } else { "MISMATCH" },
        "Type check"
    );
    st
}

// ---- geometry / search_url / base64 / files ----------------------

/// `strtol(…, base 10)` minimal : skip espaces, signe optionnel,
/// chiffres décimaux. Retourne `(valeur saturée i32, index de fin)`.
fn strtol_prefix(s: &[u8]) -> (i32, usize) {
    let mut end = 0;
    while end < s.len() && dlib::is_space(s[end]) {
        end += 1;
    }
    let neg = match s.get(end) {
        Some(b'-') => {
            end += 1;
            true
        }
        Some(b'+') => {
            end += 1;
            false
        }
        _ => false,
    };
    let digits_start = end;
    while end < s.len() && s[end].is_ascii_digit() {
        end += 1;
    }
    if end == digits_start {
        // pas de chiffre → strtol retourne 0 et end == début
        return (0, 0);
    }
    let val: i64 = s[digits_start..end].iter().fold(0i64, |acc, &digit| {
        acc.saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'))
    });
    let val = if neg { -val } else { val };
    (
        i32::try_from(val.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(i32::MAX),
        end,
    )
}

/// `a_Misc_parse_geometry` — `"WxH"` ou `"WxH+X+Y"` / `-X-Y`.
/// Retourne `Some((x, y, w, h))` ; `x`,`y` = `None` si absents.
#[must_use]
pub fn parse_geometry(s: &[u8]) -> Option<(Option<i32>, Option<i32>, i32, i32)> {
    let sep = s.iter().position(|&c| c == b'x' || c == b'X')?;
    let (n1, t1) = strtol_prefix(s);
    let (n2, rel_t2) = strtol_prefix(&s[sep + 1..]);
    let t2 = sep + 1 + rel_t2;
    if t1 == 0 || t2 == sep + 1 {
        return None;
    }
    let (width, height) = (n1, n2);
    // parse x,y optionnels
    let (nx1, e1) = strtol_prefix(&s[t2..]);
    let e1 = t2 + e1;
    let (nx2, e2) = strtol_prefix(&s[e1..]);
    let e2 = e1 + e2;
    let (pos_x, pos_y) = if e1 != t2 && e2 != e1 {
        (Some(nx1), Some(nx2))
    } else {
        (None, None)
    };
    tracing::debug!(width, height, ?pos_x, ?pos_y, "geom");
    Some((pos_x, pos_y, width, height))
}

/// `a_Misc_parse_search_url` — `[<label> ]<url>` (dillorc). Le label
/// auto-dérivé est le host quand absent. Amont borne le label à 31 chars.
#[must_use]
pub fn parse_search_url(source: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let check_url = |url: &[u8]| -> Option<usize> {
        // strchr(url,'/') && p[1] && strchr(url+p+2,'/') → index de q
        let p = url.iter().position(|&c| c == b'/')?;
        if p + 1 >= url.len() {
            return None;
        }
        url[p + 2..]
            .iter()
            .position(|&c| c == b'/')
            .map(|k| p + 2 + k)
    };

    if let Some(sp) = source.iter().rposition(|&c| c == b' ') {
        let label = &source[..sp.min(source.len()).min(31)];
        let url = &source[sp + 1..];
        check_url(url).map(|_| (label.to_vec(), url.to_vec()))
    } else {
        let p = source.iter().position(|&c| c == b'/')?;
        let q = check_url(source)?;
        let label = &source[p + 2..(p + 2 + (q - p - 2).min(31))];
        Some((label.to_vec(), source.to_vec()))
    }
}

/// `a_Misc_encode_base64` — verbatim (alphabet standard, `=` padding).
#[must_use]
pub fn encode_base64(input: &[u8]) -> Vec<u8> {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for c in &mut chunks {
        out.push(B64[usize::from(c[0] >> 2)]);
        out.push(B64[usize::from(((c[0] << 4) & 0x30) | (c[1] >> 4))]);
        out.push(B64[usize::from(((c[1] << 2) & 0x3c) | (c[2] >> 6))]);
        out.push(B64[usize::from(c[2] & 0x3f)]);
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        out.push(B64[usize::from(rem[0] >> 2)]);
        let mut frag = (rem[0] << 4) & 0x30;
        if rem.len() > 1 {
            frag |= rem[1] >> 4;
        }
        out.push(B64[usize::from(frag)]);
        out.push(if rem.len() < 2 {
            b'='
        } else {
            B64[usize::from((rem[1] << 2) & 0x3c)]
        });
        out.push(b'=');
    }
    out
}

/// `a_Misc_file2dstr` — charge un fichier local.
///
/// # Errors
/// Erreur `std::io` si le fichier est illisible.
pub fn file_to_vec(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}

/// `a_Misc_parse_content_disposition` (static inline dans misc.h,
/// RFC 6266 lenient). Retourne `(type, filename sanitizé)`.
#[must_use]
pub fn parse_content_disposition(disposition: Option<&[u8]>) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    const TERMINATORS: &[u8] = b" ;\t";
    const KEY: &[u8] = b"filename";
    const BAD: &[u8] = b"/\\|~";
    let Some(disp) = disposition else {
        return (None, None);
    };
    let str_ = disp;

    let n = token_len(str_);
    if n == 0 {
        return (None, None);
    }
    let ty = Some(str_[..n].to_vec());
    let mut s = &str_[n..];
    if s.first().is_none_or(|&c| !TERMINATORS.contains(&c)) {
        return (None, None);
    }
    while s.first().is_some_and(|&c| c == b' ' || c == b'\t') {
        s = &s[1..];
    }
    if s.first() != Some(&b';') {
        return (ty, None);
    }

    let Some(idx) = dlib::stri_ascii_str(str_, KEY) else {
        return (ty, None);
    };
    if idx != 0 && !TERMINATORS.contains(&str_[idx - 1]) {
        return (ty, None);
    }
    let mut s = &str_[idx + KEY.len()..];
    while s.first().is_some_and(|&c| c == b' ' || c == b'\t') {
        s = &s[1..];
    }
    if s.first() != Some(&b'=') {
        return (ty, None);
    }
    s = &s[1..];
    while s.first().is_some_and(|&c| c == b' ' || c == b'\t') {
        s = &s[1..];
    }

    let mut quoted = false;
    let mut raw: &[u8] = &[];
    if s.first() == Some(&b'"') {
        quoted = true;
        s = &s[1..];
        while s.first() == Some(&b'.') {
            s = &s[1..];
        }
        if s.len() < 2 {
            return (ty, None);
        }
        for i in 1..s.len() {
            if s[i - 1] != b'\\' && s[i] == b'"' {
                raw = &s[..i];
                break;
            }
        }
    } else {
        while s.first() == Some(&b'.') {
            s = &s[1..];
        }
        let len = s
            .iter()
            .position(|&c| TERMINATORS.contains(&c))
            .unwrap_or(s.len());
        if len > 0 {
            raw = &s[..len];
        }
    }
    if raw.is_empty() {
        return (ty, None);
    }

    // sanitation : `\"` → `"`, `/\|~` → `_`, non-ascii/= rejette unquoted
    let mut dst = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let c = raw[i];
        if i + 1 < raw.len() && c == b'\\' && raw[i + 1] == b'"' {
            dst.push(b'"');
            i += 1;
        } else if BAD.contains(&c) {
            dst.push(b'_');
        } else if !quoted && (!dlib::is_ascii(c) || c == b'=') {
            return (ty, None); // mauvais char → pas de filename
        } else {
            dst.push(c);
        }
        i += 1;
    }
    (ty, Some(dst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape() {
        assert_eq!(escape_chars(b"a b", b" "), b"a%20b");
        assert_eq!(escape_chars(b"\x01\x7f", b""), b"%01%7F");
        assert_eq!(escape_chars(b"ok", b" "), b"ok");
    }

    #[test]
    fn tabs() {
        let mut input: &[u8] = b"a\tb";
        let mut buf = [0u8; 64];
        let n = expand_tabs(&mut input, &mut buf);
        assert_eq!(&buf[..n], b"a       b"); // tab → taquet 8
        assert_eq!(input, b"");
    }

    #[test]
    fn tabs_combining_tail() {
        // marge amont = TAB_SIZE+32 = 40 → buflen 41 laisse la boucle 1
        // écrire 'e' puis s'arrêter ; le combining part en boucle 2
        let mut input: &[u8] = "e\u{301}x".as_bytes();
        let mut buf = [0u8; 41];
        let n = expand_tabs(&mut input, &mut buf);
        assert_eq!(&buf[..n], "e\u{301}".as_bytes());
        assert_eq!(input, b"x"); // curseur arrêté avant 'x'
    }

    #[test]
    fn sniff() {
        assert_eq!(get_content_type_from_data(b"  <html>yo").0, "text/html");
        assert_eq!(get_content_type_from_data(b"GIF89a..").0, "image/gif");
        assert_eq!(get_content_type_from_data(b"\x89PNG\r\n").0, "image/png");
        assert_eq!(get_content_type_from_data(b"\xff\xd8\xff").0, "image/jpeg");
        // texte ASCII ≥256 octets → text/plain + st=0
        let txt = vec![b'a'; 300];
        assert_eq!(get_content_type_from_data(&txt), ("text/plain", 0));
        // quirk amont : octets <0x80 (même de contrôle) → UTF-8 valide
        // → text/plain ; le binaire n'est détecté que si UTF-8 casse
        assert_eq!(get_content_type_from_data(&[0, 1, 2, 3]).0, "text/plain");
        let bin = [0xFF, 0x00, 0x01, 0x02];
        assert_eq!(
            get_content_type_from_data(&bin).0,
            "application/octet-stream"
        );
    }

    #[test]
    fn content_type_parse() {
        let t = parse_content_type(Some(b"text/html; charset=utf-8"));
        assert_eq!(t.major.as_deref(), Some(b"text".as_slice()));
        assert_eq!(t.minor.as_deref(), Some(b"html".as_slice()));
        assert_eq!(t.charset.as_deref(), Some(b"utf-8".as_slice()));

        let t = parse_content_type(Some(b"text/plain; charset=\"latin1\""));
        assert_eq!(t.charset.as_deref(), Some(b"latin1".as_slice()));
        // charset ignoré hors text/*
        let t = parse_content_type(Some(b"image/png; charset=x"));
        assert!(t.charset.is_none());
    }

    #[test]
    fn content_type_eq() {
        assert!(content_type_cmp(
            Some(b"text/html; charset=UTF-8"),
            Some(b"TEXT/HTML")
        ));
        assert!(!content_type_cmp(Some(b"text/html"), None));
        assert!(content_type_cmp(None, None));
    }

    #[test]
    fn content_check() {
        // quirk amont (vérifié dans misc.c) : st=-1 est inatteignable —
        // l'elif2 exige et=text/* + det=application/* mais l'elif3 ne
        // peut alors jamais être faux → toujours 0. Porté fidèlement.
        assert_eq!(
            content_type_check(Some(b"text/plain"), b"application/octet-stream"),
            0
        );
        // image/jpeg servi + détecté jpeg → ok
        assert_eq!(content_type_check(Some(b"image/jpeg"), b"image/jpeg"), 0);
        // type serveur inconnu → ok
        assert_eq!(content_type_check(Some(b"video/mp4"), b"text/plain"), 0);
        // pas de type serveur → ok
        assert_eq!(content_type_check(None, b"text/html"), 0);
    }

    #[test]
    fn geometry() {
        assert_eq!(parse_geometry(b"800x600"), Some((None, None, 800, 600)));
        assert_eq!(
            parse_geometry(b"800x600+10+20"),
            Some((Some(10), Some(20), 800, 600))
        );
        assert_eq!(
            parse_geometry(b"640x480-5-7"),
            Some((Some(-5), Some(-7), 640, 480))
        );
        assert!(parse_geometry(b"nonsense").is_none());
    }

    #[test]
    fn search_url() {
        let (label, url) = parse_search_url(b"Google https://google.com/search?q=%s").unwrap();
        assert_eq!(label, b"Google");
        assert_eq!(url, b"https://google.com/search?q=%s");
        // sans label → host auto
        let (label, _) = parse_search_url(b"https://duckduckgo.com/?q=%s").unwrap();
        assert_eq!(label, b"duckduckgo.com");
        assert!(parse_search_url(b"notanurl").is_none());
    }

    #[test]
    fn b64() {
        assert_eq!(encode_base64(b""), b"");
        assert_eq!(encode_base64(b"f"), b"Zg==");
        assert_eq!(encode_base64(b"fo"), b"Zm8=");
        assert_eq!(encode_base64(b"foo"), b"Zm9v");
        assert_eq!(encode_base64(b"foobar"), b"Zm9vYmFy");
    }

    #[test]
    fn disposition() {
        let (ty, name) = parse_content_disposition(Some(b"attachment; filename=\"file name.jpg\""));
        assert_eq!(ty.as_deref(), Some(b"attachment".as_slice()));
        assert_eq!(name.as_deref(), Some(b"file name.jpg".as_slice()));

        // amont saute les points *initiaux* seulement :
        // "../../etc/passwd" → raw "/../etc/passwd" → sanitizé
        let (_, name) = parse_content_disposition(Some(b"attachment; filename=../../etc/passwd"));
        assert_eq!(name.as_deref(), Some(b"_.._etc_passwd".as_slice()));

        let (_, name) = parse_content_disposition(Some(b"inline"));
        assert!(name.is_none());
    }
}
