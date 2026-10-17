//! Port de `dw/style.hh` et `dw/style.cc` (partiel — dessin de
//! bordures/background différé au lot suivant).
//!
//! Couverture : tous les enums, `Length` (packing verbatim),
//! `Box`, `StyleAttrs` (+`initValues`/`resetValues`/`sizeDiffs`/
//! `equals`/`hashValue`), `Style` + table de pooling,
//! `FontAttrs`/`Font`, `Tooltip`, `numtostr`, `Color`/`Shading`.
//! `StyleImage` + `drawBorder`/`drawBackground*` : lot dw-1b.
//!
//! Fidélité mémoire : amont pool les ressources (`Color`,
//! `Font`, `Tooltip`, `Style` via `styleTable`). Le port :
//! `Color` = valeur `Copy` (égalité valeur ≡ ptr-eq amont, le
//! pool garantissait l'unicité), `Font`/`Tooltip`/`StyleImage` =
//! `Rc` partagés, `Style` = `Rc` dédupliqué par une table
//! `thread_local` (≙ `styleTable` statique — dw est mono-
//! threadé amont).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::imgbuf::Imgbuf;
use crate::imgrenderer::ImgRenderer;
use crate::imgrenderer::ImgRendererDist;

// ------------------------------------------------------------------
// Enums (verbatim style.hh)
// ------------------------------------------------------------------

/// `dw::core::style::Cursor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Cursor {
    /// `CURSOR_CROSSHAIR`.
    Crosshair,
    /// `CURSOR_DEFAULT`.
    Default,
    /// `CURSOR_POINTER`.
    Pointer,
    /// `CURSOR_MOVE`.
    Move,
    /// `CURSOR_E_RESIZE`.
    EResize,
    /// `CURSOR_NE_RESIZE`.
    NeResize,
    /// `CURSOR_NW_RESIZE`.
    NwResize,
    /// `CURSOR_N_RESIZE`.
    NResize,
    /// `CURSOR_SE_RESIZE`.
    SeResize,
    /// `CURSOR_SW_RESIZE`.
    SwResize,
    /// `CURSOR_S_RESIZE`.
    SResize,
    /// `CURSOR_W_RESIZE`.
    WResize,
    /// `CURSOR_TEXT`.
    Text,
    /// `CURSOR_WAIT`.
    Wait,
    /// `CURSOR_HELP`.
    Help,
}

/// `dw::core::style::BorderCollapse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum BorderCollapse {
    /// `BORDER_MODEL_SEPARATE`.
    Separate,
    /// `BORDER_MODEL_COLLAPSE`.
    Collapse,
}

/// `dw::core::style::BorderStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum BorderStyle {
    /// `BORDER_NONE`.
    None,
    /// `BORDER_HIDDEN`.
    Hidden,
    /// `BORDER_DOTTED`.
    Dotted,
    /// `BORDER_DASHED`.
    Dashed,
    /// `BORDER_SOLID`.
    Solid,
    /// `BORDER_DOUBLE`.
    Double,
    /// `BORDER_GROOVE`.
    Groove,
    /// `BORDER_RIDGE`.
    Ridge,
    /// `BORDER_INSET`.
    Inset,
    /// `BORDER_OUTSET`.
    Outset,
}

/// `dw::core::style::BackgroundRepeat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum BackgroundRepeat {
    /// `BACKGROUND_REPEAT`.
    Repeat,
    /// `BACKGROUND_REPEAT_X`.
    RepeatX,
    /// `BACKGROUND_REPEAT_Y`.
    RepeatY,
    /// `BACKGROUND_NO_REPEAT`.
    NoRepeat,
}

/// `dw::core::style::BackgroundAttachment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum BackgroundAttachment {
    /// `BACKGROUND_ATTACHMENT_SCROLL`.
    Scroll,
    /// `BACKGROUND_ATTACHMENT_FIXED`.
    Fixed,
}

/// `dw::core::style::TextAlignType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TextAlignType {
    /// `TEXT_ALIGN_LEFT`.
    Left,
    /// `TEXT_ALIGN_RIGHT`.
    Right,
    /// `TEXT_ALIGN_CENTER`.
    Center,
    /// `TEXT_ALIGN_JUSTIFY`.
    Justify,
    /// `TEXT_ALIGN_STRING`.
    String,
}

/// `dw::core::style::VAlignType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VAlignType {
    /// `VALIGN_TOP`.
    Top,
    /// `VALIGN_BOTTOM`.
    Bottom,
    /// `VALIGN_MIDDLE`.
    Middle,
    /// `VALIGN_BASELINE`.
    Baseline,
    /// `VALIGN_SUB`.
    Sub,
    /// `VALIGN_SUPER`.
    Super,
    /// `VALIGN_TEXT_TOP`.
    TextTop,
    /// `VALIGN_TEXT_BOTTOM`.
    TextBottom,
}

/// `dw::core::style::TextTransform`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TextTransform {
    /// `TEXT_TRANSFORM_NONE`.
    None,
    /// `TEXT_TRANSFORM_CAPITALIZE`.
    Capitalize,
    /// `TEXT_TRANSFORM_UPPERCASE`.
    Uppercase,
    /// `TEXT_TRANSFORM_LOWERCASE`.
    Lowercase,
}

/// `dw::core::style::DisplayType` (incomplet amont).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum DisplayType {
    /// `DISPLAY_BLOCK`.
    Block,
    /// `DISPLAY_INLINE`.
    Inline,
    /// `DISPLAY_INLINE_BLOCK`.
    InlineBlock,
    /// `DISPLAY_LIST_ITEM`.
    ListItem,
    /// `DISPLAY_NONE`.
    None,
    /// `DISPLAY_TABLE`.
    Table,
    /// `DISPLAY_TABLE_ROW_GROUP`.
    TableRowGroup,
    /// `DISPLAY_TABLE_HEADER_GROUP`.
    TableHeaderGroup,
    /// `DISPLAY_TABLE_FOOTER_GROUP`.
    TableFooterGroup,
    /// `DISPLAY_TABLE_ROW`.
    TableRow,
    /// `DISPLAY_TABLE_CELL`.
    TableCell,
}

/// `dw::core::style::LineType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum LineType {
    /// `LINE_NORMAL`.
    Normal,
    /// `LINE_DOTTED`.
    Dotted,
    /// `LINE_DASHED`.
    Dashed,
}

/// `dw::core::style::ListStylePosition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ListStylePosition {
    /// `LIST_STYLE_POSITION_INSIDE`.
    Inside,
    /// `LIST_STYLE_POSITION_OUTSIDE`.
    Outside,
}

/// `dw::core::style::ListStyleType` — `PartialOrd` verbatim
/// (`listStyleType >= LIST_STYLE_TYPE_DECIMAL` amont teste les
/// types ordonnés sur l'ordre de déclaration de l'enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum ListStyleType {
    /// `LIST_STYLE_TYPE_DISC`.
    Disc,
    /// `LIST_STYLE_TYPE_CIRCLE`.
    Circle,
    /// `LIST_STYLE_TYPE_SQUARE`.
    Square,
    /// `LIST_STYLE_TYPE_DECIMAL`.
    Decimal,
    /// `LIST_STYLE_TYPE_DECIMAL_LEADING_ZERO`.
    DecimalLeadingZero,
    /// `LIST_STYLE_TYPE_LOWER_ROMAN`.
    LowerRoman,
    /// `LIST_STYLE_TYPE_UPPER_ROMAN`.
    UpperRoman,
    /// `LIST_STYLE_TYPE_LOWER_GREEK`.
    LowerGreek,
    /// `LIST_STYLE_TYPE_LOWER_ALPHA`.
    LowerAlpha,
    /// `LIST_STYLE_TYPE_LOWER_LATIN`.
    LowerLatin,
    /// `LIST_STYLE_TYPE_UPPER_ALPHA`.
    UpperAlpha,
    /// `LIST_STYLE_TYPE_UPPER_LATIN`.
    UpperLatin,
    /// `LIST_STYLE_TYPE_HEBREW`.
    Hebrew,
    /// `LIST_STYLE_TYPE_ARMENIAN`.
    Armenian,
    /// `LIST_STYLE_TYPE_GEORGIAN`.
    Georgian,
    /// `LIST_STYLE_TYPE_CJK_IDEOGRAPHIC`.
    CjkIdeographic,
    /// `LIST_STYLE_TYPE_HIRAGANA`.
    Hiragana,
    /// `LIST_STYLE_TYPE_KATAKANA`.
    Katakana,
    /// `LIST_STYLE_TYPE_HIRAGANA_IROHA`.
    HiraganaIroha,
    /// `LIST_STYLE_TYPE_KATAKANA_IROHA`.
    KatakanaIroha,
    /// `LIST_STYLE_TYPE_NONE`.
    None,
}

/// `dw::core::style::FontStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum FontStyle {
    /// `FONT_STYLE_NORMAL`.
    Normal,
    /// `FONT_STYLE_ITALIC`.
    Italic,
    /// `FONT_STYLE_OBLIQUE`.
    Oblique,
}

/// `dw::core::style::FontVariant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum FontVariant {
    /// `FONT_VARIANT_NORMAL`.
    Normal,
    /// `FONT_VARIANT_SMALL_CAPS`.
    SmallCaps,
}

/// `dw::core::style::Overflow`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Overflow {
    /// `OVERFLOW_VISIBLE`.
    Visible,
    /// `OVERFLOW_HIDDEN`.
    Hidden,
    /// `OVERFLOW_SCROLL`.
    Scroll,
    /// `OVERFLOW_AUTO`.
    Auto,
}

/// `dw::core::style::Position`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Position {
    /// `POSITION_STATIC`.
    Static,
    /// `POSITION_RELATIVE`.
    Relative,
    /// `POSITION_ABSOLUTE`.
    Absolute,
    /// `POSITION_FIXED`.
    Fixed,
}

/// `dw::core::style::TextDecoration` — masque de bits.
pub mod text_decoration {
    /// `TEXT_DECORATION_NONE`.
    pub const NONE: i32 = 0;
    /// `TEXT_DECORATION_UNDERLINE`.
    pub const UNDERLINE: i32 = 1 << 0;
    /// `TEXT_DECORATION_OVERLINE`.
    pub const OVERLINE: i32 = 1 << 1;
    /// `TEXT_DECORATION_LINE_THROUGH`.
    pub const LINE_THROUGH: i32 = 1 << 2;
    /// `TEXT_DECORATION_BLINK`.
    pub const BLINK: i32 = 1 << 3;
}

/// `dw::core::style::WhiteSpace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum WhiteSpace {
    /// `WHITE_SPACE_NORMAL`.
    Normal,
    /// `WHITE_SPACE_PRE`.
    Pre,
    /// `WHITE_SPACE_NOWRAP`.
    Nowrap,
    /// `WHITE_SPACE_PRE_WRAP`.
    PreWrap,
    /// `WHITE_SPACE_PRE_LINE`.
    PreLine,
}

/// `dw::core::style::FloatType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum FloatType {
    /// `FLOAT_NONE`.
    None,
    /// `FLOAT_LEFT`.
    Left,
    /// `FLOAT_RIGHT`.
    Right,
}

/// `dw::core::style::ClearType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ClearType {
    /// `CLEAR_LEFT`.
    Left,
    /// `CLEAR_RIGHT`.
    Right,
    /// `CLEAR_BOTH`.
    Both,
    /// `CLEAR_NONE`.
    None,
}

/// `Z_INDEX_AUTO = INT_MAX` (ne pas comparer aux z-index réels).
pub const Z_INDEX_AUTO: i32 = i32::MAX;

// ------------------------------------------------------------------
// Length — packing verbatim
// ------------------------------------------------------------------

/// `dw::core::style::Length` — `int` packé :
/// abs `n<<2|1`, per `(v*2^18 & ~3)|2`, rel `(… )|3`,
/// `LENGTH_AUTO = 0`.
pub type Length = i32;

/// `LENGTH_AUTO`.
pub const LENGTH_AUTO: Length = 0;

/// `createAbsLength (n)`.
#[must_use]
pub fn create_abs_length(n: i32) -> Length {
    (n << 2) | 1
}

/// `createPerLength (v)` — `v` relatif à 1.
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn create_per_length(v: f64) -> Length {
    ((v * f64::from(1 << 18)) as i32 & !3) | 2
}

/// `createRelLength (v)`.
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn create_rel_length(v: f64) -> Length {
    ((v * f64::from(1 << 18)) as i32 & !3) | 3
}

/// `isAbsLength`.
#[must_use]
pub fn is_abs_length(l: Length) -> bool {
    (l & 3) == 1
}

/// `isPerLength`.
#[must_use]
pub fn is_per_length(l: Length) -> bool {
    (l & 3) == 2
}

/// `isRelLength`.
#[must_use]
pub fn is_rel_length(l: Length) -> bool {
    (l & 3) == 3
}

/// `absLengthVal` — pixels.
#[must_use]
pub fn abs_length_val(l: Length) -> i32 {
    l >> 2
}

/// `perLengthVal_useThisOnlyForDebugging` — préférer
/// [`multiply_with_per_length`].
#[must_use]
pub fn per_length_val_debug_only(l: Length) -> f64 {
    f64::from(l & !3) / f64::from(1 << 18)
}

/// `relLengthVal`.
#[must_use]
pub fn rel_length_val(l: Length) -> f64 {
    f64::from(l & !3) / f64::from(1 << 18)
}

/// `multiplyWithPerLength` — `round(x * per)`.
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn multiply_with_per_length(x: i32, l: Length) -> i32 {
    (f64::from(x) * per_length_val_debug_only(l)).round() as i32
}

/// `lout::misc::roundInt` — `(int)((d>0) ? d+0.5 : d-0.5)`,
/// pas `round()` (demi-négatifs).
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn round_int(d: f64) -> i32 {
    if d > 0.0 {
        (d + 0.5) as i32
    } else {
        (d - 0.5) as i32
    }
}

/// `multiplyWithPerLengthRounded` — `roundInt` amont.
#[must_use]
pub fn multiply_with_per_length_rounded(x: i32, l: Length) -> i32 {
    round_int(f64::from(x) * per_length_val_debug_only(l))
}

/// `multiplyWithRelLength` — cast `int` amont = troncature.
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn multiply_with_rel_length(x: i32, l: Length) -> i32 {
    (f64::from(x) * rel_length_val(l)) as i32
}

// ------------------------------------------------------------------
// Box / Color
// ------------------------------------------------------------------

/// `dw::core::style::Box` — margin/borderWidth/padding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Box {
    /// `top`.
    pub top: i32,
    /// `right`.
    pub right: i32,
    /// `bottom`.
    pub bottom: i32,
    /// `left`.
    pub left: i32,
}

impl Box {
    /// `setVal`.
    pub fn set_val(&mut self, val: i32) {
        self.top = val;
        self.right = val;
        self.bottom = val;
        self.left = val;
    }

    /// `hashValue`.
    #[must_use]
    pub fn hash_value(&self) -> i64 {
        i64::from(self.top) + i64::from(self.right) + i64::from(self.bottom) + i64::from(self.left)
    }
}

/// `dw::core::style::Color` — couleur `0xRRGGBB` (≙
/// `ColorAttrs::color` + `Shading`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Color {
    color: i32,
}

/// `dw::core::style::Color::Shading`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Shading {
    /// `SHADING_NORMAL`.
    Normal,
    /// `SHADING_INVERSE`.
    Inverse,
    /// `SHADING_DARK`.
    Dark,
    /// `SHADING_LIGHT`.
    Light,
}

/// `SHADING_NUM`.
pub const SHADING_NUM: usize = 4;

impl Color {
    /// `ColorAttrs (color)` / `Color::create` (pool différé —
    /// valeur `Copy` équivalente).
    #[must_use]
    pub fn new(color: i32) -> Self {
        Color { color }
    }

    /// `getColor` — `0xRRGGBB`.
    #[must_use]
    pub fn get_color(&self) -> i32 {
        self.color
    }

    /// `shadeColor (color, d)` — verbatim : décale la luminosité
    /// (max/255) de ±0.4 près des extrêmes, ±0.2 sinon.
    /// Casts `(int)` amont → troncature volontaire.
    #[allow(clippy::cast_possible_truncation)]
    fn shade_color_int(color: i32, d: i32) -> i32 {
        let red = (color >> 16) & 255;
        let green = (color >> 8) & 255;
        let blue = color & 255;

        let old_lightness = f64::from(red.max(green).max(blue)) / 255.0;
        let new_lightness = if old_lightness > 0.8 {
            if d > 0 {
                old_lightness - 0.2
            } else {
                old_lightness - 0.4
            }
        } else if old_lightness < 0.2 {
            if d > 0 {
                old_lightness + 0.4
            } else {
                old_lightness + 0.2
            }
        } else {
            old_lightness + f64::from(d) * 0.2
        };

        let (red, green, blue) = if old_lightness == 0.0 {
            let v = (new_lightness * 255.0) as i32;
            (v, v, v)
        } else {
            let f = new_lightness / old_lightness;
            (
                (f64::from(red) * f) as i32,
                (f64::from(green) * f) as i32,
                (f64::from(blue) * f) as i32,
            )
        };

        (red << 16) | (green << 8) | blue
    }

    /// `shadeColor (color, Shading)` — `SHADING_NORMAL` →
    /// identité, `INVERSE` → `^ 0xffffff`, `LIGHT`/`DARK` → ±1.
    #[must_use]
    pub fn shade(&self, shading: Shading) -> Color {
        let c = match shading {
            Shading::Normal => self.color,
            Shading::Light => Self::shade_color_int(self.color, 1),
            Shading::Inverse => self.color ^ 0xff_ffff,
            Shading::Dark => Self::shade_color_int(self.color, -1),
        };
        Color { color: c }
    }
}

// ------------------------------------------------------------------
// Font / Tooltip
// ------------------------------------------------------------------

/// `dw::core::style::FontAttrs` — attributs de création.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontAttrs {
    /// `name`.
    pub name: Vec<u8>,
    /// `size`.
    pub size: i32,
    /// `weight`.
    pub weight: i32,
    /// `letterSpacing`.
    pub letter_spacing: i32,
    /// `fontVariant`.
    pub font_variant: FontVariant,
    /// `style`.
    pub style: FontStyle,
}

impl FontAttrs {
    /// `hashValue` — `h*31` accumulé (≙ `(h<<5)-h`) sur
    /// string-hash + champs.
    #[must_use]
    pub fn hash_value(&self) -> i64 {
        let mut h: i64 = string_hash(&self.name);
        h = (h << 5) - h + i64::from(self.size);
        h = (h << 5) - h + i64::from(self.weight);
        h = (h << 5) - h + i64::from(self.style as i32);
        h = (h << 5) - h + i64::from(self.letter_spacing);
        (h << 5) - h + i64::from(self.font_variant as i32)
    }
}

/// `lout::object::String::hashValue` (djb2 — vérifié contre
/// lout amont).
fn string_hash(s: &[u8]) -> i64 {
    let mut h: i64 = 5381;
    for &c in s {
        h = ((h << 5) + h + i64::from(c)) & 0x7fff_ffff;
    }
    h
}

/// `dw::core::style::Font` — attrs + métriques (fournies par
/// `Platform::createFont`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    /// Attributs (≙ `FontAttrs` hérités).
    pub attrs: FontAttrs,
    /// `ascent`.
    pub ascent: i32,
    /// `descent`.
    pub descent: i32,
    /// `spaceWidth`.
    pub space_width: i32,
    /// `xHeight`.
    pub x_height: i32,
    /// `zeroWidth`.
    pub zero_width: i32,
}

/// `dw::core::style::Tooltip` — texte ; les hooks
/// `onEnter`/`onLeave`/`onMotion` sont implémentés par la
/// plateforme (`ui.cc`/`fltkui` amont) — le port expose
/// `on_*` via un trait [`TooltipHooks`] optionnel.
pub struct Tooltip {
    /// Texte du tooltip.
    pub text: Rc<[u8]>,
    /// Hooks plateforme (≙ méthodes virtuelles amont).
    pub hooks: Option<Rc<dyn TooltipHooks>>,
}

/// Callbacks de `Tooltip` (≙ `virtual on*` amont).
pub trait TooltipHooks {
    /// `onEnter`.
    fn on_enter(&self);
    /// `onLeave`.
    fn on_leave(&self);
    /// `onMotion`.
    fn on_motion(&self);
}

impl Tooltip {
    /// `Tooltip::create` — hors pooling amont (chaque appel
    /// crée un nouvel objet plateforme).
    #[must_use]
    pub fn create(text: &[u8], hooks: Option<Rc<dyn TooltipHooks>>) -> Rc<Tooltip> {
        Rc::new(Tooltip {
            text: Rc::from(text),
            hooks,
        })
    }

    /// `onEnter`.
    pub fn on_enter(&self) {
        if let Some(h) = &self.hooks {
            h.on_enter();
        }
    }

    /// `onLeave`.
    pub fn on_leave(&self) {
        if let Some(h) = &self.hooks {
            h.on_leave();
        }
    }

    /// `onMotion`.
    pub fn on_motion(&self) {
        if let Some(h) = &self.hooks {
            h.on_motion();
        }
    }
}

// ------------------------------------------------------------------
// StyleAttrs / Style
// ------------------------------------------------------------------

/// `dw::core::style::StyleAttrs` — tous les attributs de style.
/// Pointeurs amont → `Option<Rc<…>>` (`backgroundColor`/`color`
/// restent des valeurs — `Color` n'est pas un trait object).
#[derive(Clone)]
pub struct StyleAttrs {
    /// `font`.
    pub font: Option<Rc<Font>>,
    /// `textDecoration` (int amont — pas l'enum).
    pub text_decoration: i32,
    /// `color`.
    pub color: Option<Color>,
    /// `backgroundColor`.
    pub background_color: Option<Color>,
    /// `backgroundImage`.
    pub background_image: Option<Rc<RefCell<StyleImage>>>,
    /// `backgroundRepeat`.
    pub background_repeat: BackgroundRepeat,
    /// `backgroundAttachment`.
    pub background_attachment: BackgroundAttachment,
    /// `backgroundPositionX` (« left » ≙ « 0% »).
    pub background_position_x: Length,
    /// `backgroundPositionY`.
    pub background_position_y: Length,
    /// `textAlign`.
    pub text_align: TextAlignType,
    /// `valign`.
    pub valign: VAlignType,
    /// `textAlignChar`.
    pub text_align_char: u8,
    /// `textTransform`.
    pub text_transform: TextTransform,
    /// `vloat` (« float » est un mot-clé amont).
    pub vloat: FloatType,
    /// `clear`.
    pub clear: ClearType,
    /// `overflow`.
    pub overflow: Overflow,
    /// `position`.
    pub position: Position,
    /// `top`.
    pub top: Length,
    /// `bottom`.
    pub bottom: Length,
    /// `left`.
    pub left: Length,
    /// `right`.
    pub right: Length,
    /// `hBorderSpacing`.
    pub h_border_spacing: i32,
    /// `vBorderSpacing`.
    pub v_border_spacing: i32,
    /// `wordSpacing`.
    pub word_spacing: i32,
    /// `width`.
    pub width: Length,
    /// `height`.
    pub height: Length,
    /// `lineHeight`.
    pub line_height: Length,
    /// `textIndent`.
    pub text_indent: Length,
    /// `minWidth`.
    pub min_width: Length,
    /// `maxWidth`.
    pub max_width: Length,
    /// `minHeight`.
    pub min_height: Length,
    /// `maxHeight`.
    pub max_height: Length,
    /// `margin`.
    pub margin: Box,
    /// `borderWidth`.
    pub border_width: Box,
    /// `padding`.
    pub padding: Box,
    /// `borderCollapse`.
    pub border_collapse: BorderCollapse,
    /// `borderColor.{top,right,bottom,left}`.
    pub border_color: [Option<Color>; 4],
    /// `borderStyle.{top,right,bottom,left}`.
    pub border_style: [BorderStyle; 4],
    /// `display`.
    pub display: DisplayType,
    /// `whiteSpace`.
    pub white_space: WhiteSpace,
    /// `listStylePosition`.
    pub list_style_position: ListStylePosition,
    /// `listStyleType`.
    pub list_style_type: ListStyleType,
    /// `cursor`.
    pub cursor: Cursor,
    /// `zIndex`.
    pub z_index: i32,
    /// `x_link`.
    pub x_link: i32,
    /// `x_img`.
    pub x_img: i32,
    /// `x_tooltip`.
    pub x_tooltip: Option<Rc<Tooltip>>,
    /// `x_lang[2]` — code RFC 1766 minuscule ou `[0,0]`.
    pub x_lang: [u8; 2],
}

impl Default for StyleAttrs {
    /// `initValues` amont (≙ valeurs par défaut).
    fn default() -> Self {
        let a = StyleAttrs {
            font: None,
            text_decoration: text_decoration::NONE,
            color: None,
            background_color: None,
            background_image: None,
            background_repeat: BackgroundRepeat::Repeat,
            background_attachment: BackgroundAttachment::Scroll,
            background_position_x: create_per_length(0.0),
            background_position_y: create_per_length(0.0),
            text_align: TextAlignType::Left,
            valign: VAlignType::Baseline,
            text_align_char: b'.',
            text_transform: TextTransform::None,
            vloat: FloatType::None,
            clear: ClearType::None,
            overflow: Overflow::Visible,
            position: Position::Static,
            top: LENGTH_AUTO,
            bottom: LENGTH_AUTO,
            left: LENGTH_AUTO,
            right: LENGTH_AUTO,
            h_border_spacing: 0,
            v_border_spacing: 0,
            word_spacing: 0,
            width: LENGTH_AUTO,
            height: LENGTH_AUTO,
            line_height: LENGTH_AUTO,
            text_indent: 0,
            min_width: LENGTH_AUTO,
            max_width: LENGTH_AUTO,
            min_height: LENGTH_AUTO,
            max_height: LENGTH_AUTO,
            margin: Box::default(),
            border_width: Box::default(),
            padding: Box::default(),
            border_collapse: BorderCollapse::Separate,
            border_color: [None; 4],
            border_style: [BorderStyle::None; 4],
            display: DisplayType::Inline,
            white_space: WhiteSpace::Normal,
            list_style_position: ListStylePosition::Outside,
            list_style_type: ListStyleType::Disc,
            cursor: Cursor::Default,
            z_index: Z_INDEX_AUTO,
            x_link: -1,
            x_img: -1,
            x_tooltip: None,
            x_lang: [0, 0],
        };
        // `initValues` pose x_lang[0]=x_lang[1]=0 — déjà fait.
        a
    }
}

impl StyleAttrs {
    /// `initValues`.
    #[must_use]
    pub fn init_values() -> Self {
        StyleAttrs::default()
    }

    /// `resetValues` — remet les attributs *non hérités* CSS à
    /// leurs valeurs standard (verbatim — `font`, couleurs texte
    /// et héritables conservés).
    pub fn reset_values(&mut self) {
        self.x_img = -1;
        self.valign = VAlignType::Baseline;
        self.text_align_char = b'.';
        self.vloat = FloatType::None;
        self.clear = ClearType::None;
        self.overflow = Overflow::Visible;
        self.position = Position::Static;
        self.top = LENGTH_AUTO;
        self.bottom = LENGTH_AUTO;
        self.left = LENGTH_AUTO;
        self.right = LENGTH_AUTO;
        self.background_color = None;
        self.background_image = None;
        self.background_repeat = BackgroundRepeat::Repeat;
        self.background_attachment = BackgroundAttachment::Scroll;
        self.background_position_x = create_per_length(0.0);
        self.background_position_y = create_per_length(0.0);
        self.width = LENGTH_AUTO;
        self.height = LENGTH_AUTO;
        self.min_width = LENGTH_AUTO;
        self.max_width = LENGTH_AUTO;
        self.min_height = LENGTH_AUTO;
        self.max_height = LENGTH_AUTO;
        self.margin.set_val(0);
        self.border_width.set_val(0);
        self.padding.set_val(0);
        self.set_border_color(None);
        self.set_border_style(BorderStyle::None);
        self.h_border_spacing = 0;
        self.v_border_spacing = 0;
        self.display = DisplayType::Inline;
    }

    /// `sizeDiffs` — amont retourne toujours `false` (TODO CSS).
    #[must_use]
    pub fn size_diffs(&self, _other: &StyleAttrs) -> bool {
        false
    }

    /// `setBorderColor`.
    pub fn set_border_color(&mut self, val: Option<Color>) {
        self.border_color = [val; 4];
    }

    /// `setBorderStyle`.
    pub fn set_border_style(&mut self, val: BorderStyle) {
        self.border_style = [val; 4];
    }

    /// `boxOffsetX`.
    #[must_use]
    pub fn box_offset_x(&self) -> i32 {
        self.margin.left + self.border_width.left + self.padding.left
    }

    /// `boxRestWidth`.
    #[must_use]
    pub fn box_rest_width(&self) -> i32 {
        self.margin.right + self.border_width.right + self.padding.right
    }

    /// `boxDiffWidth`.
    #[must_use]
    pub fn box_diff_width(&self) -> i32 {
        self.box_offset_x() + self.box_rest_width()
    }

    /// `boxOffsetY`.
    #[must_use]
    pub fn box_offset_y(&self) -> i32 {
        self.margin.top + self.border_width.top + self.padding.top
    }

    /// `boxRestHeight`.
    #[must_use]
    pub fn box_rest_height(&self) -> i32 {
        self.margin.bottom + self.border_width.bottom + self.padding.bottom
    }

    /// `boxDiffHeight`.
    #[must_use]
    pub fn box_diff_height(&self) -> i32 {
        self.box_offset_y() + self.box_rest_height()
    }

    /// `hasBackground`.
    #[must_use]
    pub fn has_background(&self) -> bool {
        self.background_color.is_some() || self.background_image.is_some()
    }

    /// `equals` — identité `Rc` pour font/image/tooltip
    /// (≙ pointeurs poolés), valeur pour le reste.
    #[must_use]
    pub fn equals(&self, other: &StyleAttrs) -> bool {
        fn rc_eq<T>(a: Option<&Rc<T>>, b: Option<&Rc<T>>) -> bool {
            match (a, b) {
                (None, None) => true,
                (Some(x), Some(y)) => Rc::ptr_eq(x, y),
                _ => false,
            }
        }
        fn rcc_eq<T: ?Sized>(a: Option<&Rc<RefCell<T>>>, b: Option<&Rc<RefCell<T>>>) -> bool {
            match (a, b) {
                (None, None) => true,
                (Some(x), Some(y)) => Rc::ptr_eq(x, y),
                _ => false,
            }
        }
        rc_eq(self.font.as_ref(), other.font.as_ref())
            && self.text_decoration == other.text_decoration
            && self.color == other.color
            && self.background_color == other.background_color
            && rcc_eq(
                self.background_image.as_ref(),
                other.background_image.as_ref(),
            )
            && self.background_repeat == other.background_repeat
            && self.background_attachment == other.background_attachment
            && self.background_position_x == other.background_position_x
            && self.background_position_y == other.background_position_y
            && self.text_align == other.text_align
            && self.valign == other.valign
            && self.text_align_char == other.text_align_char
            && self.text_transform == other.text_transform
            && self.vloat == other.vloat
            && self.clear == other.clear
            && self.overflow == other.overflow
            && self.position == other.position
            && self.top == other.top
            && self.bottom == other.bottom
            && self.left == other.left
            && self.right == other.right
            && self.h_border_spacing == other.h_border_spacing
            && self.v_border_spacing == other.v_border_spacing
            && self.word_spacing == other.word_spacing
            && self.width == other.width
            && self.height == other.height
            && self.min_width == other.min_width
            && self.max_width == other.max_width
            && self.min_height == other.min_height
            && self.max_height == other.max_height
            && self.line_height == other.line_height
            && self.text_indent == other.text_indent
            && self.margin == other.margin
            && self.border_width == other.border_width
            && self.padding == other.padding
            && self.border_collapse == other.border_collapse
            && self.border_color == other.border_color
            && self.border_style == other.border_style
            && self.display == other.display
            && self.white_space == other.white_space
            && self.list_style_position == other.list_style_position
            && self.list_style_type == other.list_style_type
            && self.cursor == other.cursor
            && self.z_index == other.z_index
            && self.x_link == other.x_link
            && self.x_lang == other.x_lang
            && self.x_img == other.x_img
            && rc_eq(self.x_tooltip.as_ref(), other.x_tooltip.as_ref())
    }

    /// `hashValue` — pointeurs `(intptr_t)` → adresses `Rc`,
    /// somme verbatim (wrap ptr→i64 intentionnel : clé de hachage).
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub fn hash_value(&self) -> i64 {
        fn ptr<T>(o: Option<&Rc<T>>) -> i64 {
            #[allow(clippy::cast_possible_wrap)]
            o.map_or(0, |r| Rc::as_ptr(r) as usize as i64)
        }
        fn ptrc<T: ?Sized>(o: Option<&Rc<RefCell<T>>>) -> i64 {
            #[allow(clippy::cast_possible_wrap)]
            o.map_or(0, |r| Rc::as_ptr(r).cast::<()>() as usize as i64)
        }
        ptr(self.font.as_ref())
            + i64::from(self.text_decoration)
            + i64::from(self.color.map_or(0, |c| c.get_color()))
            + i64::from(self.background_color.map_or(0, |c| c.get_color()))
            + ptrc(self.background_image.as_ref())
            + i64::from(self.background_repeat as i32)
            + i64::from(self.background_attachment as i32)
            + i64::from(self.background_position_x)
            + i64::from(self.background_position_y)
            + i64::from(self.text_align as i32)
            + i64::from(self.valign as i32)
            + i64::from(self.text_align_char)
            + i64::from(self.text_transform as i32)
            + i64::from(self.vloat as i32)
            + i64::from(self.clear as i32)
            + i64::from(self.overflow as i32)
            + i64::from(self.position as i32)
            + i64::from(self.top)
            + i64::from(self.bottom)
            + i64::from(self.left)
            + i64::from(self.right)
            + i64::from(self.h_border_spacing)
            + i64::from(self.v_border_spacing)
            + i64::from(self.word_spacing)
            + i64::from(self.width)
            + i64::from(self.height)
            + i64::from(self.min_width)
            + i64::from(self.max_width)
            + i64::from(self.min_height)
            + i64::from(self.max_height)
            + i64::from(self.line_height)
            + i64::from(self.text_indent)
            + self.margin.hash_value()
            + self.border_width.hash_value()
            + self.padding.hash_value()
            + i64::from(self.border_collapse as i32)
            + i64::from(self.border_color[0].map_or(0, |c| c.get_color()))
            + i64::from(self.border_color[1].map_or(0, |c| c.get_color()))
            + i64::from(self.border_color[2].map_or(0, |c| c.get_color()))
            + i64::from(self.border_color[3].map_or(0, |c| c.get_color()))
            + i64::from(self.border_style[0] as i32)
            + i64::from(self.border_style[1] as i32)
            + i64::from(self.border_style[2] as i32)
            + i64::from(self.border_style[3] as i32)
            + i64::from(self.display as i32)
            + i64::from(self.white_space as i32)
            + i64::from(self.list_style_position as i32)
            + i64::from(self.list_style_type as i32)
            + i64::from(self.cursor as i32)
            + i64::from(self.z_index)
            + i64::from(self.x_link)
            + i64::from(self.x_lang[0])
            + i64::from(self.x_lang[1])
            + i64::from(self.x_img)
            + ptr(self.x_tooltip.as_ref())
    }
}

/// Clé de la table de styles — l'upstream `HashTable<StyleAttrs,
/// Style>` hache sur `StyleAttrs::hashValue` + `equals`.
struct StyleKey {
    attrs: StyleAttrs,
}

/// Bucket de la table (clés de hash collisions possibles — on
/// re-vérifie avec `equals` comme `HashTable` amont).
type StyleBucket = Vec<(StyleKey, Weak<Style>)>;

thread_local! {
    // `styleTable` statique amont (mono-thread — dw est
    // entièrement UI-thread).
    static STYLE_TABLE: RefCell<HashMap<u64, StyleBucket>> =
        RefCell::new(HashMap::new());
}

/// `dw::core::style::Style` — attrs immuables partagés via `Rc`.
/// `ref`/`unref` amont → comptage `Rc`.
pub struct Style {
    /// Attributs (≙ `StyleAttrs` hérités + `copyAttrs`).
    pub attrs: StyleAttrs,
}

#[cfg(test)]
thread_local! {
    // `Style::totalRef` amont (compteur global de styles vivants —
    // utilisé par `test_dw_style.cc` pour détecter les fuites).
    static TOTAL_REF: RefCell<i64> = const { RefCell::new(0) };
}

impl std::fmt::Debug for Style {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Style").finish_non_exhaustive()
    }
}

impl Style {
    /// `Style::create` — déduplication par `equals` (≙
    /// `styleTable->get`). Les entrées `Weak` mortes sont
    /// purgées au passage.
    #[must_use]
    pub fn create(attrs: &StyleAttrs) -> Rc<Style> {
        #[allow(clippy::cast_sign_loss)]
        let hash = attrs.hash_value() as u64;
        STYLE_TABLE.with(|t| {
            let mut t = t.borrow_mut();
            let bucket = t.entry(hash).or_default();
            bucket.retain(|(_, w)| w.strong_count() > 0);
            for (k, w) in bucket.iter() {
                if k.attrs.equals(attrs) {
                    if let Some(s) = w.upgrade() {
                        return s;
                    }
                }
            }
            #[cfg(test)]
            TOTAL_REF.with(|n| *n.borrow_mut() += 1);
            let style = Rc::new(Style {
                attrs: attrs.clone(),
            });
            bucket.push((
                StyleKey {
                    attrs: attrs.clone(),
                },
                Rc::downgrade(&style),
            ));
            style
        })
    }

    /// `totalRef` — nombre de styles vivants (tests).
    #[cfg(test)]
    #[must_use]
    pub fn total_ref() -> i64 {
        TOTAL_REF.with(|n| *n.borrow())
    }
}

impl std::ops::Deref for Style {
    type Target = StyleAttrs;
    fn deref(&self) -> &StyleAttrs {
        &self.attrs
    }
}

impl std::ops::Drop for Style {
    /// `~Style` — retire de la table (≙ `styleTable->remove`).
    /// Les `Weak` morts sont purgés à la prochaine `create` —
    /// rien à faire ici.
    fn drop(&mut self) {
        #[cfg(test)]
        TOTAL_REF.with(|n| *n.borrow_mut() -= 1);
    }
}

// ------------------------------------------------------------------
// StyleImage (partiel — le corps draw/tile arrive avec
// drawBackground* au lot dw-1b)
// ------------------------------------------------------------------

/// `dw::core::style::StyleImage` — image de background partagée
/// (`refCount` amont → `Rc`).
pub struct StyleImage {
    /// `tilesX` — tuiles du buffer optimisé.
    pub tiles_x: i32,
    /// `tilesY`.
    pub tiles_y: i32,
    /// `imgbufSrc`.
    pub imgbuf_src: Option<Rc<dyn Imgbuf>>,
    /// `imgbufTiled`.
    pub imgbuf_tiled: Option<Rc<dyn Imgbuf>>,
    /// `imgRendererDist` (`Rc` : accès typé `put`/`remove`).
    img_renderer_dist: Rc<RefCell<ImgRendererDist>>,
    /// Le même dist coercé en `dyn ImgRenderer` (amont retourne
    /// `ImgRenderer*` — ≙ `getMainImgRenderer`).
    main_renderer: Rc<RefCell<dyn ImgRenderer>>,
}

/// `MIN_BG_IMG_W/H` + `OPT_BG_IMG_W/H` — seuils du buffer « tiled »
/// (style.cc:37-40, verbatim).
const MIN_BG_IMG_W: i32 = 10;
const MIN_BG_IMG_H: i32 = 10;
const OPT_BG_IMG_W: i32 = 50;
const OPT_BG_IMG_H: i32 = 50;

/// `drawBackgroundLineByLine` — `false` amont (style.cc:35) :
/// `finish` dessine la zone entière ; le chemin ligne-à-ligne est
/// porté pour complétude (constant → branch mort, fidèle).
const DRAW_BACKGROUND_LINE_BY_LINE: bool = false;

/// `StyleImage::StyleImgRenderer` — renderer interne qui reçoit le
/// buffer source et maintient le buffer « tiled ». `Weak` vers
/// l'image ≙ pointeur brut amont (évite le cycle `Rc`).
struct StyleImgRenderer {
    image: Weak<RefCell<StyleImage>>,
}

impl ImgRenderer for StyleImgRenderer {
    /// `StyleImgRenderer::setBuffer` — verbatim style.cc:542 :
    /// remplace `imgbufSrc`, détruit `imgbufTiled`, reconstruit le
    /// buffer tuilé si `src < MIN²` (`tilesX/Y = OPT/size`, ≥1).
    fn set_buffer(&mut self, buffer: &Rc<dyn Imgbuf>, _resize: bool) {
        let Some(image_rc) = self.image.upgrade() else {
            return;
        };
        let mut image = image_rc.borrow_mut();
        image.imgbuf_tiled = None;
        image.imgbuf_src = Some(Rc::clone(buffer));
        let src = buffer;
        if src.get_root_width() * src.get_root_height() < MIN_BG_IMG_W * MIN_BG_IMG_H {
            image.tiles_x = (OPT_BG_IMG_W / src.get_root_width()).max(1);
            image.tiles_y = (OPT_BG_IMG_H / src.get_root_height()).max(1);
            image.imgbuf_tiled = Some(src.create_similar_buf(
                image.tiles_x * src.get_root_width(),
                image.tiles_y * src.get_root_height(),
            ));
        }
    }

    /// `StyleImgRenderer::drawRow` — recopie la ligne `row` du
    /// buffer source dans toutes les tuiles du buffer tuilé.
    /// Note amont (style.cc:596) : appelable *après* d'autres
    /// `drawRow` qui dessinent — sans gravité car le dessin réel
    /// est différé par `queueResize`.
    fn draw_row(&mut self, row: i32) {
        let Some(image_rc) = self.image.upgrade() else {
            return;
        };
        let image = image_rc.borrow();
        if let (Some(tiled), Some(src)) = (&image.imgbuf_tiled, &image.imgbuf_src) {
            let w = src.get_root_width();
            let h = src.get_root_height();
            for x in 0..image.tiles_x {
                for y in 0..image.tiles_y {
                    src.copy_to(&**tiled, x * w, y * h, 0, row, w, 1);
                }
            }
        }
    }

    /// `finish` — rien (verbatim).
    fn finish(&mut self) {}

    /// `fatal` — rien (verbatim).
    fn fatal(&mut self) {}
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl StyleImage {
    /// `StyleImage::create` — `imgRendererDist->put
    /// (styleImgRenderer)` au constructeur (style.cc:626-628).
    #[must_use]
    pub fn create() -> Rc<RefCell<StyleImage>> {
        let dist = Rc::new(RefCell::new(ImgRendererDist::new()));
        let main: Rc<RefCell<dyn ImgRenderer>> = dist.clone();
        let image = Rc::new(RefCell::new(StyleImage {
            tiles_x: 0,
            tiles_y: 0,
            imgbuf_src: None,
            imgbuf_tiled: None,
            img_renderer_dist: dist,
            main_renderer: main,
        }));
        let sir: Rc<RefCell<dyn ImgRenderer>> = Rc::new(RefCell::new(StyleImgRenderer {
            image: Rc::downgrade(&image),
        }));
        image.borrow().img_renderer_dist.borrow_mut().put(&sir);
        image
    }

    /// `getMainImgRenderer` — le distributeur (≙ `imgRendererDist`).
    #[must_use]
    pub fn get_main_img_renderer(&self) -> Rc<RefCell<dyn ImgRenderer>> {
        Rc::clone(&self.main_renderer)
    }

    /// `getImgbufSrc`.
    #[must_use]
    pub fn get_imgbuf_src(&self) -> Option<Rc<dyn Imgbuf>> {
        self.imgbuf_src.clone()
    }

    /// `getImgbufTiled (repeatX, repeatY)` — verbatim.
    #[must_use]
    pub fn get_imgbuf_tiled(&self, repeat_x: bool, repeat_y: bool) -> Option<Rc<dyn Imgbuf>> {
        if self.imgbuf_tiled.is_some() && repeat_x && repeat_y {
            self.imgbuf_tiled.clone()
        } else {
            self.imgbuf_src.clone()
        }
    }

    /// `getTilesX`.
    #[must_use]
    pub fn get_tiles_x(&self, repeat_x: bool, repeat_y: bool) -> i32 {
        if self.imgbuf_tiled.is_some() && repeat_x && repeat_y {
            self.tiles_x
        } else {
            1
        }
    }

    /// `getTilesY`.
    #[must_use]
    pub fn get_tiles_y(&self, repeat_x: bool, repeat_y: bool) -> i32 {
        if self.imgbuf_tiled.is_some() && repeat_x && repeat_y {
            self.tiles_y
        } else {
            1
        }
    }

    /// `putExternalImgRenderer`.
    pub fn put_external_img_renderer(&self, ir: &Rc<RefCell<dyn ImgRenderer>>) {
        self.img_renderer_dist.borrow_mut().put(ir);
    }

    /// `removeExternalImgRenderer`.
    pub fn remove_external_img_renderer(&self, ir: &Rc<RefCell<dyn ImgRenderer>>) {
        self.img_renderer_dist.borrow_mut().remove(ir);
    }
}

// ------------------------------------------------------------------
// numtostr — verbatim
// ------------------------------------------------------------------

/// `numtostr` — formatage des compteurs de liste
/// (`%d.` / alpha / romain). Amont écrit dans un buffer C ;
/// le port retourne `String` (les appelants à buffer fixe
/// tronquent au besoin — note de fidélité : l'overflow produit
/// `"****."` verbatim).
#[must_use]
pub fn num_to_str(num: i32, list_style_type: ListStyleType) -> String {
    const ROMAN_I0: [&str; 10] = ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX"];
    const ROMAN_I1: [&str; 10] = ["", "X", "XX", "XXX", "XL", "L", "LX", "LXX", "LXXX", "XC"];
    const ROMAN_I2: [&str; 10] = ["", "C", "CC", "CCC", "CD", "D", "DC", "DCC", "DCCC", "CM"];
    const ROMAN_I3: [&str; 5] = ["", "M", "MM", "MMM", "MMMM"];

    match list_style_type {
        ListStyleType::LowerAlpha | ListStyleType::LowerLatin => alpha_num(num, b'a'),
        ListStyleType::UpperAlpha | ListStyleType::UpperLatin => alpha_num(num, b'A'),
        ListStyleType::LowerRoman => {
            roman_num(num, &ROMAN_I0, &ROMAN_I1, &ROMAN_I2, &ROMAN_I3).to_lowercase()
        }
        ListStyleType::UpperRoman => roman_num(num, &ROMAN_I0, &ROMAN_I1, &ROMAN_I2, &ROMAN_I3),
        _ => format!("{num}."),
    }
}

/// Branche alpha de `numtostr` (`%c%c%c.` avec ' ' pour digit
/// absent). Casts bornés (i % 26 ∈ 0..25).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn alpha_num(num: i32, start_ch: u8) -> String {
    let i0 = num - 1;
    let i1 = i0 / 26 - 1;
    let i2 = i1 / 26 - 1;
    if i2 > 25 {
        "****.".to_string()
    } else {
        let ch = |i: i32| -> char {
            if i < 0 {
                ' '
            } else {
                (start_ch + (i % 26) as u8) as char
            }
        };
        format!("{}{}{}.", ch(i2), ch(i1), ch(i0))
    }
}

/// Branche romaine de `numtostr`. Index bornés (i3 ≤ 4 et digits
/// ∈ 0..9 garantis par la garde).
#[allow(clippy::cast_sign_loss)]
fn roman_num(
    num: i32,
    i0s: &[&str; 10],
    i1s: &[&str; 10],
    i2s: &[&str; 10],
    i3s: &[&str; 5],
) -> String {
    let i0 = num;
    let i1 = i0 / 10;
    let i2 = i1 / 10;
    let i3 = i2 / 10;
    let d0 = i0 % 10;
    let d1 = i1 % 10;
    let d2 = i2 % 10;
    if num < 0 || i3 > 4 {
        "****.".to_string()
    } else {
        format!(
            "{}{}{}{}.",
            i3s[i3 as usize], i2s[d2 as usize], i1s[d1 as usize], i0s[d0 as usize]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shade_normal_and_inverse() {
        let c = Color::new(0x80_4020);
        assert_eq!(c.shade(Shading::Normal).get_color(), 0x80_4020);
        assert_eq!(c.shade(Shading::Inverse).get_color(), 0x7f_bfdf);
    }

    #[test]
    fn shade_light_dark_midrange() {
        // 0x808080 → lightness 128/255 ≈ 0.502 (midrange)
        let c = Color::new(0x80_8080);
        // f = 1 + 0.2*255/128 → 128*f = 179 exactement → 0xb3b3b3
        assert_eq!(c.shade(Shading::Light).get_color(), 0xb3_b3b3);
        // f = 1 - 0.2*255/128 → 128*f = 77 exactement → 0x4d4d4d
        assert_eq!(c.shade(Shading::Dark).get_color(), 0x4d_4d4d);
    }

    #[test]
    fn shade_extremes() {
        // blanc : oldLightness 1.0 > 0.8, d>0 → 0.8 → 0xcccccc
        assert_eq!(
            Color::new(0xff_ffff).shade(Shading::Light).get_color(),
            0xcc_cccc
        );
        // noir : oldLightness 0 < 0.2, d>0 → 0.4 → 102 = 0x666666
        assert_eq!(Color::new(0).shade(Shading::Light).get_color(), 0x66_6666);
    }

    #[test]
    fn length_packing() {
        let a = create_abs_length(42);
        assert!(is_abs_length(a));
        assert!(!is_per_length(a));
        assert_eq!(abs_length_val(a), 42);
        assert_eq!(a, (42 << 2) | 1);

        let p = create_per_length(0.5);
        assert!(is_per_length(p));
        assert_eq!(multiply_with_per_length(100, p), 50);

        let r = create_rel_length(2.0);
        assert!(is_rel_length(r));

        // LENGTH_AUTO = 0 → aucun type
        assert!(!is_abs_length(LENGTH_AUTO));
        assert!(!is_per_length(LENGTH_AUTO));
        assert!(!is_rel_length(LENGTH_AUTO));
    }

    #[test]
    fn per_length_rounding() {
        // round() vs roundInt() sur demi-valeur
        let half = create_per_length(0.5);
        assert_eq!(multiply_with_per_length(3, half), 2); // 1.5 → 2
        assert_eq!(multiply_with_per_length_rounded(3, half), 2);
        // rel: troncature (cast int amont)
        let r = create_rel_length(0.5);
        assert_eq!(multiply_with_rel_length(3, r), 1);
    }

    #[test]
    fn attrs_init_and_reset() {
        let a = StyleAttrs::init_values();
        assert_eq!(a.display, DisplayType::Inline);
        assert_eq!(a.x_link, -1);
        assert_eq!(a.z_index, Z_INDEX_AUTO);
        assert_eq!(a.cursor, Cursor::Default);
        assert!(!a.has_background());
        assert_eq!(a.box_diff_width(), 0);

        let mut b = a.clone();
        b.width = create_abs_length(10);
        b.padding.set_val(3);
        b.margin.left = 5;
        b.border_width.right = 2;
        b.display = DisplayType::Block;
        b.reset_values();
        // resetValues remet les non-hérités, garde font/etc.
        assert_eq!(b.width, LENGTH_AUTO);
        assert_eq!(b.padding, Box::default());
        assert_eq!(b.display, DisplayType::Inline);
        assert_eq!(b.box_offset_x(), 0);
    }

    #[test]
    fn style_pool_dedups() {
        let before = Style::total_ref();
        let a = StyleAttrs::init_values();
        let s1 = Style::create(&a);
        let s2 = Style::create(&a);
        assert!(Rc::ptr_eq(&s1, &s2));
        drop(s2);
        drop(s1);
        assert_eq!(Style::total_ref(), before);
    }

    #[test]
    fn style_pool_distinguishes() {
        let a = StyleAttrs::init_values();
        let mut b = a.clone();
        b.width = create_abs_length(7);
        let s1 = Style::create(&a);
        let s2 = Style::create(&b);
        assert!(!Rc::ptr_eq(&s1, &s2));
    }

    #[test]
    fn num_to_str_verbatim() {
        assert_eq!(num_to_str(1, ListStyleType::Decimal), "1.");
        assert_eq!(num_to_str(42, ListStyleType::Decimal), "42.");
        assert_eq!(num_to_str(4, ListStyleType::UpperRoman), "IV.");
        assert_eq!(num_to_str(2024, ListStyleType::UpperRoman), "MMXXIV.");
        assert_eq!(num_to_str(2024, ListStyleType::LowerRoman), "mmxxiv.");
        assert_eq!(num_to_str(4999, ListStyleType::UpperRoman), "MMMMCMXCIX.");
        assert_eq!(num_to_str(5000, ListStyleType::UpperRoman), "****.");
        // Amont : `num < 0` rejeté mais 0 non → "." verbatim
        assert_eq!(num_to_str(0, ListStyleType::UpperRoman), ".");
        assert_eq!(num_to_str(-1, ListStyleType::UpperRoman), "****.");
        assert_eq!(num_to_str(1, ListStyleType::UpperAlpha), "  A.");
        assert_eq!(num_to_str(26, ListStyleType::LowerAlpha), "  z.");
        assert_eq!(num_to_str(27, ListStyleType::UpperAlpha), " AA.");
        assert_eq!(num_to_str(1, ListStyleType::Disc), "1.");
    }

    #[test]
    fn font_attrs_hash() {
        let f = FontAttrs {
            name: b"helvetica".to_vec(),
            size: 14,
            weight: 400,
            letter_spacing: 0,
            font_variant: FontVariant::Normal,
            style: FontStyle::Normal,
        };
        let h = f.hash_value();
        let mut f2 = f.clone();
        f2.size = 12;
        assert_ne!(h, f2.hash_value());
    }
}

// ------------------------------------------------------------------
// ExternalImgRenderer / ExternalWidgetImgRenderer + dessin de style
// (`drawBorder*`/`drawBackground`/`drawBackgroundImage` — style.cc)
// ------------------------------------------------------------------

/// `StyleImage::ExternalImgRenderer` — renderer externe branché sur
/// un `StyleImage` (widget ou layout) : `setBuffer` ne fait rien,
/// `drawRow`/`finish` redessinent la zone de fond — les sous-traits
/// fournissent les zones et la politique de tuilage.
///
/// Les méthodes `ImgRenderer` ne peuvent pas être implémentées par
/// défaut ici : le port expose les helpers `external_img_renderer_*`
/// que chaque implémentation appelle verbatim depuis ses méthodes
/// `ImgRenderer` (même rôle que la classe de base amont).
pub trait ExternalImgRenderer: ImgRenderer {
    /// `readyToDraw` — si `false`, rien n'est dessiné.
    fn ready_to_draw(&self) -> bool;
    /// `getBgArea (x, y, w, h)` — zone couverte par le fond,
    /// coordonnées canvas.
    fn get_bg_area(&self) -> (i32, i32, i32, i32);
    /// `getRefArea` — zone de référence du tuilage.
    fn get_ref_area(&self) -> (i32, i32, i32, i32);
    /// `getBackgroundImage`.
    fn get_background_image(&self) -> Option<Rc<RefCell<StyleImage>>>;
    /// `getBackgroundRepeat`.
    fn get_background_repeat(&self) -> BackgroundRepeat;
    /// `getBackgroundAttachment`.
    fn get_background_attachment(&self) -> BackgroundAttachment;
    /// `getBackgroundPositionX`.
    fn get_background_position_x(&self) -> Length;
    /// `getBackgroundPositionY`.
    fn get_background_position_y(&self) -> Length;
    /// `draw (x, y, w, h)` — dessine (ou enfile) une zone en
    /// coordonnées *canvas*.
    fn draw(&mut self, x: i32, y: i32, width: i32, height: i32);
}

/// `ExternalImgRenderer::setBuffer` — rien (style.cc:644 verbatim).
pub fn external_img_renderer_set_buffer(_r: &mut dyn ExternalImgRenderer) {}

/// `ExternalImgRenderer::drawRow` — verbatim style.cc:650 :
/// branche `drawBackgroundLineByLine` (`false` amont → code mort
/// fidèlement porté : tuilage ligne par ligne).
pub fn external_img_renderer_draw_row(r: &mut dyn ExternalImgRenderer, row: i32) {
    if DRAW_BACKGROUND_LINE_BY_LINE && r.ready_to_draw() {
        if let Some(background_image) = r.get_background_image() {
            let imgbuf = background_image.borrow().get_imgbuf_src();
            if let Some(imgbuf) = imgbuf {
                let img_width = imgbuf.get_root_width();
                let img_height = imgbuf.get_root_height();
                let (x, y, width, height) = r.get_bg_area();
                let (x_ref, y_ref, width_ref, height_ref) = r.get_ref_area();
                let v = calc_background_related_values(
                    &background_image,
                    r.get_background_repeat(),
                    r.get_background_attachment(),
                    r.get_background_position_x(),
                    r.get_background_position_y(),
                    x,
                    y,
                    width,
                    height,
                    x_ref,
                    y_ref,
                    width_ref,
                    height_ref,
                );
                if v.do_draw {
                    for tile_y in v.tile_y1..=v.tile_y2 {
                        let x1 = (v.orig_x + v.tile_x1 * img_width).max(x);
                        let x2 = (v.orig_x + (v.tile_x2 + 1) * img_width).min(x + width);
                        let yt = v.orig_y + tile_y * img_height + row;
                        if yt >= y && yt < y + height {
                            r.draw(x1, yt, x2 - x1, 1);
                        }
                    }
                }
            }
        }
    }
}

/// `ExternalImgRenderer::finish` — verbatim style.cc:698 :
/// `!drawBackgroundLineByLine` → dessine toute la zone d'un coup.
pub fn external_img_renderer_finish(r: &mut dyn ExternalImgRenderer) {
    if !DRAW_BACKGROUND_LINE_BY_LINE && r.ready_to_draw() {
        let (x, y, width, height) = r.get_bg_area();
        r.draw(x, y, width, height);
    }
}

/// `ExternalImgRenderer::fatal` — rien (style.cc:710 verbatim).
pub fn external_img_renderer_fatal(_r: &mut dyn ExternalImgRenderer) {}

/// `StyleImage::ExternalWidgetImgRenderer` — variante widget : les
/// getters de fond lisent le style (`getStyle` abstrait), avec les
/// mêmes fallbacks qu'amont (`BACKGROUND_REPEAT`,
/// `ATTACHMENT_SCROLL`, position `0%`).
pub trait ExternalWidgetImgRenderer: ExternalImgRenderer {
    /// `getStyle`.
    fn get_style(&self) -> Option<Rc<Style>>;

    /// `getBackgroundImage` — `style ? style->backgroundImage
    /// : NULL` (style.cc:717).
    fn get_widget_background_image(&self) -> Option<Rc<RefCell<StyleImage>>> {
        self.get_style()
            .and_then(|s| s.attrs.background_image.clone())
    }

    /// `getBackgroundRepeat` — fallback `BACKGROUND_REPEAT`.
    fn get_widget_background_repeat(&self) -> BackgroundRepeat {
        self.get_style()
            .map_or(BackgroundRepeat::Repeat, |s| s.attrs.background_repeat)
    }

    /// `getBackgroundAttachment` — fallback `SCROLL`.
    fn get_widget_background_attachment(&self) -> BackgroundAttachment {
        self.get_style().map_or(BackgroundAttachment::Scroll, |s| {
            s.attrs.background_attachment
        })
    }

    /// `getBackgroundPositionX` — fallback `createPerLength (0)`.
    fn get_widget_background_position_x(&self) -> Length {
        self.get_style()
            .map_or_else(|| create_per_length(0.0), |s| s.attrs.background_position_x)
    }

    /// `getBackgroundPositionY`.
    fn get_widget_background_position_y(&self) -> Length {
        self.get_style()
            .map_or_else(|| create_per_length(0.0), |s| s.attrs.background_position_y)
    }
}

// ------------------------------------------------------------------
// drawBorderTop/Bottom/Left/Right — verbatim style.cc:757-1157.
// Trapezes pixel-exacts (fallthroughs amont déroulés en gardes).
// ------------------------------------------------------------------

fn pt(x: i32, y: i32) -> crate::types::Point {
    crate::types::Point { x, y }
}

/// `drawBorderTop` (style.cc:757) — verbatim.
#[allow(clippy::too_many_lines)]
fn draw_border_top(
    view: &mut dyn crate::view::View,
    style: &StyleAttrs,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) {
    let mut shading = Shading::Normal;
    let Some(color) = &style.border_color[0] else {
        return;
    };
    if style.border_width.top == 0 {
        return;
    }
    let filled = true;
    let convex = true;
    match style.border_style[0] {
        BorderStyle::None | BorderStyle::Hidden => {}
        BorderStyle::Dotted | BorderStyle::Dashed => {
            let w = style.border_width.top;
            let lt = if style.border_style[0] == BorderStyle::Dotted {
                LineType::Dotted
            } else {
                LineType::Dashed
            };
            view.draw_typed_line(
                color,
                shading,
                lt,
                w,
                x1 + w / 2,
                y1 + w / 2,
                x2 - w / 2,
                y2 + w / 2,
            );
        }
        BorderStyle::Solid | BorderStyle::Inset | BorderStyle::Outset => {
            let inset = style.border_style[0] == BorderStyle::Inset;
            if style.border_style[0] != BorderStyle::Solid {
                shading = if inset { Shading::Dark } else { Shading::Light };
            }
            if style.border_width.top == 1 {
                view.draw_line(color, shading, x1, y1, x2, y2);
            } else {
                let points = [
                    pt(x1, y1),
                    pt(x2 + 1, y1),
                    pt(
                        x2 + 1 - style.border_width.right,
                        y1 + style.border_width.top,
                    ),
                    pt(x1 + style.border_width.left, y1 + style.border_width.top),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
            }
        }
        BorderStyle::Ridge | BorderStyle::Groove => {
            let ridge = style.border_style[0] == BorderStyle::Ridge;
            let d = style.border_width.top & 1;
            let points = [
                pt(x1, y1),
                pt(x2 + 1, y1),
                pt(
                    x2 - style.border_width.right / 2,
                    y1 + style.border_width.top / 2 + d,
                ),
                pt(
                    x1 + style.border_width.left / 2,
                    y1 + style.border_width.top / 2 + d,
                ),
            ];
            shading = if ridge { Shading::Light } else { Shading::Dark };
            view.draw_polygon(color, shading, filled, convex, &points);
            let points = [
                pt(
                    x1 + style.border_width.left / 2 + d,
                    y1 + style.border_width.top / 2 + d,
                ),
                pt(
                    x2 - style.border_width.right / 2 + 1 - d,
                    y1 + style.border_width.top / 2 + d,
                ),
                pt(
                    x2 - style.border_width.right + 1 - d,
                    y1 + style.border_width.top,
                ),
                pt(x1 + style.border_width.left, y1 + style.border_width.top),
            ];
            shading = if ridge { Shading::Dark } else { Shading::Light };
            view.draw_polygon(color, shading, filled, convex, &points);
        }
        BorderStyle::Double => {
            #[allow(clippy::cast_possible_truncation)]
            let w = (f64::from(style.border_width.top) / 3.0).round() as i32;
            let d = if w != 0 {
                style.border_width.top - 2 * w
            } else {
                0
            };
            #[allow(clippy::cast_possible_truncation)]
            let w_l = (f64::from(style.border_width.left) / 3.0).round() as i32;
            #[allow(clippy::cast_possible_truncation)]
            let w_r = (f64::from(style.border_width.right) / 3.0).round() as i32;
            if style.border_width.top == 1 {
                view.draw_line(color, shading, x1, y1, x2, y2);
            } else {
                let points = [
                    pt(x1, y1),
                    pt(x2 + 1, y1),
                    pt(x2 + 1 - w_r, y1 + w),
                    pt(x1 + w_l, y1 + w),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
                let points = [
                    pt(x1 + style.border_width.left - w_l, y1 + w + d),
                    pt(x2 + 1 - style.border_width.right + w_r, y1 + w + d),
                    pt(
                        x2 + 1 - style.border_width.right,
                        y1 + style.border_width.top,
                    ),
                    pt(x1 + style.border_width.left, y1 + style.border_width.top),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
            }
        }
    }
}

/// `drawBorderBottom` (style.cc:857) — verbatim.
#[allow(clippy::too_many_lines)]
fn draw_border_bottom(
    view: &mut dyn crate::view::View,
    style: &StyleAttrs,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) {
    let mut shading = Shading::Normal;
    let Some(color) = &style.border_color[2] else {
        return;
    };
    if style.border_width.bottom == 0 {
        return;
    }
    let filled = true;
    let convex = true;
    match style.border_style[2] {
        BorderStyle::None | BorderStyle::Hidden => {}
        BorderStyle::Dotted | BorderStyle::Dashed => {
            let w = style.border_width.bottom;
            let lt = if style.border_style[2] == BorderStyle::Dotted {
                LineType::Dotted
            } else {
                LineType::Dashed
            };
            view.draw_typed_line(
                color,
                shading,
                lt,
                w,
                x1 + w / 2,
                y1 - w / 2,
                x2 - w / 2,
                y2 - w / 2,
            );
        }
        BorderStyle::Solid | BorderStyle::Inset | BorderStyle::Outset => {
            let inset = style.border_style[2] == BorderStyle::Inset;
            if style.border_style[2] != BorderStyle::Solid {
                shading = if inset { Shading::Light } else { Shading::Dark };
            }
            if style.border_width.bottom == 1 {
                view.draw_line(color, shading, x1, y1, x2, y2);
            } else {
                let points = [
                    pt(x1 - 1, y1 + 1),
                    pt(x2 + 2, y1 + 1),
                    pt(
                        x2 + 2 - style.border_width.right,
                        y1 + 1 - style.border_width.bottom,
                    ),
                    pt(
                        x1 - 1 + style.border_width.left,
                        y1 + 1 - style.border_width.bottom,
                    ),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
            }
        }
        BorderStyle::Ridge | BorderStyle::Groove => {
            let ridge = style.border_style[2] == BorderStyle::Ridge;
            let w = style.border_width.bottom;
            let d = w & 1;
            let points = [
                pt(x1 - 1, y1 + 1),
                pt(x2 + 2 - d, y1 + 1),
                pt(
                    x2 + 2 - d - style.border_width.right / 2,
                    y1 + 1 - w / 2 - d,
                ),
                pt(x1 - 1 + style.border_width.left / 2 + d, y1 + 1 - w / 2 - d),
            ];
            shading = if ridge { Shading::Dark } else { Shading::Light };
            view.draw_polygon(color, shading, filled, convex, &points);
            let points = [
                pt(x1 + style.border_width.left - 1, y1 - w + 1),
                pt(x2 + 1 - style.border_width.right + 1, y1 - w + 1),
                pt(
                    x2 + 1 - style.border_width.right + 1 + style.border_width.right / 2,
                    y1 - w + 1 + w / 2,
                ),
                pt(
                    x1 + style.border_width.left - 1 - style.border_width.left / 2,
                    y1 - w + 1 + w / 2,
                ),
            ];
            shading = if ridge { Shading::Light } else { Shading::Dark };
            view.draw_polygon(color, shading, filled, convex, &points);
        }
        BorderStyle::Double => {
            #[allow(clippy::cast_possible_truncation)]
            let w = (f64::from(style.border_width.bottom) / 3.0).round() as i32;
            let d = if w != 0 {
                style.border_width.bottom - 2 * w
            } else {
                0
            };
            #[allow(clippy::cast_possible_truncation)]
            let w_l = (f64::from(style.border_width.left) / 3.0).round() as i32;
            #[allow(clippy::cast_possible_truncation)]
            let w_r = (f64::from(style.border_width.right) / 3.0).round() as i32;
            if style.border_width.bottom == 1 {
                view.draw_line(color, shading, x1, y1, x2, y2);
            } else {
                let points = [
                    pt(x2 + 2, y1 + 1),
                    pt(x1 - 1, y1 + 1),
                    pt(x1 - 1 + w_l, y1 + 1 - w),
                    pt(x2 + 2 - w_r, y1 + 1 - w),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
                let points = [
                    pt(x2 + 2 - style.border_width.right + w_r, y1 + 1 - w - d),
                    pt(x1 - 1 + style.border_width.left - w_l, y1 + 1 - w - d),
                    pt(
                        x1 - 1 + style.border_width.left,
                        y1 + 1 - style.border_width.bottom,
                    ),
                    pt(
                        x2 + 2 - style.border_width.right,
                        y1 + 1 - style.border_width.bottom,
                    ),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
            }
        }
    }
}

/// `drawBorderLeft` (style.cc:959) — verbatim.
#[allow(clippy::too_many_lines)]
fn draw_border_left(
    view: &mut dyn crate::view::View,
    style: &StyleAttrs,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) {
    let mut shading = Shading::Normal;
    let Some(color) = &style.border_color[3] else {
        return;
    };
    if style.border_width.left == 0 {
        return;
    }
    let filled = true;
    let convex = true;
    match style.border_style[3] {
        BorderStyle::None | BorderStyle::Hidden => {}
        BorderStyle::Dotted | BorderStyle::Dashed => {
            let w = style.border_width.left;
            let lt = if style.border_style[3] == BorderStyle::Dotted {
                LineType::Dotted
            } else {
                LineType::Dashed
            };
            view.draw_typed_line(
                color,
                shading,
                lt,
                w,
                x1 + w / 2,
                y1 + w / 2,
                x1 + w / 2,
                y2 - w / 2,
            );
        }
        BorderStyle::Solid | BorderStyle::Inset | BorderStyle::Outset => {
            let inset = style.border_style[3] == BorderStyle::Inset;
            if style.border_style[3] != BorderStyle::Solid {
                shading = if inset { Shading::Dark } else { Shading::Light };
            }
            if style.border_width.left == 1 {
                view.draw_line(color, shading, x1, y1, x2, y2);
            } else {
                let points = [
                    pt(x1, y1 - 1),
                    pt(x1, y2 + 1),
                    pt(
                        x1 + style.border_width.left,
                        y2 + 1 - style.border_width.bottom,
                    ),
                    pt(
                        x1 + style.border_width.left,
                        y1 - 1 + style.border_width.top,
                    ),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
            }
        }
        BorderStyle::Ridge | BorderStyle::Groove => {
            let ridge = style.border_style[3] == BorderStyle::Ridge;
            let w = style.border_width.left;
            let d = w & 1;
            let points = [
                pt(x1, y1),
                pt(x1, y2),
                pt(x1 + w / 2 + d, y2 - style.border_width.bottom / 2),
                pt(x1 + w / 2 + d, y1 + style.border_width.top / 2),
            ];
            shading = if ridge { Shading::Light } else { Shading::Dark };
            view.draw_polygon(color, shading, filled, convex, &points);
            let points = [
                pt(x1 + w / 2 + d, y1 + style.border_width.top / 2),
                pt(x1 + w / 2 + d, y2 - style.border_width.bottom / 2),
                pt(x1 + w, y2 - style.border_width.bottom),
                pt(x1 + w, y1 + style.border_width.top),
            ];
            shading = if ridge { Shading::Dark } else { Shading::Light };
            view.draw_polygon(color, shading, filled, convex, &points);
        }
        BorderStyle::Double => {
            #[allow(clippy::cast_possible_truncation)]
            let w = (f64::from(style.border_width.left) / 3.0).round() as i32;
            let d = if w != 0 {
                style.border_width.left - 2 * w
            } else {
                0
            };
            #[allow(clippy::cast_possible_truncation)]
            let w_b = (f64::from(style.border_width.bottom) / 3.0).round() as i32;
            #[allow(clippy::cast_possible_truncation)]
            let w_t = (f64::from(style.border_width.top) / 3.0).round() as i32;
            if style.border_width.left == 1 {
                view.draw_line(color, shading, x1, y1, x2, y2 - 1);
            } else {
                let points = [
                    pt(x1, y1 - 1),
                    pt(x1, y2 + 1),
                    pt(x1 + w, y2 + 1 - w_b),
                    pt(x1 + w, y1 - 1 + w_t),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
                // Bord lointain du 2ᵉ trapèze = `points[0].x + w`
                // amont = `x1 + 2*w + d` (bord intérieur, pas `x1+w`
                // qui retombe sur l'espace intermédiaire — A05-D1).
                let points = [
                    pt(x1 + w + d, y1 - 1 + style.border_width.top - w_t),
                    pt(x1 + w + d, y2 + 1 - style.border_width.bottom + w_b),
                    pt(x1 + 2 * w + d, y2 + 1 - style.border_width.bottom),
                    pt(x1 + 2 * w + d, y1 - 1 + style.border_width.top),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
            }
        }
    }
}

/// `drawBorderRight` (style.cc:1059) — verbatim.
#[allow(clippy::too_many_lines)]
fn draw_border_right(
    view: &mut dyn crate::view::View,
    style: &StyleAttrs,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) {
    let mut shading = Shading::Normal;
    let Some(color) = &style.border_color[1] else {
        return;
    };
    if style.border_width.right == 0 {
        return;
    }
    let filled = true;
    let convex = true;
    match style.border_style[1] {
        BorderStyle::None | BorderStyle::Hidden => {}
        BorderStyle::Dotted | BorderStyle::Dashed => {
            let w = style.border_width.right;
            let lt = if style.border_style[1] == BorderStyle::Dotted {
                LineType::Dotted
            } else {
                LineType::Dashed
            };
            view.draw_typed_line(
                color,
                shading,
                lt,
                w,
                x1 - w / 2,
                y1 + w / 2,
                x1 - w / 2,
                y2 - w / 2,
            );
        }
        BorderStyle::Solid | BorderStyle::Inset | BorderStyle::Outset => {
            let inset = style.border_style[1] == BorderStyle::Inset;
            if style.border_style[1] != BorderStyle::Solid {
                shading = if inset { Shading::Dark } else { Shading::Light };
            }
            if style.border_width.right == 1 {
                view.draw_line(color, shading, x1, y1, x2, y2);
            } else {
                let points = [
                    pt(x1 + 1, y1 - 1),
                    pt(x1 + 1, y2 + 1),
                    pt(
                        x1 + 1 - style.border_width.right,
                        y2 + 1 - style.border_width.bottom,
                    ),
                    pt(
                        x1 + 1 - style.border_width.right,
                        y1 - 1 + style.border_width.top,
                    ),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
            }
        }
        BorderStyle::Ridge | BorderStyle::Groove => {
            let ridge = style.border_style[1] == BorderStyle::Ridge;
            let w = style.border_width.right;
            let d = w & 1;
            let points = [
                pt(x1 + 1, y1),
                pt(x1 + 1, y2),
                pt(x1 + 1 - w / 2 - d, y2 - style.border_width.bottom / 2),
                pt(x1 + 1 - w / 2 - d, y1 + style.border_width.top / 2),
            ];
            shading = if ridge { Shading::Dark } else { Shading::Light };
            view.draw_polygon(color, shading, filled, convex, &points);
            let points = [
                pt(x1 + 1 - w / 2 - d, y1 + style.border_width.top / 2),
                pt(x1 + 1 - w / 2 - d, y2 - style.border_width.bottom / 2),
                pt(x1 + 1 - w, y2 - style.border_width.bottom),
                pt(x1 + 1 - w, y1 + style.border_width.top),
            ];
            shading = if ridge { Shading::Light } else { Shading::Dark };
            view.draw_polygon(color, shading, filled, convex, &points);
        }
        BorderStyle::Double => {
            #[allow(clippy::cast_possible_truncation)]
            let w = (f64::from(style.border_width.right) / 3.0).round() as i32;
            let d = if w != 0 {
                style.border_width.right - 2 * w
            } else {
                0
            };
            #[allow(clippy::cast_possible_truncation)]
            let w_b = (f64::from(style.border_width.bottom) / 3.0).round() as i32;
            #[allow(clippy::cast_possible_truncation)]
            let w_t = (f64::from(style.border_width.top) / 3.0).round() as i32;
            if style.border_width.right == 1 {
                view.draw_line(color, shading, x1, y1, x2, y2);
            } else {
                let points = [
                    pt(x1 + 1, y1 - 1),
                    pt(x1 + 1, y2 + 1),
                    pt(x1 + 1 - w, y2 + 1 - w_b),
                    pt(x1 + 1 - w, y1 - 1 + w_t),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
                // Bord lointain = `points[0].x - w` = `x1 + 1 - 2*w - d`
                // amont (bord intérieur — A05-D1).
                let points = [
                    pt(x1 + 1 - w - d, y1 - 1 + style.border_width.top - w_t),
                    pt(x1 + 1 - w - d, y2 + 1 - style.border_width.bottom + w_b),
                    pt(x1 + 1 - 2 * w - d, y2 + 1 - style.border_width.bottom),
                    pt(x1 + 1 - 2 * w - d, y1 - 1 + style.border_width.top),
                ];
                view.draw_polygon(color, shading, filled, convex, &points);
            }
        }
    }
}

/// `drawBorder` (style.cc:1167) — `x`/`y`/`width`/`height` = boîte
/// (marge incluse), `area` = zone à dessiner, coords canvas.
/// `layout`/`area` inutilisés amont (`\todo a lot!`) — conservés
/// dans la signature pour le contrat.
#[allow(clippy::too_many_arguments)]
pub fn draw_border(
    view: &mut dyn crate::view::View,
    area: &crate::types::Rectangle,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    style: &StyleAttrs,
    _inverse: bool,
) {
    let _ = area;
    // Coins externes de la bordure (style.cc:1175-1178, verbatim).
    let xb1 = x + style.margin.left;
    let yb1 = y + style.margin.top;
    let xb2 = x + (if width > 0 { width - 1 } else { 0 }) - style.margin.right;
    let yb2 = y + (if height > 0 { height - 1 } else { 0 }) - style.margin.bottom;

    draw_border_right(view, style, xb2, yb1, xb2, yb2);
    draw_border_left(view, style, xb1, yb1, xb1, yb2);
    draw_border_top(view, style, xb1, yb1, xb2, yb1);
    draw_border_bottom(view, style, xb1, yb2, xb2, yb2);
}

/// `drawBackground` (style.cc:1232) — `(x,y,w,h)` = boîte padding,
/// `*Ref` = zone de référence du tuilage, coords canvas. `at_top` ≙
/// « toplevel widget » — la couleur/image de fond du canvas est
/// comparée via `layout_bg_color`/`layout_bg_image` (≙
/// `layout->getBgColor()` / `getBgImage()`).
#[allow(clippy::too_many_arguments)]
pub fn draw_background(
    view: &mut dyn crate::view::View,
    layout_bg_color: Option<&Color>,
    layout_bg_image: Option<&Rc<RefCell<StyleImage>>>,
    area: &crate::types::Rectangle,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    x_ref: i32,
    y_ref: i32,
    width_ref: i32,
    height_ref: i32,
    style: &StyleAttrs,
    bg_color: Option<&Color>,
    inverse: bool,
    at_top: bool,
) {
    // `hasBgColor`/`hasBgImage` — comparaisons d'identité (≙ pointeurs
    // amont ; `Rc::ptr_eq` pour les images).
    let has_bg_color = bg_color.is_some()
        && (!at_top
            || layout_bg_color.is_none_or(|c| bg_color.is_some_and(|b| !std::ptr::eq(c, b))));
    let has_bg_image = style
        .background_image
        .as_ref()
        .is_some_and(|bi| bi.borrow().get_imgbuf_src().is_some())
        && (!at_top
            || layout_bg_image.is_none_or(|lbi| {
                style
                    .background_image
                    .as_ref()
                    .is_some_and(|bi| !Rc::ptr_eq(lbi, bi))
            }));

    if has_bg_color || has_bg_image {
        let bg_area = crate::types::Rectangle {
            x,
            y,
            width,
            height,
        };
        if let Some(intersection) = area.intersects_with(&bg_area) {
            if has_bg_color {
                if let Some(c) = bg_color {
                    view.draw_rectangle(
                        c,
                        if inverse {
                            Shading::Inverse
                        } else {
                            Shading::Normal
                        },
                        true,
                        intersection.x,
                        intersection.y,
                        intersection.width,
                        intersection.height,
                    );
                }
            }
            if has_bg_image {
                if let Some(bi) = &style.background_image {
                    draw_background_image(
                        view,
                        bi,
                        style.background_repeat,
                        style.background_attachment,
                        style.background_position_x,
                        style.background_position_y,
                        intersection.x,
                        intersection.y,
                        intersection.width,
                        intersection.height,
                        x_ref,
                        y_ref,
                        width_ref,
                        height_ref,
                    );
                }
            }
        }
    }
}

/// `drawBackgroundImage` (style.cc:1285) — tuilage calculé sur le
/// buffer *source*, blit depuis le buffer *tuilé*.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::similar_names)]
pub fn draw_background_image(
    view: &mut dyn crate::view::View,
    background_image: &Rc<RefCell<StyleImage>>,
    background_repeat: BackgroundRepeat,
    background_attachment: BackgroundAttachment,
    background_position_x: Length,
    background_position_y: Length,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    x_ref: i32,
    y_ref: i32,
    width_ref: i32,
    height_ref: i32,
) {
    let v = calc_background_related_values(
        background_image,
        background_repeat,
        background_attachment,
        background_position_x,
        background_position_y,
        x,
        y,
        width,
        height,
        x_ref,
        y_ref,
        width_ref,
        height_ref,
    );

    if v.do_draw {
        let bi = background_image.borrow();
        if let Some(imgbuf_s) = bi.get_imgbuf_src() {
            let img_width_s = imgbuf_s.get_root_width();
            let img_height_s = imgbuf_s.get_root_height();
            if let Some(imgbuf_t) = bi.get_imgbuf_tiled(v.repeat_x, v.repeat_y) {
                let img_width_t = imgbuf_t.get_root_width();
                let img_height_t = imgbuf_t.get_root_height();
                let tiles_x = bi.get_tiles_x(v.repeat_x, v.repeat_y);
                let tiles_y = bi.get_tiles_y(v.repeat_x, v.repeat_y);
                drop(bi);

                let mut tile_x = v.tile_x1;
                while tile_x <= v.tile_x2 {
                    let mut tile_y = v.tile_y1;
                    while tile_y <= v.tile_y2 {
                        let xt = v.orig_x + tile_x * img_width_s;
                        let x1 = xt.max(x);
                        let x2 = (xt + img_width_t).min(x + width);
                        let yt = v.orig_y + tile_y * img_height_s;
                        let y1 = yt.max(y);
                        let y2 = (yt + img_height_t).min(y + height);
                        view.draw_image(&imgbuf_t, xt, yt, x1 - xt, y1 - yt, x2 - x1, y2 - y1);
                        tile_y += tiles_y;
                    }
                    tile_x += tiles_x;
                }
            }
        }
    }
}

/// Valeurs calculées par `calcBackgroundRelatedValues`
/// (style.cc:1339) — tuilage + visibilité.
#[derive(Debug, Clone, Copy)]
pub struct BgTile {
    /// `repeatX`.
    pub repeat_x: bool,
    /// `repeatY`.
    pub repeat_y: bool,
    /// `origX` — origine de la tuile 0 (coords canvas).
    pub orig_x: i32,
    /// `origY`.
    pub orig_y: i32,
    /// `tileX1`.
    pub tile_x1: i32,
    /// `tileX2`.
    pub tile_x2: i32,
    /// `tileY1`.
    pub tile_y1: i32,
    /// `tileY2`.
    pub tile_y2: i32,
    /// `doDraw`.
    pub do_draw: bool,
}

/// `calcBackgroundRelatedValues` — verbatim style.cc:1339. Les
/// bornes de tuiles utilisent les mêmes formules qu'amont
/// (troncature C `int/int` — `imgWidth` ≥ 1 garanti par le clamp
/// `1×1` des buffers ; sinon aucune tuile).
#[allow(clippy::too_many_arguments)]
#[must_use]
#[allow(clippy::similar_names)]
pub fn calc_background_related_values(
    background_image: &Rc<RefCell<StyleImage>>,
    background_repeat: BackgroundRepeat,
    _background_attachment: BackgroundAttachment,
    background_position_x: Length,
    background_position_y: Length,
    x_draw: i32,
    y_draw: i32,
    width_draw: i32,
    height_draw: i32,
    x_ref: i32,
    y_ref: i32,
    width_ref: i32,
    height_ref: i32,
) -> BgTile {
    let imgbuf = background_image.borrow().get_imgbuf_src();
    let (img_width, img_height) =
        imgbuf.map_or((0, 0), |b| (b.get_root_width(), b.get_root_height()));

    let repeat_x = matches!(
        background_repeat,
        BackgroundRepeat::Repeat | BackgroundRepeat::RepeatX
    );
    let repeat_y = matches!(
        background_repeat,
        BackgroundRepeat::Repeat | BackgroundRepeat::RepeatY
    );

    let orig_x = x_ref
        + if is_per_length(background_position_x) {
            multiply_with_per_length(width_ref - img_width, background_position_x)
        } else {
            abs_length_val(background_position_x)
        };
    let orig_y = y_ref
        + if is_per_length(background_position_y) {
            multiply_with_per_length(height_ref - img_height, background_position_y)
        } else {
            abs_length_val(background_position_y)
        };

    let (mut tile_x1, mut tile_x2, mut tile_y1, mut tile_y2) = if img_width > 0 && img_height > 0 {
        (
            if x_draw < orig_x {
                -(orig_x - x_draw + img_width - 1) / img_width
            } else {
                (x_draw - orig_x) / img_width
            },
            if orig_x < x_draw + width_draw {
                (x_draw + width_draw - orig_x - 1) / img_width
            } else {
                -(orig_x - (x_draw + width_draw) + img_width - 1) / img_width
            },
            if y_draw < orig_y {
                -(orig_y - y_draw + img_height - 1) / img_height
            } else {
                (y_draw - orig_y) / img_height
            },
            if orig_y < y_draw + height_draw {
                (y_draw + height_draw - orig_y - 1) / img_height
            } else {
                -(orig_y - (y_draw + height_draw) + img_height - 1) / img_height
            },
        )
    } else {
        (0, -1, 0, -1)
    };

    let mut do_draw = true;
    if !repeat_x {
        if tile_x1 <= 0 && tile_x2 >= 0 {
            tile_x1 = 0;
            tile_x2 = 0;
        } else {
            do_draw = false;
        }
    }
    if !repeat_y {
        if tile_y1 <= 0 && tile_y2 >= 0 {
            tile_y1 = 0;
            tile_y2 = 0;
        } else {
            do_draw = false;
        }
    }

    BgTile {
        repeat_x,
        repeat_y,
        orig_x,
        orig_y,
        tile_x1,
        tile_x2,
        tile_y1,
        tile_y2,
        do_draw,
    }
}

// ------------------------------------------------------------------
// Tests — dessin de style (bordures, background, tuilage)
// ------------------------------------------------------------------

#[cfg(test)]
mod draw_tests {
    use super::*;
    use crate::imgbuf::Imgbuf;
    use crate::types::{Point, Rectangle};

    /// `Imgbuf` factice — seules les dimensions comptent pour le
    /// calcul de tuilage.
    struct MockBuf {
        w: i32,
        h: i32,
    }

    impl Imgbuf for MockBuf {
        fn set_cmap(&self, _colors: &[i32]) {}
        fn copy_row(&self, _row: i32, _data: &[u8]) {}
        fn new_scan(&self) {}
        fn get_scaled_buf(self: Rc<Self>, _w: i32, _h: i32) -> Rc<dyn Imgbuf> {
            self
        }
        fn get_row_area(&self, _row: i32, _area: &mut Rectangle) {}
        fn get_root_width(&self) -> i32 {
            self.w
        }
        fn get_root_height(&self) -> i32 {
            self.h
        }
        fn width(&self) -> i32 {
            self.w
        }
        fn height(&self) -> i32 {
            self.h
        }
        fn create_similar_buf(&self, width: i32, height: i32) -> Rc<dyn Imgbuf> {
            Rc::new(MockBuf {
                w: width,
                h: height,
            })
        }
        fn rgb_row(&self, _y: i32) -> Vec<u8> {
            Vec::new()
        }
        fn write_rgb_span(&self, _x: i32, _y: i32, _data: &[u8]) {}
        fn ref_(&self) {}
        fn unref(&self) {}
        fn last_reference(&self) -> bool {
            true
        }
        fn set_delete_on_unref(&self, _d: bool) {}
        fn is_referred(&self) -> bool {
            true
        }
    }

    fn mock_buf(w: i32, h: i32) -> Rc<dyn Imgbuf> {
        Rc::new(MockBuf { w, h })
    }

    /// `StyleImage` avec un buffer source `w×h` directement posé.
    fn style_image_with_src(w: i32, h: i32) -> Rc<RefCell<StyleImage>> {
        let si = StyleImage::create();
        si.borrow_mut().imgbuf_src = Some(mock_buf(w, h));
        si
    }

    /// `View` enregistreuse — chaque primitive devient une entrée.
    #[derive(Default)]
    struct RecView {
        polys: Vec<Vec<Point>>,
        lines: Vec<(i32, i32, i32, i32)>,
        typed_lines: Vec<(i32, i32, i32, i32, i32)>,
        rects: Vec<(i32, i32, i32, i32)>,
        images: Vec<(i32, i32, i32, i32)>,
    }

    impl crate::view::View for RecView {
        fn set_layout(&mut self, _l: Option<usize>) {}
        fn set_canvas_size(&mut self, _w: i32, _a: i32, _d: i32) {}
        fn set_cursor(&mut self, _c: Cursor) {}
        fn set_bg_color(&mut self, _c: &Color) {}
        fn uses_viewport(&mut self) -> bool {
            false
        }
        fn get_h_scrollbar_thickness(&mut self) -> i32 {
            0
        }
        fn get_v_scrollbar_thickness(&mut self) -> i32 {
            0
        }
        fn get_scrollbar_on_left(&mut self) -> i32 {
            0
        }
        fn scroll_to(&mut self, _x: i32, _y: i32) {}
        fn set_viewport_size(&mut self, _w: i32, _h: i32, _ht: i32, _vt: i32) {}
        fn start_drawing(&mut self, _a: &Rectangle) {}
        fn finish_drawing(&mut self, _a: &Rectangle) {}
        fn queue_draw(&mut self, _a: &Rectangle) {}
        fn queue_draw_total(&mut self) {}
        fn cancel_queue_draw(&mut self) {}
        fn draw_point(&mut self, _c: &Color, _s: Shading, _x: i32, _y: i32) {}
        fn draw_line(&mut self, _c: &Color, _s: Shading, x1: i32, y1: i32, x2: i32, y2: i32) {
            self.lines.push((x1, y1, x2, y2));
        }
        fn draw_typed_line(
            &mut self,
            _c: &Color,
            _s: Shading,
            _t: LineType,
            w: i32,
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
        ) {
            self.typed_lines.push((w, x1, y1, x2, y2));
        }
        fn draw_rectangle(
            &mut self,
            _c: &Color,
            _s: Shading,
            _f: bool,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
        ) {
            self.rects.push((x, y, w, h));
        }
        fn draw_arc(
            &mut self,
            _c: &Color,
            _s: Shading,
            _f: bool,
            _cx: i32,
            _cy: i32,
            _w: i32,
            _h: i32,
            _a1: i32,
            _a2: i32,
        ) {
        }
        fn draw_polygon(&mut self, _c: &Color, _s: Shading, _f: bool, _cv: bool, points: &[Point]) {
            self.polys.push(points.to_vec());
        }
        fn draw_text(&mut self, _f: &Font, _c: &Color, _s: Shading, _x: i32, _y: i32, _t: &[u8]) {}
        fn draw_simple_wrapped_text(
            &mut self,
            _f: &Font,
            _c: &Color,
            _s: Shading,
            _x: i32,
            _y: i32,
            _w: i32,
            _h: i32,
            _t: &[u8],
        ) {
        }
        fn draw_image(
            &mut self,
            _b: &Rc<dyn Imgbuf>,
            _xr: i32,
            _yr: i32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
        ) {
            self.images.push((x, y, w, h));
        }
        fn get_clipping_view(
            &mut self,
            _x: i32,
            _y: i32,
            _w: i32,
            _h: i32,
        ) -> std::boxed::Box<dyn crate::view::View> {
            std::boxed::Box::new(RecView::default())
        }
        fn merge_clipping_view(&mut self, _cv: std::boxed::Box<dyn crate::view::View>) {}
    }

    /// `StyleAttrs` avec les 4 bordures configurées.
    fn attrs_with_border(style: BorderStyle, width: i32) -> StyleAttrs {
        StyleAttrs {
            border_color: [
                Some(Color::new(0x0011_2233)),
                Some(Color::new(0x0044_5566)),
                Some(Color::new(0x0077_8899)),
                Some(Color::new(0x00aa_bbcc)),
            ],
            border_style: [style; 4],
            border_width: Box {
                top: width,
                right: width,
                bottom: width,
                left: width,
            },
            ..StyleAttrs::default()
        }
    }

    #[test]
    fn calc_bg_repeat_both_axes() {
        // Image 10×10, zone 100×100 : tuiles 0..9 dans les deux axes,
        // origine en (0,0) — position abs 0.
        let si = style_image_with_src(10, 10);
        let v = calc_background_related_values(
            &si,
            BackgroundRepeat::Repeat,
            BackgroundAttachment::Scroll,
            create_abs_length(0),
            create_abs_length(0),
            0,
            0,
            100,
            100,
            0,
            0,
            100,
            100,
        );
        assert!(v.do_draw);
        assert!(v.repeat_x && v.repeat_y);
        assert_eq!((v.orig_x, v.orig_y), (0, 0));
        assert_eq!((v.tile_x1, v.tile_x2), (0, 9));
        assert_eq!((v.tile_y1, v.tile_y2), (0, 9));
    }

    #[test]
    fn calc_bg_no_repeat_clamps_to_tile_zero() {
        // NoRepeat : tuiles forcées 0..0 même sur grande zone ;
        // origine négative hors zone → do_draw = false (verbatim).
        let si = style_image_with_src(10, 10);
        let v = calc_background_related_values(
            &si,
            BackgroundRepeat::NoRepeat,
            BackgroundAttachment::Scroll,
            create_abs_length(0),
            create_abs_length(0),
            0,
            0,
            100,
            100,
            0,
            0,
            100,
            100,
        );
        assert!(v.do_draw);
        assert_eq!((v.tile_x1, v.tile_x2), (0, 0));
        assert_eq!((v.tile_y1, v.tile_y2), (0, 0));

        // Origine à -50 (position abs) : la tuile 0 ne couvre pas
        // la zone [0,100) → tuiles [-5,-2], pas de 0 → rien.
        let v2 = calc_background_related_values(
            &si,
            BackgroundRepeat::NoRepeat,
            BackgroundAttachment::Scroll,
            create_abs_length(-50),
            create_abs_length(0),
            0,
            0,
            100,
            100,
            0,
            0,
            100,
            100,
        );
        assert!(!v2.do_draw);
    }

    #[test]
    fn calc_bg_percent_position() {
        // `50%` → orig = ref + (ref_size − img_size) × 0.5 = 45.
        let si = style_image_with_src(10, 10);
        let v = calc_background_related_values(
            &si,
            BackgroundRepeat::Repeat,
            BackgroundAttachment::Scroll,
            create_per_length(0.5),
            create_per_length(0.5),
            0,
            0,
            100,
            100,
            0,
            0,
            100,
            100,
        );
        assert_eq!((v.orig_x, v.orig_y), (45, 45));
        // Tuiles négatives couvertes à gauche/haut de l'origine.
        assert_eq!(v.tile_x1, -5);
        assert_eq!(v.tile_y1, -5);
    }

    #[test]
    fn style_image_small_creates_tiled_buffer() {
        // 4×4 < MIN² (10×10) → buffer tuilé `OPT/4` = 12×12 tuiles,
        // buffer 48×48. Sélection : tuilé seulement si repeat_x &&
        // repeat_y (verbatim `getImgbufTiled`).
        let si = StyleImage::create();
        let src = mock_buf(4, 4);
        let mr = si.borrow().get_main_img_renderer();
        mr.borrow_mut().set_buffer(&src, false);
        let b = si.borrow();
        let tiled = b
            .imgbuf_tiled
            .as_ref()
            .map_or((0, 0), |t| (t.get_root_width(), t.get_root_height()));
        assert_eq!(tiled, (48, 48));
        assert_eq!(b.get_tiles_x(true, true), 12);
        assert_eq!(b.get_tiles_y(true, true), 12);
        assert!(Rc::ptr_eq(
            &b.get_imgbuf_tiled(true, true)
                .unwrap_or_else(|| mock_buf(0, 0)),
            b.imgbuf_tiled.as_ref().unwrap_or(&src)
        ));
        // Une seule direction de repeat → buffer SOURCE.
        assert!(Rc::ptr_eq(
            &b.get_imgbuf_tiled(true, false)
                .unwrap_or_else(|| mock_buf(0, 0)),
            &src
        ));
        assert_eq!(b.get_tiles_x(true, false), 1);
    }

    #[test]
    fn style_image_large_no_tiled_buffer() {
        // ≥ MIN² → pas de buffer tuilé, `getImgbufTiled` = src.
        let si = StyleImage::create();
        let src = mock_buf(20, 20);
        let mr = si.borrow().get_main_img_renderer();
        mr.borrow_mut().set_buffer(&src, false);
        let b = si.borrow();
        assert!(b.imgbuf_tiled.is_none());
        assert!(Rc::ptr_eq(
            &b.get_imgbuf_tiled(true, true)
                .unwrap_or_else(|| mock_buf(0, 0)),
            &src
        ));
    }

    #[test]
    fn border_hidden_draws_nothing() {
        let mut v = RecView::default();
        let attrs = attrs_with_border(BorderStyle::Hidden, 2);
        let area = Rectangle {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        draw_border(&mut v, &area, 0, 0, 100, 100, &attrs, false);
        assert!(v.polys.is_empty() && v.lines.is_empty() && v.typed_lines.is_empty());
    }

    #[test]
    fn border_solid_one_pixel_draws_lines() {
        // `borderWidth == 1` → `drawLine` (pas de polygone).
        let mut v = RecView::default();
        let attrs = attrs_with_border(BorderStyle::Solid, 1);
        let area = Rectangle {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        draw_border(&mut v, &area, 0, 0, 10, 10, &attrs, false);
        assert_eq!(v.lines.len(), 4);
        assert_eq!(v.polys.len(), 0);
        // Top : (0,0)-(9,0) ; bottom : (0,9)-(10? ) — verbatim :
        // bottom y2 = y + h - 1 = 9 ; left/right idem.
        assert_eq!(v.lines[2], (0, 0, 9, 0));
    }

    #[test]
    fn border_solid_wide_draws_trapezoids() {
        // width 2 → 4 trapèzes, coins déduits des marges (0 ici).
        let mut v = RecView::default();
        let attrs = attrs_with_border(BorderStyle::Solid, 2);
        let area = Rectangle {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        draw_border(&mut v, &area, 0, 0, 10, 10, &attrs, false);
        assert_eq!(v.polys.len(), 4);
        // Top (drawBorderTop) : [(0,0),(10,0),(10-2,2),(2,2)] =
        // verbatim style.cc (x2+1 = 10).
        assert_eq!(v.polys[2], vec![pt(0, 0), pt(10, 0), pt(8, 2), pt(2, 2)]);
    }

    #[test]
    fn border_double_draws_two_bands() {
        // `BORDER_DOUBLE` width 6 → w=2, d=2 → 2 trapèzes par côté ;
        // la 2ᵉ bande couvre [x1+w+d, x1+2w+d] (bord intérieur —
        // verbatim `points[0].x + w` amont, A05-D1).
        let mut v = RecView::default();
        let attrs = attrs_with_border(BorderStyle::Double, 6);
        let area = Rectangle {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        };
        draw_border(&mut v, &area, 0, 0, 20, 20, &attrs, false);
        assert_eq!(v.polys.len(), 8);
        // Ordre de draw : right(0,1), left(2,3), top(4,5), bottom(6,7).
        // Gauche — xb1=0 : bande ext. [(0,-1),(0,20),(2,18),(2,1)],
        // bande int. x ∈ {4..6} = x1+w+d .. x1+2w+d.
        assert_eq!(v.polys[2], vec![pt(0, -1), pt(0, 20), pt(2, 18), pt(2, 1)]);
        assert_eq!(v.polys[3], vec![pt(4, 3), pt(4, 16), pt(6, 14), pt(6, 5)]);
        // Droite — xb2=19 : bande int. x ∈ {14..16} = x1+1-2w-d ..
        // x1+1-w-d.
        assert_eq!(
            v.polys[0],
            vec![pt(20, -1), pt(20, 20), pt(18, 18), pt(18, 1)]
        );
        assert_eq!(
            v.polys[1],
            vec![pt(16, 3), pt(16, 16), pt(14, 14), pt(14, 5)]
        );
    }

    #[test]
    fn border_dotted_uses_typed_line() {
        let mut v = RecView::default();
        let attrs = attrs_with_border(BorderStyle::Dotted, 3);
        let area = Rectangle {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        };
        draw_border(&mut v, &area, 0, 0, 20, 20, &attrs, false);
        assert_eq!(v.typed_lines.len(), 4);
        assert!(v.typed_lines.iter().all(|t| t.0 == 3));
    }

    #[test]
    fn draw_background_color_fills_intersection() {
        // Fond opaque → un rectangle = zone ∩ boîte padding.
        let mut v = RecView::default();
        let mut attrs = StyleAttrs::default();
        let bg = Color::new(0x0000_00ff);
        let area = Rectangle {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        attrs.background_image = None;
        draw_background(
            &mut v,
            None,
            None,
            &area,
            0,
            0,
            20,
            20,
            0,
            0,
            20,
            20,
            &attrs,
            Some(&bg),
            false,
            false,
        );
        assert_eq!(v.rects, vec![(0, 0, 10, 10)]);
    }
}
