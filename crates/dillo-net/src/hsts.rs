//! Port de `src/hsts.c` + `hsts.h` — HTTP Strict Transport Security :
//! liste triée (ci-insensible) de politiques `host → expires_at,
//! subdomains`, parsing de l'en-tête `Strict-Transport-Security`,
//! fichier de preload, garde « cert propre ».
//!
//! Amont : `domains`/`hsts_latest_representable_time` globaux → ici
//! registre `thread_local` (single-writer main, portée process). La
//! propreté du certificat (`a_Tls_certificate_is_clean`, état du
//! `TlsEngine`) est **injectée** — `hsts_set(header, url, cert_clean)`.
//! `prefs.http_strict_transport_security` est injectée à `hsts_init`
//! (le pref vit dans `dillo-browser`, couche supérieure).

use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

use dillo_common::dlib;
use tracing::{debug, warn};

use crate::url::DilloUrl;

/// `HstsData_t`.
#[derive(Debug)]
struct HstsData {
    host: String,
    expires_at: i64,
    subdomains: bool,
}

/// ≙ `domains` + `hsts_latest_representable_time`.
#[derive(Debug)]
pub struct Hsts {
    /// Triée ci-insensible sur `host` (≙ `dList_insert_sorted`).
    domains: Vec<HstsData>,
    /// `mktime(2038-01-19 03:14:07 UTC)` ≙ `INT_MAX` — borne du hack
    /// `Hsts_future_time` (débordement `long` sur `tm_sec`).
    latest_representable: i64,
}

thread_local! {
    static HSTS: RefCell<Option<Hsts>> = const { RefCell::new(None) };
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

impl Hsts {
    /// `Hsts_get_policy` — `dList_find_sorted` ci-insensible.
    fn get_policy(&self, host: &str) -> Option<usize> {
        self.domains
            .binary_search_by(|p| cmp_ci(&p.host, host))
            .ok()
    }

    /// `Hsts_remove_policy`.
    fn remove_policy(&mut self, idx: usize) {
        debug!("HSTS: removed policy for {}", self.domains[idx].host);
        self.domains.remove(idx);
    }

    /// `Hsts_future_time` — `now + seconds` saturé à
    /// `latest_representable` ≙ `tm_sec = INT_MAX` puis `mktime`
    /// (amont borne à ~2038 ; les deux = « n'expire jamais »).
    fn future_time(&self, seconds: i64) -> i64 {
        now_unix()
            .checked_add(seconds)
            .filter(|&t| t <= self.latest_representable)
            .unwrap_or(self.latest_representable)
    }

    /// `Hsts_set_policy` — insert-trié ou mise à jour en place.
    fn set_policy(&mut self, host: &str, max_age: i64, subdomains: bool) {
        let exp = self.future_time(max_age);
        debug!(
            "HSTS: {} {host}{}",
            if self.get_policy(host).is_some() {
                "modify"
            } else {
                "add"
            },
            if subdomains { " and subdomains" } else { "" }
        );
        match self.get_policy(host) {
            Some(i) => {
                self.domains[i].subdomains = subdomains;
                self.domains[i].expires_at = exp;
            }
            None => self.domains.push(HstsData {
                host: host.to_string(),
                expires_at: exp,
                subdomains,
            }),
        }
        self.domains.sort_by(|a, b| cmp_ci(&a.host, &b.host));
    }

    /// `a_Hsts_set` — `cert_clean` ≙ `a_Tls_certificate_is_clean(url)`
    /// (injecté : état du `TlsEngine`, non global).
    /// RFC 6797 §14.3 : en-tête ignoré si warnings certificat.
    pub fn set(&mut self, header: &str, url: &mut DilloUrl, cert_clean: bool) {
        let host = url
            .hostname()
            .map(|h| String::from_utf8_lossy(h).into_owned())
            .unwrap_or_default();
        debug!("HSTS header for {host}: {header}");
        if !cert_clean {
            debug!("But there were certificate warnings, so ignore it (!)");
            return;
        }

        let mut p = header;
        let mut max_age: i64 = 0;
        let mut max_age_valid = false;
        let mut subdomains = false;
        while !p.is_empty() {
            let (attr, rest) = parse_attr(p);
            p = rest;
            if dlib::str_ascii_casecmp(attr.as_bytes(), b"max-age") == 0 {
                let (value, r2) = parse_value(p);
                p = r2;
                if value.as_bytes().first().is_some_and(|&c| dlib::is_digit(c)) {
                    // `strtol` + ERANGE → INT_MAX (amont)
                    max_age = strtol10(value.as_bytes()).unwrap_or(i64::from(i32::MAX));
                    max_age_valid = true;
                }
            } else if dlib::str_ascii_casecmp(attr.as_bytes(), b"includeSubDomains") == 0 {
                subdomains = true;
                p = eat_value(p);
            } else if dlib::str_ascii_casecmp(attr.as_bytes(), b"preload") == 0 {
                // hors RFC — google exige `preload` pour sa liste
            } else {
                warn!("HSTS: header contains unknown attribute: '{attr}'");
                p = eat_value(p);
            }
            if let Some(r) = p.strip_prefix(';') {
                p = r;
            }
        }
        if max_age_valid {
            if max_age > 0 {
                self.set_policy(&host, max_age, subdomains);
            } else if let Some(i) = self.get_policy(&host) {
                self.remove_policy(i);
            }
        }
    }

    /// `Hsts_expired`.
    fn expired(&self, i: usize) -> bool {
        let ret = now_unix() > self.domains[i].expires_at;
        if ret {
            debug!("HSTS: expired");
        }
        ret
    }

    /// `a_Hsts_require_https` — match exact puis marche des domaines
    /// parents (`strchr(host+1, '.')` amont → points après l'index 0)
    /// avec `subdomains` requis ; expiré → retiré.
    pub fn require_https(&mut self, host: &str) -> bool {
        let mut ret = false;
        if let Some(i) = self.get_policy(host) {
            debug!("HSTS: matched host {host}");
            if self.expired(i) {
                self.remove_policy(i);
            } else {
                ret = true;
            }
        }
        if !ret {
            // itérer `host+1` puis après chaque '.' — verbatim strchr
            for pos in host
                .char_indices()
                .skip(1)
                .filter(|(_, c)| *c == '.')
                .map(|(i, _)| i)
            {
                let Some(i) = self.get_policy(&host[pos + 1..]) else {
                    continue;
                };
                if !self.domains[i].subdomains {
                    continue;
                }
                debug!(
                    "HSTS: matched {host} under {} subdomain rule",
                    self.domains[i].host
                );
                if self.expired(i) {
                    self.remove_policy(i);
                } else {
                    ret = true;
                    break;
                }
            }
        }
        ret
    }

    /// `Hsts_preload` — lignes `domain true|false` (`ONE_YEAR`).
    pub fn preload(&mut self, content: &str) {
        const ONE_YEAR: i64 = 60 * 60 * 24 * 365;
        for raw in crate::fgets_lines(content) {
            // `dStrstrip` = `isspace()` C (ASCII + \v), pas Unicode.
            let line = raw.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\u{b}');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = line.splitn(2, |c: char| dlib::is_space(c as u8));
            let domain = it.next().unwrap_or_default();
            let sub = it.next().unwrap_or_default().trim_start();
            if dlib::str_ascii_casecmp(sub.as_bytes(), b"true") == 0 {
                self.set_policy(domain, ONE_YEAR, true);
            } else if dlib::str_ascii_casecmp(sub.as_bytes(), b"false") == 0 {
                self.set_policy(domain, ONE_YEAR, false);
            } else {
                warn!("HSTS: format of line not recognized. Ignoring '{line}'.");
            }
        }
    }
}

/// `dStrAsciiCasecmp` → `Ordering` pour `binary_search`/`sort_by`.
fn cmp_ci(a: &str, b: &str) -> std::cmp::Ordering {
    let a = a.as_bytes();
    let b = b.as_bytes();
    for i in 0..a.len().min(b.len()) {
        let c = a[i].to_ascii_lowercase().cmp(&b[i].to_ascii_lowercase());
        if c != std::cmp::Ordering::Equal {
            return c;
        }
    }
    a.len().cmp(&b.len())
}

/// `strtol(v, NULL, 10)` — préfixe de digits, `None` ≙ `ERANGE`
/// (overflow i64 → amont borne à `INT_MAX` ; on borne aussi).
fn strtol10(b: &[u8]) -> Option<i64> {
    let mut v: i64 = 0;
    let mut n = 0usize;
    while n < b.len() && dlib::is_digit(b[n]) {
        v = v.checked_mul(10)?.checked_add(i64::from(b[n] - b'0'))?;
        n += 1;
    }
    Some(v)
}

/// `Hsts_parse_attr` — skip `dIsspace`, `strcspn("=;")`, trim ' '/'\t'
/// final. Retourne (attr, reste).
fn parse_attr(h: &str) -> (String, &str) {
    let h2 = h.trim_start_matches(|c: char| dlib::is_space(c as u8));
    let len = h2
        .bytes()
        .position(|b| b == b'=' || b == b';')
        .unwrap_or(h2.len());
    let (attr, rest) = h2.split_at(len);
    (attr.trim_end_matches([' ', '\t']).to_string(), rest)
}

/// `Hsts_parse_value` — si `=` : skip + spaces, `strcspn(";")`, trim
/// ' '/'\t' final ; sinon valeur vide sur place.
fn parse_value(h: &str) -> (String, &str) {
    if let Some(h2) = h.strip_prefix('=') {
        let h2 = h2.trim_start_matches(|c: char| dlib::is_space(c as u8));
        let len = h2.bytes().position(|b| b == b';').unwrap_or(h2.len());
        let (v, rest) = h2.split_at(len);
        (v.trim_end_matches([' ', '\t']).to_string(), rest)
    } else {
        (String::new(), h)
    }
}

/// `Hsts_eat_value` — si `=` : avance jusqu'à `';'` (non incluse).
fn eat_value(h: &str) -> &str {
    if let Some(h2) = h.strip_prefix('=') {
        match h2.bytes().position(|b| b == b';') {
            Some(len) => &h2[len..],
            None => &h2[h2.len()..],
        }
    } else {
        h
    }
}

/// `a_Hsts_init` — `enabled` ≙ `prefs.http_strict_transport_security`
/// (injecté). Recrée le registre ; preload optionnel.
pub fn hsts_init(enabled: bool, preload: Option<&str>) {
    HSTS.with(|h| {
        if enabled {
            let mut hsts = Hsts {
                domains: Vec::new(),
                // mktime(2038-01-19 03:14:07 UTC) = INT_MAX
                latest_representable: i64::from(i32::MAX),
            };
            if let Some(p) = preload {
                hsts.preload(p);
            }
            *h.borrow_mut() = Some(hsts);
        } else {
            *h.borrow_mut() = None;
        }
    });
}

/// `a_Hsts_set` sur le registre global (no-op si HSTS désactivé —
/// amont : `domains` non créé → les accès planteraient ; le pref gate
/// empêche tout appel. On dégrade en no-op : `dReturn_if_fail`).
pub fn hsts_set(header: &str, url: &mut DilloUrl, cert_clean: bool) {
    HSTS.with(|h| {
        if let Some(hs) = h.borrow_mut().as_mut() {
            hs.set(header, url, cert_clean);
        }
    });
}

/// `a_Hsts_require_https` sur le registre global (faux si désactivé).
#[must_use]
pub fn hsts_require_https(host: &str) -> bool {
    HSTS.with(|h| {
        h.borrow_mut()
            .as_mut()
            .is_some_and(|hs| hs.require_https(host))
    })
}

/// `a_Hsts_freeall`.
pub fn hsts_freeall() {
    HSTS.with(|h| *h.borrow_mut() = None);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> DilloUrl {
        crate::url::url_new(s.as_bytes(), None).expect("url")
    }

    fn fresh(preload: Option<&str>) {
        hsts_freeall();
        hsts_init(true, preload);
    }

    #[test]
    fn set_and_require() {
        fresh(None);
        hsts_set("max-age=100", &mut u("http://ex.com/"), true);
        assert!(hsts_require_https("ex.com"));
        assert!(hsts_require_https("EX.COM")); // ci
        assert!(!hsts_require_https("other.com"));
    }

    #[test]
    fn subdomains_flag() {
        fresh(None);
        hsts_set(
            "max-age=100; includeSubDomains",
            &mut u("http://ex.com/"),
            true,
        );
        assert!(hsts_require_https("sub.ex.com"));
        assert!(hsts_require_https("deep.sub.ex.com"));
        // sans includeSubDomains : pas de match parent
        fresh(None);
        hsts_set("max-age=100", &mut u("http://ex.com/"), true);
        assert!(!hsts_require_https("sub.ex.com"));
    }

    #[test]
    fn max_age_zero_removes() {
        fresh(None);
        hsts_set("max-age=100", &mut u("http://ex.com/"), true);
        assert!(hsts_require_https("ex.com"));
        hsts_set("max-age=0", &mut u("http://ex.com/"), true);
        assert!(!hsts_require_https("ex.com"));
    }

    #[test]
    fn unclean_cert_ignores() {
        fresh(None);
        hsts_set("max-age=100", &mut u("http://ex.com/"), false);
        assert!(!hsts_require_https("ex.com"));
    }

    #[test]
    fn parse_quirks() {
        fresh(None);
        // whitespace + valeurs inconnues + ';' finaux
        hsts_set(
            "  max-age = 50 ; unknownAttr=x ; preload ; includeSubDomains = y",
            &mut u("http://ex.com/"),
            true,
        );
        assert!(hsts_require_https("ex.com"));
        assert!(hsts_require_https("s.ex.com"));
        // max-age non numérique → invalide, rien n'est posé
        fresh(None);
        hsts_set("max-age=abc", &mut u("http://ex.com/"), true);
        assert!(!hsts_require_https("ex.com"));
    }

    #[test]
    fn preload_list() {
        fresh(Some("ex.com true\nother.org false\n# comment\nbadline\n"));
        assert!(hsts_require_https("ex.com"));
        assert!(hsts_require_https("s.ex.com"));
        assert!(hsts_require_https("other.org"));
        assert!(!hsts_require_https("s.other.org"));
    }

    #[test]
    fn expired_policy_removed() {
        fresh(None);
        hsts_set("max-age=1", &mut u("http://ex.com/"), true);
        HSTS.with(|h| {
            if let Some(hs) = h.borrow_mut().as_mut() {
                if let Some(i) = hs.get_policy("ex.com") {
                    hs.domains[i].expires_at = 1; // passé
                }
            }
        });
        assert!(!hsts_require_https("ex.com"));
        HSTS.with(|h| {
            assert!(h
                .borrow()
                .as_ref()
                .is_none_or(|hs| hs.get_policy("ex.com").is_none()));
        });
    }

    #[test]
    fn disabled_is_noop() {
        hsts_freeall(); // enabled=false path
        hsts_set("max-age=100", &mut u("http://ex.com/"), true);
        assert!(!hsts_require_https("ex.com"));
    }
}
