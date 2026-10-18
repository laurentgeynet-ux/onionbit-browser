//! `platform` — `SkiaPlatform`, implémentation de `dw::core::Platform`.
//!
//! Remplace `dw/fltkplatform.cc` (ADR-0002) : fontes `cosmic-text`
//! (`FontSystem` + `SwashCache` ≙ `FltkFont`/`fl_width`/`fl_height`),
//! couleurs ≙ `FltkColor` (`shade` déjà dans `dillo-dw`), tooltips
//! ≙ `FltkTooltip` (hooks plateforme), imgbuf ≙ `FltkImgbuf`
//! (délégué à `dillo-images`), clipboard = OS via `arboard`
//! (≙ `Fl::copy`/`Fl::paste` — l'ex-shim in-process A10 est clos :
//! le test live exige le copier-coller inter-applications).
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

/// Remplace les séparateurs de paragraphe bidi (classe B de
/// `unicode-bidi` : LF, VT, FF, CR, FS/GS/RS, NEL, LS, PS) par des
/// espaces avant `Buffer::set_text` — le splitter de lignes de
/// `cosmic-text` ne connaît pas toute la classe B alors que
/// `BidiInfo` si : un FS/GS/RS dans une ligne y crée deux
/// paragraphes de directions potentiellement distinctes, ce que
/// `shape.rs` asserte (panic). Panique absente amont : `fl_width`/
/// `fl_draw` n'ont pas de notion de paragraphe.
#[must_use]
pub fn sanitize_shape_text(s: &str) -> std::borrow::Cow<'_, str> {
    const SEPARATORS: [char; 10] = [
        '\u{000A}', '\u{000B}', '\u{000C}', '\u{000D}', '\u{001C}', '\u{001D}', '\u{001E}',
        '\u{0085}', '\u{2028}', '\u{2029}',
    ];
    if s.contains(SEPARATORS) {
        std::borrow::Cow::Owned(s.replace(SEPARATORS, " "))
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

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
    /// ≙ `idleQueue` + `idleFuncId` — entrées taguées du
    /// `Layout::id` propriétaire (amont : plateforme par onglet,
    /// file privée ; notre plateforme est partagée — le tag rend
    /// le drain par-layout exact).
    idles: RefCell<VecDeque<(i32, usize, IdleCallback)>>,
    /// Compteur d'id ≙ `idleFuncId`.
    next_idle: Cell<i32>,
    /// Layout courant ≙ `layout` (id arène).
    layout: Cell<usize>,
    /// Une view attachée ≙ `view != NULL` (amont : une seule).
    has_view: Cell<bool>,
    /// Facteur d'échelle moniteur — `dpi = 96*scale` ≙
    /// `Fl::screen_dpi`.
    scale: Cell<f64>,
    /// Presse-papier local (repli si l'OS clipboard est
    /// indisponible — headless, environnement sans session).
    clipboard: RefCell<Vec<u8>>,
    /// Clipboard OS (`arboard` ≙ `Fl::copy`/`Fl::paste`) —
    /// `None` si indisponible (headless) → repli sur `clipboard`.
    os_clipboard: RefCell<Option<arboard::Clipboard>>,
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
            os_clipboard: RefCell::new(arboard::Clipboard::new().ok()),
            resources: RefCell::new(Vec::new()),
            factory: Rc::new(SkiaResourceFactory::new()),
        }
    }

    /// Accès partagé au `FontSystem` (chrome `dillo-browser::ui`
    /// dessine son texte avec les mêmes fontes que le rendu).
    #[must_use]
    pub fn font_system(&self) -> &RefCell<FontSystem> {
        &self.font_system
    }

    /// Cache de rasterisation des glyphes (chrome).
    #[must_use]
    pub fn swash(&self) -> &RefCell<cosmic_text::SwashCache> {
        &self.swash
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

    /// `weight >= 500 → FL_BOLD` (fltkfont.cc:107) — FLTK ne
    /// connaît que 2 graisses (variante bold de la famille) ;
    /// quantifier pour que 700 et 900 résolvent la même face,
    /// sinon 700/900 prennent des faces différentes et le rendu
    /// diverge d'amont (test `table-thead-tfoot`).
    pub(crate) fn fltk_weight(weight: i32) -> Weight {
        if weight >= 500 {
            Weight::BOLD
        } else {
            Weight::NORMAL
        }
    }

    /// `Attrs` cosmic pour `FontAttrs` (≙ `Fl_Font` + `fl_font`).
    pub(crate) fn cosmic_attrs(&self, attrs: &FontAttrs) -> Attrs<'static> {
        Attrs::new()
            .family(self.family(&attrs.name))
            .weight(Self::fltk_weight(attrs.weight))
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
        let family = self.family(&attrs.name);
        let mut id = {
            let fs = self.font_system.borrow();
            fs.db().query(&fontdb::Query {
                families: &[family],
                weight: fontdb::Weight(Self::fltk_weight(attrs.weight).0),
                style: match attrs.style {
                    FontStyle::Normal => fontdb::Style::Normal,
                    FontStyle::Italic => fontdb::Style::Italic,
                    FontStyle::Oblique => fontdb::Style::Oblique,
                },
                ..fontdb::Query::default()
            })
        };
        if id.is_none() {
            // `query` échoue pour les familles génériques ou les
            // noms absents (fontdb ne tague pas les faces système
            // par famille générique sous Windows) : probe-shape un
            // caractère et prendre la face choisie par le fallback
            // du shaping — celle qui sera réellement rasterisée
            // (≙ `fl_font` qui retombe sur une fonte connue).
            let mut fs = self.font_system.borrow_mut();
            let size = attrs.size.max(1) as f32;
            let mut buf = Buffer::new(&mut fs, Metrics::new(size, size));
            buf.set_text(&mut fs, "x", self.cosmic_attrs(attrs), Shaping::Advanced);
            buf.shape_until_scroll(&mut fs, true);
            id = buf
                .layout_runs()
                .next()
                .and_then(|r| r.glyphs.first().map(|g| g.font_id));
        }
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
        let s = sanitize_shape_text(&s);
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

    /// Contenu du presse-papier — OS d'abord (`Fl::paste` ≙ lecture
    /// du clipboard système), repli sur le buffer local si le
    /// clipboard OS est indisponible.
    #[must_use]
    pub fn clipboard_text(&self) -> Vec<u8> {
        if let Some(cb) = self.os_clipboard.borrow_mut().as_mut() {
            if let Ok(t) = cb.get_text() {
                return t.into_bytes();
            }
        }
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

    /// `nextGlyph` — verbatim `fl_utf8fwd(&text[idx + 1], text,
    /// &text[strlen])` : **début du caractère suivant** (≙
    /// `end_of_char(idx) + 1`). Retourner `end_of_char` nu bloquait
    /// `char_pos` sur le caractère courant → boucle infinie dans
    /// `sendSelectionEvent` à tout clic sur un mot TEXT.
    fn next_glyph(&mut self, text: &[u8], idx: i32) -> i32 {
        let i = (idx.max(0) as usize).min(text.len());
        (utf8::end_of_char(text, i) + 1) as i32
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
    fn add_idle(&mut self, layout: usize, func: IdleCallback) -> i32 {
        let id = self.next_idle.get() + 1;
        self.next_idle.set(id);
        self.idles.borrow_mut().push_back((id, layout, func));
        id
    }

    /// `removeIdle`.
    fn remove_idle(&mut self, idle_id: i32) {
        self.idles.borrow_mut().retain(|(id, _, _)| *id != idle_id);
    }

    /// `generalIdle` — une fonction par drain (verbatim `getFirst`)
    /// — la première *de ce layout* (≙ file privée amont).
    fn take_pending_idles(&mut self, layout: usize) -> Vec<IdleCallback> {
        let mut v = Vec::new();
        let mut q = self.idles.borrow_mut();
        if let Some(pos) = q.iter().position(|(_, l, _)| *l == layout) {
            if let Some((_, _, f)) = q.remove(pos) {
                v.push(f);
            }
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

    /// `copySelection` — clipboard OS (`Fl::copy` ≙ Win32/X11
    /// clipboard) + buffer local en repli. `destination` (sélection
    /// primaire X11 vs clipboard) est ignorée — pas d'analogue
    /// Windows, un seul canal comme amont sous Win32.
    fn copy_selection(&mut self, text: &[u8], _destination: i32) {
        *self.clipboard.borrow_mut() = text.to_vec();
        if let Some(cb) = self.os_clipboard.borrow_mut().as_mut() {
            let _ = cb.set_text(String::from_utf8_lossy(text).into_owned());
        }
    }

    /// `getResourceFactory` — `SkiaResourceFactory` (≙ `fltkui.cc`).
    fn get_resource_factory(&mut self) -> Option<Rc<dyn ResourceFactory>> {
        Some(self.factory.clone())
    }
}
