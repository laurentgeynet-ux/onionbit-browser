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

/// `dw::core::style::ListStyleType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// `imgRendererDist`.
    img_renderer_dist: RefCell<ImgRendererDist>,
}

impl StyleImage {
    /// `StyleImage::create`.
    #[must_use]
    pub fn create() -> Rc<RefCell<StyleImage>> {
        Rc::new(RefCell::new(StyleImage {
            tiles_x: 0,
            tiles_y: 0,
            imgbuf_src: None,
            imgbuf_tiled: None,
            img_renderer_dist: RefCell::new(ImgRendererDist::new()),
        }))
    }

    /// `getImgbufSrc`.
    pub fn get_imgbuf_src(&self) -> Option<Rc<dyn Imgbuf>> {
        self.imgbuf_src.clone()
    }

    /// `getImgbufTiled (repeatX, repeatY)` — verbatim.
    pub fn get_imgbuf_tiled(&self, repeat_x: bool, repeat_y: bool) -> Option<Rc<dyn Imgbuf>> {
        if self.imgbuf_tiled.is_some() && repeat_x && repeat_y {
            self.imgbuf_tiled.clone()
        } else {
            self.imgbuf_src.clone()
        }
    }

    /// `getTilesX`.
    pub fn get_tiles_x(&self, repeat_x: bool, repeat_y: bool) -> i32 {
        if self.imgbuf_tiled.is_some() && repeat_x && repeat_y {
            self.tiles_x
        } else {
            1
        }
    }

    /// `getTilesY`.
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
