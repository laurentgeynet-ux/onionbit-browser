//! Port of `src/colors.c` — noms de couleurs étendus (`EXTENDED_COLOR`
//! défini amont → table complète, triée pour recherche binaire) et
//! heuristique de couleur « visited » `a_Color_vc`.

use crate::dlib;

/// Table verbatim — ordre alphabétique exigé (recherche binaire amont).
const COLOR_KEYWORD: &[(&str, i32)] = &[
    ("aliceblue", 0x00f0_f8ff),
    ("antiquewhite", 0x00fa_ebd7),
    ("aqua", 0x0000_ffff),
    ("aquamarine", 0x007f_ffd4),
    ("azure", 0x00f0_ffff),
    ("beige", 0x00f5_f5dc),
    ("bisque", 0x00ff_e4c4),
    ("black", 0x0000_0000),
    ("blanchedalmond", 0x00ff_ebcd),
    ("blue", 0x0000_00ff),
    ("blueviolet", 0x008a_2be2),
    ("brown", 0x00a5_2a2a),
    ("burlywood", 0x00de_b887),
    ("cadetblue", 0x005f_9ea0),
    ("chartreuse", 0x007f_ff00),
    ("chocolate", 0x00d2_691e),
    ("coral", 0x00ff_7f50),
    ("cornflowerblue", 0x0064_95ed),
    ("cornsilk", 0x00ff_f8dc),
    ("crimson", 0x00dc_1436),
    ("cyan", 0x0000_ffff),
    ("darkblue", 0x0000_008b),
    ("darkcyan", 0x0000_8b8b),
    ("darkgoldenrod", 0x00b8_860b),
    ("darkgray", 0x00a9_a9a9),
    ("darkgreen", 0x0000_6400),
    ("darkgrey", 0x00a9_a9a9),
    ("darkkhaki", 0x00bd_b76b),
    ("darkmagenta", 0x008b_008b),
    ("darkolivegreen", 0x0055_6b2f),
    ("darkorange", 0x00ff_8c00),
    ("darkorchid", 0x0099_32cc),
    ("darkred", 0x008b_0000),
    ("darksalmon", 0x00e9_967a),
    ("darkseagreen", 0x008f_bc8f),
    ("darkslateblue", 0x0048_3d8b),
    ("darkslategray", 0x002f_4f4f),
    ("darkslategrey", 0x002f_4f4f),
    ("darkturquoise", 0x0000_ced1),
    ("darkviolet", 0x0094_00d3),
    ("deeppink", 0x00ff_1493),
    ("deepskyblue", 0x0000_bfff),
    ("dimgray", 0x0069_6969),
    ("dimgrey", 0x0069_6969),
    ("dodgerblue", 0x001e_90ff),
    ("firebrick", 0x00b2_2222),
    ("floralwhite", 0x00ff_faf0),
    ("forestgreen", 0x0022_8b22),
    ("fuchsia", 0x00ff_00ff),
    ("gainsboro", 0x00dc_dcdc),
    ("ghostwhite", 0x00f8_f8ff),
    ("gold", 0x00ff_d700),
    ("goldenrod", 0x00da_a520),
    ("gray", 0x0080_8080),
    ("green", 0x0000_8000),
    ("greenyellow", 0x00ad_ff2f),
    ("grey", 0x0080_8080),
    ("honeydew", 0x00f0_fff0),
    ("hotpink", 0x00ff_69b4),
    ("indianred", 0x00cd_5c5c),
    ("indigo", 0x004b_0082),
    ("ivory", 0x00ff_fff0),
    ("khaki", 0x00f0_e68c),
    ("lavender", 0x00e6_e6fa),
    ("lavenderblush", 0x00ff_f0f5),
    ("lawngreen", 0x007c_fc00),
    ("lemonchiffon", 0x00ff_facd),
    ("lightblue", 0x00ad_d8e6),
    ("lightcoral", 0x00f0_8080),
    ("lightcyan", 0x00e0_ffff),
    ("lightgoldenrodyellow", 0x00fa_fad2),
    ("lightgray", 0x00d3_d3d3),
    ("lightgreen", 0x0090_ee90),
    ("lightgrey", 0x00d3_d3d3),
    ("lightpink", 0x00ff_b6c1),
    ("lightsalmon", 0x00ff_a07a),
    ("lightseagreen", 0x0020_b2aa),
    ("lightskyblue", 0x0087_cefa),
    ("lightslategray", 0x0077_8899),
    ("lightslategrey", 0x0077_8899),
    ("lightsteelblue", 0x00b0_c4de),
    ("lightyellow", 0x00ff_ffe0),
    ("lime", 0x0000_ff00),
    ("limegreen", 0x0032_cd32),
    ("linen", 0x00fa_f0e6),
    ("magenta", 0x00ff_00ff),
    ("maroon", 0x0080_0000),
    ("mediumaquamarine", 0x0066_cdaa),
    ("mediumblue", 0x0000_00cd),
    ("mediumorchid", 0x00ba_55d3),
    ("mediumpurple", 0x0093_70db),
    ("mediumseagreen", 0x003c_b371),
    ("mediumslateblue", 0x007b_68ee),
    ("mediumspringgreen", 0x0000_fa9a),
    ("mediumturquoise", 0x0048_d1cc),
    ("mediumvioletred", 0x00c7_1585),
    ("midnightblue", 0x0019_1970),
    ("mintcream", 0x00f5_fffa),
    ("mistyrose", 0x00ff_e4e1),
    ("moccasin", 0x00ff_e4b5),
    ("navajowhite", 0x00ff_dead),
    ("navy", 0x0000_0080),
    ("oldlace", 0x00fd_f5e6),
    ("olive", 0x0080_8000),
    ("olivedrab", 0x006b_8e23),
    ("orange", 0x00ff_a500),
    ("orangered", 0x00ff_4500),
    ("orchid", 0x00da_70d6),
    ("palegoldenrod", 0x00ee_e8aa),
    ("palegreen", 0x0098_fb98),
    ("paleturquoise", 0x00af_eeee),
    ("palevioletred", 0x00db_7093),
    ("papayawhip", 0x00ff_efd5),
    ("peachpuff", 0x00ff_dab9),
    ("peru", 0x00cd_853f),
    ("pink", 0x00ff_c0cb),
    ("plum", 0x00dd_a0dd),
    ("powderblue", 0x00b0_e0e6),
    ("purple", 0x0080_0080),
    ("red", 0x00ff_0000),
    ("rosybrown", 0x00bc_8f8f),
    ("royalblue", 0x0041_69e1),
    ("saddlebrown", 0x008b_4513),
    ("salmon", 0x00fa_8072),
    ("sandybrown", 0x00f4_a460),
    ("seagreen", 0x002e_8b57),
    ("seashell", 0x00ff_f5ee),
    ("sienna", 0x00a0_522d),
    ("silver", 0x00c0_c0c0),
    ("skyblue", 0x0087_ceeb),
    ("slateblue", 0x006a_5acd),
    ("slategray", 0x0070_8090),
    ("slategrey", 0x0070_8090),
    ("snow", 0x00ff_fafa),
    ("springgreen", 0x0000_ff7f),
    ("steelblue", 0x0046_82b4),
    ("tan", 0x00d2_b48c),
    ("teal", 0x0000_8080),
    ("thistle", 0x00d8_bfd8),
    ("tomato", 0x00ff_6347),
    ("turquoise", 0x0040_e0d0),
    ("violet", 0x00ee_82ee),
    ("wheat", 0x00f5_deb3),
    ("white", 0x00ff_ffff),
    ("whitesmoke", 0x00f5_f5f5),
    ("yellow", 0x00ff_ff00),
    ("yellowgreen", 0x009a_cd32),
];

/// `Color_parse_hex` — parse `RRGGBB` ou `RGB` (expansion CSS).
/// Retourne `(couleur, err)` ; err=0 si parse ok.
fn parse_hex(s: &[u8], default_color: i32) -> (i32, i32) {
    // strtol base 16 : préfixe de chiffres hexa
    let mut val: i64 = 0;
    let mut n = 0;
    while n < s.len() {
        let d = match s[n] {
            b'0'..=b'9' => i64::from(s[n] - b'0'),
            b'a'..=b'f' => i64::from(s[n] - b'a') + 10,
            b'A'..=b'F' => i64::from(s[n] - b'A') + 10,
            _ => break,
        };
        val = val.saturating_mul(16).saturating_add(d);
        n += 1;
    }
    let mut ret =
        i32::try_from(val.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(i32::MAX);
    let err = if n == 6 {
        0
    } else if n == 3 {
        // #RGB → #RRGGBB
        ret = ((ret & 0xf00) << 12)
            | ((ret & 0xf00) << 8)
            | ((ret & 0x0f0) << 8)
            | ((ret & 0x0f0) << 4)
            | ((ret & 0x00f) << 4)
            | (ret & 0x00f);
        0
    } else {
        ret = default_color;
        1
    };
    (ret, err)
}

/// `a_Color_parse` — `#hex`, `0xhex`, nom étendu, ou hex nu en
/// dernier recours. Retourne `(couleur, err)` : 0 = `#` ou nom
/// reconnu, 1 = hex nu / défaut, 2 = `0x` reconnu.
#[must_use]
pub fn color_parse(s: &[u8], default_color: i32) -> (i32, i32) {
    // skip leading spaces
    let mut i = 0;
    while i < s.len() && dlib::is_space(s[i]) {
        i += 1;
    }
    let cp = &s[i..];

    let (ret_color, st) = if cp.first() == Some(&b'#') {
        parse_hex(&cp[1..], default_color)
    } else if cp.first() == Some(&b'0') && matches!(cp.get(1), Some(b'x' | b'X')) {
        let (c, e) = parse_hex(&cp[2..], default_color);
        (c, if e == 0 { 2 } else { e })
    } else {
        // recherche binaire dans la table triée
        if let Ok(idx) = COLOR_KEYWORD
            .binary_search_by(|&(key, _)| dlib::str_ascii_casecmp(key.as_bytes(), cp).cmp(&0))
        {
            (COLOR_KEYWORD[idx].1, 0)
        } else {
            // hex nu sans '#' → err = 1 même si parsé
            (parse_hex(cp, default_color).0, 1)
        }
    };
    tracing::debug!(s = ?String::from_utf8_lossy(s), color = format_args!("{ret_color:X}"), "color");
    (ret_color, st)
}

/// `Color_distance2` — [0-3], seuil 0x60 par canal.
fn distance2(c1: i32, c2: i32) -> i32 {
    i32::from((c1 & 0xff).abs_diff(c2 & 0xff) >= 0x60)
        + i32::from(((c1 & 0xff00).abs_diff(c2 & 0xff00)) >= 0x6000)
        + i32::from(((c1 & 0xff_0000).abs_diff(c2 & 0xff_0000)) >= 0x60_0000)
}

/// `Color_distance3` — [0-3], seuil 0x40 (moins de contraste requis).
fn distance3(c1: i32, c2: i32) -> i32 {
    i32::from((c1 & 0xff).abs_diff(c2 & 0xff) >= 0x40)
        + i32::from(((c1 & 0xff00).abs_diff(c2 & 0xff00)) >= 0x4000)
        + i32::from(((c1 & 0xff_0000).abs_diff(c2 & 0xff_0000)) >= 0x40_0000)
}

/// `a_Color_vc` — choisit une couleur « visited link » : `candidate`
/// si contraste suffisant vs texte/lien/fond, sinon la meilleure de
/// la liste interne (score verbatim amont).
#[must_use]
pub fn color_vc(candidate: i32, c_txt: i32, c_lnk: i32, c_bg: i32) -> i32 {
    let v = [
        candidate, // purple  darkcyan  darkmagenta olive
        0x80_0080, 0x00_8b8b, 0x8b_008b, 0x80_8000, // darkred   coral     black
        0x8b_0000, 0xff_7f50, 0x00_0000,
    ];
    let mut max_i = 0usize;
    let mut max_score = 0;
    for (i, &vi) in v.iter().enumerate() {
        let d_txt = distance2(c_txt, vi);
        let d_lnk = distance2(c_lnk, vi);
        let d_bg = distance2(c_bg, vi);
        let score = (if d_bg >= 2 { 4 } else { 2 * d_bg })
            + (if d_txt + d_lnk >= 2 { 2 } else { d_txt + d_lnk })
            + i32::from(distance3(c_lnk, vi) >= 1);
        if score >= 7 {
            max_i = i;
            break;
        }
        if score > max_score {
            max_score = score;
            max_i = i;
        }
    }
    v[max_i]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_sorted() {
        // invariant requis par la recherche binaire
        assert!(COLOR_KEYWORD.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn parse_named() {
        assert_eq!(color_parse(b"red", 0), (0xff_0000, 0));
        assert_eq!(color_parse(b"ReD", 0), (0xff_0000, 0)); // ci-insensible
        assert_eq!(color_parse(b"aliceblue", 0), (0xf0_f8ff, 0));
        assert_eq!(color_parse(b"yellowgreen", 0), (0x9a_cd32, 0));
    }

    #[test]
    fn parse_hexes() {
        assert_eq!(color_parse(b"#ff8000", 0), (0xff_8000, 0));
        assert_eq!(color_parse(b"#f80", 0), (0xff_8800, 0)); // RGB → RRGGBB
        assert_eq!(color_parse(b"0x123456", 0), (0x12_3456, 2));
        assert_eq!(color_parse(b"0XABCDEF", 0), (0xAB_CDEF, 2));
        // hex nu → err = 1 même si parsé
        assert_eq!(color_parse(b"ff8000", 0), (0xff_8000, 1));
        // inconnu → hex parse échoue → défaut
        assert_eq!(color_parse(b"notacolor", 0x42), (0x42, 1));
        // # trop court → défaut, err=1
        assert_eq!(color_parse(b"#ff", 0x42), (0x42, 1));
        // espaces en tête
        assert_eq!(color_parse(b"  #ffffff", 0), (0xff_ffff, 0));
    }

    #[test]
    fn visited() {
        // candidat contrasté (blanc sur fond noir) → gardé
        let col = color_vc(0xff_ffff, 0x00_0000, 0x00_00ff, 0x00_0000);
        assert_eq!(col, 0xff_ffff);
        // candidat = couleur de texte → doit en choisir une autre
        let col = color_vc(0x00_0000, 0x00_0000, 0x00_00ff, 0xff_ffff);
        assert_ne!(col, 0x00_0000);
    }
}
