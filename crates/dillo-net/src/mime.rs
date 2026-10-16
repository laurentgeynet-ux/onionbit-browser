//! Port of `src/IO/mime.c` + `mime.h` — registre MIME → viewer.
//!
//! Upstream associe un `content_type` (éventuellement suffixé de
//! paramètres `;…`) à une fonction `Viewer_t`. Ici les viewers sont des
//! identifiants [`ViewerId`] : `a_Html_text`, `a_Plain_text` et les
//! `a_Dicache_*_image` ne sont pas encore portés — le dispatch vers les
//! modules réels se branchera quand `html.cc`/`dicache.c` le seront.
//!
//! Sémantiques amont reproduites :
//! - `MinSize`/`MajSize` sont calculés dans **un seul scan** jusqu'au
//!   premier `' '`/`';'`/`'\0'` ; `MajSize` = index du 1er `'/'`, sauf
//!   si celui-ci est en position 0 (alors `MajSize` prend le suivant —
//!   `!MajSize` reste vrai tant que l'index est 0).
//! - `dStrnAsciiCasecmp` ne compare que `Size` octets : un nom
//!   enregistré plus long matche encore (et un content-type plus court
//!   peut matcher un nom enregistré par préfixe).

/// `Viewer_t` — identifiant de viewer (mapping des fonctions amont).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewerId {
    /// `a_Html_text` — `text/html`, `application/xhtml+xml`.
    HtmlText,
    /// `a_Plain_text` — `application/xml`, `rss`/`atom`/`json`,
    /// et type majeur `text/*`.
    PlainText,
    /// `a_Dicache_gif_image`.
    ImageGif,
    /// `a_Dicache_jpeg_image` (`jpeg`/`pjpeg`/`jpg`).
    ImageJpeg,
    /// `a_Dicache_png_image` (`png`/`x-png`).
    ImagePng,
    /// `a_Dicache_webp_image`.
    ImageWebp,
    /// `a_Dicache_svg_image`.
    ImageSvg,
}

/// `MimeMinItems` + `MimeMajItems` (statiques amont → instance).
/// Listes ordonnées, premier match gagne — comme `a_List_add` amont.
#[derive(Debug, Default)]
pub struct MimeRegistry {
    minor: Vec<(&'static str, ViewerId)>,
    major: Vec<(&'static str, ViewerId)>,
}

impl MimeRegistry {
    /// `a_Mime_init` — ordre d'enregistrement verbatim, tous les
    /// `ENABLE_*` activés (gif/jpeg/png/webp/svg + html + plain).
    #[must_use]
    pub fn new() -> Self {
        let mut m = Self::default();
        m.add_minor("image/gif", ViewerId::ImageGif);
        m.add_minor("image/jpeg", ViewerId::ImageJpeg);
        m.add_minor("image/pjpeg", ViewerId::ImageJpeg);
        m.add_minor("image/jpg", ViewerId::ImageJpeg);
        m.add_minor("image/png", ViewerId::ImagePng);
        m.add_minor("image/x-png", ViewerId::ImagePng);
        m.add_minor("image/webp", ViewerId::ImageWebp);
        m.add_minor("image/svg+xml", ViewerId::ImageSvg);
        m.add_minor("text/html", ViewerId::HtmlText);
        m.add_minor("application/xhtml+xml", ViewerId::HtmlText);
        m.add_minor("application/xml", ViewerId::PlainText);
        m.add_minor("application/rss+xml", ViewerId::PlainText);
        m.add_minor("application/atom+xml", ViewerId::PlainText);
        m.add_minor("application/json", ViewerId::PlainText);
        m.add_major("text", ViewerId::PlainText);
        m
    }

    /// `Mime_add_minor_type`.
    pub fn add_minor(&mut self, key: &'static str, viewer: ViewerId) {
        self.minor.push((key, viewer));
    }

    /// `Mime_add_major_type`.
    pub fn add_major(&mut self, key: &'static str, viewer: ViewerId) {
        self.major.push((key, viewer));
    }

    /// `a_Mime_get_viewer` — type spécifique d'abord, type majeur ensuite.
    #[must_use]
    pub fn get_viewer(&self, content_type: &str) -> Option<ViewerId> {
        let b = content_type.as_bytes();
        let mut maj_size = 0usize;
        let mut min_size = b.len();
        for (i, &c) in b.iter().enumerate() {
            if c == b' ' || c == b';' || c == 0 {
                min_size = i;
                break;
            }
            if c == b'/' && maj_size == 0 {
                maj_size = i;
            }
        }
        Self::fetch(&self.minor, b, min_size).or_else(|| Self::fetch(&self.major, b, maj_size))
    }

    /// `Mime_*_type_fetch` — `dStrnAsciiCasecmp` sur `size` octets :
    /// `size` dépasse le nom enregistré → pas de match (le NUL amont
    /// diffère du char de la clé) ; nom plus long → match quand même.
    fn fetch(list: &[(&'static str, ViewerId)], key: &[u8], size: usize) -> Option<ViewerId> {
        if size == 0 {
            return None;
        }
        let k = &key[..size.min(key.len())];
        list.iter().find_map(|(name, v)| {
            let n = name.as_bytes();
            (n.len() >= k.len() && n[..k.len()].eq_ignore_ascii_case(k)).then_some(*v)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> MimeRegistry {
        MimeRegistry::new()
    }

    #[test]
    fn exact_minor_types() {
        let m = reg();
        assert_eq!(m.get_viewer("text/html"), Some(ViewerId::HtmlText));
        assert_eq!(
            m.get_viewer("application/xhtml+xml"),
            Some(ViewerId::HtmlText)
        );
        assert_eq!(m.get_viewer("image/png"), Some(ViewerId::ImagePng));
        assert_eq!(m.get_viewer("image/x-png"), Some(ViewerId::ImagePng));
        assert_eq!(m.get_viewer("image/jpeg"), Some(ViewerId::ImageJpeg));
        assert_eq!(m.get_viewer("image/pjpeg"), Some(ViewerId::ImageJpeg));
        assert_eq!(m.get_viewer("image/jpg"), Some(ViewerId::ImageJpeg));
        assert_eq!(m.get_viewer("image/gif"), Some(ViewerId::ImageGif));
        assert_eq!(m.get_viewer("image/webp"), Some(ViewerId::ImageWebp));
        assert_eq!(m.get_viewer("image/svg+xml"), Some(ViewerId::ImageSvg));
        assert_eq!(m.get_viewer("application/json"), Some(ViewerId::PlainText));
    }

    #[test]
    fn parameters_and_case() {
        let m = reg();
        // `;` et ` ` terminent la comparaison (MinSize)
        assert_eq!(
            m.get_viewer("text/html; charset=utf-8"),
            Some(ViewerId::HtmlText)
        );
        assert_eq!(m.get_viewer("text/html ;x"), Some(ViewerId::HtmlText));
        assert_eq!(m.get_viewer("TEXT/HTML"), Some(ViewerId::HtmlText));
        assert_eq!(m.get_viewer("Image/PNG"), Some(ViewerId::ImagePng));
    }

    #[test]
    fn major_fallback_text() {
        let m = reg();
        // type majeur "text" → PlainText pour tout text/* non enregistré
        assert_eq!(m.get_viewer("text/plain"), Some(ViewerId::PlainText));
        assert_eq!(m.get_viewer("text/csv"), Some(ViewerId::PlainText));
        // majeur inconnu → None
        assert_eq!(m.get_viewer("application/octet-stream"), None);
        assert_eq!(m.get_viewer("video/mp4"), None);
        assert_eq!(m.get_viewer(""), None);
    }

    #[test]
    fn quirks_upstream() {
        let m = reg();
        // `dStrnAsciiCasecmp` prefixe dans les deux sens :
        // "text/ht" matche "text/html" enregistré (7 octets égaux)
        assert_eq!(m.get_viewer("text/ht"), Some(ViewerId::HtmlText));
        // mais un suffixe plus long ne matche pas (NUL vs 'x')
        assert_eq!(m.get_viewer("text/htmlx"), Some(ViewerId::PlainText));
        // '/' initial → MajSize=0 reste vrai → le slash suivant prend
        // la place : "/x/y" → MajSize = index du 2e '/' = 2 → "/x"
        assert_eq!(m.get_viewer("/x/y"), None);
        // pas de '/' du tout → MajSize 0 → fetch major inhibé
        assert_eq!(m.get_viewer("texthtml"), None);
    }
}
