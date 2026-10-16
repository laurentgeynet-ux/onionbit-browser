//! Port de `src/domain.c` + `domain.h` — filtre d'inter-domaines
//! (`domainrc`) : règles `origine → destination` + `default
//! deny|accept`, matching `*` / exact ci / `.suffixe`.
//!
//! Amont : état global (`exceptions`, `default_deny`) → ici registre
//! `thread_local` (single-writer main-thread, même portée). `URL_HOST`
//! paresseux impose `&mut DilloUrl` comme amont (macro à effet de bord).

use std::cell::RefCell;

use dillo_common::dlib;
use tracing::{debug, warn};

use crate::url::DilloUrl;

/// `Rule` — exception `origine`/`destination` (patterns).
#[derive(Debug)]
struct Rule {
    origin: String,
    destination: String,
}

/// Registre `domainrc` (≙ les deux globales amont).
#[derive(Debug, Default)]
pub struct Domain {
    exceptions: Vec<Rule>,
    default_deny: bool,
}

thread_local! {
    static DOMAIN: RefCell<Domain> = RefCell::new(Domain::default());
}

/// `strtok(line, " \t")` — tokens non vides séparés par des runs de
/// blanc/tab (strtok saute les délimiteurs initiaux).
fn strtok_tokens(line: &str) -> impl Iterator<Item = &str> {
    line.split([' ', '\t']).filter(|s| !s.is_empty())
}

impl Domain {
    /// `a_Domain_parse` — lignes `strip`, `#` = commentaire. Texte
    /// surnuméraire : avertissement seulement, la règle est quand même
    /// ajoutée (quirk amont). `default` d'action inconnue : warn + inchangé.
    pub fn parse(&mut self, content: &str) {
        debug!("Reading domainrc...");
        for (lineno, raw) in content.lines().enumerate() {
            let lineno = lineno + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = strtok_tokens(line);
            let tok1 = it.next();
            let tok2 = it.next();
            if it.next().is_some() {
                warn!("Domain: Ignoring extraneous text at end of line {lineno}.");
            }
            let Some(t2) = tok2 else {
                warn!("Domain: Not enough fields in line {lineno}.");
                continue;
            };
            let t1 = tok1.unwrap_or_default();
            if dlib::str_ascii_casecmp(t1.as_bytes(), b"default") == 0 {
                if dlib::str_ascii_casecmp(t2.as_bytes(), b"deny") == 0 {
                    self.default_deny = true;
                    debug!("Domain: Default deny.");
                } else if dlib::str_ascii_casecmp(t2.as_bytes(), b"accept") == 0 {
                    self.default_deny = false;
                    debug!("Domain: Default accept.");
                } else {
                    warn!("Domain: Default action \"{t2}\" not recognised.");
                }
            } else {
                debug!("Domain: Exception from {t1} to {t2}.");
                self.exceptions.push(Rule {
                    origin: t1.to_string(),
                    destination: t2.to_string(),
                });
            }
        }
    }

    /// `Domain_match` — `*` matche toujours ; pattern sans `.` initial =
    /// comparaison ci complète ; `.example.org` matche `example.org`
    /// (`diff == -1` → compare au suffixe sans le point) et
    /// `sub.example.org` (`diff >= 0` → suffixe incluant le point).
    fn match_host(host: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if !pattern.starts_with('.') {
            return dlib::str_ascii_casecmp(host.as_bytes(), pattern.as_bytes()) == 0;
        }
        // `host + diff` : arithmétique d'octets comme amont (ci-compare
        // sur le suffixe incluant le point).
        let hl = i64::try_from(host.len()).unwrap_or(i64::MAX);
        let pl = i64::try_from(pattern.len()).unwrap_or(i64::MAX);
        let diff = hl - pl;
        if diff == -1 {
            dlib::str_ascii_casecmp(host.as_bytes(), &pattern.as_bytes()[1..]) == 0
        } else if diff >= 0 {
            let off = usize::try_from(diff).unwrap_or(host.len());
            dlib::str_ascii_casecmp(&host.as_bytes()[off.min(host.len())..], pattern.as_bytes())
                == 0
        } else {
            false
        }
    }

    /// `a_Domain_permit` — short-circuit si rien configuré ; `dest`
    /// sans host : seuls `file:`/local (host vide) et `data:` passent ;
    /// `same_organization` court-circuite ; exception matchée inverse
    /// le défaut (`ret = default_deny`).
    pub fn permit(&self, source: &mut DilloUrl, dest: &mut DilloUrl) -> bool {
        if !self.default_deny && self.exceptions.is_empty() {
            return true;
        }
        let source_host = source.hostname().unwrap_or_default().to_vec();
        let dest_host = dest.hostname().unwrap_or_default().to_vec();

        if dest_host.is_empty() {
            let ret = source_host.is_empty()
                || dest
                    .scheme
                    .as_deref()
                    .is_some_and(|s| dlib::str_ascii_casecmp(s, b"data") == 0);
            if !ret {
                let s = String::from_utf8_lossy(&source_host);
                let d = String::from_utf8_lossy(dest.str_());
                warn!("Domain: DENIED {s} -> {d}.");
            }
            return ret;
        }
        if source.same_organization(dest) {
            return true;
        }
        let mut ret = !self.default_deny;
        for r in &self.exceptions {
            if Self::match_host(&String::from_utf8_lossy(&source_host), &r.origin)
                && Self::match_host(&String::from_utf8_lossy(&dest_host), &r.destination)
            {
                ret = self.default_deny;
                debug!(
                    "Domain: Matched rule from {} to {}.",
                    r.origin, r.destination
                );
                break;
            }
        }
        if !ret {
            let src = if source_host.is_empty() {
                String::from_utf8_lossy(source.str_()).into_owned()
            } else {
                String::from_utf8_lossy(&source_host).into_owned()
            };
            warn!(
                "Domain: DENIED {src} -> {}.",
                String::from_utf8_lossy(&dest_host)
            );
        }
        ret
    }
}

/// `a_Domain_parse` sur le registre global.
pub fn domain_parse(content: &str) {
    DOMAIN.with(|d| d.borrow_mut().parse(content));
}

/// `a_Domain_permit` sur le registre global.
pub fn domain_permit(source: &mut DilloUrl, dest: &mut DilloUrl) -> bool {
    DOMAIN.with(|d| d.borrow().permit(source, dest))
}

/// `a_Domain_freeall` — vidage du registre (tests / shutdown).
pub fn domain_freeall() {
    DOMAIN.with(|d| *d.borrow_mut() = Domain::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> DilloUrl {
        crate::url::url_new(s.as_bytes(), None).expect("url")
    }

    fn with_domain(content: &str, f: impl Fn(&Domain)) {
        domain_freeall();
        let mut d = Domain::default();
        d.parse(content);
        f(&d);
    }

    #[test]
    fn default_open_without_rules() {
        with_domain("", |d| {
            assert!(d.permit(&mut u("http://a.b/"), &mut u("http://c.d/")));
        });
    }

    #[test]
    fn match_host_semantics() {
        assert!(Domain::match_host("anything.com", "*"));
        assert!(Domain::match_host("example.org", "example.org"));
        assert!(!Domain::match_host("example.org", "ample.org"));
        // ".example.org" matche apex + sous-domaines
        assert!(Domain::match_host("example.org", ".example.org"));
        assert!(Domain::match_host("sub.example.org", ".example.org"));
        assert!(Domain::match_host("a.b.example.org", ".example.org"));
        // pas de faux suffixe : "xexample.org" ≠ ".example.org"
        assert!(!Domain::match_host("xexample.org", ".example.org"));
        // ci-insensible
        assert!(Domain::match_host("EXAMPLE.org", ".Example.ORG"));
    }

    #[test]
    fn deny_by_default_with_exception() {
        with_domain("default deny\na.b *\n", |d| {
            assert!(d.permit(&mut u("http://a.b/"), &mut u("http://x.y/")));
            assert!(!d.permit(&mut u("http://c.d/"), &mut u("http://x.y/")));
        });
    }

    #[test]
    fn accept_default_exception_blocks() {
        with_domain("* .blocked.tld\n", |d| {
            assert!(d.permit(&mut u("http://a.b/"), &mut u("http://c.d/")));
            assert!(!d.permit(&mut u("http://a.b/"), &mut u("http://x.blocked.tld/")));
        });
    }

    #[test]
    fn data_scheme_and_empty_hosts() {
        with_domain("default deny\n", |d| {
            // dest data: autorisée même en default deny
            assert!(d.permit(&mut u("http://a.b/"), &mut u("data:text/plain,x")));
            // dest http non matchée → deny
            assert!(!d.permit(&mut u("http://a.b/"), &mut u("http://c.d/")));
        });
    }

    #[test]
    fn extraneous_and_short_lines() {
        // texte en fin de ligne : warn mais règle ajoutée (quirk amont) —
        // le match inverse le défaut → deny (default accept)
        with_domain("a.b c.d extra tokens\nlonely\n", |d| {
            assert_eq!(d.exceptions.len(), 1);
            assert!(!d.permit(&mut u("http://a.b/"), &mut u("http://c.d/")));
            assert!(d.permit(&mut u("http://a.b/"), &mut u("http://x.y/")));
        });
    }

    #[test]
    fn global_registry_roundtrip() {
        domain_freeall();
        domain_parse("default deny\n");
        assert!(!domain_permit(&mut u("http://a.b/"), &mut u("http://c.d/")));
        domain_freeall();
        assert!(domain_permit(&mut u("http://a.b/"), &mut u("http://c.d/")));
    }
}
