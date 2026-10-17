//! Port of the character-reference machinery of `src/html.cc`
//! (`Html_parse_numeric_charref`, `Html_parse_named_charref`,
//! `Html_parse_entity`, `a_Html_parse_entities`,
//! `Html_charref_search`, `Html_ms_stupid_quotes_2ucs`,
//! `Html_ms_quotes_2ucs`).
//!
//! Table `Charrefs[]` : `entities_table.rs` (générée de
//! `html_charrefs.h`).
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

use crate::common::DocType;
use crate::entities_table::CHARREFS;
use dillo_common::{dlib, utf8};

/// `Html_ms_stupid_quotes_2ucs` (html.cc:866) — "smart quotes"
/// Windows-1252 mappées sur l'ASCII voisin.
fn ms_stupid_quotes_2ucs(codepoint: i32) -> i32 {
    match codepoint {
        145 | 146 => i32::from(b'\''),
        147 | 148 => i32::from(b'"'),
        149 => 176,
        150 | 151 => i32::from(b'-'),
        _ => codepoint,
    }
}

/// Contexte verbatim nécessaire aux règles mode-dépendantes
/// (`html->DocType`, `html->DocTypeVersion`, `prefs.show_extra_warnings`
/// et `BUG_MSG`).
pub struct EntityCtx<'a> {
    /// `html->DocType`.
    pub doc_type: DocType,
    /// `html->DocTypeVersion`.
    pub doc_type_version: f32,
    /// `prefs.show_extra_warnings`.
    pub show_extra_warnings: bool,
    /// `BUG_MSG` → `bw->page_bugs` + `a_UIcmd_set_bug_prog`.
    pub bug: &'a mut dyn FnMut(&str),
}

impl EntityCtx<'_> {
    /// `html->DocType == DT_HTML && html->DocTypeVersion >= 5.0f`.
    fn is_html5(&self) -> bool {
        self.doc_type == DocType::Html && self.doc_type_version >= 5.0
    }

    /// `html->DocType == DT_HTML && html->DocTypeVersion <= 4.01f`.
    fn is_html_legacy(&self) -> bool {
        self.doc_type == DocType::Html && self.doc_type_version <= 4.01
    }
}

/// `Html_parse_numeric_charref` (html.cc:886) — `tok` = ce qui suit
/// `&#`. Retourne `(Octets, entsize)` ou `None`.
fn parse_numeric_charref(
    ctx: &mut EntityCtx,
    tok: &[u8],
    is_attr: bool,
) -> Option<(Vec<u8>, usize)> {
    let mut s = 0usize;
    let mut codepoint: i32 = -1;

    match tok.first() {
        Some(b'x' | b'X') => {
            s += 1;
            if tok.get(s).is_some_and(|&c| dlib::is_xdigit(c)) {
                // strtol base 16 tolère "0x" — pas amont.
                if tok.get(s) == Some(&b'0') && tok.get(s + 1) == Some(&b'x') {
                    s += 1;
                    codepoint = 0;
                } else {
                    let (v, end) = dlib::strtol(&tok[s..], 16);
                    s += end;
                    codepoint = v;
                }
            }
        }
        _ => {
            if tok.get(s).is_some_and(|&c| dlib::is_digit(c)) {
                let (v, end) = dlib::strtol(&tok[s..], 10);
                s += end;
                codepoint = v;
            }
        }
    }
    // errno=ERANGE amont ≙ saturation strtol — pas de flag distinct
    // dans le port (valeur saturée rejetée par la plage ci-dessous).

    if tok.get(s) == Some(&b';') {
        s += 1;
    } else {
        if ctx.show_extra_warnings && (ctx.doc_type == DocType::Xhtml || ctx.is_html_legacy()) {
            (ctx.bug)(&format!(
                "Character reference '&#{}' lacks ';'.",
                String::from_utf8_lossy(&tok[..s])
            ));
        }
        // `;` optionnel pour le vieux HTML, sauf dans les attributs
        // (heuristique "&copy=1" des URLs) — verbatim.
        if is_attr || ctx.doc_type == DocType::Xhtml || ctx.is_html5() {
            return None;
        }
    }
    if (codepoint < 0x20 && codepoint != 0x09 && codepoint != 0x0a && codepoint != 0x0c)
        || (0x7f..=0x9f).contains(&codepoint)
        || (0xd800..=0xdfff).contains(&codepoint)
        || codepoint > 0x0010_ffff
        || (codepoint & 0xfffe) == 0xfffe
        || (!ctx.is_html5() && codepoint > 0xffff)
    {
        (ctx.bug)(&format!(
            "Numeric character reference '&#{}' is not valid.",
            String::from_utf8_lossy(&tok[..s])
        ));
        codepoint = if (145..=151).contains(&codepoint) {
            ms_stupid_quotes_2ucs(codepoint)
        } else {
            -1
        };
    }
    if codepoint == -1 {
        return None;
    }
    let mut buf = [0u8; 4];
    let n = if codepoint >= 128 {
        utf8::encode(codepoint as u32, &mut buf)
    } else {
        buf[0] = codepoint as u8;
        1
    };
    // *entsize = s-tok+2 (tok pointe après "&#").
    Some((buf[..n].to_vec(), s + 2))
}

/// `Html_charref_search` (html.cc:976) — `bsearch` verbatim
/// (`binary_search` sur la table triée).
fn charref_search(key: &[u8]) -> Option<&'static crate::entities_table::Charref> {
    CHARREFS
        .binary_search_by(|e| e.name.cmp(key))
        .ok()
        .map(|i| &CHARREFS[i])
}

/// `Html_parse_named_charref` (html.cc:989) — `tok` = ce qui suit `&`.
fn parse_named_charref(ctx: &mut EntityCtx, tok: &[u8], is_attr: bool) -> Option<(Vec<u8>, usize)> {
    let mut s = 0usize;
    while s < tok.len() && (dlib::is_alnum(tok[s]) || b":_.-".contains(&tok[s])) {
        s += 1;
    }
    let name = &tok[..s];
    let c = tok.get(s).copied().unwrap_or(0);
    if c != b';' {
        if ctx.show_extra_warnings && (ctx.doc_type == DocType::Xhtml || ctx.is_html_legacy()) {
            (ctx.bug)(&format!(
                "Character reference '&{}' lacks ';'.",
                String::from_utf8_lossy(name)
            ));
        }
        if is_attr || ctx.doc_type == DocType::Xhtml || ctx.is_html5() {
            return None;
        }
    }

    let mut ret: Option<&'static [u8]> =
        charref_search(name).and_then(|p| if ctx.is_html5() { p.html5 } else { p.html4 });

    if ret.is_none() && ctx.doc_type == DocType::Xhtml && name == b"apos" {
        ret = Some(b"'");
    }

    if c == b';' {
        s += 1;
    }

    if ret.is_none() {
        (ctx.bug)(&format!(
            "Undefined character reference '&{}'.",
            String::from_utf8_lossy(name)
        ));
        return None;
    }
    // *entsize = s-tok+1 (+1 pour le '&' déjà consommé).
    Some((ret.unwrap_or_default().to_vec(), s + 1))
}

/// `Html_parse_entity` (html.cc:1045) — `token[0] == '&'` supposé.
/// Retourne `(octets décodés, taille de l'entité consommée)`.
pub fn parse_entity(ctx: &mut EntityCtx, token: &[u8], is_attr: bool) -> Option<(Vec<u8>, usize)> {
    // Tronçon pathologique : attributs de plusieurs Mo remplis
    // d'entités — la référence nommée la plus longue fait ~32 octets.
    // `dStrndup` copie `min(strlen, size)` → tronquer au 1er NUL.
    let toksize = token.len().saturating_sub(1).min(50);
    let raw = &token[1..=toksize];
    let tok = &raw[..raw.iter().position(|&c| c == 0).unwrap_or(raw.len())];

    match tok.first() {
        Some(b'#') => parse_numeric_charref(ctx, &tok[1..], is_attr),
        Some(&c) if dlib::is_alpha(c) => parse_named_charref(ctx, tok, is_attr),
        _ => {
            if ctx.show_extra_warnings && !ctx.is_html5() {
                (ctx.bug)("Literal '&'.");
            }
            None
        }
    }
}

/// `a_Html_parse_entities` (html.cc:1080) — décode toutes les
/// entités d'un token (texte, `is_attr = FALSE`).
#[must_use]
pub fn parse_entities(ctx: &mut EntityCtx, token: &[u8]) -> Vec<u8> {
    // strcspn(token, "&") — s'arrête aussi au premier NUL.
    let s = token
        .iter()
        .position(|&c| c == b'&' || c == 0)
        .unwrap_or(token.len());
    if s >= token.len() {
        return token.to_vec();
    }
    let mut ds = Vec::with_capacity(token.len());
    ds.extend_from_slice(&token[..s]);
    let mut i = s;
    while i < token.len() {
        if token[i] == b'&'
            && let Some((entstr, entsize)) = parse_entity(ctx, &token[i..], false)
        {
            ds.extend_from_slice(&entstr);
            i += entsize - 1;
        } else {
            ds.push(token[i]);
        }
        i += 1;
    }
    ds
}
