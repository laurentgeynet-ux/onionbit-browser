//! Port de `src/history.c` + `history.h` — historique linéaire global
//! (fournit les index `uidx` de la pile de navigation).
//!
//! Amont : tableau global `history[]` → ici `History` en `thread_local`
//! (main-thread, portée process — même sémantique). `dReturn_if_fail`
//! → `Option` (dégradé release identique).

use std::cell::RefCell;

use tracing::{debug, error};

use crate::url::DilloUrl;

/// `H_Item`.
#[derive(Debug)]
struct HItem {
    url: DilloUrl,
    title: Option<String>,
}

/// ≙ `history`/`history_size` (`size_max` = capacité implicite).
#[derive(Debug, Default)]
pub struct History {
    items: Vec<HItem>,
}

thread_local! {
    static HISTORY: RefCell<History> = RefCell::new(History::default());
}

/// `strcmp(URL_FRAGMENT(a), URL_FRAGMENT(b))` — `NPTR2STR` rend ""
/// pour NULL : `x` et `x#` (fragment absent vs vide) dédupliquent
/// amont (`a_History_add_url`).
fn frag_eq(a: &DilloUrl, b: &DilloUrl) -> bool {
    a.fragment.as_deref().unwrap_or(b"") == b.fragment.as_deref().unwrap_or(b"")
}

impl History {
    /// `a_History_add_url` — dédup par `a_Url_cmp` **et** fragment ;
    /// retourne l'index (0-based ≙ `idx` amont).
    pub fn add_url(&mut self, url: &mut DilloUrl) -> i32 {
        for (i, it) in self.items.iter().enumerate() {
            if it.url.url_cmp(url) == 0 && frag_eq(&it.url, url) {
                debug!("FOUND at idx={i}");
                return i32::try_from(i).unwrap_or(i32::MAX);
            }
        }
        let idx = self.items.len();
        self.items.push(HItem {
            url: url.clone(),
            title: None,
        });
        debug!("ADDED at idx={idx}");
        i32::try_from(idx).unwrap_or(i32::MAX)
    }

    /// `a_History_get_url` — clone (les callers amont ne mutent pas ;
    /// `dReturn_val_if_fail` → `Option`).
    #[must_use]
    pub fn get_url(&self, idx: i32) -> Option<DilloUrl> {
        usize::try_from(idx)
            .ok()
            .and_then(|i| self.items.get(i))
            .map(|it| it.url.clone())
    }

    /// `a_History_get_title` — `force` retombe sur `URL_STR`.
    #[must_use]
    pub fn get_title(&self, idx: i32, force: bool) -> Option<String> {
        let it = usize::try_from(idx).ok().and_then(|i| self.items.get(i))?;
        if let Some(t) = &it.title {
            Some(t.clone())
        } else if force {
            let mut u = it.url.clone();
            Some(String::from_utf8_lossy(u.str_()).into_owned())
        } else {
            None
        }
    }

    /// `a_History_get_title_by_url` — scan `a_Url_cmp`, `force` →
    /// `URL_STR_(url)` (l'url passée, pas celle de l'historique —
    // verbatim amont).
    #[must_use]
    pub fn get_title_by_url(&self, url: &mut DilloUrl, force: bool) -> Option<String> {
        let found = self.items.iter().find(|it| it.url.url_cmp(url) == 0);
        if let Some(it) = found.filter(|it| it.title.is_some()) {
            it.title.clone()
        } else if force {
            Some(String::from_utf8_lossy(url.str_()).into_owned())
        } else {
            None
        }
    }

    /// `a_History_set_title_by_url` — scan **inverse** (le plus récent),
    /// `MSG_ERR` si absent.
    pub fn set_title_by_url(&mut self, url: &mut DilloUrl, title: &str) {
        let pos = self.items.iter().rposition(|it| it.url.url_cmp(url) == 0);
        if let Some(i) = pos {
            self.items[i].title = Some(title.to_string());
        } else {
            error!(
                "a_History_set_title_by_url: {} not found",
                String::from_utf8_lossy(url.str_())
            );
        }
    }
}

/// `a_History_add_url` sur le registre global.
pub fn history_add_url(url: &mut DilloUrl) -> i32 {
    HISTORY.with(|h| h.borrow_mut().add_url(url))
}

/// `a_History_get_url`.
#[must_use]
pub fn history_get_url(idx: i32) -> Option<DilloUrl> {
    HISTORY.with(|h| h.borrow().get_url(idx))
}

/// `a_History_get_title`.
#[must_use]
pub fn history_get_title(idx: i32, force: bool) -> Option<String> {
    HISTORY.with(|h| h.borrow().get_title(idx, force))
}

/// `a_History_get_title_by_url`.
#[must_use]
pub fn history_get_title_by_url(url: &mut DilloUrl, force: bool) -> Option<String> {
    HISTORY.with(|h| h.borrow().get_title_by_url(url, force))
}

/// `a_History_set_title_by_url`.
pub fn history_set_title_by_url(url: &mut DilloUrl, title: &str) {
    HISTORY.with(|h| h.borrow_mut().set_title_by_url(url, title));
}

/// `a_History_freeall`.
pub fn history_freeall() {
    HISTORY.with(|h| *h.borrow_mut() = History::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> DilloUrl {
        crate::url::url_new(s.as_bytes(), None).expect("url")
    }

    #[test]
    fn add_dedup_and_fragment() {
        history_freeall();
        let i1 = history_add_url(&mut u("http://a.b/"));
        let i2 = history_add_url(&mut u("http://a.b/")); // dedup
        let i3 = history_add_url(&mut u("http://a.b/#x")); // fragment ≠ → nouveau
        assert_eq!(i1, i2);
        assert_ne!(i1, i3);
        assert_eq!(history_get_url(i3).expect("u").str_(), b"http://a.b/#x");
        // A02-D2 : `URL_FRAGMENT` = NPTR2STR → fragment absent ≡ ""
        // — `x#` (fragment vide) déduplique avec `x` amont.
        assert_eq!(history_add_url(&mut u("http://a.b/#")), i1);
        history_freeall();
    }

    #[test]
    fn title_force_fallback() {
        history_freeall();
        let i = history_add_url(&mut u("http://a.b/"));
        assert_eq!(history_get_title(i, false), None);
        assert_eq!(history_get_title(i, true).as_deref(), Some("http://a.b/"));
        history_set_title_by_url(&mut u("http://a.b/"), "Titre");
        assert_eq!(history_get_title(i, false).as_deref(), Some("Titre"));
        assert_eq!(
            history_get_title_by_url(&mut u("http://a.b/"), false).as_deref(),
            Some("Titre")
        );
        // url absente + force → URL_STR de l'url demandée
        assert_eq!(
            history_get_title_by_url(&mut u("http://none/"), true).as_deref(),
            Some("http://none/")
        );
        history_freeall();
    }

    #[test]
    fn set_title_latest_wins() {
        history_freeall();
        history_add_url(&mut u("http://a.b/1"));
        history_add_url(&mut u("http://a.b/2"));
        history_set_title_by_url(&mut u("http://a.b/2"), "T2");
        assert_eq!(history_get_title(0, false), None);
        assert_eq!(history_get_title(1, false).as_deref(), Some("T2"));
        // out-of-range → None (dReturn amont)
        assert_eq!(history_get_title(9, true), None);
        assert!(history_get_url(-1).is_none());
        history_freeall();
    }
}
