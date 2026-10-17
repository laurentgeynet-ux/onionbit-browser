//! Port of the `Tags[]` dispatch table and tag helpers of
//! `src/html.cc` (`a_Html_tag_index`, `Html_tag_compare`,
//! `Html_match_tag`, `Html_triggers_optional_close`,
//! `Html_forbids_cross_nesting`, `Html_tag_pre_excludes`,
//! `Html_check_html5_obsolete`).
//!
//! Sémantique des flags (`B8`, bit0 = LSB) :
//! - bit0 (1) : élément inline (0 = block)
//! - bit1 (2) : inline container
//! - bit2 (4) : block container
//! - bit3 (8) : body element
//! - bit4 (16) : head element
//!   (les deux bits hauts sont inutilisés amont)
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

use crate::common::in_flags;
use crate::html::DilloHtml;
use dillo_common::dlib;

/// `TagOpenFunct` — `(DilloHtml*, tag, tagsize)`.
pub type OpenFn = fn(&mut DilloHtml, &[u8]);
/// `TagCloseFunct` — `(DilloHtml*)`.
pub type CloseFn = fn(&mut DilloHtml);

/// `EndTag` — `'R'` requis, `'O'` optionnel, `'F'` interdit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndTag {
    /// `'R'`.
    Required,
    /// `'O'`.
    Optional,
    /// `'F'`.
    Forbidden,
}

/// `TagInfo` — verbatim.
#[derive(Debug, Clone, Copy)]
pub struct TagInfo {
    /// `name`.
    pub name: &'static str,
    /// `Flags` — bits `B8` ci-dessus.
    pub flags: u8,
    /// `EndTag`.
    pub end_tag: EndTag,
    /// `open` — toujours non-NULL amont (default = `Html_tag_open_default`).
    pub open: OpenFn,
    /// `content` — appelé sauf `display:none`.
    pub content: Option<OpenFn>,
    /// `close`.
    pub close: Option<CloseFn>,
}

use crate::form;
use crate::html;
use crate::table;

/// `Tags[]` (html.cc:3483) — triée alphabétiquement pour la
/// recherche binaire (`a_Html_tag_index`).
#[allow(clippy::too_many_lines)]
pub static TAGS: &[TagInfo] = &[
    TagInfo {
        name: "a",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_a,
        content: None,
        close: Some(html::tag_close_a),
    },
    TagInfo {
        name: "abbr",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_abbr,
        content: None,
        close: None,
    },
    // acronym 010101 — obsolète HTML5
    TagInfo {
        name: "address",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: Some(html::tag_close_par),
    },
    TagInfo {
        name: "area",
        flags: 0b01001,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_default,
        content: Some(html::tag_content_area),
        close: None,
    },
    TagInfo {
        name: "article",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_sectioning,
        content: None,
        close: None,
    },
    TagInfo {
        name: "aside",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_sectioning,
        content: None,
        close: None,
    },
    TagInfo {
        name: "audio",
        flags: 0b01111,
        end_tag: EndTag::Required,
        open: html::tag_open_audio,
        content: None,
        close: Some(html::tag_close_media),
    },
    TagInfo {
        name: "b",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "base",
        flags: 0b10001,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_base,
        content: None,
        close: None,
    },
    // basefont 010001 — obsolète HTML5
    // bdo 010101
    TagInfo {
        name: "big",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "blockquote",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_blockquote,
        content: None,
        close: None,
    },
    TagInfo {
        name: "body",
        flags: 0b01110,
        end_tag: EndTag::Optional,
        open: html::tag_open_body,
        content: None,
        close: Some(html::tag_close_body),
    },
    TagInfo {
        name: "br",
        flags: 0b01001,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_default,
        content: Some(html::tag_content_br),
        close: None,
    },
    TagInfo {
        name: "button",
        flags: 0b01111,
        end_tag: EndTag::Required,
        open: form::tag_open_button,
        content: None,
        close: Some(form::tag_close_button),
    },
    // caption
    TagInfo {
        name: "center",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "cite",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "code",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    // col 010010 'F'
    // colgroup
    TagInfo {
        name: "dd",
        flags: 0b01110,
        end_tag: EndTag::Optional,
        open: html::tag_open_dd,
        content: None,
        close: None,
    },
    TagInfo {
        name: "del",
        flags: 0b01111,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "dfn",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "dir",
        flags: 0b01100,
        end_tag: EndTag::Required,
        open: html::tag_open_dir,
        content: None,
        close: Some(html::tag_close_par),
    },
    // TODO: complete <div> support!
    TagInfo {
        name: "div",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_div,
        content: None,
        close: None,
    },
    TagInfo {
        name: "dl",
        flags: 0b01100,
        end_tag: EndTag::Required,
        open: html::tag_open_dl,
        content: None,
        close: Some(html::tag_close_par),
    },
    TagInfo {
        name: "dt",
        flags: 0b01010,
        end_tag: EndTag::Optional,
        open: html::tag_open_dt,
        content: None,
        close: Some(html::tag_close_par),
    },
    TagInfo {
        name: "em",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "embed",
        flags: 0b01001,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_embed,
        content: Some(html::tag_content_embed),
        close: None,
    },
    // fieldset
    TagInfo {
        name: "figcaption",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "figure",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "font",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_font,
        content: None,
        close: None,
    },
    TagInfo {
        name: "footer",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_sectioning,
        content: None,
        close: None,
    },
    TagInfo {
        name: "form",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: form::tag_open_form,
        content: None,
        close: Some(form::tag_close_form),
    },
    TagInfo {
        name: "frame",
        flags: 0b01000,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_frame,
        content: Some(html::tag_content_frame),
        close: None,
    },
    TagInfo {
        name: "frameset",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: Some(html::tag_content_frameset),
        close: None,
    },
    TagInfo {
        name: "h1",
        flags: 0b01010,
        end_tag: EndTag::Required,
        open: html::tag_open_h,
        content: None,
        close: None,
    },
    TagInfo {
        name: "h2",
        flags: 0b01010,
        end_tag: EndTag::Required,
        open: html::tag_open_h,
        content: None,
        close: None,
    },
    TagInfo {
        name: "h3",
        flags: 0b01010,
        end_tag: EndTag::Required,
        open: html::tag_open_h,
        content: None,
        close: None,
    },
    TagInfo {
        name: "h4",
        flags: 0b01010,
        end_tag: EndTag::Required,
        open: html::tag_open_h,
        content: None,
        close: None,
    },
    TagInfo {
        name: "h5",
        flags: 0b01010,
        end_tag: EndTag::Required,
        open: html::tag_open_h,
        content: None,
        close: None,
    },
    TagInfo {
        name: "h6",
        flags: 0b01010,
        end_tag: EndTag::Required,
        open: html::tag_open_h,
        content: None,
        close: None,
    },
    TagInfo {
        name: "head",
        flags: 0b10111,
        end_tag: EndTag::Optional,
        open: html::tag_open_head,
        content: None,
        close: Some(html::tag_close_head),
    },
    TagInfo {
        name: "header",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_sectioning,
        content: None,
        close: None,
    },
    TagInfo {
        name: "hr",
        flags: 0b01000,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_hr,
        content: Some(html::tag_content_hr),
        close: None,
    },
    TagInfo {
        name: "html",
        flags: 0b00110,
        end_tag: EndTag::Optional,
        open: html::tag_open_html,
        content: None,
        close: Some(html::tag_close_html),
    },
    TagInfo {
        name: "i",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "iframe",
        flags: 0b01111,
        end_tag: EndTag::Required,
        open: html::tag_open_frame,
        content: Some(html::tag_content_frame),
        close: None,
    },
    TagInfo {
        name: "img",
        flags: 0b01001,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_img,
        content: Some(html::tag_content_img),
        close: None,
    },
    TagInfo {
        name: "input",
        flags: 0b01001,
        end_tag: EndTag::Forbidden,
        open: form::tag_open_input,
        content: None,
        close: None,
    },
    TagInfo {
        name: "ins",
        flags: 0b01111,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "isindex",
        flags: 0b11001,
        end_tag: EndTag::Forbidden,
        open: form::tag_open_isindex,
        content: None,
        close: None,
    },
    TagInfo {
        name: "kbd",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "label",
        flags: 0b01010,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    // legend 01??
    TagInfo {
        name: "li",
        flags: 0b01110,
        end_tag: EndTag::Optional,
        open: html::tag_open_li,
        content: None,
        close: Some(html::tag_close_li),
    },
    TagInfo {
        name: "link",
        flags: 0b10001,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_link,
        content: None,
        close: None,
    },
    TagInfo {
        name: "main",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_sectioning,
        content: None,
        close: None,
    },
    TagInfo {
        name: "map",
        flags: 0b01101,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: Some(html::tag_content_map),
        close: Some(html::tag_close_map),
    },
    TagInfo {
        name: "mark",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    // menu 1010 — pas tout à fait 1010 (peut contenir LI et inline)
    TagInfo {
        name: "menu",
        flags: 0b01100,
        end_tag: EndTag::Required,
        open: html::tag_open_menu,
        content: None,
        close: Some(html::tag_close_par),
    },
    TagInfo {
        name: "meta",
        flags: 0b11001,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_meta,
        content: None,
        close: None,
    },
    TagInfo {
        name: "nav",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_sectioning,
        content: None,
        close: None,
    },
    // noframes 1011 — obsolète HTML5
    // noscript 1011
    TagInfo {
        name: "object",
        flags: 0b11111,
        end_tag: EndTag::Required,
        open: html::tag_open_object,
        content: Some(html::tag_content_object),
        close: None,
    },
    TagInfo {
        name: "ol",
        flags: 0b01100,
        end_tag: EndTag::Required,
        open: html::tag_open_ol,
        content: None,
        close: None,
    },
    TagInfo {
        name: "optgroup",
        flags: 0b01011,
        end_tag: EndTag::Optional,
        open: form::tag_open_optgroup,
        content: None,
        close: Some(form::tag_close_optgroup),
    },
    TagInfo {
        name: "option",
        flags: 0b01001,
        end_tag: EndTag::Optional,
        open: form::tag_open_option,
        content: None,
        close: Some(form::tag_close_option),
    },
    TagInfo {
        name: "p",
        flags: 0b01010,
        end_tag: EndTag::Optional,
        open: html::tag_open_p,
        content: None,
        close: None,
    },
    // param 010001 'F'
    TagInfo {
        name: "pre",
        flags: 0b01010,
        end_tag: EndTag::Required,
        open: html::tag_open_pre,
        content: None,
        close: Some(html::tag_close_pre),
    },
    TagInfo {
        name: "q",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_q,
        content: None,
        close: Some(html::tag_close_q),
    },
    TagInfo {
        name: "s",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "samp",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "script",
        flags: 0b11101,
        end_tag: EndTag::Required,
        open: html::tag_open_script,
        content: None,
        close: Some(html::tag_close_script),
    },
    TagInfo {
        name: "section",
        flags: 0b01110,
        end_tag: EndTag::Required,
        open: html::tag_open_sectioning,
        content: None,
        close: None,
    },
    TagInfo {
        name: "select",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: form::tag_open_select,
        content: None,
        close: Some(form::tag_close_select),
    },
    TagInfo {
        name: "small",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "source",
        flags: 0b01001,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_source,
        content: Some(html::tag_content_source),
        close: None,
    },
    TagInfo {
        name: "span",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_span,
        content: None,
        close: None,
    },
    TagInfo {
        name: "strike",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "strong",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "style",
        flags: 0b10011,
        end_tag: EndTag::Required,
        open: html::tag_open_style,
        content: None,
        close: Some(html::tag_close_style),
    },
    TagInfo {
        name: "sub",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "sup",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "table",
        flags: 0b01100,
        end_tag: EndTag::Required,
        open: table::tag_open_table,
        content: Some(table::tag_content_table),
        close: None,
    },
    TagInfo {
        name: "tbody",
        flags: 0b01010,
        end_tag: EndTag::Optional,
        open: table::tag_open_tbody,
        content: Some(table::tag_content_tbody),
        close: None,
    },
    TagInfo {
        name: "td",
        flags: 0b01110,
        end_tag: EndTag::Optional,
        open: table::tag_open_td,
        content: Some(table::tag_content_td),
        close: None,
    },
    TagInfo {
        name: "textarea",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: form::tag_open_textarea,
        content: Some(form::tag_content_textarea),
        close: Some(form::tag_close_textarea),
    },
    TagInfo {
        name: "tfoot",
        flags: 0b01010,
        end_tag: EndTag::Optional,
        open: table::tag_open_tfoot,
        content: None,
        close: None,
    },
    TagInfo {
        name: "th",
        flags: 0b01110,
        end_tag: EndTag::Optional,
        open: table::tag_open_th,
        content: Some(table::tag_content_th),
        close: None,
    },
    TagInfo {
        name: "thead",
        flags: 0b01010,
        end_tag: EndTag::Optional,
        open: table::tag_open_thead,
        content: None,
        close: None,
    },
    TagInfo {
        name: "title",
        flags: 0b10011,
        end_tag: EndTag::Required,
        open: html::tag_open_title,
        content: None,
        close: Some(html::tag_close_title),
    },
    TagInfo {
        name: "tr",
        flags: 0b01100,
        end_tag: EndTag::Optional,
        open: table::tag_open_tr,
        content: Some(table::tag_content_tr),
        close: None,
    },
    TagInfo {
        name: "tt",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "u",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "ul",
        flags: 0b01100,
        end_tag: EndTag::Required,
        open: html::tag_open_ul,
        content: None,
        close: None,
    },
    TagInfo {
        name: "var",
        flags: 0b01011,
        end_tag: EndTag::Required,
        open: html::tag_open_default,
        content: None,
        close: None,
    },
    TagInfo {
        name: "video",
        flags: 0b01111,
        end_tag: EndTag::Required,
        open: html::tag_open_video,
        content: None,
        close: Some(html::tag_close_media),
    },
    TagInfo {
        name: "wbr",
        flags: 0b01011,
        end_tag: EndTag::Forbidden,
        open: html::tag_open_default,
        content: Some(html::tag_content_wbr),
        close: None,
    },
];

/// `HTML_NTAGS` (html.hh:24).
pub const N_TAGS: usize = 94;

// ------------------------------------------------------------------
// Index statiques (html.cc:125) — comme amont, résolus via
// `a_Html_tag_index` (initialisation paresseuse Rust : const fn bsearch
// impossible sur &[u8] → LazyLock).
// ------------------------------------------------------------------

use std::sync::LazyLock;

macro_rules! tag_index {
    ($name:ident, $tag:literal) => {
        /// Index amont `i_*`.
        pub static $name: LazyLock<i32> = LazyLock::new(|| tag_index($tag.as_bytes()));
    };
}

tag_index!(I_A, "a");
tag_index!(I_BODY, "body");
tag_index!(I_BUTTON, "button");
tag_index!(I_DD, "dd");
tag_index!(I_DT, "dt");
tag_index!(I_HTML, "html");
tag_index!(I_HR, "hr");
tag_index!(I_LI, "li");
tag_index!(I_OPTGROUP, "optgroup");
tag_index!(I_OPTION, "option");
tag_index!(I_P, "p");
tag_index!(I_PRE, "pre");
tag_index!(I_SELECT, "select");
tag_index!(I_TEXTAREA, "textarea");
tag_index!(I_TD, "td");
tag_index!(I_TR, "tr");
tag_index!(I_TBODY, "tbody");
tag_index!(I_TH, "th");

/// `Html_tag_compare` (html.cc:3608) — compare le tag du buffer
/// (`'/'`/`'>'`/espace-terminé) avec le nom en minuscules.
/// Retour : ordre comme `strcmp` (`0` ≙ match).
#[must_use]
pub fn tag_compare(p1: &[u8], p2: &[u8]) -> i32 {
    let mut i = 0;
    while i < p2.len() {
        let c1 = p1.get(i).copied().unwrap_or(0);
        let l = dlib::ascii_to_lower(c1);
        if l != p2[i] {
            return i32::from(l) - i32::from(p2[i]);
        }
        i += 1;
    }
    // `!strchr(" >/\n\r\t", *p1)` — 0 si le caractère suivant
    // termine le nom, ±1 sinon (verbatimsign of strcmp vs '\0' > 0 →
    // upstream renvoie `!found` : 0 si terminal, 1 sinon).
    match p1.get(i) {
        Some(&c) if !b" >/\n\r\t".contains(&c) => 1,
        _ => 0,
    }
}

/// `a_Html_tag_index` (html.cc:3623) — recherche binaire.
/// `-1` si non géré.
#[allow(clippy::comparison_chain)] // bsearch verbatim (`if cond < 0 …`).
#[must_use]
pub fn tag_index(tag: &[u8]) -> i32 {
    let mut low = 0usize;
    let mut high = TAGS.len().saturating_sub(1);
    while low <= high {
        let mid = usize::midpoint(low, high);
        let cond = tag_compare(tag, TAGS[mid].name.as_bytes());
        if cond < 0 {
            if mid == 0 {
                break;
            }
            high = mid - 1;
        } else if cond > 0 {
            low = mid + 1;
        } else {
            return i32::try_from(mid).unwrap_or(-1);
        }
    }
    -1
}

/// `Html_match_tag` (html.cc:1328) — `tagstr` (ex: `"p"`) vs la
/// zone `tag` après `<` (ex: `"P align=center>"`). `/` accepté
/// pour compat XML (`"empty/>"`).
#[must_use]
pub fn match_tag(tagstr: &[u8], tag: &[u8], tagsize: usize) -> bool {
    let mut i = 0;
    while i < tagsize && tagstr.get(i).copied().unwrap_or(0) != 0 {
        let a = dlib::ascii_to_lower(tagstr.get(i).copied().unwrap_or(0));
        let b = dlib::ascii_to_lower(tag.get(i).copied().unwrap_or(0));
        if a != b {
            return false;
        }
        i += 1;
    }
    i < tagsize && matches!(tag.get(i), Some(&c) if dlib::is_space(c) || c == b'>' || c == b'/')
}

/// `Html_triggers_optional_close` (html.cc:3645) — règles de
/// fermeture implicite "à la Firefox" (verbatim, tuned for speed).
#[must_use]
pub fn triggers_optional_close(old_idx: i32, cur_idx: i32) -> bool {
    let flags = TAGS.get(cur_idx as usize).map_or(0, |t| t.flags);
    if old_idx == *I_P || old_idx == *I_DT {
        // P et DT fermés par les éléments block (non-inline).
        (flags & 1) == 0
    } else if old_idx == *I_LI {
        cur_idx == *I_LI
    } else if old_idx == *I_TD || old_idx == *I_TH {
        cur_idx == *I_TD || cur_idx == *I_TH || cur_idx == *I_TR || cur_idx == *I_TBODY
    } else if old_idx == *I_TR {
        cur_idx == *I_TR || cur_idx == *I_TBODY
    } else if old_idx == *I_DD {
        cur_idx == *I_DD || cur_idx == *I_DT
    } else if old_idx == *I_OPTGROUP {
        cur_idx != *I_OPTION
    } else if old_idx == *I_OPTION {
        true // OPTION a toujours besoin d'une fermeture
    } else {
        false // HTML/HEAD/BODY gérés par test_section
    }
}

/// `Html_forbids_cross_nesting` (html.cc:3686) — BUTTON/SELECT/
/// TEXTAREA/A ne peuvent pas s'imbriquer. Retour : index de
/// l'élément ouvert, `-1` sinon.
#[must_use]
pub fn forbids_cross_nesting(in_flags: u32, new_idx: i32) -> i32 {
    let f = in_flags;
    let ni = new_idx;
    if f & (in_flags::A | in_flags::BUTTON | in_flags::SELECT | in_flags::TEXTAREA) != 0
        && (ni == *I_A || ni == *I_BUTTON || ni == *I_SELECT || ni == *I_TEXTAREA)
    {
        if f & in_flags::A != 0 {
            *I_A
        } else if f & in_flags::BUTTON != 0 {
            *I_BUTTON
        } else if f & in_flags::SELECT != 0 {
            *I_SELECT
        } else {
            *I_TEXTAREA
        }
    } else {
        -1
    }
}

/// `Html_check_html5_obsolete` (html.cc:3966) — liste W3C CR
/// 6 août 2013. Indices résolus une fois (≙ `static indexes[9]`).
pub fn check_html5_obsolete<F: FnMut(&str)>(ni: i32, mut bug: F) {
    static INDEXES: LazyLock<[i32; 9]> = LazyLock::new(|| {
        [
            tag_index(b"dir"),
            tag_index(b"frame"),
            tag_index(b"frameset"),
            tag_index(b"isindex"),
            tag_index(b"strike"),
            tag_index(b"big"),
            tag_index(b"center"),
            tag_index(b"font"),
            tag_index(b"tt"),
        ]
    });
    for &idx in INDEXES.iter() {
        if idx == ni {
            bug(&format!(
                "<{}> is obsolete in HTML5.",
                TAGS[ni as usize].name
            ));
            break;
        }
    }
}
