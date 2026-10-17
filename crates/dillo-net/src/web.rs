//! Port de `src/web.hh`/`web.cc` — `DilloWeb` et dispatch par type MIME.
//!
//! Amont maintient `ValidWebs` (liste des `DilloWeb*` vivants) et
//! `a_Web_dispatch_by_type` choisit le viewer MIME puis fait le glue
//! navigateur (style, fond, titre, progression, `a_Nav_expect_done`).
//!
//! Le struct `DilloWeb` amont est porté par [`crate::http::HttpWeb`] (il
//! sert `http.c`, `cache.c`, `capi.c` et `web.cc`) ; la validité se porte
//! via son flag `valid` (≙ membership dans `ValidWebs`). Le glue
//! navigateur passe par la seam [`WebDispatch`] injectée — `dillo-net` ne
//! connaît ni layout ni UI.

use crate::cache::CaCb;
use crate::http::HttpWeb;

/// `WEB_RootUrl` (web.hh).
pub const WEB_ROOT_URL: u32 = 1;
/// `WEB_Image`.
pub const WEB_IMAGE: u32 = 2;
/// `WEB_Stylesheet`.
pub const WEB_STYLESHEET: u32 = 4;
/// `WEB_Download`.
pub const WEB_DOWNLOAD: u32 = 8;

/// Seam `a_Mime_set_viewer` + glue navigateur amont.
///
/// Amont, pour un `WEB_RootUrl`, le dispatch : choisit le viewer MIME,
/// configure fond/style/navigateur (titre, location, progression) et
/// appelle `a_Nav_expect_done`. Tout cela vit côté `dillo-browser` — la
/// seam reçoit le web (partagé `Rc` — l'état navigateur est keyé sur
/// `web.id`) et retourne la callback cache du viewer (`None` ≙
/// `return -1` : type non affichable).
pub trait WebDispatch {
    /// `a_Web_dispatch_by_type` (partie navigateur) — retourne le viewer
    /// (`Client->Callback`/`CbData` fusionnés dans une closure) ou `None`.
    fn dispatch(&mut self, mime: &str, web: &HttpWeb) -> Option<CaCb>;
}

/// `a_Web_new` — web frais, enregistré dans `ValidWebs` amont (ici :
/// `valid = true` porté par [`HttpWeb`]).
#[must_use]
pub fn web_new(url: crate::url::DilloUrl, requester: Option<crate::url::DilloUrl>) -> HttpWeb {
    HttpWeb::new(url, requester, 0)
}

/// `a_Web_valid` — membership test amont, ici le flag `valid`.
#[must_use]
pub fn web_valid(web: &HttpWeb) -> bool {
    web.valid()
}

/// `a_Web_free` — amont libère url/requester/image/filename et retire le
/// web de `ValidWebs` ; ici `invalidate()` (le `Rc` disparaît avec son
/// dernier détenteur — pas de fuite).
pub fn web_free(web: &HttpWeb) {
    web.invalidate();
}

/// `a_Web_dispatch_by_type`.
///
/// Fidélité conservée côté net : la restriction MIME pour les requêtes
/// non-racine (seul `image/*` est dispatchable — les autres types sont
/// rejetés comme `a_Web_dispatch_by_type` amont). Tout le reste (style,
/// fond, titre, `a_Nav_expect_done`, choix du viewer) est derrière
/// [`WebDispatch`].
pub fn dispatch_by_type(mime: &str, web: &HttpWeb, seam: &mut dyn WebDispatch) -> Option<CaCb> {
    if web.flags & WEB_ROOT_URL == 0 && !mime.starts_with("image/") {
        // Requête non-racine : amont ne dispatch que les `image/*` —
        // le reste est rejeté (enfant non-image).
        return None;
    }
    seam.dispatch(mime, web)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url::url_new;
    use std::cell::Cell;
    use std::rc::Rc;

    struct FakeDispatch {
        calls: Rc<Cell<u32>>,
    }
    impl WebDispatch for FakeDispatch {
        fn dispatch(&mut self, _mime: &str, _web: &HttpWeb) -> Option<CaCb> {
            self.calls.set(self.calls.get() + 1);
            Some(Box::new(|_, _| {}))
        }
    }

    fn web(flags: u32) -> HttpWeb {
        let u = url_new(b"http://example.org/", None).unwrap_or_else(|| unreachable!());
        let mut w = HttpWeb::new(u, None, 0);
        w.flags = flags;
        w
    }

    #[test]
    fn root_url_dispatches_any_mime() {
        let mut d = FakeDispatch {
            calls: Rc::new(Cell::new(0)),
        };
        let w = web(WEB_ROOT_URL);
        assert!(dispatch_by_type("text/html", &w, &mut d).is_some());
        assert_eq!(d.calls.get(), 1);
    }

    #[test]
    fn non_root_rejects_non_image() {
        let calls = Rc::new(Cell::new(0));
        let mut d = FakeDispatch {
            calls: calls.clone(),
        };
        let w = web(WEB_IMAGE);
        // mime non-image → rejet amont (même avec WEB_Image)
        assert!(dispatch_by_type("text/css", &w, &mut d).is_none());
        assert_eq!(calls.get(), 0);
        // image/* → dispatch (le flag du web n'est pas testé amont)
        assert!(dispatch_by_type("image/png", &w, &mut d).is_some());
        assert_eq!(calls.get(), 1);
        // WEB_Stylesheet + image/* → dispatch aussi (filtre = mime)
        let w2 = web(WEB_STYLESHEET);
        assert!(dispatch_by_type("image/png", &w2, &mut d).is_some());
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn web_free_invalidates() {
        let w = web(WEB_ROOT_URL);
        assert!(web_valid(&w));
        web_free(&w);
        assert!(!web_valid(&w));
    }
}
