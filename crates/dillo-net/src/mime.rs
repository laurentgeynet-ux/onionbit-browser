//! Port de `src/IO/mime.c` — registre MIME → viewer.
//!
//! Amont : deux listes triées par insertion (`MimeMinItems` =
//! types mineurs `image/png`, `MimeMajItems` = majeurs `text`) de
//! `MimeItem_t{ Name, Data=Viewer_t }`, remplies par `a_Mime_init`
//! avec les viewers Dicache/Html/Plain compilés.
//!
//! Ici : le registre vit dans `dillo-net` (≙ `src/IO/`) mais les
//! viewers sont **injectés** par les couches au-dessus — la table
//! amont est statique dans `a_Mime_init`, la nôtre est remplie par
//! `dillo-images` (`register_image_viewers` ≙ les `Mime_add_minor_type
//! ("image/*", a_Dicache_*_image)`) et `dillo-browser`/`dillo-html`
//! (≙ `a_Html_text`/`a_Plain_text`) à l'init de l'application.

use std::cell::RefCell;
use std::rc::Rc;

use crate::cache::CaCb;
use crate::http::HttpWeb;
use dillo_common::dlib;

/// Retour d'un viewer ≙ `(Call, dw)` amont : la callback cache à
/// attacher au client + le widget produit (opaque pour la couche
/// réseau — le navigateur le récupère via `Any::downcast`).
pub struct ViewerOutcome {
    /// `*Call` amont (`a_Dicache_callback`, `a_Html_callback`…).
    pub call: CaCb,
    /// Valeur de retour du viewer (`a_Image_get_dw`, widget texte…)
    /// — typé `Any` : la couche navigateur connaît le type concret.
    pub view: Option<Box<dyn std::any::Any>>,
}

/// `Viewer_t` — `void* (*)(const char *Type, void *Ptr,
/// CA_Callback_t *Call, void **Data)`.
pub type MimeViewer = Rc<dyn Fn(&str, &Rc<HttpWeb>) -> Option<ViewerOutcome>>;

/// `MimeItem_t`.
struct MimeItem {
    /// `Name`.
    name: String,
    /// `Data`.
    viewer: MimeViewer,
}

thread_local! {
    /// `MimeMinItems` — viewers de types mineurs (`image/png`…).
    static MIN_ITEMS: RefCell<Vec<MimeItem>> = const { RefCell::new(Vec::new()) };
    /// `MimeMajItems` — viewers de types majeurs (`text`…).
    static MAJ_ITEMS: RefCell<Vec<MimeItem>> = const { RefCell::new(Vec::new()) };
}

/// `Mime_add_minor_type` — ajoute un type mineur (`image/png`).
pub fn mime_add_minor_type(key: &str, viewer: MimeViewer) {
    MIN_ITEMS.with(|v| {
        v.borrow_mut().push(MimeItem {
            name: key.to_string(),
            viewer,
        });
    });
}

/// `Mime_add_major_type` — ajoute un type majeur (`text`).
pub fn mime_add_major_type(key: &str, viewer: MimeViewer) {
    MAJ_ITEMS.with(|v| {
        v.borrow_mut().push(MimeItem {
            name: key.to_string(),
            viewer,
        });
    });
}

/// `Mime_minor_type_fetch`/`Mime_major_type_fetch` —
/// `dStrnAsciiCasecmp(Key, Name, Size) == 0` : préfixe ASCII-insensible
/// de `Size` octets (le nom enregistré est une C-string amont — fin
/// prématurée ≙ NUL → non-match, comportement que `strn_ascii_casecmp`
/// reproduit).
fn fetch(list: &RefCell<Vec<MimeItem>>, key: &str, size: usize) -> Option<MimeViewer> {
    if size == 0 {
        return None;
    }
    let list = list.borrow();
    list.iter().find_map(|it| {
        (dlib::strn_ascii_casecmp(key.as_bytes(), it.name.as_bytes(), size) == 0)
            .then(|| it.viewer.clone())
    })
}

/// `a_Mime_get_viewer` — `MinSize` = préfixe jusqu'à ` ` ou `;`,
/// `MajSize` = position du premier `/` ; mineur d'abord, majeur
/// ensuite.
#[must_use]
pub fn mime_get_viewer(content_type: &str) -> Option<MimeViewer> {
    let bytes = content_type.as_bytes();
    let mut maj_size = 0usize;
    let mut min_size = 0usize;
    for (i, &c) in bytes.iter().enumerate() {
        if c == b' ' || c == b';' {
            min_size = i;
            break;
        }
        if c == b'/' && maj_size == 0 {
            maj_size = i;
        }
        min_size = i + 1;
    }
    MIN_ITEMS
        .with(|l| fetch(l, content_type, min_size))
        .or_else(|| MAJ_ITEMS.with(|l| fetch(l, content_type, maj_size)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer(tag: &'static str) -> MimeViewer {
        Rc::new(move |_: &str, _: &Rc<HttpWeb>| {
            let _ = tag;
            Some(ViewerOutcome {
                call: Box::new(|_, _| {}),
                view: None,
            })
        })
    }

    #[test]
    fn minor_type_lookup() {
        mime_add_minor_type("image/png", viewer("png"));
        assert!(mime_get_viewer("image/png").is_some());
        // Paramètres coupés à `;`/` ` amont.
        assert!(mime_get_viewer("image/png; charset=x").is_some());
        assert!(mime_get_viewer("image/png extra").is_some());
        assert!(mime_get_viewer("image/jpg").is_none());
    }

    #[test]
    fn case_insensitive() {
        mime_add_minor_type("text/css", viewer("css"));
        assert!(mime_get_viewer("TEXT/CSS").is_some());
        assert!(mime_get_viewer("Text/Css; charset=utf-8").is_some());
    }

    #[test]
    fn major_fallback() {
        MAJ_ITEMS.with(|l| l.borrow_mut().clear());
        mime_add_major_type("text", viewer("text"));
        // Aucun mineur ne matche → majeur.
        assert!(mime_get_viewer("text/plain").is_some());
        assert!(mime_get_viewer("text/x-weird").is_some());
        assert!(mime_get_viewer("video/mp4").is_none());
    }

    #[test]
    fn key_shorter_than_name_matches() {
        MIN_ITEMS.with(|l| l.borrow_mut().clear());
        mime_add_minor_type("image/png", viewer("png"));
        // « image/png-junk » ne matche pas (NUL de fin du nom amont).
        assert!(mime_get_viewer("image/png-junk").is_none());
        // Quirk amont : `Size` borne la comparaison à la longueur de
        // la CLÉ — « image/p » matche « image/png » (7 octets égaux).
        assert!(mime_get_viewer("image/p").is_some());
    }
}
