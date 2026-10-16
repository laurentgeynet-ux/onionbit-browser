//! Port of `lout/unicode.hh` + `lout/unicode.cc`.
//!
//! Décodeur UTF-8 **lenient** (séquence invalide → l'octet brut,
//! traité comme ISO-8859-1/Windows-1252 — choix amont à préserver)
//! et table `isAlpha` bitmap couvrant U+0000..U+04FF.

/// Table bitmap `alpha` de `unicode.cc` — donnée amont recopiée
/// à l'identique. Amont déclare `alpha[0x500]` en ne listant que
/// 162 initializers (le reste = 0 en C) ; comme `isAlpha` borne
/// `ch < 0x500` ⇒ index max 159, les 162 entrées suffisent.
#[rustfmt::skip]
static ALPHA: [u8; 162] = [
    // 0000-007F: C0 Controls and Basic Latin
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xfe, 0xff, 0xff, 0x07, 0xfe, 0xff, 0xff, 0x07,
    // 0080-00FF: C1 Controls and Latin-1 Supplement
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xff, 0xff, 0x7f, 0xff, 0xff, 0xff, 0x7f, 0xff,
    // 0100-017F: Latin Extended-A
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    // 0180-024F: Latin Extended-B
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xf0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff,
    // 0250-02AF: IPA Extensions
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff,
    // 02B0-02FF: Spacing Modifier Letters
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00,
    // 0300-036F: Combining Diacritical Marks
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0370-03FF: Greek and Coptic
    0xcf, 0x00, 0x40, 0x7d, 0xff, 0xff, 0xfb, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff,
    // 0400-04FF: Cyrillic
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0x03, 0xfc, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

/// `unicode::isAlpha` — alphabétique au sens table amont
/// (limitée à ch < 0x500, comme amont).
#[must_use]
pub fn is_alpha(ch: i32) -> bool {
    usize::try_from(ch).is_ok_and(|c| c < 0x500 && (ALPHA[c / 8] & (1 << (c & 7))) != 0)
}

/// `unicode::decodeUtf8(s)` — décode le premier caractère de `s`.
/// Retourne le codepoint, ou l'octet brut en séquence invalide
/// (fallback ISO-8859-1 amont).
#[must_use]
pub fn decode_utf8(s: &[u8]) -> i32 {
    decode_utf8_len(s, usize::MAX)
}

/// `unicode::decodeUtf8(s, len)`.
#[must_use]
pub fn decode_utf8_len(s: &[u8], len: usize) -> i32 {
    let len = len.min(s.len());
    let b = |i: usize| -> i32 { i32::from(s[i]) };
    if len >= 1 && (s[0] & 0x80) == 0 {
        b(0)
    } else if len >= 2 && (s[0] & 0xe0) == 0xc0 && (s[1] & 0xc0) == 0x80 {
        ((b(0) & 0x1f) << 6) | (b(1) & 0x3f)
    } else if len >= 3 && (s[0] & 0xf0) == 0xe0 && (s[1] & 0xc0) == 0x80 && (s[2] & 0xc0) == 0x80 {
        ((b(0) & 0x0f) << 12) | ((b(1) & 0x3f) << 6) | (b(2) & 0x3f)
    } else if len >= 4
        && (s[0] & 0xf8) == 0xf0
        && (s[1] & 0xc0) == 0x80
        && (s[2] & 0xc0) == 0x80
        && (s[3] & 0xc0) == 0x80
    {
        ((b(0) & 0x0f) << 18) | ((b(1) & 0x3f) << 12) | ((b(2) & 0x3f) << 6) | (b(3) & 0x3f)
    } else {
        // Treat as ISO-8859-1 / ISO-8859-15 / Windows-1252
        b(0)
    }
}

/// `unicode::nextUtf8Char(s)` — reste de la chaîne après le premier
/// caractère ; `None` quand la chaîne est épuisée (amont : NULL si
/// `r[0] == 0` — pour `&[u8]` : slice vide).
#[must_use]
pub fn next_utf8_char(s: &[u8]) -> Option<&[u8]> {
    next_utf8_char_len(s, usize::MAX)
}

/// `unicode::nextUtf8Char(s, len)` — `len` borne le scan ; `None`
/// quand le prochain caractère dépasserait `len` (amont :
/// `r - s >= len` → NULL).
#[must_use]
pub fn next_utf8_char_len(s: &[u8], len: usize) -> Option<&[u8]> {
    let len = len.min(s.len());
    if s.is_empty() || len == 0 {
        return None;
    }
    let step = if (s[0] & 0x80) == 0 {
        1
    } else if len >= 2 && (s[0] & 0xe0) == 0xc0 && (s[1] & 0xc0) == 0x80 {
        2
    } else if len >= 3 && (s[0] & 0xf0) == 0xe0 && (s[1] & 0xc0) == 0x80 && (s[2] & 0xc0) == 0x80 {
        3
    } else if len >= 4
        && (s[0] & 0xf8) == 0xf0
        && (s[1] & 0xc0) == 0x80
        && (s[2] & 0xc0) == 0x80
        && (s[3] & 0xc0) == 0x80
    {
        4
    } else {
        // invalid UTF-8 sequence: treat as one byte.
        1
    };
    // amont : NULL si r[0]==0 ou r-s >= len → ici : slice vide restante
    // ou avance au-delà de `len`
    if step >= len {
        None
    } else {
        Some(&s[step..])
    }
}

/// `unicode::numUtf8Chars(s)`.
#[must_use]
pub fn num_utf8_chars(s: &[u8]) -> usize {
    let mut n = 0;
    let mut r = s;
    while !r.is_empty() {
        n += 1;
        match next_utf8_char(r) {
            Some(rest) => r = rest,
            None => break,
        }
    }
    n
}

/// `unicode::numUtf8Chars(s, len)` — compte jusqu'au premier
/// caractère dont la largeur ≥ `len` (ce caractère **est** compté ;
/// sémantique amont : `len` borne l'avance d'un char, pas le total
/// consommé — boucle `r = nextUtf8Char(r, len)`).
#[must_use]
pub fn num_utf8_chars_len(s: &[u8], len: usize) -> usize {
    let mut n = 0;
    let mut r = s;
    while len > 0 && !r.is_empty() {
        n += 1;
        match next_utf8_char_len(r, len) {
            Some(rest) => r = rest,
            None => break,
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chaîne du test amont `test/unit/unicode_test.cc` :
    /// `"abcäöüабв−‐"` (äöü = 2B UTF-8, cyrillique 2B, − et ‐ = 3B).
    const T1: &[u8] = "abcäöüабв−\u{2010}".as_bytes();

    #[test]
    fn decode_multibyte() {
        assert_eq!(decode_utf8(b"a"), i32::from(b'a'));
        assert_eq!(decode_utf8("ä".as_bytes()), 0xE4);
        assert_eq!(decode_utf8("−".as_bytes()), 0x2212);
        assert_eq!(decode_utf8("\u{2010}".as_bytes()), 0x2010);
        // fallback latin-1 : continuation seule
        assert_eq!(decode_utf8(&[0xC3]), 0xC3);
        assert_eq!(decode_utf8(&[0xFF]), 0xFF);
    }

    #[test]
    fn next_and_count() {
        // Mêmes positions qu'attendues par le test amont
        assert_eq!(num_utf8_chars(T1), 11);
        let mut count = 0;
        let mut r = T1;
        while !r.is_empty() {
            count += 1;
            match next_utf8_char(r) {
                Some(rest) => r = rest,
                None => break,
            }
        }
        assert_eq!(count, 11);
        // sémantique amont : len borne la largeur d'un char.
        // len=2 → compte 'a','b','c','ä' (le char de largeur 2
        // termine la boucle mais est compté) = 4 ; len=7 → tout (11).
        assert_eq!(num_utf8_chars_len(T1, 2), 4);
        assert_eq!(num_utf8_chars_len(T1, 7), 11);
        assert_eq!(num_utf8_chars_len(T1, 0), 0);
    }

    #[test]
    fn is_alpha_table() {
        assert!(is_alpha(i32::from(b'a')));
        assert!(is_alpha(i32::from(b'Z')));
        assert!(!is_alpha(i32::from(b'0')));
        assert!(!is_alpha(i32::from(b' ')));
        assert!(is_alpha(0xE9)); // é
        assert!(!is_alpha(0x0300)); // combining mark → non alpha
        assert!(is_alpha(0x410)); // cyrillique А
        assert!(!is_alpha(0x500)); // hors table → false
        assert!(!is_alpha(-1));
    }
}
