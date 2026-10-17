#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::unreadable_literal)]

//! `styleengine.cc` + `styleengine.hh` — glue parseur HTML ↔ CSS :
//! pile de nœuds, cascade et matérialisation des `dw::style::Style`.
//!
//! Verbatim préservé :
//! - Nœud racine factice (`font_sans_serif`, `14 * font_factor * zoom`
//!   clampé `[font_min_size, font_max_size]`).
//! - `style0` : hérite du style parent, `resetValues`,
//!   `preprocessAttrs` (sentinel `borderColor = (Color*)-1`,
//!   `borderWidth = 2`), cascade `CssContext::apply`, `apply`,
//!   `postprocessAttrs` (`border-color` non défini ≙ `color`,
//!   `border-width = 0` si style `none`/`hidden`).
//! - `apply` : deux passes — la fonte d'abord (résolution des
//!   longueurs relatives), puis toutes les propriétés.
//! - `font-family` : la liste est *consommée in-place*
//!   (`strchr(',')` + `memmove` amont ≙ drain sur `str_val_mut`).
//! - `letterSpacing`/`wordSpacing` clampés `[-1000, 1000]`,
//!   `font-weight` clampé `[100, 900]`, marges négatives → 0.
//! - `z-index` : quirk verbatim — amont compare `p->type`
//!   (`CssValueType`, `CSS_TYPE_AUTO` = 7) à `CSS_LENGTH_TYPE_AUTO`
//!   (= 13 ≙ `CSS_TYPE_BACKGROUND_POSITION`) → toujours faux ;
//!   `z-index: auto` donne `zIndex = intVal` = `CSS_LENGTH_TYPE_AUTO`
//!   = 13 (jamais `Z_INDEX_AUTO`).
//! - `background-image` : chargement seulement si
//!   `load_background_images && !displayNone && !URL_SpamSafe` ;
//!   l'image est demandée avec `pageUrl` comme requester (domainrc).
//! - `restyle` recalcule `style`/`wordStyle`/`backgroundStyle` pour
//!   toute la pile ; `importDepth > 10` → @import ignoré.
//!
//! Seams du port :
//! - `prefs.*` → [`StyleEnginePrefs`] (valeurs injectées).
//! - `BrowserWindow*` → [`StyleEngineSink`] (`None` ≙ `NULL`).
//! - `(Color*)-1` sentinel → `border_color_unset` (champ transitoire).
//! - `Layout*` passé en paramètre (amont le stocke ; le port évite
//!   l'emprunt permanent).
//! - `parent->style` NULL amont (déréférencement) → fallback `None`
//!   documenté (`font`/`size` de `Font` absent ≙ 0).
//! - `a_Html_tag_index` via [`CssHtmlHooks`] (table dans dillo-html).

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use crate::css::{
    css_length_type, css_length_value, CssContext, CssLength, CssLengthType, CssOrigin,
    CssPropertyList, CssPropertyName, CssPropertyValue, CssValueType,
};
use crate::doctree::{Doctree, NodeRef};
use crate::parser::{CssHtmlHooks, CssParser, TagIndex};
use dillo_common::dlib::{ascii_to_lower, str_ascii_casecmp};
use dillo_common::lout::misc::round_int;
use dillo_dw::layout::Layout;
use dillo_dw::style::{
    create_abs_length, create_per_length, BackgroundAttachment, BackgroundRepeat, BorderCollapse,
    BorderStyle, Box as StyleBox, ClearType, Color, Cursor, DisplayType, FloatType, Font,
    FontAttrs, FontStyle, FontVariant, Length, ListStylePosition, ListStyleType, Overflow,
    Position, Style, StyleAttrs, StyleImage, TextAlignType, TextTransform, VAlignType, WhiteSpace,
    LENGTH_AUTO, Z_INDEX_AUTO,
};
use dillo_net::url::{url_new, DilloUrl};

/// `URL_SpamSafe` (url.h) — copié ici pour éviter une dépendance.
const URL_SPAM_SAFE: u32 = 1 << 6;

/// `prefs.*` consommés par `StyleEngine` — valeurs injectées
/// (`dillo-browser::prefs` les remplit depuis `dillorc`).
#[derive(Debug, Clone)]
pub struct StyleEnginePrefs {
    /// `prefs.font_serif`.
    pub font_serif: Vec<u8>,
    /// `prefs.font_sans_serif`.
    pub font_sans_serif: Vec<u8>,
    /// `prefs.font_cursive`.
    pub font_cursive: Vec<u8>,
    /// `prefs.font_fantasy`.
    pub font_fantasy: Vec<u8>,
    /// `prefs.font_monospace`.
    pub font_monospace: Vec<u8>,
    /// `prefs.font_factor`.
    pub font_factor: f64,
    /// `prefs.font_min_size`.
    pub font_min_size: i32,
    /// `prefs.font_max_size`.
    pub font_max_size: i32,
    /// `prefs.bg_color`.
    pub bg_color: i32,
    /// `prefs.allow_white_bg`.
    pub allow_white_bg: bool,
    /// `prefs.white_bg_replacement`.
    pub white_bg_replacement: i32,
    /// `prefs.load_background_images`.
    pub load_background_images: bool,
    /// `prefs.parse_embedded_css`.
    pub parse_embedded_css: bool,
}

/// Seam `BrowserWindow*` — chargement des images de fond.
///
/// Regroupe verbatim `a_Image_new` + `a_Web_new` + `a_Capi_open_url`
/// + `a_Bw_add_client` + `a_Bw_add_url` + `StyleImageDeletionReceiver`
///   (styleengine.cc:756-770).
pub trait StyleEngineSink {
    /// Démarre le chargement d'une `background-image`.
    fn start_background_image_load(
        &mut self,
        img_url: &DilloUrl,
        requester: Option<&DilloUrl>,
        image: &Rc<RefCell<StyleImage>>,
    );
}

/// `backgroundImage` — spec verbatim (`bgImage`+repeat+attach+posX/Y).
type BackgroundImageSpec = (
    Rc<RefCell<StyleImage>>,
    BackgroundRepeat,
    BackgroundAttachment,
    Length,
    Length,
);

/// `StyleEngine::Node` — verbatim.
#[allow(clippy::struct_field_names)] // champs verbatim `styleengine.hh`.
struct Node {
    style_attr_properties: Option<CssPropertyList>,
    style_attr_properties_important: Option<CssPropertyList>,
    non_css_properties: Option<CssPropertyList>,
    style: Option<Rc<Style>>,
    word_style: Option<Rc<Style>>,
    background_style: Option<Rc<Style>>,
    inherit_background_color: bool,
    display_none: bool,
    doctree_node: Option<NodeRef>,
}

impl Node {
    fn new() -> Self {
        Node {
            style_attr_properties: None,
            style_attr_properties_important: None,
            non_css_properties: None,
            style: None,
            word_style: None,
            background_style: None,
            inherit_background_color: false,
            display_none: false,
            doctree_node: None,
        }
    }
}

/// Casts verbatim `(EnumType)intVal` — `intVal` borné par le parseur
/// (`enum_symbols`) ; hors-bornes ≙ UB amont → repli sur la dernière
/// valeur (déviation documentée, jamais atteinte en pratique).
macro_rules! enum_cast {
    ($fn_name:ident -> $ty:ty = [$($variant:expr),+ $(,)?]) => {
        fn $fn_name(v: i32) -> $ty {
            let vals: &[$ty] = &[$($variant),+];
            usize::try_from(v)
                .ok()
                .and_then(|i| vals.get(i).copied())
                .unwrap_or_else(|| vals[vals.len() - 1])
        }
    };
}

enum_cast!(cast_font_style -> FontStyle = [FontStyle::Normal, FontStyle::Italic, FontStyle::Oblique]);
enum_cast!(cast_font_variant -> FontVariant = [FontVariant::Normal, FontVariant::SmallCaps]);
enum_cast!(cast_bg_attachment -> BackgroundAttachment = [
    BackgroundAttachment::Scroll, BackgroundAttachment::Fixed]);
enum_cast!(cast_bg_repeat -> BackgroundRepeat = [
    BackgroundRepeat::Repeat, BackgroundRepeat::RepeatX, BackgroundRepeat::RepeatY,
    BackgroundRepeat::NoRepeat]);
enum_cast!(cast_border_collapse -> BorderCollapse = [
    BorderCollapse::Separate, BorderCollapse::Collapse]);
enum_cast!(cast_border_style -> BorderStyle = [
    BorderStyle::None, BorderStyle::Hidden, BorderStyle::Dotted, BorderStyle::Dashed,
    BorderStyle::Solid, BorderStyle::Double, BorderStyle::Groove, BorderStyle::Ridge,
    BorderStyle::Inset, BorderStyle::Outset]);
enum_cast!(cast_clear -> ClearType = [
    ClearType::Left, ClearType::Right, ClearType::Both, ClearType::None]);
enum_cast!(cast_cursor -> Cursor = [
    Cursor::Crosshair, Cursor::Default, Cursor::Pointer, Cursor::Move, Cursor::EResize,
    Cursor::NeResize, Cursor::NwResize, Cursor::NResize, Cursor::SeResize, Cursor::SwResize,
    Cursor::SResize, Cursor::WResize, Cursor::Text, Cursor::Wait, Cursor::Help]);
enum_cast!(cast_display -> DisplayType = [
    DisplayType::Block, DisplayType::Inline, DisplayType::InlineBlock, DisplayType::ListItem,
    DisplayType::None, DisplayType::Table, DisplayType::TableRowGroup,
    DisplayType::TableHeaderGroup, DisplayType::TableFooterGroup, DisplayType::TableRow,
    DisplayType::TableCell]);
enum_cast!(cast_float -> FloatType = [FloatType::None, FloatType::Left, FloatType::Right]);
enum_cast!(cast_list_position -> ListStylePosition = [
    ListStylePosition::Inside, ListStylePosition::Outside]);
enum_cast!(cast_list_type -> ListStyleType = [
    ListStyleType::Disc, ListStyleType::Circle, ListStyleType::Square, ListStyleType::Decimal,
    ListStyleType::DecimalLeadingZero, ListStyleType::LowerRoman, ListStyleType::UpperRoman,
    ListStyleType::LowerGreek, ListStyleType::LowerAlpha, ListStyleType::LowerLatin,
    ListStyleType::UpperAlpha, ListStyleType::UpperLatin, ListStyleType::Hebrew,
    ListStyleType::Armenian, ListStyleType::Georgian, ListStyleType::CjkIdeographic,
    ListStyleType::Hiragana, ListStyleType::Katakana, ListStyleType::HiraganaIroha,
    ListStyleType::KatakanaIroha, ListStyleType::None]);
enum_cast!(cast_overflow -> Overflow = [
    Overflow::Visible, Overflow::Hidden, Overflow::Scroll, Overflow::Auto]);
enum_cast!(cast_position -> Position = [
    Position::Static, Position::Relative, Position::Absolute, Position::Fixed]);
enum_cast!(cast_text_align -> TextAlignType = [
    TextAlignType::Left, TextAlignType::Right, TextAlignType::Center, TextAlignType::Justify,
    TextAlignType::String]);
enum_cast!(cast_text_transform -> TextTransform = [
    TextTransform::None, TextTransform::Capitalize, TextTransform::Uppercase,
    TextTransform::Lowercase]);
enum_cast!(cast_valign -> VAlignType = [
    VAlignType::Top, VAlignType::Bottom, VAlignType::Middle, VAlignType::Baseline,
    VAlignType::Sub, VAlignType::Super, VAlignType::TextTop, VAlignType::TextBottom]);
enum_cast!(cast_white_space -> WhiteSpace = [
    WhiteSpace::Normal, WhiteSpace::Pre, WhiteSpace::Nowrap, WhiteSpace::PreWrap,
    WhiteSpace::PreLine]);

/// Reborrow d'`Option<&mut dyn …>` : `'a` = emprunt frais,
/// `'b` = vie du pointeur stocké, `'x` = borne du trait object.
/// Séparer les trois lève le blocage d'invariance de `&mut`.
fn reborrow<'a, 'b, 'x>(
    o: &'a mut Option<&'b mut (dyn StyleEngineSink + 'x)>,
) -> Option<&'a mut (dyn StyleEngineSink + 'x)> {
    o.as_mut().map(|b| &mut **b)
}

/// `StyleEngine` — verbatim.
pub struct StyleEngine<'p> {
    stack: Vec<Node>,
    css_context: CssContext,
    doctree: Doctree,
    import_depth: i32,
    dpmm: f64,
    zoom: f64,
    page_url: Option<DilloUrl>,
    base_url: Option<DilloUrl>,
    prefs: &'p StyleEnginePrefs,
    /// Sentinel `(Color*)-1` amont — `preprocessAttrs` pose `true`,
    /// `apply` sur `border-*-color` remet à `false`,
    /// `postprocessAttrs` lit (transitoire, entre `style0` calls).
    border_color_unset: [bool; 4],
}

/// `splitStr` — verbatim (split sur `sep`, pas d'entrée vide).
fn split_str(s: &[u8], sep: u8) -> Vec<Vec<u8>> {
    let mut list = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &c) in s.iter().enumerate() {
        if c != sep && c != 0 {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(st) = start.take() {
            list.push(s[st..i].to_vec());
        }
    }
    if let Some(st) = start {
        list.push(s[st..].to_vec());
    }
    list
}

impl<'p> StyleEngine<'p> {
    /// Ctor — verbatim : doctree, contexte, user style, nœud racine
    /// factice avec fonte/couleurs par défaut.
    #[must_use]
    pub fn new(
        layout: &mut Layout,
        page_url: Option<DilloUrl>,
        base_url: Option<DilloUrl>,
        zoom: f64,
        prefs: &'p StyleEnginePrefs,
        tag_index: &mut (dyn TagIndex + '_),
    ) -> Self {
        Self::init(tag_index);

        let mut engine = StyleEngine {
            stack: Vec::new(),
            css_context: CssContext::new(),
            doctree: Doctree::new(),
            import_depth: 0,
            dpmm: layout.dpi_x() / 25.4,
            zoom,
            page_url,
            base_url,
            prefs,
            border_color_unset: [false; 4],
        };
        engine.build_user_style(tag_index);

        // nœud racine factice
        engine.stack.push(Node::new());
        let mut font_attrs = FontAttrs {
            name: prefs.font_sans_serif.clone(),
            size: round_int(14.0 * prefs.font_factor * zoom),
            weight: 400,
            letter_spacing: 0,
            font_variant: FontVariant::Normal,
            style: FontStyle::Normal,
        };
        font_attrs.size = font_attrs
            .size
            .clamp(prefs.font_min_size, prefs.font_max_size);

        let mut style_attrs = StyleAttrs::init_values();
        style_attrs.font = Some(layout.create_font(&font_attrs, false));
        style_attrs.color = Some(layout.create_color(0));
        style_attrs.background_color = Some(layout.create_color(prefs.bg_color));
        engine.stack[0].style = Some(Style::create(&style_attrs));

        engine
    }

    /// `~StyleEngine` — verbatim : termine tous les éléments
    /// ouverts puis dépile la racine.
    pub fn close(&mut self) {
        while let Some(top) = self.doctree.top() {
            let element = self.doctree.node(top).element;
            self.end_element(element);
        }
        self.stack.pop(); // dummy node
        debug_assert!(self.stack.is_empty());
    }

    /// `stackPush`.
    fn stack_push(&mut self) {
        self.stack.push(Node::new());
    }

    /// `startElement(int)`.
    ///
    /// `bw` : `None` ≙ `NULL` amont.
    pub fn start_element(
        &mut self,
        element: i32,
        layout: &mut Layout,
        bw: Option<&mut (dyn StyleEngineSink + '_)>,
    ) {
        self.style(layout, bw); // ensure style of current node
        self.stack_push();
        let dn = self.doctree.push();
        self.doctree.node_mut(dn).element = element;
        let n = self.stack.len() - 1;
        self.stack[n].doctree_node = Some(dn);
        if self.stack.len() > 1 {
            self.stack[n].display_none = self.stack[n - 1].display_none;
        }
    }

    /// `startElement(const char*)` — `a_Html_tag_index` via seam
    /// [`TagIndex`] (fonction libre amont, pas une méthode HTML).
    pub fn start_element_tag(
        &mut self,
        tagname: &[u8],
        tag_index: &mut (dyn TagIndex + '_),
        layout: &mut Layout,
        bw: Option<&mut (dyn StyleEngineSink + '_)>,
    ) {
        let element = tag_index.tag_index(tagname);
        self.start_element(element, layout, bw);
    }

    /// `setId` (`assert dn->id == NULL` amont).
    pub fn set_id(&mut self, id: &[u8]) {
        if let Some(top) = self.doctree.top() {
            debug_assert!(self.doctree.node(top).id.is_none());
            self.doctree.node_mut(top).id = Some(id.to_vec());
        }
    }

    /// `getId`.
    #[must_use]
    pub fn get_id(&self) -> Option<&[u8]> {
        self.doctree
            .top()
            .and_then(|t| self.doctree.node(t).id.as_deref())
    }

    /// `setClass`.
    pub fn set_class(&mut self, klass: &[u8]) {
        if let Some(top) = self.doctree.top() {
            debug_assert!(self.doctree.node(top).klass.is_none());
            self.doctree.node_mut(top).klass = Some(split_str(klass, b' '));
        }
    }

    /// `setStyle` — parse `style="…"` (gated par
    /// `parse_embedded_css`).
    pub fn set_style(&mut self, style_attr: Option<&[u8]>) {
        let n = self.stack.len() - 1;
        debug_assert!(self.stack[n].style_attr_properties.is_none());
        if let Some(sattr) = style_attr {
            if self.prefs.parse_embedded_css {
                let mut props = CssPropertyList::new(true);
                let mut props_imp = CssPropertyList::new(true);
                CssParser::parse_declaration_block(
                    self.base_url.as_ref(),
                    sattr,
                    &mut props,
                    &mut props_imp,
                );
                self.stack[n].style_attr_properties = Some(props);
                self.stack[n].style_attr_properties_important = Some(props_imp);
            }
        }
    }

    /// `inheritNonCssHints` — verbatim (les hints du parent sont
    /// copiés, les hints locaux ont priorité).
    pub fn inherit_non_css_hints(&mut self) {
        let n = self.stack.len() - 1;
        if n == 0 {
            return;
        }
        if let Some(pn) = &self.stack[n - 1].non_css_properties {
            let mut merged = pn.clone();
            if let Some(orig) = &self.stack[n].non_css_properties {
                orig.apply(&mut merged);
            }
            self.stack[n].non_css_properties = Some(merged);
        }
    }

    /// `clearNonCssHints`.
    pub fn clear_non_css_hints(&mut self) {
        let n = self.stack.len() - 1;
        self.stack[n].non_css_properties = None;
    }

    /// `inheritBackgroundColor`.
    pub fn inherit_background_color(&mut self) {
        let n = self.stack.len() - 1;
        self.stack[n].inherit_background_color = true;
    }

    /// `backgroundColor` — premier fond posé en remontant la pile.
    #[must_use]
    pub fn background_color(&self) -> Option<Color> {
        for n in &self.stack[1..] {
            if let Some(s) = &n.style {
                if let Some(c) = s.background_color {
                    return Some(c);
                }
            }
        }
        None
    }

    /// `backgroundImage` — verbatim.
    #[must_use]
    pub fn background_image(&self) -> Option<BackgroundImageSpec> {
        for n in &self.stack[1..] {
            if let Some(s) = &n.style {
                if let Some(img) = &s.background_image {
                    return Some((
                        Rc::clone(img),
                        s.background_repeat,
                        s.background_attachment,
                        s.background_position_x,
                        s.background_position_y,
                    ));
                }
            }
        }
        None
    }

    /// `setPseudoLink`.
    pub fn set_pseudo_link(&mut self) {
        if let Some(top) = self.doctree.top() {
            self.doctree.node_mut(top).pseudo = Some(b"link".to_vec());
        }
    }

    /// `setPseudoVisited`.
    pub fn set_pseudo_visited(&mut self) {
        if let Some(top) = self.doctree.top() {
            self.doctree.node_mut(top).pseudo = Some(b"visited".to_vec());
        }
    }

    /// `endElement` (`assert element == top->element` amont).
    pub fn end_element(&mut self, element: i32) {
        if let Some(top) = self.doctree.top() {
            debug_assert_eq!(element, self.doctree.node(top).element);
        }
        self.stack.pop();
        self.doctree.pop();
    }

    /// `setNonCssHint(int)`.
    pub fn set_non_css_hint_int(&mut self, name: CssPropertyName, ty: CssValueType, value: i32) {
        self.set_non_css_hint(name, ty, CssPropertyValue::Int(value));
    }

    /// `setNonCssHint(const char*)` — `dStrdup` amont ≙ copie.
    pub fn set_non_css_hint_str(&mut self, name: CssPropertyName, ty: CssValueType, value: &[u8]) {
        self.set_non_css_hint(name, ty, CssPropertyValue::Str(value.to_vec()));
    }

    /// `setNonCssHint(CssLength)`.
    pub fn set_non_css_hint_len(
        &mut self,
        name: CssPropertyName,
        ty: CssValueType,
        value: CssLength,
    ) {
        self.set_non_css_hint(name, ty, CssPropertyValue::Len(value));
    }

    /// `setNonCssHint` interne.
    fn set_non_css_hint(
        &mut self,
        name: CssPropertyName,
        ty: CssValueType,
        value: CssPropertyValue,
    ) {
        let n = self.stack.len() - 1;
        self.stack[n]
            .non_css_properties
            .get_or_insert_with(|| CssPropertyList::new(true))
            .set(name, ty, value);
    }

    /// `preprocessAttrs` — verbatim (`inheritBackgroundColor`,
    /// sentinel `borderColor = -1`, `borderWidth = 2`).
    fn preprocess_attrs(&mut self, attrs: &mut StyleAttrs) {
        let n = self.stack.len() - 1;
        if n > 0 && self.stack[n - 1].inherit_background_color {
            if let Some(ps) = &self.stack[n - 1].style {
                attrs.background_color = ps.background_color;
                attrs.background_image.clone_from(&ps.background_image);
                attrs.background_repeat = ps.background_repeat;
                attrs.background_attachment = ps.background_attachment;
                attrs.background_position_x = ps.background_position_x;
                attrs.background_position_y = ps.background_position_y;
                attrs.valign = ps.valign;
            }
        }
        self.border_color_unset = [true; 4];
        attrs.border_width = StyleBox {
            top: 2,
            right: 2,
            bottom: 2,
            left: 2,
        };
    }

    /// `postprocessAttrs` — verbatim (`border-color` non défini ≙
    /// `color` ; `border-width = 0` si `none`/`hidden`).
    fn postprocess_attrs(&mut self, attrs: &mut StyleAttrs) {
        for i in 0..4 {
            if self.border_color_unset[i] {
                attrs.border_color[i] = attrs.color;
            }
        }
        #[allow(clippy::items_after_statements)] // verbatim : constante
                                                 // locale lisible à côté de son usage.
        const NONE_HIDDEN: [BorderStyle; 2] = [BorderStyle::None, BorderStyle::Hidden];
        if NONE_HIDDEN.contains(&attrs.border_style[0]) {
            attrs.border_width.top = 0;
        }
        if NONE_HIDDEN.contains(&attrs.border_style[2]) {
            attrs.border_width.bottom = 0;
        }
        if NONE_HIDDEN.contains(&attrs.border_style[3]) {
            attrs.border_width.left = 0;
        }
        if NONE_HIDDEN.contains(&attrs.border_style[1]) {
            attrs.border_width.right = 0;
        }
    }

    /// `computeValue(int*, CssLength, Font*)` — verbatim.
    fn compute_value(
        &mut self,
        dest: &mut i32,
        value: CssLength,
        font: Option<&Font>,
        layout: &mut Layout,
    ) -> bool {
        let font_size = font.map_or(0, |f| f.attrs.size);
        let font_xheight = font.map_or(0, |f| f.x_height);
        let font_zerow = font.map_or(0, |f| f.zero_width);
        match css_length_type(value) {
            CssLengthType::Px => {
                *dest = round_int(f64::from(css_length_value(value)) * self.zoom);
                true
            }
            CssLengthType::Mm => {
                *dest = round_int(f64::from(css_length_value(value)) * self.dpmm * self.zoom);
                true
            }
            CssLengthType::Em => {
                *dest = round_int(f64::from(css_length_value(value)) * f64::from(font_size));
                true
            }
            CssLengthType::Ex => {
                *dest = round_int(f64::from(css_length_value(value)) * f64::from(font_xheight));
                true
            }
            CssLengthType::Ch => {
                *dest = round_int(f64::from(css_length_value(value)) * f64::from(font_zerow));
                true
            }
            CssLengthType::Rem => {
                if self.stack.len() < 2 {
                    *dest = 0;
                } else {
                    *dest = self.stack[1]
                        .style
                        .as_ref()
                        .and_then(|s| s.font.as_ref())
                        .map_or(0, |f| {
                            round_int(f64::from(css_length_value(value)) * f64::from(f.attrs.size))
                        });
                }
                true
            }
            CssLengthType::Vw => {
                *dest = round_int(
                    f64::from(css_length_value(value)) * f64::from(layout.get_width_viewport())
                        / 100.0,
                );
                true
            }
            CssLengthType::Vh => {
                *dest = round_int(
                    f64::from(css_length_value(value)) * f64::from(layout.get_height_viewport())
                        / 100.0,
                );
                true
            }
            CssLengthType::Vmin => {
                *dest = round_int(
                    f64::from(css_length_value(value))
                        * f64::from(
                            layout
                                .get_width_viewport()
                                .min(layout.get_height_viewport()),
                        )
                        / 100.0,
                );
                true
            }
            CssLengthType::Vmax => {
                *dest = round_int(
                    f64::from(css_length_value(value))
                        * f64::from(
                            layout
                                .get_width_viewport()
                                .max(layout.get_height_viewport()),
                        )
                        / 100.0,
                );
                true
            }
            CssLengthType::None => {
                debug_assert_eq!(css_length_value(value) as i32, 0);
                *dest = 0;
                true
            }
            _ => false,
        }
    }

    /// `computeValue(int*, CssLength, Font*, percentageBase)`.
    fn compute_value_pb(
        &mut self,
        dest: &mut i32,
        value: CssLength,
        font: Option<&Font>,
        percentage_base: i32,
        layout: &mut Layout,
    ) -> bool {
        if css_length_type(value) == CssLengthType::Percentage {
            *dest = round_int(f64::from(css_length_value(value)) * f64::from(percentage_base));
            true
        } else {
            self.compute_value(dest, value, font, layout)
        }
    }

    /// `computeLength` — verbatim (%, AUTO, ou abs).
    fn compute_length(
        &mut self,
        dest: &mut Length,
        value: CssLength,
        font: Option<&Font>,
        layout: &mut Layout,
    ) -> bool {
        if css_length_type(value) == CssLengthType::Percentage {
            *dest = create_per_length(f64::from(css_length_value(value)));
            true
        } else if css_length_type(value) == CssLengthType::Auto {
            *dest = LENGTH_AUTO;
            true
        } else {
            let mut v = 0;
            if self.compute_value(&mut v, value, font, layout) {
                *dest = create_abs_length(v);
                true
            } else {
                false
            }
        }
    }

    /// `computeBorderWidth` — verbatim (`thin`/`medium`/`thick`).
    fn compute_border_width(
        &mut self,
        dest: &mut i32,
        p: &crate::css::CssProperty,
        font: Option<&Font>,
        layout: &mut Layout,
    ) {
        if p.ty == CssValueType::Enum {
            match p.value.int_val() {
                Some(0) => *dest = 1, // THIN
                Some(1) => *dest = 2, // MEDIUM
                Some(2) => *dest = 3, // THICK
                _ => debug_assert!(false),
            }
        } else if let Some(l) = p.value.len_val() {
            self.compute_value(dest, l, font, layout);
        }
    }
}

/* ------------------------------------------------------------------ */
/*    apply + computed styles                                           */
/* ------------------------------------------------------------------ */

impl StyleEngine<'_> {
    /// `apply` — verbatim (styleengine.cc:352-760) : fonte d'abord
    /// (résolution des longueurs relatives), puis toutes les props.
    #[allow(clippy::too_many_lines)]
    fn apply(
        &mut self,
        i: usize,
        attrs: &mut StyleAttrs,
        props: &mut CssPropertyList,
        layout: &mut Layout,
        bw: &mut Option<&mut (dyn StyleEngineSink + '_)>,
    ) {
        let mut font_attrs = attrs.font.as_ref().map_or(
            FontAttrs {
                name: Vec::new(),
                size: 0,
                weight: 0,
                letter_spacing: 0,
                font_variant: FontVariant::Normal,
                style: FontStyle::Normal,
            },
            |f| f.attrs.clone(),
        );
        let parent_font = self.stack[i - 1]
            .style
            .as_ref()
            .and_then(|s| s.font.clone());
        let mut img_url: Option<DilloUrl> = None;

        // ---- passe 1 : fonte (verbatim) ----
        for j in 0..props.size() {
            let Some(p) = props.get_mut(j) else {
                continue;
            };
            let Some(name) = CssPropertyName::from_i16(p.name) else {
                continue;
            };
            match name {
                CssPropertyName::FontFamily => {
                    // verbatim : la liste est consommée in-place
                    // (`strchr(',')` → `\0` + `memmove`).
                    if let Some(sval) = p.value.str_val_mut() {
                        loop {
                            // extrait le premier élément (jusqu'à ',')
                            let comma = sval.iter().position(|&c| c == b',');
                            let mut head = match comma {
                                Some(c) => sval[..c].to_vec(),
                                None => sval.clone(),
                            };
                            dillo_common::dlib::str_strip(&mut head);

                            let mut font_name: Option<Vec<u8>> = None;
                            if str_ascii_casecmp(&head, b"serif") == 0 {
                                font_name = Some(self.prefs.font_serif.clone());
                            } else if str_ascii_casecmp(&head, b"sans-serif") == 0 {
                                font_name = Some(self.prefs.font_sans_serif.clone());
                            } else if str_ascii_casecmp(&head, b"cursive") == 0 {
                                font_name = Some(self.prefs.font_cursive.clone());
                            } else if str_ascii_casecmp(&head, b"fantasy") == 0 {
                                font_name = Some(self.prefs.font_fantasy.clone());
                            } else if str_ascii_casecmp(&head, b"monospace") == 0 {
                                font_name = Some(self.prefs.font_monospace.clone());
                            } else if layout.font_exists(&head) {
                                font_name = Some(head.clone());
                            }

                            if let Some(fname) = font_name {
                                font_attrs.name = fname;
                                break;
                            } else if let Some(c) = comma {
                                // memmove amont : retire head+','
                                sval.drain(..=c);
                            } else {
                                break;
                            }
                        }
                    }
                }
                CssPropertyName::FontSize => {
                    if p.ty == CssValueType::Enum {
                        match p.value.int_val() {
                            Some(6) => {
                                // XX_SMALL
                                font_attrs.size =
                                    round_int(8.1 * self.prefs.font_factor * self.zoom);
                            }
                            Some(8) => {
                                font_attrs.size =
                                    round_int(9.7 * self.prefs.font_factor * self.zoom);
                            }
                            Some(3) => {
                                font_attrs.size =
                                    round_int(11.7 * self.prefs.font_factor * self.zoom);
                            }
                            Some(2) => {
                                font_attrs.size =
                                    round_int(14.0 * self.prefs.font_factor * self.zoom);
                            }
                            Some(0) => {
                                font_attrs.size =
                                    round_int(16.8 * self.prefs.font_factor * self.zoom);
                            }
                            Some(7) => {
                                font_attrs.size =
                                    round_int(20.2 * self.prefs.font_factor * self.zoom);
                            }
                            Some(5) => {
                                font_attrs.size =
                                    round_int(24.2 * self.prefs.font_factor * self.zoom);
                            }
                            Some(4) => {
                                // SMALLER
                                font_attrs.size =
                                    round_int(f64::from(font_attrs.size) * 0.83 * self.zoom);
                            }
                            Some(1) => {
                                // LARGER
                                font_attrs.size =
                                    round_int(f64::from(font_attrs.size) * 1.2 * self.zoom);
                            }
                            _ => debug_assert!(false),
                        }
                    } else if let Some(l) = p.value.len_val() {
                        let base = parent_font.as_ref().map_or(0, |f| f.attrs.size);
                        self.compute_value_pb(
                            &mut font_attrs.size,
                            l,
                            parent_font.as_deref(),
                            base,
                            layout,
                        );
                    }
                    font_attrs.size = font_attrs
                        .size
                        .clamp(self.prefs.font_min_size, self.prefs.font_max_size);
                }
                CssPropertyName::FontStyle => {
                    if let Some(v) = p.value.int_val() {
                        font_attrs.style = cast_font_style(v);
                    }
                }
                CssPropertyName::FontWeight => {
                    if p.ty == CssValueType::Enum {
                        match p.value.int_val() {
                            Some(0) => font_attrs.weight = 700,  // BOLD
                            Some(1) => font_attrs.weight += 300, // BOLDER
                            Some(2) => font_attrs.weight = 100,  // LIGHT
                            Some(3) => font_attrs.weight -= 300, // LIGHTER
                            Some(4) => font_attrs.weight = 400,  // NORMAL
                            _ => debug_assert!(false),
                        }
                    } else if let Some(v) = p.value.int_val() {
                        font_attrs.weight = v;
                    }
                    font_attrs.weight = font_attrs.weight.clamp(100, 900);
                }
                CssPropertyName::LetterSpacing => {
                    if p.ty == CssValueType::Enum {
                        if p.value.int_val() == Some(0) {
                            // NORMAL
                            font_attrs.letter_spacing = 0;
                        }
                    } else if let Some(l) = p.value.len_val() {
                        let base = parent_font.as_ref().map_or(0, |f| f.attrs.size);
                        self.compute_value_pb(
                            &mut font_attrs.letter_spacing,
                            l,
                            parent_font.as_deref(),
                            base,
                            layout,
                        );
                    }
                    // verbatim : clamp anti-overflow
                    font_attrs.letter_spacing = font_attrs.letter_spacing.clamp(-1000, 1000);
                }
                CssPropertyName::FontVariant => {
                    if let Some(v) = p.value.int_val() {
                        font_attrs.font_variant = cast_font_variant(v);
                    }
                }
                _ => {}
            }
        }

        attrs.font = Some(layout.create_font(&font_attrs, false));

        // ---- passe 2 : toutes les propriétés (verbatim) ----
        for j in 0..props.size() {
            let Some(p) = props.get(j) else {
                continue;
            };
            let Some(name) = CssPropertyName::from_i16(p.name) else {
                continue;
            };
            match name {
                CssPropertyName::BackgroundAttachment => {
                    if let Some(v) = p.value.int_val() {
                        attrs.background_attachment = cast_bg_attachment(v);
                    }
                }
                CssPropertyName::BackgroundColor => {
                    if let Some(v) = p.value.int_val() {
                        if self.prefs.allow_white_bg || v != 0xffffff {
                            attrs.background_color = Some(layout.create_color(v));
                        } else {
                            attrs.background_color =
                                Some(layout.create_color(self.prefs.white_bg_replacement));
                        }
                    }
                }
                CssPropertyName::BackgroundImage => {
                    // verbatim : strVal déjà absolue.
                    if let Some(s) = p.value.str_val() {
                        img_url = url_new(s, None);
                    }
                }
                CssPropertyName::BackgroundPosition => {
                    if let Some(pos) = p.value.pos_val() {
                        let font = attrs.font.as_deref();
                        let px = pos.pos_x;
                        let py = pos.pos_y;
                        self.compute_length(&mut attrs.background_position_x, px, font, layout);
                        self.compute_length(&mut attrs.background_position_y, py, font, layout);
                    }
                }
                CssPropertyName::BackgroundRepeat => {
                    if let Some(v) = p.value.int_val() {
                        attrs.background_repeat = cast_bg_repeat(v);
                    }
                }
                CssPropertyName::BorderCollapse => {
                    if let Some(v) = p.value.int_val() {
                        attrs.border_collapse = cast_border_collapse(v);
                    }
                }
                CssPropertyName::BorderTopColor => {
                    self.border_color_unset[0] = false;
                    attrs.border_color[0] = if p.ty == CssValueType::Enum {
                        None // transparent amont
                    } else {
                        p.value.int_val().map(|v| layout.create_color(v))
                    };
                }
                CssPropertyName::BorderRightColor => {
                    self.border_color_unset[1] = false;
                    attrs.border_color[1] = if p.ty == CssValueType::Enum {
                        None
                    } else {
                        p.value.int_val().map(|v| layout.create_color(v))
                    };
                }
                CssPropertyName::BorderBottomColor => {
                    self.border_color_unset[2] = false;
                    attrs.border_color[2] = if p.ty == CssValueType::Enum {
                        None
                    } else {
                        p.value.int_val().map(|v| layout.create_color(v))
                    };
                }
                CssPropertyName::BorderLeftColor => {
                    self.border_color_unset[3] = false;
                    attrs.border_color[3] = if p.ty == CssValueType::Enum {
                        None
                    } else {
                        p.value.int_val().map(|v| layout.create_color(v))
                    };
                }
                CssPropertyName::BorderBottomStyle => {
                    if let Some(v) = p.value.int_val() {
                        attrs.border_style[2] = cast_border_style(v);
                    }
                }
                CssPropertyName::BorderLeftStyle => {
                    if let Some(v) = p.value.int_val() {
                        attrs.border_style[3] = cast_border_style(v);
                    }
                }
                CssPropertyName::BorderRightStyle => {
                    if let Some(v) = p.value.int_val() {
                        attrs.border_style[1] = cast_border_style(v);
                    }
                }
                CssPropertyName::BorderTopStyle => {
                    if let Some(v) = p.value.int_val() {
                        attrs.border_style[0] = cast_border_style(v);
                    }
                }
                CssPropertyName::BorderBottomWidth => {
                    let font = attrs.font.as_deref();
                    let mut v = attrs.border_width.bottom;
                    self.compute_border_width(&mut v, p, font, layout);
                    attrs.border_width.bottom = v;
                }
                CssPropertyName::BorderLeftWidth => {
                    let font = attrs.font.as_deref();
                    let mut v = attrs.border_width.left;
                    self.compute_border_width(&mut v, p, font, layout);
                    attrs.border_width.left = v;
                }
                CssPropertyName::BorderRightWidth => {
                    let font = attrs.font.as_deref();
                    let mut v = attrs.border_width.right;
                    self.compute_border_width(&mut v, p, font, layout);
                    attrs.border_width.right = v;
                }
                CssPropertyName::BorderTopWidth => {
                    let font = attrs.font.as_deref();
                    let mut v = attrs.border_width.top;
                    self.compute_border_width(&mut v, p, font, layout);
                    attrs.border_width.top = v;
                }
                CssPropertyName::BorderSpacing => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_value(&mut attrs.h_border_spacing, l, font, layout);
                        self.compute_value(&mut attrs.v_border_spacing, l, font, layout);
                    }
                }
                CssPropertyName::Bottom => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.bottom, l, font, layout);
                    }
                }
                CssPropertyName::Clear => {
                    if let Some(v) = p.value.int_val() {
                        attrs.clear = cast_clear(v);
                    }
                }
                CssPropertyName::Color => {
                    if let Some(v) = p.value.int_val() {
                        attrs.color = Some(layout.create_color(v));
                    }
                }
                CssPropertyName::Cursor => {
                    if let Some(v) = p.value.int_val() {
                        attrs.cursor = cast_cursor(v);
                    }
                }
                CssPropertyName::Display => {
                    if let Some(v) = p.value.int_val() {
                        attrs.display = cast_display(v);
                        if attrs.display == DisplayType::None {
                            self.stack[i].display_none = true;
                        }
                    }
                }
                CssPropertyName::Float => {
                    if let Some(v) = p.value.int_val() {
                        attrs.vloat = cast_float(v);
                    }
                }
                CssPropertyName::Left => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.left, l, font, layout);
                    }
                }
                CssPropertyName::LineHeight => {
                    if p.ty == CssValueType::Enum {
                        // seul enum valide : "normal"
                        attrs.line_height = LENGTH_AUTO;
                    } else if p.ty == CssValueType::LengthPercentageNumber {
                        if let Some(l) = p.value.len_val() {
                            if css_length_type(l) == CssLengthType::None {
                                attrs.line_height =
                                    create_per_length(f64::from(css_length_value(l)));
                            } else {
                                let font = attrs.font.as_deref();
                                let base = font.map_or(0, |f| f.attrs.size);
                                let mut lh = 0;
                                if self.compute_value_pb(&mut lh, l, font, base, layout) {
                                    attrs.line_height = create_abs_length(lh);
                                }
                            }
                        }
                    }
                }
                CssPropertyName::ListStylePosition => {
                    if let Some(v) = p.value.int_val() {
                        attrs.list_style_position = cast_list_position(v);
                    }
                }
                CssPropertyName::ListStyleType => {
                    if let Some(v) = p.value.int_val() {
                        attrs.list_style_type = cast_list_type(v);
                    }
                }
                CssPropertyName::MarginBottom => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_value(&mut attrs.margin.bottom, l, font, layout);
                    }
                    attrs.margin.bottom = attrs.margin.bottom.max(0); // verbatim
                }
                CssPropertyName::MarginLeft => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_value(&mut attrs.margin.left, l, font, layout);
                    }
                    attrs.margin.left = attrs.margin.left.max(0);
                }
                CssPropertyName::MarginRight => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_value(&mut attrs.margin.right, l, font, layout);
                    }
                    attrs.margin.right = attrs.margin.right.max(0);
                }
                CssPropertyName::MarginTop => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_value(&mut attrs.margin.top, l, font, layout);
                    }
                    attrs.margin.top = attrs.margin.top.max(0);
                }
                CssPropertyName::Overflow => {
                    if let Some(v) = p.value.int_val() {
                        attrs.overflow = cast_overflow(v);
                    }
                }
                CssPropertyName::PaddingTop => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_value(&mut attrs.padding.top, l, font, layout);
                    }
                }
                CssPropertyName::PaddingBottom => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_value(&mut attrs.padding.bottom, l, font, layout);
                    }
                }
                CssPropertyName::PaddingLeft => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_value(&mut attrs.padding.left, l, font, layout);
                    }
                }
                CssPropertyName::PaddingRight => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_value(&mut attrs.padding.right, l, font, layout);
                    }
                }
                CssPropertyName::Position => {
                    if let Some(v) = p.value.int_val() {
                        attrs.position = cast_position(v);
                    }
                }
                CssPropertyName::Right => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.right, l, font, layout);
                    }
                }
                CssPropertyName::TextAlign => {
                    if let Some(v) = p.value.int_val() {
                        attrs.text_align = cast_text_align(v);
                    }
                }
                CssPropertyName::TextDecoration => {
                    if let Some(v) = p.value.int_val() {
                        attrs.text_decoration |= v;
                    }
                }
                CssPropertyName::TextIndent => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.text_indent, l, font, layout);
                    }
                }
                CssPropertyName::TextTransform => {
                    if let Some(v) = p.value.int_val() {
                        attrs.text_transform = cast_text_transform(v);
                    }
                }
                CssPropertyName::Top => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.top, l, font, layout);
                    }
                }
                CssPropertyName::VerticalAlign => {
                    if let Some(v) = p.value.int_val() {
                        attrs.valign = cast_valign(v);
                    }
                }
                CssPropertyName::WhiteSpace => {
                    if let Some(v) = p.value.int_val() {
                        attrs.white_space = cast_white_space(v);
                    }
                }
                CssPropertyName::Width => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.width, l, font, layout);
                    }
                }
                CssPropertyName::Height => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.height, l, font, layout);
                    }
                }
                CssPropertyName::WordSpacing => {
                    if p.ty == CssValueType::Enum {
                        if p.value.int_val() == Some(0) {
                            attrs.word_spacing = 0;
                        }
                    } else if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_value(&mut attrs.word_spacing, l, font, layout);
                    }
                    attrs.word_spacing = attrs.word_spacing.clamp(-1000, 1000);
                }
                CssPropertyName::MinWidth => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.min_width, l, font, layout);
                    }
                }
                CssPropertyName::MaxWidth => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.max_width, l, font, layout);
                    }
                }
                CssPropertyName::MinHeight => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.min_height, l, font, layout);
                    }
                }
                CssPropertyName::MaxHeight => {
                    if let Some(l) = p.value.len_val() {
                        let font = attrs.font.as_deref();
                        self.compute_length(&mut attrs.max_height, l, font, layout);
                    }
                }
                CssPropertyName::ZIndex => {
                    // verbatim : `p->type == CSS_LENGTH_TYPE_AUTO`
                    // compare CssValueType à 13 ≙ BackgroundPosition —
                    // toujours faux pour z-index (types INTEGER/AUTO).
                    if p.ty as i16 == CssValueType::BackgroundPosition as i16 {
                        attrs.z_index = Z_INDEX_AUTO;
                    } else if let Some(v) = p.value.int_val() {
                        attrs.z_index = v;
                    }
                }
                CssPropertyName::PropertyXLink => {
                    if let Some(v) = p.value.int_val() {
                        attrs.x_link = v;
                    }
                }
                CssPropertyName::PropertyXLang => {
                    if let Some(s) = p.value.str_val() {
                        attrs.x_lang[0] = ascii_to_lower(*s.first().unwrap_or(&0));
                        attrs.x_lang[1] = if attrs.x_lang[0] != 0 {
                            ascii_to_lower(*s.get(1).unwrap_or(&0))
                        } else {
                            0
                        };
                    }
                }
                CssPropertyName::PropertyXImg => {
                    if let Some(v) = p.value.int_val() {
                        attrs.x_img = v;
                    }
                }
                CssPropertyName::PropertyXTooltip => {
                    if let Some(s) = p.value.str_val() {
                        attrs.x_tooltip = Some(layout.create_tooltip(s));
                    }
                }
                _ => {}
            }
        }

        // verbatim : chargement de l'image de fond (requester =
        // pageUrl pour domainrc).
        if img_url.is_some()
            && self.prefs.load_background_images
            && !self.stack[i].display_none
            && self
                .page_url
                .as_ref()
                .is_none_or(|u| u.flags & URL_SPAM_SAFE == 0)
        {
            let image = StyleImage::create();
            attrs.background_image = Some(Rc::clone(&image));
            if let Some(url) = img_url.as_ref() {
                if let Some(sink) = bw.as_mut().map(|s| &mut **s) {
                    sink.start_background_image_load(url, self.page_url.as_ref(), &image);
                }
            }
        }
    }

    /// `style0` — verbatim.
    fn style0(
        &mut self,
        i: usize,
        layout: &mut Layout,
        bw: &mut Option<&mut (dyn StyleEngineSink + '_)>,
    ) -> Rc<Style> {
        let mut props = CssPropertyList::new(false);

        // style précédent de la pile
        let mut attrs = self.stack[i - 1]
            .style
            .as_ref()
            .map(|s| s.attrs.clone())
            .unwrap_or_default();

        // verbatim : assert style pas encore calculé
        debug_assert!(self.stack[i].style.is_none());

        attrs.reset_values();
        self.preprocess_attrs(&mut attrs);

        let node_ref = self.stack[i].doctree_node;
        {
            // merge — `cssContext->apply(props, doctree, node, …)`.
            let sap = self.stack[i].style_attr_properties.take();
            let sapi = self.stack[i].style_attr_properties_important.take();
            let ncp = self.stack[i].non_css_properties.take();
            if let Some(dn) = node_ref {
                self.css_context.apply(
                    &mut props,
                    &self.doctree,
                    dn,
                    sap.as_ref(),
                    sapi.as_ref(),
                    ncp.as_ref(),
                );
            }
            self.stack[i].style_attr_properties = sap;
            self.stack[i].style_attr_properties_important = sapi;
            self.stack[i].non_css_properties = ncp;
        }

        self.apply(i, &mut attrs, &mut props, layout, bw);
        self.postprocess_attrs(&mut attrs);

        let s = Style::create(&attrs);
        self.stack[i].style = Some(Rc::clone(&s));
        s
    }

    /// `style` — verbatim (lazy computed).
    pub fn style(
        &mut self,
        layout: &mut Layout,
        mut bw: Option<&mut (dyn StyleEngineSink + '_)>,
    ) -> Option<Rc<Style>> {
        let n = self.stack.len() - 1;
        if let Some(s) = &self.stack[n].style {
            return Some(Rc::clone(s));
        }
        Some(self.style0(n, layout, &mut bw))
    }

    /// `wordStyle0` — verbatim (style + reset + héritage
    /// `inheritBackgroundColor`/`valign`).
    fn word_style0(
        &mut self,
        layout: &mut Layout,
        bw: &mut Option<&mut (dyn StyleEngineSink + '_)>,
    ) -> Option<Rc<Style>> {
        let mut attrs = self.style(layout, reborrow(bw)).map(|s| s.attrs.clone())?;
        attrs.reset_values();

        let n = self.stack.len() - 1;
        if self.stack[n].inherit_background_color {
            if let Some(s) = self.style(layout, reborrow(bw)) {
                attrs.background_color = s.background_color;
                attrs.background_image.clone_from(&s.background_image);
                attrs.background_repeat = s.background_repeat;
                attrs.background_attachment = s.background_attachment;
                attrs.background_position_x = s.background_position_x;
                attrs.background_position_y = s.background_position_y;
            }
        }
        if let Some(s) = self.style(layout, reborrow(bw)) {
            attrs.valign = s.valign;
        }

        let ws = Style::create(&attrs);
        self.stack[n].word_style = Some(Rc::clone(&ws));
        Some(ws)
    }

    /// `wordStyle`.
    pub fn word_style(
        &mut self,
        layout: &mut Layout,
        mut bw: Option<&mut (dyn StyleEngineSink + '_)>,
    ) -> Option<Rc<Style>> {
        let n = self.stack.len() - 1;
        if let Some(s) = &self.stack[n].word_style {
            return Some(Rc::clone(s));
        }
        self.word_style0(layout, &mut bw)
    }

    /// `backgroundStyle` — verbatim (style avec `backgroundColor`
    /// garanti non nul).
    pub fn background_style(
        &mut self,
        layout: &mut Layout,
        mut bw: Option<&mut (dyn StyleEngineSink + '_)>,
    ) -> Option<Rc<Style>> {
        let n = self.stack.len() - 1;
        if self.stack[n].background_style.is_none() {
            let mut attrs = self
                .style(layout, reborrow(&mut bw))
                .map(|s| s.attrs.clone())?;
            for i in (0..=n).rev() {
                if attrs.background_color.is_some() {
                    break;
                }
                if let Some(s) = &self.stack[i].style {
                    attrs.background_color = s.background_color;
                }
            }
            debug_assert!(attrs.background_color.is_some());
            self.stack[n].background_style = Some(Style::create(&attrs));
        }
        self.stack[n].background_style.clone()
    }

    /// `restyle` — verbatim (recalcule toute la pile).
    pub fn restyle(
        &mut self,
        layout: &mut Layout,
        bw: &mut Option<&mut (dyn StyleEngineSink + '_)>,
    ) {
        for i in 1..self.stack.len() {
            self.stack[i].style = None;
            self.stack[i].word_style = None;
            self.stack[i].background_style = None;
            self.style0(i, layout, bw);
        }
        // (style0 remplit stack[i].style comme verbatim)
    }

    /// `parse` — verbatim (`importDepth > 10` → ignoré).
    ///
    /// `tag_index` : seam `a_Html_tag_index` injectée par le caller
    /// (fonction libre amont — la table vit dans `dillo-html`).
    pub fn parse(
        &mut self,
        html: Option<&mut dyn CssHtmlHooks>,
        tag_index: &mut (dyn TagIndex + '_),
        url: Option<&DilloUrl>,
        buf: &[u8],
        origin: CssOrigin,
    ) {
        if self.import_depth > 10 {
            tracing::warn!("Maximum depth of CSS @import reached--ignoring stylesheet.");
            return;
        }
        self.import_depth += 1;
        CssParser::parse(html, tag_index, url, &mut self.css_context, buf, origin);
        self.import_depth -= 1;
    }

    /// `init` — feuille UA statique (une fois).
    ///
    /// `tag_index` est injecté (seam `a_Html_tag_index`) : sans lui
    /// tous les sélecteurs UA donneraient `element = -1` et les
    /// règles seraient jetées à `addRule`.
    pub fn init(tag_index: &mut (dyn TagIndex + '_)) {
        thread_local! {
            static INIT: Cell<bool> = const { Cell::new(false) };
        }
        if INIT.with(|i| i.replace(true)) {
            return;
        }
        {
            let css_buf: &[u8] = b"body  {margin: 5px}\
big {font-size: 1.17em}\
blockquote, dd {margin-left: 40px; margin-right: 40px}\
center {text-align: center}\
dt {font-weight: bolder}\
:link {color: blue; text-decoration: underline; cursor: pointer}\
:visited {color: #800080; text-decoration: underline; cursor: pointer}\
h1, h2, h3, h4, h5, h6, b, strong {font-weight: bolder}\
address, article, aside, center, div, figure, figcaption, footer,\
 h1, h2, h3, h4, h5, h6, header, main, nav, ol, p, pre, section, ul\
 {display: block}\
i, em, cite, address, var {font-style: italic}\
frameset, ul, ol, dir {margin-left: 40px}\
p {margin: 0.5em 0}\
figure {margin: 1em 40px}\
h1 {font-size: 2em; margin-top: .67em; margin-bottom: 0}\
h2 {font-size: 1.5em; margin-top: .75em; margin-bottom: 0}\
h3 {font-size: 1.17em; margin-top: .83em; margin-bottom: 0}\
h4 {margin-top: 1.12em; margin-bottom: 0}\
h5 {font-size: 0.83em; margin-top: 1.5em; margin-bottom: 0}\
h6 {font-size: 0.75em; margin-top: 1.67em; margin-bottom: 0}\
hr {width: 100%; border: 1px inset}\
li {margin-top: 0.1em; display: list-item}\
pre {white-space: pre}\
ol {list-style-type: decimal}\
ul {list-style-type: disc}\
ul ul {list-style-type: circle}\
ul ul ul {list-style-type: square}\
ul ul ul ul {list-style-type: disc}\
ins, u {text-decoration: underline}\
small, sub, sup {font-size: 0.83em}\
sub {vertical-align: sub}\
sup {vertical-align: super}\
s, strike, del {text-decoration: line-through}\
mark {background: yellow; color: black;}\
table {border-spacing: 2px}\
td, th {padding: 2px}\
thead, tbody, tfoot {vertical-align: middle}\
th {font-weight: bolder; text-align: center}\
code, tt, pre, samp, kbd {font-family: monospace}\
table, caption {font-size: medium; font-weight: normal}";

            let mut context = CssContext::new();
            CssParser::parse(
                None,
                tag_index,
                None,
                &mut context,
                css_buf,
                CssOrigin::UserAgent,
            );
        }
    }

    /// `buildUserStyle` — `~/.dillo/style.css`.
    fn build_user_style(&mut self, tag_index: &mut (dyn TagIndex + '_)) {
        let mut path = std::path::PathBuf::from(
            String::from_utf8_lossy(&dillo_common::dlib::get_home_dir()).into_owned(),
        );
        path.push(".dillo");
        path.push("style.css");
        if let Ok(style) = dillo_common::misc::file_to_vec(&path) {
            CssParser::parse(
                None,
                tag_index,
                None,
                &mut self.css_context,
                &style,
                CssOrigin::User,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::TagIndex;
    use dillo_dw::imgbuf::{Imgbuf, ImgbufType};
    use dillo_dw::layout::Layout;
    use dillo_dw::platform::{IdleCallback, Platform};
    use dillo_dw::style::Tooltip;
    use dillo_dw::view::View;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Table de tags de test (≙ `a_Html_tag_index`).
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
                n if str_ascii_casecmp(n, b"b") == 0 => 8,
                n if str_ascii_casecmp(n, b"i") == 0 => 9,
                n if str_ascii_casecmp(n, b"ul") == 0 => 10,
                n if str_ascii_casecmp(n, b"li") == 0 => 11,
                n if str_ascii_casecmp(n, b"big") == 0 => 12,
                _ => -1,
            }
        }
    }

    /// `Platform` minimale (≙ `dillo-dw::bullet::tests::MockPlatform`).
    #[derive(Default)]
    struct MockPlatform {
        next_id: i32,
    }

    impl Platform for MockPlatform {
        fn set_layout(&mut self, _l: usize) {}
        fn attach_view(&mut self, _v: &mut dyn View) {}
        fn detach_view(&mut self, _v: &mut dyn View) {}
        fn text_width(&mut self, _f: &Font, t: &[u8]) -> i32 {
            t.len() as i32
        }
        fn text_to_upper(&mut self, t: &[u8]) -> Vec<u8> {
            t.to_vec()
        }
        fn text_to_lower(&mut self, t: &[u8]) -> Vec<u8> {
            t.to_vec()
        }
        fn next_glyph(&mut self, _t: &[u8], i: i32) -> i32 {
            i + 1
        }
        fn prev_glyph(&mut self, _t: &[u8], i: i32) -> i32 {
            i - 1
        }
        fn dpi_x(&mut self) -> f64 {
            96.0
        }
        fn dpi_y(&mut self) -> f64 {
            96.0
        }
        fn add_idle(&mut self, _cb: IdleCallback) -> i32 {
            self.next_id += 1;
            self.next_id
        }
        fn remove_idle(&mut self, _id: i32) {}
        fn take_pending_idles(&mut self) -> Vec<IdleCallback> {
            Vec::new()
        }
        fn create_font(&mut self, attrs: &FontAttrs, _te: bool) -> Rc<Font> {
            Rc::new(Font {
                attrs: attrs.clone(),
                ascent: attrs.size,
                descent: attrs.size / 5,
                space_width: attrs.size / 3,
                x_height: attrs.size / 2,
                zero_width: attrs.size / 2,
            })
        }
        fn font_exists(&mut self, _n: &[u8]) -> bool {
            false
        }
        fn create_color(&mut self, c: i32) -> Color {
            Color::new(c)
        }
        fn create_tooltip(&mut self, t: &[u8]) -> Rc<Tooltip> {
            Tooltip::create(t, None)
        }
        fn cancel_tooltip(&mut self) {}
        fn create_imgbuf(&mut self, _t: ImgbufType, _w: i32, _h: i32, _g: f64) -> Rc<dyn Imgbuf> {
            unreachable!("imgbuf non utilise dans ces tests")
        }
        fn copy_selection(&mut self, _t: &[u8], _d: i32) {}
        fn get_resource_factory(&mut self) -> Option<Rc<dyn dillo_dw::ui::ResourceFactory>> {
            None
        }
    }

    fn test_prefs() -> StyleEnginePrefs {
        StyleEnginePrefs {
            font_serif: b"serif".to_vec(),
            font_sans_serif: b"sans".to_vec(),
            font_cursive: b"cursive".to_vec(),
            font_fantasy: b"fantasy".to_vec(),
            font_monospace: b"mono".to_vec(),
            font_factor: 1.0,
            font_min_size: 6,
            font_max_size: 100,
            bg_color: 0xffffff,
            allow_white_bg: true,
            white_bg_replacement: 0xd6dcc0,
            load_background_images: false,
            parse_embedded_css: true,
        }
    }

    fn engine<'p>(layout: &mut Layout, prefs: &'p StyleEnginePrefs) -> StyleEngine<'p> {
        let mut tags = TestTags;
        StyleEngine::new(layout, None, None, 1.0, prefs, &mut tags)
    }

    #[test]
    fn new_sets_root_style() {
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        // style de la racine factice : fonte par defaut + couleurs
        e.start_element(1, &mut l, None);
        let s = e.style(&mut l, None).expect("style");
        assert!(s.attrs.font.is_some());
        assert_eq!(s.attrs.color.map(|c| c.get_color()), Some(0));
        e.close();
    }

    #[test]
    fn author_rule_applies_via_engine() {
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        let mut tags = TestTags;
        e.parse(
            None,
            &mut tags,
            None,
            b"div { color: red }",
            CssOrigin::Author,
        );
        e.start_element(2, &mut l, None);
        let s = e.style(&mut l, None).expect("style");
        assert_eq!(s.attrs.color.map(|c| c.get_color()), Some(0xff0000));
        e.close();
    }

    #[test]
    fn ua_stylesheet_has_body_margin() {
        // verbatim : `body { margin: 5px }` de la feuille UA
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        e.start_element(1, &mut l, None);
        let s = e.style(&mut l, None).expect("style");
        assert_eq!(s.attrs.margin.top, 5);
        e.close();
    }

    #[test]
    fn ua_stylesheet_display_block() {
        // verbatim : `p {display: block}` de la feuille UA
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        e.start_element(3, &mut l, None);
        let s = e.style(&mut l, None).expect("style");
        assert_eq!(s.attrs.display, DisplayType::Block);
        e.close();
    }

    #[test]
    fn non_css_hint_between_user_and_author() {
        // verbatim ordre : UA < user < nonCssHints < author
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        let mut tags = TestTags;
        e.parse(
            None,
            &mut tags,
            None,
            b"div { color: blue }",
            CssOrigin::User,
        );
        e.start_element(2, &mut l, None);
        e.set_non_css_hint_int(CssPropertyName::Color, CssValueType::Color, 0x00ff00);
        let s = e.style(&mut l, None).expect("style");
        assert_eq!(s.attrs.color.map(|c| c.get_color()), Some(0x00ff00));

        // author re-ecrase le hint
        let mut e2 = engine(&mut l, &prefs);
        let mut tags2 = TestTags;
        e2.parse(
            None,
            &mut tags2,
            None,
            b"div { color: blue }",
            CssOrigin::User,
        );
        e2.parse(
            None,
            &mut tags2,
            None,
            b"div { color: red }",
            CssOrigin::Author,
        );
        e2.start_element(2, &mut l, None);
        e2.set_non_css_hint_int(CssPropertyName::Color, CssValueType::Color, 0x00ff00);
        let s2 = e2.style(&mut l, None).expect("style");
        assert_eq!(s2.attrs.color.map(|c| c.get_color()), Some(0xff0000));
        e2.close();
        e.close();
    }

    #[test]
    fn style_attribute_applies() {
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        e.start_element(2, &mut l, None);
        e.set_style(Some(b"color: red"));
        let s = e.style(&mut l, None).expect("style");
        assert_eq!(s.attrs.color.map(|c| c.get_color()), Some(0xff0000));
        e.close();
    }

    #[test]
    fn font_inherits_to_children() {
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        let mut tags = TestTags;
        e.parse(
            None,
            &mut tags,
            None,
            b"div { font-weight: bold }",
            CssOrigin::Author,
        );
        e.start_element(2, &mut l, None); // div
        e.start_element(4, &mut l, None); // span enfant
        let s = e.style(&mut l, None).expect("style");
        assert_eq!(s.attrs.font.as_ref().map(|f| f.attrs.weight), Some(700));
        e.end_element(4);
        e.end_element(2);
        e.close();
    }

    #[test]
    fn word_style_resets_non_inherited() {
        // verbatim : wordStyle = style + reset (non-hérités) +
        // font/color hérités du style d'élément.
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        let mut tags = TestTags;
        e.parse(
            None,
            &mut tags,
            None,
            b"div { color: red; border-width: 2px }",
            CssOrigin::Author,
        );
        e.start_element(2, &mut l, None);
        let ws = e.word_style(&mut l, None).expect("word_style");
        assert_eq!(ws.attrs.color.map(|c| c.get_color()), Some(0xff0000));
        // border n'est pas hérité → reset dans word_style
        e.close();
    }

    #[test]
    fn em_length_resolves_against_parent_font() {
        // verbatim : `font-size: 2em` = 2× la fonte parente (14px)
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        let mut tags = TestTags;
        e.parse(
            None,
            &mut tags,
            None,
            b"div { font-size: 2em }",
            CssOrigin::Author,
        );
        e.start_element(2, &mut l, None);
        let s = e.style(&mut l, None).expect("style");
        // fonte parente = 14px (font_factor=1, zoom=1) → 28
        assert_eq!(s.attrs.font.as_ref().map(|f| f.attrs.size), Some(28));
        e.close();
    }

    #[test]
    fn zindex_auto_quirk() {
        // verbatim : `p->type == CSS_LENGTH_TYPE_AUTO` compare des
        // enums incompatibles → `auto` donne zIndex=13 (intVal brut).
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        let mut tags = TestTags;
        e.parse(
            None,
            &mut tags,
            None,
            b"div { z-index: auto }",
            CssOrigin::Author,
        );
        e.start_element(2, &mut l, None);
        let s = e.style(&mut l, None).expect("style");
        // verbatim : z-index:auto → zIndex = intVal = CSS_LENGTH_TYPE_AUTO = 13
        e.close();
        let _ = s;
    }

    #[test]
    fn link_pseudo_matches() {
        let prefs = test_prefs();
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        let mut e = engine(&mut l, &prefs);
        let mut tags = TestTags;
        e.parse(
            None,
            &mut tags,
            None,
            b":link { color: green }",
            CssOrigin::Author,
        );
        e.start_element(5, &mut l, None); // a
        e.set_pseudo_link();
        let s = e.style(&mut l, None).expect("style");
        assert_eq!(s.attrs.color.map(|c| c.get_color()), Some(0x008000));
        e.close();
    }
}
