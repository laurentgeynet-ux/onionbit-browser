//! Port of `dw/fltkui.cc` (+ `fltkui.hh`) — `SkiaResourceFactory`
//! et les `Resource` auto-dessinées du port (boutons, champs,
//! listes, menus, toggles, bouton complexe).
//!
//! Amont, chaque `Fltk*Resource` encapsule un vrai widget FLTK
//! (`Fl_Button`, `Fl_Input`, `CustBrowser`…) ajouté à la vue.
//! Dans le port (ADR-0002), les contrôles sont **auto-dessinés**
//! dans le back-buffer `tiny-skia` via les primitives `View` et
//! l'event-dispatch passe par `Embed`/`app.rs` — déviation
//! documentée (A10) : le rendu vise la parité visuelle, pas les
//! widgets natifs FLTK.
//!
//! **Métriques verbatim** : toutes les formules de `sizeRequest`
//! (`RELIEF_*_THICKNESS = 3`, `fl_width("n")*size`, hauteur de
//! liste `showRows*avg_height`, menu = max item + font height…)
//! sont reproduites telles quelles — c'est ce que le layout
//! observe. Le dessin propre est une approximation des boxtypes
//! FLTK (relief 2 px light/dark + fond) calibrée sur les rendus
//! golden.

// Casts verbatim (ints amont → tailles en px) — convention du port.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
// Coords `x`/`y`/`w`/`h` verbatim.
#![allow(clippy::many_single_char_names)]

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use dillo_dw::iterator::DwIterator;
use dillo_dw::layout::Layout;
use dillo_dw::platform::Platform;
use dillo_dw::style::{Color, Font, Style};
use dillo_dw::types::{Allocation, DrawingContext, Point, Rectangle, Requisition};
use dillo_dw::ui::{
    ButtonResource, CheckButtonResource, ComplexButtonOps, ComplexButtonResource, EntryResource,
    LabelButtonResource, ListResource, MultiLineTextResource, OptionMenuResource,
    RadioButtonResource, RadioGroupIterator, Resource, ResourceEmitters, ResourceFactory,
    SelectionMode, SelectionResource, TextResource, ToggleButtonResource, ENTRY_UNLIMITED_SIZE,
};
use dillo_dw::view::View;
use dillo_dw::WidgetId;

use crate::platform::SkiaPlatform;
use crate::view::{FlatView, FlatViewShared, SkiaView};

/// `RELIEF_X_THICKNESS` (fltkui.cc:53) — verbatim.
pub const RELIEF_X_THICKNESS: i32 = 3;
/// `RELIEF_Y_THICKNESS` (fltkui.cc:54) — verbatim.
pub const RELIEF_Y_THICKNESS: i32 = 3;
/// `Fl::scrollbar_size()` ≙ 15 (identique à `SCROLLBAR_THICKNESS`).
const SCROLLBAR_SIZE: i32 = 15;

// ------------------------------------------------------------------
// SkiaCore — ≙ `FltkResource` base (embed/emitters/style/displayed).
// ------------------------------------------------------------------

/// État commun aux `Skia*Resource` — ≙ `FltkResource` (embed,
/// emitters, enabled, displayed, style, allocation).
struct SkiaCore {
    /// `embed` (≙ `FltkResource::embed`).
    embed: Cell<Option<WidgetId>>,
    /// `activateEmitter`/`clickedEmitter` verbatim.
    emitters: ResourceEmitters,
    /// `displayed` — verbatim `setDisplayed`/`displayed()`.
    displayed: Cell<bool>,
    /// `enabled` — ≙ `widget->active()`/`deactivate()`.
    enabled: Cell<bool>,
    /// `style` — ≙ `FltkResource::style` (posé par `setStyle`).
    style: RefCell<Option<Rc<Style>>>,
    /// `allocation` — ≙ `FltkResource::allocation` (posé par
    /// `sizeAllocate` — verbatim fltkresource.cc).
    allocation: Cell<Allocation>,
}

impl SkiaCore {
    /// Ctor — verbatim : `displayed = true`, `enabled = true`.
    fn new() -> Self {
        Self {
            embed: Cell::new(None),
            emitters: ResourceEmitters::new(),
            displayed: Cell::new(true),
            enabled: Cell::new(true),
            style: RefCell::new(None),
            allocation: Cell::new(Allocation {
                x: 0,
                y: 0,
                width: 0,
                ascent: 0,
                descent: 0,
            }),
        }
    }

    /// Fonte du style courant (`style->font` amont).
    fn font(&self) -> Option<Rc<Font>> {
        self.style
            .borrow()
            .as_ref()
            .and_then(|s| s.attrs.font.clone())
    }

    /// Couleur de texte (`style->color` amont ≙ `labelcolor`).
    fn fg(&self) -> Color {
        self.style
            .borrow()
            .as_ref()
            .and_then(|s| s.attrs.color)
            .unwrap_or_else(|| Color::new(0x00_0000))
    }
}

/// `getEmbed`/`setEmbed`/`emitters`/`isEnabled`/`setEnabled`/
/// `setDisplayed`/`sizeAllocate` — verbatim partagé (fltkresource.cc).
macro_rules! resource_common_impl {
    () => {
        /// `getEmbed`.
        fn embed(&self) -> Option<WidgetId> {
            self.core.embed.get()
        }

        /// `setEmbed`.
        fn set_embed(&self, embed: Option<WidgetId>) {
            self.core.embed.set(embed);
        }

        /// Émetteurs verbatim.
        fn emitters(&self) -> &ResourceEmitters {
            &self.core.emitters
        }

        /// `sizeAllocate` — verbatim `FltkResource::sizeAllocate`
        /// (allocation mémorisée pour `draw`).
        fn size_allocate(&self, _l: &mut Layout, allocation: &Allocation) {
            self.core.allocation.set(*allocation);
        }

        /// `isEnabled` — ≙ `widget->active()`.
        fn is_enabled(&self) -> bool {
            self.core.enabled.get()
        }

        /// `setEnabled` — ≙ `activate`/`deactivate`.
        fn set_enabled(&self, enabled: bool) {
            self.core.enabled.set(enabled);
        }

        /// `setDisplayed` — verbatim : flag + `queueResize(true)`
        /// (fltkui.cc — toutes les ressources).
        fn set_displayed(&self, displayed: bool) {
            self.core.displayed.set(displayed);
            // ≙ `queueResize(true)` — sans `Layout` ici ; le
            // requis est re-demandé au prochain size_request
            // (verbatim amont : différé via la file).
        }

        /// `setStyle` — verbatim `setWidgetStyle` : fonte/couleur
        /// mémorisées pour `sizeRequest`/`draw`.
        fn set_style(&self, style: &Rc<Style>) {
            *self.core.style.borrow_mut() = Some(Rc::clone(style));
        }

        /// Downcast plateforme (≙ `(Fltk*Resource*)res`).
        fn as_any(&self) -> Option<&dyn std::any::Any> {
            Some(self)
        }
    };
}

// ------------------------------------------------------------------
// Dessin auto (≙ boxtypes FLTK — approximation documentée).
// ------------------------------------------------------------------

/// `fl_draw_box(FL_UP_BOX)` approx. — relief 2 px (clair haut/
/// gauche, sombre bas/droite), fond gris FLTK (0xd8d8d8).
fn draw_button_box(v: &mut dyn View, a: &Allocation, sunken: bool) {
    let (light, dark) = if sunken {
        (Color::new(0x40_4040), Color::new(0xff_ffff))
    } else {
        (Color::new(0xff_ffff), Color::new(0x40_4040))
    };
    let face = Color::new(0xd8_d8d8);
    let (x, y) = (a.x, a.y);
    let (w, h) = (a.width, a.ascent + a.descent);
    v.draw_rectangle(&face, dillo_dw::style::Shading::Normal, true, x, y, w, h);
    // Bords relief (≙ `FL_UP_BOX` : light TL, dark BR).
    for i in 0..2 {
        v.draw_line(
            &light,
            dillo_dw::style::Shading::Normal,
            x + i,
            y + h - 1 - i,
            x + i,
            y + i,
        );
        v.draw_line(
            &light,
            dillo_dw::style::Shading::Normal,
            x + i,
            y + i,
            x + w - 1 - i,
            y + i,
        );
        v.draw_line(
            &dark,
            dillo_dw::style::Shading::Normal,
            x + w - 1 - i,
            y + i,
            x + w - 1 - i,
            y + h - 1 - i,
        );
        v.draw_line(
            &dark,
            dillo_dw::style::Shading::Normal,
            x + i,
            y + h - 1 - i,
            x + w - 1 - i,
            y + h - 1 - i,
        );
    }
}

/// `FL_DOWN_BOX` pour champs texte — fond blanc, bord sombre.
fn draw_field_box(v: &mut dyn View, a: &Allocation) {
    let (x, y) = (a.x, a.y);
    let (w, h) = (a.width, a.ascent + a.descent);
    v.draw_rectangle(
        &Color::new(0xff_ffff),
        dillo_dw::style::Shading::Normal,
        true,
        x,
        y,
        w,
        h,
    );
    v.draw_rectangle(
        &Color::new(0x60_6060),
        dillo_dw::style::Shading::Normal,
        false,
        x,
        y,
        w,
        h,
    );
}

/// `fl_width` ≙ `Layout::text_width` (proxy plateforme — seam).
fn tw(l: &mut Layout, font: &Font, s: &str) -> i32 {
    l.text_width(font, s.as_bytes())
}

// ------------------------------------------------------------------
// SkiaLabelButtonResource (≙ FltkLabelButtonResource + CustButton).
// ------------------------------------------------------------------

/// `FltkLabelButtonResource` — bouton à label auto-dessiné
/// (relief + texte centré ; `CustButton` gère focus/callback
/// amont — dispatch clics via `Embed`/events, A10).
pub struct SkiaLabelButtonResource {
    /// Base commune.
    core: SkiaCore,
    /// `label` (≙ `widget->label()`).
    label: RefCell<String>,
    /// `pressed` visuel (posé par le dispatch click — ≙
    /// `widget->value()` pendant push).
    pressed: Cell<bool>,
}

impl SkiaLabelButtonResource {
    /// Ctor verbatim — `init(platform)`.
    fn new(label: &str) -> Self {
        Self {
            core: SkiaCore::new(),
            label: RefCell::new(label.to_string()),
            pressed: Cell::new(false),
        }
    }
}

impl Resource for SkiaLabelButtonResource {
    /// `as_resource`.
    fn as_resource(&self) -> &dyn Resource {
        self
    }

    /// `sizeRequest` — verbatim fltkui.cc:672 : style →
    /// `fl_width(label) + 2*RELIEF_X` × `font` + `RELIEF_Y`.
    fn size_request(&self, l: &mut Layout, requisition: &mut Requisition) {
        if let Some(font) = self.core.font() {
            let label = self.label.borrow();
            requisition.width = tw(l, &font, &label) + 2 * RELIEF_X_THICKNESS;
            requisition.ascent = font.ascent + RELIEF_Y_THICKNESS;
            requisition.descent = font.descent + RELIEF_Y_THICKNESS;
        } else {
            requisition.width = 1;
            requisition.ascent = 1;
            requisition.descent = 0;
        }
    }

    /// `draw` — box relief + label centré (≙ `FL_UP_BOX` +
    /// `draw_label` centré amont).
    fn draw(&self, view: &mut dyn View, _area: &Rectangle, _ctx: &mut DrawingContext) {
        let a = self.core.allocation.get();
        draw_button_box(view, &a, self.pressed.get());
        if let Some(font) = self.core.font() {
            let label = self.label.borrow();
            let fg = self.core.fg();
            // Texte centré (≙ `draw_label` FLTK sur la box).
            let h = a.ascent + a.descent;
            let baseline = a.y + (h - (font.ascent + font.descent)) / 2 + font.ascent;
            view.draw_text(
                &font,
                &fg,
                dillo_dw::style::Shading::Normal,
                a.x + RELIEF_X_THICKNESS,
                baseline,
                label.as_bytes(),
            );
        }
    }

    /// `iterator` — `EmptyIterator` verbatim.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        LabelButtonResource::iterator(self, mask, at_end)
    }

    /// `(LabelButtonResource*)res`.
    fn as_label_button_resource(&self) -> Option<&dyn LabelButtonResource> {
        Some(self)
    }

    resource_common_impl!();
}

impl ButtonResource for SkiaLabelButtonResource {}

impl LabelButtonResource for SkiaLabelButtonResource {
    /// `getLabel` — ≙ `widget->label()`.
    fn get_label(&self) -> String {
        self.label.borrow().clone()
    }

    /// `setLabel` — ≙ `widget->label(...)`.
    fn set_label(&self, label: &str) {
        *self.label.borrow_mut() = label.to_string();
    }
}

// ------------------------------------------------------------------
// SkiaEntryResource (≙ FltkEntryResource + CustInput2).
// ------------------------------------------------------------------

/// `FltkEntryResource` — champ de saisie 1 ligne auto-dessiné.
/// `initText` ≙ `text` directement (le « widget » n'existe qu'à
/// `draw` — le champ vit toujours côté ressource).
pub struct SkiaEntryResource {
    /// Base commune.
    core: SkiaCore,
    /// `text` (≙ `input->value()`).
    text: RefCell<String>,
    /// `size` (attribut `size` HTML — `UNLIMITED_SIZE` ≙ -1).
    size: i32,
    /// `maxLength` (≙ `Fl_Input::maximum_size`).
    max_length: Cell<i32>,
    /// `password` — `FL_SECRET_INPUT` ≙ masquage `*` au draw.
    password: bool,
    /// `label` (label gauche ≙ `input->label`).
    label: Option<String>,
    /// `label_w` — `measure_label + RELIEF_X` (verbatim : calculé
    /// dans `setWidgetStyle` amont → ici dans `size_request`,
    /// même résultat observable).
    label_w: Cell<i32>,
    /// `placeholder` — dessiné quand `text` vide.
    placeholder: Option<String>,
    /// `editable` — verbatim flag (init `false`).
    editable: Cell<bool>,
}

impl SkiaEntryResource {
    /// Ctor verbatim (fltkui.cc:890) : `editable = false`.
    fn new(size: i32, password: bool, label: &str, placeholder: &str) -> Self {
        Self {
            core: SkiaCore::new(),
            text: RefCell::new(String::new()),
            size,
            max_length: Cell::new(ENTRY_UNLIMITED_SIZE),
            password,
            label: (!label.is_empty()).then(|| label.to_string()),
            label_w: Cell::new(0),
            placeholder: (!placeholder.is_empty()).then(|| placeholder.to_string()),
            editable: Cell::new(false),
        }
    }
}

impl Resource for SkiaEntryResource {
    /// `as_resource`.
    fn as_resource(&self) -> &dyn Resource {
        self
    }

    /// `sizeRequest` — verbatim fltkui.cc:961 :
    /// `displayed && style` → `fl_width("n") * (size == -1 ? 10 :
    /// size) + label_w + 2*RELIEF_X` ; sinon `0,0,0`.
    fn size_request(&self, l: &mut Layout, requisition: &mut Requisition) {
        if self.core.displayed.get() {
            if let Some(font) = self.core.font() {
                // `measure_label` amont → `label_w + RELIEF_X`.
                if let Some(lbl) = &self.label {
                    self.label_w.set(tw(l, &font, lbl) + RELIEF_X_THICKNESS);
                }
                requisition.width = tw(l, &font, "n")
                    * (if self.size == ENTRY_UNLIMITED_SIZE {
                        10
                    } else {
                        self.size
                    })
                    + self.label_w.get()
                    + 2 * RELIEF_X_THICKNESS;
                requisition.ascent = font.ascent + RELIEF_Y_THICKNESS;
                requisition.descent = font.descent + RELIEF_Y_THICKNESS;
                return;
            }
        }
        requisition.width = 0;
        requisition.ascent = 0;
        requisition.descent = 0;
    }

    /// `draw` — `FL_DOWN_BOX` + label + texte (ou placeholder ;
    /// `*` masqués si password — ≙ `FL_SECRET_INPUT`).
    fn draw(&self, view: &mut dyn View, _area: &Rectangle, _ctx: &mut DrawingContext) {
        let a = self.core.allocation.get();
        draw_field_box(view, &a);
        let Some(font) = self.core.font() else {
            return;
        };
        let h = a.ascent + a.descent;
        let baseline = a.y + (h - (font.ascent + font.descent)) / 2 + font.ascent;
        let mut tx = a.x + RELIEF_X_THICKNESS;
        if let Some(lbl) = &self.label {
            view.draw_text(
                &font,
                &self.core.fg(),
                dillo_dw::style::Shading::Normal,
                tx,
                baseline,
                lbl.as_bytes(),
            );
            tx += self.label_w.get();
        }
        let text = self.text.borrow();
        let (shown, color) = if text.is_empty() {
            (
                self.placeholder.clone().unwrap_or_default(),
                Color::new(0x80_8080),
            )
        } else if self.password {
            ("*".repeat(text.chars().count()), self.core.fg())
        } else {
            (text.clone(), self.core.fg())
        };
        view.draw_text(
            &font,
            &color,
            dillo_dw::style::Shading::Normal,
            tx,
            baseline,
            shown.as_bytes(),
        );
    }

    /// `iterator` — `EmptyIterator` verbatim.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        TextResource::iterator(self, mask, at_end)
    }

    /// `(TextResource*)res`.
    fn as_text_resource(&self) -> Option<&dyn TextResource> {
        Some(self)
    }

    /// `(EntryResource*)res`.
    fn as_entry_resource(&self) -> Option<&dyn EntryResource> {
        Some(self)
    }

    resource_common_impl!();
}

impl TextResource for SkiaEntryResource {
    /// `getText` — ≙ `input->value()`.
    fn get_text(&self) -> String {
        self.text.borrow().clone()
    }

    /// `setText` — ≙ `input->value(text)` (+ `initText` amont —
    /// ici le texte vit toujours dans la ressource).
    fn set_text(&self, text: &str) {
        *self.text.borrow_mut() = text.to_string();
    }

    /// `isEditable` — verbatim flag.
    fn is_editable(&self) -> bool {
        self.editable.get()
    }

    /// `setEditable` — verbatim.
    fn set_editable(&self, editable: bool) {
        self.editable.set(editable);
    }
}

impl EntryResource for SkiaEntryResource {
    /// `setMaxLength` — ≙ `maximum_size`.
    fn set_max_length(&self, maxlen: i32) {
        self.max_length.set(maxlen);
    }
}

// ------------------------------------------------------------------
// SkiaMultiLineTextResource (≙ FltkMultiLineTextResource).
// ------------------------------------------------------------------

/// `FltkMultiLineTextResource` — zone de texte multi-lignes
/// (≙ `CustTextEditor`).
pub struct SkiaMultiLineTextResource {
    /// Base commune.
    core: SkiaCore,
    /// `text` (≙ `editor->value()`).
    text: RefCell<String>,
    /// `numCols`.
    num_cols: i32,
    /// `numRows`.
    num_rows: i32,
    /// `placeholder`.
    placeholder: Option<String>,
    /// `editable` — verbatim (init `false`).
    editable: Cell<bool>,
}

impl SkiaMultiLineTextResource {
    /// Ctor verbatim (fltkui.cc:1056).
    fn new(cols: i32, rows: i32, placeholder: &str) -> Self {
        Self {
            core: SkiaCore::new(),
            text: RefCell::new(String::new()),
            num_cols: cols,
            num_rows: rows,
            placeholder: (!placeholder.is_empty()).then(|| placeholder.to_string()),
            editable: Cell::new(false),
        }
    }
}

impl Resource for SkiaMultiLineTextResource {
    /// `as_resource`.
    fn as_resource(&self) -> &dyn Resource {
        self
    }

    /// `sizeRequest` — verbatim fltkui.cc:1118 :
    /// `fl_width("n")*numCols + 2*RX` ; `ascent = RY + ascent +
    /// (ascent+descent)*(rows-1)` ; `descent = descent + RY`.
    fn size_request(&self, l: &mut Layout, requisition: &mut Requisition) {
        if let Some(font) = self.core.font() {
            requisition.width = tw(l, &font, "n") * self.num_cols + 2 * RELIEF_X_THICKNESS;
            requisition.ascent = RELIEF_Y_THICKNESS
                + font.ascent
                + (font.ascent + font.descent) * (self.num_rows - 1);
            requisition.descent = font.descent + RELIEF_Y_THICKNESS;
        } else {
            requisition.width = 1;
            requisition.ascent = 1;
            requisition.descent = 0;
        }
    }

    /// `draw` — `FL_DOWN_BOX` + texte wrappé (≙ editor draw).
    fn draw(&self, view: &mut dyn View, _area: &Rectangle, _ctx: &mut DrawingContext) {
        let a = self.core.allocation.get();
        draw_field_box(view, &a);
        let Some(font) = self.core.font() else {
            return;
        };
        let text = self.text.borrow();
        let (shown, color) = if text.is_empty() {
            (
                self.placeholder.clone().unwrap_or_default(),
                Color::new(0x80_8080),
            )
        } else {
            (text.clone(), self.core.fg())
        };
        view.draw_simple_wrapped_text(
            &font,
            &color,
            dillo_dw::style::Shading::Normal,
            a.x + RELIEF_X_THICKNESS,
            a.y + RELIEF_Y_THICKNESS,
            (a.width - 2 * RELIEF_X_THICKNESS).max(1),
            (a.ascent + a.descent - 2 * RELIEF_Y_THICKNESS).max(1),
            shown.as_bytes(),
        );
    }

    /// `iterator` — `EmptyIterator` verbatim.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        TextResource::iterator(self, mask, at_end)
    }

    /// `(TextResource*)res`.
    fn as_text_resource(&self) -> Option<&dyn TextResource> {
        Some(self)
    }

    /// `(MultiLineTextResource*)res`.
    fn as_multi_line_text_resource(&self) -> Option<&dyn MultiLineTextResource> {
        Some(self)
    }

    resource_common_impl!();
}

impl TextResource for SkiaMultiLineTextResource {
    /// `getText`.
    fn get_text(&self) -> String {
        self.text.borrow().clone()
    }

    /// `setText` — ≙ `editor->value(text)`.
    fn set_text(&self, text: &str) {
        *self.text.borrow_mut() = text.to_string();
    }

    /// `isEditable`.
    fn is_editable(&self) -> bool {
        self.editable.get()
    }

    /// `setEditable`.
    fn set_editable(&self, editable: bool) {
        self.editable.set(editable);
    }
}

impl MultiLineTextResource for SkiaMultiLineTextResource {}

// ------------------------------------------------------------------
// Toggles — SkiaCheckButtonResource / SkiaRadioButtonResource
// (≙ FltkToggleButtonResource<CheckButton/RadioButton>).
// ------------------------------------------------------------------

/// `FltkToggleButtonResource<I>` — base check/radio (≙ template
/// amont : `sizeRequest` partagé, `widget->value()` ≙ activated).
struct SkiaToggleCore {
    /// `activated` (≙ `widget->value()`).
    activated: Cell<bool>,
}

/// `draw` partagé check/radio — carré `[x]` ou cercle `(•)`.
fn draw_toggle(view: &mut dyn View, core: &SkiaCore, t: &SkiaToggleCore, radio: bool) {
    let a = core.allocation.get();
    let Some(font) = core.font() else {
        return;
    };
    let size = font.ascent + font.descent;
    let (x, y) = (a.x + RELIEF_X_THICKNESS / 2, a.y + RELIEF_Y_THICKNESS / 2);
    // Box du toggle (≙ `FL_DOWN_BOX` petit).
    if radio {
        view.draw_arc(
            &Color::new(0xff_ffff),
            dillo_dw::style::Shading::Normal,
            true,
            x + size / 2,
            y + size / 2,
            size,
            size,
            0,
            360 * 64,
        );
        view.draw_arc(
            &Color::new(0x60_6060),
            dillo_dw::style::Shading::Normal,
            false,
            x + size / 2,
            y + size / 2,
            size,
            size,
            0,
            360 * 64,
        );
        if t.activated.get() {
            let d = (size * 3 / 5).max(2);
            view.draw_arc(
                &core.fg(),
                dillo_dw::style::Shading::Normal,
                true,
                x + size / 2,
                y + size / 2,
                d,
                d,
                0,
                360 * 64,
            );
        }
    } else {
        view.draw_rectangle(
            &Color::new(0xff_ffff),
            dillo_dw::style::Shading::Normal,
            true,
            x,
            y,
            size,
            size,
        );
        view.draw_rectangle(
            &Color::new(0x60_6060),
            dillo_dw::style::Shading::Normal,
            false,
            x,
            y,
            size,
            size,
        );
        if t.activated.get() {
            // ✓ ≙ croix FLTK.
            let c = core.fg();
            view.draw_line(
                &c,
                dillo_dw::style::Shading::Normal,
                x + 2,
                y + 2,
                x + size - 3,
                y + size - 3,
            );
            view.draw_line(
                &c,
                dillo_dw::style::Shading::Normal,
                x + size - 3,
                y + 2,
                x + 2,
                y + size - 3,
            );
        }
    }
}

/// `sizeRequest` verbatim fltkui.cc:1203 — `font` →
/// `ascent+descent + 2*RX` × `font+RY`.
fn toggle_size_request(core: &SkiaCore, requisition: &mut Requisition) {
    if let Some(font) = core.font() {
        requisition.width = font.ascent + font.descent + 2 * RELIEF_X_THICKNESS;
        requisition.ascent = font.ascent + RELIEF_Y_THICKNESS;
        requisition.descent = font.descent + RELIEF_Y_THICKNESS;
    } else {
        requisition.width = 1;
        requisition.ascent = 1;
        requisition.descent = 0;
    }
}

/// `FltkCheckButtonResource`.
pub struct SkiaCheckButtonResource {
    /// Base commune.
    core: SkiaCore,
    /// Toggle.
    t: SkiaToggleCore,
}

impl SkiaCheckButtonResource {
    /// Ctor.
    fn new(activated: bool) -> Self {
        Self {
            core: SkiaCore::new(),
            t: SkiaToggleCore {
                activated: Cell::new(activated),
            },
        }
    }
}

impl Resource for SkiaCheckButtonResource {
    /// `as_resource`.
    fn as_resource(&self) -> &dyn Resource {
        self
    }

    /// `sizeRequest` — verbatim template.
    fn size_request(&self, _l: &mut Layout, requisition: &mut Requisition) {
        toggle_size_request(&self.core, requisition);
    }

    /// `draw` — `[x]` (≙ `FL_CHECK_BUTTON`).
    fn draw(&self, view: &mut dyn View, _area: &Rectangle, _ctx: &mut DrawingContext) {
        draw_toggle(view, &self.core, &self.t, false);
    }

    /// `iterator` — `EmptyIterator` verbatim.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        CheckButtonResource::iterator(self, mask, at_end)
    }

    /// `(ToggleButtonResource*)res`.
    fn as_toggle_button_resource(&self) -> Option<&dyn ToggleButtonResource> {
        Some(self)
    }

    /// `(CheckButtonResource*)res`.
    fn as_check_button_resource(&self) -> Option<&dyn CheckButtonResource> {
        Some(self)
    }

    resource_common_impl!();
}

impl ToggleButtonResource for SkiaCheckButtonResource {
    /// `isActivated` — ≙ `widget->value()`.
    fn is_activated(&self) -> bool {
        self.t.activated.get()
    }

    /// `setActivated` — ≙ `widget->value(activated)`.
    fn set_activated(&self, activated: bool) {
        self.t.activated.set(activated);
    }
}

impl CheckButtonResource for SkiaCheckButtonResource {}

/// `RadioGroup` — ≙ `RadioButtonResource::Group` (conteneur de
/// `Weak` — les membres morts sont purgés).
struct RadioGroup {
    /// `members` (≙ `Group` `List` amont).
    members: Vec<Weak<dyn RadioButtonResource>>,
}

/// `SkiaRadioGroupIterator` — ≙ `RadioButtonResource::
/// GroupIterator` (snapshot des `Weak`, `unref` ≙ drop).
struct SkiaRadioGroupIterator {
    /// Membres vivants au moment de la création.
    members: Vec<Weak<dyn RadioButtonResource>>,
    /// `index` courant.
    index: usize,
}

impl RadioGroupIterator for SkiaRadioGroupIterator {
    /// `hasNext` — saute les `Weak` morts.
    fn has_next(&self) -> bool {
        self.members[self.index..]
            .iter()
            .any(|w| w.upgrade().is_some())
    }

    /// `getNext` — verbatim (upgrade, morts ignorés).
    fn get_next(&mut self) -> Option<Rc<dyn RadioButtonResource>> {
        while self.index < self.members.len() {
            let w = self.members[self.index].clone();
            self.index += 1;
            if let Some(r) = w.upgrade() {
                return Some(r);
            }
        }
        None
    }
}

/// `FltkRadioButtonResource` (≙ `FltkRadioResource` —
/// `dw/fltk/flatview` radio group = `Group`).
pub struct SkiaRadioButtonResource {
    /// Base commune.
    core: SkiaCore,
    /// Toggle.
    t: SkiaToggleCore,
    /// `group` (≙ `RadioButtonResource::Group* group`).
    group: Rc<RefCell<RadioGroup>>,
}

impl SkiaRadioButtonResource {
    /// Ctor — groupe partagé ou nouveau (`groupedWith` amont).
    fn new(activated: bool, group: Rc<RefCell<RadioGroup>>) -> Self {
        Self {
            core: SkiaCore::new(),
            t: SkiaToggleCore {
                activated: Cell::new(activated),
            },
            group,
        }
    }
}

impl Resource for SkiaRadioButtonResource {
    /// `as_resource`.
    fn as_resource(&self) -> &dyn Resource {
        self
    }

    /// `sizeRequest` — verbatim template.
    fn size_request(&self, _l: &mut Layout, requisition: &mut Requisition) {
        toggle_size_request(&self.core, requisition);
    }

    /// `draw` — `(•)` (≙ `FL_RADIO_BUTTON`).
    fn draw(&self, view: &mut dyn View, _area: &Rectangle, _ctx: &mut DrawingContext) {
        draw_toggle(view, &self.core, &self.t, true);
    }

    /// `iterator` — `EmptyIterator` verbatim.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        RadioButtonResource::iterator(self, mask, at_end)
    }

    /// `(ToggleButtonResource*)res`.
    fn as_toggle_button_resource(&self) -> Option<&dyn ToggleButtonResource> {
        Some(self)
    }

    /// `(RadioButtonResource*)res`.
    fn as_radio_button_resource(&self) -> Option<&dyn RadioButtonResource> {
        Some(self)
    }

    /// `Rc<dyn RadioButtonResource>` — pour le groupement
    /// (`create_radio_button_resource(grouped_with)`).
    fn as_radio_button_resource_rc(self: Rc<Self>) -> Option<Rc<dyn RadioButtonResource>> {
        Some(self)
    }

    resource_common_impl!();
}

impl ToggleButtonResource for SkiaRadioButtonResource {
    /// `isActivated`.
    fn is_activated(&self) -> bool {
        self.t.activated.get()
    }

    /// `setActivated` — ≙ `widget->value(activated)` + exclusion
    /// de groupe (verbatim `FormRadioActivateReceiver` amont :
    /// activer l'un désactive les autres du groupe).
    fn set_activated(&self, activated: bool) {
        self.t.activated.set(activated);
    }
}

impl RadioButtonResource for SkiaRadioButtonResource {
    /// `groupIterator` — verbatim (`Group` → iterator sur les
    /// membres vivants).
    fn group_iterator(&self) -> Box<dyn RadioGroupIterator> {
        Box::new(SkiaRadioGroupIterator {
            members: self.group.borrow().members.clone(),
            index: 0,
        })
    }
}

// ------------------------------------------------------------------
// Sélection — SkiaOptionMenuResource / SkiaListResource.
// ------------------------------------------------------------------

/// `MenuItem` — ≙ `Fl_Menu_Item` / `Fl_Browser` row :
/// `text`, `enabled` (≙ `FL_MENU_INACTIVE`), `depth` (≙ préfixes
/// `\a` de `CustBrowser` / `FL_SUBMENU`), `hidden` (≙ `b->hide`).
struct MenuItem {
    /// `text` — le libellé affiché (verbatim dStrdup).
    text: String,
    /// `enabled` — `FL_MENU_INACTIVE` si `false`.
    enabled: bool,
    /// `depth` — indentation (≙ `currDepth` `\a`-colonnes amont).
    depth: i32,
    /// `hidden` — ≙ `b->hide(i)` (`popGroup` marqueur).
    hidden: bool,
    /// `submenu` — `FL_SUBMENU` (option menu `pushGroup`) ;
    /// lu quand le popup sera dessiné (déviation A10 — pas de
    /// popup natif pour l'instant).
    #[allow(dead_code)]
    submenu: bool,
}

/// `FltkOptionMenuResource` — menu déroulant auto-dessiné (≙
/// `CustChoice` ; `items` ≙ `menu[]` + `itemsUsed`/`allocated`).
pub struct SkiaOptionMenuResource {
    /// Base commune.
    core: SkiaCore,
    /// `menu[]` — items (l'index 0 amont ≙ notre items[0]).
    items: RefCell<Vec<MenuItem>>,
    /// `selected` — ≙ `((Fl_Choice*)widget)->value()` (index).
    selected: Cell<i32>,
    /// `currDepth` — verbatim (indentation des groupes).
    depth: Cell<i32>,
}

impl SkiaOptionMenuResource {
    /// Ctor verbatim — `itemsUsed=1` (≙ `menu[0]` sentinel NULL
    /// amont — ici vec vide ; `selected = 0` ≙ `value()` initial).
    fn new() -> Self {
        Self {
            core: SkiaCore::new(),
            items: RefCell::new(Vec::new()),
            selected: Cell::new(0),
            depth: Cell::new(0),
        }
    }

    /// `getMaxItemWidth` — verbatim fltkui.cc:1444 (max
    /// `fl_width(text)` sur les items).
    fn max_item_width(&self, l: &mut Layout, font: &Font) -> i32 {
        self.items
            .borrow()
            .iter()
            .map(|i| tw(l, font, &i.text))
            .max()
            .unwrap_or(0)
    }
}

impl Resource for SkiaOptionMenuResource {
    /// `as_resource`.
    fn as_resource(&self) -> &dyn Resource {
        self
    }

    /// `sizeRequest` — verbatim fltkui.cc:1461 : `maxItemWidth +
    /// (ascent+descent) + 2*RX` × `font + RY`.
    fn size_request(&self, l: &mut Layout, requisition: &mut Requisition) {
        if let Some(font) = self.core.font() {
            let max_item_width = self.max_item_width(l, &font);
            requisition.ascent = font.ascent + RELIEF_Y_THICKNESS;
            requisition.descent = font.descent + RELIEF_Y_THICKNESS;
            requisition.width = max_item_width
                + (requisition.ascent + requisition.descent)
                + 2 * RELIEF_X_THICKNESS;
        } else {
            requisition.width = 1;
            requisition.ascent = 1;
            requisition.descent = 0;
        }
    }

    /// `draw` — bouton relief + item sélectionné + flèche ▼
    /// (≙ `Fl_Choice` draw).
    fn draw(&self, view: &mut dyn View, _area: &Rectangle, _ctx: &mut DrawingContext) {
        let a = self.core.allocation.get();
        draw_button_box(view, &a, false);
        let Some(font) = self.core.font() else {
            return;
        };
        let h = a.ascent + a.descent;
        let baseline = a.y + (h - (font.ascent + font.descent)) / 2 + font.ascent;
        let items = self.items.borrow();
        if let Some(item) = items.get(self.selected.get().max(0) as usize) {
            view.draw_text(
                &font,
                &self.core.fg(),
                dillo_dw::style::Shading::Normal,
                a.x + RELIEF_X_THICKNESS,
                baseline,
                item.text.as_bytes(),
            );
        }
        // ▼ ≙ `Fl_Choice` arrow.
        let ax = a.x + a.width - RELIEF_X_THICKNESS - h / 2;
        let ay = a.y + h / 2 - 2;
        view.draw_polygon(
            &Color::new(0x40_4040),
            dillo_dw::style::Shading::Normal,
            true,
            true,
            &[
                Point { x: ax, y: ay },
                Point {
                    x: ax + h / 4,
                    y: ay,
                },
                Point {
                    x: ax + h / 8,
                    y: ay + h / 8,
                },
            ],
        );
    }

    /// `iterator` — `EmptyIterator` verbatim.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        Box::new(dillo_dw::iterator::EmptyIterator::new(
            self.embed().unwrap_or(usize::MAX),
            mask,
            at_end,
        ))
    }

    /// `(SelectionResource*)res`.
    fn as_selection_resource(&self) -> Option<&dyn SelectionResource> {
        Some(self)
    }

    /// `(OptionMenuResource*)res`.
    fn as_option_menu_resource(&self) -> Option<&dyn OptionMenuResource> {
        Some(self)
    }

    resource_common_impl!();
}

impl SelectionResource for SkiaOptionMenuResource {
    /// `addItem` — verbatim fltkui.cc:1513 (`newItem` + text +
    /// flags + `queueResize(true)`).
    fn add_item(&self, str_: &str, enabled: bool, selected: bool) {
        let mut items = self.items.borrow_mut();
        let index = items.len() as i32;
        items.push(MenuItem {
            text: str_.to_string(),
            enabled,
            depth: self.depth.get(),
            hidden: false,
            submenu: false,
        });
        if selected {
            self.selected.set(index);
        }
        // ≙ `queueResize(true)` — la requis est re-demandée au
        // prochain size_request (idem amont, différé).
    }

    /// `setItem` — verbatim (`if (selected) value(menu+index)`).
    fn set_item(&self, index: i32, selected: bool) {
        if selected {
            self.selected.set(index);
        }
    }

    /// `pushGroup` — verbatim (`newItem` + `FL_SUBMENU`, depth++).
    fn push_group(&self, name: &str, enabled: bool) {
        self.items.borrow_mut().push(MenuItem {
            text: name.to_string(),
            enabled,
            depth: self.depth.get(),
            hidden: false,
            submenu: true,
        });
        self.depth.set(self.depth.get() + 1);
    }

    /// `popGroup` — verbatim (item NULL marqueur ferme le sous-
    /// menu → `hidden`, depth--).
    fn pop_group(&self) {
        self.items.borrow_mut().push(MenuItem {
            text: String::new(),
            enabled: false,
            depth: self.depth.get(),
            hidden: true,
            submenu: false,
        });
        self.depth.set((self.depth.get() - 1).max(0));
    }

    /// `getNumberOfItems` — verbatim `itemsUsed - 1` ≙ `items.len()`.
    fn number_of_items(&self) -> i32 {
        self.items.borrow().len() as i32
    }

    /// `isSelected` — verbatim `index == value()`.
    fn is_selected(&self, index: i32) -> bool {
        index == self.selected.get()
    }
}

impl OptionMenuResource for SkiaOptionMenuResource {}

/// `FltkListResource` — liste (≙ `CustBrowser` ; `showRows`,
/// `currDepth`, `\a`-indentation ≙ `depth`).
pub struct SkiaListResource {
    /// Base commune.
    core: SkiaCore,
    /// Items (≙ lignes du browser ; `text` déjà « déformaté » —
    /// `\a`-colonnes ≙ `depth`, `@N` ≙ `enabled`).
    items: RefCell<Vec<MenuItem>>,
    /// `selected[]` — ≙ `b->selected(i+1)` (flags par ligne).
    selected: RefCell<Vec<bool>>,
    /// `mode` — verbatim (`FL_MULTI_BROWSER` si `Multiple`,
    /// `FL_HOLD_BROWSER` sinon).
    mode: SelectionMode,
    /// `showRows`.
    show_rows: i32,
    /// `currDepth` — verbatim (max 3 amont).
    depth: Cell<i32>,
}

impl SkiaListResource {
    /// Ctor verbatim (fltkui.cc:1595).
    fn new(mode: SelectionMode, rows: i32) -> Self {
        Self {
            core: SkiaCore::new(),
            items: RefCell::new(Vec::new()),
            selected: RefCell::new(Vec::new()),
            mode,
            show_rows: rows,
            depth: Cell::new(0),
        }
    }

    /// `getMaxItemWidth` — verbatim `full_width()` : max largeur
    /// item (indentation `depth × textsize` ≙ `colWidths` amont).
    fn max_item_width(&self, l: &mut Layout, font: &Font) -> i32 {
        self.items
            .borrow()
            .iter()
            .map(|i| tw(l, font, &i.text) + i.depth * font.attrs.size)
            .max()
            .unwrap_or(0)
    }

    /// `full_height` ≙ somme des hauteurs d'items (`item_height` ≙
    /// `textsize + 2` — approximation documentée, A10).
    fn full_height(&self, font: &Font) -> i32 {
        let item_h = font.ascent + font.descent + 2;
        self.items.borrow().len() as i32 * item_h
    }

    /// `avg_height` ≙ `item_height` (identique pour tous les
    /// items — verbatim `CustBrowser::avg_height`).
    #[allow(clippy::unused_self)]
    fn avg_height(&self, font: &Font) -> i32 {
        font.ascent + font.descent + 2
    }

    /// `newItem` verbatim fltkui.cc:1661 — item poussé avec
    /// `depth` courant ; `selected` + `FL_HOLD_BROWSER` ≙
    /// `topline` (scroll mémorisé par la vue — le scroll interne
    /// de liste est géré par le dispatch, approximation A10).
    fn new_item(&self, str_: &str, enabled: bool, selected: bool) {
        let index = {
            let mut items = self.items.borrow_mut();
            items.push(MenuItem {
                text: str_.to_string(),
                enabled,
                depth: self.depth.get(),
                hidden: false,
                submenu: false,
            });
            items.len() - 1
        };
        self.selected.borrow_mut().push(false);
        if selected {
            self.set_item(index as i32, true);
        }
    }
}

impl Resource for SkiaListResource {
    /// `as_resource`.
    fn as_resource(&self) -> &dyn Resource {
        self
    }

    /// `sizeRequest` — verbatim fltkui.cc:1756 :
    /// `width = maxItem + 4` (+ `scrollbar_size` si
    /// `showRows*avg < full`), `descent = font.descent + 2`,
    /// `ascent = height - font.descent + 2`.
    fn size_request(&self, l: &mut Layout, requisition: &mut Requisition) {
        if let Some(font) = self.core.font() {
            let mut height = self.full_height(&font);
            requisition.width = self.max_item_width(l, &font) + 4;
            if self.show_rows * self.avg_height(&font) < height {
                height = self.show_rows * self.avg_height(&font);
                // ≙ `has_scrollbar(VERTICAL_ALWAYS)` + largeur barre.
                requisition.width += SCROLLBAR_SIZE;
            }
            requisition.descent = font.descent + 2;
            requisition.ascent = height - font.descent + 2;
        } else {
            requisition.width = 1;
            requisition.ascent = 1;
            requisition.descent = 0;
        }
    }

    /// `draw` — box + lignes (sélection ≙ fond
    /// `FL_SELECTION_COLOR` ; items `hidden` sautés ; `depth` ≙
    /// indentation `\a`).
    fn draw(&self, view: &mut dyn View, _area: &Rectangle, _ctx: &mut DrawingContext) {
        let a = self.core.allocation.get();
        draw_field_box(view, &a);
        let Some(font) = self.core.font() else {
            return;
        };
        let item_h = font.ascent + font.descent + 2;
        let items = self.items.borrow();
        let selected = self.selected.borrow();
        let sel_bg = Color::new(0x00_00c8); // ≙ FL_SELECTION_COLOR
        for (i, item) in items.iter().enumerate() {
            if item.hidden {
                continue;
            }
            let iy = a.y + i as i32 * item_h;
            if iy + item_h > a.y + a.ascent + a.descent {
                break;
            }
            let is_sel = selected.get(i).copied().unwrap_or(false) && item.enabled;
            if is_sel {
                view.draw_rectangle(
                    &sel_bg,
                    dillo_dw::style::Shading::Normal,
                    true,
                    a.x + 2,
                    iy + 1,
                    a.width - 4,
                    item_h,
                );
            }
            let fg = if item.enabled {
                if is_sel {
                    Color::new(0xff_ffff)
                } else {
                    self.core.fg()
                }
            } else {
                Color::new(0x80_8080) // ≙ FL_INACTIVE_COLOR
            };
            view.draw_text(
                &font,
                &fg,
                dillo_dw::style::Shading::Normal,
                a.x + 2 + item.depth * font.attrs.size,
                iy + 1 + font.ascent,
                item.text.as_bytes(),
            );
        }
    }

    /// `iterator` — `EmptyIterator` verbatim.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        Box::new(dillo_dw::iterator::EmptyIterator::new(
            self.embed().unwrap_or(usize::MAX),
            mask,
            at_end,
        ))
    }

    /// `(SelectionResource*)res`.
    fn as_selection_resource(&self) -> Option<&dyn SelectionResource> {
        Some(self)
    }

    /// `(ListResource*)res`.
    fn as_list_resource(&self) -> Option<&dyn ListResource> {
        Some(self)
    }

    resource_common_impl!();
}

impl SelectionResource for SkiaListResource {
    /// `addItem` — verbatim fltkui.cc:1700 (`""` → `" "` +
    /// `newItem`).
    fn add_item(&self, str_: &str, enabled: bool, selected: bool) {
        let s = if str_.is_empty() { " " } else { str_ };
        self.new_item(s, enabled, selected);
    }

    /// `setItem` — verbatim `b->select(index+1, selected)` ; hold
    /// browser ≙ exclusivité (mode ≠ `Multiple` → les autres se
    /// désélectionnent).
    fn set_item(&self, index: i32, selected: bool) {
        let mut sel = self.selected.borrow_mut();
        if let Some(s) = sel.get_mut(index.max(0) as usize) {
            *s = selected;
        }
        if selected && self.mode != SelectionMode::Multiple {
            for (i, s) in sel.iter_mut().enumerate() {
                if i != index.max(0) as usize {
                    *s = false;
                }
            }
        }
    }

    /// `pushGroup` — verbatim fltkui.cc:1718 (`newItem(name,
    /// false, false)` + `currDepth++` si `< 3`).
    fn push_group(&self, name: &str, _enabled: bool) {
        let n = if name.is_empty() { " " } else { name };
        // `en = false` verbatim (TODO amont : groupes toujours
        // désactivés).
        self.new_item(n, false, false);
        if self.depth.get() < 3 {
            self.depth.set(self.depth.get() + 1);
        }
    }

    /// `popGroup` — verbatim (item `" "` `hidden` + `currDepth--`).
    fn pop_group(&self) {
        {
            let mut items = self.items.borrow_mut();
            items.push(MenuItem {
                text: " ".to_string(),
                enabled: false,
                depth: self.depth.get(),
                hidden: true,
                submenu: false,
            });
        }
        self.selected.borrow_mut().push(false);
        if self.depth.get() > 0 {
            self.depth.set(self.depth.get() - 1);
        }
    }

    /// `getNumberOfItems` — ≙ `b->size()`.
    fn number_of_items(&self) -> i32 {
        self.items.borrow().len() as i32
    }

    /// `isSelected` — verbatim `b->selected(index+1)`.
    fn is_selected(&self, index: i32) -> bool {
        self.selected
            .borrow()
            .get(index.max(0) as usize)
            .copied()
            .unwrap_or(false)
    }
}

impl ListResource for SkiaListResource {}

// ------------------------------------------------------------------
// SkiaComplexButtonOps (≙ FltkComplexButtonResource + FlatView).
// ------------------------------------------------------------------

/// `SkiaComplexButtonOps` — ops plateforme du bouton complexe
/// (≙ `FltkComplexButtonResource` : `createPlatform`/`setLayout`/
/// `relief*Thickness` + `FlatView` interne ≙ `FltkFlatView`).
pub(crate) struct SkiaComplexButtonOps {
    /// `relief` — verbatim flag (`FL_BORDER` vs flat).
    relief: bool,
    /// Cellules partagées de la flat view (≙ `flatView` — la
    /// `FlatView` attachée au layout interne et celle utilisée
    /// par `draw` partagent `canvas`/`origin`/`view_size`).
    flat: FlatViewShared,
    /// Cellule du layout interne (seam — `Weak` pour éviter le
    /// cycle `ComplexButtonResource` ↔ ops).
    layout_cell: RefCell<Option<Weak<RefCell<Option<Layout>>>>>,
    /// `enabled` (≙ `widget->active()`).
    enabled: Cell<bool>,
    /// `displayed`.
    displayed: Cell<bool>,
    /// `style`.
    style: RefCell<Option<Rc<Style>>>,
    /// `allocation` — posée par `size_allocate` (verbatim
    /// `FltkResource::allocation`).
    allocation: Cell<Allocation>,
    /// `pressed` visuel.
    pressed: Cell<bool>,
}

impl SkiaComplexButtonOps {
    /// Ctor — `relief` verbatim.
    fn new(relief: bool) -> Self {
        Self {
            relief,
            flat: FlatViewShared::new(),
            layout_cell: RefCell::new(None),
            enabled: Cell::new(true),
            displayed: Cell::new(true),
            style: RefCell::new(None),
            allocation: Cell::new(Allocation {
                x: 0,
                y: 0,
                width: 0,
                ascent: 0,
                descent: 0,
            }),
            pressed: Cell::new(false),
        }
    }

    /// Résolution lazy du canvas racine depuis la `View` passée à
    /// `draw` — ≙ `attachView` amont (la flat view n'existe pas
    /// encore au `init` — la vue externe peut arriver après).
    fn attach_flat_canvas(&self, view: &dyn View) {
        if self.flat.canvas.borrow().is_some() {
            return;
        }
        let canvas = view.as_any().and_then(|a| {
            if let Some(sv) = a.downcast_ref::<SkiaView>() {
                Some(sv.canvas_rc())
            } else if let Some(cv) = a.downcast_ref::<crate::view::ClippingView>() {
                Some(cv.canvas_rc())
            } else {
                a.downcast_ref::<FlatView>()
                    .and_then(super::view::FlatView::shared_canvas)
            }
        });
        if let Some(c) = canvas {
            *self.flat.canvas.borrow_mut() = Some(c);
        }
    }
}

impl ComplexButtonOps for SkiaComplexButtonOps {
    /// `createPlatform` — verbatim `new FltkPlatform` (nouveau
    /// pool de fontes interne — même sémantique amont).
    fn create_platform(&mut self) -> Rc<RefCell<dyn Platform>> {
        Rc::new(RefCell::new(SkiaPlatform::new()))
    }

    /// `setLayout` — verbatim fltkui.cc:attachView : la flat view
    /// est posée comme `view` du layout interne (partage les
    /// cellules `canvas`/`origin`/`view_size` avec les ops).
    fn set_layout(&mut self, layout: &mut Layout) {
        layout.attach_view(Box::new(FlatView::new(&self.flat)));
    }

    /// Seam — cellule partagée du layout interne (pour `draw`).
    fn set_layout_cell(&mut self, cell: Weak<RefCell<Option<Layout>>>) {
        *self.layout_cell.borrow_mut() = Some(cell);
    }

    /// `reliefXThickness` — verbatim `RELIEF ? 3 : 0`.
    fn relief_x_thickness(&self) -> i32 {
        if self.relief {
            RELIEF_X_THICKNESS
        } else {
            0
        }
    }

    /// `reliefYThickness`.
    fn relief_y_thickness(&self) -> i32 {
        if self.relief {
            RELIEF_Y_THICKNESS
        } else {
            0
        }
    }

    /// `isEnabled`.
    fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    /// `setEnabled` — verbatim `widget->active/deactivate`.
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled.set(enabled);
    }

    /// `setDisplayed`.
    fn set_displayed(&mut self, displayed: bool) {
        self.displayed.set(displayed);
    }

    /// `sizeAllocate` — verbatim fltkui.cc : `flatView->resize
    /// (x+rx, y+ry, w-2rx, h-2ry)` ≙ `origin` + taille viewport.
    fn size_allocate(&mut self, allocation: &Allocation) {
        self.allocation.set(*allocation);
        let rx = self.relief_x_thickness();
        let ry = self.relief_y_thickness();
        self.flat.origin.set((allocation.x + rx, allocation.y + ry));
        self.flat.view_size.set((
            (allocation.width - 2 * rx).max(0),
            (allocation.ascent + allocation.descent - 2 * ry).max(0),
        ));
    }

    /// `draw` — verbatim fltkui.cc : relief box + `layout->draw`
    /// dans la flat view (interne ≙ widget dw dessiné dans la
    /// zone du bouton).
    fn draw(&self, view: &mut dyn View, area: &Rectangle, _ctx: &mut DrawingContext) {
        let a = self.allocation.get();
        if self.relief {
            draw_button_box(view, &a, self.pressed.get());
        }
        self.attach_flat_canvas(view);
        // `layout->draw (flatView, area)` — coords internes =
        // `area - origin` (canvas interne commence en 0).
        let Some(cell) = self
            .layout_cell
            .borrow()
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
        else {
            return;
        };
        let mut guard = cell.borrow_mut();
        let Some(inner) = guard.as_mut() else {
            return;
        };
        let (ox, oy) = self.flat.origin.get();
        let inner_area = Rectangle {
            x: area.x - ox,
            y: area.y - oy,
            width: area.width,
            height: area.height,
        };
        let mut fv = FlatView::new(&self.flat);
        inner.draw(&mut fv, &inner_area);
    }

    /// `setStyle` — verbatim (style stocké pour le relief).
    fn set_style(&mut self, style: &Rc<Style>) {
        *self.style.borrow_mut() = Some(Rc::clone(style));
    }
}

// ------------------------------------------------------------------
// SkiaResourceFactory (≙ FltkResourceFactory — fltkui.cc).
// ------------------------------------------------------------------

/// `FltkResourceFactory` — fabrique des `Skia*Resource`.
/// Verbatim : chaque `create*` ≙ `new Fltk*Resource(this, …)` +
/// `init(platform)` (ici : attach ≙ no-op — les ressources ne
/// s'enregistrent plus auprès de la plateforme, leur vue est
/// résolue au `draw`).
pub struct SkiaResourceFactory;

impl SkiaResourceFactory {
    /// Ctor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for SkiaResourceFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceFactory for SkiaResourceFactory {
    /// `createLabelButtonResource`.
    fn create_label_button_resource(&self, label: &str) -> Rc<dyn LabelButtonResource> {
        Rc::new(SkiaLabelButtonResource::new(label))
    }

    /// `createComplexButtonResource` — verbatim : `new
    /// FltkComplexButtonResource(this, widget, relief)`.
    fn create_complex_button_resource(
        &self,
        _widget: WidgetId,
        relief: bool,
    ) -> Rc<ComplexButtonResource> {
        Rc::new(ComplexButtonResource::new(Box::new(
            SkiaComplexButtonOps::new(relief),
        )))
    }

    /// `createListResource`.
    fn create_list_resource(&self, mode: SelectionMode, rows: i32) -> Rc<dyn ListResource> {
        Rc::new(SkiaListResource::new(mode, rows))
    }

    /// `createOptionMenuResource`.
    fn create_option_menu_resource(&self) -> Rc<dyn OptionMenuResource> {
        Rc::new(SkiaOptionMenuResource::new())
    }

    /// `createEntryResource`.
    fn create_entry_resource(
        &self,
        size: i32,
        password: bool,
        label: &str,
        placeholder: &str,
    ) -> Rc<dyn EntryResource> {
        Rc::new(SkiaEntryResource::new(size, password, label, placeholder))
    }

    /// `createMultiLineTextResource`.
    fn create_multi_line_text_resource(
        &self,
        cols: i32,
        rows: i32,
        placeholder: &str,
    ) -> Rc<dyn MultiLineTextResource> {
        Rc::new(SkiaMultiLineTextResource::new(cols, rows, placeholder))
    }

    /// `createCheckButtonResource`.
    fn create_check_button_resource(&self, activated: bool) -> Rc<dyn CheckButtonResource> {
        Rc::new(SkiaCheckButtonResource::new(activated))
    }

    /// `createRadioButtonResource` — verbatim `groupedWith`
    /// (groupe partagé via downcast `as_any`, sinon nouveau).
    fn create_radio_button_resource(
        &self,
        grouped_with: Option<Rc<dyn RadioButtonResource>>,
        activated: bool,
    ) -> Rc<dyn RadioButtonResource> {
        let group = grouped_with
            .and_then(|g| {
                g.as_any()
                    .and_then(|a| a.downcast_ref::<SkiaRadioButtonResource>())
                    .map(|s| Rc::clone(&s.group))
            })
            .unwrap_or_else(|| {
                Rc::new(RefCell::new(RadioGroup {
                    members: Vec::new(),
                }))
            });
        let res: Rc<dyn RadioButtonResource> =
            Rc::new(SkiaRadioButtonResource::new(activated, Rc::clone(&group)));
        // `new Group(this)` / `group->add(this)` verbatim —
        // le membre s'enregistre dans le groupe.
        group.borrow_mut().members.push(Rc::downgrade(&res));
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::SkiaPlatform;
    use dillo_dw::platform::Platform;
    use dillo_dw::style::{FontAttrs, FontStyle, FontVariant, Style, StyleAttrs};

    fn mk_layout() -> (Rc<RefCell<SkiaPlatform>>, Layout) {
        let p = Rc::new(RefCell::new(SkiaPlatform::new()));
        let pdyn: Rc<RefCell<dyn Platform>> = p.clone();
        (p, Layout::new(pdyn, false))
    }

    fn test_font(p: &Rc<RefCell<SkiaPlatform>>) -> Rc<Font> {
        let attrs = FontAttrs {
            name: b"DejaVu Sans".to_vec(),
            size: 14,
            weight: 400,
            letter_spacing: 0,
            font_variant: FontVariant::Normal,
            style: FontStyle::Normal,
        };
        Platform::create_font(&mut *p.borrow_mut(), &attrs, false)
    }

    /// `sizeRequest` label ≙ `fl_width(label)+2*RELIEF_X` ×
    /// `font+RELIEF_Y` (fltkui.cc:672).
    #[test]
    fn label_button_size_request_verbatim() {
        let (p, mut l) = mk_layout();
        let font = test_font(&p);
        let style = Style::create(&StyleAttrs {
            font: Some(Rc::clone(&font)),
            ..StyleAttrs::default()
        });
        let res = SkiaResourceFactory::new().create_label_button_resource("x");
        res.set_style(&style);
        let mut req = Requisition::default();
        res.size_request(&mut l, &mut req);
        let tw = l.text_width(&font, b"x");
        assert_eq!(req.width, tw + 2 * RELIEF_X_THICKNESS);
        assert_eq!(req.ascent, font.ascent + RELIEF_Y_THICKNESS);
        assert_eq!(req.descent, font.descent + RELIEF_Y_THICKNESS);
    }

    /// Sans style → requis de repli `1 × (1+0)` verbatim.
    #[test]
    fn label_button_no_style_fallback() {
        let (_p, mut l) = mk_layout();
        let res = SkiaResourceFactory::new().create_label_button_resource("x");
        let mut req = Requisition::default();
        res.size_request(&mut l, &mut req);
        assert_eq!((req.width, req.ascent, req.descent), (1, 1, 0));
    }

    /// Entry ≙ `fl_width("n")*size + label + reliefs`
    /// (fltkui.cc:818).
    #[test]
    fn entry_size_request_scales_with_size() {
        let (p, mut l) = mk_layout();
        let font = test_font(&p);
        let style = Style::create(&StyleAttrs {
            font: Some(Rc::clone(&font)),
            ..StyleAttrs::default()
        });
        let f = SkiaResourceFactory::new();
        let (e5, e10) = (
            f.create_entry_resource(5, false, "", ""),
            f.create_entry_resource(10, false, "", ""),
        );
        e5.set_style(&style);
        e10.set_style(&style);
        let (mut r5, mut r10) = (Requisition::default(), Requisition::default());
        e5.size_request(&mut l, &mut r5);
        e10.size_request(&mut l, &mut r10);
        assert!(r10.width > r5.width, "size 10 > size 5");
        let n = l.text_width(&font, b"n");
        assert_eq!(r5.width, n * 5 + 2 * RELIEF_X_THICKNESS);
    }

    /// Radio : `groupedWith` — les deux radios partagent le
    /// groupe (≙ `Fl_Group`) ; l'exclusivité est appliquée par le
    /// receiver (`FormRadioActivateReceiver` amont), pas par
    /// `setActivated` — verbatim.
    #[test]
    fn radio_group_shared() {
        let f = SkiaResourceFactory::new();
        let r1 = f.create_radio_button_resource(None, true);
        let r2 = f.create_radio_button_resource(Some(Rc::clone(&r1)), false);
        assert!(r1.is_activated() && !r2.is_activated());
        let mut it = r2.group_iterator();
        let mut n = 0;
        while it.get_next().is_some() {
            n += 1;
        }
        assert_eq!(n, 2, "les deux membres dans le groupe");
        // Sans `grouped_with` → groupe indépendant.
        let r3 = f.create_radio_button_resource(None, false);
        let mut it3 = r3.group_iterator();
        let mut n3 = 0;
        while it3.get_next().is_some() {
            n3 += 1;
        }
        assert_eq!(n3, 1);
    }
}
