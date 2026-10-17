//! `html_charrefs.h` — table `Charrefs` generee verbatim
//! (ordre identique → `binary_search` = `bsearch` amont).
//!
//! Regeneree par extraction de `D:/Projet/dillo/src/html_charrefs.h`.
//! Ne pas editer a la main.
#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

/// `Charref_t` — verbatim.
pub struct Charref {
    /// `ref` — nom de l'entite (clef bsearch, `strcmp`).
    pub name: &'static [u8],
    /// `html5_str` — valeur pour HTML5 (`None` ≙ NULL amont).
    pub html5: Option<&'static [u8]>,
    /// `html4_str` — valeur pre-HTML5 (`None` ≙ NULL amont).
    pub html4: Option<&'static [u8]>,
}

/// `NumRef`.
pub const NUM_REF: usize = 2125;

/// `Charrefs[NumRef]` — verbatim (triee pour bsearch).
pub static CHARREFS: [Charref; NUM_REF] = [
    Charref {
        name: b"AElig",
        html5: Some(b"\xc3\x86"),
        html4: Some(b"\xc3\x86"),
    },
    Charref {
        name: b"AMP",
        html5: Some(b"&"),
        html4: None,
    },
    Charref {
        name: b"Aacute",
        html5: Some(b"\xc3\x81"),
        html4: Some(b"\xc3\x81"),
    },
    Charref {
        name: b"Abreve",
        html5: Some(b"\xc4\x82"),
        html4: None,
    },
    Charref {
        name: b"Acirc",
        html5: Some(b"\xc3\x82"),
        html4: Some(b"\xc3\x82"),
    },
    Charref {
        name: b"Acy",
        html5: Some(b"\xd0\x90"),
        html4: None,
    },
    Charref {
        name: b"Afr",
        html5: Some(b"\xf0\x9d\x94\x84"),
        html4: None,
    },
    Charref {
        name: b"Agrave",
        html5: Some(b"\xc3\x80"),
        html4: Some(b"\xc3\x80"),
    },
    Charref {
        name: b"Alpha",
        html5: Some(b"\xce\x91"),
        html4: Some(b"\xce\x91"),
    },
    Charref {
        name: b"Amacr",
        html5: Some(b"\xc4\x80"),
        html4: None,
    },
    Charref {
        name: b"And",
        html5: Some(b"\xe2\xa9\x93"),
        html4: None,
    },
    Charref {
        name: b"Aogon",
        html5: Some(b"\xc4\x84"),
        html4: None,
    },
    Charref {
        name: b"Aopf",
        html5: Some(b"\xf0\x9d\x94\xb8"),
        html4: None,
    },
    Charref {
        name: b"ApplyFunction",
        html5: Some(b"\xe2\x81\xa1"),
        html4: None,
    },
    Charref {
        name: b"Aring",
        html5: Some(b"\xc3\x85"),
        html4: Some(b"\xc3\x85"),
    },
    Charref {
        name: b"Ascr",
        html5: Some(b"\xf0\x9d\x92\x9c"),
        html4: None,
    },
    Charref {
        name: b"Assign",
        html5: Some(b"\xe2\x89\x94"),
        html4: None,
    },
    Charref {
        name: b"Atilde",
        html5: Some(b"\xc3\x83"),
        html4: Some(b"\xc3\x83"),
    },
    Charref {
        name: b"Auml",
        html5: Some(b"\xc3\x84"),
        html4: Some(b"\xc3\x84"),
    },
    Charref {
        name: b"Backslash",
        html5: Some(b"\xe2\x88\x96"),
        html4: None,
    },
    Charref {
        name: b"Barv",
        html5: Some(b"\xe2\xab\xa7"),
        html4: None,
    },
    Charref {
        name: b"Barwed",
        html5: Some(b"\xe2\x8c\x86"),
        html4: None,
    },
    Charref {
        name: b"Bcy",
        html5: Some(b"\xd0\x91"),
        html4: None,
    },
    Charref {
        name: b"Because",
        html5: Some(b"\xe2\x88\xb5"),
        html4: None,
    },
    Charref {
        name: b"Bernoullis",
        html5: Some(b"\xe2\x84\xac"),
        html4: None,
    },
    Charref {
        name: b"Beta",
        html5: Some(b"\xce\x92"),
        html4: Some(b"\xce\x92"),
    },
    Charref {
        name: b"Bfr",
        html5: Some(b"\xf0\x9d\x94\x85"),
        html4: None,
    },
    Charref {
        name: b"Bopf",
        html5: Some(b"\xf0\x9d\x94\xb9"),
        html4: None,
    },
    Charref {
        name: b"Breve",
        html5: Some(b"\xcb\x98"),
        html4: None,
    },
    Charref {
        name: b"Bscr",
        html5: Some(b"\xe2\x84\xac"),
        html4: None,
    },
    Charref {
        name: b"Bumpeq",
        html5: Some(b"\xe2\x89\x8e"),
        html4: None,
    },
    Charref {
        name: b"CHcy",
        html5: Some(b"\xd0\xa7"),
        html4: None,
    },
    Charref {
        name: b"COPY",
        html5: Some(b"\xc2\xa9"),
        html4: None,
    },
    Charref {
        name: b"Cacute",
        html5: Some(b"\xc4\x86"),
        html4: None,
    },
    Charref {
        name: b"Cap",
        html5: Some(b"\xe2\x8b\x92"),
        html4: None,
    },
    Charref {
        name: b"CapitalDifferentialD",
        html5: Some(b"\xe2\x85\x85"),
        html4: None,
    },
    Charref {
        name: b"Cayleys",
        html5: Some(b"\xe2\x84\xad"),
        html4: None,
    },
    Charref {
        name: b"Ccaron",
        html5: Some(b"\xc4\x8c"),
        html4: None,
    },
    Charref {
        name: b"Ccedil",
        html5: Some(b"\xc3\x87"),
        html4: Some(b"\xc3\x87"),
    },
    Charref {
        name: b"Ccirc",
        html5: Some(b"\xc4\x88"),
        html4: None,
    },
    Charref {
        name: b"Cconint",
        html5: Some(b"\xe2\x88\xb0"),
        html4: None,
    },
    Charref {
        name: b"Cdot",
        html5: Some(b"\xc4\x8a"),
        html4: None,
    },
    Charref {
        name: b"Cedilla",
        html5: Some(b"\xc2\xb8"),
        html4: None,
    },
    Charref {
        name: b"CenterDot",
        html5: Some(b"\xc2\xb7"),
        html4: None,
    },
    Charref {
        name: b"Cfr",
        html5: Some(b"\xe2\x84\xad"),
        html4: None,
    },
    Charref {
        name: b"Chi",
        html5: Some(b"\xce\xa7"),
        html4: Some(b"\xce\xa7"),
    },
    Charref {
        name: b"CircleDot",
        html5: Some(b"\xe2\x8a\x99"),
        html4: None,
    },
    Charref {
        name: b"CircleMinus",
        html5: Some(b"\xe2\x8a\x96"),
        html4: None,
    },
    Charref {
        name: b"CirclePlus",
        html5: Some(b"\xe2\x8a\x95"),
        html4: None,
    },
    Charref {
        name: b"CircleTimes",
        html5: Some(b"\xe2\x8a\x97"),
        html4: None,
    },
    Charref {
        name: b"ClockwiseContourIntegral",
        html5: Some(b"\xe2\x88\xb2"),
        html4: None,
    },
    Charref {
        name: b"CloseCurlyDoubleQuote",
        html5: Some(b"\xe2\x80\x9d"),
        html4: None,
    },
    Charref {
        name: b"CloseCurlyQuote",
        html5: Some(b"\xe2\x80\x99"),
        html4: None,
    },
    Charref {
        name: b"Colon",
        html5: Some(b"\xe2\x88\xb7"),
        html4: None,
    },
    Charref {
        name: b"Colone",
        html5: Some(b"\xe2\xa9\xb4"),
        html4: None,
    },
    Charref {
        name: b"Congruent",
        html5: Some(b"\xe2\x89\xa1"),
        html4: None,
    },
    Charref {
        name: b"Conint",
        html5: Some(b"\xe2\x88\xaf"),
        html4: None,
    },
    Charref {
        name: b"ContourIntegral",
        html5: Some(b"\xe2\x88\xae"),
        html4: None,
    },
    Charref {
        name: b"Copf",
        html5: Some(b"\xe2\x84\x82"),
        html4: None,
    },
    Charref {
        name: b"Coproduct",
        html5: Some(b"\xe2\x88\x90"),
        html4: None,
    },
    Charref {
        name: b"CounterClockwiseContourIntegral",
        html5: Some(b"\xe2\x88\xb3"),
        html4: None,
    },
    Charref {
        name: b"Cross",
        html5: Some(b"\xe2\xa8\xaf"),
        html4: None,
    },
    Charref {
        name: b"Cscr",
        html5: Some(b"\xf0\x9d\x92\x9e"),
        html4: None,
    },
    Charref {
        name: b"Cup",
        html5: Some(b"\xe2\x8b\x93"),
        html4: None,
    },
    Charref {
        name: b"CupCap",
        html5: Some(b"\xe2\x89\x8d"),
        html4: None,
    },
    Charref {
        name: b"DD",
        html5: Some(b"\xe2\x85\x85"),
        html4: None,
    },
    Charref {
        name: b"DDotrahd",
        html5: Some(b"\xe2\xa4\x91"),
        html4: None,
    },
    Charref {
        name: b"DJcy",
        html5: Some(b"\xd0\x82"),
        html4: None,
    },
    Charref {
        name: b"DScy",
        html5: Some(b"\xd0\x85"),
        html4: None,
    },
    Charref {
        name: b"DZcy",
        html5: Some(b"\xd0\x8f"),
        html4: None,
    },
    Charref {
        name: b"Dagger",
        html5: Some(b"\xe2\x80\xa1"),
        html4: Some(b"\xe2\x80\xa1"),
    },
    Charref {
        name: b"Darr",
        html5: Some(b"\xe2\x86\xa1"),
        html4: None,
    },
    Charref {
        name: b"Dashv",
        html5: Some(b"\xe2\xab\xa4"),
        html4: None,
    },
    Charref {
        name: b"Dcaron",
        html5: Some(b"\xc4\x8e"),
        html4: None,
    },
    Charref {
        name: b"Dcy",
        html5: Some(b"\xd0\x94"),
        html4: None,
    },
    Charref {
        name: b"Del",
        html5: Some(b"\xe2\x88\x87"),
        html4: None,
    },
    Charref {
        name: b"Delta",
        html5: Some(b"\xce\x94"),
        html4: Some(b"\xce\x94"),
    },
    Charref {
        name: b"Dfr",
        html5: Some(b"\xf0\x9d\x94\x87"),
        html4: None,
    },
    Charref {
        name: b"DiacriticalAcute",
        html5: Some(b"\xc2\xb4"),
        html4: None,
    },
    Charref {
        name: b"DiacriticalDot",
        html5: Some(b"\xcb\x99"),
        html4: None,
    },
    Charref {
        name: b"DiacriticalDoubleAcute",
        html5: Some(b"\xcb\x9d"),
        html4: None,
    },
    Charref {
        name: b"DiacriticalGrave",
        html5: Some(b"`"),
        html4: None,
    },
    Charref {
        name: b"DiacriticalTilde",
        html5: Some(b"\xcb\x9c"),
        html4: None,
    },
    Charref {
        name: b"Diamond",
        html5: Some(b"\xe2\x8b\x84"),
        html4: None,
    },
    Charref {
        name: b"DifferentialD",
        html5: Some(b"\xe2\x85\x86"),
        html4: None,
    },
    Charref {
        name: b"Dopf",
        html5: Some(b"\xf0\x9d\x94\xbb"),
        html4: None,
    },
    Charref {
        name: b"Dot",
        html5: Some(b"\xc2\xa8"),
        html4: None,
    },
    Charref {
        name: b"DotDot",
        html5: Some(b"\xe2\x83\x9c"),
        html4: None,
    },
    Charref {
        name: b"DotEqual",
        html5: Some(b"\xe2\x89\x90"),
        html4: None,
    },
    Charref {
        name: b"DoubleContourIntegral",
        html5: Some(b"\xe2\x88\xaf"),
        html4: None,
    },
    Charref {
        name: b"DoubleDot",
        html5: Some(b"\xc2\xa8"),
        html4: None,
    },
    Charref {
        name: b"DoubleDownArrow",
        html5: Some(b"\xe2\x87\x93"),
        html4: None,
    },
    Charref {
        name: b"DoubleLeftArrow",
        html5: Some(b"\xe2\x87\x90"),
        html4: None,
    },
    Charref {
        name: b"DoubleLeftRightArrow",
        html5: Some(b"\xe2\x87\x94"),
        html4: None,
    },
    Charref {
        name: b"DoubleLeftTee",
        html5: Some(b"\xe2\xab\xa4"),
        html4: None,
    },
    Charref {
        name: b"DoubleLongLeftArrow",
        html5: Some(b"\xe2\x9f\xb8"),
        html4: None,
    },
    Charref {
        name: b"DoubleLongLeftRightArrow",
        html5: Some(b"\xe2\x9f\xba"),
        html4: None,
    },
    Charref {
        name: b"DoubleLongRightArrow",
        html5: Some(b"\xe2\x9f\xb9"),
        html4: None,
    },
    Charref {
        name: b"DoubleRightArrow",
        html5: Some(b"\xe2\x87\x92"),
        html4: None,
    },
    Charref {
        name: b"DoubleRightTee",
        html5: Some(b"\xe2\x8a\xa8"),
        html4: None,
    },
    Charref {
        name: b"DoubleUpArrow",
        html5: Some(b"\xe2\x87\x91"),
        html4: None,
    },
    Charref {
        name: b"DoubleUpDownArrow",
        html5: Some(b"\xe2\x87\x95"),
        html4: None,
    },
    Charref {
        name: b"DoubleVerticalBar",
        html5: Some(b"\xe2\x88\xa5"),
        html4: None,
    },
    Charref {
        name: b"DownArrow",
        html5: Some(b"\xe2\x86\x93"),
        html4: None,
    },
    Charref {
        name: b"DownArrowBar",
        html5: Some(b"\xe2\xa4\x93"),
        html4: None,
    },
    Charref {
        name: b"DownArrowUpArrow",
        html5: Some(b"\xe2\x87\xb5"),
        html4: None,
    },
    Charref {
        name: b"DownBreve",
        html5: Some(b"\xcc\x91"),
        html4: None,
    },
    Charref {
        name: b"DownLeftRightVector",
        html5: Some(b"\xe2\xa5\x90"),
        html4: None,
    },
    Charref {
        name: b"DownLeftTeeVector",
        html5: Some(b"\xe2\xa5\x9e"),
        html4: None,
    },
    Charref {
        name: b"DownLeftVector",
        html5: Some(b"\xe2\x86\xbd"),
        html4: None,
    },
    Charref {
        name: b"DownLeftVectorBar",
        html5: Some(b"\xe2\xa5\x96"),
        html4: None,
    },
    Charref {
        name: b"DownRightTeeVector",
        html5: Some(b"\xe2\xa5\x9f"),
        html4: None,
    },
    Charref {
        name: b"DownRightVector",
        html5: Some(b"\xe2\x87\x81"),
        html4: None,
    },
    Charref {
        name: b"DownRightVectorBar",
        html5: Some(b"\xe2\xa5\x97"),
        html4: None,
    },
    Charref {
        name: b"DownTee",
        html5: Some(b"\xe2\x8a\xa4"),
        html4: None,
    },
    Charref {
        name: b"DownTeeArrow",
        html5: Some(b"\xe2\x86\xa7"),
        html4: None,
    },
    Charref {
        name: b"Downarrow",
        html5: Some(b"\xe2\x87\x93"),
        html4: None,
    },
    Charref {
        name: b"Dscr",
        html5: Some(b"\xf0\x9d\x92\x9f"),
        html4: None,
    },
    Charref {
        name: b"Dstrok",
        html5: Some(b"\xc4\x90"),
        html4: None,
    },
    Charref {
        name: b"ENG",
        html5: Some(b"\xc5\x8a"),
        html4: None,
    },
    Charref {
        name: b"ETH",
        html5: Some(b"\xc3\x90"),
        html4: Some(b"\xc3\x90"),
    },
    Charref {
        name: b"Eacute",
        html5: Some(b"\xc3\x89"),
        html4: Some(b"\xc3\x89"),
    },
    Charref {
        name: b"Ecaron",
        html5: Some(b"\xc4\x9a"),
        html4: None,
    },
    Charref {
        name: b"Ecirc",
        html5: Some(b"\xc3\x8a"),
        html4: Some(b"\xc3\x8a"),
    },
    Charref {
        name: b"Ecy",
        html5: Some(b"\xd0\xad"),
        html4: None,
    },
    Charref {
        name: b"Edot",
        html5: Some(b"\xc4\x96"),
        html4: None,
    },
    Charref {
        name: b"Efr",
        html5: Some(b"\xf0\x9d\x94\x88"),
        html4: None,
    },
    Charref {
        name: b"Egrave",
        html5: Some(b"\xc3\x88"),
        html4: Some(b"\xc3\x88"),
    },
    Charref {
        name: b"Element",
        html5: Some(b"\xe2\x88\x88"),
        html4: None,
    },
    Charref {
        name: b"Emacr",
        html5: Some(b"\xc4\x92"),
        html4: None,
    },
    Charref {
        name: b"EmptySmallSquare",
        html5: Some(b"\xe2\x97\xbb"),
        html4: None,
    },
    Charref {
        name: b"EmptyVerySmallSquare",
        html5: Some(b"\xe2\x96\xab"),
        html4: None,
    },
    Charref {
        name: b"Eogon",
        html5: Some(b"\xc4\x98"),
        html4: None,
    },
    Charref {
        name: b"Eopf",
        html5: Some(b"\xf0\x9d\x94\xbc"),
        html4: None,
    },
    Charref {
        name: b"Epsilon",
        html5: Some(b"\xce\x95"),
        html4: Some(b"\xce\x95"),
    },
    Charref {
        name: b"Equal",
        html5: Some(b"\xe2\xa9\xb5"),
        html4: None,
    },
    Charref {
        name: b"EqualTilde",
        html5: Some(b"\xe2\x89\x82"),
        html4: None,
    },
    Charref {
        name: b"Equilibrium",
        html5: Some(b"\xe2\x87\x8c"),
        html4: None,
    },
    Charref {
        name: b"Escr",
        html5: Some(b"\xe2\x84\xb0"),
        html4: None,
    },
    Charref {
        name: b"Esim",
        html5: Some(b"\xe2\xa9\xb3"),
        html4: None,
    },
    Charref {
        name: b"Eta",
        html5: Some(b"\xce\x97"),
        html4: Some(b"\xce\x97"),
    },
    Charref {
        name: b"Euml",
        html5: Some(b"\xc3\x8b"),
        html4: Some(b"\xc3\x8b"),
    },
    Charref {
        name: b"Exists",
        html5: Some(b"\xe2\x88\x83"),
        html4: None,
    },
    Charref {
        name: b"ExponentialE",
        html5: Some(b"\xe2\x85\x87"),
        html4: None,
    },
    Charref {
        name: b"Fcy",
        html5: Some(b"\xd0\xa4"),
        html4: None,
    },
    Charref {
        name: b"Ffr",
        html5: Some(b"\xf0\x9d\x94\x89"),
        html4: None,
    },
    Charref {
        name: b"FilledSmallSquare",
        html5: Some(b"\xe2\x97\xbc"),
        html4: None,
    },
    Charref {
        name: b"FilledVerySmallSquare",
        html5: Some(b"\xe2\x96\xaa"),
        html4: None,
    },
    Charref {
        name: b"Fopf",
        html5: Some(b"\xf0\x9d\x94\xbd"),
        html4: None,
    },
    Charref {
        name: b"ForAll",
        html5: Some(b"\xe2\x88\x80"),
        html4: None,
    },
    Charref {
        name: b"Fouriertrf",
        html5: Some(b"\xe2\x84\xb1"),
        html4: None,
    },
    Charref {
        name: b"Fscr",
        html5: Some(b"\xe2\x84\xb1"),
        html4: None,
    },
    Charref {
        name: b"GJcy",
        html5: Some(b"\xd0\x83"),
        html4: None,
    },
    Charref {
        name: b"GT",
        html5: Some(b">"),
        html4: None,
    },
    Charref {
        name: b"Gamma",
        html5: Some(b"\xce\x93"),
        html4: Some(b"\xce\x93"),
    },
    Charref {
        name: b"Gammad",
        html5: Some(b"\xcf\x9c"),
        html4: None,
    },
    Charref {
        name: b"Gbreve",
        html5: Some(b"\xc4\x9e"),
        html4: None,
    },
    Charref {
        name: b"Gcedil",
        html5: Some(b"\xc4\xa2"),
        html4: None,
    },
    Charref {
        name: b"Gcirc",
        html5: Some(b"\xc4\x9c"),
        html4: None,
    },
    Charref {
        name: b"Gcy",
        html5: Some(b"\xd0\x93"),
        html4: None,
    },
    Charref {
        name: b"Gdot",
        html5: Some(b"\xc4\xa0"),
        html4: None,
    },
    Charref {
        name: b"Gfr",
        html5: Some(b"\xf0\x9d\x94\x8a"),
        html4: None,
    },
    Charref {
        name: b"Gg",
        html5: Some(b"\xe2\x8b\x99"),
        html4: None,
    },
    Charref {
        name: b"Gopf",
        html5: Some(b"\xf0\x9d\x94\xbe"),
        html4: None,
    },
    Charref {
        name: b"GreaterEqual",
        html5: Some(b"\xe2\x89\xa5"),
        html4: None,
    },
    Charref {
        name: b"GreaterEqualLess",
        html5: Some(b"\xe2\x8b\x9b"),
        html4: None,
    },
    Charref {
        name: b"GreaterFullEqual",
        html5: Some(b"\xe2\x89\xa7"),
        html4: None,
    },
    Charref {
        name: b"GreaterGreater",
        html5: Some(b"\xe2\xaa\xa2"),
        html4: None,
    },
    Charref {
        name: b"GreaterLess",
        html5: Some(b"\xe2\x89\xb7"),
        html4: None,
    },
    Charref {
        name: b"GreaterSlantEqual",
        html5: Some(b"\xe2\xa9\xbe"),
        html4: None,
    },
    Charref {
        name: b"GreaterTilde",
        html5: Some(b"\xe2\x89\xb3"),
        html4: None,
    },
    Charref {
        name: b"Gscr",
        html5: Some(b"\xf0\x9d\x92\xa2"),
        html4: None,
    },
    Charref {
        name: b"Gt",
        html5: Some(b"\xe2\x89\xab"),
        html4: None,
    },
    Charref {
        name: b"HARDcy",
        html5: Some(b"\xd0\xaa"),
        html4: None,
    },
    Charref {
        name: b"Hacek",
        html5: Some(b"\xcb\x87"),
        html4: None,
    },
    Charref {
        name: b"Hat",
        html5: Some(b"^"),
        html4: None,
    },
    Charref {
        name: b"Hcirc",
        html5: Some(b"\xc4\xa4"),
        html4: None,
    },
    Charref {
        name: b"Hfr",
        html5: Some(b"\xe2\x84\x8c"),
        html4: None,
    },
    Charref {
        name: b"HilbertSpace",
        html5: Some(b"\xe2\x84\x8b"),
        html4: None,
    },
    Charref {
        name: b"Hopf",
        html5: Some(b"\xe2\x84\x8d"),
        html4: None,
    },
    Charref {
        name: b"HorizontalLine",
        html5: Some(b"\xe2\x94\x80"),
        html4: None,
    },
    Charref {
        name: b"Hscr",
        html5: Some(b"\xe2\x84\x8b"),
        html4: None,
    },
    Charref {
        name: b"Hstrok",
        html5: Some(b"\xc4\xa6"),
        html4: None,
    },
    Charref {
        name: b"HumpDownHump",
        html5: Some(b"\xe2\x89\x8e"),
        html4: None,
    },
    Charref {
        name: b"HumpEqual",
        html5: Some(b"\xe2\x89\x8f"),
        html4: None,
    },
    Charref {
        name: b"IEcy",
        html5: Some(b"\xd0\x95"),
        html4: None,
    },
    Charref {
        name: b"IJlig",
        html5: Some(b"\xc4\xb2"),
        html4: None,
    },
    Charref {
        name: b"IOcy",
        html5: Some(b"\xd0\x81"),
        html4: None,
    },
    Charref {
        name: b"Iacute",
        html5: Some(b"\xc3\x8d"),
        html4: Some(b"\xc3\x8d"),
    },
    Charref {
        name: b"Icirc",
        html5: Some(b"\xc3\x8e"),
        html4: Some(b"\xc3\x8e"),
    },
    Charref {
        name: b"Icy",
        html5: Some(b"\xd0\x98"),
        html4: None,
    },
    Charref {
        name: b"Idot",
        html5: Some(b"\xc4\xb0"),
        html4: None,
    },
    Charref {
        name: b"Ifr",
        html5: Some(b"\xe2\x84\x91"),
        html4: None,
    },
    Charref {
        name: b"Igrave",
        html5: Some(b"\xc3\x8c"),
        html4: Some(b"\xc3\x8c"),
    },
    Charref {
        name: b"Im",
        html5: Some(b"\xe2\x84\x91"),
        html4: None,
    },
    Charref {
        name: b"Imacr",
        html5: Some(b"\xc4\xaa"),
        html4: None,
    },
    Charref {
        name: b"ImaginaryI",
        html5: Some(b"\xe2\x85\x88"),
        html4: None,
    },
    Charref {
        name: b"Implies",
        html5: Some(b"\xe2\x87\x92"),
        html4: None,
    },
    Charref {
        name: b"Int",
        html5: Some(b"\xe2\x88\xac"),
        html4: None,
    },
    Charref {
        name: b"Integral",
        html5: Some(b"\xe2\x88\xab"),
        html4: None,
    },
    Charref {
        name: b"Intersection",
        html5: Some(b"\xe2\x8b\x82"),
        html4: None,
    },
    Charref {
        name: b"InvisibleComma",
        html5: Some(b"\xe2\x81\xa3"),
        html4: None,
    },
    Charref {
        name: b"InvisibleTimes",
        html5: Some(b"\xe2\x81\xa2"),
        html4: None,
    },
    Charref {
        name: b"Iogon",
        html5: Some(b"\xc4\xae"),
        html4: None,
    },
    Charref {
        name: b"Iopf",
        html5: Some(b"\xf0\x9d\x95\x80"),
        html4: None,
    },
    Charref {
        name: b"Iota",
        html5: Some(b"\xce\x99"),
        html4: Some(b"\xce\x99"),
    },
    Charref {
        name: b"Iscr",
        html5: Some(b"\xe2\x84\x90"),
        html4: None,
    },
    Charref {
        name: b"Itilde",
        html5: Some(b"\xc4\xa8"),
        html4: None,
    },
    Charref {
        name: b"Iukcy",
        html5: Some(b"\xd0\x86"),
        html4: None,
    },
    Charref {
        name: b"Iuml",
        html5: Some(b"\xc3\x8f"),
        html4: Some(b"\xc3\x8f"),
    },
    Charref {
        name: b"Jcirc",
        html5: Some(b"\xc4\xb4"),
        html4: None,
    },
    Charref {
        name: b"Jcy",
        html5: Some(b"\xd0\x99"),
        html4: None,
    },
    Charref {
        name: b"Jfr",
        html5: Some(b"\xf0\x9d\x94\x8d"),
        html4: None,
    },
    Charref {
        name: b"Jopf",
        html5: Some(b"\xf0\x9d\x95\x81"),
        html4: None,
    },
    Charref {
        name: b"Jscr",
        html5: Some(b"\xf0\x9d\x92\xa5"),
        html4: None,
    },
    Charref {
        name: b"Jsercy",
        html5: Some(b"\xd0\x88"),
        html4: None,
    },
    Charref {
        name: b"Jukcy",
        html5: Some(b"\xd0\x84"),
        html4: None,
    },
    Charref {
        name: b"KHcy",
        html5: Some(b"\xd0\xa5"),
        html4: None,
    },
    Charref {
        name: b"KJcy",
        html5: Some(b"\xd0\x8c"),
        html4: None,
    },
    Charref {
        name: b"Kappa",
        html5: Some(b"\xce\x9a"),
        html4: Some(b"\xce\x9a"),
    },
    Charref {
        name: b"Kcedil",
        html5: Some(b"\xc4\xb6"),
        html4: None,
    },
    Charref {
        name: b"Kcy",
        html5: Some(b"\xd0\x9a"),
        html4: None,
    },
    Charref {
        name: b"Kfr",
        html5: Some(b"\xf0\x9d\x94\x8e"),
        html4: None,
    },
    Charref {
        name: b"Kopf",
        html5: Some(b"\xf0\x9d\x95\x82"),
        html4: None,
    },
    Charref {
        name: b"Kscr",
        html5: Some(b"\xf0\x9d\x92\xa6"),
        html4: None,
    },
    Charref {
        name: b"LJcy",
        html5: Some(b"\xd0\x89"),
        html4: None,
    },
    Charref {
        name: b"LT",
        html5: Some(b"<"),
        html4: None,
    },
    Charref {
        name: b"Lacute",
        html5: Some(b"\xc4\xb9"),
        html4: None,
    },
    Charref {
        name: b"Lambda",
        html5: Some(b"\xce\x9b"),
        html4: Some(b"\xce\x9b"),
    },
    Charref {
        name: b"Lang",
        html5: Some(b"\xe2\x9f\xaa"),
        html4: None,
    },
    Charref {
        name: b"Laplacetrf",
        html5: Some(b"\xe2\x84\x92"),
        html4: None,
    },
    Charref {
        name: b"Larr",
        html5: Some(b"\xe2\x86\x9e"),
        html4: None,
    },
    Charref {
        name: b"Lcaron",
        html5: Some(b"\xc4\xbd"),
        html4: None,
    },
    Charref {
        name: b"Lcedil",
        html5: Some(b"\xc4\xbb"),
        html4: None,
    },
    Charref {
        name: b"Lcy",
        html5: Some(b"\xd0\x9b"),
        html4: None,
    },
    Charref {
        name: b"LeftAngleBracket",
        html5: Some(b"\xe2\x9f\xa8"),
        html4: None,
    },
    Charref {
        name: b"LeftArrow",
        html5: Some(b"\xe2\x86\x90"),
        html4: None,
    },
    Charref {
        name: b"LeftArrowBar",
        html5: Some(b"\xe2\x87\xa4"),
        html4: None,
    },
    Charref {
        name: b"LeftArrowRightArrow",
        html5: Some(b"\xe2\x87\x86"),
        html4: None,
    },
    Charref {
        name: b"LeftCeiling",
        html5: Some(b"\xe2\x8c\x88"),
        html4: None,
    },
    Charref {
        name: b"LeftDoubleBracket",
        html5: Some(b"\xe2\x9f\xa6"),
        html4: None,
    },
    Charref {
        name: b"LeftDownTeeVector",
        html5: Some(b"\xe2\xa5\xa1"),
        html4: None,
    },
    Charref {
        name: b"LeftDownVector",
        html5: Some(b"\xe2\x87\x83"),
        html4: None,
    },
    Charref {
        name: b"LeftDownVectorBar",
        html5: Some(b"\xe2\xa5\x99"),
        html4: None,
    },
    Charref {
        name: b"LeftFloor",
        html5: Some(b"\xe2\x8c\x8a"),
        html4: None,
    },
    Charref {
        name: b"LeftRightArrow",
        html5: Some(b"\xe2\x86\x94"),
        html4: None,
    },
    Charref {
        name: b"LeftRightVector",
        html5: Some(b"\xe2\xa5\x8e"),
        html4: None,
    },
    Charref {
        name: b"LeftTee",
        html5: Some(b"\xe2\x8a\xa3"),
        html4: None,
    },
    Charref {
        name: b"LeftTeeArrow",
        html5: Some(b"\xe2\x86\xa4"),
        html4: None,
    },
    Charref {
        name: b"LeftTeeVector",
        html5: Some(b"\xe2\xa5\x9a"),
        html4: None,
    },
    Charref {
        name: b"LeftTriangle",
        html5: Some(b"\xe2\x8a\xb2"),
        html4: None,
    },
    Charref {
        name: b"LeftTriangleBar",
        html5: Some(b"\xe2\xa7\x8f"),
        html4: None,
    },
    Charref {
        name: b"LeftTriangleEqual",
        html5: Some(b"\xe2\x8a\xb4"),
        html4: None,
    },
    Charref {
        name: b"LeftUpDownVector",
        html5: Some(b"\xe2\xa5\x91"),
        html4: None,
    },
    Charref {
        name: b"LeftUpTeeVector",
        html5: Some(b"\xe2\xa5\xa0"),
        html4: None,
    },
    Charref {
        name: b"LeftUpVector",
        html5: Some(b"\xe2\x86\xbf"),
        html4: None,
    },
    Charref {
        name: b"LeftUpVectorBar",
        html5: Some(b"\xe2\xa5\x98"),
        html4: None,
    },
    Charref {
        name: b"LeftVector",
        html5: Some(b"\xe2\x86\xbc"),
        html4: None,
    },
    Charref {
        name: b"LeftVectorBar",
        html5: Some(b"\xe2\xa5\x92"),
        html4: None,
    },
    Charref {
        name: b"Leftarrow",
        html5: Some(b"\xe2\x87\x90"),
        html4: None,
    },
    Charref {
        name: b"Leftrightarrow",
        html5: Some(b"\xe2\x87\x94"),
        html4: None,
    },
    Charref {
        name: b"LessEqualGreater",
        html5: Some(b"\xe2\x8b\x9a"),
        html4: None,
    },
    Charref {
        name: b"LessFullEqual",
        html5: Some(b"\xe2\x89\xa6"),
        html4: None,
    },
    Charref {
        name: b"LessGreater",
        html5: Some(b"\xe2\x89\xb6"),
        html4: None,
    },
    Charref {
        name: b"LessLess",
        html5: Some(b"\xe2\xaa\xa1"),
        html4: None,
    },
    Charref {
        name: b"LessSlantEqual",
        html5: Some(b"\xe2\xa9\xbd"),
        html4: None,
    },
    Charref {
        name: b"LessTilde",
        html5: Some(b"\xe2\x89\xb2"),
        html4: None,
    },
    Charref {
        name: b"Lfr",
        html5: Some(b"\xf0\x9d\x94\x8f"),
        html4: None,
    },
    Charref {
        name: b"Ll",
        html5: Some(b"\xe2\x8b\x98"),
        html4: None,
    },
    Charref {
        name: b"Lleftarrow",
        html5: Some(b"\xe2\x87\x9a"),
        html4: None,
    },
    Charref {
        name: b"Lmidot",
        html5: Some(b"\xc4\xbf"),
        html4: None,
    },
    Charref {
        name: b"LongLeftArrow",
        html5: Some(b"\xe2\x9f\xb5"),
        html4: None,
    },
    Charref {
        name: b"LongLeftRightArrow",
        html5: Some(b"\xe2\x9f\xb7"),
        html4: None,
    },
    Charref {
        name: b"LongRightArrow",
        html5: Some(b"\xe2\x9f\xb6"),
        html4: None,
    },
    Charref {
        name: b"Longleftarrow",
        html5: Some(b"\xe2\x9f\xb8"),
        html4: None,
    },
    Charref {
        name: b"Longleftrightarrow",
        html5: Some(b"\xe2\x9f\xba"),
        html4: None,
    },
    Charref {
        name: b"Longrightarrow",
        html5: Some(b"\xe2\x9f\xb9"),
        html4: None,
    },
    Charref {
        name: b"Lopf",
        html5: Some(b"\xf0\x9d\x95\x83"),
        html4: None,
    },
    Charref {
        name: b"LowerLeftArrow",
        html5: Some(b"\xe2\x86\x99"),
        html4: None,
    },
    Charref {
        name: b"LowerRightArrow",
        html5: Some(b"\xe2\x86\x98"),
        html4: None,
    },
    Charref {
        name: b"Lscr",
        html5: Some(b"\xe2\x84\x92"),
        html4: None,
    },
    Charref {
        name: b"Lsh",
        html5: Some(b"\xe2\x86\xb0"),
        html4: None,
    },
    Charref {
        name: b"Lstrok",
        html5: Some(b"\xc5\x81"),
        html4: None,
    },
    Charref {
        name: b"Lt",
        html5: Some(b"\xe2\x89\xaa"),
        html4: None,
    },
    Charref {
        name: b"Map",
        html5: Some(b"\xe2\xa4\x85"),
        html4: None,
    },
    Charref {
        name: b"Mcy",
        html5: Some(b"\xd0\x9c"),
        html4: None,
    },
    Charref {
        name: b"MediumSpace",
        html5: Some(b"\xe2\x81\x9f"),
        html4: None,
    },
    Charref {
        name: b"Mellintrf",
        html5: Some(b"\xe2\x84\xb3"),
        html4: None,
    },
    Charref {
        name: b"Mfr",
        html5: Some(b"\xf0\x9d\x94\x90"),
        html4: None,
    },
    Charref {
        name: b"MinusPlus",
        html5: Some(b"\xe2\x88\x93"),
        html4: None,
    },
    Charref {
        name: b"Mopf",
        html5: Some(b"\xf0\x9d\x95\x84"),
        html4: None,
    },
    Charref {
        name: b"Mscr",
        html5: Some(b"\xe2\x84\xb3"),
        html4: None,
    },
    Charref {
        name: b"Mu",
        html5: Some(b"\xce\x9c"),
        html4: Some(b"\xce\x9c"),
    },
    Charref {
        name: b"NJcy",
        html5: Some(b"\xd0\x8a"),
        html4: None,
    },
    Charref {
        name: b"Nacute",
        html5: Some(b"\xc5\x83"),
        html4: None,
    },
    Charref {
        name: b"Ncaron",
        html5: Some(b"\xc5\x87"),
        html4: None,
    },
    Charref {
        name: b"Ncedil",
        html5: Some(b"\xc5\x85"),
        html4: None,
    },
    Charref {
        name: b"Ncy",
        html5: Some(b"\xd0\x9d"),
        html4: None,
    },
    Charref {
        name: b"NegativeMediumSpace",
        html5: Some(b"\xe2\x80\x8b"),
        html4: None,
    },
    Charref {
        name: b"NegativeThickSpace",
        html5: Some(b"\xe2\x80\x8b"),
        html4: None,
    },
    Charref {
        name: b"NegativeThinSpace",
        html5: Some(b"\xe2\x80\x8b"),
        html4: None,
    },
    Charref {
        name: b"NegativeVeryThinSpace",
        html5: Some(b"\xe2\x80\x8b"),
        html4: None,
    },
    Charref {
        name: b"NestedGreaterGreater",
        html5: Some(b"\xe2\x89\xab"),
        html4: None,
    },
    Charref {
        name: b"NestedLessLess",
        html5: Some(b"\xe2\x89\xaa"),
        html4: None,
    },
    Charref {
        name: b"NewLine",
        html5: Some(b"\\n"),
        html4: None,
    },
    Charref {
        name: b"Nfr",
        html5: Some(b"\xf0\x9d\x94\x91"),
        html4: None,
    },
    Charref {
        name: b"NoBreak",
        html5: Some(b"\xe2\x81\xa0"),
        html4: None,
    },
    Charref {
        name: b"NonBreakingSpace",
        html5: Some(b"\xc2\xa0"),
        html4: None,
    },
    Charref {
        name: b"Nopf",
        html5: Some(b"\xe2\x84\x95"),
        html4: None,
    },
    Charref {
        name: b"Not",
        html5: Some(b"\xe2\xab\xac"),
        html4: None,
    },
    Charref {
        name: b"NotCongruent",
        html5: Some(b"\xe2\x89\xa2"),
        html4: None,
    },
    Charref {
        name: b"NotCupCap",
        html5: Some(b"\xe2\x89\xad"),
        html4: None,
    },
    Charref {
        name: b"NotDoubleVerticalBar",
        html5: Some(b"\xe2\x88\xa6"),
        html4: None,
    },
    Charref {
        name: b"NotElement",
        html5: Some(b"\xe2\x88\x89"),
        html4: None,
    },
    Charref {
        name: b"NotEqual",
        html5: Some(b"\xe2\x89\xa0"),
        html4: None,
    },
    Charref {
        name: b"NotEqualTilde",
        html5: Some(b"\xe2\x89\x82\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotExists",
        html5: Some(b"\xe2\x88\x84"),
        html4: None,
    },
    Charref {
        name: b"NotGreater",
        html5: Some(b"\xe2\x89\xaf"),
        html4: None,
    },
    Charref {
        name: b"NotGreaterEqual",
        html5: Some(b"\xe2\x89\xb1"),
        html4: None,
    },
    Charref {
        name: b"NotGreaterFullEqual",
        html5: Some(b"\xe2\x89\xa7\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotGreaterGreater",
        html5: Some(b"\xe2\x89\xab\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotGreaterLess",
        html5: Some(b"\xe2\x89\xb9"),
        html4: None,
    },
    Charref {
        name: b"NotGreaterSlantEqual",
        html5: Some(b"\xe2\xa9\xbe\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotGreaterTilde",
        html5: Some(b"\xe2\x89\xb5"),
        html4: None,
    },
    Charref {
        name: b"NotHumpDownHump",
        html5: Some(b"\xe2\x89\x8e\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotHumpEqual",
        html5: Some(b"\xe2\x89\x8f\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotLeftTriangle",
        html5: Some(b"\xe2\x8b\xaa"),
        html4: None,
    },
    Charref {
        name: b"NotLeftTriangleBar",
        html5: Some(b"\xe2\xa7\x8f\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotLeftTriangleEqual",
        html5: Some(b"\xe2\x8b\xac"),
        html4: None,
    },
    Charref {
        name: b"NotLess",
        html5: Some(b"\xe2\x89\xae"),
        html4: None,
    },
    Charref {
        name: b"NotLessEqual",
        html5: Some(b"\xe2\x89\xb0"),
        html4: None,
    },
    Charref {
        name: b"NotLessGreater",
        html5: Some(b"\xe2\x89\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotLessLess",
        html5: Some(b"\xe2\x89\xaa\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotLessSlantEqual",
        html5: Some(b"\xe2\xa9\xbd\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotLessTilde",
        html5: Some(b"\xe2\x89\xb4"),
        html4: None,
    },
    Charref {
        name: b"NotNestedGreaterGreater",
        html5: Some(b"\xe2\xaa\xa2\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotNestedLessLess",
        html5: Some(b"\xe2\xaa\xa1\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotPrecedes",
        html5: Some(b"\xe2\x8a\x80"),
        html4: None,
    },
    Charref {
        name: b"NotPrecedesEqual",
        html5: Some(b"\xe2\xaa\xaf\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotPrecedesSlantEqual",
        html5: Some(b"\xe2\x8b\xa0"),
        html4: None,
    },
    Charref {
        name: b"NotReverseElement",
        html5: Some(b"\xe2\x88\x8c"),
        html4: None,
    },
    Charref {
        name: b"NotRightTriangle",
        html5: Some(b"\xe2\x8b\xab"),
        html4: None,
    },
    Charref {
        name: b"NotRightTriangleBar",
        html5: Some(b"\xe2\xa7\x90\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotRightTriangleEqual",
        html5: Some(b"\xe2\x8b\xad"),
        html4: None,
    },
    Charref {
        name: b"NotSquareSubset",
        html5: Some(b"\xe2\x8a\x8f\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotSquareSubsetEqual",
        html5: Some(b"\xe2\x8b\xa2"),
        html4: None,
    },
    Charref {
        name: b"NotSquareSuperset",
        html5: Some(b"\xe2\x8a\x90\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotSquareSupersetEqual",
        html5: Some(b"\xe2\x8b\xa3"),
        html4: None,
    },
    Charref {
        name: b"NotSubset",
        html5: Some(b"\xe2\x8a\x82\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"NotSubsetEqual",
        html5: Some(b"\xe2\x8a\x88"),
        html4: None,
    },
    Charref {
        name: b"NotSucceeds",
        html5: Some(b"\xe2\x8a\x81"),
        html4: None,
    },
    Charref {
        name: b"NotSucceedsEqual",
        html5: Some(b"\xe2\xaa\xb0\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotSucceedsSlantEqual",
        html5: Some(b"\xe2\x8b\xa1"),
        html4: None,
    },
    Charref {
        name: b"NotSucceedsTilde",
        html5: Some(b"\xe2\x89\xbf\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"NotSuperset",
        html5: Some(b"\xe2\x8a\x83\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"NotSupersetEqual",
        html5: Some(b"\xe2\x8a\x89"),
        html4: None,
    },
    Charref {
        name: b"NotTilde",
        html5: Some(b"\xe2\x89\x81"),
        html4: None,
    },
    Charref {
        name: b"NotTildeEqual",
        html5: Some(b"\xe2\x89\x84"),
        html4: None,
    },
    Charref {
        name: b"NotTildeFullEqual",
        html5: Some(b"\xe2\x89\x87"),
        html4: None,
    },
    Charref {
        name: b"NotTildeTilde",
        html5: Some(b"\xe2\x89\x89"),
        html4: None,
    },
    Charref {
        name: b"NotVerticalBar",
        html5: Some(b"\xe2\x88\xa4"),
        html4: None,
    },
    Charref {
        name: b"Nscr",
        html5: Some(b"\xf0\x9d\x92\xa9"),
        html4: None,
    },
    Charref {
        name: b"Ntilde",
        html5: Some(b"\xc3\x91"),
        html4: Some(b"\xc3\x91"),
    },
    Charref {
        name: b"Nu",
        html5: Some(b"\xce\x9d"),
        html4: Some(b"\xce\x9d"),
    },
    Charref {
        name: b"OElig",
        html5: Some(b"\xc5\x92"),
        html4: Some(b"\xc5\x92"),
    },
    Charref {
        name: b"Oacute",
        html5: Some(b"\xc3\x93"),
        html4: Some(b"\xc3\x93"),
    },
    Charref {
        name: b"Ocirc",
        html5: Some(b"\xc3\x94"),
        html4: Some(b"\xc3\x94"),
    },
    Charref {
        name: b"Ocy",
        html5: Some(b"\xd0\x9e"),
        html4: None,
    },
    Charref {
        name: b"Odblac",
        html5: Some(b"\xc5\x90"),
        html4: None,
    },
    Charref {
        name: b"Ofr",
        html5: Some(b"\xf0\x9d\x94\x92"),
        html4: None,
    },
    Charref {
        name: b"Ograve",
        html5: Some(b"\xc3\x92"),
        html4: Some(b"\xc3\x92"),
    },
    Charref {
        name: b"Omacr",
        html5: Some(b"\xc5\x8c"),
        html4: None,
    },
    Charref {
        name: b"Omega",
        html5: Some(b"\xce\xa9"),
        html4: Some(b"\xce\xa9"),
    },
    Charref {
        name: b"Omicron",
        html5: Some(b"\xce\x9f"),
        html4: Some(b"\xce\x9f"),
    },
    Charref {
        name: b"Oopf",
        html5: Some(b"\xf0\x9d\x95\x86"),
        html4: None,
    },
    Charref {
        name: b"OpenCurlyDoubleQuote",
        html5: Some(b"\xe2\x80\x9c"),
        html4: None,
    },
    Charref {
        name: b"OpenCurlyQuote",
        html5: Some(b"\xe2\x80\x98"),
        html4: None,
    },
    Charref {
        name: b"Or",
        html5: Some(b"\xe2\xa9\x94"),
        html4: None,
    },
    Charref {
        name: b"Oscr",
        html5: Some(b"\xf0\x9d\x92\xaa"),
        html4: None,
    },
    Charref {
        name: b"Oslash",
        html5: Some(b"\xc3\x98"),
        html4: Some(b"\xc3\x98"),
    },
    Charref {
        name: b"Otilde",
        html5: Some(b"\xc3\x95"),
        html4: Some(b"\xc3\x95"),
    },
    Charref {
        name: b"Otimes",
        html5: Some(b"\xe2\xa8\xb7"),
        html4: None,
    },
    Charref {
        name: b"Ouml",
        html5: Some(b"\xc3\x96"),
        html4: Some(b"\xc3\x96"),
    },
    Charref {
        name: b"OverBar",
        html5: Some(b"\xe2\x80\xbe"),
        html4: None,
    },
    Charref {
        name: b"OverBrace",
        html5: Some(b"\xe2\x8f\x9e"),
        html4: None,
    },
    Charref {
        name: b"OverBracket",
        html5: Some(b"\xe2\x8e\xb4"),
        html4: None,
    },
    Charref {
        name: b"OverParenthesis",
        html5: Some(b"\xe2\x8f\x9c"),
        html4: None,
    },
    Charref {
        name: b"PartialD",
        html5: Some(b"\xe2\x88\x82"),
        html4: None,
    },
    Charref {
        name: b"Pcy",
        html5: Some(b"\xd0\x9f"),
        html4: None,
    },
    Charref {
        name: b"Pfr",
        html5: Some(b"\xf0\x9d\x94\x93"),
        html4: None,
    },
    Charref {
        name: b"Phi",
        html5: Some(b"\xce\xa6"),
        html4: Some(b"\xce\xa6"),
    },
    Charref {
        name: b"Pi",
        html5: Some(b"\xce\xa0"),
        html4: Some(b"\xce\xa0"),
    },
    Charref {
        name: b"PlusMinus",
        html5: Some(b"\xc2\xb1"),
        html4: None,
    },
    Charref {
        name: b"Poincareplane",
        html5: Some(b"\xe2\x84\x8c"),
        html4: None,
    },
    Charref {
        name: b"Popf",
        html5: Some(b"\xe2\x84\x99"),
        html4: None,
    },
    Charref {
        name: b"Pr",
        html5: Some(b"\xe2\xaa\xbb"),
        html4: None,
    },
    Charref {
        name: b"Precedes",
        html5: Some(b"\xe2\x89\xba"),
        html4: None,
    },
    Charref {
        name: b"PrecedesEqual",
        html5: Some(b"\xe2\xaa\xaf"),
        html4: None,
    },
    Charref {
        name: b"PrecedesSlantEqual",
        html5: Some(b"\xe2\x89\xbc"),
        html4: None,
    },
    Charref {
        name: b"PrecedesTilde",
        html5: Some(b"\xe2\x89\xbe"),
        html4: None,
    },
    Charref {
        name: b"Prime",
        html5: Some(b"\xe2\x80\xb3"),
        html4: Some(b"\xe2\x80\xb3"),
    },
    Charref {
        name: b"Product",
        html5: Some(b"\xe2\x88\x8f"),
        html4: None,
    },
    Charref {
        name: b"Proportion",
        html5: Some(b"\xe2\x88\xb7"),
        html4: None,
    },
    Charref {
        name: b"Proportional",
        html5: Some(b"\xe2\x88\x9d"),
        html4: None,
    },
    Charref {
        name: b"Pscr",
        html5: Some(b"\xf0\x9d\x92\xab"),
        html4: None,
    },
    Charref {
        name: b"Psi",
        html5: Some(b"\xce\xa8"),
        html4: Some(b"\xce\xa8"),
    },
    Charref {
        name: b"QUOT",
        html5: Some(b"\""),
        html4: None,
    },
    Charref {
        name: b"Qfr",
        html5: Some(b"\xf0\x9d\x94\x94"),
        html4: None,
    },
    Charref {
        name: b"Qopf",
        html5: Some(b"\xe2\x84\x9a"),
        html4: None,
    },
    Charref {
        name: b"Qscr",
        html5: Some(b"\xf0\x9d\x92\xac"),
        html4: None,
    },
    Charref {
        name: b"RBarr",
        html5: Some(b"\xe2\xa4\x90"),
        html4: None,
    },
    Charref {
        name: b"REG",
        html5: Some(b"\xc2\xae"),
        html4: None,
    },
    Charref {
        name: b"Racute",
        html5: Some(b"\xc5\x94"),
        html4: None,
    },
    Charref {
        name: b"Rang",
        html5: Some(b"\xe2\x9f\xab"),
        html4: None,
    },
    Charref {
        name: b"Rarr",
        html5: Some(b"\xe2\x86\xa0"),
        html4: None,
    },
    Charref {
        name: b"Rarrtl",
        html5: Some(b"\xe2\xa4\x96"),
        html4: None,
    },
    Charref {
        name: b"Rcaron",
        html5: Some(b"\xc5\x98"),
        html4: None,
    },
    Charref {
        name: b"Rcedil",
        html5: Some(b"\xc5\x96"),
        html4: None,
    },
    Charref {
        name: b"Rcy",
        html5: Some(b"\xd0\xa0"),
        html4: None,
    },
    Charref {
        name: b"Re",
        html5: Some(b"\xe2\x84\x9c"),
        html4: None,
    },
    Charref {
        name: b"ReverseElement",
        html5: Some(b"\xe2\x88\x8b"),
        html4: None,
    },
    Charref {
        name: b"ReverseEquilibrium",
        html5: Some(b"\xe2\x87\x8b"),
        html4: None,
    },
    Charref {
        name: b"ReverseUpEquilibrium",
        html5: Some(b"\xe2\xa5\xaf"),
        html4: None,
    },
    Charref {
        name: b"Rfr",
        html5: Some(b"\xe2\x84\x9c"),
        html4: None,
    },
    Charref {
        name: b"Rho",
        html5: Some(b"\xce\xa1"),
        html4: Some(b"\xce\xa1"),
    },
    Charref {
        name: b"RightAngleBracket",
        html5: Some(b"\xe2\x9f\xa9"),
        html4: None,
    },
    Charref {
        name: b"RightArrow",
        html5: Some(b"\xe2\x86\x92"),
        html4: None,
    },
    Charref {
        name: b"RightArrowBar",
        html5: Some(b"\xe2\x87\xa5"),
        html4: None,
    },
    Charref {
        name: b"RightArrowLeftArrow",
        html5: Some(b"\xe2\x87\x84"),
        html4: None,
    },
    Charref {
        name: b"RightCeiling",
        html5: Some(b"\xe2\x8c\x89"),
        html4: None,
    },
    Charref {
        name: b"RightDoubleBracket",
        html5: Some(b"\xe2\x9f\xa7"),
        html4: None,
    },
    Charref {
        name: b"RightDownTeeVector",
        html5: Some(b"\xe2\xa5\x9d"),
        html4: None,
    },
    Charref {
        name: b"RightDownVector",
        html5: Some(b"\xe2\x87\x82"),
        html4: None,
    },
    Charref {
        name: b"RightDownVectorBar",
        html5: Some(b"\xe2\xa5\x95"),
        html4: None,
    },
    Charref {
        name: b"RightFloor",
        html5: Some(b"\xe2\x8c\x8b"),
        html4: None,
    },
    Charref {
        name: b"RightTee",
        html5: Some(b"\xe2\x8a\xa2"),
        html4: None,
    },
    Charref {
        name: b"RightTeeArrow",
        html5: Some(b"\xe2\x86\xa6"),
        html4: None,
    },
    Charref {
        name: b"RightTeeVector",
        html5: Some(b"\xe2\xa5\x9b"),
        html4: None,
    },
    Charref {
        name: b"RightTriangle",
        html5: Some(b"\xe2\x8a\xb3"),
        html4: None,
    },
    Charref {
        name: b"RightTriangleBar",
        html5: Some(b"\xe2\xa7\x90"),
        html4: None,
    },
    Charref {
        name: b"RightTriangleEqual",
        html5: Some(b"\xe2\x8a\xb5"),
        html4: None,
    },
    Charref {
        name: b"RightUpDownVector",
        html5: Some(b"\xe2\xa5\x8f"),
        html4: None,
    },
    Charref {
        name: b"RightUpTeeVector",
        html5: Some(b"\xe2\xa5\x9c"),
        html4: None,
    },
    Charref {
        name: b"RightUpVector",
        html5: Some(b"\xe2\x86\xbe"),
        html4: None,
    },
    Charref {
        name: b"RightUpVectorBar",
        html5: Some(b"\xe2\xa5\x94"),
        html4: None,
    },
    Charref {
        name: b"RightVector",
        html5: Some(b"\xe2\x87\x80"),
        html4: None,
    },
    Charref {
        name: b"RightVectorBar",
        html5: Some(b"\xe2\xa5\x93"),
        html4: None,
    },
    Charref {
        name: b"Rightarrow",
        html5: Some(b"\xe2\x87\x92"),
        html4: None,
    },
    Charref {
        name: b"Ropf",
        html5: Some(b"\xe2\x84\x9d"),
        html4: None,
    },
    Charref {
        name: b"RoundImplies",
        html5: Some(b"\xe2\xa5\xb0"),
        html4: None,
    },
    Charref {
        name: b"Rrightarrow",
        html5: Some(b"\xe2\x87\x9b"),
        html4: None,
    },
    Charref {
        name: b"Rscr",
        html5: Some(b"\xe2\x84\x9b"),
        html4: None,
    },
    Charref {
        name: b"Rsh",
        html5: Some(b"\xe2\x86\xb1"),
        html4: None,
    },
    Charref {
        name: b"RuleDelayed",
        html5: Some(b"\xe2\xa7\xb4"),
        html4: None,
    },
    Charref {
        name: b"SHCHcy",
        html5: Some(b"\xd0\xa9"),
        html4: None,
    },
    Charref {
        name: b"SHcy",
        html5: Some(b"\xd0\xa8"),
        html4: None,
    },
    Charref {
        name: b"SOFTcy",
        html5: Some(b"\xd0\xac"),
        html4: None,
    },
    Charref {
        name: b"Sacute",
        html5: Some(b"\xc5\x9a"),
        html4: None,
    },
    Charref {
        name: b"Sc",
        html5: Some(b"\xe2\xaa\xbc"),
        html4: None,
    },
    Charref {
        name: b"Scaron",
        html5: Some(b"\xc5\xa0"),
        html4: Some(b"\xc5\xa0"),
    },
    Charref {
        name: b"Scedil",
        html5: Some(b"\xc5\x9e"),
        html4: None,
    },
    Charref {
        name: b"Scirc",
        html5: Some(b"\xc5\x9c"),
        html4: None,
    },
    Charref {
        name: b"Scy",
        html5: Some(b"\xd0\xa1"),
        html4: None,
    },
    Charref {
        name: b"Sfr",
        html5: Some(b"\xf0\x9d\x94\x96"),
        html4: None,
    },
    Charref {
        name: b"ShortDownArrow",
        html5: Some(b"\xe2\x86\x93"),
        html4: None,
    },
    Charref {
        name: b"ShortLeftArrow",
        html5: Some(b"\xe2\x86\x90"),
        html4: None,
    },
    Charref {
        name: b"ShortRightArrow",
        html5: Some(b"\xe2\x86\x92"),
        html4: None,
    },
    Charref {
        name: b"ShortUpArrow",
        html5: Some(b"\xe2\x86\x91"),
        html4: None,
    },
    Charref {
        name: b"Sigma",
        html5: Some(b"\xce\xa3"),
        html4: Some(b"\xce\xa3"),
    },
    Charref {
        name: b"SmallCircle",
        html5: Some(b"\xe2\x88\x98"),
        html4: None,
    },
    Charref {
        name: b"Sopf",
        html5: Some(b"\xf0\x9d\x95\x8a"),
        html4: None,
    },
    Charref {
        name: b"Sqrt",
        html5: Some(b"\xe2\x88\x9a"),
        html4: None,
    },
    Charref {
        name: b"Square",
        html5: Some(b"\xe2\x96\xa1"),
        html4: None,
    },
    Charref {
        name: b"SquareIntersection",
        html5: Some(b"\xe2\x8a\x93"),
        html4: None,
    },
    Charref {
        name: b"SquareSubset",
        html5: Some(b"\xe2\x8a\x8f"),
        html4: None,
    },
    Charref {
        name: b"SquareSubsetEqual",
        html5: Some(b"\xe2\x8a\x91"),
        html4: None,
    },
    Charref {
        name: b"SquareSuperset",
        html5: Some(b"\xe2\x8a\x90"),
        html4: None,
    },
    Charref {
        name: b"SquareSupersetEqual",
        html5: Some(b"\xe2\x8a\x92"),
        html4: None,
    },
    Charref {
        name: b"SquareUnion",
        html5: Some(b"\xe2\x8a\x94"),
        html4: None,
    },
    Charref {
        name: b"Sscr",
        html5: Some(b"\xf0\x9d\x92\xae"),
        html4: None,
    },
    Charref {
        name: b"Star",
        html5: Some(b"\xe2\x8b\x86"),
        html4: None,
    },
    Charref {
        name: b"Sub",
        html5: Some(b"\xe2\x8b\x90"),
        html4: None,
    },
    Charref {
        name: b"Subset",
        html5: Some(b"\xe2\x8b\x90"),
        html4: None,
    },
    Charref {
        name: b"SubsetEqual",
        html5: Some(b"\xe2\x8a\x86"),
        html4: None,
    },
    Charref {
        name: b"Succeeds",
        html5: Some(b"\xe2\x89\xbb"),
        html4: None,
    },
    Charref {
        name: b"SucceedsEqual",
        html5: Some(b"\xe2\xaa\xb0"),
        html4: None,
    },
    Charref {
        name: b"SucceedsSlantEqual",
        html5: Some(b"\xe2\x89\xbd"),
        html4: None,
    },
    Charref {
        name: b"SucceedsTilde",
        html5: Some(b"\xe2\x89\xbf"),
        html4: None,
    },
    Charref {
        name: b"SuchThat",
        html5: Some(b"\xe2\x88\x8b"),
        html4: None,
    },
    Charref {
        name: b"Sum",
        html5: Some(b"\xe2\x88\x91"),
        html4: None,
    },
    Charref {
        name: b"Sup",
        html5: Some(b"\xe2\x8b\x91"),
        html4: None,
    },
    Charref {
        name: b"Superset",
        html5: Some(b"\xe2\x8a\x83"),
        html4: None,
    },
    Charref {
        name: b"SupersetEqual",
        html5: Some(b"\xe2\x8a\x87"),
        html4: None,
    },
    Charref {
        name: b"Supset",
        html5: Some(b"\xe2\x8b\x91"),
        html4: None,
    },
    Charref {
        name: b"THORN",
        html5: Some(b"\xc3\x9e"),
        html4: Some(b"\xc3\x9e"),
    },
    Charref {
        name: b"TRADE",
        html5: Some(b"\xe2\x84\xa2"),
        html4: None,
    },
    Charref {
        name: b"TSHcy",
        html5: Some(b"\xd0\x8b"),
        html4: None,
    },
    Charref {
        name: b"TScy",
        html5: Some(b"\xd0\xa6"),
        html4: None,
    },
    Charref {
        name: b"Tab",
        html5: Some(b"\\t"),
        html4: None,
    },
    Charref {
        name: b"Tau",
        html5: Some(b"\xce\xa4"),
        html4: Some(b"\xce\xa4"),
    },
    Charref {
        name: b"Tcaron",
        html5: Some(b"\xc5\xa4"),
        html4: None,
    },
    Charref {
        name: b"Tcedil",
        html5: Some(b"\xc5\xa2"),
        html4: None,
    },
    Charref {
        name: b"Tcy",
        html5: Some(b"\xd0\xa2"),
        html4: None,
    },
    Charref {
        name: b"Tfr",
        html5: Some(b"\xf0\x9d\x94\x97"),
        html4: None,
    },
    Charref {
        name: b"Therefore",
        html5: Some(b"\xe2\x88\xb4"),
        html4: None,
    },
    Charref {
        name: b"Theta",
        html5: Some(b"\xce\x98"),
        html4: Some(b"\xce\x98"),
    },
    Charref {
        name: b"ThickSpace",
        html5: Some(b"\xe2\x81\x9f\xe2\x80\x8a"),
        html4: None,
    },
    Charref {
        name: b"ThinSpace",
        html5: Some(b"\xe2\x80\x89"),
        html4: None,
    },
    Charref {
        name: b"Tilde",
        html5: Some(b"\xe2\x88\xbc"),
        html4: None,
    },
    Charref {
        name: b"TildeEqual",
        html5: Some(b"\xe2\x89\x83"),
        html4: None,
    },
    Charref {
        name: b"TildeFullEqual",
        html5: Some(b"\xe2\x89\x85"),
        html4: None,
    },
    Charref {
        name: b"TildeTilde",
        html5: Some(b"\xe2\x89\x88"),
        html4: None,
    },
    Charref {
        name: b"Topf",
        html5: Some(b"\xf0\x9d\x95\x8b"),
        html4: None,
    },
    Charref {
        name: b"TripleDot",
        html5: Some(b"\xe2\x83\x9b"),
        html4: None,
    },
    Charref {
        name: b"Tscr",
        html5: Some(b"\xf0\x9d\x92\xaf"),
        html4: None,
    },
    Charref {
        name: b"Tstrok",
        html5: Some(b"\xc5\xa6"),
        html4: None,
    },
    Charref {
        name: b"Uacute",
        html5: Some(b"\xc3\x9a"),
        html4: Some(b"\xc3\x9a"),
    },
    Charref {
        name: b"Uarr",
        html5: Some(b"\xe2\x86\x9f"),
        html4: None,
    },
    Charref {
        name: b"Uarrocir",
        html5: Some(b"\xe2\xa5\x89"),
        html4: None,
    },
    Charref {
        name: b"Ubrcy",
        html5: Some(b"\xd0\x8e"),
        html4: None,
    },
    Charref {
        name: b"Ubreve",
        html5: Some(b"\xc5\xac"),
        html4: None,
    },
    Charref {
        name: b"Ucirc",
        html5: Some(b"\xc3\x9b"),
        html4: Some(b"\xc3\x9b"),
    },
    Charref {
        name: b"Ucy",
        html5: Some(b"\xd0\xa3"),
        html4: None,
    },
    Charref {
        name: b"Udblac",
        html5: Some(b"\xc5\xb0"),
        html4: None,
    },
    Charref {
        name: b"Ufr",
        html5: Some(b"\xf0\x9d\x94\x98"),
        html4: None,
    },
    Charref {
        name: b"Ugrave",
        html5: Some(b"\xc3\x99"),
        html4: Some(b"\xc3\x99"),
    },
    Charref {
        name: b"Umacr",
        html5: Some(b"\xc5\xaa"),
        html4: None,
    },
    Charref {
        name: b"UnderBar",
        html5: Some(b"_"),
        html4: None,
    },
    Charref {
        name: b"UnderBrace",
        html5: Some(b"\xe2\x8f\x9f"),
        html4: None,
    },
    Charref {
        name: b"UnderBracket",
        html5: Some(b"\xe2\x8e\xb5"),
        html4: None,
    },
    Charref {
        name: b"UnderParenthesis",
        html5: Some(b"\xe2\x8f\x9d"),
        html4: None,
    },
    Charref {
        name: b"Union",
        html5: Some(b"\xe2\x8b\x83"),
        html4: None,
    },
    Charref {
        name: b"UnionPlus",
        html5: Some(b"\xe2\x8a\x8e"),
        html4: None,
    },
    Charref {
        name: b"Uogon",
        html5: Some(b"\xc5\xb2"),
        html4: None,
    },
    Charref {
        name: b"Uopf",
        html5: Some(b"\xf0\x9d\x95\x8c"),
        html4: None,
    },
    Charref {
        name: b"UpArrow",
        html5: Some(b"\xe2\x86\x91"),
        html4: None,
    },
    Charref {
        name: b"UpArrowBar",
        html5: Some(b"\xe2\xa4\x92"),
        html4: None,
    },
    Charref {
        name: b"UpArrowDownArrow",
        html5: Some(b"\xe2\x87\x85"),
        html4: None,
    },
    Charref {
        name: b"UpDownArrow",
        html5: Some(b"\xe2\x86\x95"),
        html4: None,
    },
    Charref {
        name: b"UpEquilibrium",
        html5: Some(b"\xe2\xa5\xae"),
        html4: None,
    },
    Charref {
        name: b"UpTee",
        html5: Some(b"\xe2\x8a\xa5"),
        html4: None,
    },
    Charref {
        name: b"UpTeeArrow",
        html5: Some(b"\xe2\x86\xa5"),
        html4: None,
    },
    Charref {
        name: b"Uparrow",
        html5: Some(b"\xe2\x87\x91"),
        html4: None,
    },
    Charref {
        name: b"Updownarrow",
        html5: Some(b"\xe2\x87\x95"),
        html4: None,
    },
    Charref {
        name: b"UpperLeftArrow",
        html5: Some(b"\xe2\x86\x96"),
        html4: None,
    },
    Charref {
        name: b"UpperRightArrow",
        html5: Some(b"\xe2\x86\x97"),
        html4: None,
    },
    Charref {
        name: b"Upsi",
        html5: Some(b"\xcf\x92"),
        html4: None,
    },
    Charref {
        name: b"Upsilon",
        html5: Some(b"\xce\xa5"),
        html4: Some(b"\xce\xa5"),
    },
    Charref {
        name: b"Uring",
        html5: Some(b"\xc5\xae"),
        html4: None,
    },
    Charref {
        name: b"Uscr",
        html5: Some(b"\xf0\x9d\x92\xb0"),
        html4: None,
    },
    Charref {
        name: b"Utilde",
        html5: Some(b"\xc5\xa8"),
        html4: None,
    },
    Charref {
        name: b"Uuml",
        html5: Some(b"\xc3\x9c"),
        html4: Some(b"\xc3\x9c"),
    },
    Charref {
        name: b"VDash",
        html5: Some(b"\xe2\x8a\xab"),
        html4: None,
    },
    Charref {
        name: b"Vbar",
        html5: Some(b"\xe2\xab\xab"),
        html4: None,
    },
    Charref {
        name: b"Vcy",
        html5: Some(b"\xd0\x92"),
        html4: None,
    },
    Charref {
        name: b"Vdash",
        html5: Some(b"\xe2\x8a\xa9"),
        html4: None,
    },
    Charref {
        name: b"Vdashl",
        html5: Some(b"\xe2\xab\xa6"),
        html4: None,
    },
    Charref {
        name: b"Vee",
        html5: Some(b"\xe2\x8b\x81"),
        html4: None,
    },
    Charref {
        name: b"Verbar",
        html5: Some(b"\xe2\x80\x96"),
        html4: None,
    },
    Charref {
        name: b"Vert",
        html5: Some(b"\xe2\x80\x96"),
        html4: None,
    },
    Charref {
        name: b"VerticalBar",
        html5: Some(b"\xe2\x88\xa3"),
        html4: None,
    },
    Charref {
        name: b"VerticalLine",
        html5: Some(b"|"),
        html4: None,
    },
    Charref {
        name: b"VerticalSeparator",
        html5: Some(b"\xe2\x9d\x98"),
        html4: None,
    },
    Charref {
        name: b"VerticalTilde",
        html5: Some(b"\xe2\x89\x80"),
        html4: None,
    },
    Charref {
        name: b"VeryThinSpace",
        html5: Some(b"\xe2\x80\x8a"),
        html4: None,
    },
    Charref {
        name: b"Vfr",
        html5: Some(b"\xf0\x9d\x94\x99"),
        html4: None,
    },
    Charref {
        name: b"Vopf",
        html5: Some(b"\xf0\x9d\x95\x8d"),
        html4: None,
    },
    Charref {
        name: b"Vscr",
        html5: Some(b"\xf0\x9d\x92\xb1"),
        html4: None,
    },
    Charref {
        name: b"Vvdash",
        html5: Some(b"\xe2\x8a\xaa"),
        html4: None,
    },
    Charref {
        name: b"Wcirc",
        html5: Some(b"\xc5\xb4"),
        html4: None,
    },
    Charref {
        name: b"Wedge",
        html5: Some(b"\xe2\x8b\x80"),
        html4: None,
    },
    Charref {
        name: b"Wfr",
        html5: Some(b"\xf0\x9d\x94\x9a"),
        html4: None,
    },
    Charref {
        name: b"Wopf",
        html5: Some(b"\xf0\x9d\x95\x8e"),
        html4: None,
    },
    Charref {
        name: b"Wscr",
        html5: Some(b"\xf0\x9d\x92\xb2"),
        html4: None,
    },
    Charref {
        name: b"Xfr",
        html5: Some(b"\xf0\x9d\x94\x9b"),
        html4: None,
    },
    Charref {
        name: b"Xi",
        html5: Some(b"\xce\x9e"),
        html4: Some(b"\xce\x9e"),
    },
    Charref {
        name: b"Xopf",
        html5: Some(b"\xf0\x9d\x95\x8f"),
        html4: None,
    },
    Charref {
        name: b"Xscr",
        html5: Some(b"\xf0\x9d\x92\xb3"),
        html4: None,
    },
    Charref {
        name: b"YAcy",
        html5: Some(b"\xd0\xaf"),
        html4: None,
    },
    Charref {
        name: b"YIcy",
        html5: Some(b"\xd0\x87"),
        html4: None,
    },
    Charref {
        name: b"YUcy",
        html5: Some(b"\xd0\xae"),
        html4: None,
    },
    Charref {
        name: b"Yacute",
        html5: Some(b"\xc3\x9d"),
        html4: Some(b"\xc3\x9d"),
    },
    Charref {
        name: b"Ycirc",
        html5: Some(b"\xc5\xb6"),
        html4: None,
    },
    Charref {
        name: b"Ycy",
        html5: Some(b"\xd0\xab"),
        html4: None,
    },
    Charref {
        name: b"Yfr",
        html5: Some(b"\xf0\x9d\x94\x9c"),
        html4: None,
    },
    Charref {
        name: b"Yopf",
        html5: Some(b"\xf0\x9d\x95\x90"),
        html4: None,
    },
    Charref {
        name: b"Yscr",
        html5: Some(b"\xf0\x9d\x92\xb4"),
        html4: None,
    },
    Charref {
        name: b"Yuml",
        html5: Some(b"\xc5\xb8"),
        html4: Some(b"\xc5\xb8"),
    },
    Charref {
        name: b"ZHcy",
        html5: Some(b"\xd0\x96"),
        html4: None,
    },
    Charref {
        name: b"Zacute",
        html5: Some(b"\xc5\xb9"),
        html4: None,
    },
    Charref {
        name: b"Zcaron",
        html5: Some(b"\xc5\xbd"),
        html4: None,
    },
    Charref {
        name: b"Zcy",
        html5: Some(b"\xd0\x97"),
        html4: None,
    },
    Charref {
        name: b"Zdot",
        html5: Some(b"\xc5\xbb"),
        html4: None,
    },
    Charref {
        name: b"ZeroWidthSpace",
        html5: Some(b"\xe2\x80\x8b"),
        html4: None,
    },
    Charref {
        name: b"Zeta",
        html5: Some(b"\xce\x96"),
        html4: Some(b"\xce\x96"),
    },
    Charref {
        name: b"Zfr",
        html5: Some(b"\xe2\x84\xa8"),
        html4: None,
    },
    Charref {
        name: b"Zopf",
        html5: Some(b"\xe2\x84\xa4"),
        html4: None,
    },
    Charref {
        name: b"Zscr",
        html5: Some(b"\xf0\x9d\x92\xb5"),
        html4: None,
    },
    Charref {
        name: b"aacute",
        html5: Some(b"\xc3\xa1"),
        html4: Some(b"\xc3\xa1"),
    },
    Charref {
        name: b"abreve",
        html5: Some(b"\xc4\x83"),
        html4: None,
    },
    Charref {
        name: b"ac",
        html5: Some(b"\xe2\x88\xbe"),
        html4: None,
    },
    Charref {
        name: b"acE",
        html5: Some(b"\xe2\x88\xbe\xcc\xb3"),
        html4: None,
    },
    Charref {
        name: b"acd",
        html5: Some(b"\xe2\x88\xbf"),
        html4: None,
    },
    Charref {
        name: b"acirc",
        html5: Some(b"\xc3\xa2"),
        html4: Some(b"\xc3\xa2"),
    },
    Charref {
        name: b"acute",
        html5: Some(b"\xc2\xb4"),
        html4: Some(b"\xc2\xb4"),
    },
    Charref {
        name: b"acy",
        html5: Some(b"\xd0\xb0"),
        html4: None,
    },
    Charref {
        name: b"aelig",
        html5: Some(b"\xc3\xa6"),
        html4: Some(b"\xc3\xa6"),
    },
    Charref {
        name: b"af",
        html5: Some(b"\xe2\x81\xa1"),
        html4: None,
    },
    Charref {
        name: b"afr",
        html5: Some(b"\xf0\x9d\x94\x9e"),
        html4: None,
    },
    Charref {
        name: b"agrave",
        html5: Some(b"\xc3\xa0"),
        html4: Some(b"\xc3\xa0"),
    },
    Charref {
        name: b"alefsym",
        html5: Some(b"\xe2\x84\xb5"),
        html4: Some(b"\xe2\x84\xb5"),
    },
    Charref {
        name: b"aleph",
        html5: Some(b"\xe2\x84\xb5"),
        html4: None,
    },
    Charref {
        name: b"alpha",
        html5: Some(b"\xce\xb1"),
        html4: Some(b"\xce\xb1"),
    },
    Charref {
        name: b"amacr",
        html5: Some(b"\xc4\x81"),
        html4: None,
    },
    Charref {
        name: b"amalg",
        html5: Some(b"\xe2\xa8\xbf"),
        html4: None,
    },
    Charref {
        name: b"amp",
        html5: Some(b"&"),
        html4: Some(b"&"),
    },
    Charref {
        name: b"and",
        html5: Some(b"\xe2\x88\xa7"),
        html4: Some(b"\xe2\x88\xa7"),
    },
    Charref {
        name: b"andand",
        html5: Some(b"\xe2\xa9\x95"),
        html4: None,
    },
    Charref {
        name: b"andd",
        html5: Some(b"\xe2\xa9\x9c"),
        html4: None,
    },
    Charref {
        name: b"andslope",
        html5: Some(b"\xe2\xa9\x98"),
        html4: None,
    },
    Charref {
        name: b"andv",
        html5: Some(b"\xe2\xa9\x9a"),
        html4: None,
    },
    Charref {
        name: b"ang",
        html5: Some(b"\xe2\x88\xa0"),
        html4: Some(b"\xe2\x88\xa0"),
    },
    Charref {
        name: b"ange",
        html5: Some(b"\xe2\xa6\xa4"),
        html4: None,
    },
    Charref {
        name: b"angle",
        html5: Some(b"\xe2\x88\xa0"),
        html4: None,
    },
    Charref {
        name: b"angmsd",
        html5: Some(b"\xe2\x88\xa1"),
        html4: None,
    },
    Charref {
        name: b"angmsdaa",
        html5: Some(b"\xe2\xa6\xa8"),
        html4: None,
    },
    Charref {
        name: b"angmsdab",
        html5: Some(b"\xe2\xa6\xa9"),
        html4: None,
    },
    Charref {
        name: b"angmsdac",
        html5: Some(b"\xe2\xa6\xaa"),
        html4: None,
    },
    Charref {
        name: b"angmsdad",
        html5: Some(b"\xe2\xa6\xab"),
        html4: None,
    },
    Charref {
        name: b"angmsdae",
        html5: Some(b"\xe2\xa6\xac"),
        html4: None,
    },
    Charref {
        name: b"angmsdaf",
        html5: Some(b"\xe2\xa6\xad"),
        html4: None,
    },
    Charref {
        name: b"angmsdag",
        html5: Some(b"\xe2\xa6\xae"),
        html4: None,
    },
    Charref {
        name: b"angmsdah",
        html5: Some(b"\xe2\xa6\xaf"),
        html4: None,
    },
    Charref {
        name: b"angrt",
        html5: Some(b"\xe2\x88\x9f"),
        html4: None,
    },
    Charref {
        name: b"angrtvb",
        html5: Some(b"\xe2\x8a\xbe"),
        html4: None,
    },
    Charref {
        name: b"angrtvbd",
        html5: Some(b"\xe2\xa6\x9d"),
        html4: None,
    },
    Charref {
        name: b"angsph",
        html5: Some(b"\xe2\x88\xa2"),
        html4: None,
    },
    Charref {
        name: b"angst",
        html5: Some(b"\xc3\x85"),
        html4: None,
    },
    Charref {
        name: b"angzarr",
        html5: Some(b"\xe2\x8d\xbc"),
        html4: None,
    },
    Charref {
        name: b"aogon",
        html5: Some(b"\xc4\x85"),
        html4: None,
    },
    Charref {
        name: b"aopf",
        html5: Some(b"\xf0\x9d\x95\x92"),
        html4: None,
    },
    Charref {
        name: b"ap",
        html5: Some(b"\xe2\x89\x88"),
        html4: None,
    },
    Charref {
        name: b"apE",
        html5: Some(b"\xe2\xa9\xb0"),
        html4: None,
    },
    Charref {
        name: b"apacir",
        html5: Some(b"\xe2\xa9\xaf"),
        html4: None,
    },
    Charref {
        name: b"ape",
        html5: Some(b"\xe2\x89\x8a"),
        html4: None,
    },
    Charref {
        name: b"apid",
        html5: Some(b"\xe2\x89\x8b"),
        html4: None,
    },
    Charref {
        name: b"apos",
        html5: Some(b"'"),
        html4: None,
    },
    Charref {
        name: b"approx",
        html5: Some(b"\xe2\x89\x88"),
        html4: None,
    },
    Charref {
        name: b"approxeq",
        html5: Some(b"\xe2\x89\x8a"),
        html4: None,
    },
    Charref {
        name: b"aring",
        html5: Some(b"\xc3\xa5"),
        html4: Some(b"\xc3\xa5"),
    },
    Charref {
        name: b"ascr",
        html5: Some(b"\xf0\x9d\x92\xb6"),
        html4: None,
    },
    Charref {
        name: b"ast",
        html5: Some(b"*"),
        html4: None,
    },
    Charref {
        name: b"asymp",
        html5: Some(b"\xe2\x89\x88"),
        html4: Some(b"\xe2\x89\x88"),
    },
    Charref {
        name: b"asympeq",
        html5: Some(b"\xe2\x89\x8d"),
        html4: None,
    },
    Charref {
        name: b"atilde",
        html5: Some(b"\xc3\xa3"),
        html4: Some(b"\xc3\xa3"),
    },
    Charref {
        name: b"auml",
        html5: Some(b"\xc3\xa4"),
        html4: Some(b"\xc3\xa4"),
    },
    Charref {
        name: b"awconint",
        html5: Some(b"\xe2\x88\xb3"),
        html4: None,
    },
    Charref {
        name: b"awint",
        html5: Some(b"\xe2\xa8\x91"),
        html4: None,
    },
    Charref {
        name: b"bNot",
        html5: Some(b"\xe2\xab\xad"),
        html4: None,
    },
    Charref {
        name: b"backcong",
        html5: Some(b"\xe2\x89\x8c"),
        html4: None,
    },
    Charref {
        name: b"backepsilon",
        html5: Some(b"\xcf\xb6"),
        html4: None,
    },
    Charref {
        name: b"backprime",
        html5: Some(b"\xe2\x80\xb5"),
        html4: None,
    },
    Charref {
        name: b"backsim",
        html5: Some(b"\xe2\x88\xbd"),
        html4: None,
    },
    Charref {
        name: b"backsimeq",
        html5: Some(b"\xe2\x8b\x8d"),
        html4: None,
    },
    Charref {
        name: b"barvee",
        html5: Some(b"\xe2\x8a\xbd"),
        html4: None,
    },
    Charref {
        name: b"barwed",
        html5: Some(b"\xe2\x8c\x85"),
        html4: None,
    },
    Charref {
        name: b"barwedge",
        html5: Some(b"\xe2\x8c\x85"),
        html4: None,
    },
    Charref {
        name: b"bbrk",
        html5: Some(b"\xe2\x8e\xb5"),
        html4: None,
    },
    Charref {
        name: b"bbrktbrk",
        html5: Some(b"\xe2\x8e\xb6"),
        html4: None,
    },
    Charref {
        name: b"bcong",
        html5: Some(b"\xe2\x89\x8c"),
        html4: None,
    },
    Charref {
        name: b"bcy",
        html5: Some(b"\xd0\xb1"),
        html4: None,
    },
    Charref {
        name: b"bdquo",
        html5: Some(b"\xe2\x80\x9e"),
        html4: Some(b"\xe2\x80\x9e"),
    },
    Charref {
        name: b"becaus",
        html5: Some(b"\xe2\x88\xb5"),
        html4: None,
    },
    Charref {
        name: b"because",
        html5: Some(b"\xe2\x88\xb5"),
        html4: None,
    },
    Charref {
        name: b"bemptyv",
        html5: Some(b"\xe2\xa6\xb0"),
        html4: None,
    },
    Charref {
        name: b"bepsi",
        html5: Some(b"\xcf\xb6"),
        html4: None,
    },
    Charref {
        name: b"bernou",
        html5: Some(b"\xe2\x84\xac"),
        html4: None,
    },
    Charref {
        name: b"beta",
        html5: Some(b"\xce\xb2"),
        html4: Some(b"\xce\xb2"),
    },
    Charref {
        name: b"beth",
        html5: Some(b"\xe2\x84\xb6"),
        html4: None,
    },
    Charref {
        name: b"between",
        html5: Some(b"\xe2\x89\xac"),
        html4: None,
    },
    Charref {
        name: b"bfr",
        html5: Some(b"\xf0\x9d\x94\x9f"),
        html4: None,
    },
    Charref {
        name: b"bigcap",
        html5: Some(b"\xe2\x8b\x82"),
        html4: None,
    },
    Charref {
        name: b"bigcirc",
        html5: Some(b"\xe2\x97\xaf"),
        html4: None,
    },
    Charref {
        name: b"bigcup",
        html5: Some(b"\xe2\x8b\x83"),
        html4: None,
    },
    Charref {
        name: b"bigodot",
        html5: Some(b"\xe2\xa8\x80"),
        html4: None,
    },
    Charref {
        name: b"bigoplus",
        html5: Some(b"\xe2\xa8\x81"),
        html4: None,
    },
    Charref {
        name: b"bigotimes",
        html5: Some(b"\xe2\xa8\x82"),
        html4: None,
    },
    Charref {
        name: b"bigsqcup",
        html5: Some(b"\xe2\xa8\x86"),
        html4: None,
    },
    Charref {
        name: b"bigstar",
        html5: Some(b"\xe2\x98\x85"),
        html4: None,
    },
    Charref {
        name: b"bigtriangledown",
        html5: Some(b"\xe2\x96\xbd"),
        html4: None,
    },
    Charref {
        name: b"bigtriangleup",
        html5: Some(b"\xe2\x96\xb3"),
        html4: None,
    },
    Charref {
        name: b"biguplus",
        html5: Some(b"\xe2\xa8\x84"),
        html4: None,
    },
    Charref {
        name: b"bigvee",
        html5: Some(b"\xe2\x8b\x81"),
        html4: None,
    },
    Charref {
        name: b"bigwedge",
        html5: Some(b"\xe2\x8b\x80"),
        html4: None,
    },
    Charref {
        name: b"bkarow",
        html5: Some(b"\xe2\xa4\x8d"),
        html4: None,
    },
    Charref {
        name: b"blacklozenge",
        html5: Some(b"\xe2\xa7\xab"),
        html4: None,
    },
    Charref {
        name: b"blacksquare",
        html5: Some(b"\xe2\x96\xaa"),
        html4: None,
    },
    Charref {
        name: b"blacktriangle",
        html5: Some(b"\xe2\x96\xb4"),
        html4: None,
    },
    Charref {
        name: b"blacktriangledown",
        html5: Some(b"\xe2\x96\xbe"),
        html4: None,
    },
    Charref {
        name: b"blacktriangleleft",
        html5: Some(b"\xe2\x97\x82"),
        html4: None,
    },
    Charref {
        name: b"blacktriangleright",
        html5: Some(b"\xe2\x96\xb8"),
        html4: None,
    },
    Charref {
        name: b"blank",
        html5: Some(b"\xe2\x90\xa3"),
        html4: None,
    },
    Charref {
        name: b"blk12",
        html5: Some(b"\xe2\x96\x92"),
        html4: None,
    },
    Charref {
        name: b"blk14",
        html5: Some(b"\xe2\x96\x91"),
        html4: None,
    },
    Charref {
        name: b"blk34",
        html5: Some(b"\xe2\x96\x93"),
        html4: None,
    },
    Charref {
        name: b"block",
        html5: Some(b"\xe2\x96\x88"),
        html4: None,
    },
    Charref {
        name: b"bne",
        html5: Some(b"=\xe2\x83\xa5"),
        html4: None,
    },
    Charref {
        name: b"bnequiv",
        html5: Some(b"\xe2\x89\xa1\xe2\x83\xa5"),
        html4: None,
    },
    Charref {
        name: b"bnot",
        html5: Some(b"\xe2\x8c\x90"),
        html4: None,
    },
    Charref {
        name: b"bopf",
        html5: Some(b"\xf0\x9d\x95\x93"),
        html4: None,
    },
    Charref {
        name: b"bot",
        html5: Some(b"\xe2\x8a\xa5"),
        html4: None,
    },
    Charref {
        name: b"bottom",
        html5: Some(b"\xe2\x8a\xa5"),
        html4: None,
    },
    Charref {
        name: b"bowtie",
        html5: Some(b"\xe2\x8b\x88"),
        html4: None,
    },
    Charref {
        name: b"boxDL",
        html5: Some(b"\xe2\x95\x97"),
        html4: None,
    },
    Charref {
        name: b"boxDR",
        html5: Some(b"\xe2\x95\x94"),
        html4: None,
    },
    Charref {
        name: b"boxDl",
        html5: Some(b"\xe2\x95\x96"),
        html4: None,
    },
    Charref {
        name: b"boxDr",
        html5: Some(b"\xe2\x95\x93"),
        html4: None,
    },
    Charref {
        name: b"boxH",
        html5: Some(b"\xe2\x95\x90"),
        html4: None,
    },
    Charref {
        name: b"boxHD",
        html5: Some(b"\xe2\x95\xa6"),
        html4: None,
    },
    Charref {
        name: b"boxHU",
        html5: Some(b"\xe2\x95\xa9"),
        html4: None,
    },
    Charref {
        name: b"boxHd",
        html5: Some(b"\xe2\x95\xa4"),
        html4: None,
    },
    Charref {
        name: b"boxHu",
        html5: Some(b"\xe2\x95\xa7"),
        html4: None,
    },
    Charref {
        name: b"boxUL",
        html5: Some(b"\xe2\x95\x9d"),
        html4: None,
    },
    Charref {
        name: b"boxUR",
        html5: Some(b"\xe2\x95\x9a"),
        html4: None,
    },
    Charref {
        name: b"boxUl",
        html5: Some(b"\xe2\x95\x9c"),
        html4: None,
    },
    Charref {
        name: b"boxUr",
        html5: Some(b"\xe2\x95\x99"),
        html4: None,
    },
    Charref {
        name: b"boxV",
        html5: Some(b"\xe2\x95\x91"),
        html4: None,
    },
    Charref {
        name: b"boxVH",
        html5: Some(b"\xe2\x95\xac"),
        html4: None,
    },
    Charref {
        name: b"boxVL",
        html5: Some(b"\xe2\x95\xa3"),
        html4: None,
    },
    Charref {
        name: b"boxVR",
        html5: Some(b"\xe2\x95\xa0"),
        html4: None,
    },
    Charref {
        name: b"boxVh",
        html5: Some(b"\xe2\x95\xab"),
        html4: None,
    },
    Charref {
        name: b"boxVl",
        html5: Some(b"\xe2\x95\xa2"),
        html4: None,
    },
    Charref {
        name: b"boxVr",
        html5: Some(b"\xe2\x95\x9f"),
        html4: None,
    },
    Charref {
        name: b"boxbox",
        html5: Some(b"\xe2\xa7\x89"),
        html4: None,
    },
    Charref {
        name: b"boxdL",
        html5: Some(b"\xe2\x95\x95"),
        html4: None,
    },
    Charref {
        name: b"boxdR",
        html5: Some(b"\xe2\x95\x92"),
        html4: None,
    },
    Charref {
        name: b"boxdl",
        html5: Some(b"\xe2\x94\x90"),
        html4: None,
    },
    Charref {
        name: b"boxdr",
        html5: Some(b"\xe2\x94\x8c"),
        html4: None,
    },
    Charref {
        name: b"boxh",
        html5: Some(b"\xe2\x94\x80"),
        html4: None,
    },
    Charref {
        name: b"boxhD",
        html5: Some(b"\xe2\x95\xa5"),
        html4: None,
    },
    Charref {
        name: b"boxhU",
        html5: Some(b"\xe2\x95\xa8"),
        html4: None,
    },
    Charref {
        name: b"boxhd",
        html5: Some(b"\xe2\x94\xac"),
        html4: None,
    },
    Charref {
        name: b"boxhu",
        html5: Some(b"\xe2\x94\xb4"),
        html4: None,
    },
    Charref {
        name: b"boxminus",
        html5: Some(b"\xe2\x8a\x9f"),
        html4: None,
    },
    Charref {
        name: b"boxplus",
        html5: Some(b"\xe2\x8a\x9e"),
        html4: None,
    },
    Charref {
        name: b"boxtimes",
        html5: Some(b"\xe2\x8a\xa0"),
        html4: None,
    },
    Charref {
        name: b"boxuL",
        html5: Some(b"\xe2\x95\x9b"),
        html4: None,
    },
    Charref {
        name: b"boxuR",
        html5: Some(b"\xe2\x95\x98"),
        html4: None,
    },
    Charref {
        name: b"boxul",
        html5: Some(b"\xe2\x94\x98"),
        html4: None,
    },
    Charref {
        name: b"boxur",
        html5: Some(b"\xe2\x94\x94"),
        html4: None,
    },
    Charref {
        name: b"boxv",
        html5: Some(b"\xe2\x94\x82"),
        html4: None,
    },
    Charref {
        name: b"boxvH",
        html5: Some(b"\xe2\x95\xaa"),
        html4: None,
    },
    Charref {
        name: b"boxvL",
        html5: Some(b"\xe2\x95\xa1"),
        html4: None,
    },
    Charref {
        name: b"boxvR",
        html5: Some(b"\xe2\x95\x9e"),
        html4: None,
    },
    Charref {
        name: b"boxvh",
        html5: Some(b"\xe2\x94\xbc"),
        html4: None,
    },
    Charref {
        name: b"boxvl",
        html5: Some(b"\xe2\x94\xa4"),
        html4: None,
    },
    Charref {
        name: b"boxvr",
        html5: Some(b"\xe2\x94\x9c"),
        html4: None,
    },
    Charref {
        name: b"bprime",
        html5: Some(b"\xe2\x80\xb5"),
        html4: None,
    },
    Charref {
        name: b"breve",
        html5: Some(b"\xcb\x98"),
        html4: None,
    },
    Charref {
        name: b"brvbar",
        html5: Some(b"\xc2\xa6"),
        html4: Some(b"\xc2\xa6"),
    },
    Charref {
        name: b"bscr",
        html5: Some(b"\xf0\x9d\x92\xb7"),
        html4: None,
    },
    Charref {
        name: b"bsemi",
        html5: Some(b"\xe2\x81\x8f"),
        html4: None,
    },
    Charref {
        name: b"bsim",
        html5: Some(b"\xe2\x88\xbd"),
        html4: None,
    },
    Charref {
        name: b"bsime",
        html5: Some(b"\xe2\x8b\x8d"),
        html4: None,
    },
    Charref {
        name: b"bsol",
        html5: Some(b"\\"),
        html4: None,
    },
    Charref {
        name: b"bsolb",
        html5: Some(b"\xe2\xa7\x85"),
        html4: None,
    },
    Charref {
        name: b"bsolhsub",
        html5: Some(b"\xe2\x9f\x88"),
        html4: None,
    },
    Charref {
        name: b"bull",
        html5: Some(b"\xe2\x80\xa2"),
        html4: Some(b"\xe2\x80\xa2"),
    },
    Charref {
        name: b"bullet",
        html5: Some(b"\xe2\x80\xa2"),
        html4: None,
    },
    Charref {
        name: b"bump",
        html5: Some(b"\xe2\x89\x8e"),
        html4: None,
    },
    Charref {
        name: b"bumpE",
        html5: Some(b"\xe2\xaa\xae"),
        html4: None,
    },
    Charref {
        name: b"bumpe",
        html5: Some(b"\xe2\x89\x8f"),
        html4: None,
    },
    Charref {
        name: b"bumpeq",
        html5: Some(b"\xe2\x89\x8f"),
        html4: None,
    },
    Charref {
        name: b"cacute",
        html5: Some(b"\xc4\x87"),
        html4: None,
    },
    Charref {
        name: b"cap",
        html5: Some(b"\xe2\x88\xa9"),
        html4: Some(b"\xe2\x88\xa9"),
    },
    Charref {
        name: b"capand",
        html5: Some(b"\xe2\xa9\x84"),
        html4: None,
    },
    Charref {
        name: b"capbrcup",
        html5: Some(b"\xe2\xa9\x89"),
        html4: None,
    },
    Charref {
        name: b"capcap",
        html5: Some(b"\xe2\xa9\x8b"),
        html4: None,
    },
    Charref {
        name: b"capcup",
        html5: Some(b"\xe2\xa9\x87"),
        html4: None,
    },
    Charref {
        name: b"capdot",
        html5: Some(b"\xe2\xa9\x80"),
        html4: None,
    },
    Charref {
        name: b"caps",
        html5: Some(b"\xe2\x88\xa9\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"caret",
        html5: Some(b"\xe2\x81\x81"),
        html4: None,
    },
    Charref {
        name: b"caron",
        html5: Some(b"\xcb\x87"),
        html4: None,
    },
    Charref {
        name: b"ccaps",
        html5: Some(b"\xe2\xa9\x8d"),
        html4: None,
    },
    Charref {
        name: b"ccaron",
        html5: Some(b"\xc4\x8d"),
        html4: None,
    },
    Charref {
        name: b"ccedil",
        html5: Some(b"\xc3\xa7"),
        html4: Some(b"\xc3\xa7"),
    },
    Charref {
        name: b"ccirc",
        html5: Some(b"\xc4\x89"),
        html4: None,
    },
    Charref {
        name: b"ccups",
        html5: Some(b"\xe2\xa9\x8c"),
        html4: None,
    },
    Charref {
        name: b"ccupssm",
        html5: Some(b"\xe2\xa9\x90"),
        html4: None,
    },
    Charref {
        name: b"cdot",
        html5: Some(b"\xc4\x8b"),
        html4: None,
    },
    Charref {
        name: b"cedil",
        html5: Some(b"\xc2\xb8"),
        html4: Some(b"\xc2\xb8"),
    },
    Charref {
        name: b"cemptyv",
        html5: Some(b"\xe2\xa6\xb2"),
        html4: None,
    },
    Charref {
        name: b"cent",
        html5: Some(b"\xc2\xa2"),
        html4: Some(b"\xc2\xa2"),
    },
    Charref {
        name: b"centerdot",
        html5: Some(b"\xc2\xb7"),
        html4: None,
    },
    Charref {
        name: b"cfr",
        html5: Some(b"\xf0\x9d\x94\xa0"),
        html4: None,
    },
    Charref {
        name: b"chcy",
        html5: Some(b"\xd1\x87"),
        html4: None,
    },
    Charref {
        name: b"check",
        html5: Some(b"\xe2\x9c\x93"),
        html4: None,
    },
    Charref {
        name: b"checkmark",
        html5: Some(b"\xe2\x9c\x93"),
        html4: None,
    },
    Charref {
        name: b"chi",
        html5: Some(b"\xcf\x87"),
        html4: Some(b"\xcf\x87"),
    },
    Charref {
        name: b"cir",
        html5: Some(b"\xe2\x97\x8b"),
        html4: None,
    },
    Charref {
        name: b"cirE",
        html5: Some(b"\xe2\xa7\x83"),
        html4: None,
    },
    Charref {
        name: b"circ",
        html5: Some(b"\xcb\x86"),
        html4: Some(b"\xcb\x86"),
    },
    Charref {
        name: b"circeq",
        html5: Some(b"\xe2\x89\x97"),
        html4: None,
    },
    Charref {
        name: b"circlearrowleft",
        html5: Some(b"\xe2\x86\xba"),
        html4: None,
    },
    Charref {
        name: b"circlearrowright",
        html5: Some(b"\xe2\x86\xbb"),
        html4: None,
    },
    Charref {
        name: b"circledR",
        html5: Some(b"\xc2\xae"),
        html4: None,
    },
    Charref {
        name: b"circledS",
        html5: Some(b"\xe2\x93\x88"),
        html4: None,
    },
    Charref {
        name: b"circledast",
        html5: Some(b"\xe2\x8a\x9b"),
        html4: None,
    },
    Charref {
        name: b"circledcirc",
        html5: Some(b"\xe2\x8a\x9a"),
        html4: None,
    },
    Charref {
        name: b"circleddash",
        html5: Some(b"\xe2\x8a\x9d"),
        html4: None,
    },
    Charref {
        name: b"cire",
        html5: Some(b"\xe2\x89\x97"),
        html4: None,
    },
    Charref {
        name: b"cirfnint",
        html5: Some(b"\xe2\xa8\x90"),
        html4: None,
    },
    Charref {
        name: b"cirmid",
        html5: Some(b"\xe2\xab\xaf"),
        html4: None,
    },
    Charref {
        name: b"cirscir",
        html5: Some(b"\xe2\xa7\x82"),
        html4: None,
    },
    Charref {
        name: b"clubs",
        html5: Some(b"\xe2\x99\xa3"),
        html4: Some(b"\xe2\x99\xa3"),
    },
    Charref {
        name: b"clubsuit",
        html5: Some(b"\xe2\x99\xa3"),
        html4: None,
    },
    Charref {
        name: b"colon",
        html5: Some(b":"),
        html4: None,
    },
    Charref {
        name: b"colone",
        html5: Some(b"\xe2\x89\x94"),
        html4: None,
    },
    Charref {
        name: b"coloneq",
        html5: Some(b"\xe2\x89\x94"),
        html4: None,
    },
    Charref {
        name: b"comma",
        html5: Some(b","),
        html4: None,
    },
    Charref {
        name: b"commat",
        html5: Some(b"@"),
        html4: None,
    },
    Charref {
        name: b"comp",
        html5: Some(b"\xe2\x88\x81"),
        html4: None,
    },
    Charref {
        name: b"compfn",
        html5: Some(b"\xe2\x88\x98"),
        html4: None,
    },
    Charref {
        name: b"complement",
        html5: Some(b"\xe2\x88\x81"),
        html4: None,
    },
    Charref {
        name: b"complexes",
        html5: Some(b"\xe2\x84\x82"),
        html4: None,
    },
    Charref {
        name: b"cong",
        html5: Some(b"\xe2\x89\x85"),
        html4: Some(b"\xe2\x89\x85"),
    },
    Charref {
        name: b"congdot",
        html5: Some(b"\xe2\xa9\xad"),
        html4: None,
    },
    Charref {
        name: b"conint",
        html5: Some(b"\xe2\x88\xae"),
        html4: None,
    },
    Charref {
        name: b"copf",
        html5: Some(b"\xf0\x9d\x95\x94"),
        html4: None,
    },
    Charref {
        name: b"coprod",
        html5: Some(b"\xe2\x88\x90"),
        html4: None,
    },
    Charref {
        name: b"copy",
        html5: Some(b"\xc2\xa9"),
        html4: Some(b"\xc2\xa9"),
    },
    Charref {
        name: b"copysr",
        html5: Some(b"\xe2\x84\x97"),
        html4: None,
    },
    Charref {
        name: b"crarr",
        html5: Some(b"\xe2\x86\xb5"),
        html4: Some(b"\xe2\x86\xb5"),
    },
    Charref {
        name: b"cross",
        html5: Some(b"\xe2\x9c\x97"),
        html4: None,
    },
    Charref {
        name: b"cscr",
        html5: Some(b"\xf0\x9d\x92\xb8"),
        html4: None,
    },
    Charref {
        name: b"csub",
        html5: Some(b"\xe2\xab\x8f"),
        html4: None,
    },
    Charref {
        name: b"csube",
        html5: Some(b"\xe2\xab\x91"),
        html4: None,
    },
    Charref {
        name: b"csup",
        html5: Some(b"\xe2\xab\x90"),
        html4: None,
    },
    Charref {
        name: b"csupe",
        html5: Some(b"\xe2\xab\x92"),
        html4: None,
    },
    Charref {
        name: b"ctdot",
        html5: Some(b"\xe2\x8b\xaf"),
        html4: None,
    },
    Charref {
        name: b"cudarrl",
        html5: Some(b"\xe2\xa4\xb8"),
        html4: None,
    },
    Charref {
        name: b"cudarrr",
        html5: Some(b"\xe2\xa4\xb5"),
        html4: None,
    },
    Charref {
        name: b"cuepr",
        html5: Some(b"\xe2\x8b\x9e"),
        html4: None,
    },
    Charref {
        name: b"cuesc",
        html5: Some(b"\xe2\x8b\x9f"),
        html4: None,
    },
    Charref {
        name: b"cularr",
        html5: Some(b"\xe2\x86\xb6"),
        html4: None,
    },
    Charref {
        name: b"cularrp",
        html5: Some(b"\xe2\xa4\xbd"),
        html4: None,
    },
    Charref {
        name: b"cup",
        html5: Some(b"\xe2\x88\xaa"),
        html4: Some(b"\xe2\x88\xaa"),
    },
    Charref {
        name: b"cupbrcap",
        html5: Some(b"\xe2\xa9\x88"),
        html4: None,
    },
    Charref {
        name: b"cupcap",
        html5: Some(b"\xe2\xa9\x86"),
        html4: None,
    },
    Charref {
        name: b"cupcup",
        html5: Some(b"\xe2\xa9\x8a"),
        html4: None,
    },
    Charref {
        name: b"cupdot",
        html5: Some(b"\xe2\x8a\x8d"),
        html4: None,
    },
    Charref {
        name: b"cupor",
        html5: Some(b"\xe2\xa9\x85"),
        html4: None,
    },
    Charref {
        name: b"cups",
        html5: Some(b"\xe2\x88\xaa\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"curarr",
        html5: Some(b"\xe2\x86\xb7"),
        html4: None,
    },
    Charref {
        name: b"curarrm",
        html5: Some(b"\xe2\xa4\xbc"),
        html4: None,
    },
    Charref {
        name: b"curlyeqprec",
        html5: Some(b"\xe2\x8b\x9e"),
        html4: None,
    },
    Charref {
        name: b"curlyeqsucc",
        html5: Some(b"\xe2\x8b\x9f"),
        html4: None,
    },
    Charref {
        name: b"curlyvee",
        html5: Some(b"\xe2\x8b\x8e"),
        html4: None,
    },
    Charref {
        name: b"curlywedge",
        html5: Some(b"\xe2\x8b\x8f"),
        html4: None,
    },
    Charref {
        name: b"curren",
        html5: Some(b"\xc2\xa4"),
        html4: Some(b"\xc2\xa4"),
    },
    Charref {
        name: b"curvearrowleft",
        html5: Some(b"\xe2\x86\xb6"),
        html4: None,
    },
    Charref {
        name: b"curvearrowright",
        html5: Some(b"\xe2\x86\xb7"),
        html4: None,
    },
    Charref {
        name: b"cuvee",
        html5: Some(b"\xe2\x8b\x8e"),
        html4: None,
    },
    Charref {
        name: b"cuwed",
        html5: Some(b"\xe2\x8b\x8f"),
        html4: None,
    },
    Charref {
        name: b"cwconint",
        html5: Some(b"\xe2\x88\xb2"),
        html4: None,
    },
    Charref {
        name: b"cwint",
        html5: Some(b"\xe2\x88\xb1"),
        html4: None,
    },
    Charref {
        name: b"cylcty",
        html5: Some(b"\xe2\x8c\xad"),
        html4: None,
    },
    Charref {
        name: b"dArr",
        html5: Some(b"\xe2\x87\x93"),
        html4: Some(b"\xe2\x87\x93"),
    },
    Charref {
        name: b"dHar",
        html5: Some(b"\xe2\xa5\xa5"),
        html4: None,
    },
    Charref {
        name: b"dagger",
        html5: Some(b"\xe2\x80\xa0"),
        html4: Some(b"\xe2\x80\xa0"),
    },
    Charref {
        name: b"daleth",
        html5: Some(b"\xe2\x84\xb8"),
        html4: None,
    },
    Charref {
        name: b"darr",
        html5: Some(b"\xe2\x86\x93"),
        html4: Some(b"\xe2\x86\x93"),
    },
    Charref {
        name: b"dash",
        html5: Some(b"\xe2\x80\x90"),
        html4: None,
    },
    Charref {
        name: b"dashv",
        html5: Some(b"\xe2\x8a\xa3"),
        html4: None,
    },
    Charref {
        name: b"dbkarow",
        html5: Some(b"\xe2\xa4\x8f"),
        html4: None,
    },
    Charref {
        name: b"dblac",
        html5: Some(b"\xcb\x9d"),
        html4: None,
    },
    Charref {
        name: b"dcaron",
        html5: Some(b"\xc4\x8f"),
        html4: None,
    },
    Charref {
        name: b"dcy",
        html5: Some(b"\xd0\xb4"),
        html4: None,
    },
    Charref {
        name: b"dd",
        html5: Some(b"\xe2\x85\x86"),
        html4: None,
    },
    Charref {
        name: b"ddagger",
        html5: Some(b"\xe2\x80\xa1"),
        html4: None,
    },
    Charref {
        name: b"ddarr",
        html5: Some(b"\xe2\x87\x8a"),
        html4: None,
    },
    Charref {
        name: b"ddotseq",
        html5: Some(b"\xe2\xa9\xb7"),
        html4: None,
    },
    Charref {
        name: b"deg",
        html5: Some(b"\xc2\xb0"),
        html4: Some(b"\xc2\xb0"),
    },
    Charref {
        name: b"delta",
        html5: Some(b"\xce\xb4"),
        html4: Some(b"\xce\xb4"),
    },
    Charref {
        name: b"demptyv",
        html5: Some(b"\xe2\xa6\xb1"),
        html4: None,
    },
    Charref {
        name: b"dfisht",
        html5: Some(b"\xe2\xa5\xbf"),
        html4: None,
    },
    Charref {
        name: b"dfr",
        html5: Some(b"\xf0\x9d\x94\xa1"),
        html4: None,
    },
    Charref {
        name: b"dharl",
        html5: Some(b"\xe2\x87\x83"),
        html4: None,
    },
    Charref {
        name: b"dharr",
        html5: Some(b"\xe2\x87\x82"),
        html4: None,
    },
    Charref {
        name: b"diam",
        html5: Some(b"\xe2\x8b\x84"),
        html4: None,
    },
    Charref {
        name: b"diamond",
        html5: Some(b"\xe2\x8b\x84"),
        html4: None,
    },
    Charref {
        name: b"diamondsuit",
        html5: Some(b"\xe2\x99\xa6"),
        html4: None,
    },
    Charref {
        name: b"diams",
        html5: Some(b"\xe2\x99\xa6"),
        html4: Some(b"\xe2\x99\xa6"),
    },
    Charref {
        name: b"die",
        html5: Some(b"\xc2\xa8"),
        html4: None,
    },
    Charref {
        name: b"digamma",
        html5: Some(b"\xcf\x9d"),
        html4: None,
    },
    Charref {
        name: b"disin",
        html5: Some(b"\xe2\x8b\xb2"),
        html4: None,
    },
    Charref {
        name: b"div",
        html5: Some(b"\xc3\xb7"),
        html4: None,
    },
    Charref {
        name: b"divide",
        html5: Some(b"\xc3\xb7"),
        html4: Some(b"\xc3\xb7"),
    },
    Charref {
        name: b"divideontimes",
        html5: Some(b"\xe2\x8b\x87"),
        html4: None,
    },
    Charref {
        name: b"divonx",
        html5: Some(b"\xe2\x8b\x87"),
        html4: None,
    },
    Charref {
        name: b"djcy",
        html5: Some(b"\xd1\x92"),
        html4: None,
    },
    Charref {
        name: b"dlcorn",
        html5: Some(b"\xe2\x8c\x9e"),
        html4: None,
    },
    Charref {
        name: b"dlcrop",
        html5: Some(b"\xe2\x8c\x8d"),
        html4: None,
    },
    Charref {
        name: b"dollar",
        html5: Some(b"$"),
        html4: None,
    },
    Charref {
        name: b"dopf",
        html5: Some(b"\xf0\x9d\x95\x95"),
        html4: None,
    },
    Charref {
        name: b"dot",
        html5: Some(b"\xcb\x99"),
        html4: None,
    },
    Charref {
        name: b"doteq",
        html5: Some(b"\xe2\x89\x90"),
        html4: None,
    },
    Charref {
        name: b"doteqdot",
        html5: Some(b"\xe2\x89\x91"),
        html4: None,
    },
    Charref {
        name: b"dotminus",
        html5: Some(b"\xe2\x88\xb8"),
        html4: None,
    },
    Charref {
        name: b"dotplus",
        html5: Some(b"\xe2\x88\x94"),
        html4: None,
    },
    Charref {
        name: b"dotsquare",
        html5: Some(b"\xe2\x8a\xa1"),
        html4: None,
    },
    Charref {
        name: b"doublebarwedge",
        html5: Some(b"\xe2\x8c\x86"),
        html4: None,
    },
    Charref {
        name: b"downarrow",
        html5: Some(b"\xe2\x86\x93"),
        html4: None,
    },
    Charref {
        name: b"downdownarrows",
        html5: Some(b"\xe2\x87\x8a"),
        html4: None,
    },
    Charref {
        name: b"downharpoonleft",
        html5: Some(b"\xe2\x87\x83"),
        html4: None,
    },
    Charref {
        name: b"downharpoonright",
        html5: Some(b"\xe2\x87\x82"),
        html4: None,
    },
    Charref {
        name: b"drbkarow",
        html5: Some(b"\xe2\xa4\x90"),
        html4: None,
    },
    Charref {
        name: b"drcorn",
        html5: Some(b"\xe2\x8c\x9f"),
        html4: None,
    },
    Charref {
        name: b"drcrop",
        html5: Some(b"\xe2\x8c\x8c"),
        html4: None,
    },
    Charref {
        name: b"dscr",
        html5: Some(b"\xf0\x9d\x92\xb9"),
        html4: None,
    },
    Charref {
        name: b"dscy",
        html5: Some(b"\xd1\x95"),
        html4: None,
    },
    Charref {
        name: b"dsol",
        html5: Some(b"\xe2\xa7\xb6"),
        html4: None,
    },
    Charref {
        name: b"dstrok",
        html5: Some(b"\xc4\x91"),
        html4: None,
    },
    Charref {
        name: b"dtdot",
        html5: Some(b"\xe2\x8b\xb1"),
        html4: None,
    },
    Charref {
        name: b"dtri",
        html5: Some(b"\xe2\x96\xbf"),
        html4: None,
    },
    Charref {
        name: b"dtrif",
        html5: Some(b"\xe2\x96\xbe"),
        html4: None,
    },
    Charref {
        name: b"duarr",
        html5: Some(b"\xe2\x87\xb5"),
        html4: None,
    },
    Charref {
        name: b"duhar",
        html5: Some(b"\xe2\xa5\xaf"),
        html4: None,
    },
    Charref {
        name: b"dwangle",
        html5: Some(b"\xe2\xa6\xa6"),
        html4: None,
    },
    Charref {
        name: b"dzcy",
        html5: Some(b"\xd1\x9f"),
        html4: None,
    },
    Charref {
        name: b"dzigrarr",
        html5: Some(b"\xe2\x9f\xbf"),
        html4: None,
    },
    Charref {
        name: b"eDDot",
        html5: Some(b"\xe2\xa9\xb7"),
        html4: None,
    },
    Charref {
        name: b"eDot",
        html5: Some(b"\xe2\x89\x91"),
        html4: None,
    },
    Charref {
        name: b"eacute",
        html5: Some(b"\xc3\xa9"),
        html4: Some(b"\xc3\xa9"),
    },
    Charref {
        name: b"easter",
        html5: Some(b"\xe2\xa9\xae"),
        html4: None,
    },
    Charref {
        name: b"ecaron",
        html5: Some(b"\xc4\x9b"),
        html4: None,
    },
    Charref {
        name: b"ecir",
        html5: Some(b"\xe2\x89\x96"),
        html4: None,
    },
    Charref {
        name: b"ecirc",
        html5: Some(b"\xc3\xaa"),
        html4: Some(b"\xc3\xaa"),
    },
    Charref {
        name: b"ecolon",
        html5: Some(b"\xe2\x89\x95"),
        html4: None,
    },
    Charref {
        name: b"ecy",
        html5: Some(b"\xd1\x8d"),
        html4: None,
    },
    Charref {
        name: b"edot",
        html5: Some(b"\xc4\x97"),
        html4: None,
    },
    Charref {
        name: b"ee",
        html5: Some(b"\xe2\x85\x87"),
        html4: None,
    },
    Charref {
        name: b"efDot",
        html5: Some(b"\xe2\x89\x92"),
        html4: None,
    },
    Charref {
        name: b"efr",
        html5: Some(b"\xf0\x9d\x94\xa2"),
        html4: None,
    },
    Charref {
        name: b"eg",
        html5: Some(b"\xe2\xaa\x9a"),
        html4: None,
    },
    Charref {
        name: b"egrave",
        html5: Some(b"\xc3\xa8"),
        html4: Some(b"\xc3\xa8"),
    },
    Charref {
        name: b"egs",
        html5: Some(b"\xe2\xaa\x96"),
        html4: None,
    },
    Charref {
        name: b"egsdot",
        html5: Some(b"\xe2\xaa\x98"),
        html4: None,
    },
    Charref {
        name: b"el",
        html5: Some(b"\xe2\xaa\x99"),
        html4: None,
    },
    Charref {
        name: b"elinters",
        html5: Some(b"\xe2\x8f\xa7"),
        html4: None,
    },
    Charref {
        name: b"ell",
        html5: Some(b"\xe2\x84\x93"),
        html4: None,
    },
    Charref {
        name: b"els",
        html5: Some(b"\xe2\xaa\x95"),
        html4: None,
    },
    Charref {
        name: b"elsdot",
        html5: Some(b"\xe2\xaa\x97"),
        html4: None,
    },
    Charref {
        name: b"emacr",
        html5: Some(b"\xc4\x93"),
        html4: None,
    },
    Charref {
        name: b"empty",
        html5: Some(b"\xe2\x88\x85"),
        html4: Some(b"\xe2\x88\x85"),
    },
    Charref {
        name: b"emptyset",
        html5: Some(b"\xe2\x88\x85"),
        html4: None,
    },
    Charref {
        name: b"emptyv",
        html5: Some(b"\xe2\x88\x85"),
        html4: None,
    },
    Charref {
        name: b"emsp",
        html5: Some(b"\xe2\x80\x83"),
        html4: Some(b"\xe2\x80\x83"),
    },
    Charref {
        name: b"emsp13",
        html5: Some(b"\xe2\x80\x84"),
        html4: None,
    },
    Charref {
        name: b"emsp14",
        html5: Some(b"\xe2\x80\x85"),
        html4: None,
    },
    Charref {
        name: b"eng",
        html5: Some(b"\xc5\x8b"),
        html4: None,
    },
    Charref {
        name: b"ensp",
        html5: Some(b"\xe2\x80\x82"),
        html4: Some(b"\xe2\x80\x82"),
    },
    Charref {
        name: b"eogon",
        html5: Some(b"\xc4\x99"),
        html4: None,
    },
    Charref {
        name: b"eopf",
        html5: Some(b"\xf0\x9d\x95\x96"),
        html4: None,
    },
    Charref {
        name: b"epar",
        html5: Some(b"\xe2\x8b\x95"),
        html4: None,
    },
    Charref {
        name: b"eparsl",
        html5: Some(b"\xe2\xa7\xa3"),
        html4: None,
    },
    Charref {
        name: b"eplus",
        html5: Some(b"\xe2\xa9\xb1"),
        html4: None,
    },
    Charref {
        name: b"epsi",
        html5: Some(b"\xce\xb5"),
        html4: None,
    },
    Charref {
        name: b"epsilon",
        html5: Some(b"\xce\xb5"),
        html4: Some(b"\xce\xb5"),
    },
    Charref {
        name: b"epsiv",
        html5: Some(b"\xcf\xb5"),
        html4: None,
    },
    Charref {
        name: b"eqcirc",
        html5: Some(b"\xe2\x89\x96"),
        html4: None,
    },
    Charref {
        name: b"eqcolon",
        html5: Some(b"\xe2\x89\x95"),
        html4: None,
    },
    Charref {
        name: b"eqsim",
        html5: Some(b"\xe2\x89\x82"),
        html4: None,
    },
    Charref {
        name: b"eqslantgtr",
        html5: Some(b"\xe2\xaa\x96"),
        html4: None,
    },
    Charref {
        name: b"eqslantless",
        html5: Some(b"\xe2\xaa\x95"),
        html4: None,
    },
    Charref {
        name: b"equals",
        html5: Some(b"="),
        html4: None,
    },
    Charref {
        name: b"equest",
        html5: Some(b"\xe2\x89\x9f"),
        html4: None,
    },
    Charref {
        name: b"equiv",
        html5: Some(b"\xe2\x89\xa1"),
        html4: Some(b"\xe2\x89\xa1"),
    },
    Charref {
        name: b"equivDD",
        html5: Some(b"\xe2\xa9\xb8"),
        html4: None,
    },
    Charref {
        name: b"eqvparsl",
        html5: Some(b"\xe2\xa7\xa5"),
        html4: None,
    },
    Charref {
        name: b"erDot",
        html5: Some(b"\xe2\x89\x93"),
        html4: None,
    },
    Charref {
        name: b"erarr",
        html5: Some(b"\xe2\xa5\xb1"),
        html4: None,
    },
    Charref {
        name: b"escr",
        html5: Some(b"\xe2\x84\xaf"),
        html4: None,
    },
    Charref {
        name: b"esdot",
        html5: Some(b"\xe2\x89\x90"),
        html4: None,
    },
    Charref {
        name: b"esim",
        html5: Some(b"\xe2\x89\x82"),
        html4: None,
    },
    Charref {
        name: b"eta",
        html5: Some(b"\xce\xb7"),
        html4: Some(b"\xce\xb7"),
    },
    Charref {
        name: b"eth",
        html5: Some(b"\xc3\xb0"),
        html4: Some(b"\xc3\xb0"),
    },
    Charref {
        name: b"euml",
        html5: Some(b"\xc3\xab"),
        html4: Some(b"\xc3\xab"),
    },
    Charref {
        name: b"euro",
        html5: Some(b"\xe2\x82\xac"),
        html4: Some(b"\xe2\x82\xac"),
    },
    Charref {
        name: b"excl",
        html5: Some(b"!"),
        html4: None,
    },
    Charref {
        name: b"exist",
        html5: Some(b"\xe2\x88\x83"),
        html4: Some(b"\xe2\x88\x83"),
    },
    Charref {
        name: b"expectation",
        html5: Some(b"\xe2\x84\xb0"),
        html4: None,
    },
    Charref {
        name: b"exponentiale",
        html5: Some(b"\xe2\x85\x87"),
        html4: None,
    },
    Charref {
        name: b"fallingdotseq",
        html5: Some(b"\xe2\x89\x92"),
        html4: None,
    },
    Charref {
        name: b"fcy",
        html5: Some(b"\xd1\x84"),
        html4: None,
    },
    Charref {
        name: b"female",
        html5: Some(b"\xe2\x99\x80"),
        html4: None,
    },
    Charref {
        name: b"ffilig",
        html5: Some(b"\xef\xac\x83"),
        html4: None,
    },
    Charref {
        name: b"fflig",
        html5: Some(b"\xef\xac\x80"),
        html4: None,
    },
    Charref {
        name: b"ffllig",
        html5: Some(b"\xef\xac\x84"),
        html4: None,
    },
    Charref {
        name: b"ffr",
        html5: Some(b"\xf0\x9d\x94\xa3"),
        html4: None,
    },
    Charref {
        name: b"filig",
        html5: Some(b"\xef\xac\x81"),
        html4: None,
    },
    Charref {
        name: b"fjlig",
        html5: Some(b"fj"),
        html4: None,
    },
    Charref {
        name: b"flat",
        html5: Some(b"\xe2\x99\xad"),
        html4: None,
    },
    Charref {
        name: b"fllig",
        html5: Some(b"\xef\xac\x82"),
        html4: None,
    },
    Charref {
        name: b"fltns",
        html5: Some(b"\xe2\x96\xb1"),
        html4: None,
    },
    Charref {
        name: b"fnof",
        html5: Some(b"\xc6\x92"),
        html4: Some(b"\xc6\x92"),
    },
    Charref {
        name: b"fopf",
        html5: Some(b"\xf0\x9d\x95\x97"),
        html4: None,
    },
    Charref {
        name: b"forall",
        html5: Some(b"\xe2\x88\x80"),
        html4: Some(b"\xe2\x88\x80"),
    },
    Charref {
        name: b"fork",
        html5: Some(b"\xe2\x8b\x94"),
        html4: None,
    },
    Charref {
        name: b"forkv",
        html5: Some(b"\xe2\xab\x99"),
        html4: None,
    },
    Charref {
        name: b"fpartint",
        html5: Some(b"\xe2\xa8\x8d"),
        html4: None,
    },
    Charref {
        name: b"frac12",
        html5: Some(b"\xc2\xbd"),
        html4: Some(b"\xc2\xbd"),
    },
    Charref {
        name: b"frac13",
        html5: Some(b"\xe2\x85\x93"),
        html4: None,
    },
    Charref {
        name: b"frac14",
        html5: Some(b"\xc2\xbc"),
        html4: Some(b"\xc2\xbc"),
    },
    Charref {
        name: b"frac15",
        html5: Some(b"\xe2\x85\x95"),
        html4: None,
    },
    Charref {
        name: b"frac16",
        html5: Some(b"\xe2\x85\x99"),
        html4: None,
    },
    Charref {
        name: b"frac18",
        html5: Some(b"\xe2\x85\x9b"),
        html4: None,
    },
    Charref {
        name: b"frac23",
        html5: Some(b"\xe2\x85\x94"),
        html4: None,
    },
    Charref {
        name: b"frac25",
        html5: Some(b"\xe2\x85\x96"),
        html4: None,
    },
    Charref {
        name: b"frac34",
        html5: Some(b"\xc2\xbe"),
        html4: Some(b"\xc2\xbe"),
    },
    Charref {
        name: b"frac35",
        html5: Some(b"\xe2\x85\x97"),
        html4: None,
    },
    Charref {
        name: b"frac38",
        html5: Some(b"\xe2\x85\x9c"),
        html4: None,
    },
    Charref {
        name: b"frac45",
        html5: Some(b"\xe2\x85\x98"),
        html4: None,
    },
    Charref {
        name: b"frac56",
        html5: Some(b"\xe2\x85\x9a"),
        html4: None,
    },
    Charref {
        name: b"frac58",
        html5: Some(b"\xe2\x85\x9d"),
        html4: None,
    },
    Charref {
        name: b"frac78",
        html5: Some(b"\xe2\x85\x9e"),
        html4: None,
    },
    Charref {
        name: b"frasl",
        html5: Some(b"\xe2\x81\x84"),
        html4: Some(b"\xe2\x81\x84"),
    },
    Charref {
        name: b"frown",
        html5: Some(b"\xe2\x8c\xa2"),
        html4: None,
    },
    Charref {
        name: b"fscr",
        html5: Some(b"\xf0\x9d\x92\xbb"),
        html4: None,
    },
    Charref {
        name: b"gE",
        html5: Some(b"\xe2\x89\xa7"),
        html4: None,
    },
    Charref {
        name: b"gEl",
        html5: Some(b"\xe2\xaa\x8c"),
        html4: None,
    },
    Charref {
        name: b"gacute",
        html5: Some(b"\xc7\xb5"),
        html4: None,
    },
    Charref {
        name: b"gamma",
        html5: Some(b"\xce\xb3"),
        html4: Some(b"\xce\xb3"),
    },
    Charref {
        name: b"gammad",
        html5: Some(b"\xcf\x9d"),
        html4: None,
    },
    Charref {
        name: b"gap",
        html5: Some(b"\xe2\xaa\x86"),
        html4: None,
    },
    Charref {
        name: b"gbreve",
        html5: Some(b"\xc4\x9f"),
        html4: None,
    },
    Charref {
        name: b"gcirc",
        html5: Some(b"\xc4\x9d"),
        html4: None,
    },
    Charref {
        name: b"gcy",
        html5: Some(b"\xd0\xb3"),
        html4: None,
    },
    Charref {
        name: b"gdot",
        html5: Some(b"\xc4\xa1"),
        html4: None,
    },
    Charref {
        name: b"ge",
        html5: Some(b"\xe2\x89\xa5"),
        html4: Some(b"\xe2\x89\xa5"),
    },
    Charref {
        name: b"gel",
        html5: Some(b"\xe2\x8b\x9b"),
        html4: None,
    },
    Charref {
        name: b"geq",
        html5: Some(b"\xe2\x89\xa5"),
        html4: None,
    },
    Charref {
        name: b"geqq",
        html5: Some(b"\xe2\x89\xa7"),
        html4: None,
    },
    Charref {
        name: b"geqslant",
        html5: Some(b"\xe2\xa9\xbe"),
        html4: None,
    },
    Charref {
        name: b"ges",
        html5: Some(b"\xe2\xa9\xbe"),
        html4: None,
    },
    Charref {
        name: b"gescc",
        html5: Some(b"\xe2\xaa\xa9"),
        html4: None,
    },
    Charref {
        name: b"gesdot",
        html5: Some(b"\xe2\xaa\x80"),
        html4: None,
    },
    Charref {
        name: b"gesdoto",
        html5: Some(b"\xe2\xaa\x82"),
        html4: None,
    },
    Charref {
        name: b"gesdotol",
        html5: Some(b"\xe2\xaa\x84"),
        html4: None,
    },
    Charref {
        name: b"gesl",
        html5: Some(b"\xe2\x8b\x9b\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"gesles",
        html5: Some(b"\xe2\xaa\x94"),
        html4: None,
    },
    Charref {
        name: b"gfr",
        html5: Some(b"\xf0\x9d\x94\xa4"),
        html4: None,
    },
    Charref {
        name: b"gg",
        html5: Some(b"\xe2\x89\xab"),
        html4: None,
    },
    Charref {
        name: b"ggg",
        html5: Some(b"\xe2\x8b\x99"),
        html4: None,
    },
    Charref {
        name: b"gimel",
        html5: Some(b"\xe2\x84\xb7"),
        html4: None,
    },
    Charref {
        name: b"gjcy",
        html5: Some(b"\xd1\x93"),
        html4: None,
    },
    Charref {
        name: b"gl",
        html5: Some(b"\xe2\x89\xb7"),
        html4: None,
    },
    Charref {
        name: b"glE",
        html5: Some(b"\xe2\xaa\x92"),
        html4: None,
    },
    Charref {
        name: b"gla",
        html5: Some(b"\xe2\xaa\xa5"),
        html4: None,
    },
    Charref {
        name: b"glj",
        html5: Some(b"\xe2\xaa\xa4"),
        html4: None,
    },
    Charref {
        name: b"gnE",
        html5: Some(b"\xe2\x89\xa9"),
        html4: None,
    },
    Charref {
        name: b"gnap",
        html5: Some(b"\xe2\xaa\x8a"),
        html4: None,
    },
    Charref {
        name: b"gnapprox",
        html5: Some(b"\xe2\xaa\x8a"),
        html4: None,
    },
    Charref {
        name: b"gne",
        html5: Some(b"\xe2\xaa\x88"),
        html4: None,
    },
    Charref {
        name: b"gneq",
        html5: Some(b"\xe2\xaa\x88"),
        html4: None,
    },
    Charref {
        name: b"gneqq",
        html5: Some(b"\xe2\x89\xa9"),
        html4: None,
    },
    Charref {
        name: b"gnsim",
        html5: Some(b"\xe2\x8b\xa7"),
        html4: None,
    },
    Charref {
        name: b"gopf",
        html5: Some(b"\xf0\x9d\x95\x98"),
        html4: None,
    },
    Charref {
        name: b"grave",
        html5: Some(b"`"),
        html4: None,
    },
    Charref {
        name: b"gscr",
        html5: Some(b"\xe2\x84\x8a"),
        html4: None,
    },
    Charref {
        name: b"gsim",
        html5: Some(b"\xe2\x89\xb3"),
        html4: None,
    },
    Charref {
        name: b"gsime",
        html5: Some(b"\xe2\xaa\x8e"),
        html4: None,
    },
    Charref {
        name: b"gsiml",
        html5: Some(b"\xe2\xaa\x90"),
        html4: None,
    },
    Charref {
        name: b"gt",
        html5: Some(b">"),
        html4: Some(b">"),
    },
    Charref {
        name: b"gtcc",
        html5: Some(b"\xe2\xaa\xa7"),
        html4: None,
    },
    Charref {
        name: b"gtcir",
        html5: Some(b"\xe2\xa9\xba"),
        html4: None,
    },
    Charref {
        name: b"gtdot",
        html5: Some(b"\xe2\x8b\x97"),
        html4: None,
    },
    Charref {
        name: b"gtlPar",
        html5: Some(b"\xe2\xa6\x95"),
        html4: None,
    },
    Charref {
        name: b"gtquest",
        html5: Some(b"\xe2\xa9\xbc"),
        html4: None,
    },
    Charref {
        name: b"gtrapprox",
        html5: Some(b"\xe2\xaa\x86"),
        html4: None,
    },
    Charref {
        name: b"gtrarr",
        html5: Some(b"\xe2\xa5\xb8"),
        html4: None,
    },
    Charref {
        name: b"gtrdot",
        html5: Some(b"\xe2\x8b\x97"),
        html4: None,
    },
    Charref {
        name: b"gtreqless",
        html5: Some(b"\xe2\x8b\x9b"),
        html4: None,
    },
    Charref {
        name: b"gtreqqless",
        html5: Some(b"\xe2\xaa\x8c"),
        html4: None,
    },
    Charref {
        name: b"gtrless",
        html5: Some(b"\xe2\x89\xb7"),
        html4: None,
    },
    Charref {
        name: b"gtrsim",
        html5: Some(b"\xe2\x89\xb3"),
        html4: None,
    },
    Charref {
        name: b"gvertneqq",
        html5: Some(b"\xe2\x89\xa9\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"gvnE",
        html5: Some(b"\xe2\x89\xa9\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"hArr",
        html5: Some(b"\xe2\x87\x94"),
        html4: Some(b"\xe2\x87\x94"),
    },
    Charref {
        name: b"hairsp",
        html5: Some(b"\xe2\x80\x8a"),
        html4: None,
    },
    Charref {
        name: b"half",
        html5: Some(b"\xc2\xbd"),
        html4: None,
    },
    Charref {
        name: b"hamilt",
        html5: Some(b"\xe2\x84\x8b"),
        html4: None,
    },
    Charref {
        name: b"hardcy",
        html5: Some(b"\xd1\x8a"),
        html4: None,
    },
    Charref {
        name: b"harr",
        html5: Some(b"\xe2\x86\x94"),
        html4: Some(b"\xe2\x86\x94"),
    },
    Charref {
        name: b"harrcir",
        html5: Some(b"\xe2\xa5\x88"),
        html4: None,
    },
    Charref {
        name: b"harrw",
        html5: Some(b"\xe2\x86\xad"),
        html4: None,
    },
    Charref {
        name: b"hbar",
        html5: Some(b"\xe2\x84\x8f"),
        html4: None,
    },
    Charref {
        name: b"hcirc",
        html5: Some(b"\xc4\xa5"),
        html4: None,
    },
    Charref {
        name: b"hearts",
        html5: Some(b"\xe2\x99\xa5"),
        html4: Some(b"\xe2\x99\xa5"),
    },
    Charref {
        name: b"heartsuit",
        html5: Some(b"\xe2\x99\xa5"),
        html4: None,
    },
    Charref {
        name: b"hellip",
        html5: Some(b"\xe2\x80\xa6"),
        html4: Some(b"\xe2\x80\xa6"),
    },
    Charref {
        name: b"hercon",
        html5: Some(b"\xe2\x8a\xb9"),
        html4: None,
    },
    Charref {
        name: b"hfr",
        html5: Some(b"\xf0\x9d\x94\xa5"),
        html4: None,
    },
    Charref {
        name: b"hksearow",
        html5: Some(b"\xe2\xa4\xa5"),
        html4: None,
    },
    Charref {
        name: b"hkswarow",
        html5: Some(b"\xe2\xa4\xa6"),
        html4: None,
    },
    Charref {
        name: b"hoarr",
        html5: Some(b"\xe2\x87\xbf"),
        html4: None,
    },
    Charref {
        name: b"homtht",
        html5: Some(b"\xe2\x88\xbb"),
        html4: None,
    },
    Charref {
        name: b"hookleftarrow",
        html5: Some(b"\xe2\x86\xa9"),
        html4: None,
    },
    Charref {
        name: b"hookrightarrow",
        html5: Some(b"\xe2\x86\xaa"),
        html4: None,
    },
    Charref {
        name: b"hopf",
        html5: Some(b"\xf0\x9d\x95\x99"),
        html4: None,
    },
    Charref {
        name: b"horbar",
        html5: Some(b"\xe2\x80\x95"),
        html4: None,
    },
    Charref {
        name: b"hscr",
        html5: Some(b"\xf0\x9d\x92\xbd"),
        html4: None,
    },
    Charref {
        name: b"hslash",
        html5: Some(b"\xe2\x84\x8f"),
        html4: None,
    },
    Charref {
        name: b"hstrok",
        html5: Some(b"\xc4\xa7"),
        html4: None,
    },
    Charref {
        name: b"hybull",
        html5: Some(b"\xe2\x81\x83"),
        html4: None,
    },
    Charref {
        name: b"hyphen",
        html5: Some(b"\xe2\x80\x90"),
        html4: None,
    },
    Charref {
        name: b"iacute",
        html5: Some(b"\xc3\xad"),
        html4: Some(b"\xc3\xad"),
    },
    Charref {
        name: b"ic",
        html5: Some(b"\xe2\x81\xa3"),
        html4: None,
    },
    Charref {
        name: b"icirc",
        html5: Some(b"\xc3\xae"),
        html4: Some(b"\xc3\xae"),
    },
    Charref {
        name: b"icy",
        html5: Some(b"\xd0\xb8"),
        html4: None,
    },
    Charref {
        name: b"iecy",
        html5: Some(b"\xd0\xb5"),
        html4: None,
    },
    Charref {
        name: b"iexcl",
        html5: Some(b"\xc2\xa1"),
        html4: Some(b"\xc2\xa1"),
    },
    Charref {
        name: b"iff",
        html5: Some(b"\xe2\x87\x94"),
        html4: None,
    },
    Charref {
        name: b"ifr",
        html5: Some(b"\xf0\x9d\x94\xa6"),
        html4: None,
    },
    Charref {
        name: b"igrave",
        html5: Some(b"\xc3\xac"),
        html4: Some(b"\xc3\xac"),
    },
    Charref {
        name: b"ii",
        html5: Some(b"\xe2\x85\x88"),
        html4: None,
    },
    Charref {
        name: b"iiiint",
        html5: Some(b"\xe2\xa8\x8c"),
        html4: None,
    },
    Charref {
        name: b"iiint",
        html5: Some(b"\xe2\x88\xad"),
        html4: None,
    },
    Charref {
        name: b"iinfin",
        html5: Some(b"\xe2\xa7\x9c"),
        html4: None,
    },
    Charref {
        name: b"iiota",
        html5: Some(b"\xe2\x84\xa9"),
        html4: None,
    },
    Charref {
        name: b"ijlig",
        html5: Some(b"\xc4\xb3"),
        html4: None,
    },
    Charref {
        name: b"imacr",
        html5: Some(b"\xc4\xab"),
        html4: None,
    },
    Charref {
        name: b"image",
        html5: Some(b"\xe2\x84\x91"),
        html4: Some(b"\xe2\x84\x91"),
    },
    Charref {
        name: b"imagline",
        html5: Some(b"\xe2\x84\x90"),
        html4: None,
    },
    Charref {
        name: b"imagpart",
        html5: Some(b"\xe2\x84\x91"),
        html4: None,
    },
    Charref {
        name: b"imath",
        html5: Some(b"\xc4\xb1"),
        html4: None,
    },
    Charref {
        name: b"imof",
        html5: Some(b"\xe2\x8a\xb7"),
        html4: None,
    },
    Charref {
        name: b"imped",
        html5: Some(b"\xc6\xb5"),
        html4: None,
    },
    Charref {
        name: b"in",
        html5: Some(b"\xe2\x88\x88"),
        html4: None,
    },
    Charref {
        name: b"incare",
        html5: Some(b"\xe2\x84\x85"),
        html4: None,
    },
    Charref {
        name: b"infin",
        html5: Some(b"\xe2\x88\x9e"),
        html4: Some(b"\xe2\x88\x9e"),
    },
    Charref {
        name: b"infintie",
        html5: Some(b"\xe2\xa7\x9d"),
        html4: None,
    },
    Charref {
        name: b"inodot",
        html5: Some(b"\xc4\xb1"),
        html4: None,
    },
    Charref {
        name: b"int",
        html5: Some(b"\xe2\x88\xab"),
        html4: Some(b"\xe2\x88\xab"),
    },
    Charref {
        name: b"intcal",
        html5: Some(b"\xe2\x8a\xba"),
        html4: None,
    },
    Charref {
        name: b"integers",
        html5: Some(b"\xe2\x84\xa4"),
        html4: None,
    },
    Charref {
        name: b"intercal",
        html5: Some(b"\xe2\x8a\xba"),
        html4: None,
    },
    Charref {
        name: b"intlarhk",
        html5: Some(b"\xe2\xa8\x97"),
        html4: None,
    },
    Charref {
        name: b"intprod",
        html5: Some(b"\xe2\xa8\xbc"),
        html4: None,
    },
    Charref {
        name: b"iocy",
        html5: Some(b"\xd1\x91"),
        html4: None,
    },
    Charref {
        name: b"iogon",
        html5: Some(b"\xc4\xaf"),
        html4: None,
    },
    Charref {
        name: b"iopf",
        html5: Some(b"\xf0\x9d\x95\x9a"),
        html4: None,
    },
    Charref {
        name: b"iota",
        html5: Some(b"\xce\xb9"),
        html4: Some(b"\xce\xb9"),
    },
    Charref {
        name: b"iprod",
        html5: Some(b"\xe2\xa8\xbc"),
        html4: None,
    },
    Charref {
        name: b"iquest",
        html5: Some(b"\xc2\xbf"),
        html4: Some(b"\xc2\xbf"),
    },
    Charref {
        name: b"iscr",
        html5: Some(b"\xf0\x9d\x92\xbe"),
        html4: None,
    },
    Charref {
        name: b"isin",
        html5: Some(b"\xe2\x88\x88"),
        html4: Some(b"\xe2\x88\x88"),
    },
    Charref {
        name: b"isinE",
        html5: Some(b"\xe2\x8b\xb9"),
        html4: None,
    },
    Charref {
        name: b"isindot",
        html5: Some(b"\xe2\x8b\xb5"),
        html4: None,
    },
    Charref {
        name: b"isins",
        html5: Some(b"\xe2\x8b\xb4"),
        html4: None,
    },
    Charref {
        name: b"isinsv",
        html5: Some(b"\xe2\x8b\xb3"),
        html4: None,
    },
    Charref {
        name: b"isinv",
        html5: Some(b"\xe2\x88\x88"),
        html4: None,
    },
    Charref {
        name: b"it",
        html5: Some(b"\xe2\x81\xa2"),
        html4: None,
    },
    Charref {
        name: b"itilde",
        html5: Some(b"\xc4\xa9"),
        html4: None,
    },
    Charref {
        name: b"iukcy",
        html5: Some(b"\xd1\x96"),
        html4: None,
    },
    Charref {
        name: b"iuml",
        html5: Some(b"\xc3\xaf"),
        html4: Some(b"\xc3\xaf"),
    },
    Charref {
        name: b"jcirc",
        html5: Some(b"\xc4\xb5"),
        html4: None,
    },
    Charref {
        name: b"jcy",
        html5: Some(b"\xd0\xb9"),
        html4: None,
    },
    Charref {
        name: b"jfr",
        html5: Some(b"\xf0\x9d\x94\xa7"),
        html4: None,
    },
    Charref {
        name: b"jmath",
        html5: Some(b"\xc8\xb7"),
        html4: None,
    },
    Charref {
        name: b"jopf",
        html5: Some(b"\xf0\x9d\x95\x9b"),
        html4: None,
    },
    Charref {
        name: b"jscr",
        html5: Some(b"\xf0\x9d\x92\xbf"),
        html4: None,
    },
    Charref {
        name: b"jsercy",
        html5: Some(b"\xd1\x98"),
        html4: None,
    },
    Charref {
        name: b"jukcy",
        html5: Some(b"\xd1\x94"),
        html4: None,
    },
    Charref {
        name: b"kappa",
        html5: Some(b"\xce\xba"),
        html4: Some(b"\xce\xba"),
    },
    Charref {
        name: b"kappav",
        html5: Some(b"\xcf\xb0"),
        html4: None,
    },
    Charref {
        name: b"kcedil",
        html5: Some(b"\xc4\xb7"),
        html4: None,
    },
    Charref {
        name: b"kcy",
        html5: Some(b"\xd0\xba"),
        html4: None,
    },
    Charref {
        name: b"kfr",
        html5: Some(b"\xf0\x9d\x94\xa8"),
        html4: None,
    },
    Charref {
        name: b"kgreen",
        html5: Some(b"\xc4\xb8"),
        html4: None,
    },
    Charref {
        name: b"khcy",
        html5: Some(b"\xd1\x85"),
        html4: None,
    },
    Charref {
        name: b"kjcy",
        html5: Some(b"\xd1\x9c"),
        html4: None,
    },
    Charref {
        name: b"kopf",
        html5: Some(b"\xf0\x9d\x95\x9c"),
        html4: None,
    },
    Charref {
        name: b"kscr",
        html5: Some(b"\xf0\x9d\x93\x80"),
        html4: None,
    },
    Charref {
        name: b"lAarr",
        html5: Some(b"\xe2\x87\x9a"),
        html4: None,
    },
    Charref {
        name: b"lArr",
        html5: Some(b"\xe2\x87\x90"),
        html4: Some(b"\xe2\x87\x90"),
    },
    Charref {
        name: b"lAtail",
        html5: Some(b"\xe2\xa4\x9b"),
        html4: None,
    },
    Charref {
        name: b"lBarr",
        html5: Some(b"\xe2\xa4\x8e"),
        html4: None,
    },
    Charref {
        name: b"lE",
        html5: Some(b"\xe2\x89\xa6"),
        html4: None,
    },
    Charref {
        name: b"lEg",
        html5: Some(b"\xe2\xaa\x8b"),
        html4: None,
    },
    Charref {
        name: b"lHar",
        html5: Some(b"\xe2\xa5\xa2"),
        html4: None,
    },
    Charref {
        name: b"lacute",
        html5: Some(b"\xc4\xba"),
        html4: None,
    },
    Charref {
        name: b"laemptyv",
        html5: Some(b"\xe2\xa6\xb4"),
        html4: None,
    },
    Charref {
        name: b"lagran",
        html5: Some(b"\xe2\x84\x92"),
        html4: None,
    },
    Charref {
        name: b"lambda",
        html5: Some(b"\xce\xbb"),
        html4: Some(b"\xce\xbb"),
    },
    Charref {
        name: b"lang",
        html5: Some(b"\xe2\x9f\xa8"),
        html4: Some(b"\xe2\x8c\xa9"),
    },
    Charref {
        name: b"langd",
        html5: Some(b"\xe2\xa6\x91"),
        html4: None,
    },
    Charref {
        name: b"langle",
        html5: Some(b"\xe2\x9f\xa8"),
        html4: None,
    },
    Charref {
        name: b"lap",
        html5: Some(b"\xe2\xaa\x85"),
        html4: None,
    },
    Charref {
        name: b"laquo",
        html5: Some(b"\xc2\xab"),
        html4: Some(b"\xc2\xab"),
    },
    Charref {
        name: b"larr",
        html5: Some(b"\xe2\x86\x90"),
        html4: Some(b"\xe2\x86\x90"),
    },
    Charref {
        name: b"larrb",
        html5: Some(b"\xe2\x87\xa4"),
        html4: None,
    },
    Charref {
        name: b"larrbfs",
        html5: Some(b"\xe2\xa4\x9f"),
        html4: None,
    },
    Charref {
        name: b"larrfs",
        html5: Some(b"\xe2\xa4\x9d"),
        html4: None,
    },
    Charref {
        name: b"larrhk",
        html5: Some(b"\xe2\x86\xa9"),
        html4: None,
    },
    Charref {
        name: b"larrlp",
        html5: Some(b"\xe2\x86\xab"),
        html4: None,
    },
    Charref {
        name: b"larrpl",
        html5: Some(b"\xe2\xa4\xb9"),
        html4: None,
    },
    Charref {
        name: b"larrsim",
        html5: Some(b"\xe2\xa5\xb3"),
        html4: None,
    },
    Charref {
        name: b"larrtl",
        html5: Some(b"\xe2\x86\xa2"),
        html4: None,
    },
    Charref {
        name: b"lat",
        html5: Some(b"\xe2\xaa\xab"),
        html4: None,
    },
    Charref {
        name: b"latail",
        html5: Some(b"\xe2\xa4\x99"),
        html4: None,
    },
    Charref {
        name: b"late",
        html5: Some(b"\xe2\xaa\xad"),
        html4: None,
    },
    Charref {
        name: b"lates",
        html5: Some(b"\xe2\xaa\xad\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"lbarr",
        html5: Some(b"\xe2\xa4\x8c"),
        html4: None,
    },
    Charref {
        name: b"lbbrk",
        html5: Some(b"\xe2\x9d\xb2"),
        html4: None,
    },
    Charref {
        name: b"lbrace",
        html5: Some(b"{"),
        html4: None,
    },
    Charref {
        name: b"lbrack",
        html5: Some(b"["),
        html4: None,
    },
    Charref {
        name: b"lbrke",
        html5: Some(b"\xe2\xa6\x8b"),
        html4: None,
    },
    Charref {
        name: b"lbrksld",
        html5: Some(b"\xe2\xa6\x8f"),
        html4: None,
    },
    Charref {
        name: b"lbrkslu",
        html5: Some(b"\xe2\xa6\x8d"),
        html4: None,
    },
    Charref {
        name: b"lcaron",
        html5: Some(b"\xc4\xbe"),
        html4: None,
    },
    Charref {
        name: b"lcedil",
        html5: Some(b"\xc4\xbc"),
        html4: None,
    },
    Charref {
        name: b"lceil",
        html5: Some(b"\xe2\x8c\x88"),
        html4: Some(b"\xe2\x8c\x88"),
    },
    Charref {
        name: b"lcub",
        html5: Some(b"{"),
        html4: None,
    },
    Charref {
        name: b"lcy",
        html5: Some(b"\xd0\xbb"),
        html4: None,
    },
    Charref {
        name: b"ldca",
        html5: Some(b"\xe2\xa4\xb6"),
        html4: None,
    },
    Charref {
        name: b"ldquo",
        html5: Some(b"\xe2\x80\x9c"),
        html4: Some(b"\xe2\x80\x9c"),
    },
    Charref {
        name: b"ldquor",
        html5: Some(b"\xe2\x80\x9e"),
        html4: None,
    },
    Charref {
        name: b"ldrdhar",
        html5: Some(b"\xe2\xa5\xa7"),
        html4: None,
    },
    Charref {
        name: b"ldrushar",
        html5: Some(b"\xe2\xa5\x8b"),
        html4: None,
    },
    Charref {
        name: b"ldsh",
        html5: Some(b"\xe2\x86\xb2"),
        html4: None,
    },
    Charref {
        name: b"le",
        html5: Some(b"\xe2\x89\xa4"),
        html4: Some(b"\xe2\x89\xa4"),
    },
    Charref {
        name: b"leftarrow",
        html5: Some(b"\xe2\x86\x90"),
        html4: None,
    },
    Charref {
        name: b"leftarrowtail",
        html5: Some(b"\xe2\x86\xa2"),
        html4: None,
    },
    Charref {
        name: b"leftharpoondown",
        html5: Some(b"\xe2\x86\xbd"),
        html4: None,
    },
    Charref {
        name: b"leftharpoonup",
        html5: Some(b"\xe2\x86\xbc"),
        html4: None,
    },
    Charref {
        name: b"leftleftarrows",
        html5: Some(b"\xe2\x87\x87"),
        html4: None,
    },
    Charref {
        name: b"leftrightarrow",
        html5: Some(b"\xe2\x86\x94"),
        html4: None,
    },
    Charref {
        name: b"leftrightarrows",
        html5: Some(b"\xe2\x87\x86"),
        html4: None,
    },
    Charref {
        name: b"leftrightharpoons",
        html5: Some(b"\xe2\x87\x8b"),
        html4: None,
    },
    Charref {
        name: b"leftrightsquigarrow",
        html5: Some(b"\xe2\x86\xad"),
        html4: None,
    },
    Charref {
        name: b"leftthreetimes",
        html5: Some(b"\xe2\x8b\x8b"),
        html4: None,
    },
    Charref {
        name: b"leg",
        html5: Some(b"\xe2\x8b\x9a"),
        html4: None,
    },
    Charref {
        name: b"leq",
        html5: Some(b"\xe2\x89\xa4"),
        html4: None,
    },
    Charref {
        name: b"leqq",
        html5: Some(b"\xe2\x89\xa6"),
        html4: None,
    },
    Charref {
        name: b"leqslant",
        html5: Some(b"\xe2\xa9\xbd"),
        html4: None,
    },
    Charref {
        name: b"les",
        html5: Some(b"\xe2\xa9\xbd"),
        html4: None,
    },
    Charref {
        name: b"lescc",
        html5: Some(b"\xe2\xaa\xa8"),
        html4: None,
    },
    Charref {
        name: b"lesdot",
        html5: Some(b"\xe2\xa9\xbf"),
        html4: None,
    },
    Charref {
        name: b"lesdoto",
        html5: Some(b"\xe2\xaa\x81"),
        html4: None,
    },
    Charref {
        name: b"lesdotor",
        html5: Some(b"\xe2\xaa\x83"),
        html4: None,
    },
    Charref {
        name: b"lesg",
        html5: Some(b"\xe2\x8b\x9a\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"lesges",
        html5: Some(b"\xe2\xaa\x93"),
        html4: None,
    },
    Charref {
        name: b"lessapprox",
        html5: Some(b"\xe2\xaa\x85"),
        html4: None,
    },
    Charref {
        name: b"lessdot",
        html5: Some(b"\xe2\x8b\x96"),
        html4: None,
    },
    Charref {
        name: b"lesseqgtr",
        html5: Some(b"\xe2\x8b\x9a"),
        html4: None,
    },
    Charref {
        name: b"lesseqqgtr",
        html5: Some(b"\xe2\xaa\x8b"),
        html4: None,
    },
    Charref {
        name: b"lessgtr",
        html5: Some(b"\xe2\x89\xb6"),
        html4: None,
    },
    Charref {
        name: b"lesssim",
        html5: Some(b"\xe2\x89\xb2"),
        html4: None,
    },
    Charref {
        name: b"lfisht",
        html5: Some(b"\xe2\xa5\xbc"),
        html4: None,
    },
    Charref {
        name: b"lfloor",
        html5: Some(b"\xe2\x8c\x8a"),
        html4: Some(b"\xe2\x8c\x8a"),
    },
    Charref {
        name: b"lfr",
        html5: Some(b"\xf0\x9d\x94\xa9"),
        html4: None,
    },
    Charref {
        name: b"lg",
        html5: Some(b"\xe2\x89\xb6"),
        html4: None,
    },
    Charref {
        name: b"lgE",
        html5: Some(b"\xe2\xaa\x91"),
        html4: None,
    },
    Charref {
        name: b"lhard",
        html5: Some(b"\xe2\x86\xbd"),
        html4: None,
    },
    Charref {
        name: b"lharu",
        html5: Some(b"\xe2\x86\xbc"),
        html4: None,
    },
    Charref {
        name: b"lharul",
        html5: Some(b"\xe2\xa5\xaa"),
        html4: None,
    },
    Charref {
        name: b"lhblk",
        html5: Some(b"\xe2\x96\x84"),
        html4: None,
    },
    Charref {
        name: b"ljcy",
        html5: Some(b"\xd1\x99"),
        html4: None,
    },
    Charref {
        name: b"ll",
        html5: Some(b"\xe2\x89\xaa"),
        html4: None,
    },
    Charref {
        name: b"llarr",
        html5: Some(b"\xe2\x87\x87"),
        html4: None,
    },
    Charref {
        name: b"llcorner",
        html5: Some(b"\xe2\x8c\x9e"),
        html4: None,
    },
    Charref {
        name: b"llhard",
        html5: Some(b"\xe2\xa5\xab"),
        html4: None,
    },
    Charref {
        name: b"lltri",
        html5: Some(b"\xe2\x97\xba"),
        html4: None,
    },
    Charref {
        name: b"lmidot",
        html5: Some(b"\xc5\x80"),
        html4: None,
    },
    Charref {
        name: b"lmoust",
        html5: Some(b"\xe2\x8e\xb0"),
        html4: None,
    },
    Charref {
        name: b"lmoustache",
        html5: Some(b"\xe2\x8e\xb0"),
        html4: None,
    },
    Charref {
        name: b"lnE",
        html5: Some(b"\xe2\x89\xa8"),
        html4: None,
    },
    Charref {
        name: b"lnap",
        html5: Some(b"\xe2\xaa\x89"),
        html4: None,
    },
    Charref {
        name: b"lnapprox",
        html5: Some(b"\xe2\xaa\x89"),
        html4: None,
    },
    Charref {
        name: b"lne",
        html5: Some(b"\xe2\xaa\x87"),
        html4: None,
    },
    Charref {
        name: b"lneq",
        html5: Some(b"\xe2\xaa\x87"),
        html4: None,
    },
    Charref {
        name: b"lneqq",
        html5: Some(b"\xe2\x89\xa8"),
        html4: None,
    },
    Charref {
        name: b"lnsim",
        html5: Some(b"\xe2\x8b\xa6"),
        html4: None,
    },
    Charref {
        name: b"loang",
        html5: Some(b"\xe2\x9f\xac"),
        html4: None,
    },
    Charref {
        name: b"loarr",
        html5: Some(b"\xe2\x87\xbd"),
        html4: None,
    },
    Charref {
        name: b"lobrk",
        html5: Some(b"\xe2\x9f\xa6"),
        html4: None,
    },
    Charref {
        name: b"longleftarrow",
        html5: Some(b"\xe2\x9f\xb5"),
        html4: None,
    },
    Charref {
        name: b"longleftrightarrow",
        html5: Some(b"\xe2\x9f\xb7"),
        html4: None,
    },
    Charref {
        name: b"longmapsto",
        html5: Some(b"\xe2\x9f\xbc"),
        html4: None,
    },
    Charref {
        name: b"longrightarrow",
        html5: Some(b"\xe2\x9f\xb6"),
        html4: None,
    },
    Charref {
        name: b"looparrowleft",
        html5: Some(b"\xe2\x86\xab"),
        html4: None,
    },
    Charref {
        name: b"looparrowright",
        html5: Some(b"\xe2\x86\xac"),
        html4: None,
    },
    Charref {
        name: b"lopar",
        html5: Some(b"\xe2\xa6\x85"),
        html4: None,
    },
    Charref {
        name: b"lopf",
        html5: Some(b"\xf0\x9d\x95\x9d"),
        html4: None,
    },
    Charref {
        name: b"loplus",
        html5: Some(b"\xe2\xa8\xad"),
        html4: None,
    },
    Charref {
        name: b"lotimes",
        html5: Some(b"\xe2\xa8\xb4"),
        html4: None,
    },
    Charref {
        name: b"lowast",
        html5: Some(b"\xe2\x88\x97"),
        html4: Some(b"\xe2\x88\x97"),
    },
    Charref {
        name: b"lowbar",
        html5: Some(b"_"),
        html4: None,
    },
    Charref {
        name: b"loz",
        html5: Some(b"\xe2\x97\x8a"),
        html4: Some(b"\xe2\x97\x8a"),
    },
    Charref {
        name: b"lozenge",
        html5: Some(b"\xe2\x97\x8a"),
        html4: None,
    },
    Charref {
        name: b"lozf",
        html5: Some(b"\xe2\xa7\xab"),
        html4: None,
    },
    Charref {
        name: b"lpar",
        html5: Some(b"("),
        html4: None,
    },
    Charref {
        name: b"lparlt",
        html5: Some(b"\xe2\xa6\x93"),
        html4: None,
    },
    Charref {
        name: b"lrarr",
        html5: Some(b"\xe2\x87\x86"),
        html4: None,
    },
    Charref {
        name: b"lrcorner",
        html5: Some(b"\xe2\x8c\x9f"),
        html4: None,
    },
    Charref {
        name: b"lrhar",
        html5: Some(b"\xe2\x87\x8b"),
        html4: None,
    },
    Charref {
        name: b"lrhard",
        html5: Some(b"\xe2\xa5\xad"),
        html4: None,
    },
    Charref {
        name: b"lrm",
        html5: Some(b"\xe2\x80\x8e"),
        html4: Some(b"\xe2\x80\x8e"),
    },
    Charref {
        name: b"lrtri",
        html5: Some(b"\xe2\x8a\xbf"),
        html4: None,
    },
    Charref {
        name: b"lsaquo",
        html5: Some(b"\xe2\x80\xb9"),
        html4: Some(b"\xe2\x80\xb9"),
    },
    Charref {
        name: b"lscr",
        html5: Some(b"\xf0\x9d\x93\x81"),
        html4: None,
    },
    Charref {
        name: b"lsh",
        html5: Some(b"\xe2\x86\xb0"),
        html4: None,
    },
    Charref {
        name: b"lsim",
        html5: Some(b"\xe2\x89\xb2"),
        html4: None,
    },
    Charref {
        name: b"lsime",
        html5: Some(b"\xe2\xaa\x8d"),
        html4: None,
    },
    Charref {
        name: b"lsimg",
        html5: Some(b"\xe2\xaa\x8f"),
        html4: None,
    },
    Charref {
        name: b"lsqb",
        html5: Some(b"["),
        html4: None,
    },
    Charref {
        name: b"lsquo",
        html5: Some(b"\xe2\x80\x98"),
        html4: Some(b"\xe2\x80\x98"),
    },
    Charref {
        name: b"lsquor",
        html5: Some(b"\xe2\x80\x9a"),
        html4: None,
    },
    Charref {
        name: b"lstrok",
        html5: Some(b"\xc5\x82"),
        html4: None,
    },
    Charref {
        name: b"lt",
        html5: Some(b"<"),
        html4: Some(b"<"),
    },
    Charref {
        name: b"ltcc",
        html5: Some(b"\xe2\xaa\xa6"),
        html4: None,
    },
    Charref {
        name: b"ltcir",
        html5: Some(b"\xe2\xa9\xb9"),
        html4: None,
    },
    Charref {
        name: b"ltdot",
        html5: Some(b"\xe2\x8b\x96"),
        html4: None,
    },
    Charref {
        name: b"lthree",
        html5: Some(b"\xe2\x8b\x8b"),
        html4: None,
    },
    Charref {
        name: b"ltimes",
        html5: Some(b"\xe2\x8b\x89"),
        html4: None,
    },
    Charref {
        name: b"ltlarr",
        html5: Some(b"\xe2\xa5\xb6"),
        html4: None,
    },
    Charref {
        name: b"ltquest",
        html5: Some(b"\xe2\xa9\xbb"),
        html4: None,
    },
    Charref {
        name: b"ltrPar",
        html5: Some(b"\xe2\xa6\x96"),
        html4: None,
    },
    Charref {
        name: b"ltri",
        html5: Some(b"\xe2\x97\x83"),
        html4: None,
    },
    Charref {
        name: b"ltrie",
        html5: Some(b"\xe2\x8a\xb4"),
        html4: None,
    },
    Charref {
        name: b"ltrif",
        html5: Some(b"\xe2\x97\x82"),
        html4: None,
    },
    Charref {
        name: b"lurdshar",
        html5: Some(b"\xe2\xa5\x8a"),
        html4: None,
    },
    Charref {
        name: b"luruhar",
        html5: Some(b"\xe2\xa5\xa6"),
        html4: None,
    },
    Charref {
        name: b"lvertneqq",
        html5: Some(b"\xe2\x89\xa8\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"lvnE",
        html5: Some(b"\xe2\x89\xa8\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"mDDot",
        html5: Some(b"\xe2\x88\xba"),
        html4: None,
    },
    Charref {
        name: b"macr",
        html5: Some(b"\xc2\xaf"),
        html4: Some(b"\xc2\xaf"),
    },
    Charref {
        name: b"male",
        html5: Some(b"\xe2\x99\x82"),
        html4: None,
    },
    Charref {
        name: b"malt",
        html5: Some(b"\xe2\x9c\xa0"),
        html4: None,
    },
    Charref {
        name: b"maltese",
        html5: Some(b"\xe2\x9c\xa0"),
        html4: None,
    },
    Charref {
        name: b"map",
        html5: Some(b"\xe2\x86\xa6"),
        html4: None,
    },
    Charref {
        name: b"mapsto",
        html5: Some(b"\xe2\x86\xa6"),
        html4: None,
    },
    Charref {
        name: b"mapstodown",
        html5: Some(b"\xe2\x86\xa7"),
        html4: None,
    },
    Charref {
        name: b"mapstoleft",
        html5: Some(b"\xe2\x86\xa4"),
        html4: None,
    },
    Charref {
        name: b"mapstoup",
        html5: Some(b"\xe2\x86\xa5"),
        html4: None,
    },
    Charref {
        name: b"marker",
        html5: Some(b"\xe2\x96\xae"),
        html4: None,
    },
    Charref {
        name: b"mcomma",
        html5: Some(b"\xe2\xa8\xa9"),
        html4: None,
    },
    Charref {
        name: b"mcy",
        html5: Some(b"\xd0\xbc"),
        html4: None,
    },
    Charref {
        name: b"mdash",
        html5: Some(b"\xe2\x80\x94"),
        html4: Some(b"\xe2\x80\x94"),
    },
    Charref {
        name: b"measuredangle",
        html5: Some(b"\xe2\x88\xa1"),
        html4: None,
    },
    Charref {
        name: b"mfr",
        html5: Some(b"\xf0\x9d\x94\xaa"),
        html4: None,
    },
    Charref {
        name: b"mho",
        html5: Some(b"\xe2\x84\xa7"),
        html4: None,
    },
    Charref {
        name: b"micro",
        html5: Some(b"\xc2\xb5"),
        html4: Some(b"\xc2\xb5"),
    },
    Charref {
        name: b"mid",
        html5: Some(b"\xe2\x88\xa3"),
        html4: None,
    },
    Charref {
        name: b"midast",
        html5: Some(b"*"),
        html4: None,
    },
    Charref {
        name: b"midcir",
        html5: Some(b"\xe2\xab\xb0"),
        html4: None,
    },
    Charref {
        name: b"middot",
        html5: Some(b"\xc2\xb7"),
        html4: Some(b"\xc2\xb7"),
    },
    Charref {
        name: b"minus",
        html5: Some(b"\xe2\x88\x92"),
        html4: Some(b"\xe2\x88\x92"),
    },
    Charref {
        name: b"minusb",
        html5: Some(b"\xe2\x8a\x9f"),
        html4: None,
    },
    Charref {
        name: b"minusd",
        html5: Some(b"\xe2\x88\xb8"),
        html4: None,
    },
    Charref {
        name: b"minusdu",
        html5: Some(b"\xe2\xa8\xaa"),
        html4: None,
    },
    Charref {
        name: b"mlcp",
        html5: Some(b"\xe2\xab\x9b"),
        html4: None,
    },
    Charref {
        name: b"mldr",
        html5: Some(b"\xe2\x80\xa6"),
        html4: None,
    },
    Charref {
        name: b"mnplus",
        html5: Some(b"\xe2\x88\x93"),
        html4: None,
    },
    Charref {
        name: b"models",
        html5: Some(b"\xe2\x8a\xa7"),
        html4: None,
    },
    Charref {
        name: b"mopf",
        html5: Some(b"\xf0\x9d\x95\x9e"),
        html4: None,
    },
    Charref {
        name: b"mp",
        html5: Some(b"\xe2\x88\x93"),
        html4: None,
    },
    Charref {
        name: b"mscr",
        html5: Some(b"\xf0\x9d\x93\x82"),
        html4: None,
    },
    Charref {
        name: b"mstpos",
        html5: Some(b"\xe2\x88\xbe"),
        html4: None,
    },
    Charref {
        name: b"mu",
        html5: Some(b"\xce\xbc"),
        html4: Some(b"\xce\xbc"),
    },
    Charref {
        name: b"multimap",
        html5: Some(b"\xe2\x8a\xb8"),
        html4: None,
    },
    Charref {
        name: b"mumap",
        html5: Some(b"\xe2\x8a\xb8"),
        html4: None,
    },
    Charref {
        name: b"nGg",
        html5: Some(b"\xe2\x8b\x99\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nGt",
        html5: Some(b"\xe2\x89\xab\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nGtv",
        html5: Some(b"\xe2\x89\xab\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nLeftarrow",
        html5: Some(b"\xe2\x87\x8d"),
        html4: None,
    },
    Charref {
        name: b"nLeftrightarrow",
        html5: Some(b"\xe2\x87\x8e"),
        html4: None,
    },
    Charref {
        name: b"nLl",
        html5: Some(b"\xe2\x8b\x98\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nLt",
        html5: Some(b"\xe2\x89\xaa\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nLtv",
        html5: Some(b"\xe2\x89\xaa\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nRightarrow",
        html5: Some(b"\xe2\x87\x8f"),
        html4: None,
    },
    Charref {
        name: b"nVDash",
        html5: Some(b"\xe2\x8a\xaf"),
        html4: None,
    },
    Charref {
        name: b"nVdash",
        html5: Some(b"\xe2\x8a\xae"),
        html4: None,
    },
    Charref {
        name: b"nabla",
        html5: Some(b"\xe2\x88\x87"),
        html4: Some(b"\xe2\x88\x87"),
    },
    Charref {
        name: b"nacute",
        html5: Some(b"\xc5\x84"),
        html4: None,
    },
    Charref {
        name: b"nang",
        html5: Some(b"\xe2\x88\xa0\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nap",
        html5: Some(b"\xe2\x89\x89"),
        html4: None,
    },
    Charref {
        name: b"napE",
        html5: Some(b"\xe2\xa9\xb0\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"napid",
        html5: Some(b"\xe2\x89\x8b\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"napos",
        html5: Some(b"\xc5\x89"),
        html4: None,
    },
    Charref {
        name: b"napprox",
        html5: Some(b"\xe2\x89\x89"),
        html4: None,
    },
    Charref {
        name: b"natur",
        html5: Some(b"\xe2\x99\xae"),
        html4: None,
    },
    Charref {
        name: b"natural",
        html5: Some(b"\xe2\x99\xae"),
        html4: None,
    },
    Charref {
        name: b"naturals",
        html5: Some(b"\xe2\x84\x95"),
        html4: None,
    },
    Charref {
        name: b"nbsp",
        html5: Some(b"\xc2\xa0"),
        html4: Some(b"\xc2\xa0"),
    },
    Charref {
        name: b"nbump",
        html5: Some(b"\xe2\x89\x8e\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nbumpe",
        html5: Some(b"\xe2\x89\x8f\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"ncap",
        html5: Some(b"\xe2\xa9\x83"),
        html4: None,
    },
    Charref {
        name: b"ncaron",
        html5: Some(b"\xc5\x88"),
        html4: None,
    },
    Charref {
        name: b"ncedil",
        html5: Some(b"\xc5\x86"),
        html4: None,
    },
    Charref {
        name: b"ncong",
        html5: Some(b"\xe2\x89\x87"),
        html4: None,
    },
    Charref {
        name: b"ncongdot",
        html5: Some(b"\xe2\xa9\xad\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"ncup",
        html5: Some(b"\xe2\xa9\x82"),
        html4: None,
    },
    Charref {
        name: b"ncy",
        html5: Some(b"\xd0\xbd"),
        html4: None,
    },
    Charref {
        name: b"ndash",
        html5: Some(b"\xe2\x80\x93"),
        html4: Some(b"\xe2\x80\x93"),
    },
    Charref {
        name: b"ne",
        html5: Some(b"\xe2\x89\xa0"),
        html4: Some(b"\xe2\x89\xa0"),
    },
    Charref {
        name: b"neArr",
        html5: Some(b"\xe2\x87\x97"),
        html4: None,
    },
    Charref {
        name: b"nearhk",
        html5: Some(b"\xe2\xa4\xa4"),
        html4: None,
    },
    Charref {
        name: b"nearr",
        html5: Some(b"\xe2\x86\x97"),
        html4: None,
    },
    Charref {
        name: b"nearrow",
        html5: Some(b"\xe2\x86\x97"),
        html4: None,
    },
    Charref {
        name: b"nedot",
        html5: Some(b"\xe2\x89\x90\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nequiv",
        html5: Some(b"\xe2\x89\xa2"),
        html4: None,
    },
    Charref {
        name: b"nesear",
        html5: Some(b"\xe2\xa4\xa8"),
        html4: None,
    },
    Charref {
        name: b"nesim",
        html5: Some(b"\xe2\x89\x82\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nexist",
        html5: Some(b"\xe2\x88\x84"),
        html4: None,
    },
    Charref {
        name: b"nexists",
        html5: Some(b"\xe2\x88\x84"),
        html4: None,
    },
    Charref {
        name: b"nfr",
        html5: Some(b"\xf0\x9d\x94\xab"),
        html4: None,
    },
    Charref {
        name: b"ngE",
        html5: Some(b"\xe2\x89\xa7\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nge",
        html5: Some(b"\xe2\x89\xb1"),
        html4: None,
    },
    Charref {
        name: b"ngeq",
        html5: Some(b"\xe2\x89\xb1"),
        html4: None,
    },
    Charref {
        name: b"ngeqq",
        html5: Some(b"\xe2\x89\xa7\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"ngeqslant",
        html5: Some(b"\xe2\xa9\xbe\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nges",
        html5: Some(b"\xe2\xa9\xbe\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"ngsim",
        html5: Some(b"\xe2\x89\xb5"),
        html4: None,
    },
    Charref {
        name: b"ngt",
        html5: Some(b"\xe2\x89\xaf"),
        html4: None,
    },
    Charref {
        name: b"ngtr",
        html5: Some(b"\xe2\x89\xaf"),
        html4: None,
    },
    Charref {
        name: b"nhArr",
        html5: Some(b"\xe2\x87\x8e"),
        html4: None,
    },
    Charref {
        name: b"nharr",
        html5: Some(b"\xe2\x86\xae"),
        html4: None,
    },
    Charref {
        name: b"nhpar",
        html5: Some(b"\xe2\xab\xb2"),
        html4: None,
    },
    Charref {
        name: b"ni",
        html5: Some(b"\xe2\x88\x8b"),
        html4: Some(b"\xe2\x88\x8b"),
    },
    Charref {
        name: b"nis",
        html5: Some(b"\xe2\x8b\xbc"),
        html4: None,
    },
    Charref {
        name: b"nisd",
        html5: Some(b"\xe2\x8b\xba"),
        html4: None,
    },
    Charref {
        name: b"niv",
        html5: Some(b"\xe2\x88\x8b"),
        html4: None,
    },
    Charref {
        name: b"njcy",
        html5: Some(b"\xd1\x9a"),
        html4: None,
    },
    Charref {
        name: b"nlArr",
        html5: Some(b"\xe2\x87\x8d"),
        html4: None,
    },
    Charref {
        name: b"nlE",
        html5: Some(b"\xe2\x89\xa6\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nlarr",
        html5: Some(b"\xe2\x86\x9a"),
        html4: None,
    },
    Charref {
        name: b"nldr",
        html5: Some(b"\xe2\x80\xa5"),
        html4: None,
    },
    Charref {
        name: b"nle",
        html5: Some(b"\xe2\x89\xb0"),
        html4: None,
    },
    Charref {
        name: b"nleftarrow",
        html5: Some(b"\xe2\x86\x9a"),
        html4: None,
    },
    Charref {
        name: b"nleftrightarrow",
        html5: Some(b"\xe2\x86\xae"),
        html4: None,
    },
    Charref {
        name: b"nleq",
        html5: Some(b"\xe2\x89\xb0"),
        html4: None,
    },
    Charref {
        name: b"nleqq",
        html5: Some(b"\xe2\x89\xa6\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nleqslant",
        html5: Some(b"\xe2\xa9\xbd\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nles",
        html5: Some(b"\xe2\xa9\xbd\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nless",
        html5: Some(b"\xe2\x89\xae"),
        html4: None,
    },
    Charref {
        name: b"nlsim",
        html5: Some(b"\xe2\x89\xb4"),
        html4: None,
    },
    Charref {
        name: b"nlt",
        html5: Some(b"\xe2\x89\xae"),
        html4: None,
    },
    Charref {
        name: b"nltri",
        html5: Some(b"\xe2\x8b\xaa"),
        html4: None,
    },
    Charref {
        name: b"nltrie",
        html5: Some(b"\xe2\x8b\xac"),
        html4: None,
    },
    Charref {
        name: b"nmid",
        html5: Some(b"\xe2\x88\xa4"),
        html4: None,
    },
    Charref {
        name: b"nopf",
        html5: Some(b"\xf0\x9d\x95\x9f"),
        html4: None,
    },
    Charref {
        name: b"not",
        html5: Some(b"\xc2\xac"),
        html4: Some(b"\xc2\xac"),
    },
    Charref {
        name: b"notin",
        html5: Some(b"\xe2\x88\x89"),
        html4: Some(b"\xe2\x88\x89"),
    },
    Charref {
        name: b"notinE",
        html5: Some(b"\xe2\x8b\xb9\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"notindot",
        html5: Some(b"\xe2\x8b\xb5\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"notinva",
        html5: Some(b"\xe2\x88\x89"),
        html4: None,
    },
    Charref {
        name: b"notinvb",
        html5: Some(b"\xe2\x8b\xb7"),
        html4: None,
    },
    Charref {
        name: b"notinvc",
        html5: Some(b"\xe2\x8b\xb6"),
        html4: None,
    },
    Charref {
        name: b"notni",
        html5: Some(b"\xe2\x88\x8c"),
        html4: None,
    },
    Charref {
        name: b"notniva",
        html5: Some(b"\xe2\x88\x8c"),
        html4: None,
    },
    Charref {
        name: b"notnivb",
        html5: Some(b"\xe2\x8b\xbe"),
        html4: None,
    },
    Charref {
        name: b"notnivc",
        html5: Some(b"\xe2\x8b\xbd"),
        html4: None,
    },
    Charref {
        name: b"npar",
        html5: Some(b"\xe2\x88\xa6"),
        html4: None,
    },
    Charref {
        name: b"nparallel",
        html5: Some(b"\xe2\x88\xa6"),
        html4: None,
    },
    Charref {
        name: b"nparsl",
        html5: Some(b"\xe2\xab\xbd\xe2\x83\xa5"),
        html4: None,
    },
    Charref {
        name: b"npart",
        html5: Some(b"\xe2\x88\x82\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"npolint",
        html5: Some(b"\xe2\xa8\x94"),
        html4: None,
    },
    Charref {
        name: b"npr",
        html5: Some(b"\xe2\x8a\x80"),
        html4: None,
    },
    Charref {
        name: b"nprcue",
        html5: Some(b"\xe2\x8b\xa0"),
        html4: None,
    },
    Charref {
        name: b"npre",
        html5: Some(b"\xe2\xaa\xaf\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nprec",
        html5: Some(b"\xe2\x8a\x80"),
        html4: None,
    },
    Charref {
        name: b"npreceq",
        html5: Some(b"\xe2\xaa\xaf\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nrArr",
        html5: Some(b"\xe2\x87\x8f"),
        html4: None,
    },
    Charref {
        name: b"nrarr",
        html5: Some(b"\xe2\x86\x9b"),
        html4: None,
    },
    Charref {
        name: b"nrarrc",
        html5: Some(b"\xe2\xa4\xb3\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nrarrw",
        html5: Some(b"\xe2\x86\x9d\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nrightarrow",
        html5: Some(b"\xe2\x86\x9b"),
        html4: None,
    },
    Charref {
        name: b"nrtri",
        html5: Some(b"\xe2\x8b\xab"),
        html4: None,
    },
    Charref {
        name: b"nrtrie",
        html5: Some(b"\xe2\x8b\xad"),
        html4: None,
    },
    Charref {
        name: b"nsc",
        html5: Some(b"\xe2\x8a\x81"),
        html4: None,
    },
    Charref {
        name: b"nsccue",
        html5: Some(b"\xe2\x8b\xa1"),
        html4: None,
    },
    Charref {
        name: b"nsce",
        html5: Some(b"\xe2\xaa\xb0\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nscr",
        html5: Some(b"\xf0\x9d\x93\x83"),
        html4: None,
    },
    Charref {
        name: b"nshortmid",
        html5: Some(b"\xe2\x88\xa4"),
        html4: None,
    },
    Charref {
        name: b"nshortparallel",
        html5: Some(b"\xe2\x88\xa6"),
        html4: None,
    },
    Charref {
        name: b"nsim",
        html5: Some(b"\xe2\x89\x81"),
        html4: None,
    },
    Charref {
        name: b"nsime",
        html5: Some(b"\xe2\x89\x84"),
        html4: None,
    },
    Charref {
        name: b"nsimeq",
        html5: Some(b"\xe2\x89\x84"),
        html4: None,
    },
    Charref {
        name: b"nsmid",
        html5: Some(b"\xe2\x88\xa4"),
        html4: None,
    },
    Charref {
        name: b"nspar",
        html5: Some(b"\xe2\x88\xa6"),
        html4: None,
    },
    Charref {
        name: b"nsqsube",
        html5: Some(b"\xe2\x8b\xa2"),
        html4: None,
    },
    Charref {
        name: b"nsqsupe",
        html5: Some(b"\xe2\x8b\xa3"),
        html4: None,
    },
    Charref {
        name: b"nsub",
        html5: Some(b"\xe2\x8a\x84"),
        html4: Some(b"\xe2\x8a\x84"),
    },
    Charref {
        name: b"nsubE",
        html5: Some(b"\xe2\xab\x85\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nsube",
        html5: Some(b"\xe2\x8a\x88"),
        html4: None,
    },
    Charref {
        name: b"nsubset",
        html5: Some(b"\xe2\x8a\x82\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nsubseteq",
        html5: Some(b"\xe2\x8a\x88"),
        html4: None,
    },
    Charref {
        name: b"nsubseteqq",
        html5: Some(b"\xe2\xab\x85\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nsucc",
        html5: Some(b"\xe2\x8a\x81"),
        html4: None,
    },
    Charref {
        name: b"nsucceq",
        html5: Some(b"\xe2\xaa\xb0\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nsup",
        html5: Some(b"\xe2\x8a\x85"),
        html4: None,
    },
    Charref {
        name: b"nsupE",
        html5: Some(b"\xe2\xab\x86\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"nsupe",
        html5: Some(b"\xe2\x8a\x89"),
        html4: None,
    },
    Charref {
        name: b"nsupset",
        html5: Some(b"\xe2\x8a\x83\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nsupseteq",
        html5: Some(b"\xe2\x8a\x89"),
        html4: None,
    },
    Charref {
        name: b"nsupseteqq",
        html5: Some(b"\xe2\xab\x86\xcc\xb8"),
        html4: None,
    },
    Charref {
        name: b"ntgl",
        html5: Some(b"\xe2\x89\xb9"),
        html4: None,
    },
    Charref {
        name: b"ntilde",
        html5: Some(b"\xc3\xb1"),
        html4: Some(b"\xc3\xb1"),
    },
    Charref {
        name: b"ntlg",
        html5: Some(b"\xe2\x89\xb8"),
        html4: None,
    },
    Charref {
        name: b"ntriangleleft",
        html5: Some(b"\xe2\x8b\xaa"),
        html4: None,
    },
    Charref {
        name: b"ntrianglelefteq",
        html5: Some(b"\xe2\x8b\xac"),
        html4: None,
    },
    Charref {
        name: b"ntriangleright",
        html5: Some(b"\xe2\x8b\xab"),
        html4: None,
    },
    Charref {
        name: b"ntrianglerighteq",
        html5: Some(b"\xe2\x8b\xad"),
        html4: None,
    },
    Charref {
        name: b"nu",
        html5: Some(b"\xce\xbd"),
        html4: Some(b"\xce\xbd"),
    },
    Charref {
        name: b"num",
        html5: Some(b"#"),
        html4: None,
    },
    Charref {
        name: b"numero",
        html5: Some(b"\xe2\x84\x96"),
        html4: None,
    },
    Charref {
        name: b"numsp",
        html5: Some(b"\xe2\x80\x87"),
        html4: None,
    },
    Charref {
        name: b"nvDash",
        html5: Some(b"\xe2\x8a\xad"),
        html4: None,
    },
    Charref {
        name: b"nvHarr",
        html5: Some(b"\xe2\xa4\x84"),
        html4: None,
    },
    Charref {
        name: b"nvap",
        html5: Some(b"\xe2\x89\x8d\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nvdash",
        html5: Some(b"\xe2\x8a\xac"),
        html4: None,
    },
    Charref {
        name: b"nvge",
        html5: Some(b"\xe2\x89\xa5\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nvgt",
        html5: Some(b">\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nvinfin",
        html5: Some(b"\xe2\xa7\x9e"),
        html4: None,
    },
    Charref {
        name: b"nvlArr",
        html5: Some(b"\xe2\xa4\x82"),
        html4: None,
    },
    Charref {
        name: b"nvle",
        html5: Some(b"\xe2\x89\xa4\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nvlt",
        html5: Some(b"<\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nvltrie",
        html5: Some(b"\xe2\x8a\xb4\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nvrArr",
        html5: Some(b"\xe2\xa4\x83"),
        html4: None,
    },
    Charref {
        name: b"nvrtrie",
        html5: Some(b"\xe2\x8a\xb5\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nvsim",
        html5: Some(b"\xe2\x88\xbc\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"nwArr",
        html5: Some(b"\xe2\x87\x96"),
        html4: None,
    },
    Charref {
        name: b"nwarhk",
        html5: Some(b"\xe2\xa4\xa3"),
        html4: None,
    },
    Charref {
        name: b"nwarr",
        html5: Some(b"\xe2\x86\x96"),
        html4: None,
    },
    Charref {
        name: b"nwarrow",
        html5: Some(b"\xe2\x86\x96"),
        html4: None,
    },
    Charref {
        name: b"nwnear",
        html5: Some(b"\xe2\xa4\xa7"),
        html4: None,
    },
    Charref {
        name: b"oS",
        html5: Some(b"\xe2\x93\x88"),
        html4: None,
    },
    Charref {
        name: b"oacute",
        html5: Some(b"\xc3\xb3"),
        html4: Some(b"\xc3\xb3"),
    },
    Charref {
        name: b"oast",
        html5: Some(b"\xe2\x8a\x9b"),
        html4: None,
    },
    Charref {
        name: b"ocir",
        html5: Some(b"\xe2\x8a\x9a"),
        html4: None,
    },
    Charref {
        name: b"ocirc",
        html5: Some(b"\xc3\xb4"),
        html4: Some(b"\xc3\xb4"),
    },
    Charref {
        name: b"ocy",
        html5: Some(b"\xd0\xbe"),
        html4: None,
    },
    Charref {
        name: b"odash",
        html5: Some(b"\xe2\x8a\x9d"),
        html4: None,
    },
    Charref {
        name: b"odblac",
        html5: Some(b"\xc5\x91"),
        html4: None,
    },
    Charref {
        name: b"odiv",
        html5: Some(b"\xe2\xa8\xb8"),
        html4: None,
    },
    Charref {
        name: b"odot",
        html5: Some(b"\xe2\x8a\x99"),
        html4: None,
    },
    Charref {
        name: b"odsold",
        html5: Some(b"\xe2\xa6\xbc"),
        html4: None,
    },
    Charref {
        name: b"oelig",
        html5: Some(b"\xc5\x93"),
        html4: Some(b"\xc5\x93"),
    },
    Charref {
        name: b"ofcir",
        html5: Some(b"\xe2\xa6\xbf"),
        html4: None,
    },
    Charref {
        name: b"ofr",
        html5: Some(b"\xf0\x9d\x94\xac"),
        html4: None,
    },
    Charref {
        name: b"ogon",
        html5: Some(b"\xcb\x9b"),
        html4: None,
    },
    Charref {
        name: b"ograve",
        html5: Some(b"\xc3\xb2"),
        html4: Some(b"\xc3\xb2"),
    },
    Charref {
        name: b"ogt",
        html5: Some(b"\xe2\xa7\x81"),
        html4: None,
    },
    Charref {
        name: b"ohbar",
        html5: Some(b"\xe2\xa6\xb5"),
        html4: None,
    },
    Charref {
        name: b"ohm",
        html5: Some(b"\xce\xa9"),
        html4: None,
    },
    Charref {
        name: b"oint",
        html5: Some(b"\xe2\x88\xae"),
        html4: None,
    },
    Charref {
        name: b"olarr",
        html5: Some(b"\xe2\x86\xba"),
        html4: None,
    },
    Charref {
        name: b"olcir",
        html5: Some(b"\xe2\xa6\xbe"),
        html4: None,
    },
    Charref {
        name: b"olcross",
        html5: Some(b"\xe2\xa6\xbb"),
        html4: None,
    },
    Charref {
        name: b"oline",
        html5: Some(b"\xe2\x80\xbe"),
        html4: Some(b"\xe2\x80\xbe"),
    },
    Charref {
        name: b"olt",
        html5: Some(b"\xe2\xa7\x80"),
        html4: None,
    },
    Charref {
        name: b"omacr",
        html5: Some(b"\xc5\x8d"),
        html4: None,
    },
    Charref {
        name: b"omega",
        html5: Some(b"\xcf\x89"),
        html4: Some(b"\xcf\x89"),
    },
    Charref {
        name: b"omicron",
        html5: Some(b"\xce\xbf"),
        html4: Some(b"\xce\xbf"),
    },
    Charref {
        name: b"omid",
        html5: Some(b"\xe2\xa6\xb6"),
        html4: None,
    },
    Charref {
        name: b"ominus",
        html5: Some(b"\xe2\x8a\x96"),
        html4: None,
    },
    Charref {
        name: b"oopf",
        html5: Some(b"\xf0\x9d\x95\xa0"),
        html4: None,
    },
    Charref {
        name: b"opar",
        html5: Some(b"\xe2\xa6\xb7"),
        html4: None,
    },
    Charref {
        name: b"operp",
        html5: Some(b"\xe2\xa6\xb9"),
        html4: None,
    },
    Charref {
        name: b"oplus",
        html5: Some(b"\xe2\x8a\x95"),
        html4: Some(b"\xe2\x8a\x95"),
    },
    Charref {
        name: b"or",
        html5: Some(b"\xe2\x88\xa8"),
        html4: Some(b"\xe2\x88\xa8"),
    },
    Charref {
        name: b"orarr",
        html5: Some(b"\xe2\x86\xbb"),
        html4: None,
    },
    Charref {
        name: b"ord",
        html5: Some(b"\xe2\xa9\x9d"),
        html4: None,
    },
    Charref {
        name: b"order",
        html5: Some(b"\xe2\x84\xb4"),
        html4: None,
    },
    Charref {
        name: b"orderof",
        html5: Some(b"\xe2\x84\xb4"),
        html4: None,
    },
    Charref {
        name: b"ordf",
        html5: Some(b"\xc2\xaa"),
        html4: Some(b"\xc2\xaa"),
    },
    Charref {
        name: b"ordm",
        html5: Some(b"\xc2\xba"),
        html4: Some(b"\xc2\xba"),
    },
    Charref {
        name: b"origof",
        html5: Some(b"\xe2\x8a\xb6"),
        html4: None,
    },
    Charref {
        name: b"oror",
        html5: Some(b"\xe2\xa9\x96"),
        html4: None,
    },
    Charref {
        name: b"orslope",
        html5: Some(b"\xe2\xa9\x97"),
        html4: None,
    },
    Charref {
        name: b"orv",
        html5: Some(b"\xe2\xa9\x9b"),
        html4: None,
    },
    Charref {
        name: b"oscr",
        html5: Some(b"\xe2\x84\xb4"),
        html4: None,
    },
    Charref {
        name: b"oslash",
        html5: Some(b"\xc3\xb8"),
        html4: Some(b"\xc3\xb8"),
    },
    Charref {
        name: b"osol",
        html5: Some(b"\xe2\x8a\x98"),
        html4: None,
    },
    Charref {
        name: b"otilde",
        html5: Some(b"\xc3\xb5"),
        html4: Some(b"\xc3\xb5"),
    },
    Charref {
        name: b"otimes",
        html5: Some(b"\xe2\x8a\x97"),
        html4: Some(b"\xe2\x8a\x97"),
    },
    Charref {
        name: b"otimesas",
        html5: Some(b"\xe2\xa8\xb6"),
        html4: None,
    },
    Charref {
        name: b"ouml",
        html5: Some(b"\xc3\xb6"),
        html4: Some(b"\xc3\xb6"),
    },
    Charref {
        name: b"ovbar",
        html5: Some(b"\xe2\x8c\xbd"),
        html4: None,
    },
    Charref {
        name: b"par",
        html5: Some(b"\xe2\x88\xa5"),
        html4: None,
    },
    Charref {
        name: b"para",
        html5: Some(b"\xc2\xb6"),
        html4: Some(b"\xc2\xb6"),
    },
    Charref {
        name: b"parallel",
        html5: Some(b"\xe2\x88\xa5"),
        html4: None,
    },
    Charref {
        name: b"parsim",
        html5: Some(b"\xe2\xab\xb3"),
        html4: None,
    },
    Charref {
        name: b"parsl",
        html5: Some(b"\xe2\xab\xbd"),
        html4: None,
    },
    Charref {
        name: b"part",
        html5: Some(b"\xe2\x88\x82"),
        html4: Some(b"\xe2\x88\x82"),
    },
    Charref {
        name: b"pcy",
        html5: Some(b"\xd0\xbf"),
        html4: None,
    },
    Charref {
        name: b"percnt",
        html5: Some(b"%"),
        html4: None,
    },
    Charref {
        name: b"period",
        html5: Some(b"."),
        html4: None,
    },
    Charref {
        name: b"permil",
        html5: Some(b"\xe2\x80\xb0"),
        html4: Some(b"\xe2\x80\xb0"),
    },
    Charref {
        name: b"perp",
        html5: Some(b"\xe2\x8a\xa5"),
        html4: Some(b"\xe2\x8a\xa5"),
    },
    Charref {
        name: b"pertenk",
        html5: Some(b"\xe2\x80\xb1"),
        html4: None,
    },
    Charref {
        name: b"pfr",
        html5: Some(b"\xf0\x9d\x94\xad"),
        html4: None,
    },
    Charref {
        name: b"phi",
        html5: Some(b"\xcf\x86"),
        html4: Some(b"\xcf\x86"),
    },
    Charref {
        name: b"phiv",
        html5: Some(b"\xcf\x95"),
        html4: None,
    },
    Charref {
        name: b"phmmat",
        html5: Some(b"\xe2\x84\xb3"),
        html4: None,
    },
    Charref {
        name: b"phone",
        html5: Some(b"\xe2\x98\x8e"),
        html4: None,
    },
    Charref {
        name: b"pi",
        html5: Some(b"\xcf\x80"),
        html4: Some(b"\xcf\x80"),
    },
    Charref {
        name: b"pitchfork",
        html5: Some(b"\xe2\x8b\x94"),
        html4: None,
    },
    Charref {
        name: b"piv",
        html5: Some(b"\xcf\x96"),
        html4: Some(b"\xcf\x96"),
    },
    Charref {
        name: b"planck",
        html5: Some(b"\xe2\x84\x8f"),
        html4: None,
    },
    Charref {
        name: b"planckh",
        html5: Some(b"\xe2\x84\x8e"),
        html4: None,
    },
    Charref {
        name: b"plankv",
        html5: Some(b"\xe2\x84\x8f"),
        html4: None,
    },
    Charref {
        name: b"plus",
        html5: Some(b"+"),
        html4: None,
    },
    Charref {
        name: b"plusacir",
        html5: Some(b"\xe2\xa8\xa3"),
        html4: None,
    },
    Charref {
        name: b"plusb",
        html5: Some(b"\xe2\x8a\x9e"),
        html4: None,
    },
    Charref {
        name: b"pluscir",
        html5: Some(b"\xe2\xa8\xa2"),
        html4: None,
    },
    Charref {
        name: b"plusdo",
        html5: Some(b"\xe2\x88\x94"),
        html4: None,
    },
    Charref {
        name: b"plusdu",
        html5: Some(b"\xe2\xa8\xa5"),
        html4: None,
    },
    Charref {
        name: b"pluse",
        html5: Some(b"\xe2\xa9\xb2"),
        html4: None,
    },
    Charref {
        name: b"plusmn",
        html5: Some(b"\xc2\xb1"),
        html4: Some(b"\xc2\xb1"),
    },
    Charref {
        name: b"plussim",
        html5: Some(b"\xe2\xa8\xa6"),
        html4: None,
    },
    Charref {
        name: b"plustwo",
        html5: Some(b"\xe2\xa8\xa7"),
        html4: None,
    },
    Charref {
        name: b"pm",
        html5: Some(b"\xc2\xb1"),
        html4: None,
    },
    Charref {
        name: b"pointint",
        html5: Some(b"\xe2\xa8\x95"),
        html4: None,
    },
    Charref {
        name: b"popf",
        html5: Some(b"\xf0\x9d\x95\xa1"),
        html4: None,
    },
    Charref {
        name: b"pound",
        html5: Some(b"\xc2\xa3"),
        html4: Some(b"\xc2\xa3"),
    },
    Charref {
        name: b"pr",
        html5: Some(b"\xe2\x89\xba"),
        html4: None,
    },
    Charref {
        name: b"prE",
        html5: Some(b"\xe2\xaa\xb3"),
        html4: None,
    },
    Charref {
        name: b"prap",
        html5: Some(b"\xe2\xaa\xb7"),
        html4: None,
    },
    Charref {
        name: b"prcue",
        html5: Some(b"\xe2\x89\xbc"),
        html4: None,
    },
    Charref {
        name: b"pre",
        html5: Some(b"\xe2\xaa\xaf"),
        html4: None,
    },
    Charref {
        name: b"prec",
        html5: Some(b"\xe2\x89\xba"),
        html4: None,
    },
    Charref {
        name: b"precapprox",
        html5: Some(b"\xe2\xaa\xb7"),
        html4: None,
    },
    Charref {
        name: b"preccurlyeq",
        html5: Some(b"\xe2\x89\xbc"),
        html4: None,
    },
    Charref {
        name: b"preceq",
        html5: Some(b"\xe2\xaa\xaf"),
        html4: None,
    },
    Charref {
        name: b"precnapprox",
        html5: Some(b"\xe2\xaa\xb9"),
        html4: None,
    },
    Charref {
        name: b"precneqq",
        html5: Some(b"\xe2\xaa\xb5"),
        html4: None,
    },
    Charref {
        name: b"precnsim",
        html5: Some(b"\xe2\x8b\xa8"),
        html4: None,
    },
    Charref {
        name: b"precsim",
        html5: Some(b"\xe2\x89\xbe"),
        html4: None,
    },
    Charref {
        name: b"prime",
        html5: Some(b"\xe2\x80\xb2"),
        html4: Some(b"\xe2\x80\xb2"),
    },
    Charref {
        name: b"primes",
        html5: Some(b"\xe2\x84\x99"),
        html4: None,
    },
    Charref {
        name: b"prnE",
        html5: Some(b"\xe2\xaa\xb5"),
        html4: None,
    },
    Charref {
        name: b"prnap",
        html5: Some(b"\xe2\xaa\xb9"),
        html4: None,
    },
    Charref {
        name: b"prnsim",
        html5: Some(b"\xe2\x8b\xa8"),
        html4: None,
    },
    Charref {
        name: b"prod",
        html5: Some(b"\xe2\x88\x8f"),
        html4: Some(b"\xe2\x88\x8f"),
    },
    Charref {
        name: b"profalar",
        html5: Some(b"\xe2\x8c\xae"),
        html4: None,
    },
    Charref {
        name: b"profline",
        html5: Some(b"\xe2\x8c\x92"),
        html4: None,
    },
    Charref {
        name: b"profsurf",
        html5: Some(b"\xe2\x8c\x93"),
        html4: None,
    },
    Charref {
        name: b"prop",
        html5: Some(b"\xe2\x88\x9d"),
        html4: Some(b"\xe2\x88\x9d"),
    },
    Charref {
        name: b"propto",
        html5: Some(b"\xe2\x88\x9d"),
        html4: None,
    },
    Charref {
        name: b"prsim",
        html5: Some(b"\xe2\x89\xbe"),
        html4: None,
    },
    Charref {
        name: b"prurel",
        html5: Some(b"\xe2\x8a\xb0"),
        html4: None,
    },
    Charref {
        name: b"pscr",
        html5: Some(b"\xf0\x9d\x93\x85"),
        html4: None,
    },
    Charref {
        name: b"psi",
        html5: Some(b"\xcf\x88"),
        html4: Some(b"\xcf\x88"),
    },
    Charref {
        name: b"puncsp",
        html5: Some(b"\xe2\x80\x88"),
        html4: None,
    },
    Charref {
        name: b"qfr",
        html5: Some(b"\xf0\x9d\x94\xae"),
        html4: None,
    },
    Charref {
        name: b"qint",
        html5: Some(b"\xe2\xa8\x8c"),
        html4: None,
    },
    Charref {
        name: b"qopf",
        html5: Some(b"\xf0\x9d\x95\xa2"),
        html4: None,
    },
    Charref {
        name: b"qprime",
        html5: Some(b"\xe2\x81\x97"),
        html4: None,
    },
    Charref {
        name: b"qscr",
        html5: Some(b"\xf0\x9d\x93\x86"),
        html4: None,
    },
    Charref {
        name: b"quaternions",
        html5: Some(b"\xe2\x84\x8d"),
        html4: None,
    },
    Charref {
        name: b"quatint",
        html5: Some(b"\xe2\xa8\x96"),
        html4: None,
    },
    Charref {
        name: b"quest",
        html5: Some(b"?"),
        html4: None,
    },
    Charref {
        name: b"questeq",
        html5: Some(b"\xe2\x89\x9f"),
        html4: None,
    },
    Charref {
        name: b"quot",
        html5: Some(b"\""),
        html4: Some(b"\""),
    },
    Charref {
        name: b"rAarr",
        html5: Some(b"\xe2\x87\x9b"),
        html4: None,
    },
    Charref {
        name: b"rArr",
        html5: Some(b"\xe2\x87\x92"),
        html4: Some(b"\xe2\x87\x92"),
    },
    Charref {
        name: b"rAtail",
        html5: Some(b"\xe2\xa4\x9c"),
        html4: None,
    },
    Charref {
        name: b"rBarr",
        html5: Some(b"\xe2\xa4\x8f"),
        html4: None,
    },
    Charref {
        name: b"rHar",
        html5: Some(b"\xe2\xa5\xa4"),
        html4: None,
    },
    Charref {
        name: b"race",
        html5: Some(b"\xe2\x88\xbd\xcc\xb1"),
        html4: None,
    },
    Charref {
        name: b"racute",
        html5: Some(b"\xc5\x95"),
        html4: None,
    },
    Charref {
        name: b"radic",
        html5: Some(b"\xe2\x88\x9a"),
        html4: Some(b"\xe2\x88\x9a"),
    },
    Charref {
        name: b"raemptyv",
        html5: Some(b"\xe2\xa6\xb3"),
        html4: None,
    },
    Charref {
        name: b"rang",
        html5: Some(b"\xe2\x9f\xa9"),
        html4: Some(b"\xe2\x8c\xaa"),
    },
    Charref {
        name: b"rangd",
        html5: Some(b"\xe2\xa6\x92"),
        html4: None,
    },
    Charref {
        name: b"range",
        html5: Some(b"\xe2\xa6\xa5"),
        html4: None,
    },
    Charref {
        name: b"rangle",
        html5: Some(b"\xe2\x9f\xa9"),
        html4: None,
    },
    Charref {
        name: b"raquo",
        html5: Some(b"\xc2\xbb"),
        html4: Some(b"\xc2\xbb"),
    },
    Charref {
        name: b"rarr",
        html5: Some(b"\xe2\x86\x92"),
        html4: Some(b"\xe2\x86\x92"),
    },
    Charref {
        name: b"rarrap",
        html5: Some(b"\xe2\xa5\xb5"),
        html4: None,
    },
    Charref {
        name: b"rarrb",
        html5: Some(b"\xe2\x87\xa5"),
        html4: None,
    },
    Charref {
        name: b"rarrbfs",
        html5: Some(b"\xe2\xa4\xa0"),
        html4: None,
    },
    Charref {
        name: b"rarrc",
        html5: Some(b"\xe2\xa4\xb3"),
        html4: None,
    },
    Charref {
        name: b"rarrfs",
        html5: Some(b"\xe2\xa4\x9e"),
        html4: None,
    },
    Charref {
        name: b"rarrhk",
        html5: Some(b"\xe2\x86\xaa"),
        html4: None,
    },
    Charref {
        name: b"rarrlp",
        html5: Some(b"\xe2\x86\xac"),
        html4: None,
    },
    Charref {
        name: b"rarrpl",
        html5: Some(b"\xe2\xa5\x85"),
        html4: None,
    },
    Charref {
        name: b"rarrsim",
        html5: Some(b"\xe2\xa5\xb4"),
        html4: None,
    },
    Charref {
        name: b"rarrtl",
        html5: Some(b"\xe2\x86\xa3"),
        html4: None,
    },
    Charref {
        name: b"rarrw",
        html5: Some(b"\xe2\x86\x9d"),
        html4: None,
    },
    Charref {
        name: b"ratail",
        html5: Some(b"\xe2\xa4\x9a"),
        html4: None,
    },
    Charref {
        name: b"ratio",
        html5: Some(b"\xe2\x88\xb6"),
        html4: None,
    },
    Charref {
        name: b"rationals",
        html5: Some(b"\xe2\x84\x9a"),
        html4: None,
    },
    Charref {
        name: b"rbarr",
        html5: Some(b"\xe2\xa4\x8d"),
        html4: None,
    },
    Charref {
        name: b"rbbrk",
        html5: Some(b"\xe2\x9d\xb3"),
        html4: None,
    },
    Charref {
        name: b"rbrace",
        html5: Some(b"}"),
        html4: None,
    },
    Charref {
        name: b"rbrack",
        html5: Some(b"]"),
        html4: None,
    },
    Charref {
        name: b"rbrke",
        html5: Some(b"\xe2\xa6\x8c"),
        html4: None,
    },
    Charref {
        name: b"rbrksld",
        html5: Some(b"\xe2\xa6\x8e"),
        html4: None,
    },
    Charref {
        name: b"rbrkslu",
        html5: Some(b"\xe2\xa6\x90"),
        html4: None,
    },
    Charref {
        name: b"rcaron",
        html5: Some(b"\xc5\x99"),
        html4: None,
    },
    Charref {
        name: b"rcedil",
        html5: Some(b"\xc5\x97"),
        html4: None,
    },
    Charref {
        name: b"rceil",
        html5: Some(b"\xe2\x8c\x89"),
        html4: Some(b"\xe2\x8c\x89"),
    },
    Charref {
        name: b"rcub",
        html5: Some(b"}"),
        html4: None,
    },
    Charref {
        name: b"rcy",
        html5: Some(b"\xd1\x80"),
        html4: None,
    },
    Charref {
        name: b"rdca",
        html5: Some(b"\xe2\xa4\xb7"),
        html4: None,
    },
    Charref {
        name: b"rdldhar",
        html5: Some(b"\xe2\xa5\xa9"),
        html4: None,
    },
    Charref {
        name: b"rdquo",
        html5: Some(b"\xe2\x80\x9d"),
        html4: Some(b"\xe2\x80\x9d"),
    },
    Charref {
        name: b"rdquor",
        html5: Some(b"\xe2\x80\x9d"),
        html4: None,
    },
    Charref {
        name: b"rdsh",
        html5: Some(b"\xe2\x86\xb3"),
        html4: None,
    },
    Charref {
        name: b"real",
        html5: Some(b"\xe2\x84\x9c"),
        html4: Some(b"\xe2\x84\x9c"),
    },
    Charref {
        name: b"realine",
        html5: Some(b"\xe2\x84\x9b"),
        html4: None,
    },
    Charref {
        name: b"realpart",
        html5: Some(b"\xe2\x84\x9c"),
        html4: None,
    },
    Charref {
        name: b"reals",
        html5: Some(b"\xe2\x84\x9d"),
        html4: None,
    },
    Charref {
        name: b"rect",
        html5: Some(b"\xe2\x96\xad"),
        html4: None,
    },
    Charref {
        name: b"reg",
        html5: Some(b"\xc2\xae"),
        html4: Some(b"\xc2\xae"),
    },
    Charref {
        name: b"rfisht",
        html5: Some(b"\xe2\xa5\xbd"),
        html4: None,
    },
    Charref {
        name: b"rfloor",
        html5: Some(b"\xe2\x8c\x8b"),
        html4: Some(b"\xe2\x8c\x8b"),
    },
    Charref {
        name: b"rfr",
        html5: Some(b"\xf0\x9d\x94\xaf"),
        html4: None,
    },
    Charref {
        name: b"rhard",
        html5: Some(b"\xe2\x87\x81"),
        html4: None,
    },
    Charref {
        name: b"rharu",
        html5: Some(b"\xe2\x87\x80"),
        html4: None,
    },
    Charref {
        name: b"rharul",
        html5: Some(b"\xe2\xa5\xac"),
        html4: None,
    },
    Charref {
        name: b"rho",
        html5: Some(b"\xcf\x81"),
        html4: Some(b"\xcf\x81"),
    },
    Charref {
        name: b"rhov",
        html5: Some(b"\xcf\xb1"),
        html4: None,
    },
    Charref {
        name: b"rightarrow",
        html5: Some(b"\xe2\x86\x92"),
        html4: None,
    },
    Charref {
        name: b"rightarrowtail",
        html5: Some(b"\xe2\x86\xa3"),
        html4: None,
    },
    Charref {
        name: b"rightharpoondown",
        html5: Some(b"\xe2\x87\x81"),
        html4: None,
    },
    Charref {
        name: b"rightharpoonup",
        html5: Some(b"\xe2\x87\x80"),
        html4: None,
    },
    Charref {
        name: b"rightleftarrows",
        html5: Some(b"\xe2\x87\x84"),
        html4: None,
    },
    Charref {
        name: b"rightleftharpoons",
        html5: Some(b"\xe2\x87\x8c"),
        html4: None,
    },
    Charref {
        name: b"rightrightarrows",
        html5: Some(b"\xe2\x87\x89"),
        html4: None,
    },
    Charref {
        name: b"rightsquigarrow",
        html5: Some(b"\xe2\x86\x9d"),
        html4: None,
    },
    Charref {
        name: b"rightthreetimes",
        html5: Some(b"\xe2\x8b\x8c"),
        html4: None,
    },
    Charref {
        name: b"ring",
        html5: Some(b"\xcb\x9a"),
        html4: None,
    },
    Charref {
        name: b"risingdotseq",
        html5: Some(b"\xe2\x89\x93"),
        html4: None,
    },
    Charref {
        name: b"rlarr",
        html5: Some(b"\xe2\x87\x84"),
        html4: None,
    },
    Charref {
        name: b"rlhar",
        html5: Some(b"\xe2\x87\x8c"),
        html4: None,
    },
    Charref {
        name: b"rlm",
        html5: Some(b"\xe2\x80\x8f"),
        html4: Some(b"\xe2\x80\x8f"),
    },
    Charref {
        name: b"rmoust",
        html5: Some(b"\xe2\x8e\xb1"),
        html4: None,
    },
    Charref {
        name: b"rmoustache",
        html5: Some(b"\xe2\x8e\xb1"),
        html4: None,
    },
    Charref {
        name: b"rnmid",
        html5: Some(b"\xe2\xab\xae"),
        html4: None,
    },
    Charref {
        name: b"roang",
        html5: Some(b"\xe2\x9f\xad"),
        html4: None,
    },
    Charref {
        name: b"roarr",
        html5: Some(b"\xe2\x87\xbe"),
        html4: None,
    },
    Charref {
        name: b"robrk",
        html5: Some(b"\xe2\x9f\xa7"),
        html4: None,
    },
    Charref {
        name: b"ropar",
        html5: Some(b"\xe2\xa6\x86"),
        html4: None,
    },
    Charref {
        name: b"ropf",
        html5: Some(b"\xf0\x9d\x95\xa3"),
        html4: None,
    },
    Charref {
        name: b"roplus",
        html5: Some(b"\xe2\xa8\xae"),
        html4: None,
    },
    Charref {
        name: b"rotimes",
        html5: Some(b"\xe2\xa8\xb5"),
        html4: None,
    },
    Charref {
        name: b"rpar",
        html5: Some(b")"),
        html4: None,
    },
    Charref {
        name: b"rpargt",
        html5: Some(b"\xe2\xa6\x94"),
        html4: None,
    },
    Charref {
        name: b"rppolint",
        html5: Some(b"\xe2\xa8\x92"),
        html4: None,
    },
    Charref {
        name: b"rrarr",
        html5: Some(b"\xe2\x87\x89"),
        html4: None,
    },
    Charref {
        name: b"rsaquo",
        html5: Some(b"\xe2\x80\xba"),
        html4: Some(b"\xe2\x80\xba"),
    },
    Charref {
        name: b"rscr",
        html5: Some(b"\xf0\x9d\x93\x87"),
        html4: None,
    },
    Charref {
        name: b"rsh",
        html5: Some(b"\xe2\x86\xb1"),
        html4: None,
    },
    Charref {
        name: b"rsqb",
        html5: Some(b"]"),
        html4: None,
    },
    Charref {
        name: b"rsquo",
        html5: Some(b"\xe2\x80\x99"),
        html4: Some(b"\xe2\x80\x99"),
    },
    Charref {
        name: b"rsquor",
        html5: Some(b"\xe2\x80\x99"),
        html4: None,
    },
    Charref {
        name: b"rthree",
        html5: Some(b"\xe2\x8b\x8c"),
        html4: None,
    },
    Charref {
        name: b"rtimes",
        html5: Some(b"\xe2\x8b\x8a"),
        html4: None,
    },
    Charref {
        name: b"rtri",
        html5: Some(b"\xe2\x96\xb9"),
        html4: None,
    },
    Charref {
        name: b"rtrie",
        html5: Some(b"\xe2\x8a\xb5"),
        html4: None,
    },
    Charref {
        name: b"rtrif",
        html5: Some(b"\xe2\x96\xb8"),
        html4: None,
    },
    Charref {
        name: b"rtriltri",
        html5: Some(b"\xe2\xa7\x8e"),
        html4: None,
    },
    Charref {
        name: b"ruluhar",
        html5: Some(b"\xe2\xa5\xa8"),
        html4: None,
    },
    Charref {
        name: b"rx",
        html5: Some(b"\xe2\x84\x9e"),
        html4: None,
    },
    Charref {
        name: b"sacute",
        html5: Some(b"\xc5\x9b"),
        html4: None,
    },
    Charref {
        name: b"sbquo",
        html5: Some(b"\xe2\x80\x9a"),
        html4: Some(b"\xe2\x80\x9a"),
    },
    Charref {
        name: b"sc",
        html5: Some(b"\xe2\x89\xbb"),
        html4: None,
    },
    Charref {
        name: b"scE",
        html5: Some(b"\xe2\xaa\xb4"),
        html4: None,
    },
    Charref {
        name: b"scap",
        html5: Some(b"\xe2\xaa\xb8"),
        html4: None,
    },
    Charref {
        name: b"scaron",
        html5: Some(b"\xc5\xa1"),
        html4: Some(b"\xc5\xa1"),
    },
    Charref {
        name: b"sccue",
        html5: Some(b"\xe2\x89\xbd"),
        html4: None,
    },
    Charref {
        name: b"sce",
        html5: Some(b"\xe2\xaa\xb0"),
        html4: None,
    },
    Charref {
        name: b"scedil",
        html5: Some(b"\xc5\x9f"),
        html4: None,
    },
    Charref {
        name: b"scirc",
        html5: Some(b"\xc5\x9d"),
        html4: None,
    },
    Charref {
        name: b"scnE",
        html5: Some(b"\xe2\xaa\xb6"),
        html4: None,
    },
    Charref {
        name: b"scnap",
        html5: Some(b"\xe2\xaa\xba"),
        html4: None,
    },
    Charref {
        name: b"scnsim",
        html5: Some(b"\xe2\x8b\xa9"),
        html4: None,
    },
    Charref {
        name: b"scpolint",
        html5: Some(b"\xe2\xa8\x93"),
        html4: None,
    },
    Charref {
        name: b"scsim",
        html5: Some(b"\xe2\x89\xbf"),
        html4: None,
    },
    Charref {
        name: b"scy",
        html5: Some(b"\xd1\x81"),
        html4: None,
    },
    Charref {
        name: b"sdot",
        html5: Some(b"\xe2\x8b\x85"),
        html4: Some(b"\xe2\x8b\x85"),
    },
    Charref {
        name: b"sdotb",
        html5: Some(b"\xe2\x8a\xa1"),
        html4: None,
    },
    Charref {
        name: b"sdote",
        html5: Some(b"\xe2\xa9\xa6"),
        html4: None,
    },
    Charref {
        name: b"seArr",
        html5: Some(b"\xe2\x87\x98"),
        html4: None,
    },
    Charref {
        name: b"searhk",
        html5: Some(b"\xe2\xa4\xa5"),
        html4: None,
    },
    Charref {
        name: b"searr",
        html5: Some(b"\xe2\x86\x98"),
        html4: None,
    },
    Charref {
        name: b"searrow",
        html5: Some(b"\xe2\x86\x98"),
        html4: None,
    },
    Charref {
        name: b"sect",
        html5: Some(b"\xc2\xa7"),
        html4: Some(b"\xc2\xa7"),
    },
    Charref {
        name: b"semi",
        html5: Some(b";"),
        html4: None,
    },
    Charref {
        name: b"seswar",
        html5: Some(b"\xe2\xa4\xa9"),
        html4: None,
    },
    Charref {
        name: b"setminus",
        html5: Some(b"\xe2\x88\x96"),
        html4: None,
    },
    Charref {
        name: b"setmn",
        html5: Some(b"\xe2\x88\x96"),
        html4: None,
    },
    Charref {
        name: b"sext",
        html5: Some(b"\xe2\x9c\xb6"),
        html4: None,
    },
    Charref {
        name: b"sfr",
        html5: Some(b"\xf0\x9d\x94\xb0"),
        html4: None,
    },
    Charref {
        name: b"sfrown",
        html5: Some(b"\xe2\x8c\xa2"),
        html4: None,
    },
    Charref {
        name: b"sharp",
        html5: Some(b"\xe2\x99\xaf"),
        html4: None,
    },
    Charref {
        name: b"shchcy",
        html5: Some(b"\xd1\x89"),
        html4: None,
    },
    Charref {
        name: b"shcy",
        html5: Some(b"\xd1\x88"),
        html4: None,
    },
    Charref {
        name: b"shortmid",
        html5: Some(b"\xe2\x88\xa3"),
        html4: None,
    },
    Charref {
        name: b"shortparallel",
        html5: Some(b"\xe2\x88\xa5"),
        html4: None,
    },
    Charref {
        name: b"shy",
        html5: Some(b"\xc2\xad"),
        html4: Some(b"\xc2\xad"),
    },
    Charref {
        name: b"sigma",
        html5: Some(b"\xcf\x83"),
        html4: Some(b"\xcf\x83"),
    },
    Charref {
        name: b"sigmaf",
        html5: Some(b"\xcf\x82"),
        html4: Some(b"\xcf\x82"),
    },
    Charref {
        name: b"sigmav",
        html5: Some(b"\xcf\x82"),
        html4: None,
    },
    Charref {
        name: b"sim",
        html5: Some(b"\xe2\x88\xbc"),
        html4: Some(b"\xe2\x88\xbc"),
    },
    Charref {
        name: b"simdot",
        html5: Some(b"\xe2\xa9\xaa"),
        html4: None,
    },
    Charref {
        name: b"sime",
        html5: Some(b"\xe2\x89\x83"),
        html4: None,
    },
    Charref {
        name: b"simeq",
        html5: Some(b"\xe2\x89\x83"),
        html4: None,
    },
    Charref {
        name: b"simg",
        html5: Some(b"\xe2\xaa\x9e"),
        html4: None,
    },
    Charref {
        name: b"simgE",
        html5: Some(b"\xe2\xaa\xa0"),
        html4: None,
    },
    Charref {
        name: b"siml",
        html5: Some(b"\xe2\xaa\x9d"),
        html4: None,
    },
    Charref {
        name: b"simlE",
        html5: Some(b"\xe2\xaa\x9f"),
        html4: None,
    },
    Charref {
        name: b"simne",
        html5: Some(b"\xe2\x89\x86"),
        html4: None,
    },
    Charref {
        name: b"simplus",
        html5: Some(b"\xe2\xa8\xa4"),
        html4: None,
    },
    Charref {
        name: b"simrarr",
        html5: Some(b"\xe2\xa5\xb2"),
        html4: None,
    },
    Charref {
        name: b"slarr",
        html5: Some(b"\xe2\x86\x90"),
        html4: None,
    },
    Charref {
        name: b"smallsetminus",
        html5: Some(b"\xe2\x88\x96"),
        html4: None,
    },
    Charref {
        name: b"smashp",
        html5: Some(b"\xe2\xa8\xb3"),
        html4: None,
    },
    Charref {
        name: b"smeparsl",
        html5: Some(b"\xe2\xa7\xa4"),
        html4: None,
    },
    Charref {
        name: b"smid",
        html5: Some(b"\xe2\x88\xa3"),
        html4: None,
    },
    Charref {
        name: b"smile",
        html5: Some(b"\xe2\x8c\xa3"),
        html4: None,
    },
    Charref {
        name: b"smt",
        html5: Some(b"\xe2\xaa\xaa"),
        html4: None,
    },
    Charref {
        name: b"smte",
        html5: Some(b"\xe2\xaa\xac"),
        html4: None,
    },
    Charref {
        name: b"smtes",
        html5: Some(b"\xe2\xaa\xac\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"softcy",
        html5: Some(b"\xd1\x8c"),
        html4: None,
    },
    Charref {
        name: b"sol",
        html5: Some(b"/"),
        html4: None,
    },
    Charref {
        name: b"solb",
        html5: Some(b"\xe2\xa7\x84"),
        html4: None,
    },
    Charref {
        name: b"solbar",
        html5: Some(b"\xe2\x8c\xbf"),
        html4: None,
    },
    Charref {
        name: b"sopf",
        html5: Some(b"\xf0\x9d\x95\xa4"),
        html4: None,
    },
    Charref {
        name: b"spades",
        html5: Some(b"\xe2\x99\xa0"),
        html4: Some(b"\xe2\x99\xa0"),
    },
    Charref {
        name: b"spadesuit",
        html5: Some(b"\xe2\x99\xa0"),
        html4: None,
    },
    Charref {
        name: b"spar",
        html5: Some(b"\xe2\x88\xa5"),
        html4: None,
    },
    Charref {
        name: b"sqcap",
        html5: Some(b"\xe2\x8a\x93"),
        html4: None,
    },
    Charref {
        name: b"sqcaps",
        html5: Some(b"\xe2\x8a\x93\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"sqcup",
        html5: Some(b"\xe2\x8a\x94"),
        html4: None,
    },
    Charref {
        name: b"sqcups",
        html5: Some(b"\xe2\x8a\x94\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"sqsub",
        html5: Some(b"\xe2\x8a\x8f"),
        html4: None,
    },
    Charref {
        name: b"sqsube",
        html5: Some(b"\xe2\x8a\x91"),
        html4: None,
    },
    Charref {
        name: b"sqsubset",
        html5: Some(b"\xe2\x8a\x8f"),
        html4: None,
    },
    Charref {
        name: b"sqsubseteq",
        html5: Some(b"\xe2\x8a\x91"),
        html4: None,
    },
    Charref {
        name: b"sqsup",
        html5: Some(b"\xe2\x8a\x90"),
        html4: None,
    },
    Charref {
        name: b"sqsupe",
        html5: Some(b"\xe2\x8a\x92"),
        html4: None,
    },
    Charref {
        name: b"sqsupset",
        html5: Some(b"\xe2\x8a\x90"),
        html4: None,
    },
    Charref {
        name: b"sqsupseteq",
        html5: Some(b"\xe2\x8a\x92"),
        html4: None,
    },
    Charref {
        name: b"squ",
        html5: Some(b"\xe2\x96\xa1"),
        html4: None,
    },
    Charref {
        name: b"square",
        html5: Some(b"\xe2\x96\xa1"),
        html4: None,
    },
    Charref {
        name: b"squarf",
        html5: Some(b"\xe2\x96\xaa"),
        html4: None,
    },
    Charref {
        name: b"squf",
        html5: Some(b"\xe2\x96\xaa"),
        html4: None,
    },
    Charref {
        name: b"srarr",
        html5: Some(b"\xe2\x86\x92"),
        html4: None,
    },
    Charref {
        name: b"sscr",
        html5: Some(b"\xf0\x9d\x93\x88"),
        html4: None,
    },
    Charref {
        name: b"ssetmn",
        html5: Some(b"\xe2\x88\x96"),
        html4: None,
    },
    Charref {
        name: b"ssmile",
        html5: Some(b"\xe2\x8c\xa3"),
        html4: None,
    },
    Charref {
        name: b"sstarf",
        html5: Some(b"\xe2\x8b\x86"),
        html4: None,
    },
    Charref {
        name: b"star",
        html5: Some(b"\xe2\x98\x86"),
        html4: None,
    },
    Charref {
        name: b"starf",
        html5: Some(b"\xe2\x98\x85"),
        html4: None,
    },
    Charref {
        name: b"straightepsilon",
        html5: Some(b"\xcf\xb5"),
        html4: None,
    },
    Charref {
        name: b"straightphi",
        html5: Some(b"\xcf\x95"),
        html4: None,
    },
    Charref {
        name: b"strns",
        html5: Some(b"\xc2\xaf"),
        html4: None,
    },
    Charref {
        name: b"sub",
        html5: Some(b"\xe2\x8a\x82"),
        html4: Some(b"\xe2\x8a\x82"),
    },
    Charref {
        name: b"subE",
        html5: Some(b"\xe2\xab\x85"),
        html4: None,
    },
    Charref {
        name: b"subdot",
        html5: Some(b"\xe2\xaa\xbd"),
        html4: None,
    },
    Charref {
        name: b"sube",
        html5: Some(b"\xe2\x8a\x86"),
        html4: Some(b"\xe2\x8a\x86"),
    },
    Charref {
        name: b"subedot",
        html5: Some(b"\xe2\xab\x83"),
        html4: None,
    },
    Charref {
        name: b"submult",
        html5: Some(b"\xe2\xab\x81"),
        html4: None,
    },
    Charref {
        name: b"subnE",
        html5: Some(b"\xe2\xab\x8b"),
        html4: None,
    },
    Charref {
        name: b"subne",
        html5: Some(b"\xe2\x8a\x8a"),
        html4: None,
    },
    Charref {
        name: b"subplus",
        html5: Some(b"\xe2\xaa\xbf"),
        html4: None,
    },
    Charref {
        name: b"subrarr",
        html5: Some(b"\xe2\xa5\xb9"),
        html4: None,
    },
    Charref {
        name: b"subset",
        html5: Some(b"\xe2\x8a\x82"),
        html4: None,
    },
    Charref {
        name: b"subseteq",
        html5: Some(b"\xe2\x8a\x86"),
        html4: None,
    },
    Charref {
        name: b"subseteqq",
        html5: Some(b"\xe2\xab\x85"),
        html4: None,
    },
    Charref {
        name: b"subsetneq",
        html5: Some(b"\xe2\x8a\x8a"),
        html4: None,
    },
    Charref {
        name: b"subsetneqq",
        html5: Some(b"\xe2\xab\x8b"),
        html4: None,
    },
    Charref {
        name: b"subsim",
        html5: Some(b"\xe2\xab\x87"),
        html4: None,
    },
    Charref {
        name: b"subsub",
        html5: Some(b"\xe2\xab\x95"),
        html4: None,
    },
    Charref {
        name: b"subsup",
        html5: Some(b"\xe2\xab\x93"),
        html4: None,
    },
    Charref {
        name: b"succ",
        html5: Some(b"\xe2\x89\xbb"),
        html4: None,
    },
    Charref {
        name: b"succapprox",
        html5: Some(b"\xe2\xaa\xb8"),
        html4: None,
    },
    Charref {
        name: b"succcurlyeq",
        html5: Some(b"\xe2\x89\xbd"),
        html4: None,
    },
    Charref {
        name: b"succeq",
        html5: Some(b"\xe2\xaa\xb0"),
        html4: None,
    },
    Charref {
        name: b"succnapprox",
        html5: Some(b"\xe2\xaa\xba"),
        html4: None,
    },
    Charref {
        name: b"succneqq",
        html5: Some(b"\xe2\xaa\xb6"),
        html4: None,
    },
    Charref {
        name: b"succnsim",
        html5: Some(b"\xe2\x8b\xa9"),
        html4: None,
    },
    Charref {
        name: b"succsim",
        html5: Some(b"\xe2\x89\xbf"),
        html4: None,
    },
    Charref {
        name: b"sum",
        html5: Some(b"\xe2\x88\x91"),
        html4: Some(b"\xe2\x88\x91"),
    },
    Charref {
        name: b"sung",
        html5: Some(b"\xe2\x99\xaa"),
        html4: None,
    },
    Charref {
        name: b"sup",
        html5: Some(b"\xe2\x8a\x83"),
        html4: Some(b"\xe2\x8a\x83"),
    },
    Charref {
        name: b"sup1",
        html5: Some(b"\xc2\xb9"),
        html4: Some(b"\xc2\xb9"),
    },
    Charref {
        name: b"sup2",
        html5: Some(b"\xc2\xb2"),
        html4: Some(b"\xc2\xb2"),
    },
    Charref {
        name: b"sup3",
        html5: Some(b"\xc2\xb3"),
        html4: Some(b"\xc2\xb3"),
    },
    Charref {
        name: b"supE",
        html5: Some(b"\xe2\xab\x86"),
        html4: None,
    },
    Charref {
        name: b"supdot",
        html5: Some(b"\xe2\xaa\xbe"),
        html4: None,
    },
    Charref {
        name: b"supdsub",
        html5: Some(b"\xe2\xab\x98"),
        html4: None,
    },
    Charref {
        name: b"supe",
        html5: Some(b"\xe2\x8a\x87"),
        html4: Some(b"\xe2\x8a\x87"),
    },
    Charref {
        name: b"supedot",
        html5: Some(b"\xe2\xab\x84"),
        html4: None,
    },
    Charref {
        name: b"suphsol",
        html5: Some(b"\xe2\x9f\x89"),
        html4: None,
    },
    Charref {
        name: b"suphsub",
        html5: Some(b"\xe2\xab\x97"),
        html4: None,
    },
    Charref {
        name: b"suplarr",
        html5: Some(b"\xe2\xa5\xbb"),
        html4: None,
    },
    Charref {
        name: b"supmult",
        html5: Some(b"\xe2\xab\x82"),
        html4: None,
    },
    Charref {
        name: b"supnE",
        html5: Some(b"\xe2\xab\x8c"),
        html4: None,
    },
    Charref {
        name: b"supne",
        html5: Some(b"\xe2\x8a\x8b"),
        html4: None,
    },
    Charref {
        name: b"supplus",
        html5: Some(b"\xe2\xab\x80"),
        html4: None,
    },
    Charref {
        name: b"supset",
        html5: Some(b"\xe2\x8a\x83"),
        html4: None,
    },
    Charref {
        name: b"supseteq",
        html5: Some(b"\xe2\x8a\x87"),
        html4: None,
    },
    Charref {
        name: b"supseteqq",
        html5: Some(b"\xe2\xab\x86"),
        html4: None,
    },
    Charref {
        name: b"supsetneq",
        html5: Some(b"\xe2\x8a\x8b"),
        html4: None,
    },
    Charref {
        name: b"supsetneqq",
        html5: Some(b"\xe2\xab\x8c"),
        html4: None,
    },
    Charref {
        name: b"supsim",
        html5: Some(b"\xe2\xab\x88"),
        html4: None,
    },
    Charref {
        name: b"supsub",
        html5: Some(b"\xe2\xab\x94"),
        html4: None,
    },
    Charref {
        name: b"supsup",
        html5: Some(b"\xe2\xab\x96"),
        html4: None,
    },
    Charref {
        name: b"swArr",
        html5: Some(b"\xe2\x87\x99"),
        html4: None,
    },
    Charref {
        name: b"swarhk",
        html5: Some(b"\xe2\xa4\xa6"),
        html4: None,
    },
    Charref {
        name: b"swarr",
        html5: Some(b"\xe2\x86\x99"),
        html4: None,
    },
    Charref {
        name: b"swarrow",
        html5: Some(b"\xe2\x86\x99"),
        html4: None,
    },
    Charref {
        name: b"swnwar",
        html5: Some(b"\xe2\xa4\xaa"),
        html4: None,
    },
    Charref {
        name: b"szlig",
        html5: Some(b"\xc3\x9f"),
        html4: Some(b"\xc3\x9f"),
    },
    Charref {
        name: b"target",
        html5: Some(b"\xe2\x8c\x96"),
        html4: None,
    },
    Charref {
        name: b"tau",
        html5: Some(b"\xcf\x84"),
        html4: Some(b"\xcf\x84"),
    },
    Charref {
        name: b"tbrk",
        html5: Some(b"\xe2\x8e\xb4"),
        html4: None,
    },
    Charref {
        name: b"tcaron",
        html5: Some(b"\xc5\xa5"),
        html4: None,
    },
    Charref {
        name: b"tcedil",
        html5: Some(b"\xc5\xa3"),
        html4: None,
    },
    Charref {
        name: b"tcy",
        html5: Some(b"\xd1\x82"),
        html4: None,
    },
    Charref {
        name: b"tdot",
        html5: Some(b"\xe2\x83\x9b"),
        html4: None,
    },
    Charref {
        name: b"telrec",
        html5: Some(b"\xe2\x8c\x95"),
        html4: None,
    },
    Charref {
        name: b"tfr",
        html5: Some(b"\xf0\x9d\x94\xb1"),
        html4: None,
    },
    Charref {
        name: b"there4",
        html5: Some(b"\xe2\x88\xb4"),
        html4: Some(b"\xe2\x88\xb4"),
    },
    Charref {
        name: b"therefore",
        html5: Some(b"\xe2\x88\xb4"),
        html4: None,
    },
    Charref {
        name: b"theta",
        html5: Some(b"\xce\xb8"),
        html4: Some(b"\xce\xb8"),
    },
    Charref {
        name: b"thetasym",
        html5: Some(b"\xcf\x91"),
        html4: Some(b"\xcf\x91"),
    },
    Charref {
        name: b"thetav",
        html5: Some(b"\xcf\x91"),
        html4: None,
    },
    Charref {
        name: b"thickapprox",
        html5: Some(b"\xe2\x89\x88"),
        html4: None,
    },
    Charref {
        name: b"thicksim",
        html5: Some(b"\xe2\x88\xbc"),
        html4: None,
    },
    Charref {
        name: b"thinsp",
        html5: Some(b"\xe2\x80\x89"),
        html4: Some(b"\xe2\x80\x89"),
    },
    Charref {
        name: b"thkap",
        html5: Some(b"\xe2\x89\x88"),
        html4: None,
    },
    Charref {
        name: b"thksim",
        html5: Some(b"\xe2\x88\xbc"),
        html4: None,
    },
    Charref {
        name: b"thorn",
        html5: Some(b"\xc3\xbe"),
        html4: Some(b"\xc3\xbe"),
    },
    Charref {
        name: b"tilde",
        html5: Some(b"\xcb\x9c"),
        html4: Some(b"\xcb\x9c"),
    },
    Charref {
        name: b"times",
        html5: Some(b"\xc3\x97"),
        html4: Some(b"\xc3\x97"),
    },
    Charref {
        name: b"timesb",
        html5: Some(b"\xe2\x8a\xa0"),
        html4: None,
    },
    Charref {
        name: b"timesbar",
        html5: Some(b"\xe2\xa8\xb1"),
        html4: None,
    },
    Charref {
        name: b"timesd",
        html5: Some(b"\xe2\xa8\xb0"),
        html4: None,
    },
    Charref {
        name: b"tint",
        html5: Some(b"\xe2\x88\xad"),
        html4: None,
    },
    Charref {
        name: b"toea",
        html5: Some(b"\xe2\xa4\xa8"),
        html4: None,
    },
    Charref {
        name: b"top",
        html5: Some(b"\xe2\x8a\xa4"),
        html4: None,
    },
    Charref {
        name: b"topbot",
        html5: Some(b"\xe2\x8c\xb6"),
        html4: None,
    },
    Charref {
        name: b"topcir",
        html5: Some(b"\xe2\xab\xb1"),
        html4: None,
    },
    Charref {
        name: b"topf",
        html5: Some(b"\xf0\x9d\x95\xa5"),
        html4: None,
    },
    Charref {
        name: b"topfork",
        html5: Some(b"\xe2\xab\x9a"),
        html4: None,
    },
    Charref {
        name: b"tosa",
        html5: Some(b"\xe2\xa4\xa9"),
        html4: None,
    },
    Charref {
        name: b"tprime",
        html5: Some(b"\xe2\x80\xb4"),
        html4: None,
    },
    Charref {
        name: b"trade",
        html5: Some(b"\xe2\x84\xa2"),
        html4: Some(b"\xe2\x84\xa2"),
    },
    Charref {
        name: b"triangle",
        html5: Some(b"\xe2\x96\xb5"),
        html4: None,
    },
    Charref {
        name: b"triangledown",
        html5: Some(b"\xe2\x96\xbf"),
        html4: None,
    },
    Charref {
        name: b"triangleleft",
        html5: Some(b"\xe2\x97\x83"),
        html4: None,
    },
    Charref {
        name: b"trianglelefteq",
        html5: Some(b"\xe2\x8a\xb4"),
        html4: None,
    },
    Charref {
        name: b"triangleq",
        html5: Some(b"\xe2\x89\x9c"),
        html4: None,
    },
    Charref {
        name: b"triangleright",
        html5: Some(b"\xe2\x96\xb9"),
        html4: None,
    },
    Charref {
        name: b"trianglerighteq",
        html5: Some(b"\xe2\x8a\xb5"),
        html4: None,
    },
    Charref {
        name: b"tridot",
        html5: Some(b"\xe2\x97\xac"),
        html4: None,
    },
    Charref {
        name: b"trie",
        html5: Some(b"\xe2\x89\x9c"),
        html4: None,
    },
    Charref {
        name: b"triminus",
        html5: Some(b"\xe2\xa8\xba"),
        html4: None,
    },
    Charref {
        name: b"triplus",
        html5: Some(b"\xe2\xa8\xb9"),
        html4: None,
    },
    Charref {
        name: b"trisb",
        html5: Some(b"\xe2\xa7\x8d"),
        html4: None,
    },
    Charref {
        name: b"tritime",
        html5: Some(b"\xe2\xa8\xbb"),
        html4: None,
    },
    Charref {
        name: b"trpezium",
        html5: Some(b"\xe2\x8f\xa2"),
        html4: None,
    },
    Charref {
        name: b"tscr",
        html5: Some(b"\xf0\x9d\x93\x89"),
        html4: None,
    },
    Charref {
        name: b"tscy",
        html5: Some(b"\xd1\x86"),
        html4: None,
    },
    Charref {
        name: b"tshcy",
        html5: Some(b"\xd1\x9b"),
        html4: None,
    },
    Charref {
        name: b"tstrok",
        html5: Some(b"\xc5\xa7"),
        html4: None,
    },
    Charref {
        name: b"twixt",
        html5: Some(b"\xe2\x89\xac"),
        html4: None,
    },
    Charref {
        name: b"twoheadleftarrow",
        html5: Some(b"\xe2\x86\x9e"),
        html4: None,
    },
    Charref {
        name: b"twoheadrightarrow",
        html5: Some(b"\xe2\x86\xa0"),
        html4: None,
    },
    Charref {
        name: b"uArr",
        html5: Some(b"\xe2\x87\x91"),
        html4: Some(b"\xe2\x87\x91"),
    },
    Charref {
        name: b"uHar",
        html5: Some(b"\xe2\xa5\xa3"),
        html4: None,
    },
    Charref {
        name: b"uacute",
        html5: Some(b"\xc3\xba"),
        html4: Some(b"\xc3\xba"),
    },
    Charref {
        name: b"uarr",
        html5: Some(b"\xe2\x86\x91"),
        html4: Some(b"\xe2\x86\x91"),
    },
    Charref {
        name: b"ubrcy",
        html5: Some(b"\xd1\x9e"),
        html4: None,
    },
    Charref {
        name: b"ubreve",
        html5: Some(b"\xc5\xad"),
        html4: None,
    },
    Charref {
        name: b"ucirc",
        html5: Some(b"\xc3\xbb"),
        html4: Some(b"\xc3\xbb"),
    },
    Charref {
        name: b"ucy",
        html5: Some(b"\xd1\x83"),
        html4: None,
    },
    Charref {
        name: b"udarr",
        html5: Some(b"\xe2\x87\x85"),
        html4: None,
    },
    Charref {
        name: b"udblac",
        html5: Some(b"\xc5\xb1"),
        html4: None,
    },
    Charref {
        name: b"udhar",
        html5: Some(b"\xe2\xa5\xae"),
        html4: None,
    },
    Charref {
        name: b"ufisht",
        html5: Some(b"\xe2\xa5\xbe"),
        html4: None,
    },
    Charref {
        name: b"ufr",
        html5: Some(b"\xf0\x9d\x94\xb2"),
        html4: None,
    },
    Charref {
        name: b"ugrave",
        html5: Some(b"\xc3\xb9"),
        html4: Some(b"\xc3\xb9"),
    },
    Charref {
        name: b"uharl",
        html5: Some(b"\xe2\x86\xbf"),
        html4: None,
    },
    Charref {
        name: b"uharr",
        html5: Some(b"\xe2\x86\xbe"),
        html4: None,
    },
    Charref {
        name: b"uhblk",
        html5: Some(b"\xe2\x96\x80"),
        html4: None,
    },
    Charref {
        name: b"ulcorn",
        html5: Some(b"\xe2\x8c\x9c"),
        html4: None,
    },
    Charref {
        name: b"ulcorner",
        html5: Some(b"\xe2\x8c\x9c"),
        html4: None,
    },
    Charref {
        name: b"ulcrop",
        html5: Some(b"\xe2\x8c\x8f"),
        html4: None,
    },
    Charref {
        name: b"ultri",
        html5: Some(b"\xe2\x97\xb8"),
        html4: None,
    },
    Charref {
        name: b"umacr",
        html5: Some(b"\xc5\xab"),
        html4: None,
    },
    Charref {
        name: b"uml",
        html5: Some(b"\xc2\xa8"),
        html4: Some(b"\xc2\xa8"),
    },
    Charref {
        name: b"uogon",
        html5: Some(b"\xc5\xb3"),
        html4: None,
    },
    Charref {
        name: b"uopf",
        html5: Some(b"\xf0\x9d\x95\xa6"),
        html4: None,
    },
    Charref {
        name: b"uparrow",
        html5: Some(b"\xe2\x86\x91"),
        html4: None,
    },
    Charref {
        name: b"updownarrow",
        html5: Some(b"\xe2\x86\x95"),
        html4: None,
    },
    Charref {
        name: b"upharpoonleft",
        html5: Some(b"\xe2\x86\xbf"),
        html4: None,
    },
    Charref {
        name: b"upharpoonright",
        html5: Some(b"\xe2\x86\xbe"),
        html4: None,
    },
    Charref {
        name: b"uplus",
        html5: Some(b"\xe2\x8a\x8e"),
        html4: None,
    },
    Charref {
        name: b"upsi",
        html5: Some(b"\xcf\x85"),
        html4: None,
    },
    Charref {
        name: b"upsih",
        html5: Some(b"\xcf\x92"),
        html4: Some(b"\xcf\x92"),
    },
    Charref {
        name: b"upsilon",
        html5: Some(b"\xcf\x85"),
        html4: Some(b"\xcf\x85"),
    },
    Charref {
        name: b"upuparrows",
        html5: Some(b"\xe2\x87\x88"),
        html4: None,
    },
    Charref {
        name: b"urcorn",
        html5: Some(b"\xe2\x8c\x9d"),
        html4: None,
    },
    Charref {
        name: b"urcorner",
        html5: Some(b"\xe2\x8c\x9d"),
        html4: None,
    },
    Charref {
        name: b"urcrop",
        html5: Some(b"\xe2\x8c\x8e"),
        html4: None,
    },
    Charref {
        name: b"uring",
        html5: Some(b"\xc5\xaf"),
        html4: None,
    },
    Charref {
        name: b"urtri",
        html5: Some(b"\xe2\x97\xb9"),
        html4: None,
    },
    Charref {
        name: b"uscr",
        html5: Some(b"\xf0\x9d\x93\x8a"),
        html4: None,
    },
    Charref {
        name: b"utdot",
        html5: Some(b"\xe2\x8b\xb0"),
        html4: None,
    },
    Charref {
        name: b"utilde",
        html5: Some(b"\xc5\xa9"),
        html4: None,
    },
    Charref {
        name: b"utri",
        html5: Some(b"\xe2\x96\xb5"),
        html4: None,
    },
    Charref {
        name: b"utrif",
        html5: Some(b"\xe2\x96\xb4"),
        html4: None,
    },
    Charref {
        name: b"uuarr",
        html5: Some(b"\xe2\x87\x88"),
        html4: None,
    },
    Charref {
        name: b"uuml",
        html5: Some(b"\xc3\xbc"),
        html4: Some(b"\xc3\xbc"),
    },
    Charref {
        name: b"uwangle",
        html5: Some(b"\xe2\xa6\xa7"),
        html4: None,
    },
    Charref {
        name: b"vArr",
        html5: Some(b"\xe2\x87\x95"),
        html4: None,
    },
    Charref {
        name: b"vBar",
        html5: Some(b"\xe2\xab\xa8"),
        html4: None,
    },
    Charref {
        name: b"vBarv",
        html5: Some(b"\xe2\xab\xa9"),
        html4: None,
    },
    Charref {
        name: b"vDash",
        html5: Some(b"\xe2\x8a\xa8"),
        html4: None,
    },
    Charref {
        name: b"vangrt",
        html5: Some(b"\xe2\xa6\x9c"),
        html4: None,
    },
    Charref {
        name: b"varepsilon",
        html5: Some(b"\xcf\xb5"),
        html4: None,
    },
    Charref {
        name: b"varkappa",
        html5: Some(b"\xcf\xb0"),
        html4: None,
    },
    Charref {
        name: b"varnothing",
        html5: Some(b"\xe2\x88\x85"),
        html4: None,
    },
    Charref {
        name: b"varphi",
        html5: Some(b"\xcf\x95"),
        html4: None,
    },
    Charref {
        name: b"varpi",
        html5: Some(b"\xcf\x96"),
        html4: None,
    },
    Charref {
        name: b"varpropto",
        html5: Some(b"\xe2\x88\x9d"),
        html4: None,
    },
    Charref {
        name: b"varr",
        html5: Some(b"\xe2\x86\x95"),
        html4: None,
    },
    Charref {
        name: b"varrho",
        html5: Some(b"\xcf\xb1"),
        html4: None,
    },
    Charref {
        name: b"varsigma",
        html5: Some(b"\xcf\x82"),
        html4: None,
    },
    Charref {
        name: b"varsubsetneq",
        html5: Some(b"\xe2\x8a\x8a\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"varsubsetneqq",
        html5: Some(b"\xe2\xab\x8b\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"varsupsetneq",
        html5: Some(b"\xe2\x8a\x8b\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"varsupsetneqq",
        html5: Some(b"\xe2\xab\x8c\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"vartheta",
        html5: Some(b"\xcf\x91"),
        html4: None,
    },
    Charref {
        name: b"vartriangleleft",
        html5: Some(b"\xe2\x8a\xb2"),
        html4: None,
    },
    Charref {
        name: b"vartriangleright",
        html5: Some(b"\xe2\x8a\xb3"),
        html4: None,
    },
    Charref {
        name: b"vcy",
        html5: Some(b"\xd0\xb2"),
        html4: None,
    },
    Charref {
        name: b"vdash",
        html5: Some(b"\xe2\x8a\xa2"),
        html4: None,
    },
    Charref {
        name: b"vee",
        html5: Some(b"\xe2\x88\xa8"),
        html4: None,
    },
    Charref {
        name: b"veebar",
        html5: Some(b"\xe2\x8a\xbb"),
        html4: None,
    },
    Charref {
        name: b"veeeq",
        html5: Some(b"\xe2\x89\x9a"),
        html4: None,
    },
    Charref {
        name: b"vellip",
        html5: Some(b"\xe2\x8b\xae"),
        html4: None,
    },
    Charref {
        name: b"verbar",
        html5: Some(b"|"),
        html4: None,
    },
    Charref {
        name: b"vert",
        html5: Some(b"|"),
        html4: None,
    },
    Charref {
        name: b"vfr",
        html5: Some(b"\xf0\x9d\x94\xb3"),
        html4: None,
    },
    Charref {
        name: b"vltri",
        html5: Some(b"\xe2\x8a\xb2"),
        html4: None,
    },
    Charref {
        name: b"vnsub",
        html5: Some(b"\xe2\x8a\x82\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"vnsup",
        html5: Some(b"\xe2\x8a\x83\xe2\x83\x92"),
        html4: None,
    },
    Charref {
        name: b"vopf",
        html5: Some(b"\xf0\x9d\x95\xa7"),
        html4: None,
    },
    Charref {
        name: b"vprop",
        html5: Some(b"\xe2\x88\x9d"),
        html4: None,
    },
    Charref {
        name: b"vrtri",
        html5: Some(b"\xe2\x8a\xb3"),
        html4: None,
    },
    Charref {
        name: b"vscr",
        html5: Some(b"\xf0\x9d\x93\x8b"),
        html4: None,
    },
    Charref {
        name: b"vsubnE",
        html5: Some(b"\xe2\xab\x8b\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"vsubne",
        html5: Some(b"\xe2\x8a\x8a\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"vsupnE",
        html5: Some(b"\xe2\xab\x8c\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"vsupne",
        html5: Some(b"\xe2\x8a\x8b\xef\xb8\x80"),
        html4: None,
    },
    Charref {
        name: b"vzigzag",
        html5: Some(b"\xe2\xa6\x9a"),
        html4: None,
    },
    Charref {
        name: b"wcirc",
        html5: Some(b"\xc5\xb5"),
        html4: None,
    },
    Charref {
        name: b"wedbar",
        html5: Some(b"\xe2\xa9\x9f"),
        html4: None,
    },
    Charref {
        name: b"wedge",
        html5: Some(b"\xe2\x88\xa7"),
        html4: None,
    },
    Charref {
        name: b"wedgeq",
        html5: Some(b"\xe2\x89\x99"),
        html4: None,
    },
    Charref {
        name: b"weierp",
        html5: Some(b"\xe2\x84\x98"),
        html4: Some(b"\xe2\x84\x98"),
    },
    Charref {
        name: b"wfr",
        html5: Some(b"\xf0\x9d\x94\xb4"),
        html4: None,
    },
    Charref {
        name: b"wopf",
        html5: Some(b"\xf0\x9d\x95\xa8"),
        html4: None,
    },
    Charref {
        name: b"wp",
        html5: Some(b"\xe2\x84\x98"),
        html4: None,
    },
    Charref {
        name: b"wr",
        html5: Some(b"\xe2\x89\x80"),
        html4: None,
    },
    Charref {
        name: b"wreath",
        html5: Some(b"\xe2\x89\x80"),
        html4: None,
    },
    Charref {
        name: b"wscr",
        html5: Some(b"\xf0\x9d\x93\x8c"),
        html4: None,
    },
    Charref {
        name: b"xcap",
        html5: Some(b"\xe2\x8b\x82"),
        html4: None,
    },
    Charref {
        name: b"xcirc",
        html5: Some(b"\xe2\x97\xaf"),
        html4: None,
    },
    Charref {
        name: b"xcup",
        html5: Some(b"\xe2\x8b\x83"),
        html4: None,
    },
    Charref {
        name: b"xdtri",
        html5: Some(b"\xe2\x96\xbd"),
        html4: None,
    },
    Charref {
        name: b"xfr",
        html5: Some(b"\xf0\x9d\x94\xb5"),
        html4: None,
    },
    Charref {
        name: b"xhArr",
        html5: Some(b"\xe2\x9f\xba"),
        html4: None,
    },
    Charref {
        name: b"xharr",
        html5: Some(b"\xe2\x9f\xb7"),
        html4: None,
    },
    Charref {
        name: b"xi",
        html5: Some(b"\xce\xbe"),
        html4: Some(b"\xce\xbe"),
    },
    Charref {
        name: b"xlArr",
        html5: Some(b"\xe2\x9f\xb8"),
        html4: None,
    },
    Charref {
        name: b"xlarr",
        html5: Some(b"\xe2\x9f\xb5"),
        html4: None,
    },
    Charref {
        name: b"xmap",
        html5: Some(b"\xe2\x9f\xbc"),
        html4: None,
    },
    Charref {
        name: b"xnis",
        html5: Some(b"\xe2\x8b\xbb"),
        html4: None,
    },
    Charref {
        name: b"xodot",
        html5: Some(b"\xe2\xa8\x80"),
        html4: None,
    },
    Charref {
        name: b"xopf",
        html5: Some(b"\xf0\x9d\x95\xa9"),
        html4: None,
    },
    Charref {
        name: b"xoplus",
        html5: Some(b"\xe2\xa8\x81"),
        html4: None,
    },
    Charref {
        name: b"xotime",
        html5: Some(b"\xe2\xa8\x82"),
        html4: None,
    },
    Charref {
        name: b"xrArr",
        html5: Some(b"\xe2\x9f\xb9"),
        html4: None,
    },
    Charref {
        name: b"xrarr",
        html5: Some(b"\xe2\x9f\xb6"),
        html4: None,
    },
    Charref {
        name: b"xscr",
        html5: Some(b"\xf0\x9d\x93\x8d"),
        html4: None,
    },
    Charref {
        name: b"xsqcup",
        html5: Some(b"\xe2\xa8\x86"),
        html4: None,
    },
    Charref {
        name: b"xuplus",
        html5: Some(b"\xe2\xa8\x84"),
        html4: None,
    },
    Charref {
        name: b"xutri",
        html5: Some(b"\xe2\x96\xb3"),
        html4: None,
    },
    Charref {
        name: b"xvee",
        html5: Some(b"\xe2\x8b\x81"),
        html4: None,
    },
    Charref {
        name: b"xwedge",
        html5: Some(b"\xe2\x8b\x80"),
        html4: None,
    },
    Charref {
        name: b"yacute",
        html5: Some(b"\xc3\xbd"),
        html4: Some(b"\xc3\xbd"),
    },
    Charref {
        name: b"yacy",
        html5: Some(b"\xd1\x8f"),
        html4: None,
    },
    Charref {
        name: b"ycirc",
        html5: Some(b"\xc5\xb7"),
        html4: None,
    },
    Charref {
        name: b"ycy",
        html5: Some(b"\xd1\x8b"),
        html4: None,
    },
    Charref {
        name: b"yen",
        html5: Some(b"\xc2\xa5"),
        html4: Some(b"\xc2\xa5"),
    },
    Charref {
        name: b"yfr",
        html5: Some(b"\xf0\x9d\x94\xb6"),
        html4: None,
    },
    Charref {
        name: b"yicy",
        html5: Some(b"\xd1\x97"),
        html4: None,
    },
    Charref {
        name: b"yopf",
        html5: Some(b"\xf0\x9d\x95\xaa"),
        html4: None,
    },
    Charref {
        name: b"yscr",
        html5: Some(b"\xf0\x9d\x93\x8e"),
        html4: None,
    },
    Charref {
        name: b"yucy",
        html5: Some(b"\xd1\x8e"),
        html4: None,
    },
    Charref {
        name: b"yuml",
        html5: Some(b"\xc3\xbf"),
        html4: Some(b"\xc3\xbf"),
    },
    Charref {
        name: b"zacute",
        html5: Some(b"\xc5\xba"),
        html4: None,
    },
    Charref {
        name: b"zcaron",
        html5: Some(b"\xc5\xbe"),
        html4: None,
    },
    Charref {
        name: b"zcy",
        html5: Some(b"\xd0\xb7"),
        html4: None,
    },
    Charref {
        name: b"zdot",
        html5: Some(b"\xc5\xbc"),
        html4: None,
    },
    Charref {
        name: b"zeetrf",
        html5: Some(b"\xe2\x84\xa8"),
        html4: None,
    },
    Charref {
        name: b"zeta",
        html5: Some(b"\xce\xb6"),
        html4: Some(b"\xce\xb6"),
    },
    Charref {
        name: b"zfr",
        html5: Some(b"\xf0\x9d\x94\xb7"),
        html4: None,
    },
    Charref {
        name: b"zhcy",
        html5: Some(b"\xd0\xb6"),
        html4: None,
    },
    Charref {
        name: b"zigrarr",
        html5: Some(b"\xe2\x87\x9d"),
        html4: None,
    },
    Charref {
        name: b"zopf",
        html5: Some(b"\xf0\x9d\x95\xab"),
        html4: None,
    },
    Charref {
        name: b"zscr",
        html5: Some(b"\xf0\x9d\x93\x8f"),
        html4: None,
    },
    Charref {
        name: b"zwj",
        html5: Some(b"\xe2\x80\x8d"),
        html4: Some(b"\xe2\x80\x8d"),
    },
    Charref {
        name: b"zwnj",
        html5: Some(b"\xe2\x80\x8c"),
        html4: Some(b"\xe2\x80\x8c"),
    },
];
