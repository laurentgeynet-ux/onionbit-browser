//! Port de `src/auth.c` + `auth.h` — HTTP authentication (RFC 2617) :
//! parse des challenges `WWW-Authenticate`, arbre `AuthHost`
//! (`scheme`+`authority`) → `AuthRealm` (nom, chemins, credentials),
//! dialog user/password **injecté** (`a_Dialog_user_password` amont
//! est synchrone-bloquant — ici `dialog(title, msg) ->
//! Option<(user, pass)>`).
//!
//! Amont : `auth_hosts` global → `Auth` en `thread_local`. Le digest
//! vit dans [`crate::digest`] (≙ `digest.c`).

use std::cell::RefCell;

use dillo_common::dlib;
use tracing::{debug, warn};

use crate::url::DilloUrl;

/// `AuthParseHTTPAuthType_t`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AuthType {
    #[default]
    NotSet,
    Basic,
    Digest,
}

/// `AuthParseDigestAlgorithm_t`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DigestAlgorithm {
    #[default]
    NotSet,
    Md5,
    Md5Sess,
}

/// `AuthParseDigestQOP_t`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DigestQop {
    #[default]
    NotSet,
    Auth,
    AuthInt,
}

/// `AuthRealm_t` — champs pub (partagés avec [`crate::digest`]).
#[derive(Debug)]
pub struct AuthRealm {
    pub typ: AuthType,
    pub name: String,
    /// `paths` — trailing `/` retiré (racine = `""`).
    pub paths: Vec<String>,
    /// BASIC : en-tête complet ; DIGEST : `H(A1)`.
    pub authorization: String,
    pub username: String,
    pub cnonce: String,
    pub nonce_count: u32,
    pub nonce: String,
    pub opaque: Option<String>,
    pub algorithm: DigestAlgorithm,
    /// `domain` — NOT USED (verbatim amont).
    pub domain: String,
    pub qop: DigestQop,
}

/// `AuthHost_t`.
#[derive(Debug)]
struct AuthHost {
    scheme: String,
    authority: String,
    realms: Vec<AuthRealm>,
}

/// `AuthParse_t` — résultat du parse d'un challenge.
#[derive(Debug, Default)]
struct AuthParse {
    ok: bool,
    typ: AuthType,
    realm: Option<String>,
    nonce: Option<String>,
    opaque: Option<String>,
    stale: bool,
    algorithm: DigestAlgorithm,
    domain: Option<String>,
    qop: DigestQop,
}

/// Dialog user/password injecté ≙ `a_Dialog_user_password` amont
/// (synchrone-bloquant → `Option<(user, pass)>`).
pub type AuthDialog<'a> = dyn FnMut(&str, &str) -> Option<(String, String)> + 'a;

/// ≙ `auth_hosts` amont — registre global `thread_local` (single-writer).
#[derive(Debug, Default)]
pub struct Auth {
    hosts: Vec<AuthHost>,
}

thread_local! {
    static AUTH: RefCell<Auth> = RefCell::new(Auth::default());
}

/// `Auth_is_token_char` — `!dIsascii || strchr(invalid) || dIscntrl` →
/// non-token. `invalid` verbatim.
fn is_token_char(c: u8) -> bool {
    const INVALID: &[u8] = b"\"()<>@,;:\\[]?=/{} \t";
    c.is_ascii() && !INVALID.contains(&c) && !dlib::is_cntrl(c)
}

/// `Auth_unquote_value` — skip ' '/'\t' ; si `"` : jusqu'à `"` fermante
/// (`\x` escape) ; sinon run de token-chars. Retourne (valeur, reste,
/// était-quoté).
fn unquote_value(v: &[u8]) -> (Vec<u8>, &[u8], bool) {
    let mut i = 0;
    while i < v.len() && (v[i] == b' ' || v[i] == b'\t') {
        i += 1;
    }
    let quoted = i < v.len() && v[i] == b'"';
    if quoted {
        i += 1;
    }
    let mut out = Vec::new();
    while i < v.len() {
        let c = v[i];
        let cont = if quoted { c != b'"' } else { is_token_char(c) };
        if !cont {
            break;
        }
        // `(c=='\\' && value[1]) ? *++value : c` — escape consommé ;
        // `value[1]` falsy si NUL → backslash littéral (verbatim).
        if c == b'\\' && i + 1 < v.len() && v[i + 1] != 0 {
            i += 1;
            out.push(v[i]);
        } else {
            out.push(c);
        }
        i += 1;
    }
    if quoted && i < v.len() && v[i] == b'"' {
        i += 1;
    }
    (out, &v[i..], quoted)
}

type ParseCb<'a> = dyn FnMut(&mut AuthParse, &[u8], &[u8]) -> bool + 'a;

/// `Auth_parse_token_value` — token `=` valeur (quoted ou token-run) ;
/// erreurs verbatim : token manquant / valeur manquante / garbage.
fn parse_token_value(auth_parse: &mut AuthParse, auth: &mut &[u8], cb: &mut ParseCb) -> bool {
    while !auth.is_empty() {
        // token
        let start = *auth;
        let mut n = 0;
        while n < auth.len() && is_token_char(auth[n]) {
            n += 1;
        }
        if n == 0 {
            warn!("Auth_parse_token_value: missing auth token");
            return false;
        }
        let token = &start[..n];
        *auth = &auth[n..];
        // skip LWS
        while auth.first().is_some_and(|&c| c == b' ' || c == b'\t') {
            *auth = &auth[1..];
        }
        // '='
        match auth.first() {
            Some(b'=') => {
                *auth = &auth[1..];
            }
            None | Some(b',') => {
                warn!("Auth_parse_token_value: missing auth token value");
                return false;
            }
            _ => {
                warn!("Auth_parse_token_value: garbage after auth token");
                return false;
            }
        }
        let (value, rest, was_quoted) = unquote_value(auth);
        *auth = rest;
        // `expect_quoted` : tout sauf stale/algorithm doit être quoté —
        // `(*auth)[-1] == '"'` amont = dernier octet consommé par
        // `unquote` était la quote fermante ≙ `was_quoted`.
        let expect_quoted = !(token == b"stale" || token == b"algorithm");
        if was_quoted != expect_quoted {
            warn!(
                "Auth_parse_token_value: Values for key {} should{} be quoted.",
                String::from_utf8_lossy(token),
                if expect_quoted { "" } else { " not" }
            );
        }
        if !cb(auth_parse, token, &value) {
            break;
        }
        // skip ' ' et ','
        while auth.first().is_some_and(|&c| c == b' ' || c == b',') {
            *auth = &auth[1..];
        }
    }
    true
}

/// `Auth_parse_basic_challenge_cb` — premier `realm` seul, stop.
fn basic_cb(p: &mut AuthParse, token: &[u8], value: &[u8]) -> bool {
    if dlib::str_ascii_casecmp(b"realm", token) == 0 {
        if p.realm.is_none() {
            p.realm = Some(String::from_utf8_lossy(value).into_owned());
        }
        return false; // end parsing
    }
    warn!(
        "Auth_parse_basic_challenge_cb: Ignoring unknown parameter: {} = '{}'",
        String::from_utf8_lossy(token),
        String::from_utf8_lossy(value)
    );
    true
}

/// `Auth_parse_digest_challenge_cb` — verbatim : champs 1er-arrivé,
/// `stale` bool, `MD5`/`MD5-sess` (sess refusé), `qop` liste → `auth`.
fn digest_cb(p: &mut AuthParse, token: &[u8], value: &[u8]) -> bool {
    if dlib::str_ascii_casecmp(b"realm", token) == 0 {
        if p.realm.is_none() {
            p.realm = Some(lossy(value));
        }
    } else if token == b"domain" {
        if p.domain.is_none() {
            p.domain = Some(lossy(value));
        }
    } else if token == b"nonce" {
        if p.nonce.is_none() {
            p.nonce = Some(lossy(value));
        }
    } else if token == b"opaque" {
        if p.opaque.is_none() {
            p.opaque = Some(lossy(value));
        }
    } else if token == b"stale" {
        if dlib::str_ascii_casecmp(b"true", value) == 0 {
            p.stale = true;
        } else if dlib::str_ascii_casecmp(b"false", value) == 0 {
            p.stale = false;
        } else {
            warn!(
                "Auth_parse_digest_challenge_cb: Invalid stale value: {}",
                lossy(value)
            );
            return false;
        }
    } else if token == b"algorithm" {
        if value == b"MD5" {
            p.algorithm = DigestAlgorithm::Md5;
        } else if value == b"MD5-sess" {
            warn!("Auth_parse_digest_challenge_cb: MD5-sess algorithm disabled (not tested because 'not correctly implemented yet' in Apache 2.2)");
            return false;
        } else {
            warn!(
                "Auth_parse_digest_challenge_cb: Unknown algorithm: {}",
                lossy(value)
            );
            return false;
        }
    } else if token == b"qop" {
        // scan `strcspn(", \t")` + skips — verbatim
        let mut v: &[u8] = value;
        while !v.is_empty() {
            let len = v
                .iter()
                .position(|&c| c == b',' || c == b' ' || c == b'\t')
                .unwrap_or(v.len());
            if len == 4 && &v[..4] == b"auth" {
                p.qop = DigestQop::Auth;
                break;
            }
            if len == 8 && &v[..8] == b"auth-int" {
                // auth-int désactivé amont — on continue à chercher « auth »
                warn!("Auth_parse_digest_challenge_cb: auth-int qop disabled (not tested because 'not implemented yet' in Apache 2.2)");
            } else {
                warn!(
                    "Auth_parse_digest_challenge_cb: Unknown qop value in {}",
                    lossy(v)
                );
            }
            v = &v[len..];
            while v.first().is_some_and(|&c| c == b' ' || c == b'\t') {
                v = &v[1..];
            }
            if v.first() == Some(&b',') {
                v = &v[1..];
            }
            while v.first().is_some_and(|&c| c == b' ' || c == b'\t') {
                v = &v[1..];
            }
        }
    } else {
        warn!(
            "Auth_parse_digest_challenge_cb: Ignoring unknown parameter: {} = '{}'",
            String::from_utf8_lossy(token),
            lossy(value)
        );
    }
    true
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// `Auth_parse_challenge_args` — boucle `,`/` ` + parse ; exigences
/// finales : BASIC→realm, DIGEST→realm+nonce.
fn parse_challenge_args(p: &mut AuthParse, challenge: &[u8], digest: bool) {
    let mut c = challenge;
    loop {
        while c.first().is_some_and(|&b| b == b' ' || b == b',') {
            c = &c[1..];
        }
        if c.is_empty() {
            break;
        }
        if digest {
            if !parse_token_value(p, &mut c, &mut digest_cb) {
                break;
            }
        } else if !parse_token_value(p, &mut c, &mut basic_cb) {
            break;
        }
    }
    match p.typ {
        AuthType::Basic => {
            if p.realm.is_some() {
                p.ok = true;
            } else {
                warn!("Auth_parse_challenge_args: missing Basic auth realm");
            }
        }
        AuthType::Digest => {
            if p.realm.is_some() && p.nonce.is_some() {
                p.ok = true;
            } else {
                warn!("Auth_parse_challenge_args: Digest challenge incomplete");
            }
        }
        AuthType::NotSet => {}
    }
}

/// `Auth_parse_challenge` — offset verbatim : `Digest `→+7, `Basic `→+6.
fn parse_challenge(challenge: &str, typ: AuthType) -> AuthParse {
    debug!("auth.c: Auth_parse_challenge: challenge = '{challenge}'");
    let mut p = AuthParse {
        typ,
        ..AuthParse::default()
    };
    let skip = if typ == AuthType::Digest { 7 } else { 6 };
    parse_challenge_args(
        &mut p,
        challenge.as_bytes().get(skip..).unwrap_or(&[]),
        typ == AuthType::Digest,
    );
    p
}

/// `Auth_path_is_inside` — `strncmp(p1,p2,len)==0 && (p1[len]=='\0' ||
/// p1[len]=='/')`.
fn path_is_inside(p1: &str, p2: &str, len: usize) -> bool {
    let b1 = p1.as_bytes();
    let b2 = p2.as_bytes();
    b1.len() >= len
        && b2.len() >= len
        && b1[..len] == b2[..len]
        && (b1.len() == len || b1[len] == b'/')
}

impl Auth {
    /// `Auth_host_by_url` — match `scheme`+`authority` ci.
    fn host_by_url(&self, url: &mut DilloUrl) -> Option<usize> {
        let _ = url.hostname();
        let scheme = url.scheme.clone().unwrap_or_default();
        let authority = url.authority.clone().unwrap_or_default();
        self.hosts.iter().position(|h| {
            dlib::str_ascii_casecmp(&scheme, h.scheme.as_bytes()) == 0
                && dlib::str_ascii_casecmp(&authority, h.authority.as_bytes()) == 0
        })
    }

    /// `Auth_realm_by_name` → **index** (le realm est muté en place).
    fn realm_idx_by_name(host: &AuthHost, name: &str) -> Option<usize> {
        host.realms.iter().position(|r| r.name == name)
    }

    /// `Auth_realm_by_path` — meilleur (plus long) chemin contenant
    /// → **index** (le realm est muté en place : `nonce_count`).
    fn realm_idx_by_path(host: &AuthHost, path: &str) -> Option<usize> {
        let mut best: Option<usize> = None;
        let mut best_len = 0usize;
        for (i, r) in host.realms.iter().enumerate() {
            for rp in &r.paths {
                if path_is_inside(path, rp, rp.len()) && !(best.is_some() && best_len >= rp.len()) {
                    best = Some(i);
                    best_len = rp.len();
                }
            }
        }
        best
    }

    /// `Auth_realm_includes_path`.
    fn realm_includes_path(realm: &AuthRealm, path: &str) -> bool {
        realm.paths.iter().any(|p| path_is_inside(path, p, p.len()))
    }

    /// `Auth_realm_add_path` — strip `/` final, supprime les chemins
    /// couverts (`dList_remove_fast` + `i--` amont), append.
    fn realm_add_path(realm: &mut AuthRealm, path: &str) {
        let n_path = path.strip_suffix('/').unwrap_or(path).to_string();
        let len = n_path.len();
        realm.paths.retain(|p| !path_is_inside(p, &n_path, len));
        realm.paths.push(n_path);
    }

    /// `a_Auth_get_auth_str` — realm par chemin → en-tête complet
    /// (Basic stocké / Digest recomputé par requête ; `nonce_count`
    /// incrémenté **dans le realm stocké** — le `nc` doit persister).
    pub fn get_auth_str(&mut self, url: &mut DilloUrl, request_uri: &str) -> Option<String> {
        let hi = self.host_by_url(url)?;
        let _ = url.hostname(); // URL_PATH amont
        let path = String::from_utf8_lossy(&url.path.clone().unwrap_or_default()).into_owned();
        let ri = Self::realm_idx_by_path(&self.hosts[hi], &path)?;
        let realm = &mut self.hosts[hi].realms[ri];
        match realm.typ {
            AuthType::Basic => Some(realm.authorization.clone()),
            AuthType::Digest => crate::digest::authorization_hdr(realm, url, request_uri),
            AuthType::NotSet => {
                warn!("a_Auth_get_auth_str() got an unknown realm type");
                None
            }
        }
    }

    /// `Auth_do_auth_required` — retourne vrai s'il faut un dialog :
    /// realm connu + chemin couvert → met à jour, pas de dialog ;
    /// digest+stale → nouveau nonce, pas de dialog.
    fn do_auth_required(&mut self, p: &AuthParse, url: &mut DilloUrl) -> bool {
        let _ = url.hostname();
        let path = String::from_utf8_lossy(&url.path.clone().unwrap_or_default()).into_owned();
        let realm_name = p.realm.clone().unwrap_or_default();
        if let Some(hi) = self.host_by_url(url) {
            let host = &mut self.hosts[hi];
            if let Some(ri) = Self::realm_idx_by_name(host, &realm_name) {
                let realm = &mut host.realms[ri];
                if !Self::realm_includes_path(realm, &path) {
                    debug!("Auth_do_auth_required: updating realm '{realm_name}'");
                    Self::realm_add_path(realm, &path);
                    return false;
                }
                if p.typ == AuthType::Digest && p.stale {
                    realm.nonce = p.nonce.clone().unwrap_or_default();
                    return false;
                }
            }
        }
        true
    }

    /// `Auth_do_auth_dialog_cb` — applique les credentials saisis :
    /// find/create host + realm, path, puis Basic (b64) ou Digest
    /// (fields + cnonce + `compute_digest` ; échec → realm retiré).
    fn apply_credentials(&mut self, p: &AuthParse, url: &mut DilloUrl, user: &str, password: &str) {
        let _ = url.hostname();
        let scheme = String::from_utf8_lossy(&url.scheme.clone().unwrap_or_default()).into_owned();
        let authority =
            String::from_utf8_lossy(&url.authority.clone().unwrap_or_default()).into_owned();
        let path = String::from_utf8_lossy(&url.path.clone().unwrap_or_default()).into_owned();
        let realm_name = p.realm.clone().unwrap_or_default();
        let hi = if let Some(i) = self.host_by_url(url) {
            i
        } else {
            self.hosts.push(AuthHost {
                scheme,
                authority,
                realms: Vec::new(),
            });
            self.hosts.len() - 1
        };
        let host = &mut self.hosts[hi];
        let ri = if let Some(i) = Self::realm_idx_by_name(host, &realm_name) {
            i
        } else {
            host.realms.push(AuthRealm {
                typ: AuthType::NotSet,
                name: realm_name,
                paths: Vec::new(),
                authorization: String::new(),
                username: String::new(),
                cnonce: String::new(),
                nonce_count: 0,
                nonce: String::new(),
                opaque: None,
                algorithm: DigestAlgorithm::NotSet,
                domain: String::new(),
                qop: DigestQop::NotSet,
            });
            host.realms.len() - 1
        };
        let realm = &mut host.realms[ri];
        realm.typ = p.typ;
        realm.authorization.clear();
        Self::realm_add_path(realm, &path);
        match realm.typ {
            AuthType::Basic => {
                let user_password = format!("{user}:{password}");
                let response = dillo_common::misc::encode_base64(user_password.as_bytes());
                realm.authorization = format!(
                    "Authorization: Basic {}\r\n",
                    String::from_utf8_lossy(&response)
                );
            }
            AuthType::Digest => {
                realm.username = user.to_string();
                realm.nonce_count = 0;
                realm.nonce = p.nonce.clone().unwrap_or_default();
                realm.opaque.clone_from(&p.opaque);
                realm.algorithm = p.algorithm;
                realm.domain = p.domain.clone().unwrap_or_default();
                realm.qop = p.qop;
                realm.cnonce = if matches!(realm.qop, DigestQop::NotSet) {
                    String::new()
                } else {
                    crate::digest::create_cnonce()
                };
                if !crate::digest::compute_digest(realm, user, password) {
                    warn!("Auth_do_auth_dialog_cb: a_Digest_compute_digest failed.");
                    host.realms.remove(ri);
                }
            }
            AuthType::NotSet => warn!("Auth_do_auth_dialog_cb: Unknown auth type"),
        }
    }

    /// `Auth_do_auth` — verbatim amont :
    /// `reload = ok ? (required ? dialog() : 1) : 0`.
    fn do_auth(
        &mut self,
        challenge: &str,
        typ: AuthType,
        url: &mut DilloUrl,
        dialog: &mut AuthDialog,
    ) -> bool {
        debug!("auth.c: Auth_do_auth: challenge={{{challenge}}}");
        let p = parse_challenge(challenge, typ);
        if !p.ok {
            return false;
        }
        if !self.do_auth_required(&p, url) {
            return true; // credentials à jour → reload sans dialog
        }
        // dialog : titre/message verbatim amont
        let typestr = if p.typ == AuthType::Digest {
            "Digest"
        } else {
            "Basic"
        };
        let realm = p.realm.clone().unwrap_or_default();
        let host = url
            .hostname()
            .map(|h| String::from_utf8_lossy(h).into_owned())
            .unwrap_or_default();
        let title = format!("Dillo: Password for {realm}");
        let msg = format!(
            "The server at {host} requires a username and password for  \"{realm}\".\n\nAuthentication scheme: {typestr}"
        );
        let Some((user, mut password)) = dialog(&title, &msg) else {
            return false;
        };
        self.apply_credentials(&p, url, &user, &password);
        shred_str(&mut password); // `dStrshred(password)` amont
        true
    }

    /// `a_Auth_do_auth` — challenges Digest d'abord, puis Basic.
    pub fn do_auth_list(
        &mut self,
        challenges: &[String],
        url: &mut DilloUrl,
        dialog: &mut AuthDialog,
    ) -> bool {
        for chal in challenges {
            if dlib::strn_ascii_casecmp(chal.as_bytes(), b"Digest ", 7) == 0
                && self.do_auth(chal, AuthType::Digest, url, dialog)
            {
                return true;
            }
        }
        for chal in challenges {
            if dlib::strn_ascii_casecmp(chal.as_bytes(), b"Basic ", 6) == 0
                && self.do_auth(chal, AuthType::Basic, url, dialog)
            {
                return true;
            }
        }
        false
    }
}

/// `dStrshred` — efface les octets avant drop (safe : move vers un
/// `Vec<u8>` mutable, `fill(0)`, puis drop — le secret n'est plus en
/// mémoire dans l'allocation retournée).
fn shred_str(s: &mut String) {
    let mut bytes = std::mem::take(s).into_bytes();
    bytes.fill(0);
}

/// `a_Auth_init` — recrée le registre global.
pub fn auth_init() {
    AUTH.with(|a| *a.borrow_mut() = Auth::default());
}

/// `a_Auth_get_auth_str` sur le registre global.
#[must_use]
pub fn auth_get_auth_str(url: &mut DilloUrl, request_uri: &str) -> Option<String> {
    AUTH.with(|a| a.borrow_mut().get_auth_str(url, request_uri))
}

/// `a_Auth_do_auth` sur le registre global.
pub fn auth_do_auth(challenges: &[String], url: &mut DilloUrl, dialog: &mut AuthDialog) -> bool {
    AUTH.with(|a| a.borrow_mut().do_auth_list(challenges, url, dialog))
}

/// `a_Auth_freeall` (≙ reset du registre — tests/shutdown).
pub fn auth_freeall() {
    AUTH.with(|a| *a.borrow_mut() = Auth::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> DilloUrl {
        crate::url::url_new(s.as_bytes(), None).expect("url")
    }
    #[test]
    fn token_char_and_unquote() {
        assert!(is_token_char(b'a'));
        assert!(!is_token_char(b' '));
        assert!(!is_token_char(b'"'));
        assert!(!is_token_char(0x80)); // !dIsascii
        let (v, _, q) = unquote_value(b" \"a\\\"b\" rest");
        assert_eq!(v, b"a\"b");
        assert!(q);
        let (v, _, q) = unquote_value(b" tok=x");
        assert_eq!(v, b"tok");
        assert!(!q);
    }

    #[test]
    fn parse_basic_challenge() {
        let p = parse_challenge("Basic realm=\"WallyWorld\"", AuthType::Basic);
        assert!(p.ok);
        assert_eq!(p.realm.as_deref(), Some("WallyWorld"));
        // realm non quoté → warn mais ok quand même
        let p2 = parse_challenge("Basic realm=x", AuthType::Basic);
        assert!(p2.ok);
        // pas de realm → !ok
        let p3 = parse_challenge("Basic other=\"x\"", AuthType::Basic);
        assert!(!p3.ok);
    }

    #[test]
    fn parse_digest_challenge() {
        let p = parse_challenge(
            "Digest realm=\"r\", nonce=\"n\", qop=\"auth,auth-int\", algorithm=MD5, stale=false",
            AuthType::Digest,
        );
        assert!(p.ok);
        assert_eq!(p.qop, DigestQop::Auth);
        assert!(!p.stale);
        // nonce manquant → !ok
        let p2 = parse_challenge("Digest realm=\"r\"", AuthType::Digest);
        assert!(!p2.ok);
        // stale invalide → le cb avorte *ce* parse_token_value, mais
        // le parsing continue (amont retourne 1) → nonce lu → ok, stale=0
        let p3 = parse_challenge(
            "Digest realm=\"r\", stale=maybe, nonce=\"n\"",
            AuthType::Digest,
        );
        assert!(p3.ok);
        assert!(!p3.stale);
    }

    #[test]
    fn path_inside_and_realm_add() {
        assert!(path_is_inside("/a/b", "/a", 2));
        assert!(path_is_inside("/a", "/a", 2));
        assert!(!path_is_inside("/ab", "/a", 2)); // '/' boundary requis
        let mut r = AuthRealm {
            typ: AuthType::Basic,
            name: "r".into(),
            paths: vec!["/a/b".into()],
            authorization: String::new(),
            username: String::new(),
            cnonce: String::new(),
            nonce_count: 0,
            nonce: String::new(),
            opaque: None,
            algorithm: DigestAlgorithm::NotSet,
            domain: String::new(),
            qop: DigestQop::NotSet,
        };
        // "/a" couvre "/a/b" → ce dernier est remplacé
        Auth::realm_add_path(&mut r, "/a/");
        assert_eq!(r.paths, vec!["/a".to_string()]);
    }

    #[test]
    fn basic_flow_end_to_end() {
        auth_freeall();
        let mut url = u("http://h.tld/dir/page");
        let mut dialog = |_: &str, _: &str| Some(("u".to_string(), "p".to_string()));
        let reload = auth_do_auth(&["Basic realm=\"r1\"".to_string()], &mut url, &mut dialog);
        assert!(reload);
        // le realm couvre le chemin complet « /dir/page » (fichier
        // inclus — amont ajoute URL_PATH tel quel)
        let hdr = auth_get_auth_str(&mut u("http://h.tld/dir/page"), "/dir/page").expect("hdr");
        // base64("u:p") = "dTpw"
        assert_eq!(hdr, "Authorization: Basic dTpw\r\n");
        // … et tout ce qui est *sous* ce chemin
        assert!(auth_get_auth_str(&mut u("http://h.tld/dir/page/sub"), "/dir/page/sub").is_some());
        // mais PAS les frères : « /dir/x » n'est pas dans « /dir/page »
        assert!(auth_get_auth_str(&mut u("http://h.tld/dir/x"), "/dir/x").is_none());
        assert!(auth_get_auth_str(&mut u("http://h.tld/other"), "/other").is_none());
        auth_freeall();
    }

    #[test]
    fn realm_update_without_dialog() {
        auth_freeall();
        let calls = RefCell::new(0);
        let mut url = u("http://h.tld/a/1");
        let mut dialog = |_: &str, _: &str| {
            *calls.borrow_mut() += 1;
            Some(("u".to_string(), "p".to_string()))
        };
        assert!(auth_do_auth(
            &["Basic realm=\"r\"".to_string()],
            &mut url,
            &mut dialog
        ));
        assert_eq!(*calls.borrow(), 1);
        // nouveau chemin même realm → pas de dialog, realm étendu
        let mut url2 = u("http://h.tld/b/2");
        assert!(auth_do_auth(
            &["Basic realm=\"r\"".to_string()],
            &mut url2,
            &mut dialog
        ));
        assert_eq!(*calls.borrow(), 1);
        // les deux chemins complets du realm produisent l'en-tête
        assert!(auth_get_auth_str(&mut u("http://h.tld/b/2"), "/b/2").is_some());
        assert!(auth_get_auth_str(&mut u("http://h.tld/a/1"), "/a/1").is_some());
        // … mais pas « /b/x » (non couvert — verbatim : chemins complets)
        assert!(auth_get_auth_str(&mut u("http://h.tld/b/x"), "/b/x").is_none());
        auth_freeall();
    }

    #[test]
    fn digest_preferred_then_basic() {
        auth_freeall();
        let mut url = u("http://h.tld/");
        let mut dialog = |_: &str, _: &str| Some(("u".to_string(), "p".to_string()));
        let chals = vec![
            "Basic realm=\"b\"".to_string(),
            "Digest realm=\"d\", nonce=\"n\", qop=\"auth\"".to_string(),
        ];
        // Digest est tenté en premier (ordre amont)
        assert!(auth_do_auth(&chals, &mut url, &mut dialog));
        let hdr = auth_get_auth_str(&mut u("http://h.tld/"), "/").expect("hdr");
        assert!(hdr.starts_with("Authorization: Digest "), "{hdr}");
        auth_freeall();
    }
}
