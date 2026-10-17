//! Port of `dw/ui.hh` + `dw/ui.cc` — `Embed`/`Resource` (form
//! controls, phase 4) verbatim.
//!
//! Modèle D :
//! - `Embed : Widget` ≙ `WidgetKind::Embed { res }` — dispatch dans
//!   `widget.rs` (`w_*` arms).
//! - `Resource*` ≙ `Rc<dyn Resource>` ; méthodes `&self` + mutabilité
//!   intérieure (Cell/RefCell) car le même objet est re-entrant :
//!   la machinerie du layout interne remonte vers `Embed::*OfChild`
//!   → resource pendant que `size_request` tourne (`RefCell`
//!   interdirait un second `borrow_mut`).
//! - `childWidget`/`embed` partagent la MÊME arène (2 `Layout`, 1
//!   `Arena` — `Layout::with_arena`) : `quasiParent` cross-layout
//!   amont ≙ `WidgetId` plain. Écart documenté : `getEmbed()->*`
//!   s'exécute dans le contexte du layout appelant (identique sauf
//!   `uses_viewport`/viewport fallback pour un widget sans parent —
//!   `Embed` a toujours un parent externe).
//! - `LayoutReceiver::resizeQueued` ≙ `EmbedResizeRelay` poussant
//!   `(layout_id, embed, 0, ec)` dans `Arena::pending_resizes`
//!   (différé vs synchrone amont — même issue après `drain_idles`).
//! - `Object*` args des signaux ≙ `&dyn Resource` / `&EventButton`.
//! - `GroupIterator::unref` ≙ drop (`Box<dyn RadioGroupIterator>`).
//! - `delete layout` (`~ComplexButtonResource`) ≙ `Drop` naturel.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use dillo_common::lout::signal::Emitter;

use crate::events::EventButton;
use crate::iterator::{DwIterator, EmptyIterator};
use crate::layout::{Arena, Layout, LayoutReceiver};
use crate::platform::Platform;
use crate::style::{is_per_length, Style};
use crate::types::{Allocation, DrawingContext, Extremes, Rectangle, Requisition};
use crate::view::View;
use crate::widget::{SplitHeightFun, WidgetNode};
use crate::WidgetId;

// ------------------------------------------------------------------
// Receivers (interfaces verbatim ui.hh).
// ------------------------------------------------------------------

/// `Resource::ActivateReceiver` — signaux `activate`/`enter`/`leave`.
pub trait ActivateReceiver {
    /// `activate`.
    fn activate(&mut self, _resource: &dyn Resource) {}
    /// `enter`.
    fn enter(&mut self, _resource: &dyn Resource) {}
    /// `leave`.
    fn leave(&mut self, _resource: &dyn Resource) {}
}

/// `Resource::ClickedReceiver` — signal `clicked(resource, event)`.
pub trait ClickedReceiver {
    /// `clicked`.
    fn clicked(&mut self, _resource: &dyn Resource, _event: &EventButton) {}
}

/// `ActivateEmitter` + `ClickedEmitter` — membres verbatim de
/// `Resource`. `RefCell` car `Resource` s'expose en `&self`.
pub struct ResourceEmitters {
    /// `activateEmitter`.
    activate: RefCell<Emitter<dyn ActivateReceiver>>,
    /// `clickedEmitter`.
    clicked: RefCell<Emitter<dyn ClickedReceiver>>,
}

impl ResourceEmitters {
    /// Émetteurs vides (verbatim : `connect` à la demande).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for ResourceEmitters {
    fn default() -> Self {
        Self {
            activate: RefCell::new(Emitter::new()),
            clicked: RefCell::new(Emitter::new()),
        }
    }
}

// ------------------------------------------------------------------
// Resource — interface verbatim.
// ------------------------------------------------------------------

/// `dw::core::ui::Resource`. Toutes les méthodes sont `&self` (état en
/// `Cell`/`RefCell` interne) — ré-entrance requise par le dispatch
/// `w_*_of_child` (Embed) pendant `size_request` interne.
pub trait Resource {
    /// ≙ `this` (passé aux receivers comme `Resource*`).
    fn as_resource(&self) -> &dyn Resource;
    /// `getEmbed` — `None` ≙ `NULL`.
    fn embed(&self) -> Option<WidgetId>;
    /// `setEmbed` (appelé par le ctor `Embed` ≙ `add_embed_widget`).
    fn set_embed(&self, embed: Option<WidgetId>);
    /// Émetteurs (verbatim membres `activateEmitter`/`clickedEmitter`).
    fn emitters(&self) -> &ResourceEmitters;

    /// `sizeRequest` — pure virtual amont.
    fn size_request(&self, l: &mut Layout, requisition: &mut Requisition);
    /// `getExtremes` — défaut verbatim ui.cc:222 : requisition →
    /// min/max (+ intrinsic identiques).
    fn get_extremes(&self, l: &mut Layout, extremes: &mut Extremes) {
        let mut req = Requisition::default();
        self.size_request(l, &mut req);
        extremes.min_width = req.width;
        extremes.max_width = req.width;
        extremes.min_width_intrinsic = extremes.min_width;
        extremes.max_width_intrinsic = extremes.max_width;
    }
    /// `sizeAllocate` — défaut no-op (ui.cc:236).
    fn size_allocate(&self, _l: &mut Layout, _allocation: &Allocation) {}
    /// `getAvailWidthOfChild` — `notImplemented` → `0` (ui.cc:237).
    fn get_avail_width_of_child(
        &self,
        _l: &mut Layout,
        _child: WidgetId,
        _force_value: bool,
    ) -> i32 {
        0
    }
    /// `getAvailHeightOfChild` — `notImplemented` → `0`.
    fn get_avail_height_of_child(
        &self,
        _l: &mut Layout,
        _child: WidgetId,
        _force_value: bool,
    ) -> i32 {
        0
    }
    /// `correctRequisitionOfChild` — `notImplemented` (ui.cc:251).
    fn correct_requisition_of_child(
        &self,
        _l: &mut Layout,
        _child: WidgetId,
        _requisition: &mut Requisition,
        _split_height_fun: SplitHeightFun,
        _allow_decrease_width: bool,
        _allow_decrease_height: bool,
    ) {
    }
    /// `correctExtremesOfChild` — `notImplemented` (ui.cc:262).
    fn correct_extremes_of_child(
        &self,
        _l: &mut Layout,
        _child: WidgetId,
        _extremes: &mut Extremes,
        _use_adjustment_width: bool,
    ) {
    }
    /// `containerSizeChangedForChildren` — défaut « no children ».
    fn container_size_changed_for_children(&self, _l: &mut Layout) {}
    /// `setDisplayed` — défaut no-op.
    fn set_displayed(&self, _displayed: bool) {}
    /// `draw` — défaut no-op (ui.cc:274).
    fn draw(&self, _view: &mut dyn View, _area: &Rectangle, _context: &mut DrawingContext) {}
    /// `iterator` — pure virtual amont.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator>;
    /// `setStyle` — défaut no-op.
    fn set_style(&self, _style: &Rc<Style>) {}
    /// `isEnabled` — pure virtual.
    fn is_enabled(&self) -> bool;
    /// `setEnabled` — pure virtual.
    fn set_enabled(&self, enabled: bool);

    /// `connectActivate` — verbatim.
    fn connect_activate(&self, receiver: &Rc<RefCell<dyn ActivateReceiver>>) {
        self.emitters().activate.borrow_mut().connect(receiver);
    }
    /// `connectClicked` — verbatim.
    fn connect_clicked(&self, receiver: &Rc<RefCell<dyn ClickedReceiver>>) {
        self.emitters().clicked.borrow_mut().connect(receiver);
    }
    /// `emitActivate` — `ActivateEmitter::emitActivate(this)`.
    fn emit_activate(&self) {
        let res = self.as_resource();
        self.emitters()
            .activate
            .borrow_mut()
            .emit_void(|r| r.activate(res));
    }
    /// `emitClicked` — `ClickedEmitter::emitClicked(this, event)`.
    fn emit_clicked(&self, event: &EventButton) {
        let res = self.as_resource();
        self.emitters()
            .clicked
            .borrow_mut()
            .emit_void(|r| r.clicked(res, event));
    }
    // -- downcasts verbatim `form.cc` ((EntryResource*)res etc.) ------
    // Chaque impl concrète override l'accesseur de SON sous-trait.

    /// `(TextResource*)res`.
    fn as_text_resource(&self) -> Option<&dyn TextResource> {
        None
    }
    /// `(EntryResource*)res`.
    fn as_entry_resource(&self) -> Option<&dyn EntryResource> {
        None
    }
    /// `(MultiLineTextResource*)res`.
    fn as_multi_line_text_resource(&self) -> Option<&dyn MultiLineTextResource> {
        None
    }
    /// `(ToggleButtonResource*)res`.
    fn as_toggle_button_resource(&self) -> Option<&dyn ToggleButtonResource> {
        None
    }
    /// `(CheckButtonResource*)res`.
    fn as_check_button_resource(&self) -> Option<&dyn CheckButtonResource> {
        None
    }
    /// `(RadioButtonResource*)res`.
    fn as_radio_button_resource(&self) -> Option<&dyn RadioButtonResource> {
        None
    }
    /// `Rc` du sous-trait — ≙ cast `RadioButtonResource*` quand un
    /// `Rc` typé est requis (`create_radio_button_resource`,
    /// groupement amont). Les impls radio le surchargent ; défaut
    /// `None` (≙ cast échoué).
    fn as_radio_button_resource_rc(self: Rc<Self>) -> Option<Rc<dyn RadioButtonResource>> {
        let _ = self;
        None
    }

    /// Downcast plateforme — ≙ les casts `Fltk*Resource*` amont.
    /// Les impls concrètes retournent `Some(self)` ; `None` par
    /// défaut (≙ cast échoué).
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }
    /// `(SelectionResource*)res`.
    fn as_selection_resource(&self) -> Option<&dyn SelectionResource> {
        None
    }
    /// `(OptionMenuResource*)res`.
    fn as_option_menu_resource(&self) -> Option<&dyn OptionMenuResource> {
        None
    }
    /// `(ListResource*)res`.
    fn as_list_resource(&self) -> Option<&dyn ListResource> {
        None
    }
    /// `(LabelButtonResource*)res`.
    fn as_label_button_resource(&self) -> Option<&dyn LabelButtonResource> {
        None
    }
    /// `(ComplexButtonResource*)res`.
    fn as_complex_button_resource(&self) -> Option<&ComplexButtonResource> {
        None
    }

    /// `emitEnter` (ui.cc:278) — signal 1.
    fn emit_enter(&self) {
        let res = self.as_resource();
        self.emitters()
            .activate
            .borrow_mut()
            .emit_void(|r| r.enter(res));
    }
    /// `emitLeave` (ui.cc:282) — signal 2.
    fn emit_leave(&self) {
        let res = self.as_resource();
        self.emitters()
            .activate
            .borrow_mut()
            .emit_void(|r| r.leave(res));
    }
    /// `queueResize` — verbatim ui.hh:339 : `embed->queueResize(0, ec)`.
    fn queue_resize(&self, l: &mut Layout, extremes_changed: bool) {
        if let Some(e) = self.embed() {
            l.widget_queue_resize(e, 0, extremes_changed);
        }
    }
}

/// `Debug` pour `dyn Resource` (requis par `#[derive(Debug)]
/// WidgetKind`) — identité par `embed()`.
impl std::fmt::Debug for dyn Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resource")
            .field("embed", &self.embed())
            .finish()
    }
}

/// Helper `iterator` partagé : `EmptyIterator(embed, mask, atEnd)`
/// verbatim (toutes les impls d'`iterator` de ui.cc retournent
/// `EmptyIterator` avec `\bug Not implemented`).
fn empty_iterator(embed: Option<WidgetId>, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
    // `usize::MAX` ≙ `NULL` amont — un EmptyIterator ne produit pas
    // de part, l'id n'est jamais lu.
    Box::new(EmptyIterator::new(
        embed.unwrap_or(usize::MAX),
        mask,
        at_end,
    ))
}

// ------------------------------------------------------------------
// Sous-interfaces (verbatim ui.hh:387-537).
// ------------------------------------------------------------------

/// `ButtonResource` — marker trait amont.
pub trait ButtonResource: Resource {}

/// `LabelButtonResource` (ui.hh:400) — `iterator` ≙ `EmptyIterator`
/// verbatim (`\bug Not implemented` — `TextIterator` commenté).
pub trait LabelButtonResource: ButtonResource {
    /// `getLabel`.
    fn get_label(&self) -> String;
    /// `setLabel`.
    fn set_label(&self, label: &str);
    /// `iterator` — verbatim ui.cc:312.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        empty_iterator(self.embed(), mask, at_end)
    }
}

/// `SelectionResource` (ui.hh:447) — listes/option-menus.
pub trait SelectionResource: Resource {
    /// `addItem`.
    fn add_item(&self, str_: &str, enabled: bool, selected: bool);
    /// `setItem`.
    fn set_item(&self, index: i32, selected: bool);
    /// `pushGroup`.
    fn push_group(&self, name: &str, enabled: bool);
    /// `popGroup`.
    fn pop_group(&self);
    /// `getNumberOfItems`.
    fn number_of_items(&self) -> i32;
    /// `isSelected`.
    fn is_selected(&self, index: i32) -> bool;
}

/// `ListResource` (ui.hh:457).
pub trait ListResource: SelectionResource {}

/// `ListResource::SelectionMode` — verbatim enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    /// `SELECTION_EXACTLY_ONE`.
    ExactlyOne,
    /// `SELECTION_EXACTLY_ONE_BY_USER`.
    ExactlyOneByUser,
    /// `SELECTION_AT_MOST_ONE`.
    AtMostOne,
    /// `SELECTION_MULTIPLE`.
    Multiple,
}

/// `OptionMenuResource` (ui.hh:487) — marker.
pub trait OptionMenuResource: SelectionResource {}

/// `TextResource` (ui.hh:491) — `iterator` ≙ `EmptyIterator`
/// verbatim (`\bug Not implemented`).
pub trait TextResource: Resource {
    /// `getText`.
    fn get_text(&self) -> String;
    /// `setText`.
    fn set_text(&self, text: &str);
    /// `isEditable`.
    fn is_editable(&self) -> bool;
    /// `setEditable`.
    fn set_editable(&self, editable: bool);
    /// `iterator` — verbatim ui.cc:495.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        empty_iterator(self.embed(), mask, at_end)
    }
}

/// `EntryResource` (ui.hh:502) — `UNLIMITED_SIZE = -1` verbatim.
pub trait EntryResource: TextResource {
    /// `setMaxLength` (`-1` = `UNLIMITED_SIZE`).
    fn set_max_length(&self, maxlen: i32);
}

/// `UNLIMITED_SIZE` verbatim.
pub const ENTRY_UNLIMITED_SIZE: i32 = -1;

/// `MultiLineTextResource` (ui.hh:508) — marker.
pub trait MultiLineTextResource: TextResource {}

/// `ToggleButtonResource` (ui.hh:513).
pub trait ToggleButtonResource: Resource {
    /// `isActivated`.
    fn is_activated(&self) -> bool;
    /// `setActivated`.
    fn set_activated(&self, activated: bool);
}

/// `CheckButtonResource` (ui.hh:520) — `iterator` ≙ `EmptyIterator`
/// verbatim (`\bug Not implemented` — `"[X]"/"[ ]"` commenté).
pub trait CheckButtonResource: ToggleButtonResource {
    /// `iterator` — verbatim ui.cc:503.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        empty_iterator(self.embed(), mask, at_end)
    }
}

/// `RadioButtonResource::GroupIterator` — `unref` ≙ drop.
pub trait RadioGroupIterator {
    /// `hasNext`.
    fn has_next(&self) -> bool;
    /// `getNext` — `None` ≙ fin.
    fn get_next(&mut self) -> Option<Rc<dyn RadioButtonResource>>;
}

/// `RadioButtonResource` (ui.hh:526) — `iterator` ≙ `EmptyIterator`
/// verbatim (`\bug Not implemented` — `"(*)"/"( )"` commenté).
pub trait RadioButtonResource: ToggleButtonResource {
    /// `groupIterator`.
    fn group_iterator(&self) -> Box<dyn RadioGroupIterator>;
    /// `iterator` — verbatim ui.cc:522.
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        empty_iterator(self.embed(), mask, at_end)
    }
}

// ------------------------------------------------------------------
// ResourceFactory (ui.hh:545) — verbatim.
// ------------------------------------------------------------------

/// `dw::core::ui::ResourceFactory` — impl dans `dillo-platform`
/// (≙ `dw/fltk/fltkui.cc`).
pub trait ResourceFactory {
    /// `createLabelButtonResource`.
    fn create_label_button_resource(&self, label: &str) -> Rc<dyn LabelButtonResource>;
    /// `createComplexButtonResource` — `widget` = `WidgetId` dans
    /// l'arène partagée (modèle D).
    fn create_complex_button_resource(
        &self,
        widget: WidgetId,
        relief: bool,
    ) -> Rc<ComplexButtonResource>;
    /// `createListResource`.
    fn create_list_resource(&self, mode: SelectionMode, rows: i32) -> Rc<dyn ListResource>;
    /// `createOptionMenuResource`.
    fn create_option_menu_resource(&self) -> Rc<dyn OptionMenuResource>;
    /// `createEntryResource`.
    fn create_entry_resource(
        &self,
        size: i32,
        password: bool,
        label: &str,
        placeholder: &str,
    ) -> Rc<dyn EntryResource>;
    /// `createMultiLineTextResource`.
    fn create_multi_line_text_resource(
        &self,
        cols: i32,
        rows: i32,
        placeholder: &str,
    ) -> Rc<dyn MultiLineTextResource>;
    /// `createCheckButtonResource`.
    fn create_check_button_resource(&self, activated: bool) -> Rc<dyn CheckButtonResource>;
    /// `createRadioButtonResource`.
    fn create_radio_button_resource(
        &self,
        grouped_with: Option<Rc<dyn RadioButtonResource>>,
        activated: bool,
    ) -> Rc<dyn RadioButtonResource>;
}

// ------------------------------------------------------------------
// ComplexButtonResource (ui.hh:406 + ui.cc:322-489) — verbatim.
// ------------------------------------------------------------------

/// Opérations déléguées à l'implémentation plateforme — ≙ méthodes
/// virtuelles pures de `ComplexButtonResource` amont
/// (`createPlatform`/`setLayout`/`relief*Thickness`) + les
/// `Resource` abstraites que la sous-classe fournissait
/// (`isEnabled`/`setEnabled`/`setDisplayed`/`draw`/`setStyle`).
pub trait ComplexButtonOps {
    /// `createPlatform` — nouvelle plateforme du layout interne.
    fn create_platform(&mut self) -> Rc<RefCell<dyn Platform>>;
    /// `setLayout` — transmission du layout interne (≙ `this->layout`).
    fn set_layout(&mut self, layout: &mut Layout);
    /// Seam plateforme (pas amont) : cellule partagée du layout
    /// interne — `Rc` possédé par `ComplexButtonResource` — pour
    /// que `draw` de la plateforme pilote `layout->draw` (≙
    /// `flatView->draw` amont).
    fn set_layout_cell(&mut self, _cell: std::rc::Weak<RefCell<Option<Layout>>>) {}
    /// `reliefXThickness` — épaisseur relief horizontale.
    fn relief_x_thickness(&self) -> i32;
    /// `reliefYThickness` — épaisseur relief verticale.
    fn relief_y_thickness(&self) -> i32;
    /// `isEnabled` (abstract `Resource` implémenté par la sous-classe).
    fn is_enabled(&self) -> bool;
    /// `setEnabled`.
    fn set_enabled(&mut self, enabled: bool);
    /// `setDisplayed`.
    fn set_displayed(&mut self, displayed: bool);
    /// `draw` (dessin du bouton par la plateforme).
    fn draw(&self, view: &mut dyn View, area: &Rectangle, context: &mut DrawingContext);
    /// `setStyle`.
    fn set_style(&mut self, style: &Rc<Style>);
    /// `sizeAllocate` — verbatim `FltkComplexButtonResource::
    /// sizeAllocate` (positionne la flat view interne).
    fn size_allocate(&mut self, _allocation: &Allocation) {}
}

/// `ComplexButtonResource::LayoutReceiver` — verbatim : relay de
/// `resizeQueued` interne vers `resource->queueResize` ≙
/// `embed->queueResize (0, ec)`. Modèle D : file différée
/// `pending_resizes` taguée `outer_layout` (drainée par
/// `Layout::drain_idles` du layout externe).
struct EmbedResizeRelay {
    /// Arène partagée externe.
    outer: Weak<RefCell<Arena>>,
    /// `Layout::id` du layout externe.
    outer_layout: usize,
    /// Le widget `Embed`.
    embed: WidgetId,
}

impl LayoutReceiver for EmbedResizeRelay {
    fn resize_queued(&mut self, extremes_changed: bool) {
        if let Some(a) = self.outer.upgrade() {
            a.borrow_mut().pending_resizes.push((
                self.outer_layout,
                self.embed,
                0,
                extremes_changed,
            ));
        }
    }
    /// `canvasSizeChanged` — non relayé amont.
    fn canvas_size_changed(&mut self, _w: i32, _a: i32, _d: i32) {}
}

/// `dw::core::ui::ComplexButtonResource` — bouton contenant un
/// vrai widget dw dans un `Layout` interne (verbatim ui.cc:322+).
pub struct ComplexButtonResource {
    /// Opérations plateforme (≙ virtuelles amont).
    ops: RefCell<Box<dyn ComplexButtonOps>>,
    /// `layout` — layout interne (≙ `Layout *layout` ; `RefCell`
    /// pour méthodes `&self` — `Option` car posé dans `init`).
    layout: Rc<RefCell<Option<Layout>>>,
    /// `childWidget`.
    child_widget: Cell<Option<WidgetId>>,
    /// `embed` (≙ `Resource::embed`).
    embed: Cell<Option<WidgetId>>,
    /// `click_x`/`click_y`.
    click_x: Cell<i32>,
    click_y: Cell<i32>,
    /// Émetteurs verbatim.
    emitters: ResourceEmitters,
    /// `layoutReceiver` — garde le `Rc` vivant (Emitter tient `Weak`).
    relay: RefCell<Option<Rc<RefCell<EmbedResizeRelay>>>>,
}

impl std::fmt::Debug for ComplexButtonResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComplexButtonResource")
            .field("embed", &self.embed.get())
            .field("child_widget", &self.child_widget.get())
            .finish_non_exhaustive()
    }
}

impl ComplexButtonResource {
    /// `ComplexButtonResource()` — verbatim : `layout=NULL`,
    /// `click_x=click_y=-1`.
    #[must_use]
    pub fn new(ops: Box<dyn ComplexButtonOps>) -> Self {
        Self {
            ops: RefCell::new(ops),
            layout: Rc::new(RefCell::new(None)),
            child_widget: Cell::new(None),
            embed: Cell::new(None),
            click_x: Cell::new(-1),
            click_y: Cell::new(-1),
            emitters: ResourceEmitters::new(),
            relay: RefCell::new(None),
        }
    }

    /// `init (Widget *widget)` — verbatim ui.cc:343 :
    /// `childWidget`, `new Layout (createPlatform(), false)` (≙
    /// `Layout::with_arena` — arène partagée, `uses_viewport=false`),
    /// `setLayout`, `layout->setWidget`, `connect(&layoutReceiver)`,
    /// `if (getEmbed()) childWidget->setQuasiParent(getEmbed())`.
    ///
    /// `outer` = layout externe contenant le `Embed` — fournit
    /// l'arène partagée + l'id de tag.
    pub fn init(&self, outer: &Layout, widget: WidgetId) {
        self.child_widget.set(Some(widget));
        let platform = self.ops.borrow_mut().create_platform();
        // `new Layout (createPlatform(), false)` — verbatim
        // uses_viewport=false ; arène partagée (modèle D).
        let mut layout = Layout::with_arena(outer.arena.clone(), platform, false);
        self.ops.borrow_mut().set_layout(&mut layout);
        layout.set_widget(widget);
        // `layout->connect(&layoutReceiver)`.
        let relay = Rc::new(RefCell::new(EmbedResizeRelay {
            outer: Rc::downgrade(&outer.arena),
            outer_layout: outer.id,
            embed: self.embed.get().unwrap_or(usize::MAX),
        }));
        let relay_dyn: Rc<RefCell<dyn LayoutReceiver>> = relay.clone();
        layout.connect_layout_receiver(&relay_dyn);
        *self.relay.borrow_mut() = Some(relay);
        *self.layout.borrow_mut() = Some(layout);
        // Seam port : l'ops plateforme pilote `layout->draw` depuis
        // son `draw` (≙ `flatView->draw` amont).
        self.ops
            .borrow_mut()
            .set_layout_cell(Rc::downgrade(&self.layout));
        // `if (getEmbed()) childWidget->setQuasiParent(getEmbed())`.
        if let Some(e) = self.embed.get() {
            if let Some(l) = self.layout.borrow_mut().as_mut() {
                l.widget_set_quasi_parent(widget, Some(e));
            }
        }
    }

    /// `getClickX`/`getClickY` — verbatim.
    #[must_use]
    pub fn get_click_x(&self) -> i32 {
        self.click_x.get()
    }
    /// `getClickY`.
    #[must_use]
    pub fn get_click_y(&self) -> i32 {
        self.click_y.get()
    }
    /// `click_x`/`click_y` — posés par l'implémentation plateforme.
    pub fn set_click(&self, x: i32, y: i32) {
        self.click_x.set(x);
        self.click_y.set(y);
    }
}

impl ButtonResource for ComplexButtonResource {}

impl Resource for ComplexButtonResource {
    fn as_resource(&self) -> &dyn Resource {
        self
    }
    fn embed(&self) -> Option<WidgetId> {
        self.embed.get()
    }

    /// `setEmbed` — verbatim ui.cc:358 : `ButtonResource::setEmbed`
    /// (stockage) + `if (childWidget) childWidget->setQuasiParent`.
    fn set_embed(&self, embed: Option<WidgetId>) {
        self.embed.set(embed);
        if let (Some(w), Some(e)) = (self.child_widget.get(), embed) {
            if let Some(l) = self.layout.borrow_mut().as_mut() {
                l.widget_set_quasi_parent(w, Some(e));
            }
        }
    }

    fn emitters(&self) -> &ResourceEmitters {
        &self.emitters
    }

    /// `sizeRequest` — verbatim ui.cc:372 : requisition enfant +
    /// `2*reliefXThickness` / `reliefYThickness`.
    fn size_request(&self, _l: &mut Layout, requisition: &mut Requisition) {
        let mut req = Requisition::default();
        if let (Some(l), Some(w)) = (self.layout.borrow_mut().as_mut(), self.child_widget.get()) {
            l.widget_size_request(w, &mut req, 0, &[], &[], &[]);
        }
        let rx = self.ops.borrow().relief_x_thickness();
        let ry = self.ops.borrow().relief_y_thickness();
        requisition.width = req.width + 2 * rx;
        requisition.ascent = req.ascent + ry;
        requisition.descent = req.descent + ry;
    }

    /// `getExtremes` — verbatim ui.cc:387 : extremes enfant +
    /// `2*reliefXThickness`, intrinsic = ajustés.
    fn get_extremes(&self, _l: &mut Layout, extremes: &mut Extremes) {
        let mut ext = Extremes::default();
        if let (Some(l), Some(w)) = (self.layout.borrow_mut().as_mut(), self.child_widget.get()) {
            l.widget_get_extremes(w, &mut ext, 0, &[], &[], &[]);
        }
        let rx = self.ops.borrow().relief_x_thickness();
        extremes.min_width = ext.min_width + 2 * rx;
        extremes.max_width = ext.max_width + 2 * rx;
        extremes.min_width_intrinsic = extremes.min_width;
        extremes.max_width_intrinsic = extremes.max_width;
    }

    /// `sizeAllocate` — relayé aux ops (≙ `FltkComplexButtonResource`
    /// qui redimensionne la flat view — fltkui.cc).
    fn size_allocate(&self, _l: &mut Layout, allocation: &Allocation) {
        self.ops.borrow_mut().size_allocate(allocation);
    }

    /// `getAvailWidthOfChild` — verbatim ui.cc:407 :
    /// `max(getEmbed()->getAvailWidth - 2*reliefX, 0)` (`-1` propagé).
    fn get_avail_width_of_child(&self, l: &mut Layout, _child: WidgetId, force_value: bool) -> i32 {
        let Some(e) = self.embed.get() else {
            return -1;
        };
        let w = l.widget_get_avail_width(e, force_value);
        if w == -1 {
            return -1;
        }
        (w - 2 * self.ops.borrow().relief_x_thickness()).max(0)
    }

    /// `getAvailHeightOfChild` — verbatim ui.cc:416.
    fn get_avail_height_of_child(
        &self,
        l: &mut Layout,
        _child: WidgetId,
        force_value: bool,
    ) -> i32 {
        let Some(e) = self.embed.get() else {
            return -1;
        };
        let h = l.widget_get_avail_height(e, force_value);
        if h == -1 {
            return -1;
        }
        (h - 2 * self.ops.borrow().relief_y_thickness()).max(0)
    }

    /// `correctRequisitionOfChild` — verbatim ui.cc:426 : pour les
    /// % le relief est déduit de `availWidth - boxDiffWidth - 2*reliefX`
    /// (avec le `FIXME` typo `getAvailHeight` verbatim) ; sinon
    /// `correctReq*OfChildNoRec` (non-virtuel = impl `Widget` de
    /// base sur `Embed`).
    fn correct_requisition_of_child(
        &self,
        l: &mut Layout,
        child: WidgetId,
        requisition: &mut Requisition,
        split_height_fun: SplitHeightFun,
        allow_decrease_width: bool,
        allow_decrease_height: bool,
    ) {
        let Some(e) = self.embed.get() else {
            return;
        };
        let child_width = l.node(child, |n| n.style.as_ref().map(|s| s.attrs.width));
        if let Some(w_per) = child_width.filter(|w| is_per_length(*w)) {
            // FIXME amont verbatim : `getAvailHeight` (typo pour
            // getAvailWidth) — conservé.
            let avail_width = l.widget_get_avail_height(e, false);
            if avail_width != -1 {
                let base = (avail_width
                    - l.node(e, WidgetNode::box_diff_width)
                    - 2 * self.ops.borrow().relief_x_thickness())
                .max(0);
                let new_width = l.w_apply_per_width(child, base, w_per);
                requisition.width = if allow_decrease_width {
                    new_width
                } else {
                    requisition.width.max(new_width)
                };
            }
        } else {
            // `getEmbed()->correctReqWidthOfChildNoRec` =
            // `Widget::correctReqWidthOfChild` (non-virtuel).
            l.widget_correct_req_width_of_child(e, child, requisition, allow_decrease_width);
        }
        // `TODO Percentage heights are ignored again` — verbatim.
        l.widget_correct_req_height_of_child(
            e,
            child,
            requisition,
            split_height_fun,
            allow_decrease_height,
        );
    }

    /// `correctExtremesOfChild` — verbatim ui.cc:459 : même
    /// correction %-relief, sinon `correctExtremesOfChildNoRec`
    /// (= `Widget::correctExtremesOfChild` non-virtuel sur Embed).
    fn correct_extremes_of_child(
        &self,
        l: &mut Layout,
        child: WidgetId,
        extremes: &mut Extremes,
        use_adjustment_width: bool,
    ) {
        let Some(e) = self.embed.get() else {
            return;
        };
        let child_width = l.node(child, |n| n.style.as_ref().map(|s| s.attrs.width));
        if let Some(w_per) = child_width.filter(|w| is_per_length(*w)) {
            let avail_width = l.widget_get_avail_height(e, false);
            if avail_width != -1 {
                let base = (avail_width
                    - l.node(e, WidgetNode::box_diff_width)
                    - 2 * self.ops.borrow().relief_x_thickness())
                .max(0);
                let w = l.w_apply_per_width(child, base, w_per);
                extremes.min_width = w;
                extremes.max_width = w;
            }
        } else {
            // `correctExtremesOfChildNoRec` ≙ branche de base de
            // `w_correct_extremes_of_child` (dispatch court-circuité).
            l.w_correct_extremes_of_child_base(e, child, extremes, use_adjustment_width);
        }
    }

    /// `containerSizeChangedForChildren` — verbatim ui.cc:481 :
    /// `layout->containerSizeChanged()`.
    fn container_size_changed_for_children(&self, _l: &mut Layout) {
        if let Some(inner) = self.layout.borrow_mut().as_mut() {
            inner.container_size_changed();
        }
    }

    /// `setDisplayed` — délégué aux ops plateforme (verbatim :
    /// implémenté par la sous-classe).
    fn set_displayed(&self, displayed: bool) {
        self.ops.borrow_mut().set_displayed(displayed);
    }

    /// `draw` — délégué aux ops plateforme.
    fn draw(&self, view: &mut dyn View, area: &Rectangle, context: &mut DrawingContext) {
        self.ops.borrow().draw(view, area, context);
    }

    /// `iterator` — verbatim ui.cc:486 : `EmptyIterator`
    /// (`\bug Implementation` — deux layouts, non implémenté amont).
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        empty_iterator(self.embed.get(), mask, at_end)
    }

    /// `setStyle` — délégué aux ops plateforme.
    fn set_style(&self, style: &Rc<Style>) {
        self.ops.borrow_mut().set_style(style);
    }

    /// `isEnabled` — délégué aux ops plateforme.
    fn is_enabled(&self) -> bool {
        self.ops.borrow().is_enabled()
    }
    /// `setEnabled` — délégué aux ops plateforme.
    fn set_enabled(&self, enabled: bool) {
        self.ops.borrow_mut().set_enabled(enabled);
    }
}

// ------------------------------------------------------------------
// Tests.
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventCrossing;
    use crate::imgbuf::{Imgbuf, ImgbufType};
    use crate::platform::{IdleCallback, Platform};
    use crate::style::{create_per_length, Color, Font, FontAttrs, StyleAttrs, Tooltip};
    use crate::types::content_type;
    use crate::widget::WidgetKind;

    // ---- Platform mock (verbatim du mock layout.rs) ----

    /// `Platform` mock — file d'idles interne.
    #[derive(Default)]
    struct MockPlatform {
        /// `(id, callback)` en attente.
        pending: Vec<(i32, usize, IdleCallback)>,
        /// Prochain id.
        next_id: i32,
    }

    impl Platform for MockPlatform {
        fn set_layout(&mut self, _l: usize) {}
        fn attach_view(&mut self, _v: &mut dyn View) {}
        fn detach_view(&mut self, _v: &mut dyn View) {}
        fn text_width(&mut self, _f: &Font, _t: &[u8]) -> i32 {
            0
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
        fn add_idle(&mut self, layout: usize, cb: IdleCallback) -> i32 {
            self.next_id += 1;
            self.pending.push((self.next_id, layout, cb));
            self.next_id
        }
        fn remove_idle(&mut self, id: i32) {
            self.pending.retain(|(i, _, _)| *i != id);
        }
        fn take_pending_idles(&mut self, layout: usize) -> Vec<IdleCallback> {
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
                .into_iter()
                .partition(|(_, l, _)| *l == layout);
            self.pending = rest;
            mine.into_iter().map(|(_, _, cb)| cb).collect()
        }
        fn create_font(&mut self, attrs: &FontAttrs, _te: bool) -> Rc<Font> {
            Rc::new(Font {
                attrs: attrs.clone(),
                ascent: 10,
                descent: 2,
                space_width: 4,
                x_height: 5,
                zero_width: 5,
            })
        }
        fn font_exists(&mut self, _n: &[u8]) -> bool {
            true
        }
        fn create_color(&mut self, c: i32) -> Color {
            Color::new(c)
        }
        fn create_tooltip(&mut self, t: &[u8]) -> Rc<Tooltip> {
            Tooltip::create(t, None)
        }
        fn cancel_tooltip(&mut self) {}
        fn create_imgbuf(&mut self, _t: ImgbufType, w: i32, h: i32, _g: f64) -> Rc<dyn Imgbuf> {
            Rc::new(NullImgbuf { w, h })
        }
        fn copy_selection(&mut self, _t: &[u8], _d: i32) {}
        fn get_resource_factory(&mut self) -> Option<Rc<dyn crate::ui::ResourceFactory>> {
            None
        }
    }

    /// `Imgbuf` minimal pour le mock.
    struct NullImgbuf {
        /// Largeur.
        w: i32,
        /// Hauteur.
        h: i32,
    }

    impl Imgbuf for NullImgbuf {
        fn set_cmap(&self, _c: &[i32]) {}
        fn copy_row(&self, _r: i32, _d: &[u8]) {}
        fn new_scan(&self) {}
        fn get_scaled_buf(self: Rc<Self>, _w: i32, _h: i32) -> Rc<dyn Imgbuf> {
            self
        }
        fn get_row_area(&self, _r: i32, _a: &mut Rectangle) {}
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
        fn create_similar_buf(&self, w: i32, h: i32) -> Rc<dyn Imgbuf> {
            Rc::new(NullImgbuf { w, h })
        }
        fn rgb_row(&self, _y: i32) -> Vec<u8> {
            Vec::new()
        }
        fn write_rgb_span(&self, _x: i32, _y: i32, _d: &[u8]) {}
        fn ref_(&self) {}
        fn unref(&self) {}
        fn last_reference(&self) -> bool {
            true
        }
        fn is_referred(&self) -> bool {
            false
        }
        fn set_delete_on_unref(&self, _v: bool) {}
    }

    // ---- Resource mock générique ----

    /// `Resource` enregistreur : requisition fixe + compteurs
    /// d'appels + signaux émis.
    struct MockRes {
        /// `embed`.
        embed: Cell<Option<WidgetId>>,
        /// Requisition retournée.
        req: Cell<Requisition>,
        /// `setStyle` appelé.
        styled: Cell<bool>,
        /// `enabled`.
        enabled: Cell<bool>,
        /// Émetteurs.
        emitters: ResourceEmitters,
    }

    impl MockRes {
        /// Ctor — requisition `(100, 20, 5)`.
        fn new() -> Self {
            Self {
                embed: Cell::new(None),
                req: Cell::new(Requisition {
                    width: 100,
                    ascent: 20,
                    descent: 5,
                }),
                styled: Cell::new(false),
                enabled: Cell::new(true),
                emitters: ResourceEmitters::new(),
            }
        }
    }

    impl Resource for MockRes {
        fn as_resource(&self) -> &dyn Resource {
            self
        }
        fn embed(&self) -> Option<WidgetId> {
            self.embed.get()
        }
        fn set_embed(&self, embed: Option<WidgetId>) {
            self.embed.set(embed);
        }
        fn emitters(&self) -> &ResourceEmitters {
            &self.emitters
        }
        fn size_request(&self, _l: &mut Layout, requisition: &mut Requisition) {
            *requisition = self.req.get();
        }
        fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
            empty_iterator(self.embed(), mask, at_end)
        }
        fn set_style(&self, _style: &Rc<Style>) {
            self.styled.set(true);
        }
        fn is_enabled(&self) -> bool {
            self.enabled.get()
        }
        fn set_enabled(&self, enabled: bool) {
            self.enabled.set(enabled);
        }
    }

    /// Receivers enregistreurs.
    #[derive(Default)]
    struct Recv {
        /// Événements reçus (`"a"`,`"e"`,`"l"`,`"c"`).
        log: RefCell<Vec<&'static str>>,
    }
    impl ActivateReceiver for Recv {
        fn activate(&mut self, _r: &dyn Resource) {
            self.log.borrow_mut().push("a");
        }
        fn enter(&mut self, _r: &dyn Resource) {
            self.log.borrow_mut().push("e");
        }
        fn leave(&mut self, _r: &dyn Resource) {
            self.log.borrow_mut().push("l");
        }
    }
    impl ClickedReceiver for Recv {
        fn clicked(&mut self, _r: &dyn Resource, _e: &EventButton) {
            self.log.borrow_mut().push("c");
        }
    }

    // ---- ComplexButtonResource mocks ----

    /// `ComplexButtonOps` mock : relief `5`/`2`, plateforme mock.
    /// Flags en `Rc<Cell>` — `ops` est un `Box<dyn>` non
    /// downcastable depuis le test.
    struct MockOps {
        /// `reliefXThickness`.
        rx: i32,
        /// `reliefYThickness`.
        ry: i32,
        /// `setLayout` appelé.
        layout_set: Rc<Cell<bool>>,
        /// `draw` appelé.
        drawn: Rc<Cell<bool>>,
    }

    impl ComplexButtonOps for MockOps {
        fn create_platform(&mut self) -> Rc<RefCell<dyn Platform>> {
            Rc::new(RefCell::new(MockPlatform::default()))
        }
        fn set_layout(&mut self, _layout: &mut Layout) {
            self.layout_set.set(true);
        }
        fn relief_x_thickness(&self) -> i32 {
            self.rx
        }
        fn relief_y_thickness(&self) -> i32 {
            self.ry
        }
        fn is_enabled(&self) -> bool {
            true
        }
        fn set_enabled(&mut self, _e: bool) {}
        fn set_displayed(&mut self, _d: bool) {}
        fn draw(&self, _v: &mut dyn View, _a: &Rectangle, _c: &mut DrawingContext) {
            self.drawn.set(true);
        }
        fn set_style(&mut self, _s: &Rc<Style>) {}
    }

    /// Layout externe + embed d'un `ComplexButtonResource` (relief
    /// `5`/`2`) — helper partagé.
    fn make_complex() -> (
        Layout,
        Rc<ComplexButtonResource>,
        WidgetId,
        WidgetId,
        Rc<Cell<bool>>,
    ) {
        let l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), false);
        let layout_set = Rc::new(Cell::new(false));
        let cbr = Rc::new(ComplexButtonResource::new(Box::new(MockOps {
            rx: 5,
            ry: 2,
            layout_set: layout_set.clone(),
            drawn: Rc::new(Cell::new(false)),
        })));
        let res: Rc<dyn Resource> = cbr.clone();
        let embed = l.add_embed_widget(&res);
        // Widget enfant dans l'arène partagée (≙ `Widget*` amont).
        let child = l.add_widget_node(WidgetKind::Text {
            text: Rc::from(&b"x"[..]),
        });
        cbr.init(&l, child);
        (l, cbr, embed, child, layout_set)
    }

    // ---- Embed ----

    /// `new Embed (resource)` — `resource->setEmbed(this)`
    /// verbatim (ui.cc:40).
    #[test]
    fn embed_creation_sets_embed() {
        let l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), false);
        let res = Rc::new(MockRes::new());
        let w = l.add_embed_widget(&(res.clone() as Rc<dyn Resource>));
        assert_eq!(res.embed(), Some(w));
        assert!(l.embed_resource(w).is_some());
        assert_eq!(l.embed_resource(w).and_then(|r| r.embed()), Some(w));
    }

    /// `Embed::sizeRequestSimpl` — `resource->sizeRequest` +
    /// correction (no-op sans style — verbatim viewport early
    /// return sur style `NULL`).
    #[test]
    fn embed_size_request_simpl_delegates() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), false);
        let res = Rc::new(MockRes::new());
        let res_dyn: Rc<dyn Resource> = res.clone();
        let w = l.add_embed_widget(&res_dyn);
        let mut req = Requisition::default();
        l.w_size_request_simpl(w, &mut req);
        assert_eq!((req.width, req.ascent, req.descent), (100, 20, 5));
    }

    /// `Embed::getExtremesSimpl` — `resource->getExtremes`
    /// (défaut → req.width) + `adjustmentWidth = min(intr, min)`.
    #[test]
    fn embed_get_extremes_simpl_delegates() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), false);
        let res = Rc::new(MockRes::new());
        let res_dyn: Rc<dyn Resource> = res.clone();
        let w = l.add_embed_widget(&res_dyn);
        let mut ext = Extremes::default();
        l.w_get_extremes_simpl(w, &mut ext);
        assert_eq!((ext.min_width, ext.max_width), (100, 100));
        assert_eq!(ext.adjustment_width, 100);
    }

    /// `Embed::buttonPressImpl` — bouton 3 → `emitClicked` +
    /// `handled` ; autre bouton → `false` (ui.cc:113).
    #[test]
    fn embed_button_press_clicked() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), false);
        let res = Rc::new(MockRes::new());
        let recv = Rc::new(RefCell::new(Recv::default()));
        let recv_dyn: Rc<RefCell<dyn ClickedReceiver>> = recv.clone();
        res.connect_clicked(&recv_dyn);
        let res_dyn: Rc<dyn Resource> = res.clone();
        let w = l.add_embed_widget(&res_dyn);
        let ev3 = EventButton {
            state: 0,
            x_canvas: 0,
            y_canvas: 0,
            x_widget: 0,
            y_widget: 0,
            num_pressed: 1,
            button: 3,
        };
        assert!(l.w_button_press_impl(w, &ev3));
        assert_eq!(recv.borrow().log.borrow().as_slice(), &["c"]);
        let ev1 = EventButton {
            state: 0,
            x_canvas: 0,
            y_canvas: 0,
            x_widget: 0,
            y_widget: 0,
            num_pressed: 1,
            button: 1,
        };
        assert!(!l.w_button_press_impl(w, &ev1));
        assert_eq!(recv.borrow().log.borrow().len(), 1);
    }

    /// `Embed::enterNotifyImpl`/`leaveNotifyImpl` —
    /// `emitEnter`/`emitLeave` (ui.cc:104-110).
    #[test]
    fn embed_enter_leave_emit() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), false);
        let res = Rc::new(MockRes::new());
        let recv = Rc::new(RefCell::new(Recv::default()));
        let recv_dyn: Rc<RefCell<dyn ActivateReceiver>> = recv.clone();
        res.connect_activate(&recv_dyn);
        let res_dyn: Rc<dyn Resource> = res.clone();
        let w = l.add_embed_widget(&res_dyn);
        let ev = EventCrossing {
            state: 0,
            last_widget: None,
            current_widget: Some(w),
        };
        l.w_enter_notify_impl(w, &ev);
        l.w_leave_notify_impl(w, &ev);
        assert_eq!(recv.borrow().log.borrow().as_slice(), &["e", "l"]);
    }

    /// `Embed::iterator` — `resource->iterator` ≙ `EmptyIterator`
    /// (ui.cc:137).
    #[test]
    fn embed_iterator_is_empty() {
        let l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), false);
        let res = Rc::new(MockRes::new());
        let res_dyn: Rc<dyn Resource> = res.clone();
        let w = l.add_embed_widget(&res_dyn);
        let mut it = l.w_iterator(w, content_type::SELECTION_CONTENT, false);
        assert!(!it.next(&l));
    }

    /// `Embed::setStyle` — `resource->setStyle` + style posé
    /// (ui.cc:142).
    #[test]
    fn embed_set_style_delegates() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), false);
        let res = Rc::new(MockRes::new());
        let w = l.add_embed_widget(&(res.clone() as Rc<dyn Resource>));
        let st = Style::create(&StyleAttrs::default());
        l.widget_set_style(w, &st);
        assert!(res.styled.get());
        assert!(l.node(w, |n| n.style.is_some()));
    }

    // ---- ComplexButtonResource ----

    /// `init` — verbatim : layout interne créé (arène partagée),
    /// `setLayout`, `setWidget`, quasiParent `embed` posé
    /// (ui.cc:343-355).
    #[test]
    fn complex_button_init_sets_quasi_parent() {
        let (l, cbr, embed, child, layout_set) = make_complex();
        assert_eq!(l.node(child, |n| n.quasi_parent), Some(embed));
        // `setLayout` appelé sur les ops.
        assert!(layout_set.get());
        assert!(cbr.layout.borrow().is_some());
    }

    /// `sizeRequest`/`getExtremes` — verbatim ui.cc:372-396 :
    /// enfant + `2*reliefX`/`reliefY`. Enfant `Text` sans impl
    /// → requisition `0` → résultat = relief pur.
    #[test]
    fn complex_button_size_request_relief() {
        let (mut l, cbr, embed, _child, _f) = make_complex();
        let mut req = Requisition::default();
        cbr.size_request(&mut l, &mut req);
        assert_eq!((req.width, req.ascent, req.descent), (10, 2, 2));
        let mut ext = Extremes::default();
        cbr.get_extremes(&mut l, &mut ext);
        assert_eq!((ext.min_width, ext.max_width), (10, 10));
        // Via le dispatch Embed (verbatim sizeRequestSimpl).
        let mut req2 = Requisition::default();
        l.w_size_request_simpl(embed, &mut req2);
        assert_eq!(req2.width, 10);
    }

    /// `LayoutReceiver::resizeQueued` — verbatim : relay vers
    /// `embed->queueResize (0, ec)` ≙ `pending_resizes` tagué
    /// `outer.id` (modèle D).
    #[test]
    fn complex_button_resize_relay() {
        let (l, cbr, embed, _child, _f) = make_complex();
        if let Some(inner) = cbr.layout.borrow_mut().as_mut() {
            inner.queue_resize(true);
        }
        let a = l.arena.borrow();
        assert!(a
            .pending_resizes
            .iter()
            .any(|(id, w, r, ec)| *id == l.id && *w == embed && *r == 0 && *ec));
    }

    /// `containerSizeChangedForChildren` — verbatim :
    /// `layout->containerSizeChanged()` (inner).
    #[test]
    fn complex_button_container_size_changed() {
        let (mut l, cbr, _embed, child, _f) = make_complex();
        // Enfant avec `width` en % → `affectedByContainerSizeChange`
        // → `queueResizeFast` interne (flags `RESIZE_QUEUED` lus
        // via l'arène partagée).
        let attrs = StyleAttrs {
            width: create_per_length(1.0),
            ..StyleAttrs::default()
        };
        l.widget_set_style(child, &Style::create(&attrs));
        cbr.container_size_changed_for_children(&mut l);
        assert!(l.node(child, |n| n.resize_queued() || n.needs_resize()));
    }

    /// `iterator` — `EmptyIterator` verbatim (`\bug` amont).
    #[test]
    fn complex_button_iterator_empty() {
        let (l, cbr, _e, _c, _f) = make_complex();
        let mut it = cbr.iterator(content_type::SELECTION_CONTENT, false);
        assert!(!it.next(&l));
    }

    /// `emitActivate`/`emitClicked` — chemins de signaux
    /// (ui.cc:150-300).
    #[test]
    fn resource_signals_emit() {
        let res = Rc::new(MockRes::new());
        let a = Rc::new(RefCell::new(Recv::default()));
        let c = Rc::new(RefCell::new(Recv::default()));
        let a_dyn: Rc<RefCell<dyn ActivateReceiver>> = a.clone();
        let c_dyn: Rc<RefCell<dyn ClickedReceiver>> = c.clone();
        res.connect_activate(&a_dyn);
        res.connect_clicked(&c_dyn);
        res.emit_activate();
        let ev = EventButton {
            state: 0,
            x_canvas: 0,
            y_canvas: 0,
            x_widget: 0,
            y_widget: 0,
            num_pressed: 1,
            button: 3,
        };
        res.emit_clicked(&ev);
        assert_eq!(a.borrow().log.borrow().as_slice(), &["a"]);
        assert_eq!(c.borrow().log.borrow().as_slice(), &["c"]);
    }
}
