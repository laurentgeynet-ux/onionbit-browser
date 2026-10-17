//! Port de `src/digest.c` + `digest.h` — HTTP Digest authentication
//! (RFC 2617) : MD5 (crate `md-5`, ≙ `md5.c` bundled — crypto vendue,
//! jamais portée), cnonce, A1/A2/response, en-tête `Authorization:
//! Digest` dans l'ordre de champs IE (quirk amont conservé).
//!
//! `AuthRealm_t` est défini dans [`crate::auth`] (partagé).

use std::fmt::Write as _;

use md5::{Digest, Md5};

use crate::auth::{AuthRealm, DigestAlgorithm, DigestQop};
use crate::url::{DilloUrl, URL_POST};

/// `md5hexdigest` — hex lowercase des 16 octets.
#[must_use]
pub fn md5hexdigest(data: &[u8]) -> String {
    let digest = Md5::digest(data);
    let mut out = String::with_capacity(32);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// `a_Digest_create_cnonce` — 32 hex chars. Amont `rand() % 16`
/// (PRNG non-crypto) → `fastrand` : même classe d'entropie.
#[must_use]
pub fn create_cnonce() -> String {
    let mut s = String::with_capacity(32);
    for _ in 0..32 {
        s.push(char::from_digit(fastrand::u32(0..16), 16).unwrap_or('0'));
    }
    s
}

/// `a_Digest_compute_digest` — A1 selon l'algorithme, stocké en hex
/// dans `realm.authorization` (= `H(A1)`). `false` ≙ 0 amont.
pub fn compute_digest(realm: &mut AuthRealm, username: &str, passwd: &str) -> bool {
    let a1 = match realm.algorithm {
        DigestAlgorithm::Md5 | DigestAlgorithm::NotSet => {
            format!("{username}:{}:{passwd}", realm.name)
        }
        DigestAlgorithm::Md5Sess => {
            let ha0 = md5hexdigest(format!("{username}:{}:{passwd}", realm.name).as_bytes());
            format!("{ha0}:{}:{}", realm.nonce, realm.cnonce)
        }
    };
    realm.authorization = md5hexdigest(a1.as_bytes());
    true
}

/// `Digest_create_response` — A2 selon qop, puis `H(A1:nonce:…)`.
fn create_response(realm: &AuthRealm, method: &str, digest_uri: &str, entity: &[u8]) -> String {
    let ha2 = match realm.qop {
        DigestQop::NotSet | DigestQop::Auth => {
            md5hexdigest(format!("{method}:{digest_uri}").as_bytes())
        }
        DigestQop::AuthInt => {
            let hentity = md5hexdigest(entity);
            md5hexdigest(format!("{method}:{digest_uri}:{hentity}").as_bytes())
        }
    };
    let inner = match realm.qop {
        DigestQop::Auth | DigestQop::AuthInt => {
            let qop_str = match realm.qop {
                DigestQop::Auth => "auth",
                _ => "auth-int",
            };
            format!(
                "{}:{}:{:08x}:{}:{}:{ha2}",
                realm.authorization, realm.nonce, realm.nonce_count, realm.cnonce, qop_str
            )
        }
        DigestQop::NotSet => format!("{}:{}:{ha2}", realm.authorization, realm.nonce),
    };
    md5hexdigest(inner.as_bytes())
}

/// `Digest_Dstr_append_token_value` — `token="value"` (quoted) avec
/// `\"` échappé, ou `token=value` ; `delimiter` → `", "` préfixe.
fn append_token_value(out: &mut String, delimiter: bool, token: &str, value: &str, quoted: bool) {
    if delimiter {
        out.push_str(", ");
    }
    out.push_str(token);
    out.push('=');
    if quoted {
        out.push('"');
        for c in value.chars() {
            if c == '"' {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
    } else {
        out.push_str(value);
    }
}

/// `a_Digest_authorization_hdr` — ordre de champs verbatim :
/// `username, realm, nonce, uri, cnonce, nc, algorithm, response,
/// opaque?, qop` (bug de lecteur DVD → ordre IE requis, note amont).
/// `nonce_count` incrémenté avant usage (`%08x`).
#[must_use]
pub fn authorization_hdr(
    realm: &mut AuthRealm,
    url: &mut DilloUrl,
    digest_uri: &str,
) -> Option<String> {
    let method = if url.flags & URL_POST != 0 {
        "POST"
    } else {
        "GET"
    };
    realm.nonce_count += 1;
    let entity = url.data.clone();
    let response = create_response(realm, method, digest_uri, &entity);

    let mut out = String::from("Authorization: Digest ");
    append_token_value(&mut out, false, "username", &realm.username, true);
    append_token_value(&mut out, true, "realm", &realm.name, true);
    append_token_value(&mut out, true, "nonce", &realm.nonce, true);
    append_token_value(&mut out, true, "uri", digest_uri, true);
    if !matches!(realm.qop, DigestQop::NotSet) {
        append_token_value(&mut out, true, "cnonce", &realm.cnonce, true);
        let _ = write!(out, ", nc={:08x}", realm.nonce_count);
    }
    match realm.algorithm {
        DigestAlgorithm::Md5 => append_token_value(&mut out, true, "algorithm", "MD5", false),
        DigestAlgorithm::Md5Sess => {
            append_token_value(&mut out, true, "algorithm", "MD5-sess", false);
        }
        DigestAlgorithm::NotSet => {}
    }
    append_token_value(&mut out, true, "response", &response, true);
    // `if (realm->opaque)` amont — `Some("")` s'affiche, `None` non
    if let Some(opaque) = &realm.opaque {
        append_token_value(&mut out, true, "opaque", opaque, true);
    }
    match realm.qop {
        DigestQop::Auth => append_token_value(&mut out, true, "qop", "auth", true),
        DigestQop::AuthInt => append_token_value(&mut out, true, "qop", "auth-int", true),
        DigestQop::NotSet => {}
    }
    out.push_str("\r\n");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthRealm;

    fn realm() -> AuthRealm {
        AuthRealm {
            typ: crate::auth::AuthType::Digest,
            name: "testrealm@host.com".into(),
            paths: vec![],
            authorization: String::new(),
            username: "Mufasa".into(),
            cnonce: "0a4f113b".into(),
            nonce_count: 0,
            nonce: "dcd98b7102dd2f0e8b11d0f600bfb0c093".into(),
            opaque: None,
            algorithm: DigestAlgorithm::Md5,
            domain: String::new(),
            qop: DigestQop::Auth,
        }
    }

    #[test]
    fn md5_hex() {
        // vecteur RFC 1321
        assert_eq!(
            md5hexdigest(b"The quick brown fox jumps over the lazy dog"),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        assert_eq!(md5hexdigest(b""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn cnonce_shape() {
        let c = create_cnonce();
        assert_eq!(c.len(), 32);
        assert!(c.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    /// Vecteur RFC 2617 §3.5 (exemple canonique).
    #[test]
    fn rfc2617_example() {
        let mut r = realm();
        assert!(compute_digest(&mut r, "Mufasa", "Circle Of Life"));
        assert_eq!(r.authorization, "939e7578ed9e3c518a452acee763bce9");
        // GET /dir/index.html, nc=1, cnonce=0a4f113b, qop=auth
        let mut url =
            crate::url::url_new(b"http://www.nowhere.org/dir/index.html", None).expect("url");
        let hdr = authorization_hdr(&mut r, &mut url, "/dir/index.html").expect("hdr");
        // H(A1):nonce:00000001:cnonce:auth:H(A2) → response RFC
        assert!(
            hdr.contains("response=\"6629fae49393a05397450978507c4ef1\""),
            "{hdr}"
        );
        assert!(hdr.contains("nc=00000001"), "{hdr}");
        assert!(hdr.contains("qop=\"auth\""), "{hdr}");
        assert!(
            hdr.starts_with("Authorization: Digest username=\"Mufasa\""),
            "{hdr}"
        );
        assert!(hdr.contains("realm=\"testrealm@host.com\""), "{hdr}");
        assert!(hdr.ends_with("\r\n"), "{hdr}");
    }

    #[test]
    fn no_qop_no_nc() {
        let mut r = realm();
        r.qop = DigestQop::NotSet;
        assert!(compute_digest(&mut r, "u", "p"));
        let mut url = crate::url::url_new(b"http://h/", None).expect("u");
        let hdr = authorization_hdr(&mut r, &mut url, "/").expect("h");
        assert!(!hdr.contains("cnonce"), "{hdr}");
        assert!(!hdr.contains("nc="), "{hdr}");
        assert!(!hdr.contains("qop"), "{hdr}");
    }
}
