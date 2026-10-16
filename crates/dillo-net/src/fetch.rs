//! `fetch` minimal pour la tranche verticale (phase 0.5).
//!
//! Port précurseur de `src/IO/http.c`+`a_Nav_*` : `file://` seul
//! (http/https = phase 1 avec tokio/rustls — ADR-0003). La
//! résolution relative suit la sémantique URL amont de façon
//! réduite (base `file://` uniquement).

/// Erreurs de fetch (sera absorbé par l'enum `NetError` phase 1).
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// Schéma non supporté par la tranche verticale.
    #[error("unsupported scheme: {0}")]
    UnsupportedScheme(String),
    /// URL malformée (pas de `file://` ni chemin relatif).
    #[error("malformed url: {0}")]
    Malformed(String),
    /// Erreur disque sur `file://`.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Décodage `%XX` minimal (chemins `file://`).
#[must_use]
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| -> Option<u8> {
                match c {
                    b'0'..=b'9' => Some(c - b'0'),
                    b'a'..=b'f' => Some(c - b'a' + 10),
                    b'A'..=b'F' => Some(c - b'A' + 10),
                    _ => None,
                }
            };
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `fetch(url)` — `file://` seulement ; `http(s)://` → phase 1.
///
/// # Errors
/// `UnsupportedScheme` hors `file://`, `Io` sur fichier absent.
pub fn fetch(url: &str) -> Result<Vec<u8>, FetchError> {
    if let Some(path) = url.strip_prefix("file://") {
        Ok(std::fs::read(percent_decode(path))?)
    } else if let Some(scheme) = url.split("://").next().filter(|s| !s.is_empty()) {
        Err(FetchError::UnsupportedScheme(scheme.to_owned()))
    } else {
        Err(FetchError::Malformed(url.to_owned()))
    }
}

/// Résout `href` contre `base` — sémantique *URL* (pas `PathBuf`,
/// qui produit des `\` sous Windows). Réduit à `file://` : absolu
/// (`/x`) → racine ; relatif → répertoire du dernier `/`.
/// Sémantique complète `url.c` = phase 1.
#[must_use]
pub fn resolve_href(base: &str, href: &str) -> String {
    if href.contains("://") {
        return href.to_owned();
    }
    let Some(rest) = base.strip_prefix("file://") else {
        return href.to_owned();
    };
    if let Some(abs) = href.strip_prefix('/') {
        return format!("file:///{abs}");
    }
    let rest = percent_decode(rest);
    let dir = rest.rfind('/').map_or("", |i| &rest[..=i]);
    format!("file://{dir}{href}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_percent() {
        assert_eq!(percent_decode("/a%20b/c"), "/a b/c");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("trailing%"), "trailing%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn fetch_file_roundtrip() {
        let dir = std::env::temp_dir().join("dillorust_fetch_test");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| unreachable!());
        let p = dir.join("a.html");
        std::fs::write(&p, b"<p>x").unwrap_or_else(|_| unreachable!());
        let url = format!("file://{}", p.display());
        assert_eq!(fetch(&url).unwrap_or_else(|_| unreachable!()), b"<p>x");
        assert!(matches!(
            fetch("http://example.com"),
            Err(FetchError::UnsupportedScheme(_))
        ));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn resolve_relative() {
        assert_eq!(
            resolve_href("file:///d:/site/index.html", "page2.html"),
            "file:///d:/site/page2.html"
        );
        assert_eq!(
            resolve_href("file:///d:/site/index.html", "http://x/"),
            "http://x/"
        );
    }
}
