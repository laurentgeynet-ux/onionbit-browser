#![allow(clippy::cast_possible_truncation)] // `int`/`char` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::too_many_lines)] // tokenizer verbatim monolithique.

//! `cssparser.cc` + `cssparser.hh` — tokenizer et parseur CSS tolérant.
//!
//! Verbatim préservé :
//! - `tval` capé à 255 caractères (`maxStrLen-1`), troncation silencieuse.
//! - Whitespace/commentaires `/*` (non terminé toléré), `<!--`, `-->`
//!   ignorés ; `spaceSeparated` reflète la présence d'espace avant le
//!   token (utilisé par les sélecteurs et `CSS_TYPE_SYMBOL`).
//! - `getChar`/`ungetChar` : `bufptr` avance même en fin de buffer.
//! - Nombres négatifs : `-` préfixe uniquement devant chiffre/point ;
//!   un `-` isolé retombe en symbole.
//! - Strings `'…'`/`"…"` : `\x` hex ≤4 digits converti, `\c` littéral,
//!   non terminé toléré.
//! - `#hex` = `CSS_TK_COLOR` seulement `withinBlock` (sélecteur sinon).
//! - Types de valeur, unités (`cm`/`in`/`pt`/`pc`→mm), `%`,
//!   `rgb(r,g,b)` (mix %/entiers interdit), noms de couleurs via
//!   `a_Color_parse`, `!important`, shorthands (dont le bloc `font`
//!   non implémenté amont qui retombe sur `MULTIPLE`).
//! - Récupération : `parseDeclaration` saute jusqu'à `;`/`}`/END ;
//!   `ignoreBlock`/`ignoreStatement` comptent les accolades.
//! - `parse` : `@import` admis tant qu'aucun ruleset n'a été vu
//!   (`importsAreAllowed`), `@media` filtré `all`/`screen`, autres
//!   `@…` ignorés.
//!
//! Seams du port :
//! - `DilloHtml*` → [`CssHtmlHooks`] (`a_Html_tag_index`,
//!   `a_Html_url_new`, `a_Html_load_stylesheet`).
//! - `baseUrl` : seule sa chaîne (`a_Url_str`) est lue amont →
//!   `Option<Vec<u8>>`.
//! - `context` peut être `None` (≙ `NULL` dans
//!   `parseDeclarationBlock`).
//! - Les octets sont non signés (`u8`) : amont lit `char` signé —
//!   pour l'ASCII les classifications `dIs*` sont identiques.

use std::rc::Rc;

use crate::css::{
    css_create_length, Combinator, CssBackgroundPosition, CssContext, CssLengthType, CssOrigin,
    CssPrimaryOrder, CssPropertyList, CssPropertyName, CssPropertyValue, CssSelector,
    CssShorthandType, CssValueType, SelectType, CSS_NUM_PARSED_PROPERTIES, CSS_PROPERTY_END,
    CSS_PROPERTY_INFO, CSS_SHORTHAND_INFO,
};
use dillo_common::colors::color_parse;
use dillo_common::dlib::{is_alnum, is_alpha, is_digit, is_space, is_xdigit, str_ascii_casecmp};
use dillo_net::url::{url_new, DilloUrl};

/// `maxStrLen` — taille du buffer token amont (NUL inclus).
const MAX_STR_LEN: usize = 256;
/// `EOF`.
const EOF: i32 = -1;

/// `CssTokenType` — verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CssTokenType {
    /// `CSS_TK_DECINT`.
    Decint,
    /// `CSS_TK_FLOAT`.
    Float,
    /// `CSS_TK_COLOR`.
    Color,
    /// `CSS_TK_SYMBOL`.
    Symbol,
    /// `CSS_TK_STRING`.
    String,
    /// `CSS_TK_CHAR`.
    Char,
    /// `CSS_TK_END`.
    End,
}

/// Seam `a_Html_tag_index` — *fonction libre* amont (pas une méthode
/// de `DilloHtml`) : nécessaire même quand `html == NULL` (feuilles
/// UA/user). Le provider par défaut [`NullTagIndex`] ≙ table absente
/// (tout inconnu → `ELEMENT_NONE` → règle jetée, verbatim).
pub trait TagIndex {
    /// `a_Html_tag_index` — index du tag ou -1.
    fn tag_index(&mut self, name: &[u8]) -> i32;
}

/// `TagIndex` nul — tout retourne -1 (verbatim `a_Html_tag_index`
/// sur tag inconnu).
pub struct NullTagIndex;

impl TagIndex for NullTagIndex {
    fn tag_index(&mut self, _name: &[u8]) -> i32 {
        -1
    }
}

/// Seam `DilloHtml*` — opérations HTML consommées par le parseur.
pub trait CssHtmlHooks {
    /// `a_Html_url_new` — résolution `use_base` ≙ `base_url` vs
    /// `html->base_url` amont.
    fn resolve_url(
        &mut self,
        url_str: &[u8],
        base_url: Option<&[u8]>,
        use_base: bool,
    ) -> Option<DilloUrl>;
    /// `a_Html_load_stylesheet` — `Some(data)` = feuille en cache :
    /// le parser la parse **inline** (≙ récursion amont
    /// `html->styleEngine->parse` dans `a_Html_load_stylesheet` —
    /// les règles s'insèrent à la position exacte du `@import`) ;
    /// `None` = feuille absente du cache — l'impl a lancé le fetch
    /// réseau (repush ultérieur, verbatim).
    fn load_stylesheet(&mut self, url: DilloUrl) -> Option<Vec<u8>>;
}

/// `strtol(tval, NULL, 10)` — whitespace, signe, digits, saturé i64.
fn strtol(s: &[u8]) -> i64 {
    let mut i = 0;
    while i < s.len() && is_space(s[i]) {
        i += 1;
    }
    let neg = if i < s.len() && (s[i] == b'-' || s[i] == b'+') {
        let n = s[i] == b'-';
        i += 1;
        n
    } else {
        false
    };
    let mut v: i64 = 0;
    while i < s.len() && is_digit(s[i]) {
        v = v.saturating_mul(10).saturating_add(i64::from(s[i] - b'0'));
        i += 1;
    }
    if neg {
        -v
    } else {
        v
    }
}

/// `atof(tval)` — préfixe flottant tolérant, `0.0` si rien.
fn atof(s: &[u8]) -> f64 {
    let mut i = 0;
    while i < s.len() && is_space(s[i]) {
        i += 1;
    }
    let start = i;
    if i < s.len() && (s[i] == b'-' || s[i] == b'+') {
        i += 1;
    }
    let mut any = false;
    while i < s.len() && is_digit(s[i]) {
        i += 1;
        any = true;
    }
    if i < s.len() && s[i] == b'.' {
        i += 1;
        while i < s.len() && is_digit(s[i]) {
            i += 1;
            any = true;
        }
    }
    if !any {
        return 0.0;
    }
    // exposant e/E éventuel
    if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
        let save = i;
        i += 1;
        if i < s.len() && (s[i] == b'-' || s[i] == b'+') {
            i += 1;
        }
        let mut any_e = false;
        while i < s.len() && is_digit(s[i]) {
            i += 1;
            any_e = true;
        }
        if !any_e {
            i = save;
        }
    }
    std::str::from_utf8(&s[start..i])
        .ok()
        .and_then(|t| t.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// `CssParser` — verbatim.
pub struct CssParser<'a> {
    /// `a_Html_tag_index` — seam injectée (verbatim fonction libre).
    tag_index: &'a mut (dyn TagIndex + 'a),
    context: Option<&'a mut CssContext>,
    origin: CssOrigin,
    /// `a_Url_str(this->baseUrl)` — seule la chaîne est lue amont.
    base_url_str: Option<Vec<u8>>,
    buf: &'a [u8],
    /// `bufptr` — peut dépasser `buf.len()` (getChar avance toujours).
    bufptr: usize,
    ttype: CssTokenType,
    /// `tval` — ≤ `maxStrLen-1` octets (NUL omis).
    tval: Vec<u8>,
    /// `importDepth` amont vit sur `StyleEngine` ; comme la récursion
    /// est réalisée ici (le hook rend les octets), le compteur vit
    /// sur le parser — `1` à la racine, `+1` par feuille importée.
    import_depth: i32,
    within_block: bool,
    /// `spaceSeparated` — whitespace vu avant ce token.
    space_separated: bool,
}

impl<'a> CssParser<'a> {
    /// Ctor — `nextToken()` initial verbatim.
    fn new(
        context: Option<&'a mut CssContext>,
        tag_index: &'a mut (dyn TagIndex + 'a),
        origin: CssOrigin,
        base_url: Option<&DilloUrl>,
        buf: &'a [u8],
    ) -> Self {
        let mut p = CssParser {
            tag_index,
            context,
            origin,
            base_url_str: base_url.map(|u| u.clone().str_().to_vec()),
            buf,
            bufptr: 0,
            ttype: CssTokenType::Char,
            tval: Vec::new(),
            import_depth: 1,
            within_block: false,
            space_separated: false,
        };
        p.next_token();
        p
    }

    /// `getChar` — EOF passé la fin (le pointeur avance toujours,
    /// verbatim).
    fn get_char(&mut self) -> i32 {
        let c = if self.bufptr >= self.buf.len() {
            EOF
        } else {
            i32::from(self.buf[self.bufptr])
        };
        self.bufptr += 1;
        c
    }

    /// `ungetChar`.
    fn unget_char(&mut self) {
        self.bufptr -= 1;
    }

    /// `skipString` — avance si `s` suit `c` ; sinon rembobine les
    /// caractères relus (verbatim).
    fn skip_string(&mut self, mut c: i32, s: &[u8]) -> bool {
        for (n, &want) in s.iter().enumerate() {
            if n > 0 {
                c = self.get_char();
            }
            if i32::from(want) != c {
                for _ in 0..n {
                    self.unget_char();
                }
                return false;
            }
        }
        true
    }

    /// `nextToken` — verbatim.
    fn next_token(&mut self) {
        let mut c;
        self.ttype = CssTokenType::Char;
        self.space_separated = false;
        self.tval.clear();

        loop {
            c = self.get_char();
            if c >= 0 && is_space(c as u8) {
                self.space_separated = true;
            } else if self.skip_string(c, b"/*") {
                // commentaire — non terminé toléré
                loop {
                    c = self.get_char();
                    if c == EOF || self.skip_string(c, b"*/") {
                        break;
                    }
                }
            } else if self.skip_string(c, b"<!--") || self.skip_string(c, b"-->") {
                // ignore XML comment markers (verbatim : les deux
                // `else if` amont partagent le même corps)
            } else {
                break;
            }
        }

        let mut i = 0usize;

        // nombres négatifs
        if c == i32::from(b'-') {
            if i < MAX_STR_LEN - 1 {
                self.tval.push(b'-');
                i += 1;
            }
            c = self.get_char();
        }

        if c >= 0 && is_digit(c as u8) {
            self.ttype = CssTokenType::Decint;
            loop {
                if i < MAX_STR_LEN - 1 {
                    self.tval.push(c as u8);
                    i += 1;
                }
                c = self.get_char();
                if !(c >= 0 && is_digit(c as u8)) {
                    break;
                }
            }
            if c != i32::from(b'.') {
                self.unget_char();
            }
            /* …mais on continue pour voir si c'est un float */
        }

        if c == i32::from(b'.') {
            c = self.get_char();
            if c >= 0 && is_digit(c as u8) {
                self.ttype = CssTokenType::Float;
                if i < MAX_STR_LEN - 1 {
                    self.tval.push(b'.');
                    i += 1;
                }
                loop {
                    if i < MAX_STR_LEN - 1 {
                        self.tval.push(c as u8);
                        i += 1;
                    }
                    c = self.get_char();
                    if !(c >= 0 && is_digit(c as u8)) {
                        break;
                    }
                }
                self.unget_char();
                return;
            }
            self.unget_char();
            if self.ttype == CssTokenType::Decint {
                self.unget_char();
            } else {
                c = i32::from(b'.');
            }
        }

        if self.ttype == CssTokenType::Decint {
            return;
        }

        if i != 0 {
            self.unget_char(); /* ungetChar '-' */
            #[allow(unused_assignments)] // verbatim : `i--` amont
            {
                i -= 1;
            }
            c = self.get_char();
        }

        if (c >= 0 && is_alpha(c as u8)) || c == i32::from(b'_') || c == i32::from(b'-') {
            self.ttype = CssTokenType::Symbol;
            self.tval.clear();
            self.tval.push(c as u8);
            let mut j = 1usize;
            c = self.get_char();
            while (c >= 0 && is_alnum(c as u8)) || c == i32::from(b'_') || c == i32::from(b'-') {
                if j < MAX_STR_LEN - 1 {
                    self.tval.push(c as u8);
                    j += 1;
                }
                c = self.get_char();
            }
            self.unget_char();
            return;
        }

        if c == i32::from(b'"') || c == i32::from(b'\'') {
            let c1 = c;
            self.ttype = CssTokenType::String;
            self.tval.clear();
            let mut j = 0usize;
            c = self.get_char();
            while c != EOF && c != c1 {
                if c == i32::from(b'\\') {
                    let mut d = self.get_char();
                    if d >= 0 && is_xdigit(d as u8) {
                        // hex unicode ≤4 digits (verbatim 8-bit)
                        let mut hexbuf: Vec<u8> = vec![d as u8];
                        let mut jj = 1;
                        d = self.get_char();
                        while jj < 4 && d >= 0 && is_xdigit(d as u8) {
                            hexbuf.push(d as u8);
                            jj += 1;
                            d = self.get_char();
                        }
                        self.unget_char();
                        let hex = String::from_utf8_lossy(&hexbuf);
                        c = i32::from_str_radix(&hex, 16).unwrap_or(0);
                    } else {
                        c = d;
                    }
                }
                if j < MAX_STR_LEN - 1 {
                    self.tval.push(c as u8);
                    j += 1;
                }
                c = self.get_char();
            }
            /* Pas de ungetChar (verbatim). */
            return;
        }

        // `#` = couleur dans un bloc, sélecteur sinon (verbatim).
        if c == i32::from(b'#') && self.within_block {
            self.ttype = CssTokenType::Color;
            self.tval.clear();
            self.tval.push(b'#');
            let mut j = 1usize;
            c = self.get_char();
            while c >= 0 && is_xdigit(c as u8) {
                if j < MAX_STR_LEN - 1 {
                    self.tval.push(c as u8);
                    j += 1;
                }
                c = self.get_char();
            }
            self.unget_char();
            return;
        }

        if c == EOF {
            self.ttype = CssTokenType::End;
            self.tval.clear();
            return;
        }

        self.ttype = CssTokenType::Char;
        self.tval.clear();
        self.tval.push(c as u8);
    }

    /// `tokenMatchesProperty` — verbatim : essaye les types dans
    /// l'ordre de la table, restaure `type` si échec.
    fn token_matches_property(&self, prop: CssPropertyName, ty: &mut CssValueType) -> bool {
        let saved_type = *ty;
        let info = &CSS_PROPERTY_INFO[prop as usize];

        for cand in info.types.iter().copied() {
            if cand == CssValueType::Unused {
                break;
            }
            *ty = cand;
            let tval = &self.tval;
            match cand {
                CssValueType::Enum => {
                    if self.ttype == CssTokenType::Symbol {
                        for (i, s) in info.enum_symbols.iter().enumerate() {
                            let _ = i;
                            if str_ascii_casecmp(tval, s.as_bytes()) == 0 {
                                return true;
                            }
                        }
                    }
                }
                CssValueType::MultiEnum => {
                    if self.ttype == CssTokenType::Symbol {
                        if str_ascii_casecmp(tval, b"none") == 0 {
                            return true;
                        }
                        for s in info.enum_symbols {
                            if str_ascii_casecmp(tval, s.as_bytes()) == 0 {
                                return true;
                            }
                        }
                    }
                }
                CssValueType::BackgroundPosition => {
                    if self.ttype == CssTokenType::Symbol
                        && (str_ascii_casecmp(tval, b"center") == 0
                            || str_ascii_casecmp(tval, b"left") == 0
                            || str_ascii_casecmp(tval, b"right") == 0
                            || str_ascii_casecmp(tval, b"top") == 0
                            || str_ascii_casecmp(tval, b"bottom") == 0)
                    {
                        return true;
                    }
                    if self.ttype == CssTokenType::Decint || self.ttype == CssTokenType::Float {
                        return true;
                    }
                }
                CssValueType::LengthPercentage
                | CssValueType::LengthPercentageNumber
                | CssValueType::Length => {
                    if self.tval.first() == Some(&b'-') {
                        return false;
                    }
                    if self.ttype == CssTokenType::Decint || self.ttype == CssTokenType::Float {
                        return true;
                    }
                }
                CssValueType::SignedLength => {
                    if self.ttype == CssTokenType::Decint || self.ttype == CssTokenType::Float {
                        return true;
                    }
                }
                CssValueType::Auto => {
                    if self.ttype == CssTokenType::Symbol && str_ascii_casecmp(tval, b"auto") == 0 {
                        return true;
                    }
                }
                CssValueType::Color => {
                    if (self.ttype == CssTokenType::Color || self.ttype == CssTokenType::Symbol)
                        && (str_ascii_casecmp(tval, b"rgb") == 0 || color_parse(tval, -1).1 != 1)
                    {
                        return true;
                    }
                }
                CssValueType::String => {
                    if self.ttype == CssTokenType::String {
                        return true;
                    }
                }
                CssValueType::Symbol => {
                    if self.ttype == CssTokenType::Symbol || self.ttype == CssTokenType::String {
                        return true;
                    }
                }
                CssValueType::FontWeight => {
                    if self.ttype == CssTokenType::Decint {
                        let v = strtol(tval);
                        if (100..=900).contains(&v) {
                            return true;
                        }
                    }
                }
                CssValueType::Uri => {
                    if self.ttype == CssTokenType::Symbol
                        && (str_ascii_casecmp(tval, b"url") == 0
                            || str_ascii_casecmp(tval, b"none") == 0)
                    {
                        return true;
                    }
                }
                CssValueType::Integer => {
                    if self.ttype == CssTokenType::Decint {
                        return true;
                    }
                }
                CssValueType::Unused => {}
            }
        }

        *ty = saved_type;
        false
    }

    /// `parseRgbColorComponent` — verbatim (mix %/entier interdit,
    /// clamp [0,255]).
    fn parse_rgb_color_component(&mut self, cc: &mut i32, percentage: &mut i32) -> bool {
        if self.ttype != CssTokenType::Decint {
            return false;
        }

        *cc = strtol(&self.tval) as i32;

        self.next_token();
        if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b'%') {
            if *percentage == 0 {
                return false;
            }
            *percentage = 1;
            *cc = *cc * 255 / 100;
            self.next_token();
        } else {
            if *percentage == 1 {
                return false;
            }
            *percentage = 0;
        }

        *cc = (*cc).clamp(0, 255);
        true
    }

    /// `parseRgbColor` — verbatim.
    fn parse_rgb_color(&mut self, c: &mut i32) -> bool {
        let mut cc = 0i32;
        let mut percentage = -1i32;

        *c = 0;

        if self.ttype != CssTokenType::Char || self.tval.first() != Some(&b'(') {
            return false;
        }
        self.next_token();

        if !self.parse_rgb_color_component(&mut cc, &mut percentage) {
            return false;
        }
        *c |= cc << 16;

        if self.ttype != CssTokenType::Char || self.tval.first() != Some(&b',') {
            return false;
        }
        self.next_token();

        if !self.parse_rgb_color_component(&mut cc, &mut percentage) {
            return false;
        }
        *c |= cc << 8;

        if self.ttype != CssTokenType::Char || self.tval.first() != Some(&b',') {
            return false;
        }
        self.next_token();

        if !self.parse_rgb_color_component(&mut cc, &mut percentage) {
            return false;
        }
        *c |= cc;

        if self.ttype != CssTokenType::Char || self.tval.first() != Some(&b')') {
            return false;
        }
        true
    }

    /// `parseValue` — verbatim.
    fn parse_value(
        &mut self,
        prop: CssPropertyName,
        ty: CssValueType,
        val: &mut CssPropertyValue,
    ) -> bool {
        let mut ret = false;

        match ty {
            CssValueType::Enum => {
                if self.ttype == CssTokenType::Symbol {
                    for (i, s) in CSS_PROPERTY_INFO[prop as usize]
                        .enum_symbols
                        .iter()
                        .enumerate()
                    {
                        if str_ascii_casecmp(&self.tval, s.as_bytes()) == 0 {
                            *val = CssPropertyValue::Int(i as i32);
                            ret = true;
                            break;
                        }
                    }
                    self.next_token();
                }
            }
            CssValueType::MultiEnum => {
                *val = CssPropertyValue::Int(0);
                ret = true;
                let mut ival = 0i32;
                while self.ttype == CssTokenType::Symbol {
                    if str_ascii_casecmp(&self.tval, b"none") != 0 {
                        for (i, s) in CSS_PROPERTY_INFO[prop as usize]
                            .enum_symbols
                            .iter()
                            .enumerate()
                        {
                            if str_ascii_casecmp(&self.tval, s.as_bytes()) == 0 {
                                ival |= 1 << i;
                            }
                        }
                    }
                    self.next_token();
                }
                *val = CssPropertyValue::Int(ival);
            }
            CssValueType::LengthPercentage
            | CssValueType::LengthPercentageNumber
            | CssValueType::Length
            | CssValueType::SignedLength => {
                if self.ttype == CssTokenType::Decint || self.ttype == CssTokenType::Float {
                    let mut fval = atof(&self.tval) as f32;
                    let mut lentype = CssLengthType::None;

                    self.next_token();
                    if !self.space_separated && self.ttype == CssTokenType::Symbol {
                        ret = true;
                        if str_ascii_casecmp(&self.tval, b"px") == 0 {
                            lentype = CssLengthType::Px;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"mm") == 0 {
                            lentype = CssLengthType::Mm;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"cm") == 0 {
                            lentype = CssLengthType::Mm;
                            fval *= 10.0;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"in") == 0 {
                            lentype = CssLengthType::Mm;
                            fval *= 25.4;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"pt") == 0 {
                            lentype = CssLengthType::Mm;
                            fval *= 25.4 / 72.0;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"pc") == 0 {
                            lentype = CssLengthType::Mm;
                            fval *= 25.4 / 6.0;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"em") == 0 {
                            lentype = CssLengthType::Em;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"ex") == 0 {
                            lentype = CssLengthType::Ex;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"ch") == 0 {
                            lentype = CssLengthType::Ch;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"rem") == 0 {
                            lentype = CssLengthType::Rem;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"vw") == 0 {
                            lentype = CssLengthType::Vw;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"vh") == 0 {
                            lentype = CssLengthType::Vh;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"vmin") == 0 {
                            lentype = CssLengthType::Vmin;
                            self.next_token();
                        } else if str_ascii_casecmp(&self.tval, b"vmax") == 0 {
                            lentype = CssLengthType::Vmax;
                            self.next_token();
                        } else {
                            ret = false;
                        }
                    } else if !self.space_separated
                        && (ty == CssValueType::LengthPercentage
                            || ty == CssValueType::LengthPercentageNumber)
                        && self.ttype == CssTokenType::Char
                        && self.tval.first() == Some(&b'%')
                    {
                        fval /= 100.0;
                        lentype = CssLengthType::Percentage;
                        ret = true;
                        self.next_token();
                    }

                    /* 0 sans unité toujours admis ; numbers sans unité
                     * seulement pour LENGTH_PERCENTAGE_NUMBER. */
                    if lentype == CssLengthType::None
                        && (ty == CssValueType::LengthPercentageNumber || fval == 0.0)
                    {
                        ret = true;
                    }

                    *val = CssPropertyValue::Len(css_create_length(fval, lentype));
                }
            }
            CssValueType::Auto => {
                debug_assert!(
                    self.ttype == CssTokenType::Symbol
                        && str_ascii_casecmp(&self.tval, b"auto") == 0
                );
                ret = true;
                // verbatim : `intVal = CSS_LENGTH_TYPE_AUTO` (= 13).
                *val = CssPropertyValue::Int(CssLengthType::Auto as i32);
                self.next_token();
            }
            CssValueType::Color => {
                if self.ttype == CssTokenType::Color {
                    let (color, err) = color_parse(&self.tval, -1);
                    if err == 0 {
                        ret = true;
                    }
                    *val = CssPropertyValue::Int(color);
                    self.next_token();
                } else if self.ttype == CssTokenType::Symbol {
                    if str_ascii_casecmp(&self.tval, b"rgb") == 0 {
                        self.next_token();
                        let mut c = 0;
                        if self.parse_rgb_color(&mut c) {
                            *val = CssPropertyValue::Int(c);
                            ret = true;
                        }
                    } else {
                        let (color, err) = color_parse(&self.tval, -1);
                        if err == 0 {
                            ret = true;
                        }
                        *val = CssPropertyValue::Int(color);
                    }
                    self.next_token();
                }
            }
            CssValueType::String => {
                if self.ttype == CssTokenType::String {
                    *val = CssPropertyValue::Str(self.tval.clone());
                    ret = true;
                    self.next_token();
                }
            }
            CssValueType::Symbol => {
                // liste comma-separated (font-family) — verbatim.
                let mut dstr: Vec<u8> = Vec::new();
                while self.ttype == CssTokenType::Symbol
                    || self.ttype == CssTokenType::String
                    || (self.ttype == CssTokenType::Char && self.tval.first() == Some(&b','))
                {
                    if self.space_separated {
                        dstr.push(b' ');
                    }
                    dstr.extend_from_slice(&self.tval);
                    ret = true;
                    self.next_token();
                }
                if ret {
                    let mut stripped = dstr;
                    dillo_common::dlib::str_strip(&mut stripped);
                    *val = CssPropertyValue::Str(stripped);
                }
            }
            CssValueType::FontWeight => {
                let mut ival = 0i64;
                if self.ttype == CssTokenType::Decint {
                    ival = strtol(&self.tval);
                    if !(100..=900).contains(&ival) {
                        ival = 0;
                    }
                }
                if ival != 0 {
                    *val = CssPropertyValue::Int(ival as i32);
                    ret = true;
                    self.next_token();
                }
            }
            CssValueType::Uri => {
                if self.ttype == CssTokenType::Symbol {
                    if str_ascii_casecmp(&self.tval, b"url") == 0 {
                        if let Some(url) = self.parse_url() {
                            *val = CssPropertyValue::Str(url);
                            ret = true;
                        }
                    } else if str_ascii_casecmp(&self.tval, b"none") == 0 {
                        *val = CssPropertyValue::None;
                        ret = true;
                    }
                    self.next_token();
                }
            }
            CssValueType::BackgroundPosition => {
                // verbatim : deux valeurs h/v testées en parallèle.
                let mut h = [false; 2];
                let mut v = [false; 2];
                let mut pos = [
                    css_create_length(0.0, CssLengthType::None),
                    css_create_length(0.0, CssLengthType::None),
                ];

                for i in 0..2 {
                    let mut type_tmp = CssValueType::Unused;
                    if self
                        .token_matches_property(CssPropertyName::BackgroundPosition, &mut type_tmp)
                    {
                        h[i] = self.ttype != CssTokenType::Symbol
                            || (str_ascii_casecmp(&self.tval, b"top") != 0
                                && str_ascii_casecmp(&self.tval, b"bottom") != 0);
                        v[i] = self.ttype != CssTokenType::Symbol
                            || (str_ascii_casecmp(&self.tval, b"left") != 0
                                && str_ascii_casecmp(&self.tval, b"right") != 0);
                    }

                    if h[i] || v[i] {
                        if self.ttype == CssTokenType::Symbol {
                            if str_ascii_casecmp(&self.tval, b"top") == 0
                                || str_ascii_casecmp(&self.tval, b"left") == 0
                            {
                                pos[i] = css_create_length(0.0, CssLengthType::Percentage);
                                self.next_token();
                            } else if str_ascii_casecmp(&self.tval, b"center") == 0 {
                                pos[i] = css_create_length(0.5, CssLengthType::Percentage);
                                self.next_token();
                            } else if str_ascii_casecmp(&self.tval, b"bottom") == 0
                                || str_ascii_casecmp(&self.tval, b"right") == 0
                            {
                                pos[i] = css_create_length(1.0, CssLengthType::Percentage);
                                self.next_token();
                            }
                        } else {
                            // <length> ou <percentage> — verbatim.
                            let mut val_tmp = CssPropertyValue::None;
                            if self.parse_value(prop, CssValueType::LengthPercentage, &mut val_tmp)
                                || self.parse_value(prop, CssValueType::SignedLength, &mut val_tmp)
                            {
                                if let CssPropertyValue::Len(l) = val_tmp {
                                    pos[i] = l;
                                }
                                ret = true;
                            } else {
                                h[i] = false;
                                v[i] = false;
                            }
                        }
                    }

                    if !h[i] && !v[i] {
                        break;
                    }
                }

                if v[0] || h[0] {
                    if !h[1] && !v[1] {
                        pos[1] = css_create_length(0.5, CssLengthType::Percentage);
                        h[1] = true;
                        v[1] = true;
                    }
                    if (h[0] && v[1]) || (v[0] && h[1]) {
                        ret = true;
                        let bp = if h[0] && v[1] {
                            CssBackgroundPosition {
                                pos_x: pos[0],
                                pos_y: pos[1],
                            }
                        } else {
                            CssBackgroundPosition {
                                pos_x: pos[1],
                                pos_y: pos[0],
                            }
                        };
                        *val = CssPropertyValue::Pos(Box::new(bp));
                    }
                }
            }
            CssValueType::Integer => {
                if self.ttype == CssTokenType::Decint {
                    *val = CssPropertyValue::Int(strtol(&self.tval) as i32);
                    ret = true;
                    self.next_token();
                }
            }
            CssValueType::Unused => {}
        }

        ret
    }

    /// `parseWeight` — verbatim (`!important`).
    fn parse_weight(&mut self) -> bool {
        if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b'!') {
            self.next_token();
            if self.ttype == CssTokenType::Symbol
                && str_ascii_casecmp(&self.tval, b"important") == 0
            {
                self.next_token();
                return true;
            }
        }
        false
    }

    /// `parseDeclaration` — verbatim (propriété ou shorthand puis
    /// skip jusqu'à `;`/`}`/END).
    fn parse_declaration(
        &mut self,
        props: &mut CssPropertyList,
        important_props: &mut CssPropertyList,
    ) {
        let dir_set: [[usize; 4]; 4] = [
            /* 1 value  */ [0, 0, 0, 0],
            /* 2 values */ [0, 0, 1, 1],
            /* 3 values */ [0, 2, 1, 1],
            /* 4 values */ [0, 2, 3, 1],
        ];

        if self.ttype == CssTokenType::Symbol {
            // bsearch verbatim dans les NUM_PARSED premières entrées.
            let needle = self.tval.clone();
            let found = CSS_PROPERTY_INFO[..CSS_NUM_PARSED_PROPERTIES]
                .binary_search_by(|pi| str_ascii_casecmp(pi.symbol.as_bytes(), &needle).cmp(&0));

            if let Ok(idx) = found {
                let Some(prop) = CssPropertyName::from_i16(idx as i16) else {
                    return;
                };
                self.next_token();
                if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b':') {
                    self.next_token();
                    let mut ty = CssValueType::Unused;
                    let mut val = CssPropertyValue::None;
                    if self.token_matches_property(prop, &mut ty)
                        && self.parse_value(prop, ty, &mut val)
                    {
                        let weight = self.parse_weight();
                        if weight {
                            important_props.set(prop, ty, val);
                        } else {
                            props.set(prop, ty, val);
                        }
                    }
                }
            } else {
                // shorthands — bsearch verbatim.
                let sh = CSS_SHORTHAND_INFO.binary_search_by(|si| {
                    str_ascii_casecmp(si.symbol.as_bytes(), &needle).cmp(&0)
                });

                if let Ok(sh_index) = sh {
                    self.next_token();
                    if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b':') {
                        self.next_token();
                        let info = &CSS_SHORTHAND_INFO[sh_index];

                        match info.ty {
                            CssShorthandType::Font | CssShorthandType::Multiple => {
                                // verbatim : FONT non implémenté →
                                // même chemin que MULTIPLE.
                                loop {
                                    let mut found2 = false;
                                    for &p16 in info.properties {
                                        if p16 == CSS_PROPERTY_END {
                                            break;
                                        }
                                        let Some(p) = CssPropertyName::from_i16(p16) else {
                                            continue;
                                        };
                                        let mut ty = CssValueType::Unused;
                                        if self.token_matches_property(p, &mut ty) {
                                            found2 = true;
                                            let mut val = CssPropertyValue::None;
                                            if self.parse_value(p, ty, &mut val) {
                                                let weight = self.parse_weight();
                                                if weight {
                                                    important_props.set(p, ty, val);
                                                } else {
                                                    props.set(p, ty, val);
                                                }
                                            }
                                            break;
                                        }
                                    }
                                    if !found2 {
                                        break;
                                    }
                                }
                            }
                            CssShorthandType::Directions => {
                                let mut n = 0usize;
                                let mut dir_vals = [
                                    CssPropertyValue::None,
                                    CssPropertyValue::None,
                                    CssPropertyValue::None,
                                    CssPropertyValue::None,
                                ];
                                let mut dir_types = [
                                    CssValueType::Unused,
                                    CssValueType::Unused,
                                    CssValueType::Unused,
                                    CssValueType::Unused,
                                ];
                                let Some(p0) = CssPropertyName::from_i16(info.properties[0]) else {
                                    return;
                                };
                                while n < 4 {
                                    let mut ty = CssValueType::Unused;
                                    if self.token_matches_property(p0, &mut ty) {
                                        let mut val = CssPropertyValue::None;
                                        if self.parse_value(p0, ty, &mut val) {
                                            dir_vals[n] = val;
                                            dir_types[n] = ty;
                                            n += 1;
                                            continue;
                                        }
                                    }
                                    break;
                                }

                                let weight = self.parse_weight();
                                if n > 0 {
                                    for (i, &src) in dir_set[n - 1].iter().enumerate() {
                                        let Some(p) = CssPropertyName::from_i16(info.properties[i])
                                        else {
                                            continue;
                                        };
                                        if weight {
                                            important_props.set(
                                                p,
                                                dir_types[src],
                                                dir_vals[src].clone(),
                                            );
                                        } else {
                                            props.set(p, dir_types[src], dir_vals[src].clone());
                                        }
                                    }
                                }
                            }
                            CssShorthandType::Border => loop {
                                let mut found2 = false;
                                for i in 0..3 {
                                    let Some(p) = CssPropertyName::from_i16(info.properties[i])
                                    else {
                                        continue;
                                    };
                                    let mut ty = CssValueType::Unused;
                                    if self.token_matches_property(p, &mut ty) {
                                        found2 = true;
                                        let mut val = CssPropertyValue::None;
                                        if self.parse_value(p, ty, &mut val) {
                                            let weight = self.parse_weight();
                                            for j in 0..4 {
                                                let Some(pj) = CssPropertyName::from_i16(
                                                    info.properties[j * 3 + i],
                                                ) else {
                                                    continue;
                                                };
                                                if weight {
                                                    important_props.set(pj, ty, val.clone());
                                                } else {
                                                    props.set(pj, ty, val.clone());
                                                }
                                            }
                                        }
                                        break;
                                    }
                                }
                                if !found2 {
                                    break;
                                }
                            },
                        }
                    }
                }
            }
        }

        // verbatim : skip jusqu'à END, ';' ou '}'.
        while !(self.ttype == CssTokenType::End
            || (self.ttype == CssTokenType::Char
                && (self.tval.first() == Some(&b';') || self.tval.first() == Some(&b'}'))))
        {
            self.next_token();
        }
        if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b';') {
            self.next_token();
        }
    }

    /// `parseSimpleSelector` — verbatim (élément | `*` | `#` | `.` |
    /// `:` puis sélecteurs ; une seule pseudo-classe).
    fn parse_simple_selector(&mut self, selector: &mut crate::css::CssSimpleSelector) -> bool {
        if self.ttype == CssTokenType::Symbol {
            let element = self.tag_index.tag_index(&self.tval);
            selector.set_element(element);
            self.next_token();
            if self.space_separated {
                return true;
            }
        } else if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b'*') {
            selector.set_element(crate::css::ELEMENT_ANY);
            self.next_token();
            if self.space_separated {
                return true;
            }
        } else if !(self.ttype == CssTokenType::Char
            && matches!(self.tval.first(), Some(b'#' | b'.' | b':')))
        {
            return false;
        }

        loop {
            let mut select_type = SelectType::None;
            if self.ttype == CssTokenType::Char {
                match self.tval.first() {
                    Some(b'#') => select_type = SelectType::Id,
                    Some(b'.') => select_type = SelectType::Class,
                    Some(b':') => {
                        select_type = SelectType::PseudoClass;
                        if selector.pseudo.is_some() {
                            // une seule pseudo-classe supportée —
                            // règle jetée (verbatim).
                            return false;
                        }
                    }
                    _ => {}
                }
            }

            if select_type == SelectType::None {
                break;
            }

            self.next_token();
            if self.space_separated {
                return false;
            }

            if self.ttype == CssTokenType::Symbol {
                let v = self.tval.clone();
                selector.set_select(select_type, &v);
                self.next_token();
            } else {
                // classes/ids commençant par un chiffre refusés.
                return false;
            }
            if self.space_separated {
                return true;
            }
        }

        true
    }

    /// `parseSelector` — verbatim (`,`/`{` terminent ; `>`/`+`/
    /// espace = combinateurs).
    fn parse_selector(&mut self) -> Option<CssSelector> {
        let mut selector = CssSelector::new();

        loop {
            {
                let sel = selector.top_mut();
                if !self.parse_simple_selector(sel) {
                    break;
                }
            }

            if self.ttype == CssTokenType::Char
                && (self.tval.first() == Some(&b',') || self.tval.first() == Some(&b'{'))
            {
                // sélecteur complet
                self.skip_selector_tail();
                return Some(selector);
            } else if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b'>') {
                selector.add_simple_selector(Combinator::Child);
                self.next_token();
            } else if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b'+') {
                selector.add_simple_selector(Combinator::AdjacentSibling);
                self.next_token();
            } else if self.ttype != CssTokenType::End && self.space_separated {
                selector.add_simple_selector(Combinator::Descendant);
            } else {
                break;
            }
        }

        self.skip_selector_tail();
        None
    }

    /// Queue commune de `parseSelector` — skip jusqu'à `,`/`{`/END
    /// (verbatim).
    fn skip_selector_tail(&mut self) {
        while self.ttype != CssTokenType::End
            && (self.ttype != CssTokenType::Char
                || (self.tval.first() != Some(&b',') && self.tval.first() != Some(&b'{')))
        {
            self.next_token();
        }
    }

    /// `parseRuleset` — verbatim (sélecteurs `,`-séparés partageant
    /// `props`/`importantProps` ; routage par origine).
    fn parse_ruleset(&mut self) {
        let mut list: Vec<CssSelector> = Vec::new();

        loop {
            if let Some(selector) = self.parse_selector() {
                list.push(selector);
            }
            if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b',') {
                self.next_token();
            } else {
                break;
            }
        }

        // Les listes ne deviennent partagées (Rc) qu'au routage
        // `add_rule` — verbatim `props->ref()` par règle.
        let mut props = CssPropertyList::new(true);
        let mut important_props = CssPropertyList::new(true);

        // bloc `{ … }` ('{' déjà lu)
        if self.ttype != CssTokenType::End {
            self.within_block = true;
            self.next_token();
            while !(self.ttype == CssTokenType::End
                || (self.ttype == CssTokenType::Char && self.tval.first() == Some(&b'}')))
            {
                self.parse_declaration(&mut props, &mut important_props);
            }
            self.within_block = false;
        }

        let props = Rc::new(props);
        let important_props = Rc::new(important_props);

        if let Some(context) = self.context.as_deref_mut() {
            for s in list {
                let s = Rc::new(s);
                match self.origin {
                    CssOrigin::UserAgent => {
                        context.add_rule(s, Rc::clone(&props), CssPrimaryOrder::UserAgent);
                    }
                    CssOrigin::User => {
                        context.add_rule(Rc::clone(&s), Rc::clone(&props), CssPrimaryOrder::User);
                        context.add_rule(
                            s,
                            Rc::clone(&important_props),
                            CssPrimaryOrder::UserImportant,
                        );
                    }
                    CssOrigin::Author => {
                        context.add_rule(Rc::clone(&s), Rc::clone(&props), CssPrimaryOrder::Author);
                        context.add_rule(
                            s,
                            Rc::clone(&important_props),
                            CssPrimaryOrder::AuthorImportant,
                        );
                    }
                }
            }
        }

        if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b'}') {
            self.next_token();
        }
    }

    /// `parseUrl` — verbatim (`url(…)` quoted ou brut, résolution
    /// contre `baseUrl`).
    fn parse_url(&mut self) -> Option<Vec<u8>> {
        if self.ttype != CssTokenType::Symbol || str_ascii_casecmp(&self.tval, b"url") != 0 {
            return None;
        }
        self.next_token();
        if self.ttype != CssTokenType::Char || self.tval.first() != Some(&b'(') {
            return None;
        }
        self.next_token();

        let url_str: Vec<u8> = if self.ttype == CssTokenType::String {
            let s = self.tval.clone();
            self.next_token();
            s
        } else {
            let mut s = Vec::new();
            while self.ttype != CssTokenType::End
                && (self.ttype != CssTokenType::Char || self.tval.first() != Some(&b')'))
            {
                s.extend_from_slice(&self.tval);
                self.next_token();
            }
            s
        };

        if self.ttype != CssTokenType::Char || self.tval.first() != Some(&b')') {
            return None;
        }

        let base = self.base_url_str.as_deref();
        url_new(&url_str, base).map(|mut u| u.url_str().to_vec())
    }

    /// `parseImport` — verbatim (`@import url media… ;`).
    fn parse_import(&mut self, html: &mut dyn CssHtmlHooks) {
        let mut url_str: Option<Vec<u8>> = None;
        let mut import_syntax_ok = false;
        let mut media_syntax_ok = true;
        let mut media_selected = true;

        self.next_token();

        if self.ttype == CssTokenType::Symbol && str_ascii_casecmp(&self.tval, b"url") == 0 {
            url_str = self.parse_url();
        } else if self.ttype == CssTokenType::String {
            url_str = Some(self.tval.clone());
        }

        self.next_token();

        if self.ttype == CssTokenType::Symbol {
            media_syntax_ok = false;
            media_selected = false;
            while self.ttype == CssTokenType::Symbol {
                if str_ascii_casecmp(&self.tval, b"all") == 0
                    || str_ascii_casecmp(&self.tval, b"screen") == 0
                {
                    media_selected = true;
                }
                self.next_token();
                if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b',') {
                    self.next_token();
                } else {
                    media_syntax_ok = true;
                    break;
                }
            }
        }

        if media_syntax_ok && self.ttype == CssTokenType::Char && self.tval.first() == Some(&b';') {
            import_syntax_ok = true;
            self.next_token();
        } else {
            self.ignore_statement();
        }

        if let Some(us) = url_str {
            if import_syntax_ok && media_selected {
                let base = self.base_url_str.clone();
                let use_base = self.base_url_str.is_some();
                if let Some(url) = html.resolve_url(&us, base.as_deref(), use_base) {
                    // verbatim : `a_Html_load_stylesheet` fetch le
                    // sheet puis appelle `styleEngine->parse`
                    // récursivement — le check `importDepth > 10` vit
                    // à l'entrée du parse amont (ici avant récursion).
                    if let Some(data) = html.load_stylesheet(url.clone()) {
                        if self.import_depth > 10 {
                            tracing::warn!(
                                "Maximum depth of CSS @import reached--ignoring stylesheet."
                            );
                        } else if let Some(ctx) = self.context.as_deref_mut() {
                            self.import_depth += 1;
                            CssParser::parse(
                                Some(&mut *html),
                                &mut *self.tag_index,
                                Some(&url),
                                ctx,
                                &data,
                                self.origin,
                            );
                            self.import_depth -= 1;
                        }
                    }
                }
            }
        }
    }

    /// `parseMedia` — verbatim (`@media … { … }` filtré
    /// `all`/`screen`).
    fn parse_media(&mut self) {
        let mut media_syntax_ok = false;
        let mut media_selected = false;

        self.next_token();

        while self.ttype == CssTokenType::Symbol {
            if str_ascii_casecmp(&self.tval, b"all") == 0
                || str_ascii_casecmp(&self.tval, b"screen") == 0
            {
                media_selected = true;
            }
            self.next_token();
            if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b',') {
                self.next_token();
            } else {
                media_syntax_ok = true;
                break;
            }
        }

        if !(media_syntax_ok
            && self.ttype == CssTokenType::Char
            && self.tval.first() == Some(&b'{'))
        {
            self.ignore_statement();
            return;
        }

        if media_selected {
            self.next_token();
            while self.ttype != CssTokenType::End {
                self.parse_ruleset();
                if self.ttype == CssTokenType::Char && self.tval.first() == Some(&b'}') {
                    self.next_token();
                    break;
                }
            }
        } else {
            self.ignore_block();
        }
    }

    /// `ignoreBlock` — verbatim (compte les accolades).
    fn ignore_block(&mut self) {
        let mut depth = 0i32;
        while self.ttype != CssTokenType::End {
            if self.ttype == CssTokenType::Char {
                match self.tval.first() {
                    Some(b'{') => depth += 1,
                    Some(b'}') => {
                        depth -= 1;
                        if depth == 0 {
                            self.next_token();
                            return;
                        }
                    }
                    _ => {}
                }
            }
            self.next_token();
        }
    }

    /// `ignoreStatement` — verbatim (`;` ou bloc).
    fn ignore_statement(&mut self) {
        while self.ttype != CssTokenType::End {
            if self.ttype == CssTokenType::Char {
                match self.tval.first() {
                    Some(b';') => {
                        self.next_token();
                        return;
                    }
                    Some(b'{') => {
                        self.ignore_block();
                        return;
                    }
                    _ => {}
                }
            }
            self.next_token();
        }
    }

    /// `CssParser::parse` — verbatim (boucle top-level :
    /// `@`-règles et rulesets).
    /// `html` : `None` ≙ `NULL` amont (seul `@import` l'utilise).
    /// `tag_index` : seam `a_Html_tag_index` (fonction libre amont —
    /// appelée même pour les feuilles sans `html`).
    pub fn parse(
        html: Option<&mut (dyn CssHtmlHooks + '_)>,
        tag_index: &mut (dyn TagIndex + '_),
        base_url: Option<&DilloUrl>,
        context: &mut CssContext,
        buf: &[u8],
        origin: CssOrigin,
    ) {
        let mut parser = CssParser::new(Some(context), tag_index, origin, base_url, buf);
        let mut hooks = html;
        let mut imports_allowed = true;

        while parser.ttype != CssTokenType::End {
            if parser.ttype == CssTokenType::Char && parser.tval.first() == Some(&b'@') {
                parser.next_token();
                if parser.ttype == CssTokenType::Symbol {
                    if str_ascii_casecmp(&parser.tval, b"import") == 0
                        && hooks.is_some()
                        && imports_allowed
                    {
                        if let Some(h) = hooks.as_mut() {
                            parser.parse_import(&mut **h);
                        }
                    } else if str_ascii_casecmp(&parser.tval, b"media") == 0 {
                        parser.parse_media();
                    } else {
                        parser.ignore_statement();
                    }
                } else {
                    parser.ignore_statement();
                }
            } else {
                imports_allowed = false;
                parser.parse_ruleset();
            }
        }
    }

    /// `parseDeclarationBlock` — verbatim (`style="…"` et hints
    /// non-CSS parsés comme déclarations).
    pub fn parse_declaration_block(
        base_url: Option<&DilloUrl>,
        buf: &[u8],
        props: &mut CssPropertyList,
        props_important: &mut CssPropertyList,
    ) {
        // verbatim : un declaration block n'a pas de sélecteurs —
        // `tag_index` n'est jamais consulté → `NullTagIndex` suffit.
        let mut null_tags = NullTagIndex;
        let mut parser = CssParser::new(None, &mut null_tags, CssOrigin::Author, base_url, buf);
        parser.within_block = true;
        while !(parser.ttype == CssTokenType::End
            || (parser.ttype == CssTokenType::Char && parser.tval.first() == Some(&b'}')))
        {
            parser.parse_declaration(props, props_important);
        }
    }

    /// `propertyNameString`.
    #[must_use]
    pub fn property_name_string(name: CssPropertyName) -> &'static str {
        CSS_PROPERTY_INFO[name as usize].symbol
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::{
        css_length_type, css_length_value, CssLengthType, CssProperty, CssPropertyName,
    };
    use crate::doctree::Doctree;

    /// Table de tags minimale (≙ `a_Html_tag_index`).
    struct TestTags;

    impl TagIndex for TestTags {
        fn tag_index(&mut self, name: &[u8]) -> i32 {
            match name {
                n if str_ascii_casecmp(n, b"body") == 0 => 1,
                n if str_ascii_casecmp(n, b"div") == 0 => 2,
                n if str_ascii_casecmp(n, b"p") == 0 => 3,
                n if str_ascii_casecmp(n, b"span") == 0 => 4,
                n if str_ascii_casecmp(n, b"a") == 0 => 5,
                n if str_ascii_casecmp(n, b"table") == 0 => 6,
                n if str_ascii_casecmp(n, b"td") == 0 => 7,
                _ => -1,
            }
        }
    }

    /// Hooks enregistrant les `@import`.
    struct RecHooks {
        imported: Vec<DilloUrl>,
    }

    impl CssHtmlHooks for RecHooks {
        fn resolve_url(
            &mut self,
            url_str: &[u8],
            base_url: Option<&[u8]>,
            use_base: bool,
        ) -> Option<DilloUrl> {
            let _ = (base_url, use_base);
            let s = std::str::from_utf8(url_str).ok()?;
            let mut buf = String::from("http://example.org/");
            buf.push_str(s);
            url_new(buf.as_bytes(), None)
        }
        fn load_stylesheet(&mut self, url: DilloUrl) -> Option<Vec<u8>> {
            self.imported.push(url);
            None
        }
    }

    fn parse_author(css: &[u8], ctx: &mut CssContext) {
        let mut tags = TestTags;
        CssParser::parse(None, &mut tags, None, ctx, css, CssOrigin::Author);
    }

    /// Applique le contexte sur un nœud `element` simple.
    fn apply_on(ctx: &mut CssContext, element: i32) -> CssPropertyList {
        let mut dt = Doctree::new();
        let n = dt.push();
        dt.node_mut(n).element = element;
        let mut out = CssPropertyList::new(true);
        ctx.apply(&mut out, &dt, n, None, None, None);
        out
    }

    fn get(out: &CssPropertyList, name: CssPropertyName) -> Option<&CssProperty> {
        (0..out.size())
            .filter_map(|i| out.get(i))
            .find(|p| p.name == name as i16)
    }

    #[test]
    fn parse_simple_ruleset() {
        let mut ctx = CssContext::new();
        parse_author(b"div { color: red }", &mut ctx);
        let out = apply_on(&mut ctx, 2);
        let p = get(&out, CssPropertyName::Color).expect("color");
        assert_eq!(p.value.int_val(), Some(0xff0000));
    }

    #[test]
    fn parse_lengths_units() {
        let mut ctx = CssContext::new();
        parse_author(
            b"div { margin-top: 5px; margin-right: 1.5em; width: 10%; margin-left: 2mm }",
            &mut ctx,
        );
        let out = apply_on(&mut ctx, 2);
        let mt = get(&out, CssPropertyName::MarginTop).expect("mt");
        assert_eq!(
            css_length_type(mt.value.len_val().expect("len")),
            CssLengthType::Px
        );
        let mr = get(&out, CssPropertyName::MarginRight).expect("mr");
        let l = mr.value.len_val().expect("len");
        assert_eq!(css_length_type(l), CssLengthType::Em);
        assert!((css_length_value(l) - 1.5).abs() < 0.01);
        // verbatim : margin-bottom n'accepte PAS '%' (SIGNED_LENGTH
        // sans LENGTH_PERCENTAGE) — le % va sur `width`.
        let mb = get(&out, CssPropertyName::Width).expect("width");
        assert_eq!(
            css_length_type(mb.value.len_val().expect("len")),
            CssLengthType::Percentage
        );
        let ml = get(&out, CssPropertyName::MarginLeft).expect("ml");
        assert_eq!(
            css_length_type(ml.value.len_val().expect("len")),
            CssLengthType::Mm
        );
    }

    #[test]
    fn parse_color_forms() {
        let mut ctx = CssContext::new();
        parse_author(
            b"div { color: #f00; background-color: #112233; border-top-color: rgb(0,128,255) }",
            &mut ctx,
        );
        let out = apply_on(&mut ctx, 2);
        assert_eq!(
            get(&out, CssPropertyName::Color).and_then(|p| p.value.int_val()),
            Some(0xff0000)
        );
        assert_eq!(
            get(&out, CssPropertyName::BackgroundColor).and_then(|p| p.value.int_val()),
            Some(0x112233)
        );
        assert_eq!(
            get(&out, CssPropertyName::BorderTopColor).and_then(|p| p.value.int_val()),
            Some(0x0080ff)
        );
    }

    #[test]
    fn margin_shorthand_expands() {
        let mut ctx = CssContext::new();
        parse_author(b"div { margin: 1px 2px 3px 4px }", &mut ctx);
        let out = apply_on(&mut ctx, 2);
        for (name, want) in [
            (CssPropertyName::MarginTop, 1.0f32),
            (CssPropertyName::MarginRight, 2.0),
            (CssPropertyName::MarginBottom, 3.0),
            (CssPropertyName::MarginLeft, 4.0),
        ] {
            let p = get(&out, name).expect("margin");
            let l = p.value.len_val().expect("len");
            assert_eq!(css_length_type(l), CssLengthType::Px);
            assert!((css_length_value(l) - want).abs() < 0.01);
        }
    }

    #[test]
    fn margin_shorthand_two_values() {
        let mut ctx = CssContext::new();
        parse_author(b"div { margin: 1px 2px }", &mut ctx);
        let out = apply_on(&mut ctx, 2);
        assert!(get(&out, CssPropertyName::MarginTop).is_some());
        assert!(get(&out, CssPropertyName::MarginBottom).is_some());
        let ml = get(&out, CssPropertyName::MarginLeft).expect("ml");
        assert!((css_length_value(ml.value.len_val().expect("len")) - 2.0).abs() < 0.01);
    }

    #[test]
    fn important_routes_to_important_list() {
        // `!important` auteur → sheet AuthorImportant → bat author normal
        let mut ctx = CssContext::new();
        parse_author(
            b"div { color: red !important } div { color: blue }",
            &mut ctx,
        );
        let out = apply_on(&mut ctx, 2);
        assert_eq!(
            get(&out, CssPropertyName::Color).and_then(|p| p.value.int_val()),
            Some(0xff0000)
        );
    }

    #[test]
    fn media_screen_selected_print_ignored() {
        let mut ctx = CssContext::new();
        parse_author(
            b"@media screen { div { color: red } } @media print { div { color: blue } }",
            &mut ctx,
        );
        let out = apply_on(&mut ctx, 2);
        assert_eq!(
            get(&out, CssPropertyName::Color).and_then(|p| p.value.int_val()),
            Some(0xff0000)
        );
    }

    #[test]
    fn broken_declaration_does_not_kill_rule() {
        // verbatim : déclaration invalide ignorée, la suite survit
        let mut ctx = CssContext::new();
        parse_author(b"div { color: ??? ; margin-top: 5px }", &mut ctx);
        let out = apply_on(&mut ctx, 2);
        assert!(get(&out, CssPropertyName::MarginTop).is_some());
    }

    #[test]
    fn unknown_tag_rule_dropped() {
        // verbatim : `a_Html_tag_index` -1 → ELEMENT_NONE → addRule jette
        let mut ctx = CssContext::new();
        parse_author(b"nosuchtag { color: red }", &mut ctx);
        let out = apply_on(&mut ctx, 1);
        assert_eq!(out.size(), 0);
    }

    #[test]
    fn class_and_id_selectors() {
        let mut ctx = CssContext::new();
        parse_author(b".foo { color: red } #bar { color: blue }", &mut ctx);

        let mut dt = Doctree::new();
        let n = dt.push();
        dt.node_mut(n).klass = Some(vec![b"foo".to_vec()]);
        dt.node_mut(n).id = Some(b"bar".to_vec());
        let mut out = CssPropertyList::new(true);
        ctx.apply(&mut out, &dt, n, None, None, None);
        // id bat class (specificite)
        assert_eq!(
            get(&out, CssPropertyName::Color).and_then(|p| p.value.int_val()),
            Some(0x0000ff)
        );
    }

    #[test]
    fn child_combinator_parsed() {
        let mut ctx = CssContext::new();
        parse_author(b"div > p { color: red }", &mut ctx);

        let mut dt = Doctree::new();
        let d = dt.push();
        dt.node_mut(d).element = 2;
        let p = dt.push();
        dt.node_mut(p).element = 3;
        let mut out = CssPropertyList::new(true);
        ctx.apply(&mut out, &dt, p, None, None, None);
        assert!(get(&out, CssPropertyName::Color).is_some());
    }

    #[test]
    fn comments_and_xml_markers_skipped() {
        let mut ctx = CssContext::new();
        parse_author(b"<!-- /* comment */ div { color: red } -->", &mut ctx);
        let out = apply_on(&mut ctx, 2);
        assert!(get(&out, CssPropertyName::Color).is_some());
    }

    #[test]
    fn border_rigth_typo_quirk() {
        // verbatim : la table amont contient `border-rigth-width`
        // (typo) — l'orthographe correcte n'est PAS reconnue.
        let mut ctx = CssContext::new();
        parse_author(b"div { border-right-width: 3px }", &mut ctx);
        let out = apply_on(&mut ctx, 2);
        assert!(get(&out, CssPropertyName::BorderRightWidth).is_none());

        let mut ctx2 = CssContext::new();
        parse_author(b"div { border-rigth-width: 3px }", &mut ctx2);
        let out2 = apply_on(&mut ctx2, 2);
        assert!(get(&out2, CssPropertyName::BorderRightWidth).is_some());
    }

    #[test]
    fn import_calls_load_stylesheet() {
        let mut ctx = CssContext::new();
        let mut tags = TestTags;
        let mut hooks = RecHooks {
            imported: Vec::new(),
        };
        CssParser::parse(
            Some(&mut hooks),
            &mut tags,
            None,
            &mut ctx,
            b"@import \"foo.css\"; div { color: red }",
            CssOrigin::Author,
        );
        assert_eq!(hooks.imported.len(), 1);
        let out = apply_on(&mut ctx, 2);
        assert!(get(&out, CssPropertyName::Color).is_some());
    }

    #[test]
    fn import_after_ruleset_ignored() {
        // verbatim : @import non initial → ignore_statement
        let mut ctx = CssContext::new();
        let mut tags = TestTags;
        let mut hooks = RecHooks {
            imported: Vec::new(),
        };
        CssParser::parse(
            Some(&mut hooks),
            &mut tags,
            None,
            &mut ctx,
            b"div { color: red } @import \"foo.css\";",
            CssOrigin::Author,
        );
        assert_eq!(hooks.imported.len(), 0);
    }

    #[test]
    fn declaration_block_parses() {
        let mut props = CssPropertyList::new(true);
        let mut imp = CssPropertyList::new(true);
        CssParser::parse_declaration_block(
            None,
            b"color: red; margin-top: 2px !important",
            &mut props,
            &mut imp,
        );
        assert!(get(&props, CssPropertyName::Color).is_some());
        assert!(get(&props, CssPropertyName::MarginTop).is_none());
        assert!(get(&imp, CssPropertyName::MarginTop).is_some());
    }

    #[test]
    fn border_shorthand_expands() {
        let mut ctx = CssContext::new();
        parse_author(b"div { border: 1px solid red }", &mut ctx);
        let out = apply_on(&mut ctx, 2);
        assert!(get(&out, CssPropertyName::BorderTopWidth).is_some());
        assert!(get(&out, CssPropertyName::BorderTopStyle).is_some());
        assert_eq!(
            get(&out, CssPropertyName::BorderTopColor).and_then(|p| p.value.int_val()),
            Some(0xff0000)
        );
    }
}
