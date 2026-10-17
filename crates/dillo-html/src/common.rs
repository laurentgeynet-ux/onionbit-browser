//! Port of `src/html_common.hh` — états partagés du parseur HTML.
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

use dillo_dw::WidgetId;
use dillo_dw::layout::Layout;
use dillo_net::url::DilloUrl;
use std::cell::RefCell;
use std::rc::Rc;

/// `TAB_SIZE` (html_common.hh:29).
pub const TAB_SIZE: i32 = 8;

/// `SGML_SPCDEL` (html.hh:53) — toujours false amont.
pub const SGML_SPCDEL: bool = false;

/// `IMAGE_MAX_AREA` (html.cc:96).
pub const IMAGE_MAX_AREA: i64 = 6000 * 6000;

// ------------------------------------------------------------------
// DilloHtmlDocumentType (html_common.hh:9)
// ------------------------------------------------------------------

/// `DilloHtmlDocumentType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocType {
    /// `None`.
    #[default]
    None,
    /// `Unrecognized` — DOCTYPE non reconnu (BUG_MSG émis).
    Unrecognized,
    /// `Html` — `major_version` ∈ {2,3,4,5} via `doc_type_version`.
    Html,
    /// `Xhtml`.
    Xhtml,
}

// ------------------------------------------------------------------
// DilloHtmlParseMode / TableMode / TableBorderMode / ListMode
// (html_common.hh:22-53)
// ------------------------------------------------------------------

/// `DilloHtmlParseMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseMode {
    /// `DILLO_HTML_PARSE_MODE_INIT`.
    Init,
    /// `DILLO_HTML_PARSE_MODE_STASH`.
    Stash,
    /// `DILLO_HTML_PARSE_MODE_STASH_AND_BODY`.
    StashAndBody,
    /// `DILLO_HTML_PARSE_MODE_VERBATIM`.
    Verbatim,
    /// `DILLO_HTML_PARSE_MODE_BODY`.
    Body,
    /// `DILLO_HTML_PARSE_MODE_PRE`.
    Pre,
}

/// `DilloHtmlTableMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableMode {
    /// `DILLO_HTML_TABLE_MODE_NONE`.
    None,
    /// `DILLO_HTML_TABLE_MODE_TOP`.
    Top,
    /// `DILLO_HTML_TABLE_MODE_TR`.
    Tr,
    /// `DILLO_HTML_TABLE_MODE_TD`.
    Td,
}

/// `DilloHtmlTableBorderMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableBorderMode {
    /// `DILLO_HTML_TABLE_BORDER_MODE_SEPARATE`.
    Separate,
    /// `DILLO_HTML_TABLE_BORDER_MODE_COLLAPSE`.
    Collapse,
}

/// `DilloHtmlListMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMode {
    /// `HTML_LIST_NONE`.
    None,
    /// `HTML_LIST_UNORDERED`.
    Unordered,
    /// `HTML_LIST_ORDERED`.
    Ordered,
}

// ------------------------------------------------------------------
// DilloHtmlProcessingState / InFlags (html_common.hh:89)
// ------------------------------------------------------------------

/// `InFlags` — bits verbatim.
pub mod in_flags {
    /// `IN_NONE`.
    pub const NONE: u32 = 0;
    /// `IN_HTML`.
    pub const HTML: u32 = 1;
    /// `IN_HEAD`.
    pub const HEAD: u32 = 1 << 1;
    /// `IN_BODY`.
    pub const BODY: u32 = 1 << 2;
    /// `IN_FORM`.
    pub const FORM: u32 = 1 << 3;
    /// `IN_SELECT`.
    pub const SELECT: u32 = 1 << 4;
    /// `IN_OPTION`.
    pub const OPTION: u32 = 1 << 5;
    /// `IN_OPTGROUP`.
    pub const OPTGROUP: u32 = 1 << 6;
    /// `IN_TEXTAREA`.
    pub const TEXTAREA: u32 = 1 << 7;
    /// `IN_BUTTON`.
    pub const BUTTON: u32 = 1 << 8;
    /// `IN_MAP`.
    pub const MAP: u32 = 1 << 9;
    /// `IN_PRE`.
    pub const PRE: u32 = 1 << 10;
    /// `IN_LI`.
    pub const LI: u32 = 1 << 11;
    /// `IN_MEDIA`.
    pub const MEDIA: u32 = 1 << 12;
    /// `IN_META_HACK`.
    pub const META_HACK: u32 = 1 << 13;
    /// `IN_A`.
    pub const A: u32 = 1 << 14;
    /// `IN_EOF`.
    pub const EOF: u32 = 1 << 15;
}

// ------------------------------------------------------------------
// DilloHtmlState (html_common.hh:55)
// ------------------------------------------------------------------

/// `DilloHtmlState` — entrée de pile. Les `*style` caches sont
/// remplacés par les appels `StyleEngine::*style` du port
/// (`StyleEngine` interne déjà le cache).
#[derive(Clone)]
pub struct HtmlState {
    /// `parse_mode`.
    pub parse_mode: ParseMode,
    /// `table_mode`.
    pub table_mode: TableMode,
    /// `table_border_mode`.
    pub table_border_mode: TableBorderMode,
    /// `cell_text_align_set`.
    pub cell_text_align_set: bool,
    /// `display_none` (style `display: none` hérité amont via
    /// `hand_over_display` + propagation `display_none`).
    pub display_none: bool,
    /// `list_type`.
    pub list_type: ListMode,
    /// `list_number`.
    pub list_number: i32,
    /// `tag_idx` — index dans `Tags`.
    pub tag_idx: i32,
    /// `textblock` — le widget courant (≙ `dw` dupliqué) —
    /// `None` ≙ `NULL` (`stack[0]` avant `initDw`).
    pub textblock: Option<WidgetId>,
    /// `table` — la table dw de ce niveau (`DILLO_HTML_TABLE_MODE_TOP`).
    pub table: Option<WidgetId>,
    /// `ref_list_item` — widget ListItem de référence pour le marker
    /// texte (verbatim `ref_list_item`, utilisé par
    /// `Textblock::addParbreak`/espaces ≙ parbreak de listitem).
    pub ref_list_item: Option<WidgetId>,
    /// `hand_over_break` — style amont récupéré.
    pub hand_over_break: bool,
}

// ------------------------------------------------------------------
// DilloHtmlImage (html_common.hh:78)
// ------------------------------------------------------------------

/// `DilloImage*` — données du widget image côté document
/// (`src/image.cc` complet = périmètre A09 ; ici l'handle porte le
/// `WidgetId` de l'arène et les couleurs verbatim).
pub struct HtmlImage {
    /// `Image->dw` ≙ widget `dw::Image`.
    pub widget: WidgetId,
    /// `Image->layout` ≙ le `Layout` partagé (pour ops `image_*`).
    pub layout: Rc<RefCell<Layout>>,
    /// `Image->bg_color`.
    pub bg_color: i32,
    /// `Image->fg_color`.
    pub fg_color: i32,
    /// `Image->width` / `Image->height` (`html.cc` les met à 0
    /// pour les `<img>` sans attributs size — verbatim).
    pub width: i32,
    /// `Image->height`.
    pub height: i32,
}

/// `DilloImage*` — refcount amont ≙ `Rc<RefCell<…>>`.
pub type ImageHandle = Rc<RefCell<HtmlImage>>;

/// `DilloHtmlImage` (html_common.hh:78) — entrée `html->images`.
pub struct HtmlImageEntry {
    /// `url`.
    pub url: DilloUrl,
    /// `image` — `None` ≙ `NULL` amont (chargement terminé).
    pub image: Option<ImageHandle>,
}
