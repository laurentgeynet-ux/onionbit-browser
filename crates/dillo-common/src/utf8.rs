//! Port of `src/utf8.cc` + `src/utf8.hh`.
//!
//! Utilitaires UTF-8 de `src/` — amont encapsule les `fl_utf8*` de
//! FLTK ; sémantique lenient identique à `lout::unicode` (séquence
//! invalide → octet brut ISO-8859-1/CP1252). `encode` reproduit les
//! formes historiques 1..=6 octets de `fl_utf8encode`.

/// `utf8_replacement_char` — U+FFFD.
pub const UTF8_REPLACEMENT_CHAR: &[u8] = b"\xEF\xBF\xBD";

/// `utf8_zero_width_space` — U+200B.
pub const UTF8_ZERO_WIDTH_SPACE: &[u8] = b"\xE2\x80\x8B";

/// `a_Utf8_end_of_char(str, i)` — index du dernier octet du
/// caractère UTF-8 contenant l'index `i` (port verbatim : au plus
/// 4 octets au total).
#[must_use]
pub fn end_of_char(s: &[u8], i: usize) -> usize {
    let mut i = i;
    if !s.is_empty() && i < s.len() && (s[i] & 0x80) != 0 {
        let mut internal_bytes = usize::from((s[i] & 0x40) == 0);
        while i + 1 < s.len() && (s[i + 1] & 0xc0) == 0x80 {
            internal_bytes += 1;
            if internal_bytes >= 4 {
                break;
            }
            i += 1;
        }
    }
    i
}

/// `a_Utf8_decode` + `fl_utf8decode` — décode le caractère à `pos`
/// (borne `end`). Séquence invalide → octet brut (fallback amont).
/// Retourne `(codepoint, len)`.
#[must_use]
pub fn decode(s: &[u8], pos: usize, end: usize) -> (i32, usize) {
    let avail = end.min(s.len()) - pos.min(end.min(s.len()));
    if avail == 0 {
        return (0, 0);
    }
    let b = |k: usize| -> i32 { i32::from(s[pos + k]) };
    let b0 = s[pos];
    if (b0 & 0x80) == 0 {
        (b(0), 1)
    } else if avail >= 2 && (b0 & 0xe0) == 0xc0 && (s[pos + 1] & 0xc0) == 0x80 {
        (((b(0) & 0x1f) << 6) | (b(1) & 0x3f), 2)
    } else if avail >= 3
        && (b0 & 0xf0) == 0xe0
        && (s[pos + 1] & 0xc0) == 0x80
        && (s[pos + 2] & 0xc0) == 0x80
    {
        (
            ((b(0) & 0x0f) << 12) | ((b(1) & 0x3f) << 6) | (b(2) & 0x3f),
            3,
        )
    } else if avail >= 4
        && (b0 & 0xf8) == 0xf0
        && (s[pos + 1] & 0xc0) == 0x80
        && (s[pos + 2] & 0xc0) == 0x80
        && (s[pos + 3] & 0xc0) == 0x80
    {
        (
            ((b(0) & 0x0f) << 18) | ((b(1) & 0x3f) << 12) | ((b(2) & 0x3f) << 6) | (b(3) & 0x3f),
            4,
        )
    } else {
        // séquence invalide → octet brut (ISO-8859-1/CP1252 amont)
        (b(0), 1)
    }
}

/// `a_Utf8_encode` + `fl_utf8encode` — écrit l'encodage UTF-8 de
/// `ucs` dans `buf` (formes historiques 1..=6 octets, sans check de
/// surrogate comme amont). Retourne les octets écrits (0 si buf court).
#[must_use]
pub fn encode(ucs: u32, buf: &mut [u8]) -> usize {
    let put = |buf: &mut [u8], i: usize, v: u8| {
        if i < buf.len() {
            buf[i] = v;
        }
    };
    #[allow(clippy::cast_possible_truncation)]
    if ucs < 0x80 {
        put(buf, 0, ucs as u8);
        1
    } else if ucs < 0x800 {
        put(buf, 0, 0xc0 | (ucs >> 6) as u8);
        put(buf, 1, 0x80 | (ucs & 0x3f) as u8);
        2
    } else if ucs < 0x1_0000 {
        put(buf, 0, 0xe0 | (ucs >> 12) as u8);
        put(buf, 1, 0x80 | ((ucs >> 6) & 0x3f) as u8);
        put(buf, 2, 0x80 | (ucs & 0x3f) as u8);
        3
    } else if ucs < 0x20_0000 {
        put(buf, 0, 0xf0 | (ucs >> 18) as u8);
        put(buf, 1, 0x80 | ((ucs >> 12) & 0x3f) as u8);
        put(buf, 2, 0x80 | ((ucs >> 6) & 0x3f) as u8);
        put(buf, 3, 0x80 | (ucs & 0x3f) as u8);
        4
    } else if ucs < 0x400_0000 {
        put(buf, 0, 0xf8 | (ucs >> 24) as u8);
        put(buf, 1, 0x80 | ((ucs >> 18) & 0x3f) as u8);
        put(buf, 2, 0x80 | ((ucs >> 12) & 0x3f) as u8);
        put(buf, 3, 0x80 | ((ucs >> 6) & 0x3f) as u8);
        put(buf, 4, 0x80 | (ucs & 0x3f) as u8);
        5
    } else {
        put(buf, 0, 0xfc | (ucs >> 30) as u8);
        put(buf, 1, 0x80 | ((ucs >> 24) & 0x3f) as u8);
        put(buf, 2, 0x80 | ((ucs >> 18) & 0x3f) as u8);
        put(buf, 3, 0x80 | ((ucs >> 12) & 0x3f) as u8);
        put(buf, 4, 0x80 | ((ucs >> 6) & 0x3f) as u8);
        put(buf, 5, 0x80 | (ucs & 0x3f) as u8);
        6
    }
}

/// `a_Utf8_test` + `fl_utf8test` — 0 illégal, 1 tout ASCII,
/// 2 tout < 0x800, 3 tout < 0x10000, 4 sinon.
#[must_use]
pub fn test(src: &[u8]) -> i32 {
    let mut ret = 1;
    let mut i = 0;
    while i < src.len() {
        if src[i] < 0x80 {
            i += 1;
            continue;
        }
        let b = src[i];
        if (b & 0xc0) != 0xc0 {
            return 0; // continuation orpheline
        }
        let n = if (b & 0xe0) == 0xc0 {
            1
        } else if (b & 0xf0) == 0xe0 {
            2
        } else if (b & 0xf8) == 0xf0 {
            3
        } else if (b & 0xfc) == 0xf8 {
            4
        } else if (b & 0xfe) == 0xfc {
            5
        } else {
            return 0; // 0xfe/0xff illégaux
        };
        ret = ret.max((n + 1).min(4));
        for _ in 0..n {
            i += 1;
            if i >= src.len() || (src[i] & 0xc0) != 0x80 {
                return 0;
            }
        }
        i += 1;
    }
    ret
}

/// `a_Utf8_ideographic` — tr14#ID + guesses amont :
/// `0x2e80..=0xa4cf`, `0xf900..=0xfaff`, `0xff00..=0xff9f`.
/// Retourne `(is_ideographic, len)`.
#[must_use]
pub fn ideographic(s: &[u8], pos: usize, end: usize) -> (bool, usize) {
    if pos < s.len() && s[pos] >= 0xe2 {
        // char >= U+2000
        let (unicode, len) = decode(s, pos, end);
        let is = unicode >= 0x2e80
            && (unicode <= 0xa4cf
                || (0xf900..=0xfaff).contains(&unicode)
                || (0xff00..=0xff9f).contains(&unicode));
        (is, len)
    } else {
        (false, end_of_char(s, pos).saturating_sub(pos) + 1)
    }
}

/// `a_Utf8_combining_char` — plages verbatim.
#[must_use]
pub fn combining_char(unicode: i32) -> bool {
    (0x0300..=0x036f).contains(&unicode)
        || (0x1dc0..=0x1dff).contains(&unicode)
        || (0x20d0..=0x20ff).contains(&unicode)
        || (0xfe20..=0xfe2f).contains(&unicode)
}

/// `a_Utf8_char_count` + `fl_utf_nb_char` — nombre de caractères =
/// octets non-continuation (`b & 0xc0 != 0x80`).
#[must_use]
pub fn char_count(s: &[u8]) -> usize {
    s.iter().filter(|&&b| (b & 0xc0) != 0x80).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_of_char_walks() {
        let s = "aé€".as_bytes(); // 1+2+3 octets
        assert_eq!(end_of_char(s, 0), 0);
        assert_eq!(end_of_char(s, 1), 2); // é → index du dernier octet
        assert_eq!(end_of_char(s, 2), 2); // à l'intérieur du char
        assert_eq!(end_of_char(s, 3), 5); // €
        assert_eq!(end_of_char("é".as_bytes(), 0), 1);
        // borne finale : lead seul sans continuation → i reste
        assert_eq!(end_of_char(&[0xC3], 0), 0);
    }

    #[test]
    fn decode_lenient() {
        assert_eq!(decode("é".as_bytes(), 0, 2), (0xE9, 2));
        assert_eq!(decode("€".as_bytes(), 0, 3), (0x20AC, 3));
        assert_eq!(decode(&[0xFF], 0, 1), (0xFF, 1)); // fallback latin-1
        assert_eq!(decode(b"a", 0, 1), (97, 1));
        // borne end : char à cheval → fallback octet
        assert_eq!(decode("€".as_bytes(), 0, 1), (0xE2, 1));
    }

    #[test]
    fn encode_historic() {
        let mut buf = [0u8; 8];
        assert_eq!(encode(0x41, &mut buf), 1);
        assert_eq!(&buf[..1], b"A");
        assert_eq!(encode(0xE9, &mut buf), 2);
        assert_eq!(&buf[..2], "é".as_bytes());
        assert_eq!(encode(0x20AC, &mut buf), 3);
        assert_eq!(&buf[..3], "€".as_bytes());
        // formes 5/6 octets (FLTK historique)
        assert_eq!(encode(0x20_0000, &mut buf), 5);
        assert_eq!(buf[0] & 0xfc, 0xf8);
        assert_eq!(encode(0x400_0000, &mut buf), 6);
        assert_eq!(buf[0] & 0xfe, 0xfc);
    }

    #[test]
    fn test_categories() {
        assert_eq!(test(b"ascii"), 1);
        assert_eq!(test("é".as_bytes()), 2);
        assert_eq!(test("€".as_bytes()), 3);
        assert_eq!(test(&[0xFF]), 0); // illégal
        assert_eq!(test(&[0x80]), 0); // continuation orpheline
                                      // lead sans suite → 0
        assert_eq!(test(&[0xC3]), 0);
    }

    #[test]
    fn ideographic_ranges() {
        assert!(ideographic("中".as_bytes(), 0, 3).0); // U+4E2D ∈ range
        assert!(!ideographic("€".as_bytes(), 0, 3).0); // U+20AC hors
        assert!(!ideographic(b"a", 0, 1).0);
        assert_eq!(ideographic(b"a", 0, 1).1, 1);
    }

    #[test]
    fn combining() {
        assert!(combining_char(0x0300)); // accent grave combining
        assert!(!combining_char(0xE9));
        assert!(combining_char(0x20d0));
    }

    #[test]
    fn count() {
        assert_eq!(char_count("aé€".as_bytes()), 3);
        assert_eq!(char_count(&[0xFF, 0x80]), 1); // compte non-continuation
    }
}
