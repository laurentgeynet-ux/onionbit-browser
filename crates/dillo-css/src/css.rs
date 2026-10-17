#![allow(clippy::cast_possible_truncation)] // `int`/`char` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::unreadable_literal)] // couleurs `0xRRGGBB` idiomatiques.

//! `css.hh` + `css.cc` — modèle CSS : valeurs, propriétés, sélecteurs,
//! règles, feuilles de style et contexte de cascade.
//!
//! Verbatim préservé :
//! - `CssLength` est une union 64 bits `{type, {int|float}}` : PX et AUTO
//!   stockent un `int`, toutes les autres unités un `float`
//!   (`CSS_CREATE_LENGTH` / `CSS_LENGTH_VALUE`).
//! - `CssPropertyName` = index dans `Css_property_info` — la table est
//!   triée alphabétiquement SAUF la queue `x-link`/`x-colspan`/`x-rowspan`/
//!   `last` qui est désordonnée (quirk amont : `bsearch` peut ne pas
//!   trouver ces entrées ; `binary_search_by` sur le même ordre reproduit
//!   le même comportement).
//! - La typo amont `"border-rigth-width"` est conservée.
//! - `CssPrimaryOrder` : `USER_AGENT` va dans la feuille statique globale
//!   `USER_AGENT_SHEET` (partagée entre contextes, remplie par
//!   `StyleEngine::init`), les autres dans `sheet[order]`.
//! - `apply` : ordre de cascade verbatim (UA, user, non-CSS hints,
//!   author, tag style, author-important, tag-style-important,
//!   user-important).
//! - `RuleList::insert` : tri par spécificité croissante, insertion
//!   *après* les règles de même spécificité (les règles tardives
//!   gagnent).
//! - `CssRule::isSafe` : les règles auteur avec pseudo-classe ET
//!   propriété `display`/`background-image` sont rejetées (protection
//!   historique de navigation).
//! - `CssSimpleSelector::specificity` : `id << 20`, `klass|pseudo << 10`,
//!   `element << 0`.
//!
//! Seams du port :
//! - `CssPropertyValue` est une union C → enum Rust. Les lectures se font
//!   via `int_val`/`len_val`/`str_val`/`pos_val` (sûres — le variant
//!   correspond toujours au `CssValueType` retenu au parse).
//! - `owner_of_strings` (gestion `dStrdup`/`dFree`) n'a pas
//!   d'équivalent : toutes les chaînes sont `Vec<u8>` possédées ;
//!   `apply` clone toujours (≙ `ownerOfStrings=true` partout).
//! - `refCount`/`ref`/`unref` → `Rc` sur sélecteurs et listes de
//!   propriétés.
//! - `MatchCache` = `Vec<i32>` (verbatim `SimpleVector<int>` rempli à
//!   -1 par `setSize`).
//! - `elementTable[node->element]` est indexé sans borne amont (UB si
//!   `element < 0 || >= NTAGS` — tag inconnu) : le port saute
//!   silencieusement cette liste (déviation documentée vs UB).

use std::collections::HashMap;
use std::rc::Rc;

use crate::doctree::{Doctree, NodeRef};
use dillo_common::dlib::str_ascii_casecmp;
use dillo_common::lout::misc::round_int;

/// `HTML_NTAGS` (html.hh:24) — taille de `elementTable`.
pub const HTML_NTAGS: usize = 94;

/// `CssPrimaryOrder` — origine + poids (interne).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
pub enum CssPrimaryOrder {
    /// `CSS_PRIMARY_USER_AGENT` — va dans la feuille statique globale.
    UserAgent = 0,
    /// `CSS_PRIMARY_USER`.
    User = 1,
    /// `CSS_PRIMARY_AUTHOR`.
    Author = 2,
    /// `CSS_PRIMARY_AUTHOR_IMPORTANT`.
    AuthorImportant = 3,
    /// `CSS_PRIMARY_USER_IMPORTANT`.
    UserImportant = 4,
}

/// `CSS_PRIMARY_LAST`.
pub const CSS_PRIMARY_LAST: i16 = 5;

/// `CssOrigin` — origine passée au parseur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssOrigin {
    /// `CSS_ORIGIN_USER_AGENT`.
    UserAgent,
    /// `CSS_ORIGIN_USER`.
    User,
    /// `CSS_ORIGIN_AUTHOR`.
    Author,
}

/// `CssValueType` — verbatim (`#[repr]` conservé : `z-index` compare
/// `p->type` à `CSS_LENGTH_TYPE_AUTO` = 13 ≙ `BackgroundPosition`,
/// quirk reproduit dans `styleengine`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
pub enum CssValueType {
    /// `CSS_TYPE_INTEGER` — interne, propriétés `x-*`.
    Integer = 0,
    /// `CSS_TYPE_ENUM` — valeur = index dans `enum_symbols`.
    Enum = 1,
    /// `CSS_TYPE_MULTI_ENUM` — masque `1 << i`.
    MultiEnum = 2,
    /// `CSS_TYPE_LENGTH_PERCENTAGE`.
    LengthPercentage = 3,
    /// `CSS_TYPE_LENGTH` (pas de % quand dw ne les gère pas).
    Length = 4,
    /// `CSS_TYPE_SIGNED_LENGTH`.
    SignedLength = 5,
    /// `CSS_TYPE_LENGTH_PERCENTAGE_NUMBER`.
    LengthPercentageNumber = 6,
    /// `CSS_TYPE_AUTO` — `CssLength` de type AUTO.
    Auto = 7,
    /// `CSS_TYPE_COLOR` — entier `0xRRGGBB`.
    Color = 8,
    /// `CSS_TYPE_FONT_WEIGHT` — `font-weight` seulement.
    FontWeight = 9,
    /// `CSS_TYPE_STRING`.
    String = 10,
    /// `CSS_TYPE_SYMBOL` — copié brut (`font-family`).
    Symbol = 11,
    /// `CSS_TYPE_URI`.
    Uri = 12,
    /// `CSS_TYPE_BACKGROUND_POSITION`.
    BackgroundPosition = 13,
    /// `CSS_TYPE_UNUSED`.
    Unused = 14,
}

/// `CssLengthType` — verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum CssLengthType {
    /// `CSS_LENGTH_TYPE_NONE`.
    None = 0,
    /// `CSS_LENGTH_TYPE_PX`.
    Px = 1,
    /// `CSS_LENGTH_TYPE_MM` — `cm`/`in`/`pt`/`pc` convertis en mm.
    Mm = 2,
    /// `CSS_LENGTH_TYPE_EM`.
    Em = 3,
    /// `CSS_LENGTH_TYPE_EX`.
    Ex = 4,
    /// `CSS_LENGTH_TYPE_CH`.
    Ch = 5,
    /// `CSS_LENGTH_TYPE_REM`.
    Rem = 6,
    /// `CSS_LENGTH_TYPE_VW`.
    Vw = 7,
    /// `CSS_LENGTH_TYPE_VH`.
    Vh = 8,
    /// `CSS_LENGTH_TYPE_VMIN`.
    Vmin = 9,
    /// `CSS_LENGTH_TYPE_VMAX`.
    Vmax = 10,
    /// `CSS_LENGTH_TYPE_PERCENTAGE`.
    Percentage = 11,
    /// `CSS_LENGTH_TYPE_RELATIVE` — pas en CSS, utilisé par le HTML.
    Relative = 12,
    /// `CSS_LENGTH_TYPE_AUTO`.
    Auto = 13,
}

/// `CssLength` — verbatim : union `{int i | float f}` selon le type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CssLength {
    /// `type` de la longueur.
    pub ty: CssLengthType,
    /// membre `i` de l'union (PX, AUTO).
    pub i: i32,
    /// membre `f` de l'union (toutes les autres unités).
    pub f: f32,
}

/// `CSS_CREATE_LENGTH` — verbatim.
#[must_use]
pub fn css_create_length(v: f32, t: CssLengthType) -> CssLength {
    let mut l = CssLength {
        ty: t,
        i: 0,
        f: 0.0,
    };
    match t {
        CssLengthType::Px => l.i = round_int(f64::from(v)),
        CssLengthType::Auto => l.i = 0,
        _ => l.f = v,
    }
    l
}

/// `CSS_LENGTH_TYPE` — verbatim.
#[must_use]
pub fn css_length_type(l: CssLength) -> CssLengthType {
    l.ty
}

/// `CSS_LENGTH_VALUE` — verbatim.
#[must_use]
pub fn css_length_value(l: CssLength) -> f32 {
    match l.ty {
        CssLengthType::Px => l.i as f32,
        CssLengthType::Auto => 0.0,
        _ => l.f,
    }
}

/// `CssPropertyName` — verbatim (`#[repr]` = index dans
/// [`CSS_PROPERTY_INFO`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i16)]
pub enum CssPropertyName {
    /// `background-attachment`.
    BackgroundAttachment = 0,
    /// `background-color`.
    BackgroundColor = 1,
    /// `background-image`.
    BackgroundImage = 2,
    /// `background-position`.
    BackgroundPosition = 3,
    /// `background-repeat`.
    BackgroundRepeat = 4,
    /// `border-bottom-color`.
    BorderBottomColor = 5,
    /// `border-bottom-style`.
    BorderBottomStyle = 6,
    /// `border-bottom-width`.
    BorderBottomWidth = 7,
    /// `border-collapse`.
    BorderCollapse = 8,
    /// `border-left-color`.
    BorderLeftColor = 9,
    /// `border-left-style`.
    BorderLeftStyle = 10,
    /// `border-left-width`.
    BorderLeftWidth = 11,
    /// `border-right-color`.
    BorderRightColor = 12,
    /// `border-right-style`.
    BorderRightStyle = 13,
    /// `border-right-width` (typo amont `border-rigth-width`).
    BorderRightWidth = 14,
    /// `border-spacing`.
    BorderSpacing = 15,
    /// `border-top-color`.
    BorderTopColor = 16,
    /// `border-top-style`.
    BorderTopStyle = 17,
    /// `border-top-width`.
    BorderTopWidth = 18,
    /// `bottom`.
    Bottom = 19,
    /// `caption-side`.
    CaptionSide = 20,
    /// `clear`.
    Clear = 21,
    /// `clip`.
    Clip = 22,
    /// `color`.
    Color = 23,
    /// `content`.
    Content = 24,
    /// `counter-increment`.
    CounterIncrement = 25,
    /// `counter-reset`.
    CounterReset = 26,
    /// `cursor`.
    Cursor = 27,
    /// `direction`.
    Direction = 28,
    /// `display`.
    Display = 29,
    /// `empty-cells`.
    EmptyCells = 30,
    /// `float`.
    Float = 31,
    /// `font-family`.
    FontFamily = 32,
    /// `font-size`.
    FontSize = 33,
    /// `font-size-adjust`.
    FontSizeAdjust = 34,
    /// `font-stretch`.
    FontStretch = 35,
    /// `font-style`.
    FontStyle = 36,
    /// `font-variant`.
    FontVariant = 37,
    /// `font-weight`.
    FontWeight = 38,
    /// `height`.
    Height = 39,
    /// `left`.
    Left = 40,
    /// `letter-spacing`.
    LetterSpacing = 41,
    /// `line-height`.
    LineHeight = 42,
    /// `list-style-image`.
    ListStyleImage = 43,
    /// `list-style-position`.
    ListStylePosition = 44,
    /// `list-style-type`.
    ListStyleType = 45,
    /// `margin-bottom`.
    MarginBottom = 46,
    /// `margin-left`.
    MarginLeft = 47,
    /// `margin-right`.
    MarginRight = 48,
    /// `margin-top`.
    MarginTop = 49,
    /// `marker-offset`.
    MarkerOffset = 50,
    /// `marks`.
    Marks = 51,
    /// `max-height`.
    MaxHeight = 52,
    /// `max-width`.
    MaxWidth = 53,
    /// `min-height`.
    MinHeight = 54,
    /// `min-width`.
    MinWidth = 55,
    /// `outline-color`.
    OutlineColor = 56,
    /// `outline-style`.
    OutlineStyle = 57,
    /// `outline-width`.
    OutlineWidth = 58,
    /// `overflow`.
    Overflow = 59,
    /// `padding-bottom`.
    PaddingBottom = 60,
    /// `padding-left`.
    PaddingLeft = 61,
    /// `padding-right`.
    PaddingRight = 62,
    /// `padding-top`.
    PaddingTop = 63,
    /// `position`.
    Position = 64,
    /// `quotes`.
    Quotes = 65,
    /// `right`.
    Right = 66,
    /// `text-align`.
    TextAlign = 67,
    /// `text-decoration`.
    TextDecoration = 68,
    /// `text-indent`.
    TextIndent = 69,
    /// `text-shadow`.
    TextShadow = 70,
    /// `text-transform`.
    TextTransform = 71,
    /// `top`.
    Top = 72,
    /// `unicode-bidi`.
    UnicodeBidi = 73,
    /// `vertical-align`.
    VerticalAlign = 74,
    /// `visibility`.
    Visibility = 75,
    /// `white-space`.
    WhiteSpace = 76,
    /// `width`.
    Width = 77,
    /// `word-spacing`.
    WordSpacing = 78,
    /// `z-index`.
    ZIndex = 79,
    /// `x-link` (interne, jamais parsé).
    XLink = 80,
    /// `x-colspan` (interne).
    XColspan = 81,
    /// `x-rowspan` (interne).
    XRowspan = 82,
    /// `PROPERTY_X_LINK` (interne, utilisé par html.cc).
    PropertyXLink = 83,
    /// `PROPERTY_X_LANG`.
    PropertyXLang = 84,
    /// `PROPERTY_X_IMG`.
    PropertyXImg = 85,
    /// `PROPERTY_X_TOOLTIP`.
    PropertyXTooltip = 86,
}

/// `CSS_PROPERTY_END` — terminateur de listes de propriétés.
pub const CSS_PROPERTY_END: i16 = -1;
/// `CSS_PROPERTY_LAST`.
pub const CSS_PROPERTY_LAST: usize = 87;
/// `CSS_NUM_INTERNAL_PROPERTIES` — les 3 derniers ne sont jamais parsés.
pub const CSS_NUM_INTERNAL_PROPERTIES: usize = 3;
/// `CSS_NUM_PARSED_PROPERTIES` — taille du `bsearch` amont.
pub const CSS_NUM_PARSED_PROPERTIES: usize = CSS_PROPERTY_LAST - CSS_NUM_INTERNAL_PROPERTIES;

/// `CssBorderWidthExtensions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssBorderWidthExtensions {
    /// `CSS_BORDER_WIDTH_THIN`.
    Thin = 0,
    /// `CSS_BORDER_WIDTH_MEDIUM`.
    Medium = 1,
    /// `CSS_BORDER_WIDTH_THICK`.
    Thick = 2,
}

/// `CssFontWeightExtensions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssFontWeightExtensions {
    /// `CSS_FONT_WEIGHT_BOLD`.
    Bold = 0,
    /// `CSS_FONT_WEIGHT_BOLDER`.
    Bolder = 1,
    /// `CSS_FONT_WEIGHT_LIGHT`.
    Light = 2,
    /// `CSS_FONT_WEIGHT_LIGHTER`.
    Lighter = 3,
    /// `CSS_FONT_WEIGHT_NORMAL`.
    Normal = 4,
}

/// `CssFontSizeExtensions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssFontSizeExtensions {
    /// `CSS_FONT_SIZE_LARGE`.
    Large = 0,
    /// `CSS_FONT_SIZE_LARGER`.
    Larger = 1,
    /// `CSS_FONT_SIZE_MEDIUM`.
    Medium = 2,
    /// `CSS_FONT_SIZE_SMALL`.
    Small = 3,
    /// `CSS_FONT_SIZE_SMALLER`.
    Smaller = 4,
    /// `CSS_FONT_SIZE_XX_LARGE`.
    XxLarge = 5,
    /// `CSS_FONT_SIZE_XX_SMALL`.
    XxSmall = 6,
    /// `CSS_FONT_SIZE_X_LARGE`.
    XLarge = 7,
    /// `CSS_FONT_SIZE_X_SMALL`.
    XSmall = 8,
}

/// `CSS_LETTER_SPACING_NORMAL`.
pub const CSS_LETTER_SPACING_NORMAL: i32 = 0;
/// `CSS_WORD_SPACING_NORMAL`.
pub const CSS_WORD_SPACING_NORMAL: i32 = 0;

/// `CssBackgroundPosition` — verbatim `{posX, posY}`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CssBackgroundPosition {
    /// `posX`.
    pub pos_x: CssLength,
    /// `posY`.
    pub pos_y: CssLength,
}

/// `CssPropertyValue` — union C → enum Rust.
///
/// Correspondance verbatim au `CssValueType` :
/// `Integer`/`Enum`/`MultiEnum`/`Color`/`FontWeight`/`Auto` →
/// [`CssPropertyValue::Int`] (pour `Auto`, `intVal` vaut
/// `CSS_LENGTH_TYPE_AUTO` = 13 — verbatim) ;
/// `Length*`/`SignedLength` → [`CssPropertyValue::Len`] ;
/// `String`/`Symbol`/`Uri` → [`CssPropertyValue::Str`] ;
/// `BackgroundPosition` → [`CssPropertyValue::Pos`].
#[derive(Debug, Clone, PartialEq)]
pub enum CssPropertyValue {
    /// `intVal`.
    Int(i32),
    /// `lenVal`.
    Len(CssLength),
    /// `strVal` (`NULL` amont ≙ `Str(Vec::new())` — pose par le
    /// parseur pour `background-image: none`, verbatim).
    Str(Vec<u8>),
    /// `posVal`.
    Pos(Box<CssBackgroundPosition>),
    /// `strVal == NULL` amont (`background-image: none`).
    None,
}

impl CssPropertyValue {
    /// `value.intVal` — `None` si le variant ne correspond pas
    /// (les call sites verbatim connaissent le type).
    #[must_use]
    pub fn int_val(&self) -> Option<i32> {
        match self {
            CssPropertyValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// `value.lenVal`.
    #[must_use]
    pub fn len_val(&self) -> Option<CssLength> {
        match self {
            CssPropertyValue::Len(l) => Some(*l),
            _ => None,
        }
    }

    /// `value.strVal`.
    #[must_use]
    pub fn str_val(&self) -> Option<&[u8]> {
        match self {
            CssPropertyValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// `value.strVal` mutable (`font-family` est consommé in-place
    /// amont : `strchr(',')` + `\0` + `memmove`).
    pub fn str_val_mut(&mut self) -> Option<&mut Vec<u8>> {
        match self {
            CssPropertyValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// `value.posVal`.
    #[must_use]
    pub fn pos_val(&self) -> Option<&CssBackgroundPosition> {
        match self {
            CssPropertyValue::Pos(p) => Some(p),
            _ => None,
        }
    }
}

/// `CssProperty` — verbatim (`name`+`type`+union).
#[derive(Debug, Clone, PartialEq)]
pub struct CssProperty {
    /// `name`.
    pub name: i16,
    /// `type` — le `CssValueType` retenu au parse.
    pub ty: CssValueType,
    /// `value`.
    pub value: CssPropertyValue,
}

/// `CssPropertyList` — verbatim `SimpleVector<CssProperty>` +
/// `safe`.
///
/// `ownerOfStrings` n'a pas d'équivalent (les `Vec<u8>` sont toujours
/// possédées — ≙ `ownerOfStrings=true` partout, doc seam) ; le
/// paramètre ctor est conservé pour la correspondance d'API.
#[derive(Debug, Clone)]
pub struct CssPropertyList {
    props: Vec<CssProperty>,
    safe: bool,
}

impl Default for CssPropertyList {
    /// `safe = true` amont (ctor pose toujours `safe = true`).
    fn default() -> Self {
        Self::new(false)
    }
}

impl CssPropertyList {
    /// Ctor `CssPropertyList(bool ownerOfStrings)`.
    #[must_use]
    pub fn new(_owner_of_strings: bool) -> Self {
        CssPropertyList {
            props: Vec::new(),
            safe: true,
        }
    }

    /// `size`.
    #[must_use]
    pub fn size(&self) -> usize {
        self.props.len()
    }

    /// `isEmpty`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.props.is_empty()
    }

    /// `getRef` — accès lecture.
    #[must_use]
    pub fn get(&self, i: usize) -> Option<&CssProperty> {
        self.props.get(i)
    }

    /// `getRef` mutable (`font-family` consommé in-place amont).
    pub fn get_mut(&mut self, i: usize) -> Option<&mut CssProperty> {
        self.props.get_mut(i)
    }

    /// `isSafe` — `false` dès que `display` ou `background-image`
    /// a été posé (verbatim `set`).
    #[must_use]
    pub fn is_safe(&self) -> bool {
        self.safe
    }

    /// `set` — verbatim : remplace la propriété de même nom sinon
    /// ajoute ; `display`/`background-image` marquent la liste
    /// `unsafe`.
    pub fn set(&mut self, name: CssPropertyName, ty: CssValueType, value: CssPropertyValue) {
        if name == CssPropertyName::Display || name == CssPropertyName::BackgroundImage {
            self.safe = false;
        }

        for prop in &mut self.props {
            if prop.name == name as i16 {
                prop.ty = ty;
                prop.value = value;
                return;
            }
        }

        self.props.push(CssProperty {
            name: name as i16,
            ty,
            value,
        });
    }

    /// `apply` — fusionne `self` dans `props` (verbatim : copie
    /// profonde des strings ≙ `ownerOfStrings` du receveur).
    pub fn apply(&self, props: &mut CssPropertyList) {
        for p in &self.props {
            if let Some(name) = CssPropertyName::from_i16(p.name) {
                props.set(name, p.ty, p.value.clone());
            }
        }
    }
}

impl CssPropertyName {
    /// Conversion index → enum (`None` hors `[0, LAST)` ou `END`).
    #[must_use]
    /// Conversion verbatim `Css_property_name` → enum (le match
    /// exhaustif reflète la table amont 1:1).
    #[allow(clippy::too_many_lines)]
    pub fn from_i16(v: i16) -> Option<Self> {
        use CssPropertyName::{
            BackgroundAttachment, BackgroundColor, BackgroundImage, BackgroundPosition,
            BackgroundRepeat, BorderBottomColor, BorderBottomStyle, BorderBottomWidth,
            BorderCollapse, BorderLeftColor, BorderLeftStyle, BorderLeftWidth, BorderRightColor,
            BorderRightStyle, BorderRightWidth, BorderSpacing, BorderTopColor, BorderTopStyle,
            BorderTopWidth, Bottom, CaptionSide, Clear, Clip, Color, Content, CounterIncrement,
            CounterReset, Cursor, Direction, Display, EmptyCells, Float, FontFamily, FontSize,
            FontSizeAdjust, FontStretch, FontStyle, FontVariant, FontWeight, Height, Left,
            LetterSpacing, LineHeight, ListStyleImage, ListStylePosition, ListStyleType,
            MarginBottom, MarginLeft, MarginRight, MarginTop, MarkerOffset, Marks, MaxHeight,
            MaxWidth, MinHeight, MinWidth, OutlineColor, OutlineStyle, OutlineWidth, Overflow,
            PaddingBottom, PaddingLeft, PaddingRight, PaddingTop, Position, PropertyXImg,
            PropertyXLang, PropertyXLink, PropertyXTooltip, Quotes, Right, TextAlign,
            TextDecoration, TextIndent, TextShadow, TextTransform, Top, UnicodeBidi, VerticalAlign,
            Visibility, WhiteSpace, Width, WordSpacing, XColspan, XLink, XRowspan, ZIndex,
        };
        Some(match v {
            0 => BackgroundAttachment,
            1 => BackgroundColor,
            2 => BackgroundImage,
            3 => BackgroundPosition,
            4 => BackgroundRepeat,
            5 => BorderBottomColor,
            6 => BorderBottomStyle,
            7 => BorderBottomWidth,
            8 => BorderCollapse,
            9 => BorderLeftColor,
            10 => BorderLeftStyle,
            11 => BorderLeftWidth,
            12 => BorderRightColor,
            13 => BorderRightStyle,
            14 => BorderRightWidth,
            15 => BorderSpacing,
            16 => BorderTopColor,
            17 => BorderTopStyle,
            18 => BorderTopWidth,
            19 => Bottom,
            20 => CaptionSide,
            21 => Clear,
            22 => Clip,
            23 => Color,
            24 => Content,
            25 => CounterIncrement,
            26 => CounterReset,
            27 => Cursor,
            28 => Direction,
            29 => Display,
            30 => EmptyCells,
            31 => Float,
            32 => FontFamily,
            33 => FontSize,
            34 => FontSizeAdjust,
            35 => FontStretch,
            36 => FontStyle,
            37 => FontVariant,
            38 => FontWeight,
            39 => Height,
            40 => Left,
            41 => LetterSpacing,
            42 => LineHeight,
            43 => ListStyleImage,
            44 => ListStylePosition,
            45 => ListStyleType,
            46 => MarginBottom,
            47 => MarginLeft,
            48 => MarginRight,
            49 => MarginTop,
            50 => MarkerOffset,
            51 => Marks,
            52 => MaxHeight,
            53 => MaxWidth,
            54 => MinHeight,
            55 => MinWidth,
            56 => OutlineColor,
            57 => OutlineStyle,
            58 => OutlineWidth,
            59 => Overflow,
            60 => PaddingBottom,
            61 => PaddingLeft,
            62 => PaddingRight,
            63 => PaddingTop,
            64 => Position,
            65 => Quotes,
            66 => Right,
            67 => TextAlign,
            68 => TextDecoration,
            69 => TextIndent,
            70 => TextShadow,
            71 => TextTransform,
            72 => Top,
            73 => UnicodeBidi,
            74 => VerticalAlign,
            75 => Visibility,
            76 => WhiteSpace,
            77 => Width,
            78 => WordSpacing,
            79 => ZIndex,
            80 => XLink,
            81 => XColspan,
            82 => XRowspan,
            83 => PropertyXLink,
            84 => PropertyXLang,
            85 => PropertyXImg,
            86 => PropertyXTooltip,
            _ => return None,
        })
    }
}

/// `CssPropertyInfo` — verbatim `{symbol, type[3], enum_symbols}`.
pub struct CssPropertyInfo {
    /// `symbol`.
    pub symbol: &'static str,
    /// `type[3]` — types essayés dans l'ordre (`Unused` = fin).
    pub types: [CssValueType; 3],
    /// `enum_symbols` (`NULL` terminé amont → `&[]`).
    pub enum_symbols: &'static [&'static str],
}

const U: CssValueType = CssValueType::Unused;
use CssValueType as T;

/// `Css_property_info` — verbatim, triée alphabétiquement SAUF la
/// queue interne (`x-link`,`x-colspan`,`x-rowspan`,`last`) qui est
/// désordonnée amont + 3 entrées vides (taille `CSS_PROPERTY_LAST`).
///
/// ⚠ la typo amont `border-rigth-width` est conservée — la propriété
/// `border-right-width` n'est donc jamais reconnue par le `bsearch`
/// (verbatim).
pub static CSS_PROPERTY_INFO: [CssPropertyInfo; CSS_PROPERTY_LAST] = [
    CssPropertyInfo {
        symbol: "background-attachment",
        types: [T::Enum, U, U],
        enum_symbols: &["scroll", "fixed"],
    },
    CssPropertyInfo {
        symbol: "background-color",
        types: [T::Color, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "background-image",
        types: [T::Uri, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "background-position",
        types: [T::BackgroundPosition, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "background-repeat",
        types: [T::Enum, U, U],
        enum_symbols: &["repeat", "repeat-x", "repeat-y", "no-repeat"],
    },
    CssPropertyInfo {
        symbol: "border-bottom-color",
        types: [T::Enum, T::Color, U],
        enum_symbols: &["transparent"],
    },
    CssPropertyInfo {
        symbol: "border-bottom-style",
        types: [T::Enum, U, U],
        enum_symbols: &[
            "none", "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset",
            "outset",
        ],
    },
    CssPropertyInfo {
        symbol: "border-bottom-width",
        types: [T::Enum, T::Length, U],
        enum_symbols: &["thin", "medium", "thick"],
    },
    CssPropertyInfo {
        symbol: "border-collapse",
        types: [T::Enum, U, U],
        enum_symbols: &["separate", "collapse"],
    },
    CssPropertyInfo {
        symbol: "border-left-color",
        types: [T::Enum, T::Color, U],
        enum_symbols: &["transparent"],
    },
    CssPropertyInfo {
        symbol: "border-left-style",
        types: [T::Enum, U, U],
        enum_symbols: &[
            "none", "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset",
            "outset",
        ],
    },
    CssPropertyInfo {
        symbol: "border-left-width",
        types: [T::Enum, T::Length, U],
        enum_symbols: &["thin", "medium", "thick"],
    },
    CssPropertyInfo {
        symbol: "border-right-color",
        types: [T::Enum, T::Color, U],
        enum_symbols: &["transparent"],
    },
    CssPropertyInfo {
        symbol: "border-right-style",
        types: [T::Enum, U, U],
        enum_symbols: &[
            "none", "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset",
            "outset",
        ],
    },
    CssPropertyInfo {
        symbol: "border-rigth-width",
        types: [T::Enum, T::Length, U],
        enum_symbols: &["thin", "medium", "thick"],
    },
    CssPropertyInfo {
        symbol: "border-spacing",
        types: [T::Length, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "border-top-color",
        types: [T::Enum, T::Color, U],
        enum_symbols: &["transparent"],
    },
    CssPropertyInfo {
        symbol: "border-top-style",
        types: [T::Enum, U, U],
        enum_symbols: &[
            "none", "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset",
            "outset",
        ],
    },
    CssPropertyInfo {
        symbol: "border-top-width",
        types: [T::Enum, T::Length, U],
        enum_symbols: &["thin", "medium", "thick"],
    },
    CssPropertyInfo {
        symbol: "bottom",
        types: [T::SignedLength, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "caption-side",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "clear",
        types: [T::Enum, U, U],
        enum_symbols: &["left", "right", "both", "none"],
    },
    CssPropertyInfo {
        symbol: "clip",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "color",
        types: [T::Color, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "content",
        types: [T::String, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "counter-increment",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "counter-reset",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "cursor",
        types: [T::Enum, U, U],
        enum_symbols: &[
            "crosshair",
            "default",
            "pointer",
            "move",
            "e-resize",
            "ne-resize",
            "nw-resize",
            "n-resize",
            "se-resize",
            "sw-resize",
            "s-resize",
            "w-resize",
            "text",
            "wait",
            "help",
        ],
    },
    CssPropertyInfo {
        symbol: "direction",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "display",
        types: [T::Enum, U, U],
        enum_symbols: &[
            "block",
            "inline",
            "inline-block",
            "list-item",
            "none",
            "table",
            "table-row-group",
            "table-header-group",
            "table-footer-group",
            "table-row",
            "table-cell",
        ],
    },
    CssPropertyInfo {
        symbol: "empty-cells",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "float",
        types: [T::Enum, U, U],
        enum_symbols: &["none", "left", "right"],
    },
    CssPropertyInfo {
        symbol: "font-family",
        types: [T::Symbol, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "font-size",
        types: [T::Enum, T::LengthPercentage, U],
        enum_symbols: &[
            "large", "larger", "medium", "small", "smaller", "xx-large", "xx-small", "x-large",
            "x-small",
        ],
    },
    CssPropertyInfo {
        symbol: "font-size-adjust",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "font-stretch",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "font-style",
        types: [T::Enum, U, U],
        enum_symbols: &["normal", "italic", "oblique"],
    },
    CssPropertyInfo {
        symbol: "font-variant",
        types: [T::Enum, U, U],
        enum_symbols: &["normal", "small-caps"],
    },
    CssPropertyInfo {
        symbol: "font-weight",
        types: [T::Enum, T::FontWeight, U],
        enum_symbols: &["bold", "bolder", "light", "lighter", "normal"],
    },
    CssPropertyInfo {
        symbol: "height",
        types: [T::LengthPercentage, T::Auto, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "left",
        types: [T::SignedLength, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "letter-spacing",
        types: [T::Enum, T::SignedLength, U],
        enum_symbols: &["normal"],
    },
    CssPropertyInfo {
        symbol: "line-height",
        types: [T::Enum, T::LengthPercentageNumber, U],
        enum_symbols: &["normal"],
    },
    CssPropertyInfo {
        symbol: "list-style-image",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "list-style-position",
        types: [T::Enum, U, U],
        enum_symbols: &["inside", "outside"],
    },
    CssPropertyInfo {
        symbol: "list-style-type",
        types: [T::Enum, U, U],
        enum_symbols: &[
            "disc",
            "circle",
            "square",
            "decimal",
            "decimal-leading-zero",
            "lower-roman",
            "upper-roman",
            "lower-greek",
            "lower-alpha",
            "lower-latin",
            "upper-alpha",
            "upper-latin",
            "hebrew",
            "armenian",
            "georgian",
            "cjk-ideographic",
            "hiragana",
            "katakana",
            "hiragana-iroha",
            "katakana-iroha",
            "none",
        ],
    },
    CssPropertyInfo {
        symbol: "margin-bottom",
        types: [T::SignedLength, T::Auto, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "margin-left",
        types: [T::SignedLength, T::Auto, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "margin-right",
        types: [T::SignedLength, T::Auto, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "margin-top",
        types: [T::SignedLength, T::Auto, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "marker-offset",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "marks",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "max-height",
        types: [T::LengthPercentage, T::Auto, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "max-width",
        types: [T::LengthPercentage, T::Auto, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "min-height",
        types: [T::LengthPercentage, T::Auto, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "min-width",
        types: [T::LengthPercentage, T::Auto, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "outline-color",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "outline-style",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "outline-width",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "overflow",
        types: [T::Enum, U, U],
        enum_symbols: &["visible", "hidden", "scroll", "auto"],
    },
    CssPropertyInfo {
        symbol: "padding-bottom",
        types: [T::Length, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "padding-left",
        types: [T::Length, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "padding-right",
        types: [T::Length, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "padding-top",
        types: [T::Length, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "position",
        types: [T::Enum, U, U],
        enum_symbols: &["static", "relative", "absolute", "fixed"],
    },
    CssPropertyInfo {
        symbol: "quotes",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "right",
        types: [T::SignedLength, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "text-align",
        types: [T::Enum, U, U],
        enum_symbols: &["left", "right", "center", "justify", "string"],
    },
    CssPropertyInfo {
        symbol: "text-decoration",
        types: [T::MultiEnum, U, U],
        enum_symbols: &["underline", "overline", "line-through", "blink"],
    },
    CssPropertyInfo {
        symbol: "text-indent",
        types: [T::LengthPercentage, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "text-shadow",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "text-transform",
        types: [T::Enum, U, U],
        enum_symbols: &["none", "capitalize", "uppercase", "lowercase"],
    },
    CssPropertyInfo {
        symbol: "top",
        types: [T::SignedLength, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "unicode-bidi",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "vertical-align",
        types: [T::Enum, U, U],
        enum_symbols: &[
            "top",
            "bottom",
            "middle",
            "baseline",
            "sub",
            "super",
            "text-top",
            "text-bottom",
        ],
    },
    CssPropertyInfo {
        symbol: "visibility",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "white-space",
        types: [T::Enum, U, U],
        enum_symbols: &["normal", "pre", "nowrap", "pre-wrap", "pre-line"],
    },
    CssPropertyInfo {
        symbol: "width",
        types: [T::LengthPercentage, T::Auto, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "word-spacing",
        types: [T::Enum, T::SignedLength, U],
        enum_symbols: &["normal"],
    },
    CssPropertyInfo {
        symbol: "z-index",
        types: [T::Integer, T::Auto, U],
        enum_symbols: &[],
    },
    /* Extensions internes — jamais parsées, ordre non trié amont. */
    CssPropertyInfo {
        symbol: "x-link",
        types: [T::Integer, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "x-colspan",
        types: [T::Integer, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "x-rowspan",
        types: [T::Integer, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "last",
        types: [U, U, U],
        enum_symbols: &[],
    },
    /* 3 entrées finales zéro-init amont (`{NULL,{0},NULL}`). */
    CssPropertyInfo {
        symbol: "",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "",
        types: [U, U, U],
        enum_symbols: &[],
    },
    CssPropertyInfo {
        symbol: "",
        types: [U, U, U],
        enum_symbols: &[],
    },
];

/// `CssShorthandInfo::type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssShorthandType {
    /// `CSS_SHORTHAND_MULTIPLE` — `[p1 || p2 || ...]`, propriété
    /// déterminée par le type.
    Multiple,
    /// `CSS_SHORTHAND_DIRECTIONS` — `<t>{1,4}`.
    Directions,
    /// `CSS_SHORTHAND_BORDER` — spécial, `border`.
    Border,
    /// `CSS_SHORTHAND_FONT` — spécial, `font` (même chemin que
    /// `MULTIPLE` amont — `\todo` non implémenté).
    Font,
}

/// `CssShorthandInfo` — verbatim.
pub struct CssShorthandInfo {
    /// `symbol`.
    pub symbol: &'static str,
    /// `type`.
    pub ty: CssShorthandType,
    /// `properties` (`CSS_PROPERTY_END` terminé pour `Multiple`,
    /// longueur 4 pour `Directions`, 12 pour `Border`).
    pub properties: &'static [i16],
}

/// `Css_shorthand_info` — verbatim (triée pour `bsearch`).
pub static CSS_SHORTHAND_INFO: &[CssShorthandInfo] = &[
    CssShorthandInfo {
        symbol: "background",
        ty: CssShorthandType::Multiple,
        properties: &[1, 2, 4, 0, 3, CSS_PROPERTY_END],
    },
    CssShorthandInfo {
        symbol: "border",
        ty: CssShorthandType::Border,
        properties: &[18, 17, 16, 7, 6, 5, 11, 10, 9, 14, 13, 12],
    },
    CssShorthandInfo {
        symbol: "border-bottom",
        ty: CssShorthandType::Multiple,
        properties: &[7, 6, 5, CSS_PROPERTY_END],
    },
    CssShorthandInfo {
        symbol: "border-color",
        ty: CssShorthandType::Directions,
        properties: &[16, 5, 9, 12],
    },
    CssShorthandInfo {
        symbol: "border-left",
        ty: CssShorthandType::Multiple,
        properties: &[11, 10, 9, CSS_PROPERTY_END],
    },
    CssShorthandInfo {
        symbol: "border-right",
        ty: CssShorthandType::Multiple,
        properties: &[14, 13, 12, CSS_PROPERTY_END],
    },
    CssShorthandInfo {
        symbol: "border-style",
        ty: CssShorthandType::Directions,
        properties: &[17, 6, 10, 13],
    },
    CssShorthandInfo {
        symbol: "border-top",
        ty: CssShorthandType::Multiple,
        properties: &[18, 17, 16, CSS_PROPERTY_END],
    },
    CssShorthandInfo {
        symbol: "border-width",
        ty: CssShorthandType::Directions,
        properties: &[18, 7, 11, 14],
    },
    CssShorthandInfo {
        symbol: "font",
        ty: CssShorthandType::Font,
        properties: &[33, 36, 37, 38, 32, CSS_PROPERTY_END],
    },
    CssShorthandInfo {
        symbol: "list-style",
        ty: CssShorthandType::Multiple,
        properties: &[45, 44, 43, CSS_PROPERTY_END],
    },
    CssShorthandInfo {
        symbol: "margin",
        ty: CssShorthandType::Directions,
        properties: &[49, 46, 47, 48],
    },
    CssShorthandInfo {
        symbol: "outline",
        ty: CssShorthandType::Multiple,
        properties: &[56, 57, 58, CSS_PROPERTY_END],
    },
    CssShorthandInfo {
        symbol: "padding",
        ty: CssShorthandType::Directions,
        properties: &[63, 60, 61, 62],
    },
];

/// `CSS_SHORTHAND_NUM`.
pub const CSS_SHORTHAND_NUM: usize = 14;

/* ------------------------------------------------------------------ */
/*    Sélecteurs                                                       */
/* ------------------------------------------------------------------ */

/// `CssSimpleSelector::ELEMENT_NONE`.
pub const ELEMENT_NONE: i32 = -1;
/// `CssSimpleSelector::ELEMENT_ANY`.
pub const ELEMENT_ANY: i32 = -2;

/// `CssSimpleSelector::SelectType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectType {
    /// `SELECT_NONE`.
    None,
    /// `SELECT_CLASS`.
    Class,
    /// `SELECT_PSEUDO_CLASS`.
    PseudoClass,
    /// `SELECT_ID`.
    Id,
}

/// `CssSimpleSelector` — verbatim.
#[derive(Debug, Clone)]
pub struct CssSimpleSelector {
    /// `element` — `ELEMENT_ANY` par défaut amont.
    pub element: i32,
    /// `pseudo`.
    pub pseudo: Option<Vec<u8>>,
    /// `id`.
    pub id: Option<Vec<u8>>,
    /// `klass`.
    pub klass: Vec<Vec<u8>>,
}

impl CssSimpleSelector {
    /// Ctor (`element = ELEMENT_ANY` amont).
    #[must_use]
    pub fn new() -> Self {
        CssSimpleSelector {
            element: ELEMENT_ANY,
            pseudo: None,
            id: None,
            klass: Vec::new(),
        }
    }

    /// `setElement`.
    pub fn set_element(&mut self, e: i32) {
        self.element = e;
    }

    /// `setSelect` — verbatim (`pseudo`/`id` ne sont posés que la
    /// première fois).
    pub fn set_select(&mut self, t: SelectType, v: &[u8]) {
        match t {
            SelectType::Class => self.klass.push(v.to_vec()),
            SelectType::PseudoClass => {
                if self.pseudo.is_none() {
                    self.pseudo = Some(v.to_vec());
                }
            }
            SelectType::Id => {
                if self.id.is_none() {
                    self.id = Some(v.to_vec());
                }
            }
            SelectType::None => {}
        }
    }

    /// `match` — verbatim (comparaisons `dStrAsciiCasecmp`).
    #[must_use]
    pub fn matches(&self, n: &crate::doctree::DoctreeNode) -> bool {
        if self.element != ELEMENT_ANY && self.element != n.element {
            return false;
        }
        if let Some(p) = &self.pseudo {
            match &n.pseudo {
                Some(np) if str_ascii_casecmp(p, np) == 0 => {}
                _ => return false,
            }
        }
        if let Some(id) = &self.id {
            match &n.id {
                Some(nid) if str_ascii_casecmp(id, nid) == 0 => {}
                _ => return false,
            }
        }
        for k in &self.klass {
            let found = n
                .klass
                .as_ref()
                .is_some_and(|nk| nk.iter().any(|c| str_ascii_casecmp(k, c) == 0));
            if !found {
                return false;
            }
        }
        true
    }

    /// `specificity` — verbatim (`id << 20`, classes+pseudo `<< 10`,
    /// élément `<< 0`).
    #[must_use]
    pub fn specificity(&self) -> i32 {
        let mut spec = 0;
        if self.id.is_some() {
            spec += 1 << 20;
        }
        spec += (self.klass.len() as i32) << 10;
        if self.pseudo.is_some() {
            spec += 1 << 10;
        }
        if self.element != ELEMENT_ANY {
            spec += 1;
        }
        spec
    }
}

impl Default for CssSimpleSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// `MatchCache` — verbatim `SimpleVector<int>` rempli à -1.
pub type MatchCache = Vec<i32>;

/// `CssSelector::Combinator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    /// `COMB_NONE`.
    None,
    /// `COMB_DESCENDANT`.
    Descendant,
    /// `COMB_CHILD`.
    Child,
    /// `COMB_ADJACENT_SIBLING`.
    AdjacentSibling,
}

/// `CombinatorAndSelector` — verbatim.
#[derive(Debug)]
struct CombinatorAndSelector {
    combinator: Combinator,
    selector: CssSimpleSelector,
}

/// `CssSelector` — verbatim (chaîne de `CssSimpleSelector` avec
/// combinateur par étape, `matchCacheOffset` posé une fois).
///
/// `selectorList` amont est un `SimpleVector` créé avec 1 élément →
/// le port stocke `first` hors du `Vec` : la liste est non vide par
/// construction, `top()` ne peut pas être nul (verbatim).
#[derive(Debug)]
pub struct CssSelector {
    match_cache_offset: std::cell::Cell<i32>,
    first: CssSimpleSelector,
    /// `selectorList[1..]` — `first` ≙ `selectorList[0]` avec
    /// `COMB_NONE`.
    rest: Vec<CombinatorAndSelector>,
}

impl Default for CssSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl CssSelector {
    /// Ctor — un `CssSimpleSelector` initial, `COMB_NONE`,
    /// `matchCacheOffset = -1`.
    #[must_use]
    pub fn new() -> Self {
        CssSelector {
            match_cache_offset: std::cell::Cell::new(-1),
            first: CssSimpleSelector::new(),
            rest: Vec::new(),
        }
    }

    /// `addSimpleSelector` — pousse un nouveau maillon
    /// (`assert matchCacheOffset == -1` amont ≙ pas encore dans une
    /// feuille).
    pub fn add_simple_selector(&mut self, c: Combinator) {
        debug_assert_eq!(self.match_cache_offset.get(), -1);
        self.rest.push(CombinatorAndSelector {
            combinator: c,
            selector: CssSimpleSelector::new(),
        });
    }

    /// `top` — dernier `CssSimpleSelector` (mutable pour le parseur).
    pub fn top_mut(&mut self) -> &mut CssSimpleSelector {
        match self.rest.last_mut() {
            Some(cs) => &mut cs.selector,
            None => &mut self.first,
        }
    }

    /// `top` — lecture.
    #[must_use]
    pub fn top(&self) -> &CssSimpleSelector {
        match self.rest.last() {
            Some(cs) => &cs.selector,
            None => &self.first,
        }
    }

    /// `size`.
    #[must_use]
    pub fn size(&self) -> usize {
        1 + self.rest.len()
    }

    /// `setMatchCacheOffset` — posé une seule fois (verbatim).
    pub fn set_match_cache_offset(&self, mo: i32) {
        if self.match_cache_offset.get() == -1 {
            self.match_cache_offset.set(mo);
        }
    }

    /// `getRequiredMatchCache`.
    #[must_use]
    pub fn get_required_match_cache(&self) -> i32 {
        self.match_cache_offset.get() + self.size() as i32
    }

    /// Accès interne à `selectorList[i]` (`first` ≙ index 0).
    fn at(&self, i: i32) -> (&CssSimpleSelector, Combinator) {
        if i == 0 {
            (&self.first, Combinator::None)
        } else {
            let cs = &self.rest[(i - 1) as usize];
            (&cs.selector, cs.combinator)
        }
    }

    /// `match` — verbatim (récursif, `matchCache` partagé).
    #[must_use]
    pub fn matches(&self, dt: &Doctree, node: NodeRef, match_cache: &mut MatchCache) -> bool {
        self.match_rec(
            dt,
            node,
            self.size() as i32 - 1,
            Combinator::None,
            match_cache,
        )
    }

    /// `match (docTree, node, i, comb, matchCache)` — verbatim.
    fn match_rec(
        &self,
        dt: &Doctree,
        mut node: NodeRef,
        i: i32,
        comb: Combinator,
        match_cache: &mut MatchCache,
    ) -> bool {
        if i < 0 {
            return true;
        }

        let (sel, cs_comb) = self.at(i);

        match comb {
            Combinator::None => {}
            Combinator::Child => match dt.parent(node) {
                Some(p) => node = p,
                None => return false,
            },
            Combinator::AdjacentSibling => match dt.sibling(node) {
                Some(s) => node = s,
                None => return false,
            },
            Combinator::Descendant => {
                let Some(start) = dt.parent(node) else {
                    // verbatim : node == NULL → la boucle ne tourne
                    // pas et `if (node)` garde l'entrée à -1.
                    return false;
                };

                let cache_idx = (self.match_cache_offset.get() + i) as usize;
                let mut n = Some(start);
                while let Some(cur) = n {
                    let cur_num = dt.node(cur).num;
                    let entry = match_cache.get(cache_idx).copied().unwrap_or(-1);
                    if cur_num <= entry {
                        break;
                    }
                    if sel.matches(dt.node(cur))
                        && self.match_rec(dt, cur, i - 1, cs_comb, match_cache)
                    {
                        return true;
                    }
                    n = dt.parent(cur);
                }

                // verbatim : mémorise le num du nœud où la recherche
                // a échoué pour éviter de retester.
                if let Some(e) = match_cache.get_mut(cache_idx) {
                    *e = dt.node(start).num;
                }
                return false;
            }
        }

        if !sel.matches(dt.node(node)) {
            return false;
        }
        self.match_rec(dt, node, i - 1, cs_comb, match_cache)
    }

    /// `checksPseudoClass`.
    #[must_use]
    pub fn checks_pseudo_class(&self) -> bool {
        self.first.pseudo.is_some() || self.rest.iter().any(|cs| cs.selector.pseudo.is_some())
    }

    /// `specificity` — somme des `CssSimpleSelector`.
    #[must_use]
    pub fn specificity(&self) -> i32 {
        self.first.specificity()
            + self
                .rest
                .iter()
                .map(|cs| cs.selector.specificity())
                .sum::<i32>()
    }
}

/* ------------------------------------------------------------------ */
/*    Règles et feuilles de style                                      */
/* ------------------------------------------------------------------ */

/// `CssRule` — verbatim (selector + props + spécificité + position).
pub struct CssRule {
    /// `selector` (partagé `Rc` ≙ refcount amont).
    pub selector: Rc<CssSelector>,
    /// `props`.
    props: Rc<CssPropertyList>,
    /// `spec`.
    spec: i32,
    /// `pos`.
    pos: i32,
}

impl CssRule {
    /// Ctor — `assert selector->size() > 0` amont.
    #[must_use]
    pub fn new(selector: Rc<CssSelector>, props: Rc<CssPropertyList>, pos: i32) -> Self {
        debug_assert!(selector.size() > 0);
        let spec = selector.specificity();
        CssRule {
            selector,
            props,
            spec,
            pos,
        }
    }

    /// `apply` — verbatim.
    pub fn apply(
        &self,
        props: &mut CssPropertyList,
        doc_tree: &Doctree,
        node: NodeRef,
        match_cache: &mut MatchCache,
    ) {
        if self.selector.matches(doc_tree, node, match_cache) {
            self.props.apply(props);
        }
    }

    /// `isSafe` — verbatim (pseudo-classe + propriété unsafe → rejet).
    #[must_use]
    pub fn is_safe(&self) -> bool {
        !self.selector.checks_pseudo_class() || self.props.is_safe()
    }

    /// `specificity`.
    #[must_use]
    pub fn specificity(&self) -> i32 {
        self.spec
    }

    /// `position`.
    #[must_use]
    pub fn position(&self) -> i32 {
        self.pos
    }
}

/// `CssStyleSheet::RuleList` — verbatim (tri par spécificité
/// croissante, insertion après les égaux).
#[derive(Default)]
struct RuleList {
    rules: Vec<Rc<CssRule>>,
}

impl RuleList {
    /// `insert` — verbatim.
    fn insert(&mut self, rule: Rc<CssRule>) {
        let mut i = self.rules.len();
        while i > 0 && rule.specificity() < self.rules[i - 1].specificity() {
            i -= 1;
        }
        self.rules.insert(i, rule);
    }
}

/// `CssStyleSheet` — verbatim (indexation par élément / id / classe /
/// universel).
pub struct CssStyleSheet {
    element_table: Vec<RuleList>,
    any_table: RuleList,
    id_table: HashMap<Vec<u8>, RuleList>,
    class_table: HashMap<Vec<u8>, RuleList>,
    required_match_cache: i32,
}

impl Default for CssStyleSheet {
    fn default() -> Self {
        Self::new()
    }
}

impl CssStyleSheet {
    /// Ctor.
    #[must_use]
    pub fn new() -> Self {
        CssStyleSheet {
            element_table: (0..HTML_NTAGS).map(|_| RuleList::default()).collect(),
            any_table: RuleList::default(),
            id_table: HashMap::new(),
            class_table: HashMap::new(),
            required_match_cache: 0,
        }
    }

    /// `getRequiredMatchCache`.
    #[must_use]
    pub fn get_required_match_cache(&self) -> i32 {
        self.required_match_cache
    }

    /// `addRule` — verbatim : indexe sur le topmost simple selector
    /// (id > classe > élément > universel ; `ELEMENT_NONE` → rejet).
    pub fn add_rule(&mut self, rule: CssRule) {
        let rule = Rc::new(rule);
        let top = rule.selector.top();

        let target: Option<&mut RuleList> = if let Some(id) = &top.id {
            Some(self.id_table.entry(id.clone()).or_default())
        } else if !top.klass.is_empty() {
            Some(self.class_table.entry(top.klass[0].clone()).or_default())
        } else if top.element >= 0 && (top.element as usize) < HTML_NTAGS {
            Some(&mut self.element_table[top.element as usize])
        } else if top.element == ELEMENT_ANY {
            Some(&mut self.any_table)
        } else {
            None
        };

        if let Some(rl) = target {
            rl.insert(rule.clone());
            if rule.selector.get_required_match_cache() > self.required_match_cache {
                self.required_match_cache = rule.selector.get_required_match_cache();
            }
        }
        // verbatim : `ELEMENT_NONE` (et tout autre cas sans liste) →
        // la règle est jetée (`delete rule` amont).
    }

    /// `apply` — verbatim : fusion des listes candidates, application
    /// par (spécificité, position) croissants.
    pub fn apply(
        &self,
        props: &mut CssPropertyList,
        doc_tree: &Doctree,
        node: NodeRef,
        match_cache: &mut MatchCache,
    ) {
        const MAX_LISTS: usize = 32;
        let mut rule_lists: [&RuleList; MAX_LISTS] = std::array::from_fn(|_| &self.any_table);
        let mut num_lists = 0usize;
        let mut index = [0usize; MAX_LISTS];

        let n = doc_tree.node(node);

        if let Some(id) = &n.id {
            if let Some(rl) = self.id_table.get(id) {
                rule_lists[num_lists] = rl;
                num_lists += 1;
            }
        }

        if let Some(klass) = &n.klass {
            for (i, k) in klass.iter().enumerate() {
                if i >= MAX_LISTS - 4 {
                    tracing::warn!("Maximum number of classes per element exceeded.");
                    break;
                }
                if let Some(rl) = self.class_table.get(k) {
                    rule_lists[num_lists] = rl;
                    num_lists += 1;
                }
            }
        }

        // verbatim `elementTable[node->element]` — amont indexe sans
        // borne (UB si element < 0 ou >= NTAGS) : guard documenté.
        if (0..HTML_NTAGS as i32).contains(&n.element) {
            rule_lists[num_lists] = &self.element_table[n.element as usize];
            num_lists += 1;
        }

        rule_lists[num_lists] = &self.any_table;
        num_lists += 1;

        // Application par spécificité croissante, position croissante
        // à spécificité égale (verbatim — chaque liste est déjà triée).
        loop {
            let mut min_spec = 1 << 30;
            let mut min_pos = 1 << 30;
            let mut min_index: i32 = -1;

            for i in 0..num_lists {
                let rl = rule_lists[i];
                if let Some(rule) = rl.rules.get(index[i]) {
                    if rule.specificity() < min_spec
                        || (rule.specificity() == min_spec && rule.position() < min_pos)
                    {
                        min_spec = rule.specificity();
                        min_pos = rule.position();
                        min_index = i as i32;
                    }
                }
            }

            if min_index >= 0 {
                let i = min_index as usize;
                if let Some(rule) = rule_lists[i].rules.get(index[i]) {
                    rule.apply(props, doc_tree, node, match_cache);
                }
                index[i] += 1;
            } else {
                break;
            }
        }
    }
}

/* ------------------------------------------------------------------ */
/*    CssContext                                                        */
/* ------------------------------------------------------------------ */

// `CssContext::userAgentSheet` — feuille statique partagée (remplie
// par `crate::styleengine::StyleEngine::init`).
//
// Amont : `static` process-wide. Le port utilise `thread_local` —
// le CSS vit sur le thread UI (mono-thread, §10 AGENTS) : observable
// identique, et évite `Arc<Mutex>` sur les `Rc` internes.
thread_local! {
    static USER_AGENT_SHEET: std::cell::RefCell<CssStyleSheet> =
        std::cell::RefCell::new(CssStyleSheet::new());
}

/// `CssContext` — verbatim (ensemble de feuilles + cache + `pos`).
pub struct CssContext {
    /// `sheet[CSS_PRIMARY_USER..CSS_PRIMARY_USER_IMPORTANT]`.
    sheets: [CssStyleSheet; 4],
    match_cache: MatchCache,
    pos: i32,
}

impl Default for CssContext {
    fn default() -> Self {
        Self::new()
    }
}

impl CssContext {
    /// Ctor — `matchCache` dimensionné pour la feuille UA.
    #[must_use]
    pub fn new() -> Self {
        let ua_cache = USER_AGENT_SHEET.with(|s| s.borrow().get_required_match_cache());
        CssContext {
            sheets: [
                CssStyleSheet::new(),
                CssStyleSheet::new(),
                CssStyleSheet::new(),
                CssStyleSheet::new(),
            ],
            match_cache: vec![-1; ua_cache as usize],
            pos: 0,
        }
    }

    /// `addRule` — verbatim : rejet des règles auteur unsafe,
    /// extension du `matchCache`, routage vers la bonne feuille.
    pub fn add_rule(
        &mut self,
        sel: Rc<CssSelector>,
        props: Rc<CssPropertyList>,
        order: CssPrimaryOrder,
    ) {
        if props.size() == 0 {
            return;
        }

        let rule = CssRule::new(sel, props, self.pos);
        self.pos += 1;

        if matches!(
            order,
            CssPrimaryOrder::Author | CssPrimaryOrder::AuthorImportant
        ) && !rule.is_safe()
        {
            tracing::warn!("Ignoring unsafe author style that might reveal browsing history");
            return;
        }

        rule.selector
            .set_match_cache_offset(self.match_cache.len() as i32);
        if rule.selector.get_required_match_cache() > self.match_cache.len() as i32 {
            self.match_cache
                .resize(rule.selector.get_required_match_cache() as usize, -1);
        }

        if order == CssPrimaryOrder::UserAgent {
            USER_AGENT_SHEET.with(|s| s.borrow_mut().add_rule(rule));
        } else {
            self.sheets[order as usize - 1].add_rule(rule);
        }
    }

    /// `apply` — verbatim (ordre de cascade CSS 2.1 amont).
    pub fn apply(
        &mut self,
        props: &mut CssPropertyList,
        doc_tree: &Doctree,
        node: NodeRef,
        tag_style: Option<&CssPropertyList>,
        tag_style_important: Option<&CssPropertyList>,
        non_css_hints: Option<&CssPropertyList>,
    ) {
        USER_AGENT_SHEET.with(|ua| {
            ua.borrow()
                .apply(props, doc_tree, node, &mut self.match_cache);
        });

        self.sheets[0].apply(props, doc_tree, node, &mut self.match_cache);

        if let Some(hints) = non_css_hints {
            hints.apply(props);
        }

        self.sheets[1].apply(props, doc_tree, node, &mut self.match_cache);

        if let Some(ts) = tag_style {
            ts.apply(props);
        }

        self.sheets[2].apply(props, doc_tree, node, &mut self.match_cache);

        if let Some(tsi) = tag_style_important {
            tsi.apply(props);
        }

        self.sheets[3].apply(props, doc_tree, node, &mut self.match_cache);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctree::Doctree;
    use std::rc::Rc;

    const E_DIV: i32 = 2;
    const E_P: i32 = 3;
    const E_SPAN: i32 = 4;

    /// `matchCache` dimensionné pour un sélecteur.
    fn cache_for(sel: &CssSelector) -> MatchCache {
        vec![-1; sel.get_required_match_cache().max(0) as usize]
    }

    fn color_of(out: &CssPropertyList) -> Option<i32> {
        (0..out.size())
            .filter_map(|i| out.get(i))
            .find(|p| p.name == CssPropertyName::Color as i16)
            .and_then(|p| p.value.int_val())
    }

    #[test]
    fn length_roundtrip() {
        let l = css_create_length(12.5, CssLengthType::Mm);
        assert_eq!(css_length_type(l), CssLengthType::Mm);
        assert!((css_length_value(l) - 12.5).abs() < 0.001);
        assert_eq!(
            css_length_type(css_create_length(0.0, CssLengthType::Auto)),
            CssLengthType::Auto
        );
    }

    #[test]
    fn property_list_set_overrides() {
        let mut pl = CssPropertyList::new(true);
        pl.set(
            CssPropertyName::Color,
            CssValueType::Integer,
            CssPropertyValue::Int(0xff0000),
        );
        pl.set(
            CssPropertyName::Color,
            CssValueType::Integer,
            CssPropertyValue::Int(0x00ff00),
        );
        assert_eq!(pl.size(), 1);
        assert_eq!(pl.get(0).and_then(|p| p.value.int_val()), Some(0x00ff00));
    }

    #[test]
    fn property_list_apply_merges() {
        let mut a = CssPropertyList::new(true);
        a.set(
            CssPropertyName::Color,
            CssValueType::Integer,
            CssPropertyValue::Int(1),
        );
        let mut b = CssPropertyList::new(true);
        b.set(
            CssPropertyName::FontWeight,
            CssValueType::Integer,
            CssPropertyValue::Int(700),
        );
        b.set(
            CssPropertyName::Color,
            CssValueType::Integer,
            CssPropertyValue::Int(2),
        );
        b.apply(&mut a);
        assert_eq!(a.size(), 2);
        assert_eq!(color_of(&a), Some(2));
    }

    #[test]
    fn simple_selector_element_class_id() {
        let mut dt = Doctree::new();
        let n = dt.push();
        {
            let node = dt.node_mut(n);
            node.element = E_DIV;
            node.id = Some(b"main".to_vec());
            node.klass = Some(vec![b"foo".to_vec(), b"bar".to_vec()]);
        }
        let node = dt.node(n).clone();

        let mut s = CssSimpleSelector::new();
        s.set_element(E_DIV);
        assert!(s.matches(&node));
        s.set_element(E_P);
        assert!(!s.matches(&node));

        let mut s = CssSimpleSelector::new();
        s.set_select(SelectType::Class, b"foo");
        assert!(s.matches(&node));
        let mut s2 = CssSimpleSelector::new();
        s2.set_select(SelectType::Class, b"baz");
        assert!(!s2.matches(&node));

        let mut s = CssSimpleSelector::new();
        s.set_select(SelectType::Id, b"MAIN"); // case-insensitive
        assert!(s.matches(&node));
    }

    #[test]
    fn simple_selector_pseudo_case_insensitive() {
        let mut dt = Doctree::new();
        let n = dt.push();
        dt.node_mut(n).pseudo = Some(b"link".to_vec());
        let node = dt.node(n).clone();
        let mut s = CssSimpleSelector::new();
        s.set_select(SelectType::PseudoClass, b"LINK");
        assert!(s.matches(&node));
        let mut s2 = CssSimpleSelector::new();
        s2.set_select(SelectType::PseudoClass, b"visited");
        assert!(!s2.matches(&node));
    }

    #[test]
    fn specificity_order() {
        let mut s = CssSimpleSelector::new();
        s.set_element(E_DIV);
        let e = s.specificity();
        s.set_select(SelectType::Class, b"a");
        let ec = s.specificity();
        s.set_select(SelectType::Id, b"x");
        let eci = s.specificity();
        assert_eq!(e, 1);
        assert_eq!(ec, (1 << 10) + 1);
        assert_eq!(eci, (1 << 20) + (1 << 10) + 1);
    }

    fn sel_with(top_el: i32) -> CssSelector {
        let mut s = CssSelector::new();
        s.top_mut().set_element(top_el);
        s
    }

    #[test]
    fn descendant_combinator_walks_parents() {
        let mut dt = Doctree::new();
        let d = dt.push();
        dt.node_mut(d).element = E_DIV;
        let p = dt.push();
        dt.node_mut(p).element = E_P;

        let mut sel = sel_with(E_DIV); // leftmost
        sel.add_simple_selector(Combinator::Descendant);
        sel.top_mut().set_element(E_P); // sujet = top()

        let mut cache = cache_for(&sel);
        assert!(sel.matches(&dt, p, &mut cache));
    }

    #[test]
    fn child_combinator_only_direct() {
        let mut dt = Doctree::new();
        let d = dt.push();
        dt.node_mut(d).element = E_DIV;
        let s = dt.push();
        dt.node_mut(s).element = E_SPAN;
        let p = dt.push();
        dt.node_mut(p).element = E_P;

        let mut sel = sel_with(E_DIV);
        sel.add_simple_selector(Combinator::Child);
        sel.top_mut().set_element(E_P);

        let mut cache = cache_for(&sel);
        assert!(!sel.matches(&dt, p, &mut cache));

        let mut sel2 = sel_with(E_SPAN);
        sel2.add_simple_selector(Combinator::Child);
        sel2.top_mut().set_element(E_P);
        let mut cache2 = cache_for(&sel2);
        assert!(sel2.matches(&dt, p, &mut cache2));
    }

    #[test]
    fn adjacent_sibling_combinator() {
        // sibling = frere *precedent* (insertion en tete, verbatim)
        let mut dt = Doctree::new();
        dt.push(); // parent
        let sp = dt.push();
        dt.node_mut(sp).element = E_SPAN;
        dt.pop();
        let p = dt.push();
        dt.node_mut(p).element = E_P;

        let mut sel = sel_with(E_SPAN);
        sel.add_simple_selector(Combinator::AdjacentSibling);
        sel.top_mut().set_element(E_P);
        let mut cache = cache_for(&sel);
        assert!(sel.matches(&dt, p, &mut cache));

        let mut sel2 = sel_with(E_P);
        sel2.add_simple_selector(Combinator::AdjacentSibling);
        sel2.top_mut().set_element(E_P);
        let mut cache2 = cache_for(&sel2);
        assert!(!sel2.matches(&dt, p, &mut cache2));
    }

    #[test]
    fn stylesheet_routes_by_element() {
        let mut sheet = CssStyleSheet::new();
        let mut props = CssPropertyList::new(true);
        props.set(
            CssPropertyName::Color,
            CssValueType::Integer,
            CssPropertyValue::Int(7),
        );
        let sel = Rc::new(sel_with(E_DIV));
        sheet.add_rule(CssRule::new(sel, Rc::new(props), 0));

        let mut dt = Doctree::new();
        let d = dt.push();
        dt.node_mut(d).element = E_DIV;
        let other = dt.push();
        dt.node_mut(other).element = E_P;

        let mut out = CssPropertyList::new(true);
        let mut cache = vec![-1; sheet.get_required_match_cache().max(0) as usize];
        sheet.apply(&mut out, &dt, d, &mut cache);
        assert_eq!(out.size(), 1);

        let mut out2 = CssPropertyList::new(true);
        sheet.apply(&mut out2, &dt, other, &mut cache);
        assert_eq!(out2.size(), 0);
    }

    #[test]
    fn stylesheet_element_none_rejected() {
        let mut sheet = CssStyleSheet::new();
        let mut props = CssPropertyList::new(true);
        props.set(
            CssPropertyName::Color,
            CssValueType::Integer,
            CssPropertyValue::Int(7),
        );
        let mut sel = CssSelector::new();
        sel.top_mut().set_element(ELEMENT_NONE);
        sheet.add_rule(CssRule::new(Rc::new(sel), Rc::new(props), 0));

        let mut dt = Doctree::new();
        let n = dt.push();
        dt.node_mut(n).element = ELEMENT_NONE;
        let mut out = CssPropertyList::new(true);
        let mut cache = Vec::new();
        sheet.apply(&mut out, &dt, n, &mut cache);
        assert_eq!(out.size(), 0);
    }

    #[test]
    fn unsafe_author_rule_with_pseudo_rejected() {
        let mut ctx = CssContext::new();
        let mut props = CssPropertyList::new(true);
        props.set(
            CssPropertyName::Display,
            CssValueType::Integer,
            CssPropertyValue::Int(1),
        );
        let mut sel = CssSelector::new();
        sel.top_mut().set_element(ELEMENT_ANY);
        sel.top_mut()
            .set_select(SelectType::PseudoClass, b"visited");
        ctx.add_rule(Rc::new(sel), Rc::new(props), CssPrimaryOrder::Author);

        let mut dt = Doctree::new();
        let n = dt.push();
        dt.node_mut(n).pseudo = Some(b"visited".to_vec());
        let mut out = CssPropertyList::new(true);
        ctx.apply(&mut out, &dt, n, None, None, None);
        assert_eq!(out.size(), 0);
    }

    #[test]
    fn safe_author_rule_with_pseudo_accepted() {
        let mut ctx = CssContext::new();
        let mut props = CssPropertyList::new(true);
        props.set(
            CssPropertyName::BorderBottomColor,
            CssValueType::Integer,
            CssPropertyValue::Int(3),
        );
        let mut sel = CssSelector::new();
        sel.top_mut().set_element(ELEMENT_ANY);
        sel.top_mut().set_select(SelectType::PseudoClass, b"link");
        ctx.add_rule(Rc::new(sel), Rc::new(props), CssPrimaryOrder::Author);

        let mut dt = Doctree::new();
        let n = dt.push();
        dt.node_mut(n).pseudo = Some(b"link".to_vec());
        let mut out = CssPropertyList::new(true);
        ctx.apply(&mut out, &dt, n, None, None, None);
        assert_eq!(out.size(), 1);
    }

    #[test]
    fn cascade_author_overrides_user() {
        let mut ctx = CssContext::new();
        let mk = |v: i32| {
            let mut p = CssPropertyList::new(true);
            p.set(
                CssPropertyName::Color,
                CssValueType::Integer,
                CssPropertyValue::Int(v),
            );
            p
        };
        ctx.add_rule(
            Rc::new(sel_with(E_DIV)),
            Rc::new(mk(10)),
            CssPrimaryOrder::User,
        );
        ctx.add_rule(
            Rc::new(sel_with(E_DIV)),
            Rc::new(mk(20)),
            CssPrimaryOrder::Author,
        );

        let mut dt = Doctree::new();
        let d = dt.push();
        dt.node_mut(d).element = E_DIV;
        let mut out = CssPropertyList::new(true);
        ctx.apply(&mut out, &dt, d, None, None, None);
        assert_eq!(color_of(&out), Some(20));
    }

    #[test]
    fn cascade_user_important_beats_author_important() {
        let mut ctx = CssContext::new();
        let mk = |v: i32| {
            let mut p = CssPropertyList::new(true);
            p.set(
                CssPropertyName::Color,
                CssValueType::Integer,
                CssPropertyValue::Int(v),
            );
            p
        };
        ctx.add_rule(
            Rc::new(sel_with(E_DIV)),
            Rc::new(mk(10)),
            CssPrimaryOrder::AuthorImportant,
        );
        ctx.add_rule(
            Rc::new(sel_with(E_DIV)),
            Rc::new(mk(99)),
            CssPrimaryOrder::UserImportant,
        );

        let mut dt = Doctree::new();
        let d = dt.push();
        dt.node_mut(d).element = E_DIV;
        let mut out = CssPropertyList::new(true);
        ctx.apply(&mut out, &dt, d, None, None, None);
        assert_eq!(color_of(&out), Some(99));
    }

    #[test]
    fn specificity_tie_last_source_wins() {
        let mut ctx = CssContext::new();
        let mk = |v: i32| {
            let mut p = CssPropertyList::new(true);
            p.set(
                CssPropertyName::Color,
                CssValueType::Integer,
                CssPropertyValue::Int(v),
            );
            p
        };
        ctx.add_rule(
            Rc::new(sel_with(E_DIV)),
            Rc::new(mk(1)),
            CssPrimaryOrder::Author,
        );
        ctx.add_rule(
            Rc::new(sel_with(E_DIV)),
            Rc::new(mk(2)),
            CssPrimaryOrder::Author,
        );

        let mut dt = Doctree::new();
        let d = dt.push();
        dt.node_mut(d).element = E_DIV;
        let mut out = CssPropertyList::new(true);
        ctx.apply(&mut out, &dt, d, None, None, None);
        assert_eq!(color_of(&out), Some(2));
    }
}
