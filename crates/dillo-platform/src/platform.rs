//! `platform` — `SkiaPlatform`, implémentation de `dw::core::Platform`.
//!
//! Remplace `dw/fltkplatform.cc` (ADR-0002) : fontes `cosmic-text`
//! (`FontSystem` + `SwashCache` ≙ `FltkFont`/`fl_width`/`fl_height`),
//! couleurs ≙ `FltkColor` (`shade` déjà dans `dillo-dw`), tooltips
//! ≙ `FltkTooltip` (hooks plateforme), imgbuf ≙ `FltkImgbuf`
//! (délégué à `dillo-images`), clipboard = shim in-process
//! (pas de `arboard`/`copypasta` dans les deps — déviation A10
//! documentée : la sélection ne sort pas du processus).
//!
//! Métriques de fontes : `rustybuzz` (`ttf_parser`) donne
//! `ascender`/`descender`/advances en unités em → mise à l'échelle
//! `size/units_per_em` ≙ `fl_height`/`fl_descent`/`fl_text_extents`.
//! La calibration exacte vs FLTK est documentée dans A10
//! (`docs/audits/reports/A10_platform-ui.md`).

// Casts verbatim (unités em/px `f32`→`i32` ≙ `fl_*` int) —
// convention du port.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use cosmic_text::{fontdb, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};
use dillo_common::utf8;
use dillo_dw::imgbuf::{Imgbuf, ImgbufType};
use dillo_dw::platform::{IdleCallback, Platform};
use dillo_dw::style::{Color, Font, FontAttrs, FontStyle, FontVariant, Shading, Tooltip};
use dillo_dw::ui::{Resource, ResourceFactory};
use dillo_dw::view::View;

use crate::ui::SkiaResourceFactory;

/// `FltkPlatform` — plateforme concrète winit/skia.
///
/// Mutabilité intérieure : les ressources/fabriques gardent un
/// `Rc` partagé — `RefCell`/`Cell` reproduisent les accès
/// concurrents amont (`FltkPlatform*` circule partout).
pub struct SkiaPlatform {
    /// `FontSystem` cosmic — faces système + shaping.
    pub(crate) font_system: RefCell<FontSystem>,
    /// Cache swash (raster de glyphs — utilisé par `view.rs`).
    pub(crate) swash: RefCell<cosmic_text::SwashCache>,
    /// ≙ `FltkFont::fontsTable` (pool par `FontAttrs`).
    fonts: RefCell<HashMap<FontAttrs, Rc<Font>>>,
    /// Cache `fontdb::ID` résolu par `FontAttrs` (évite le query
    /// `fontdb` à chaque mesure — ≙ `FontFamily::get` amont).
    resolved: RefCell<HashMap<FontAttrs, Option<fontdb::ID>>>,
    /// ≙ `systemFonts` — familles internées `&'static` (≙
    /// `ConstString` pool amont ; leak borné par le nombre de
    /// familles distinctes, comme le pool amont).
    interned: RefCell<HashSet<&'static str>>,
    /// ≙ `colorsTable` — shades pré-calculées (base,shading)→rgb.
    shades: RefCell<HashMap<(i32, i32), i32>>,
    /// ≙ `idleQueue` + `idleFuncId`.
    idles: RefCell<VecDeque<(i32, IdleCallback)>>,
    /// Compteur d'id ≙ `idleFuncId`.
    next_idle: Cell<i32>,
    /// Layout courant ≙ `layout` (id arène).
    layout: Cell<usize>,
    /// Une view attachée ≙ `view != NULL` (amont : une seule).
    has_view: Cell<bool>,
    /// Facteur d'échelle moniteur — `dpi = 96*scale` ≙
    /// `Fl::screen_dpi`.
    scale: Cell<f64>,
    /// Presse-papier in-process (shim — cf. doc module).
    clipboard: RefCell<Vec<u8>>,
    /// ≙ `resources` — ressources vivantes (attach/detachView).
    resources: RefCell<Vec<Rc<dyn Resource>>>,
    /// `FltkResourceFactory resourceFactory` (ctor).
    factory: Rc<SkiaResourceFactory>,
}

impl Default for SkiaPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl SkiaPlatform {
    /// `FltkPlatform::FltkPlatform` — fontdb système lazy.
    #[must_use]
    pub fn new() -> Self {
        Self {
            font_system: RefCell::new(FontSystem::new()),
            swash: RefCell::new(cosmic_text::SwashCache::new()),
            fonts: RefCell::new(HashMap::new()),
            resolved: RefCell::new(HashMap::new()),
            interned: RefCell::new(HashSet::new()),
            shades: RefCell::new(HashMap::new()),
            idles: RefCell::new(VecDeque::new()),
            next_idle: Cell::new(0),
            layout: Cell::new(0),
            has_view: Cell::new(false),
            scale: Cell::new(1.0),
            clipboard: RefCell::new(Vec::new()),
            resources: RefCell::new(Vec::new()),
            factory: Rc::new(SkiaResourceFactory::new()),
        }
    }

    /// Facteur d'échelle (≙ `Fl::screen_dpi` par moniteur).
    /// Appelé par le shell fenêtré à la création et sur
    /// `ScaleFactorChanged`.
    pub fn set_scale_factor(&self, scale: f64) {
        self.scale.set(scale.max(0.25));
    }

    /// ≙ `standardFontFamily` + `systemFonts` — nom de famille
    /// interné : génériques CSS (`serif`, `sans-serif`…) →
    /// `fontdb` génériques, sinon `Family::Name` interné
    /// (`&'static` via `interned` — pool ≙ `ConstString` amont).
    pub(crate) fn family(&self, name: &[u8]) -> Family<'static> {
        let n = String::from_utf8_lossy(name);
        let n = n.trim();
        match n.to_lowercase().as_str() {
            "serif" | "times" | "times new roman" => Family::Serif,
            "sans" | "sans-serif" | "sans serif" | "helvetica" | "arial" => Family::SansSerif,
            "monospace" | "mono" | "courier" | "courier new" | "fixed" => Family::Monospace,
            "cursive" => Family::Cursive,
            "fantasy" => Family::Fantasy,
            _ => {
                let mut interned = self.interned.borrow_mut();
                if let Some(&s) = interned.get(n) {
                    return Family::Name(s);
                }
                let s: &'static str = Box::leak(n.to_string().into_boxed_str());
                interned.insert(s);
                Family::Name(s)
            }
        }
    }

    /// `Attrs` cosmic pour `FontAttrs` (≙ `Fl_Font` + `fl_font`).
    pub(crate) fn cosmic_attrs(&self, attrs: &FontAttrs) -> Attrs<'static> {
        Attrs::new()
            .family(self.family(&attrs.name))
            .weight(Weight(attrs.weight.clamp(1, 1000) as u16))
            .style(match attrs.style {
                FontStyle::Normal => cosmic_text::Style::Normal,
                FontStyle::Italic => cosmic_text::Style::Italic,
                FontStyle::Oblique => cosmic_text::Style::Oblique,
            })
    }

    /// `FltkFont::create` — pool `fontsTable` verbatim.
    fn pooled_font(&self, attrs: &FontAttrs) -> Rc<Font> {
        if let Some(f) = self.fonts.borrow().get(attrs) {
            return f.clone();
        }
        let f = Rc::new(self.make_font(attrs));
        self.fonts.borrow_mut().insert(attrs.clone(), f.clone());
        f
    }

    /// Résolution `fontdb::ID` pour `FontAttrs` (cache).
    fn resolve(&self, attrs: &FontAttrs) -> Option<fontdb::ID> {
        if let Some(&id) = self.resolved.borrow().get(attrs) {
            return id;
        }
        let fs = self.font_system.borrow();
        let family = self.family(&attrs.name);
        let id = fs.db().query(&fontdb::Query {
            families: &[family],
            weight: fontdb::Weight(attrs.weight.clamp(1, 1000) as u16),
            style: match attrs.style {
                FontStyle::Normal => fontdb::Style::Normal,
                FontStyle::Italic => fontdb::Style::Italic,
                FontStyle::Oblique => fontdb::Style::Oblique,
            },
            ..fontdb::Query::default()
        });
        self.resolved.borrow_mut().insert(attrs.clone(), id);
        id
    }

    /// `FltkFont::FltkFont` — métriques verbatim :
    /// `spaceWidth = fl_width(" ") + letterSpacing`, `xHeight` =
    /// bbox "x", `zeroWidth` = advance "0", `descent = fl_descent`,
    /// `ascent = fl_height - descent`.
    fn make_font(&self, attrs: &FontAttrs) -> Font {
        let mut f = Font {
            attrs: attrs.clone(),
            ascent: 0,
            descent: 0,
            space_width: 0,
            x_height: 0,
            zero_width: 0,
        };
        let Some(id) = self.resolve(attrs).or_else(|| {
            // famille inconnue → `standardFontFamily` (fallback
            // ≙ HELVETICA amont).
            let mut fallback = attrs.clone();
            fallback.name = b"sans-serif".to_vec();
            self.resolve(&fallback)
        }) else {
            return f;
        };
        let mut fs = self.font_system.borrow_mut();
        let Some(font) = fs.get_font(id) else {
            return f;
        };
        let face = font.rustybuzz();
        let upem = face.units_per_em();
        if upem <= 0 {
            return f;
        }
        let scale = attrs.size as f32 / upem as f32;
        // ≙ `fl_height()`/`fl_descent()` : hhea ascent/descent.
        f.ascent = (f32::from(face.ascender()) * scale) as i32;
        f.descent = (f32::from(-face.descender()) * scale) as i32;
        let adv = |ch: char| -> f32 {
            face.glyph_index(ch)
                .and_then(|g| face.glyph_hor_advance(g))
                .map_or(0.0, |a| f32::from(a) * scale)
        };
        f.space_width = (adv(' ') as i32).max(0) + attrs.letter_spacing;
        f.x_height = face
            .glyph_index('x')
            .and_then(|g| face.glyph_bounding_box(g))
            .map_or(0, |bb| (f32::from(bb.y_max) * scale) as i32);
        f.zero_width = adv('0') as i32;
        f
    }

    /// Mesure shapeée `text` avec `attrs` — ≙ `fl_font` + `fl_width`.
    pub(crate) fn measure(&self, text: &[u8], attrs: &FontAttrs, size: i32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let s = String::from_utf8_lossy(text);
        let mut fs = self.font_system.borrow_mut();
        let metrics = Metrics::new(size.max(1) as f32, size.max(1) as f32);
        let mut buf = Buffer::new(&mut fs, metrics);
        buf.set_text(&mut fs, &s, self.cosmic_attrs(attrs), Shaping::Advanced);
        buf.shape_until_scroll(&mut fs, true);
        buf.layout_runs().map(|run| run.line_w).sum()
    }

    /// Caractères espacés (non-combinants) ≙ `fl_nonspacing` —
    /// verbatim : `letterSpacing` s'ajoute par glyph non-espacé.
    fn spacing_chars_count(text: &[u8]) -> i32 {
        let mut n = 0;
        let mut i = 0;
        while i < text.len() {
            let (ucs, len) = utf8::decode(text, i, text.len());
            if !utf8::combining_char(ucs) {
                n += 1;
            }
            i += len.max(1);
        }
        n
    }

    /// ≙ `attachResource`/`detachResource` — les `Rc` gardent les
    /// ressources vivantes comme la liste amont.
    pub fn attach_resource(&self, res: Rc<dyn Resource>) {
        self.resources.borrow_mut().push(res);
    }

    /// `detachResource`.
    pub fn detach_resource(&self, res: &Rc<dyn Resource>) {
        self.resources.borrow_mut().retain(|r| !Rc::ptr_eq(r, res));
    }

    /// Contenu du presse-papier shim (`app.rs` paste).
    #[must_use]
    pub fn clipboard_text(&self) -> Vec<u8> {
        self.clipboard.borrow().clone()
    }

    /// Shade pré-calculée (≙ `FltkColor::colors[shading]`).
    #[must_use]
    pub fn shade(&self, color: &Color, shading: Shading) -> i32 {
        let key = (color.get_color(), shading as i32);
        if let Some(&v) = self.shades.borrow().get(&key) {
            return v;
        }
        let v = color.shade(shading).get_color();
        self.shades.borrow_mut().insert(key, v);
        v
    }
}

impl Platform for SkiaPlatform {
    /// `setLayout`.
    fn set_layout(&mut self, layout: usize) {
        self.layout.set(layout);
    }

    /// `attachView` — enregistrement seul (nos ressources
    /// s'auto-dessinent via `Embed::draw`, pas d'attachement de
    /// widget natif à notifier — déviation A10).
    fn attach_view(&mut self, _view: &mut dyn View) {
        self.has_view.set(true);
    }

    /// `detachView`.
    fn detach_view(&mut self, _view: &mut dyn View) {
        self.has_view.set(false);
    }

    /// `textWidth` — verbatim petites-caps + `letterSpacing` par
    /// caractère non-combinant (`fl_nonspacing` ≙ `combining_char`).
    fn text_width(&mut self, font: &Font, text: &[u8]) -> i32 {
        let attrs = &font.attrs;
        if attrs.font_variant == FontVariant::SmallCaps {
            let sc = (f64::from(attrs.size) * 0.78).round() as i32;
            let mut width = 0.0f32;
            let mut i = 0;
            while i < text.len() {
                let (ucs, len) = utf8::decode(text, i, text.len());
                let next = i + len.max(1);
                let ch = char::from_u32(ucs as u32);
                let up = ch.map(|c| c.to_uppercase().next().unwrap_or(c));
                if up == ch {
                    if !utf8::combining_char(ucs) {
                        width += attrs.letter_spacing as f32;
                    }
                    width += self.measure(&text[i..next], attrs, attrs.size);
                } else if let Some(cu) = up {
                    let mut buf = [0u8; 4];
                    width += attrs.letter_spacing as f32;
                    width += self.measure(cu.encode_utf8(&mut buf).as_bytes(), attrs, sc);
                }
                i = next;
            }
            width as i32
        } else {
            let mut w = self.measure(text, attrs, attrs.size);
            if attrs.letter_spacing != 0 {
                w += (Self::spacing_chars_count(text) * attrs.letter_spacing) as f32;
            }
            w as i32
        }
    }

    /// `textToUpper` — `fl_utf_toupper` ≙ `char::to_uppercase`.
    fn text_to_upper(&mut self, text: &[u8]) -> Vec<u8> {
        String::from_utf8_lossy(text)
            .chars()
            .flat_map(char::to_uppercase)
            .collect::<String>()
            .into_bytes()
    }

    /// `textToLower` — `fl_utf_tolower`.
    fn text_to_lower(&mut self, text: &[u8]) -> Vec<u8> {
        String::from_utf8_lossy(text)
            .chars()
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .into_bytes()
    }

    /// `nextGlyph` — `fl_utf8fwd` : fin du caractère à `idx`.
    fn next_glyph(&mut self, text: &[u8], idx: i32) -> i32 {
        let i = (idx.max(0) as usize).min(text.len());
        utf8::end_of_char(text, i) as i32
    }

    /// `prevGlyph` — `fl_utf8back` : début du caractère avant `idx`.
    fn prev_glyph(&mut self, text: &[u8], idx: i32) -> i32 {
        let mut i = (idx.max(0) as usize).min(text.len());
        if i == 0 {
            return 0;
        }
        i -= 1;
        while i > 0 && (text[i] & 0xC0) == 0x80 {
            i -= 1;
        }
        i as i32
    }

    /// `dpiX`/`dpiY` — `Fl::screen_dpi` ≙ `96 * scale_factor`.
    fn dpi_x(&mut self) -> f64 {
        96.0 * self.scale.get()
    }

    /// `dpiY`.
    fn dpi_y(&mut self) -> f64 {
        96.0 * self.scale.get()
    }

    /// `addIdle` — file FIFO + id croissant (verbatim ; l'armement
    /// du drain est côté `app.rs` ≙ `Fl::add_idle`).
    fn add_idle(&mut self, func: IdleCallback) -> i32 {
        let id = self.next_idle.get() + 1;
        self.next_idle.set(id);
        self.idles.borrow_mut().push_back((id, func));
        id
    }

    /// `removeIdle`.
    fn remove_idle(&mut self, idle_id: i32) {
        self.idles.borrow_mut().retain(|(id, _)| *id != idle_id);
    }

    /// `generalIdle` — une fonction par drain (verbatim `getFirst`).
    fn take_pending_idles(&mut self) -> Vec<IdleCallback> {
        let mut v = Vec::new();
        if let Some((_, f)) = self.idles.borrow_mut().pop_front() {
            v.push(f);
        }
        v
    }

    /// `createFont` — pool verbatim (`tryEverything` ignoré amont
    /// aussi : `FltkFont::create` n'en tient pas compte).
    fn create_font(&mut self, attrs: &FontAttrs, _try_everything: bool) -> Rc<Font> {
        self.pooled_font(attrs)
    }

    /// `fontExists` — `systemFonts->get(name)` ≙ query `fontdb`
    /// (famille seule — `Family::Name` fait match exact, pas de
    /// fallback implicite).
    fn font_exists(&mut self, name: &[u8]) -> bool {
        let fs = self.font_system.borrow();
        let family = self.family(name);
        fs.db()
            .query(&fontdb::Query {
                families: &[family],
                ..fontdb::Query::default()
            })
            .is_some()
    }

    /// `createColor` — pré-chauffe les 4 shades ≙ ctor `FltkColor`.
    fn create_color(&mut self, color: i32) -> Color {
        let base = Color::new(color);
        for s in [
            Shading::Normal,
            Shading::Inverse,
            Shading::Dark,
            Shading::Light,
        ] {
            let v = base.shade(s).get_color();
            self.shades.borrow_mut().insert((color, s as i32), v);
        }
        base
    }

    /// `createTooltip` — `FltkTooltip` ≙ `Tooltip` + hooks (fenêtre
    /// override 1 s côté `app.rs`).
    fn create_tooltip(&mut self, text: &[u8]) -> Rc<Tooltip> {
        Tooltip::create(text, None)
    }

    /// `cancelTooltip` — relayé par `app.rs` (registry tooltip
    /// côté fenêtre — verbatim `FltkTooltip::cancel`).
    fn cancel_tooltip(&mut self) {}

    /// `createImgbuf` — `dillo-images::imgbuf::Imgbuf` (verbatim
    /// `src/imgbuf.cc` — RGB, clamps, arbres scalés). Verbatim
    /// `FltkImgbuf` : buffer toujours RGB 3bpp (`buf_type` amont
    /// n'est jamais que `RGB` en pratique).
    fn create_imgbuf(
        &mut self,
        _buf_type: ImgbufType,
        width: i32,
        height: i32,
        gamma: f64,
    ) -> Rc<dyn Imgbuf> {
        dillo_images::imgbuf::Imgbuf::new(
            dillo_images::imgbuf::ImgbufType::Rgb,
            width.max(0) as u32,
            height.max(0) as u32,
            gamma,
        )
    }

    /// `copySelection` — shim in-process (cf. doc module).
    fn copy_selection(&mut self, text: &[u8], _destination: i32) {
        *self.clipboard.borrow_mut() = text.to_vec();
    }

    /// `getResourceFactory` — `SkiaResourceFactory` (≙ `fltkui.cc`).
    fn get_resource_factory(&mut self) -> Option<Rc<dyn ResourceFactory>> {
        Some(self.factory.clone())
    }
}
