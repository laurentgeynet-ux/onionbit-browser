//! Port of `src/url.c` + `src/url.h` — `DilloUrl` lenient.
//!
//! Différences de structure par rapport à l'amont (même sémantique) :
//! les champs `scheme`/`authority`/`path`/`query`/`fragment` sont des
//! `Option<Vec<u8>>` owned plutôt que des vues NUL-terminées dans un
//! `buffer` unique ; `Parsed::query_sep` conserve la position du `?`
//! pour l'arithmétique pointeur de `Url_resolve_relative`.
//!
//! Le switch HSTS (`prefs.http_strict_transport_security` /
//! `http_force_https` + `a_Hsts_require_https`) est isolé dans
//! [`DilloUrl::switch_to_https`] — le branchement prefs/hsts arrive
//! avec leurs ports respectifs.

use dillo_common::dlib;

pub const URL_HTTP_PORT: i32 = 80;
pub const URL_HTTPS_PORT: i32 = 443;

/// `URL_*` flags (`url.h` amont) — set par le demandeur (capi).
pub const URL_GET: u32 = 1 << 0;
pub const URL_POST: u32 = 1 << 1;
pub const URL_ILC: u32 = 1 << 2;
pub const URL_MAP: u32 = 1 << 3;
pub const URL_RELOAD_PAGE: u32 = 1 << 4;
pub const URL_RELOAD_FROM_CACHE: u32 = 1 << 5;
pub const URL_WEBKIT_BACK_TRACK: u32 = 1 << 6;

/// `a_Url_host_type` return codes.
pub const URL_HOST_ERROR: i32 = -1;
pub const URL_HOST_NAME: i32 = 0;
pub const URL_HOST_IPV4: i32 = 1;
pub const URL_HOST_IPV6: i32 = 2;

/// `DilloUrl->flags` bits (verbatim).
pub mod flags {
    pub const GET: u32 = 1 << 0;
    pub const POST: u32 = 1 << 1;
    pub const E2E_QUERY: u32 = 1 << 5;
    pub const RELOAD_PAGE: u32 = 1 << 7;
    pub const RELOAD_FROM_CACHE: u32 = 1 << 8;
    pub const IGNORE_SCROLL: u32 = 1 << 9;
    pub const SPAM_SAFE: u32 = 1 << 10;
    pub const MULTIPART_ENC: u32 = 1 << 11;
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// `DilloUrl` — fields `Option` (`NULL` amont).
#[derive(Debug, Default, Clone)]
pub struct DilloUrl {
    url_string: Option<Vec<u8>>,
    pub scheme: Option<Vec<u8>>,
    pub authority: Option<Vec<u8>>,
    pub path: Option<Vec<u8>>,
    pub query: Option<Vec<u8>>,
    pub fragment: Option<Vec<u8>>,
    hostname: Option<Vec<u8>>,
    pub port: i32,
    pub flags: u32,
    pub data: Vec<u8>, // POST (Dstr amont)
    pub ismap_url_len: usize,
    pub illegal_chars: usize,
    pub illegal_chars_spc: usize,
    hostname_resolved: bool,
}

/// `strpbrk(s, set)` — index du 1er octet de `s` présent dans `set`.
fn strpbrk(s: &[u8], set: &[u8]) -> Option<usize> {
    s.iter().position(|b| set.contains(b))
}

/// Parse interne = `Url_object_new` : retourne les champs + `query_sep`.
/// `buffer` = chaîne strippée (espaces en tête/queue).
fn url_object_new(uri_str: &[u8]) -> Parsed {
    let mut buf = uri_str.to_vec();
    dlib::str_strip(&mut buf); // strip leading & trailing space

    let mut p = Parsed {
        buffer: buf,
        ..Parsed::default()
    };
    let mut s: usize = 0; // curseur dans buffer
    let b = &p.buffer;
    let blen = b.len();

    // scheme : 1er octet parmi ":/?#" — doit être ':' et non en pos 0
    let mut mark = strpbrk(&b[s..], b":/?#").map(|i| s + i);
    if let Some(i) = mark.filter(|&i| b[i] == b':' && i > s) {
        p.scheme = Some(b[s..i].to_vec());
        s = i + 1;
        mark = Some(s); // p == s après incrément
    }
    // authority : le reste commence par "//"  (upstream teste p == s)
    if mark == Some(s) && s + 1 < blen && b[s] == b'/' && b[s + 1] == b'/' {
        let start = s + 2;
        if let Some(rel) = strpbrk(&b[start..], b"/?#") {
            let end = start + rel;
            p.authority = Some(b[start..end].to_vec());
            s = end;
        } else {
            if start < blen {
                p.authority = Some(b[start..].to_vec());
                return p;
            }
            s = start;
        }
    }

    // path : jusqu'au prochain '?' ou '#'
    if let Some(rel) = strpbrk(&b[s..], b"?#") {
        let end = s + rel;
        p.path = if end > s {
            Some(b[s..end].to_vec())
        } else {
            None
        };
        s = end;
    } else {
        if s < blen {
            p.path = Some(b[s..].to_vec());
            return p;
        }
        s = blen;
    }

    // query : '?' → jusqu'à '#'
    let mut mark = if s < blen {
        strpbrk(&b[s..], b"?#").map(|i| s + i)
    } else {
        None
    };
    if let Some(i) = mark.filter(|&i| b[i] == b'?') {
        p.query_sep = Some(i);
        s = i + 1;
        p.query = Some(b[s..].to_vec()); // sera tronqué au '#' ci-dessous
        p.flags |= flags::GET;
        mark = strpbrk(&b[s..], b"#").map(|k| s + k);
        if let Some(hash) = mark {
            p.query = Some(b[s..hash].to_vec());
        }
    }
    // fragment
    if let Some(i) = mark {
        if i < blen && b[i] == b'#' {
            p.fragment = Some(b[i + 1..].to_vec());
        }
    }
    p
}

#[derive(Debug, Default)]
struct Parsed {
    buffer: Vec<u8>,
    scheme: Option<Vec<u8>>,
    authority: Option<Vec<u8>>,
    path: Option<Vec<u8>>,
    query: Option<Vec<u8>>,
    fragment: Option<Vec<u8>>,
    flags: u32,
    query_sep: Option<usize>,
}

impl DilloUrl {
    /// `a_Url_str` — reconstruction paresseuse de l'URL complète.
    pub fn url_str(&mut self) -> &[u8] {
        if self.url_string.is_none() {
            let mut out = Vec::with_capacity(60);
            if let Some(s) = &self.scheme {
                out.extend_from_slice(s);
                out.push(b':');
            }
            if let Some(a) = &self.authority {
                out.extend_from_slice(b"//");
                out.extend_from_slice(a);
                if self.path.as_ref().is_none_or(|p| p.first() != Some(&b'/')) {
                    out.push(b'/');
                }
            }
            if let Some(p) = &self.path {
                out.extend_from_slice(p);
            }
            if let Some(q) = &self.query {
                out.push(b'?');
                out.extend_from_slice(q);
            }
            if let Some(f) = &self.fragment {
                out.push(b'#');
                out.extend_from_slice(f);
            }
            self.url_string = Some(out);
        }
        self.url_string.as_deref().unwrap_or(&[])
    }

    /// `URL_STR` — jamais NULL ("" si pas de string).
    pub fn str_(&mut self) -> &[u8] {
        self.url_str()
    }

    /// `a_Url_hostname` — parse lazy de `authority` (ipv6 `[..]`,
    /// `host:port`) + port par défaut http/https.
    pub fn hostname(&mut self) -> Option<&[u8]> {
        if !self.hostname_resolved {
            self.hostname_resolved = true;
            if let Some(auth) = self.authority.clone() {
                if auth.first() == Some(&b'[') {
                    if let Some(close) = auth.iter().position(|&c| c == b']') {
                        self.hostname = Some(auth[1..close].to_vec());
                        if let Some(colon) = auth[close..].iter().position(|&c| c == b':') {
                            self.port = strtol10(&auth[close + colon + 1..]);
                        }
                    }
                } else if let Some(colon) = auth.iter().position(|&c| c == b':') {
                    self.port = strtol10(&auth[colon + 1..]);
                    self.hostname = Some(auth[..colon].to_vec());
                } else {
                    self.hostname = Some(auth);
                }
            }
            if self.port == 0 {
                if let Some(sch) = &self.scheme {
                    if dlib::str_ascii_casecmp(sch, b"http") == 0 {
                        self.port = URL_HTTP_PORT;
                    } else if dlib::str_ascii_casecmp(sch, b"https") == 0 {
                        self.port = URL_HTTPS_PORT;
                    }
                }
            }
        }
        self.hostname.as_deref()
    }

    /// `URL_PORT` — force la résolution du port (macro amont appelle
    /// `URL_HOST` d'abord).
    pub fn port(&mut self) -> i32 {
        let _ = self.hostname();
        self.port
    }

    /// `a_Url_set_flags`.
    pub fn set_flags(&mut self, flags: u32) {
        self.flags = flags;
    }

    /// `a_Url_set_data` (POST…).
    pub fn set_data(&mut self, data: Vec<u8>) {
        self.data = data;
    }

    /// `a_Url_set_ismap_coords` — append `coord_str` à
    /// l'`url_string`, `query` pointe après le séparateur
    /// (`ismap_url_len + 1`).
    pub fn set_ismap_coords(&mut self, coord_str: &[u8]) {
        if self.ismap_url_len == 0 {
            let _ = self.url_str();
            self.ismap_url_len = self.url_string.as_ref().map_or(0, Vec::len);
        }
        if let Some(us) = &mut self.url_string {
            us.truncate(self.ismap_url_len);
            us.extend_from_slice(coord_str);
            let q = us[self.ismap_url_len + 1..].to_vec();
            self.query = Some(q);
        }
    }

    /// Switch HSTS de `a_Url_new` — `http:` → `https:` : scheme,
    /// port 80→443, `:80` en fin d'authority → `:443`, `url_string`
    /// invalidé. Appelé par le caller selon `prefs`/`hsts`.
    pub fn switch_to_https(&mut self) {
        if self
            .scheme
            .as_deref()
            .is_none_or(|s| dlib::str_ascii_casecmp(s, b"http") != 0)
        {
            return;
        }
        self.scheme = Some(b"https".to_vec());
        let _ = self.hostname();
        if self.port == URL_HTTP_PORT {
            self.port = URL_HTTPS_PORT;
        }
        if let Some(auth) = &mut self.authority {
            let len = auth.len();
            if len >= 3 && &auth[len - 3..] == b":80" {
                auth.truncate(len - 2); // ":80" → ":"
                auth.extend_from_slice(b"443"); // → ":443"
            }
        }
        self.url_string = None;
    }

    /// `a_Url_same_organization` — même suffixe public (ci-insensible).
    #[must_use]
    pub fn same_organization(&mut self, other: &mut DilloUrl) -> bool {
        let h1 = self.hostname().map(<[u8]>::to_vec);
        let h2 = other.hostname().map(<[u8]>::to_vec);
        match (h1, h2) {
            (Some(a), Some(b)) => {
                dlib::str_ascii_casecmp(host_public_suffix(&a), host_public_suffix(&b)) == 0
            }
            _ => false,
        }
    }

    /// `a_Url_cmp` — ordre de tri : authority ci, path sans `/`
    /// initial, query, data, scheme ci. 0 = égaux.
    #[must_use]
    pub fn url_cmp(&self, other: &Self) -> i32 {
        let st = field_icmp(self.authority.as_deref(), other.authority.as_deref());
        if st != 0 {
            return st;
        }
        let st = strcmp(
            path_no_slash(self.path.as_deref()),
            path_no_slash(other.path.as_deref()),
        );
        if st != 0 {
            return st;
        }
        let st = field_cmp(self.query.as_deref(), other.query.as_deref());
        if st != 0 {
            return st;
        }
        let st = strcmp(&self.data, &other.data);
        if st != 0 {
            return st;
        }
        field_icmp(self.scheme.as_deref(), other.scheme.as_deref())
    }
}

/// `URL_STR_FIELD_I_CMP` amont.
fn field_icmp(a: Option<&[u8]>, b: Option<&[u8]>) -> i32 {
    match (a, b) {
        (Some(x), Some(y)) => dlib::str_ascii_casecmp(x, y),
        (None, None) => 0,
        (Some(_), None) => 1,
        (None, Some(_)) => -1,
    }
}

/// `URL_STR_FIELD_CMP` amont.
fn field_cmp(a: Option<&[u8]>, b: Option<&[u8]>) -> i32 {
    match (a, b) {
        (Some(x), Some(y)) => strcmp(x, y),
        (None, None) => 0,
        (Some(_), None) => 1,
        (None, Some(_)) => -1,
    }
}

/// path sans `'/'` initial (`""` si absent).
fn path_no_slash(p: Option<&[u8]>) -> &[u8] {
    p.map_or(b"", |s| if s.first() == Some(&b'/') { &s[1..] } else { s })
}

/// `strcmp` byte-lexicographique.
fn strcmp(a: &[u8], b: &[u8]) -> i32 {
    for i in 0..a.len().max(b.len()) {
        match (a.get(i), b.get(i)) {
            (Some(&x), Some(&y)) if x == y => {}
            (Some(&x), Some(&y)) => return i32::from(x) - i32::from(y),
            (Some(&x), None) => return i32::from(x),
            (None, Some(&y)) => return -i32::from(y),
            (None, None) => break,
        }
    }
    0
}

/// `strtol(…, 10)` sur les chiffres de tête (port).
fn strtol10(s: &[u8]) -> i32 {
    let mut i = 0;
    while i < s.len() && dlib::is_space(s[i]) {
        i += 1;
    }
    let neg = matches!(s.get(i), Some(b'-'));
    if matches!(s.get(i), Some(b'+' | b'-')) {
        i += 1;
    }
    let mut v: i64 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        v = v.saturating_mul(10).saturating_add(i64::from(s[i] - b'0'));
        i += 1;
    }
    let v = if neg { -v } else { v };
    i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(i32::MAX)
}

/// `Url_resolve_relative` — RFC-3986 + hacks Dillo. Retourne la
/// chaîne résolue.
fn resolve_relative(rel_str: &[u8], base_str: &[u8]) -> Vec<u8> {
    let rel = url_object_new(rel_str);
    // base parsée seulement si pas de scheme dans rel (amont)
    let base = if rel.scheme.is_none() {
        Some(url_object_new(base_str))
    } else {
        None
    };

    let mut solved: Vec<u8> = Vec::with_capacity(64);
    let mut path: Vec<u8> = Vec::with_capacity(64);

    if rel.path.is_none() && rel.scheme.is_none() && rel.authority.is_none() {
        // référence « same document » : base − fragment, + query/fragment
        // de rel
        solved.extend_from_slice(base_str);
        if let Some(h) = solved.iter().position(|&c| c == b'#') {
            solved.truncate(h);
        }
        let base = base.as_ref();
        if base.is_none_or(|b| b.path.is_none()) {
            solved.push(b'/');
        }
        if let Some(q) = &rel.query {
            if let Some(sep) = base.and_then(|b| b.query_sep) {
                // tronquer au '?' de la base (BaseUrl.query - buffer - 1)
                solved.truncate(sep.min(solved.len()));
            }
            solved.push(b'?');
            solved.extend_from_slice(q);
        }
        if let Some(f) = &rel.fragment {
            solved.push(b'#');
            solved.extend_from_slice(f);
        }
        return solved;
    }

    if rel.scheme.is_some() {
        solved.extend_from_slice(rel_str);
        return solved;
    }

    if rel.authority.is_some() {
        if let Some(p) = &rel.path {
            path.extend_from_slice(p);
        }
    } else {
        let base = base.as_ref();
        let absolute = rel.path.as_ref().is_some_and(|p| p.first() == Some(&b'/'));
        if !absolute {
            if let Some(bp) = base.and_then(|b| b.path.as_ref()) {
                path.extend_from_slice(bp);
                // tronquer après le dernier '/' (= répertoire de base)
                let mut i = path.len();
                loop {
                    if i == 0 {
                        break;
                    }
                    i -= 1;
                    if path[i] == b'/' {
                        path.truncate(i + 1);
                        break;
                    }
                }
            }
        }
        if let Some(rp) = &rel.path {
            path.extend_from_slice(rp);
        }
        remove_dot_segments(&mut path);
    }

    // STEP 7
    if let Some(bs) = base.as_ref().and_then(|b| b.scheme.as_ref()) {
        solved.extend_from_slice(bs);
        solved.push(b':');
    }
    if let Some(ra) = &rel.authority {
        solved.extend_from_slice(b"//");
        solved.extend_from_slice(ra);
    } else if let Some(ba) = base.as_ref().and_then(|b| b.authority.as_ref()) {
        solved.extend_from_slice(b"//");
        solved.extend_from_slice(ba);
    }
    // hack '/' avant un path relatif quand il y a une authority
    let has_auth = rel.authority.is_some() || base.as_ref().is_some_and(|b| b.authority.is_some());
    let empty_path_with_qf = path.is_empty() && (rel.query.is_some() || rel.fragment.is_some());
    let nonempty_nonabs = !path.is_empty() && path.first() != Some(&b'/');
    if has_auth && (empty_path_with_qf || nonempty_nonabs) {
        solved.push(b'/');
    }
    solved.extend_from_slice(&path);
    if let Some(q) = &rel.query {
        solved.push(b'?');
        solved.extend_from_slice(q);
    }
    if let Some(f) = &rel.fragment {
        solved.push(b'#');
        solved.extend_from_slice(f);
    }
    solved
}

/// `Url_resolve_relative` §path — suppression des dot-segments
/// (`./`, `.` final, `<seg>/..[/]`), port verbatim des boucles amont.
fn remove_dot_segments(path: &mut Vec<u8>) {
    // erase "./" (au début ou après '/')
    while let Some(pos) = find_subslice(path, b"./") {
        if pos == 0 || path[pos - 1] == b'/' {
            path.drain(pos..pos + 2);
        } else {
            break;
        }
    }
    // erase "." final
    if path.last() == Some(&b'.') && (path.len() == 1 || path[path.len() - 2] == b'/') {
        path.truncate(path.len() - 1);
    }
    // erase "<segment>/../" et "<segment>/.."
    let mut scan = 0usize;
    while let Some(rel_i) = find_subslice(&path[scan.min(path.len())..], b"/..") {
        let mut pp = scan.min(path.len()) + rel_i;
        // p[3] doit être '/' ou fin ("/../" | "/..")
        if pp + 3 == path.len() || path[pp + 3] == b'/' {
            let e = pp + 3;
            // reculer pp jusqu'au '/' précédent ou début
            while pp > 0 && path[pp - 1] != b'/' {
                pp -= 1;
            }
            let extra = usize::from(pp > 0 && e < path.len());
            path.drain(pp..(e + extra).min(path.len()));
            scan = pp.saturating_sub(usize::from(pp > 0));
        } else {
            scan = pp + 3;
        }
    }
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// `a_Url_new` §1 — encode les caractères illégaux
/// (hors `]0x1F,0x7F[` ou espace) en `%XX`. Retourne
/// `(encodé, n_ic, n_ic_spc)`.
fn encode_illegal(url_str: &[u8]) -> (Vec<u8>, usize, usize) {
    let mut n_ic = 0usize;
    let mut n_ic_spc = 0usize;
    for &c in url_str {
        if c == b' ' {
            n_ic_spc += 1;
        } else if !(0x20..0x7f).contains(&c) {
            n_ic += 1;
        }
    }
    let mut out = Vec::with_capacity(url_str.len() + 2 * (n_ic + n_ic_spc));
    if n_ic + n_ic_spc == 0 {
        out.extend_from_slice(url_str);
    } else {
        for &c in url_str {
            if (0x20..0x7f).contains(&c) && c != b' ' {
                out.push(c);
            } else {
                out.push(b'%');
                out.push(HEX[usize::from(c >> 4)]);
                out.push(HEX[usize::from(c & 15)]);
            }
        }
    }
    (out, n_ic, n_ic_spc)
}

/// `a_Url_new` §2 — heuristique sans base : base `"http:"` et, selon
/// la forme de `urlstr`, préfixe `"//"` (ou `'/'` pour `"/x"` →
/// `"//x"`). Retourne l'URL effective.
fn apply_default_heuristic(urlstr: &[u8]) -> Vec<u8> {
    if urlstr.first() != Some(&b'/') {
        // préfixe "//" sauf si le premier char spécial est ':'
        let first_special = strpbrk(urlstr, b"/#?:");
        if first_special.is_none_or(|i| urlstr[i] != b':') {
            let mut out = Vec::with_capacity(urlstr.len() + 2);
            out.extend_from_slice(b"//");
            out.extend_from_slice(urlstr);
            return out;
        }
        urlstr.to_vec()
    } else if urlstr.get(1) != Some(&b'/') {
        let mut out = Vec::with_capacity(urlstr.len() + 1);
        out.push(b'/');
        out.extend_from_slice(urlstr);
        out
    } else {
        urlstr.to_vec()
    }
}

/// `a_Url_new` — encode les caractères illégaux, heuristique `http:`
/// par défaut, résout contre `base_url`. `None` si malformée.
#[must_use]
pub fn url_new(url_str: &[u8], base_url: Option<&[u8]>) -> Option<DilloUrl> {
    // Empty URL sans base invalide (forms action="" avec base)
    if url_str.is_empty() && base_url.is_none() {
        return None;
    }

    let (encoded, n_ic, n_ic_spc) = encode_illegal(url_str);
    let (effective, base): (Vec<u8>, &[u8]) = if let Some(b) = base_url {
        (encoded, b)
    } else {
        (apply_default_heuristic(&encoded), b"http:")
    };

    let solved = resolve_relative(&effective, base);
    tracing::debug!(solved = ?String::from_utf8_lossy(&solved), "SolvedUrl");

    let parsed = url_object_new(&solved);
    Some(DilloUrl {
        url_string: Some(solved),
        scheme: parsed.scheme,
        authority: parsed.authority,
        path: parsed.path,
        query: parsed.query,
        fragment: parsed.fragment,
        hostname: None,
        port: 0,
        flags: parsed.flags,
        data: Vec::new(),
        ismap_url_len: 0,
        illegal_chars: n_ic,
        illegal_chars_spc: n_ic_spc,
        hostname_resolved: false,
    })
}

/// `a_Url_decode_hex_str` — décode `%XX`, laisse les `%` invalides.
#[must_use]
pub fn decode_hex_str(s: &[u8]) -> Vec<u8> {
    if !s.contains(&b'%') {
        return s.to_vec();
    }
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' && i + 2 < s.len() {
            if let Some(v) = decode_hex_octet(s[i + 1], s[i + 2]) {
                out.push(v);
                i += 3; // % + 2 chars (amont : `i+=2` puis `i++`)
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

/// `Url_decode_hex_octet` — `strtol(hex,…,16)` sur 2 chars :
/// `tail-hex==2` accepte aussi espace C ou `+` avant un chiffre
/// (`%+f`→15, `% f`→15) ; `-` donne `val<0` → rejet amont (littéral).
fn decode_hex_octet(a: u8, b: u8) -> Option<u8> {
    if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() {
        Some((hex_val(a) << 4) | hex_val(b))
    } else if b.is_ascii_hexdigit() && (a == b'+' || is_c_space(a)) {
        Some(hex_val(b))
    } else {
        None
    }
}

/// `isspace()` C locale — `str::is_ascii_whitespace` n'a pas `\v`.
const fn is_c_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn hex_val(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => c - b'A' + 10,
    }
}

/// `a_Url_encode_hex_str` — alnum-ascii + `-_.*` verbatim,
/// `' '`→`+`, `'\n'`→`%0D%0A`, reste `%XX`.
#[must_use]
pub fn encode_hex_str(s: &[u8]) -> Vec<u8> {
    const VERBATIM: &[u8] = b"-_.*";
    let mut out = Vec::with_capacity(s.len() + 8);
    for &c in s {
        if (dlib::is_alnum(c) && dlib::is_ascii(c)) || VERBATIM.contains(&c) {
            out.push(c);
        } else if c == b' ' {
            out.push(b'+');
        } else if c == b'\n' {
            out.extend_from_slice(b"%0D%0A");
        } else {
            out.push(b'%');
            out.push(HEX[usize::from(c >> 4)]);
            out.push(HEX[usize::from(c & 15)]);
        }
    }
    out
}

/// `a_Url_string_strip_delimiters` — retire `"URL:"`, `<…>` et
/// espaces/chars illégaux internes (import RFC-3986).
#[must_use]
pub fn string_strip_delimiters(s: &[u8]) -> Vec<u8> {
    let mut text = s;
    if text.len() >= 4 && &text[..4] == b"URL:" {
        text = &text[4..];
    }
    if text.first() == Some(&b'<') {
        text = &text[1..];
    }
    let mut out = Vec::with_capacity(text.len());
    for &c in text {
        if (0x20..0x7f).contains(&c) && c != b' ' {
            out.push(c);
        }
    }
    if out.last() == Some(&b'>') {
        out.pop();
    }
    out
}

/// `a_Url_host_type` — IPV4 si que chiffres+points, IPV6 si ':' et
/// alphabet `[hex:.]`, sinon NAME (ERROR si vide).
#[must_use]
pub fn host_type(host: &[u8]) -> i32 {
    if host.is_empty() {
        return URL_HOST_ERROR;
    }
    if host.iter().all(|&c| c.is_ascii_digit() || c == b'.') {
        return URL_HOST_IPV4;
    }
    if host.contains(&b':')
        && host
            .iter()
            .all(|&c| c.is_ascii_hexdigit() || c == b':' || c == b'.')
    {
        return URL_HOST_IPV6;
    }
    URL_HOST_NAME
}

/// `Url_host_public_internal_dots` — 1 (+1 si TLD de la liste amont).
fn host_public_internal_dots(host: &[u8]) -> usize {
    let mut ret = 1usize;
    let mut after = host.len();
    if after > 0 && host[after - 1] == b'.' {
        after -= 1;
    }
    let mut start = after;
    while start > 0 && host[start - 1] != b'.' {
        start -= 1;
    }
    let tld_len = after - start;
    if tld_len > 0 {
        const TLDS: &[&str] = &[
            "bd", "bn", "ck", "cy", "er", "fj", "fk", "gu", "jm", "ke", "kh", "kw", "mm", "mz",
            "ni", "np", "pg", "ye", "za", "zw",
        ];
        for tld in TLDS {
            if tld.len() == tld_len
                && dlib::strn_ascii_casecmp(tld.as_bytes(), &host[start..start + tld_len], tld_len)
                    == 0
            {
                tracing::debug!(tld, "TLD code matched");
                ret += 1;
                break;
            }
        }
    }
    ret
}

/// `Url_host_find_public_suffix` — portion publique du host
/// (`www.dillo.org` → `dillo.org`).
fn host_public_suffix(host: &[u8]) -> &[u8] {
    if host_type(host) != URL_HOST_NAME {
        return host;
    }
    let mut s = host.len();
    if s > 0 && host[s - 1] == b'.' {
        s -= 1; // trailing dot ignoré
    }
    let mut dots = host_public_internal_dots(host);
    let mut i = s;
    while i > 0 {
        if host[i - 1] == b'.' {
            if dots == 0 {
                break;
            }
            dots -= 1;
        }
        i -= 1;
    }
    tracing::debug!(host = ?String::from_utf8_lossy(host), suffix = ?String::from_utf8_lossy(&host[i..]), "public suffix");
    &host[i..]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &[u8], base: Option<&[u8]>) -> DilloUrl {
        url_new(s, base).unwrap_or_else(|| panic!("url_new failed for {s:?}"))
    }

    #[test]
    fn parse_full() {
        // exemple verbatim du doc-comment amont
        let mut url = u(b"http://dillo.sf.net:8080/index.html?long#part2", None);
        assert_eq!(url.scheme.as_deref(), Some(b"http".as_slice()));
        assert_eq!(
            url.authority.as_deref(),
            Some(b"dillo.sf.net:8080".as_slice())
        );
        assert_eq!(url.path.as_deref(), Some(b"/index.html".as_slice()));
        assert_eq!(url.query.as_deref(), Some(b"long".as_slice()));
        assert_eq!(url.fragment.as_deref(), Some(b"part2".as_slice()));
        assert_eq!(url.hostname(), Some(b"dillo.sf.net".as_slice()));
        assert_eq!(url.port, 8080);
        assert!(url.flags & flags::GET != 0);
        assert_eq!(
            url.url_str(),
            b"http://dillo.sf.net:8080/index.html?long#part2"
        );
    }

    #[test]
    fn default_port() {
        let mut url = u(b"http://dillo.org/x", None);
        assert_eq!(url.hostname(), Some(b"dillo.org".as_slice()));
        assert_eq!(url.port(), URL_HTTP_PORT);
        let mut url = u(b"https://dillo.org/", None);
        let _ = url.hostname();
        assert_eq!(url.port, URL_HTTPS_PORT);
    }

    #[test]
    fn heuristic_no_base() {
        // pas de scheme → http: par défaut
        let mut url = u(b"dillo.org/path", None);
        assert_eq!(url.url_str(), b"http://dillo.org/path");
        // scheme explicite conservé
        let mut url = u(b"ftp://dillo.org/f", None);
        assert_eq!(url.url_str(), b"ftp://dillo.org/f");
    }

    #[test]
    fn illegal_chars_encoded() {
        // espace → %20 ; byte >0x7E → %XX
        let mut url = u(b"http://dillo.org/a b/\xc3\xa9", None);
        assert_eq!(url.url_str(), b"http://dillo.org/a%20b/%C3%A9");
        assert_eq!(url.illegal_chars_spc, 1);
        assert_eq!(url.illegal_chars, 2);
    }

    #[test]
    fn resolve_relative_rfc() {
        let base = b"http://a/b/c/d;p?q";
        // cas du RFC-3986 §5.4 (normal)
        let cases: &[(&[u8], &[u8])] = &[
            (b"g:h", b"g:h"),
            (b"g", b"http://a/b/c/g"),
            (b"./g", b"http://a/b/c/g"),
            (b"g/", b"http://a/b/c/g/"),
            (b"/g", b"http://a/g"),
            (b"//g", b"http://g"),
            (b"?y", b"http://a/b/c/d;p?y"),
            (b"g?y", b"http://a/b/c/g?y"),
            (b"#s", b"http://a/b/c/d;p?q#s"),
            (b"g#s", b"http://a/b/c/g#s"),
            (b"g?y#s", b"http://a/b/c/g?y#s"),
            (b";x", b"http://a/b/c/;x"),
            (b"g;x", b"http://a/b/c/g;x"),
            (b"g;x?y#s", b"http://a/b/c/g;x?y#s"),
            (b"", b"http://a/b/c/d;p?q"),
            (b".", b"http://a/b/c/"),
            (b"./", b"http://a/b/c/"),
            (b"..", b"http://a/b/"),
            (b"../", b"http://a/b/"),
            (b"../g", b"http://a/b/g"),
            (b"../..", b"http://a/"),
            (b"../../", b"http://a/"),
            (b"../../g", b"http://a/g"),
        ];
        for (rel, want) in cases {
            let mut url = url_new(rel, Some(base)).unwrap_or_else(|| panic!("failed rel {rel:?}"));
            assert_eq!(url.url_str(), *want, "rel={rel:?}");
        }
    }

    #[test]
    fn resolve_dotdot_abnormal() {
        let base = b"http://a/b/c/d;p?q";
        // RFC-3986 §5.4.2 abnormal — sémantique Dillo
        for (rel, want) in [
            (&b"../../../g"[..], &b"http://a/g"[..]),
            (b"../../../../g", b"http://a/g"),
            (b"/./g", b"http://a/g"),
            (b"/../g", b"http://a/g"),
        ] {
            let mut url = url_new(rel, Some(base)).unwrap_or_else(|| panic!("{rel:?}"));
            assert_eq!(url.url_str(), want, "rel={rel:?}");
        }
    }

    #[test]
    fn same_doc_refs() {
        let base = b"http://a/b/c/d;p?q";
        // fragment seul → base + fragment
        let mut url = u(b"#frag", Some(base));
        assert_eq!(url.url_str(), b"http://a/b/c/d;p?q#frag");
        // query seule → remplace la query
        let mut url = u(b"?new", Some(base));
        assert_eq!(url.url_str(), b"http://a/b/c/d;p?new");
    }

    #[test]
    fn empty_url_needs_base() {
        assert!(url_new(b"", None).is_none());
        assert!(url_new(b"", Some(b"http://a/x")).is_some());
    }

    #[test]
    fn url_cmp_order() {
        let a = u(b"http://x.org/A?1", None);
        let b = u(b"http://x.org/A?1", None);
        assert_eq!(a.url_cmp(&b), 0);
        // path compare ignore le '/' initial → "/A" == "A"
        let c = u(b"http://x.org/A?2", None);
        assert_ne!(a.url_cmp(&c), 0);
        // authority + scheme ci-insensibles
        let up = u(b"HTTP://X.ORG/A?1", None);
        assert_eq!(a.url_cmp(&up), 0);
    }

    #[test]
    fn decode_encode() {
        assert_eq!(decode_hex_str(b"a%20b%2Fc"), b"a b/c");
        assert_eq!(decode_hex_str(b"100%"), b"100%"); // % invalide conservé
        assert_eq!(decode_hex_str(b"%zz"), b"%zz");
        // `strtol` lenient (A02-D1) : espace/signe + chiffre valent 2
        assert_eq!(decode_hex_str(b"%+f"), b"\x0f");
        assert_eq!(decode_hex_str(b"% f"), b"\x0f");
        assert_eq!(decode_hex_str(b"%-f"), b"%-f"); // val<0 → littéral
        assert_eq!(decode_hex_str(b"%f "), b"%f "); // tail=1 → littéral
        assert_eq!(decode_hex_str(b"%0x"), b"%0x"); // tail=1 → littéral
        assert_eq!(encode_hex_str(b"a b"), b"a+b");
        assert_eq!(encode_hex_str(b"a\nb"), b"a%0D%0Ab");
        assert_eq!(encode_hex_str(b"caf\xc3\xa9"), b"caf%C3%A9");
        assert_eq!(encode_hex_str(b"a/b"), b"a%2Fb");
    }

    #[test]
    fn strip_delim() {
        // amont supprime l'espace (pas terminateur) → "a b" → "ab"
        assert_eq!(
            string_strip_delimiters(b"URL:<http://x.org/a b>"),
            b"http://x.org/ab"
        );
        assert_eq!(
            string_strip_delimiters(b"<http://x.org/>"),
            b"http://x.org/"
        );
    }

    #[test]
    fn host_kinds() {
        assert_eq!(host_type(b"192.168.0.1"), URL_HOST_IPV4);
        assert_eq!(host_type(b"::1"), URL_HOST_IPV6);
        assert_eq!(host_type(b"2001:db8::1"), URL_HOST_IPV6);
        assert_eq!(host_type(b"dillo.org"), URL_HOST_NAME);
        assert_eq!(host_type(b""), URL_HOST_ERROR);
    }

    #[test]
    fn public_suffix() {
        assert_eq!(host_public_suffix(b"www.dillo.org"), b"dillo.org");
        assert_eq!(host_public_suffix(b"a.b.co.ck"), b"b.co.ck"); // tld list
        assert_eq!(host_public_suffix(b"host"), b"host");
        assert_eq!(host_public_suffix(b"192.168.0.1"), b"192.168.0.1");
        let mut u1 = u(b"http://www.dillo.org/a", None);
        let mut u2 = u(b"http://sub.dillo.org/b", None);
        assert!(u1.same_organization(&mut u2));
        let mut u3 = u(b"http://other.org/", None);
        assert!(!u1.same_organization(&mut u3));
    }

    #[test]
    fn ipv6_authority() {
        let mut url = u(b"http://[::1]:8080/x", None);
        assert_eq!(url.hostname(), Some(b"::1".as_slice()));
        assert_eq!(url.port, 8080);
    }

    #[test]
    fn https_switch() {
        let mut url = u(b"http://dillo.org:80/x", None);
        url.switch_to_https();
        assert_eq!(url.url_str(), b"https://dillo.org:443/x");
        let mut url = u(b"http://dillo.org/x", None);
        url.switch_to_https();
        assert_eq!(url.url_str(), b"https://dillo.org/x");
    }
}
