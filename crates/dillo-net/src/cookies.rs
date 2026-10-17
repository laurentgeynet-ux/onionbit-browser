//! Port de `src/cookies.c` + `cookies.h` — client cookies : règles
//! `cookiesrc` (`DEFAULT DENY|ACCEPT|ACCEPT_SESSION`, triées domaine
//! le plus long d'abord) + requêtes vers le serveur `cookies.dpi`
//! (commandes `set_cookie`/`get_cookie` via [`crate::dpip`]).
//!
//! Le stockage réel vit dans `cookies.dpi` (plugin, phase DPI) — le
//! canal `a_Dpi_send_blocking_cmd` est **injecté** :
//! `dpi_send(cmd) -> Option<tag>`. Amont : globales `ccontrol`/
//! `default_action`/`disabled` → `Cookies` en `thread_local`.
//! `cookies_init(content)` reçoit le contenu de `cookiesrc` (lecture/
//! création `Cookies_fopen` ≙ `~/.dillo/cookiesrc` : côté appelant,
//! glue fichier de la couche browser).
//!
//! Différence avec `domain.c` : un pattern `.ex.com` NE matche PAS
//! l'apex `ex.com` ici (`diff >= 0` seul — pas de branche `diff==-1`).

use std::cell::RefCell;

use dillo_common::dlib;
use tracing::{debug, warn};

use crate::url::DilloUrl;

/// `CookieControlAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieControlAction {
    Accept,
    AcceptSession,
    Deny,
}

/// `CookieControl` — règle `(action, domain)` triée par longueur
/// de domaine décroissante (premier match = plus spécifique).
#[derive(Debug)]
struct CookieControl {
    action: CookieControlAction,
    domain: String,
}

/// ≙ `ccontrol`/`default_action`/`disabled` globales.
#[derive(Debug)]
pub struct Cookies {
    ccontrol: Vec<CookieControl>,
    default_action: CookieControlAction,
    disabled: bool,
}

thread_local! {
    static COOKIES: RefCell<Cookies> = const {
        RefCell::new(Cookies {
            ccontrol: Vec::new(),
            default_action: CookieControlAction::Deny,
            disabled: true,
        })
    };
}

/// Canal DPI injecté ≙ `a_Dpi_send_blocking_cmd("cookies", cmd)`
/// (retourne le tag de réponse ou `None`).
pub type DpiSend<'a> = dyn FnMut(&str) -> Option<String> + 'a;

impl Cookies {
    /// `a_Cookies_init` + `Cookie_control_init` — `content` =
    /// contenu de `cookiesrc` (`None` ≙ `!stream` → code 2 → disabled).
    /// Retourne le code amont : 0 = parsé+enabled, 1 = parsé mais tout
    /// deny, 2 = pas de contenu.
    pub fn init(&mut self, content: Option<&str>) -> i32 {
        self.disabled = true;
        let Some(content) = content else {
            return 2;
        };
        let mut enabled = false;
        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // domain = run non-espace ; rule = run suivant (verbatim
            // `dIsspace` — le reste de ligne est ignoré)
            let mut it = line.splitn(2, |c: char| dlib::is_space(c as u8));
            let domain = it.next().unwrap_or_default();
            let rule = it
                .next()
                .unwrap_or_default()
                .trim_start_matches(|c: char| dlib::is_space(c as u8))
                .split(|c: char| dlib::is_space(c as u8))
                .next()
                .unwrap_or_default();
            let action = if dlib::str_ascii_casecmp(rule.as_bytes(), b"ACCEPT") == 0 {
                CookieControlAction::Accept
            } else if dlib::str_ascii_casecmp(rule.as_bytes(), b"ACCEPT_SESSION") == 0 {
                CookieControlAction::AcceptSession
            } else if dlib::str_ascii_casecmp(rule.as_bytes(), b"DENY") == 0 {
                CookieControlAction::Deny
            } else {
                warn!("Cookies: rule '{rule}' for domain '{domain}' is not recognised.");
                continue;
            };
            if dlib::str_ascii_casecmp(domain.as_bytes(), b"DEFAULT") == 0 {
                self.default_action = action;
            } else {
                // insertion verbatim : les règles plus longues passent
                // devant (premier match = plus spécifique)
                let pos = self
                    .ccontrol
                    .iter()
                    .position(|cc| domain.len() > cc.domain.len())
                    .unwrap_or(self.ccontrol.len());
                self.ccontrol.insert(
                    pos,
                    CookieControl {
                        action,
                        domain: domain.to_string(),
                    },
                );
            }
            if action != CookieControlAction::Deny {
                enabled = true;
            }
        }
        if enabled {
            debug!("Enabling cookies as from cookiesrc...");
            self.disabled = false;
            0
        } else {
            debug!("Disabling cookies.");
            1
        }
    }

    /// `Cookies_control_check_domain` — `.domain` → suffixe
    /// (`diff >= 0`, point inclus — PAS de match apex), sinon exact ci ;
    /// premier match (le plus long d'abord) ; défaut sinon.
    fn control_check_domain(&self, domain: &[u8]) -> CookieControlAction {
        for cc in &self.ccontrol {
            let cd = cc.domain.as_bytes();
            if cd.first() == Some(&b'.') {
                if domain.len() < cd.len() {
                    continue;
                }
                let diff = domain.len() - cd.len();
                if dlib::str_ascii_casecmp(&domain[diff..], cd) != 0 {
                    continue;
                }
            } else if dlib::str_ascii_casecmp(domain, cd) != 0 {
                continue;
            }
            return cc.action;
        }
        self.default_action
    }

    /// `Cookies_control_check` — sur `URL_HOST(url)`.
    fn control_check(&self, url: &mut DilloUrl) -> CookieControlAction {
        let host = url.hostname().map(<[u8]>::to_vec).unwrap_or_default();
        self.control_check_domain(&host)
    }

    /// `a_Cookies_set` — envoie `set_cookie` au serveur dpi pour
    /// chaque chaîne (en-têtes `Set-Cookie` bruts). DENY → drop.
    pub fn set(
        &mut self,
        cookie_strings: &[String],
        set_url: &mut DilloUrl,
        date: Option<&str>,
        dpi_send: &mut DpiSend,
    ) {
        if self.disabled {
            return;
        }
        if self.control_check(set_url) == CookieControlAction::Deny {
            debug!("Cookies: denied SET");
            return;
        }
        let host = set_url
            .hostname()
            .map(|h| String::from_utf8_lossy(h).into_owned())
            .unwrap_or_default();
        let path = set_url.path.clone().unwrap_or_else(|| b"/".to_vec());
        let path = String::from_utf8_lossy(&path).into_owned();
        for cookie in cookie_strings {
            let cmd = if let Some(d) = date {
                crate::dpip::build_cmd(
                    "cmd=%s cookie=%s host=%s path=%s date=%s",
                    &["set_cookie", cookie, &host, &path, d],
                )
            } else {
                crate::dpip::build_cmd(
                    "cmd=%s cookie=%s host=%s path=%s",
                    &["set_cookie", cookie, &host, &path],
                )
            };
            let Some(cmd) = cmd else { continue };
            debug!("Cookies.c: a_Cookies_set \n\t \"{cmd}\"");
            let _ = dpi_send(&cmd);
        }
    }

    /// `a_Cookies_get_query` — DENY → `""` ; requester cross-org sur
    /// requête non-racine → `""` ; sinon `get_cookie` dpi → attribut
    /// `cookie` de la réponse (`""` si pas de tag).
    #[must_use]
    pub fn get_query(
        &mut self,
        query_url: &mut DilloUrl,
        requester: Option<&mut DilloUrl>,
        is_root_url: bool,
        dpi_send: &mut DpiSend,
    ) -> String {
        if self.disabled {
            return String::new();
        }
        if self.control_check(query_url) == CookieControlAction::Deny {
            debug!("Cookies: denied GET");
            return String::new();
        }
        if let Some(req) = requester {
            if !is_root_url && !query_url.same_organization(req) {
                debug!("Cookies: not sent for non-root request");
                return String::new();
            }
        }
        let host = query_url
            .hostname()
            .map(|h| String::from_utf8_lossy(h).into_owned())
            .unwrap_or_default();
        let scheme = query_url
            .scheme
            .clone()
            .map(|s| String::from_utf8_lossy(&s).into_owned())
            .unwrap_or_default();
        let path = query_url.path.clone().unwrap_or_else(|| b"/".to_vec());
        let path = String::from_utf8_lossy(&path).into_owned();
        let Some(cmd) = crate::dpip::build_cmd(
            "cmd=%s scheme=%s host=%s path=%s",
            &["get_cookie", &scheme, &host, &path],
        ) else {
            return String::new();
        };
        dpi_send(&cmd)
            .and_then(|tag| crate::dpip::get_attr(&tag, "cookie"))
            .unwrap_or_default()
    }
}

/// `a_Cookies_init` — `content` ≙ `cookiesrc` lu/créé par l'appelant.
/// Retourne le code amont (0 enabled / 1 parsed-deny / 2 no file).
#[must_use]
pub fn cookies_init(content: Option<&str>) -> i32 {
    COOKIES.with(|c| c.borrow_mut().init(content))
}

/// `a_Cookies_set`.
pub fn cookies_set(
    cookie_strings: &[String],
    set_url: &mut DilloUrl,
    date: Option<&str>,
    dpi_send: &mut DpiSend,
) {
    COOKIES.with(|c| c.borrow_mut().set(cookie_strings, set_url, date, dpi_send));
}

/// `a_Cookies_get_query`.
#[must_use]
pub fn cookies_get_query(
    query_url: &mut DilloUrl,
    requester: Option<&mut DilloUrl>,
    is_root_url: bool,
    dpi_send: &mut DpiSend,
) -> String {
    COOKIES.with(|c| {
        c.borrow_mut()
            .get_query(query_url, requester, is_root_url, dpi_send)
    })
}

/// `a_Cookies_freeall` — reset du registre (amont no-op + mémoire).
pub fn cookies_freeall() {
    COOKIES.with(|c| {
        *c.borrow_mut() = Cookies {
            ccontrol: Vec::new(),
            default_action: CookieControlAction::Deny,
            disabled: true,
        };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> DilloUrl {
        crate::url::url_new(s.as_bytes(), None).expect("url")
    }

    fn no_dpi() -> impl FnMut(&str) -> Option<String> {
        |_| None
    }

    fn cookies_check(url: &mut DilloUrl) -> CookieControlAction {
        COOKIES.with(|c| c.borrow_mut().control_check(url))
    }

    #[test]
    fn rules_parse_and_longest_first() {
        cookies_freeall();
        let rc = cookies_init(Some("DEFAULT DENY\n.example.org ACCEPT\n"));
        assert_eq!(rc, 0);
        // `.example.org` → suffixe seul : l'apex n'est PAS matché
        // (diff >= 0 requis — différence avec domain.c)
        assert!(matches!(
            cookies_check(&mut u("http://example.org/")),
            CookieControlAction::Deny
        ));
        assert!(matches!(
            cookies_check(&mut u("http://sub.example.org/")),
            CookieControlAction::Accept
        ));
        cookies_freeall();
        // la règle la plus longue passe d'abord
        let _ = cookies_init(Some(
            "DEFAULT DENY\n.example.org ACCEPT\nsub.example.org DENY\n",
        ));
        assert!(matches!(
            cookies_check(&mut u("http://sub.example.org/")),
            CookieControlAction::Deny
        ));
        assert!(matches!(
            cookies_check(&mut u("http://other.example.org/")),
            CookieControlAction::Accept
        ));
        cookies_freeall();
    }

    #[test]
    fn disabled_or_deny_gives_empty_query() {
        cookies_freeall();
        let mut dpi = no_dpi();
        // disabled (pas d'init) → ""
        assert_eq!(
            cookies_get_query(&mut u("http://a.b/"), None, true, &mut dpi),
            ""
        );
        let _ = cookies_init(Some("DEFAULT DENY\n"));
        assert_eq!(
            cookies_get_query(&mut u("http://a.b/"), None, true, &mut dpi),
            ""
        );
        cookies_freeall();
    }

    #[test]
    fn get_query_builds_dpip_cmd() {
        cookies_freeall();
        let _ = cookies_init(Some("DEFAULT ACCEPT\n"));
        let mut got = Vec::new();
        let mut dpi = |cmd: &str| {
            got.push(cmd.to_string());
            Some("<cookie='a=b' '>".to_string())
        };
        let q = cookies_get_query(&mut u("http://h.tld/dir/x"), None, true, &mut dpi);
        assert_eq!(q, "a=b");
        assert_eq!(
            got[0],
            "<cmd='get_cookie' scheme='http' host='h.tld' path='/dir/x' '>"
        );
        cookies_freeall();
    }

    #[test]
    fn cross_org_requester_blocked() {
        cookies_freeall();
        let _ = cookies_init(Some("DEFAULT ACCEPT\n"));
        let mut dpi = |_: &str| Some("<cookie='x=1' '>".to_string());
        // requester cross-organisation sur requête non-racine → ""
        let mut req = u("http://other.tld/");
        assert_eq!(
            cookies_get_query(&mut u("http://h.tld/"), Some(&mut req), false, &mut dpi),
            ""
        );
        // même org → cookie
        let mut req2 = u("http://h.tld/");
        assert_eq!(
            cookies_get_query(&mut u("http://h.tld/"), Some(&mut req2), false, &mut dpi),
            "x=1"
        );
        // cross-org mais racine → cookie
        assert_eq!(
            cookies_get_query(&mut u("http://h.tld/"), Some(&mut req), true, &mut dpi),
            "x=1"
        );
        cookies_freeall();
    }

    #[test]
    fn set_sends_dpip_cmds() {
        cookies_freeall();
        let _ = cookies_init(Some("DEFAULT ACCEPT\n"));
        let mut got = Vec::new();
        let mut dpi = |cmd: &str| {
            got.push(cmd.to_string());
            None
        };
        cookies_set(
            &["a=b".to_string(), "c=d".to_string()],
            &mut u("http://h.tld/p"),
            Some("Mon, 01 Jan 2024"),
            &mut dpi,
        );
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0],
            "<cmd='set_cookie' cookie='a=b' host='h.tld' path='/p' date='Mon, 01 Jan 2024' '>"
        );
        // sans date → 4 args
        cookies_freeall();
        let _ = cookies_init(Some("DEFAULT ACCEPT\n"));
        let mut got2 = Vec::new();
        let mut dpi2 = |cmd: &str| {
            got2.push(cmd.to_string());
            None
        };
        cookies_set(
            &["a=b".to_string()],
            &mut u("http://h.tld/"),
            None,
            &mut dpi2,
        );
        assert_eq!(
            got2[0],
            "<cmd='set_cookie' cookie='a=b' host='h.tld' path='/' '>"
        );
        cookies_freeall();
    }
}
