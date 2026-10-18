//! Port de `dw/layout.hh` + `dw/layout.cc` — le `Layout` :
//! arène des widgets, vue attachée, file de resize, scroll,
//! ancres, fond, signaux et dispatch d'événements.
//!
//! Modèle ADR-0005 : les `Widget*` amont sont des
//! [`WidgetId`] dans `Arena::nodes` (`Rc<RefCell>` partagée —
//! les renderers externes y accèdent via `Weak`, ≙ pointeur
//! amont). `widget->layout == NULL` amont ≙
//! `Arena::detached`.
//!
//! Idle : amont `platform->addIdle(&Layout::foo)` enregistre
//! un pointeur de méthode ; le port passe par
//! [`Platform::add_idle`] (`IdleCallback` =
//! `Rc<dyn Fn(&mut Layout)>`) — la plateforme décide *quand*,
//! [`Layout::drain_idles`] collecte les callbacks dus et les
//! appelle avec `&mut self` (modèle D : pas de `&mut`
//! flottant).
//!
//! `selectionState` : portée (`crate::selection`). `findtextState` :
//! stub fidèle — `setWidget`/`resetSearch` existent, la recherche
//! (`NOT_FOUND`) arrive avec la phase « findtext ». `textZone`
//! (`ZoneAllocator`) : allocation interne amont — non portée,
//! sans effet observable.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use dillo_common::lout::signal::Emitter;

use crate::events::{EventButton, EventCrossing, EventMotion};
use crate::findtext::FindtextState;
use crate::imgrenderer::ImgRenderer;
use crate::platform::Platform;
use crate::selection::SelectionState;
use crate::style::{self, Color, Cursor, Length, StyleImage};
use crate::types::{
    Allocation, DrawingContext, GettingWidgetAtPointContext, HPosition, Rectangle, ScrollCommand,
    VPosition,
};
use crate::view::View;
use crate::widget::WidgetNode;
use crate::WidgetId;

/// `dw::core::ButtonState` — masque de bits amont → `i32`
/// (consts dans `events.rs` ≙ `BUTTON_PRESSED` etc.).
pub use crate::events::ButtonEventType;

/// `void (Layout::*func)()` amont — callback idle appelé par
/// [`Layout::drain_idles`] (≙ `misc::InstanceOf<Layout>`).
pub type IdleCallback = Rc<dyn Fn(&mut Layout)>;

/// `resizeIdle` : amont « Emergency layout stop after 1000
/// iterations » + « 100 iterations before redraw »
/// (layout.cc:885-899).
const RESIZE_ITER_LIMIT: usize = 100;
/// `resizeCounter` limite d'urgence.
const RESIZE_COUNTER_LIMIT: usize = 1000;

// ------------------------------------------------------------------
// Arena — stockage des nœuds widgets (modèle D).
// ------------------------------------------------------------------

/// Arène de widgets — les `Widget*` amont vivent ici.
///
/// `pending_draws` : zones `queueDrawArea` émises par les
/// renderers externes (qui n'ont pas `&mut Layout`) — vidées
/// vers `view.queue_draw` par [`Layout::flush_pending_draws`].
///
/// `pub` (ADR-0005) : les opérations « méthode de widget » amont
/// (`tb_add_text`, `tbl_add_cell`, `w_*`…) vivent sur `Arena` —
/// `dillo-html` les appelle via [`Layout::arena`]. Les champs
/// restent `pub(crate)` ; l'accès se fait uniquement par les
/// méthodes `pub`.
pub struct Arena {
    /// Nœuds — `nodes[0]` est une tombe sentinel (modèle D :
    /// `WidgetId` commence à 1, les ids ne sont pas réutilisés).
    pub(crate) nodes: Vec<WidgetNode>,
    /// Nœuds détachés (`widget->layout == NULL` amont).
    pub(crate) detached: HashSet<WidgetId>,
    /// Draws différés des renderers externes — coords **canvas
    /// figées à l'émission** (≙ `layout->queueDraw` immédiat amont :
    /// `queueDrawArea` résout `+ allocation` au moment de l'appel —
    /// différer la résolution décalait le damage quand le widget
    /// bougeait entre push et flush → vestiges, corrigé passe-D).
    /// Le premier champ tague le [`Layout`] émetteur (`id`) : une
    /// arène peut être partagée entre plusieurs layouts
    /// (`ui::ComplexButtonResource`), chacun ne draine que les siens.
    pub(crate) pending_draws: Vec<(usize, Rectangle)>,
    /// Resizes différés des renderers externes (≙
    /// `queueResize(ref, extremesChanged)` — ex. `Image::
    /// setBuffer`) — vidés vers `widget_queue_resize` par
    /// [`Layout::drain_idles`]. Même tag `Layout::id` que
    /// `pending_draws`.
    pub(crate) pending_resizes: Vec<(usize, WidgetId, i32, bool)>,
}

impl Arena {
    /// Arène vide (nœud 0 sentinel).
    fn new() -> Self {
        Arena {
            nodes: vec![WidgetNode::new(crate::widget::WidgetKind::Base)],
            detached: HashSet::new(),
            pending_draws: Vec::new(),
            pending_resizes: Vec::new(),
        }
    }

    /// `Embed::getResource` — lookup direct sur l'arène, sans passer
    /// par `Layout` : appelable pendant que `layout` est
    /// `borrow_mut` (dispatch d'événement widget → receiver, ex.
    /// formulaire `get_resource` ; A03-D7). `None` ≙ `NULL`.
    #[must_use]
    pub fn embed_resource(&self, w: WidgetId) -> Option<Rc<dyn crate::ui::Resource>> {
        match &self.nodes[w].kind {
            crate::widget::WidgetKind::Embed { res } => Some(res.clone()),
            _ => None,
        }
    }
}

// ------------------------------------------------------------------
// Anchor, receivers, stubs sélection/findtext.
// ------------------------------------------------------------------

/// `struct Anchor` (layout.hh:96) — clé = `name`.
#[derive(Debug)]
struct Anchor {
    /// `widget`.
    widget: WidgetId,
    /// `y` — position canvas (-1 = « pas encore alloué »).
    y: i32,
}

/// `Layout::Receiver` — verbatim layout.hh.
pub trait LayoutReceiver {
    /// `resizeQueued`.
    fn resize_queued(&mut self, _extremes_changed: bool) {}
    /// `canvasSizeChanged`.
    fn canvas_size_changed(&mut self, _width: i32, _ascent: i32, _descent: i32) {}
}

/// `LinkReceiver` (layout.hh) — `Widget*` amont →
/// `Option<WidgetId>` (`NULL` passé par `processMouseEvent`
/// quand aucun widget sensible ne consomme).
pub trait LinkReceiver {
    /// `enter`.
    fn enter(
        &mut self,
        _widget: Option<WidgetId>,
        _link: i32,
        _img: i32,
        _x: i32,
        _y: i32,
    ) -> bool {
        false
    }
    /// `pressed`.
    fn pressed(
        &mut self,
        _widget: Option<WidgetId>,
        _link: i32,
        _img: i32,
        _x: i32,
        _y: i32,
        _event: &EventButton,
    ) -> bool {
        false
    }
    /// `released`.
    fn released(
        &mut self,
        _widget: Option<WidgetId>,
        _link: i32,
        _img: i32,
        _x: i32,
        _y: i32,
        _event: &EventButton,
    ) -> bool {
        false
    }
    /// `clicked`.
    fn clicked(
        &mut self,
        _widget: Option<WidgetId>,
        _link: i32,
        _img: i32,
        _x: i32,
        _y: i32,
        _event: &EventButton,
    ) -> bool {
        false
    }
}

/// `FindtextState::Result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindtextResult {
    /// `SUCCESS`.
    Success,
    /// `RESTART`.
    Restart,
    /// `NOT_FOUND`.
    NotFound,
}

/// `SelectionState::EventType` (selection.hh:238).
#[derive(Debug, Clone, Copy)]
pub enum SelectionEvent {
    /// `BUTTON_PRESS`.
    ButtonPress,
    /// `BUTTON_RELEASE`.
    ButtonRelease,
    /// `BUTTON_MOTION`.
    ButtonMotion,
}

// ------------------------------------------------------------------
// Layout — champs verbatim layout.hh.
// ------------------------------------------------------------------

/// Identifiant unique de `Layout` (≙ identité du pointeur amont) —
/// tague les entrées `pending_draws`/`pending_resizes` quand une
/// arène est partagée entre plusieurs layouts
/// (`ui::ComplexButtonResource`).
static NEXT_LAYOUT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// `dw::core::Layout`.
#[allow(clippy::struct_excessive_bools)]
pub struct Layout {
    /// Identité du layout (modèle D — cf. `NEXT_LAYOUT_ID`).
    pub(crate) id: usize,
    /// `platform` (`Rc` ≙ pointeur possédé amont — `delete
    /// platform` dans `~Layout`).
    pub(crate) platform: Rc<RefCell<dyn Platform>>,
    /// `view`.
    view: Option<std::boxed::Box<dyn View>>,
    /// Arène des widgets.
    pub(crate) arena: Rc<RefCell<Arena>>,
    /// `topLevel`.
    pub(crate) top_level: Option<WidgetId>,
    /// `widgetAtPoint`.
    widget_at_point: Option<WidgetId>,
    /// `queueResizeList`.
    queue_resize_list: Vec<WidgetId>,
    /// `bgColor`.
    pub(crate) bg_color: Option<Color>,
    /// `bgImage`.
    bg_image: Option<Rc<RefCell<StyleImage>>>,
    /// `bgRepeat`/`bgAttachment`/`bgPosition{X,Y}`.
    bg_repeat: style::BackgroundRepeat,
    bg_attachment: style::BackgroundAttachment,
    bg_position_x: Length,
    bg_position_y: Length,
    /// `cursor`.
    cursor: Cursor,
    /// `canvasWidth`/`canvasAscent`/`canvasDescent`.
    canvas_width: i32,
    canvas_ascent: i32,
    canvas_descent: i32,
    /// `usesViewport`.
    uses_viewport: bool,
    /// `drawAfterScrollReq`.
    draw_after_scroll_req: bool,
    /// `scrollX`/`scrollY`.
    scroll_x: i32,
    scroll_y: i32,
    /// `viewportWidth`/`viewportHeight`.
    pub(crate) viewport_width: i32,
    pub(crate) viewport_height: i32,
    /// `canvasHeightGreater`.
    pub(crate) canvas_height_greater: bool,
    /// `hScrollbarThickness`/`vScrollbarThickness`.
    pub(crate) h_scrollbar_thickness: i32,
    pub(crate) v_scrollbar_thickness: i32,
    /// `scrollTarget{Hpos,Vpos,X,Y,Width,Height}`.
    scroll_target_hpos: HPosition,
    scroll_target_vpos: VPosition,
    scroll_target_x: i32,
    scroll_target_y: i32,
    scroll_target_width: i32,
    scroll_target_height: i32,
    /// `requestedAnchor`.
    requested_anchor: Option<Vec<u8>>,
    /// `scrollIdleId`/`resizeIdleId` — `-1` = aucun (verbatim).
    scroll_idle_id: i32,
    resize_idle_id: i32,
    /// `scrollIdleNotInterrupted`.
    scroll_idle_not_interrupted: bool,
    /// `anchorsTable`.
    anchors_table: HashMap<Vec<u8>, Anchor>,
    /// `selectionState`.
    selection_state: SelectionState,
    /// `findtextState`.
    findtext_state: FindtextState,
    /// `resizeIdleCounter`/`queueResizeCounter`/…/`resizeCounter`.
    resize_idle_counter: usize,
    pub(crate) queue_resize_counter: usize,
    pub(crate) size_allocate_counter: usize,
    pub(crate) size_request_counter: usize,
    pub(crate) get_extremes_counter: usize,
    resize_counter: usize,
    /// `resizeLimit`.
    resize_limit: bool,
    /// `Emitter<LayoutReceiver>` ≙ `Layout::Emitter` amont.
    emitter: Emitter<dyn LayoutReceiver>,
    /// `linkEmitter` — un seul récepteur amont (`connectLink`).
    link_receiver: Option<Rc<RefCell<dyn LinkReceiver>>>,
    /// `layoutImgRenderer` — partagé avec `bgImage` (nom de
    /// champ verbatim amont).
    #[allow(clippy::struct_field_names)]
    layout_img_renderer: Option<Rc<RefCell<LayoutImgRenderer>>>,
    /// `widgetImgRenderer` par widget (≙ champ amont).
    pub(crate) widget_img_renderers: HashMap<WidgetId, Rc<RefCell<dyn ImgRenderer>>>,
}

impl Layout {
    /// `Layout (platform, limit)` — verbatim layout.cc:270.
    #[must_use]
    pub fn new(platform: Rc<RefCell<dyn Platform>>, limit: bool) -> Self {
        Self::with_arena(Rc::new(RefCell::new(Arena::new())), platform, limit)
    }

    /// `Layout` partageant une arène existante — modèle D pour
    /// `ui::ComplexButtonResource` (le `childWidget` amont vit
    /// dans un `Layout` dédié mais référence le `Embed` externe
    /// par pointeur ; en arène partagée, `quasiParent` reste un
    /// `WidgetId` valide). `platform`/`view`/compteurs restent
    /// par-layout (verbatim).
    ///
    /// `pub(crate)` : `Arena` n'est pas exposée — la création de
    /// ressources complexes se fait via `ui::ComplexButtonResource`.
    #[must_use]
    pub(crate) fn with_arena(
        arena: Rc<RefCell<Arena>>,
        platform: Rc<RefCell<dyn Platform>>,
        limit: bool,
    ) -> Self {
        let layout = Layout {
            id: NEXT_LAYOUT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            platform,
            view: None,
            arena,
            top_level: None,
            widget_at_point: None,
            queue_resize_list: Vec::new(),
            bg_color: None,
            bg_image: None,
            bg_repeat: style::BackgroundRepeat::Repeat,
            bg_attachment: style::BackgroundAttachment::Scroll,
            bg_position_x: style::create_per_length(0.0),
            bg_position_y: style::create_per_length(0.0),
            cursor: Cursor::Default,
            canvas_width: 0,
            canvas_ascent: 0,
            canvas_descent: 0,
            uses_viewport: false,
            draw_after_scroll_req: false,
            scroll_x: 0,
            scroll_y: 0,
            viewport_width: 0,
            viewport_height: 0,
            canvas_height_greater: false,
            h_scrollbar_thickness: 0,
            v_scrollbar_thickness: 0,
            scroll_target_hpos: HPosition::Left,
            scroll_target_vpos: VPosition::Top,
            scroll_target_x: 0,
            scroll_target_y: 0,
            scroll_target_width: 0,
            scroll_target_height: 0,
            requested_anchor: None,
            scroll_idle_id: -1,
            resize_idle_id: -1,
            scroll_idle_not_interrupted: false,
            anchors_table: HashMap::new(),
            selection_state: SelectionState::new(),
            findtext_state: FindtextState::new(),
            resize_idle_counter: 0,
            queue_resize_counter: 0,
            size_allocate_counter: 0,
            size_request_counter: 0,
            get_extremes_counter: 0,
            resize_counter: 0,
            resize_limit: limit,
            emitter: Emitter::new(),
            link_receiver: None,
            layout_img_renderer: None,
            widget_img_renderers: HashMap::new(),
        };
        layout.platform.borrow_mut().set_layout(1);
        // `selectionState.setLayout` amont ≙ `layout` passé en
        // paramètre des méthodes (`crate::selection`).
        layout
    }

    /// Accès à l'arène partagée — ≙ les appels
    /// `((Textblock*)dw)->*` de `src/html.cc`/`form.cc` (modèle D :
    /// les widgets sont des nœuds, les opérations vivent sur
    /// `Arena`). Retourne le `Rc` pour que les receivers
    /// (`HtmlLinkReceiver` ≙ `html->linkReceiver`) puissent tenir
    /// un `Weak`.
    #[must_use]
    pub fn arena(&self) -> &Rc<RefCell<Arena>> {
        &self.arena
    }

    /// `Layout::getResourceFactory` — verbatim
    /// (`platform->getResourceFactory`).
    pub fn resource_factory(&mut self) -> Option<Rc<dyn crate::ui::ResourceFactory>> {
        self.platform.borrow_mut().get_resource_factory()
    }

    /// `new Widget` amont → nouveau nœud d'arène (id ≥ 1). Le nœud
    /// naît `detached` ≙ `widget->layout == NULL` (A05-D4) — levé
    /// par `add_widget`/`widget_set_parent`, reposé par
    /// `detach_widget`/`widget_delete`.
    #[must_use]
    pub fn add_widget_node(&self, kind: crate::widget::WidgetKind) -> WidgetId {
        let mut a = self.arena.borrow_mut();
        a.nodes.push(WidgetNode::new(kind));
        let w = a.nodes.len() - 1;
        a.detached.insert(w);
        w
    }

    /// `widget->layout == NULL` amont (≙ `detachWidget`).
    pub(crate) fn node_detached(&self, w: WidgetId) -> bool {
        self.arena.borrow().detached.contains(&w)
    }

    /// Enfile un nœud dans `queueResizeList`.
    pub(crate) fn queue_resize_list_put(&mut self, w: WidgetId) {
        self.queue_resize_list.push(w);
    }

    /// Compteurs verbatim — `Widget::{enter,leave}X` amont sont
    /// `if (layout) layout->xCounter±±` ; l'arène est toujours
    /// dans le layout.
    pub(crate) fn enter_queue_resize(&mut self) {
        self.queue_resize_counter += 1;
    }
    /// `leaveQueueResize`.
    pub(crate) fn leave_queue_resize(&mut self) {
        self.queue_resize_counter = self.queue_resize_counter.saturating_sub(1);
    }
    /// `enterSizeRequest`.
    pub(crate) fn enter_size_request(&mut self) {
        self.size_request_counter += 1;
    }
    /// `leaveSizeRequest`.
    pub(crate) fn leave_size_request(&mut self) {
        self.size_request_counter = self.size_request_counter.saturating_sub(1);
    }
    /// `enterGetExtremes`.
    pub(crate) fn enter_get_extremes(&mut self) {
        self.get_extremes_counter += 1;
    }
    /// `leaveGetExtremes`.
    pub(crate) fn leave_get_extremes(&mut self) {
        self.get_extremes_counter = self.get_extremes_counter.saturating_sub(1);
    }
    /// `enterSizeAllocate`.
    pub(crate) fn enter_size_allocate(&mut self) {
        self.size_allocate_counter += 1;
    }
    /// `leaveSizeAllocate`.
    pub(crate) fn leave_size_allocate(&mut self) {
        self.size_allocate_counter = self.size_allocate_counter.saturating_sub(1);
    }
    /// `resizeIdleEntered`.
    #[must_use]
    pub fn resize_idle_entered(&self) -> bool {
        self.resize_idle_counter != 0
    }
    /// `enterResizeIdle`.
    fn enter_resize_idle(&mut self) {
        self.resize_idle_counter += 1;
    }
    /// `leaveResizeIdle`.
    fn leave_resize_idle(&mut self) {
        self.resize_idle_counter = self.resize_idle_counter.saturating_sub(1);
    }

    /// Références de fond pour `drawBackground`/`drawBox` —
    /// `(bgColor, bgImage)`.
    pub(crate) fn layout_bg_refs(&self) -> (Option<Color>, Option<Rc<RefCell<StyleImage>>>) {
        (self.bg_color, self.bg_image.clone())
    }

    /// Vide `pending_draws` vers `view->queueDraw` — les
    /// renderers externes n'ont pas `&mut Layout`. Ne consomme
    /// que les entrées taguées `self.id` (arène partageable) ;
    /// les autres restent en file pour leur propre layout.
    pub fn flush_pending_draws(&mut self) {
        let draws = {
            let mut a = self.arena.borrow_mut();
            let (mine, rest): (Vec<_>, Vec<_>) =
                a.pending_draws.drain(..).partition(|e| e.0 == self.id);
            a.pending_draws = rest;
            mine
        };
        for (_, rect) in draws {
            self.queue_draw(rect.x, rect.y, rect.width, rect.height);
        }
    }

    // ---- attachView / detachView (layout.cc:461-527) ----

    /// `Layout::attachView` — verbatim : `platform->attachView`,
    /// projection de l'état (bg, curseur, canvas), initialisation
    /// du viewport et `view->setLayout(this)` en dernier.
    pub fn attach_view(&mut self, mut view: std::boxed::Box<dyn View>) {
        self.platform.borrow_mut().attach_view(view.as_mut());
        if let Some(c) = &self.bg_color {
            view.set_bg_color(c);
        }
        view.set_cursor(self.cursor);
        view.set_canvas_size(self.canvas_width, self.canvas_ascent, self.canvas_descent);

        if view.uses_viewport() {
            if self.uses_viewport {
                view.scroll_to(self.scroll_x, self.scroll_y);
                view.set_viewport_size(
                    self.viewport_width,
                    self.viewport_height,
                    self.h_scrollbar_thickness,
                    self.v_scrollbar_thickness,
                );
                self.h_scrollbar_thickness = self
                    .h_scrollbar_thickness
                    .max(view.get_h_scrollbar_thickness());
                self.v_scrollbar_thickness = self
                    .v_scrollbar_thickness
                    .max(view.get_v_scrollbar_thickness());
            } else {
                self.uses_viewport = true;
                self.scroll_x = 0;
                self.scroll_y = 0;
                // « random values » verbatim.
                self.viewport_width = 100;
                self.viewport_height = 100;
                self.h_scrollbar_thickness = view.get_h_scrollbar_thickness();
                self.v_scrollbar_thickness = view.get_v_scrollbar_thickness();
            }
        }

        view.set_layout(Some(1));
        self.view = Some(view);
    }

    /// `Layout::detachView` — verbatim : `setLayout(NULL)`,
    /// `platform->detachView`, `view = NULL`.
    pub fn detach_view(&mut self) {
        if let Some(mut v) = self.view.take() {
            v.set_layout(None);
            self.platform.borrow_mut().detach_view(v.as_mut());
        }
    }

    /// `Layout::hasEvent` — `view != NULL` (inline amont).
    #[must_use]
    pub fn has_event(&self) -> bool {
        self.view.is_some()
    }

    /// `Layout::getTopLevel`.
    #[must_use]
    pub fn get_top_level(&self) -> Option<WidgetId> {
        self.top_level
    }

    /// `getUsesViewport`.
    #[must_use]
    pub fn get_uses_viewport(&self) -> bool {
        self.uses_viewport
    }
    /// `getScrollPosX`.
    #[must_use]
    pub fn get_scroll_pos_x(&self) -> i32 {
        self.scroll_x
    }
    /// `getScrollPosY`.
    #[must_use]
    pub fn get_scroll_pos_y(&self) -> i32 {
        self.scroll_y
    }
    /// `getWidthViewport`.
    #[must_use]
    pub fn get_width_viewport(&self) -> i32 {
        self.viewport_width
    }
    /// `getHeightViewport`.
    #[must_use]
    pub fn get_height_viewport(&self) -> i32 {
        self.viewport_height
    }

    // ---- proxies platform (layout.hh:395-422 inlines amont) ----

    /// `dpiX` — `platform->dpiX()` verbatim.
    pub fn dpi_x(&mut self) -> f64 {
        self.platform.borrow_mut().dpi_x()
    }

    /// `textWidth` — `platform->textWidth` verbatim
    /// (layout.hh:370). Mesure `text` dans la fonte donnée.
    pub fn text_width(&mut self, font: &crate::style::Font, text: &[u8]) -> i32 {
        self.platform.borrow_mut().text_width(font, text)
    }

    /// `createFont` — `platform->createFont` verbatim
    /// (`try_everything` ≙ fallback agressif).
    pub fn create_font(
        &mut self,
        attrs: &crate::style::FontAttrs,
        try_everything: bool,
    ) -> std::rc::Rc<crate::style::Font> {
        self.platform
            .borrow_mut()
            .create_font(attrs, try_everything)
    }

    /// `fontExists` — `platform->fontExists` verbatim.
    pub fn font_exists(&mut self, name: &[u8]) -> bool {
        self.platform.borrow_mut().font_exists(name)
    }

    /// `createColor` — `platform->createColor` verbatim.
    pub fn create_color(&mut self, color: i32) -> crate::style::Color {
        self.platform.borrow_mut().create_color(color)
    }

    /// `createTooltip` — `platform->createTooltip` verbatim.
    pub fn create_tooltip(&mut self, text: &[u8]) -> std::rc::Rc<crate::style::Tooltip> {
        self.platform.borrow_mut().create_tooltip(text)
    }

    // ---- widgets top-level (layout.cc:351-458) ----

    /// `Layout::detachWidget` — verbatim : marque `detached`
    /// pour le sous-arbre (récursion `WIDGET_IN_FLOW |
    /// WIDGET_OOF_CONT` amont — seam
    /// `widget_children_for_container_change`).
    pub fn detach_widget(&mut self, widget: WidgetId) {
        let mut stack = vec![widget];
        while let Some(w) = stack.pop() {
            self.arena.borrow_mut().detached.insert(w);
            for c in self.widget_children_for_detach(w) {
                stack.push(c);
            }
        }
    }

    /// `Layout::addWidget` — verbatim layout.cc:369 : warn si
    /// topLevel déjà posé, bloc SCM (`IMPL_POS` false → inactif),
    /// `topLevel`, `layout`, `container=NULL`,
    /// `queueResizeList->clear()`, `notifySetAsTopLevel`,
    /// `findtextState.setWidget`.
    pub fn add_widget(&mut self, widget: WidgetId) {
        if self.top_level.is_some() {
            // MSG_WARN amont — pas d'effet.
            return;
        }
        // Bloc `IMPL_POS` — inactif verbatim (phase OOF).
        self.top_level = Some(widget);
        self.arena.borrow_mut().detached.remove(&widget);
        self.node_mut(widget, |n| n.container = None);
        self.queue_resize_list.clear();
        self.w_notify_set_as_top_level(widget);
        self.findtext_state.set_widget(Some(widget));
        // Verbatim fin de `addWidget` : reset puis `queueResize` du
        // top level (remplit `queueResizeList` + arme l'idle —
        // A05-D2).
        self.canvas_height_greater = false;
        self.widget_queue_resize(widget, -1, false);
    }

    /// `Layout::setWidget` — verbatim layout.cc:438 : supprime
    /// l'ancien topLevel (≙ `delete` — callbacks + sous-arbre
    /// `detached`), `textZone->zoneFree` (non porté : alloc),
    /// `addWidget`, `updateCursor`, `resizeCounter = 0`.
    pub fn set_widget(&mut self, widget: WidgetId) {
        self.widget_at_point = None;
        if let Some(old) = self.top_level {
            self.top_level = None;
            self.widget_delete_subtree(old);
        }
        self.add_widget(widget);
        self.update_cursor();
        self.resize_counter = 0;
    }

    /// `Layout::removeWidget` — verbatim layout.cc:412 (la
    /// boucle en amont du corps est `topLevel->queueResize` —
    /// voir [`Self::queue_resize_top_level`]).
    pub fn remove_widget(&mut self) {
        self.top_level = None;
        self.queue_resize_list.clear();
        self.widget_at_point = None;
        self.canvas_width = 0;
        self.canvas_ascent = 0;
        self.canvas_descent = 0;
        self.scroll_x = 0;
        self.scroll_y = 0;

        if let Some(v) = &mut self.view {
            v.set_canvas_size(0, 0, 0);
            if v.uses_viewport() {
                v.set_viewport_size(self.viewport_width, self.viewport_height, 0, 0);
            }
            v.queue_draw_total();
        }

        self.set_anchor(None);
        self.update_anchor();

        let (w, a, d) = (self.canvas_width, self.canvas_ascent, self.canvas_descent);
        self.emitter.emit_void(|r| r.canvas_size_changed(w, a, d));

        self.findtext_state.set_widget(None);
        self.selection_state.reset();

        self.update_cursor();
    }

    /// `topLevel->queueResize (-1, false)` — verbatim du bloc
    /// précédant `removeWidget` (layout.cc:405) — appelé par
    /// `~Layout`/`set_widget` quand le top level disparaît.
    pub fn queue_resize_top_level(&mut self) {
        if let Some(t) = self.top_level {
            self.widget_queue_resize(t, -1, false);
        }
    }

    // ---- draw / curseur (layout.cc:670-869) ----

    /// `Layout::draw` — verbatim : image de fond d'abord, puis
    /// widget top-level (draw différé si scroll pendant).
    pub fn draw(&mut self, view: &mut dyn View, area: &Rectangle) {
        let bg_image = self.bg_image.clone();
        if let Some(bi) = &bg_image {
            if bi.borrow().get_imgbuf_src().is_some() {
                style::draw_background_image(
                    view,
                    bi,
                    self.bg_repeat,
                    self.bg_attachment,
                    self.bg_position_x,
                    self.bg_position_y,
                    area.x,
                    area.y,
                    area.width,
                    area.height,
                    0,
                    0,
                    // zone de référence : max(canvas, viewport).
                    (self.viewport_width
                        - if self.canvas_height_greater {
                            self.v_scrollbar_thickness
                        } else {
                            0
                        })
                    .max(self.canvas_width),
                    (self.viewport_height - self.h_scrollbar_thickness)
                        .max(self.canvas_ascent + self.canvas_descent),
                );
            }
        }

        if self.scroll_idle_id != -1 {
            // scroll pendant → draw différé après scrollIdle. Le
            // damage reste marqué : chez FLTK `damage()` persiste
            // jusqu'au prochain expose ; ici `take_damage` consomme —
            // re-queuer l'aire pour ne pas perdre la passe (canvas
            // blanc sinon si le scroll ne bouge pas →
            // `scrollIdle` ne ré-émet pas `queueDrawTotal`).
            self.draw_after_scroll_req = true;
            view.queue_draw(area);
        } else if let Some(top) = self.top_level {
            let wa = self.node(top, |n| Allocation {
                width: n.allocation.width,
                ascent: n.allocation.ascent,
                descent: n.allocation.descent,
                ..n.allocation
            });
            let widget_area = Rectangle {
                x: wa.x,
                y: wa.y,
                width: wa.width,
                height: wa.ascent + wa.descent,
            };
            if let Some(intersection) = area.intersects_with(&widget_area) {
                view.start_drawing(&intersection);
                let widget_draw_area = Rectangle {
                    x: intersection.x - wa.x,
                    y: intersection.y - wa.y,
                    width: intersection.width,
                    height: intersection.height,
                };
                let mut context = DrawingContext::new(widget_area);
                self.w_draw(top, view, &widget_draw_area, &mut context);
                view.finish_drawing(&intersection);
            }
        }
    }

    /// `Layout::currHScrollbarThickness` (inline amont).
    #[must_use]
    pub fn curr_h_scrollbar_thickness(&self) -> i32 {
        if self.canvas_width > self.viewport_width {
            self.h_scrollbar_thickness
        } else {
            0
        }
    }

    /// `Layout::currVScrollbarThickness`.
    #[must_use]
    pub fn curr_v_scrollbar_thickness(&self) -> i32 {
        if self.canvas_ascent + self.canvas_descent > self.viewport_height {
            self.v_scrollbar_thickness
        } else {
            0
        }
    }

    /// `Layout::setAnchor` — verbatim.
    pub fn set_anchor(&mut self, anchor: Option<&[u8]>) {
        self.requested_anchor = anchor.map(<[u8]>::to_vec);
        self.update_anchor();
    }

    /// `Layout::addAnchor` — verbatim (y = -1 ≙ non alloué ;
    /// retourne le nom ≙ `anchor->name` amont, `None` si la clé
    /// existe déjà).
    pub fn add_anchor(&mut self, widget: WidgetId, name: &[u8], y: i32) -> Option<Vec<u8>> {
        if self.anchors_table.contains_key(name) {
            None
        } else {
            self.anchors_table
                .insert(name.to_vec(), Anchor { widget, y });
            self.update_anchor();
            Some(name.to_vec())
        }
    }

    /// `Layout::changeAnchor` — verbatim (asserts amont →
    /// no-op si absent — fidélité : la panique amont est un
    /// `assert` de debug, le port reste sain).
    pub fn change_anchor(&mut self, widget: WidgetId, name: &[u8], y: i32) {
        if let Some(a) = self.anchors_table.get_mut(name) {
            if a.widget == widget {
                a.y = y;
            }
        }
        self.update_anchor();
    }

    /// `Layout::removeAnchor`.
    pub fn remove_anchor(&mut self, name: &[u8]) {
        self.anchors_table.remove(name);
    }

    /// `Layout::updateAnchor` — verbatim layout.cc:791.
    fn update_anchor(&mut self) {
        let anchor_y = self
            .requested_anchor
            .as_ref()
            .and_then(|name| self.anchors_table.get(name).map(|a| a.y));
        match anchor_y {
            None => {
                if self.scroll_idle_id != -1 && !self.scroll_idle_not_interrupted {
                    self.platform.borrow_mut().remove_idle(self.scroll_idle_id);
                    self.scroll_idle_id = -1;
                }
            }
            Some(y) if y != -1 => {
                self.scroll_to0(HPosition::NoChange, VPosition::Top, 0, y, 0, 0, false);
            }
            _ => {}
        }
    }

    /// `Layout::setCursor` — verbatim (view->setCursor non
    /// conditionnel amont ; `has_view` garde le port sain quand
    /// aucune vue n'est attachée).
    pub fn set_cursor(&mut self, cursor: Cursor) {
        if cursor != self.cursor {
            self.cursor = cursor;
            if let Some(v) = &mut self.view {
                v.set_cursor(cursor);
            }
        }
    }

    /// `Layout::updateCursor` — verbatim : curseur du style du
    /// `widgetAtPoint` sinon `CURSOR_DEFAULT`.
    pub fn update_cursor(&mut self) {
        let cursor = self
            .widget_at_point
            .and_then(|w| self.node(w, |n| n.style.clone()))
            .map_or(Cursor::Default, |s| s.attrs.cursor);
        self.set_cursor(cursor);
    }

    /// `Layout::setBgColor` — verbatim (`ref`/`unref` ≙ copie
    /// de la valeur `Color` du port).
    pub fn set_bg_color(&mut self, color: Option<Color>) {
        self.bg_color = color;
        if let Some(v) = &mut self.view {
            if let Some(c) = &self.bg_color {
                v.set_bg_color(c);
            }
        }
    }

    /// `Layout::setBgImage` — verbatim layout.cc:837 :
    /// `removeExternalImgRenderer` ancien, retenir le nouveau,
    /// renderer partagé créé une fois (`layoutImgRenderer` lié au
    /// layout, pas au style).
    pub fn set_bg_image(
        &mut self,
        bg_image: Option<Rc<RefCell<StyleImage>>>,
        bg_repeat: style::BackgroundRepeat,
        bg_attachment: style::BackgroundAttachment,
        bg_position_x: Length,
        bg_position_y: Length,
    ) {
        if let Some(r) = &self.layout_img_renderer {
            if let Some(old) = &self.bg_image {
                let r: Rc<RefCell<dyn ImgRenderer>> = r.clone();
                old.borrow().remove_external_img_renderer(&r);
            }
        }
        self.bg_image = bg_image;
        self.bg_repeat = bg_repeat;
        self.bg_attachment = bg_attachment;
        self.bg_position_x = bg_position_x;
        self.bg_position_y = bg_position_y;
        if let Some(new) = self.bg_image.clone() {
            if self.layout_img_renderer.is_none() {
                let arena = Rc::downgrade(&self.arena);
                self.layout_img_renderer = Some(Rc::new(RefCell::new(LayoutImgRenderer {
                    canvas_width: 0,
                    canvas_height: 0,
                    viewport_width: 0,
                    viewport_height: 0,
                    canvas_height_greater: false,
                    h_scrollbar_thickness: 0,
                    v_scrollbar_thickness: 0,
                    bg_image: Rc::downgrade(&new),
                    bg_repeat: self.bg_repeat,
                    bg_attachment: self.bg_attachment,
                    bg_position_x: self.bg_position_x,
                    bg_position_y: self.bg_position_y,
                    layout_id: self.id,
                    pending_draws: arena,
                })));
            }
            self.sync_layout_img_renderer();
            if let Some(r) = &self.layout_img_renderer {
                let r: Rc<RefCell<dyn ImgRenderer>> = r.clone();
                new.borrow().put_external_img_renderer(&r);
            }
        }
    }

    /// Synchronise l'état du `LayoutImgRenderer` avec le layout
    /// (≙ `renderer->layout` amont — le renderer ne tient pas
    /// `&mut Layout`).
    pub(crate) fn sync_layout_img_renderer(&mut self) {
        if let Some(r) = &self.layout_img_renderer {
            let mut r = r.borrow_mut();
            r.canvas_width = self.canvas_width;
            r.canvas_height = self.canvas_ascent + self.canvas_descent;
            r.viewport_width = self.viewport_width;
            r.viewport_height = self.viewport_height;
            r.canvas_height_greater = self.canvas_height_greater;
            r.h_scrollbar_thickness = self.h_scrollbar_thickness;
            r.v_scrollbar_thickness = self.v_scrollbar_thickness;
            r.bg_repeat = self.bg_repeat;
            r.bg_attachment = self.bg_attachment;
            r.bg_position_x = self.bg_position_x;
            r.bg_position_y = self.bg_position_y;
            r.bg_image = self
                .bg_image
                .as_ref()
                .map_or(std::rc::Weak::new(), Rc::downgrade);
        }
    }

    // ---- resize machinery (layout.cc:870-1047) ----

    /// `Layout::queueDraw` — verbatim (zone vide ignorée).
    pub fn queue_draw(&mut self, x: i32, y: i32, width: i32, height: i32) {
        let area = Rectangle {
            x,
            y,
            width,
            height,
        };
        if area.is_empty() {
            return;
        }
        if let Some(v) = &mut self.view {
            v.queue_draw(&area);
        }
    }

    /// `Layout::queueDrawExcept` — verbatim layout.cc:994 :
    /// les quatre rectangles autour de `excl`.
    #[allow(clippy::too_many_arguments)]
    pub fn queue_draw_except(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        ex: i32,
        ey: i32,
        ewidth: i32,
        eheight: i32,
    ) {
        if x == ex && y == ey && width == ewidth && height == eheight {
            return;
        }
        let ix1 = x.max(ex);
        let iy1 = y.max(ey);
        let ix2 = (x + width).min(ex + ewidth);
        let iy2 = (y + height).min(ey + eheight);

        self.queue_draw(x, y, width, iy1 - y);
        self.queue_draw(x, iy2, width, y + height - iy2);
        self.queue_draw(x, iy1, ix1 - x, iy2 - iy1);
        self.queue_draw(ix2, iy1, x + width - ix2, iy2 - iy1);
    }

    /// `Layout::queueResize` — verbatim : `cancelQueueDraw` +
    /// idle + `emitResizeQueued` (émission *même* si l'idle
    /// existait déjà — verbatim).
    pub fn queue_resize(&mut self, extremes_changed: bool) {
        if self.resize_idle_id == -1 {
            if let Some(v) = &mut self.view {
                v.cancel_queue_draw();
            }
            self.resize_idle_id = self
                .platform
                .borrow_mut()
                .add_idle(self.id, Rc::new(Layout::resize_idle));
        }
        self.emitter
            .emit_void(|r| r.resize_queued(extremes_changed));
    }

    /// `Layout::resizeIdle` — verbatim layout.cc:870 : boucle
    /// tant que `resizeIdleId != -1`, limites 100/1000,
    /// conversion QUEUED→NEEDS, resize du top level, canvas +
    /// viewport, `updateAnchor`.
    fn resize_idle(&mut self) {
        self.enter_resize_idle();

        let mut iteration = 0usize;
        while self.resize_idle_id != -1 {
            if self.resize_limit && self.resize_counter >= RESIZE_COUNTER_LIMIT {
                // « Emergency layout stop » amont.
                self.resize_idle_id = -1;
                break;
            }
            if iteration >= RESIZE_ITER_LIMIT {
                // « Stopping layout loop » amont — retour redraw.
                break;
            }
            iteration += 1;
            self.resize_counter += 1;

            let queued = self.queue_resize_list.clone();
            for widget_id in queued {
                self.node_mut(widget_id, |n| {
                    if n.resize_queued() {
                        n.set_flags(crate::widget::NEEDS_RESIZE);
                        n.unset_flags(crate::widget::RESIZE_QUEUED);
                    }
                    if n.allocate_queued() {
                        n.set_flags(crate::widget::NEEDS_ALLOCATE);
                        n.unset_flags(crate::widget::ALLOCATE_QUEUED);
                    }
                    if n.extremes_queued() {
                        n.set_flags(crate::widget::EXTREMES_CHANGED);
                        n.unset_flags(crate::widget::EXTREMES_QUEUED);
                    }
                });
            }
            self.queue_resize_list.clear();

            // reset ici — queueResize peut être rappelé plus bas.
            self.resize_idle_id = -1;

            let top = self.top_level;
            let needs = top.is_some_and(|top_id| {
                self.node(top_id, |n| n.needs_resize() || n.needs_allocate())
            });
            if needs {
                let top_id = top.unwrap_or(0);
                let mut requisition = crate::types::Requisition::default();
                self.widget_size_request(top_id, &mut requisition, 0, &[], &[], &[]);

                let mut allocation = Allocation {
                    x: 0,
                    y: 0,
                    width: requisition.width,
                    ascent: requisition.ascent,
                    descent: requisition.descent,
                };
                let scrollbar_on_left = self
                    .view
                    .as_mut()
                    .is_some_and(|v| v.uses_viewport() && v.get_scrollbar_on_left() != 0);
                if self.uses_viewport && scrollbar_on_left {
                    allocation.x += self.curr_v_scrollbar_thickness();
                }
                self.widget_size_allocate(top_id, &allocation);

                self.canvas_width = requisition.width;
                self.canvas_ascent = requisition.ascent;
                self.canvas_descent = requisition.descent;
                let canvas = (self.canvas_width, self.canvas_ascent, self.canvas_descent);
                self.emitter
                    .emit_void(|r| r.canvas_size_changed(canvas.0, canvas.1, canvas.2));
                if let Some(v) = &mut self.view {
                    v.set_canvas_size(canvas.0, canvas.1, canvas.2);
                }

                if self.uses_viewport {
                    let curr_h = self.curr_h_scrollbar_thickness();
                    let curr_v = self.curr_v_scrollbar_thickness();
                    if !self.canvas_height_greater
                        && self.canvas_ascent + self.canvas_descent > self.viewport_height - curr_h
                    {
                        self.canvas_height_greater = true;
                        self.container_size_changed();
                        if scrollbar_on_left {
                            self.node_mut(top_id, |n| {
                                n.set_flags(crate::widget::NEEDS_ALLOCATE);
                            });
                        }
                    }
                    if let Some(v) = &mut self.view {
                        v.set_viewport_size(
                            self.viewport_width,
                            self.viewport_height,
                            curr_h,
                            curr_v,
                        );
                    }
                }
                self.sync_layout_img_renderer();
            }
        }
        self.update_anchor();

        self.leave_resize_idle();
    }

    /// `Layout::drain_idles` — drain des callbacks plateforme
    /// dus (≙ boucle événementielle toolkit amont : la
    /// plateforme *décide* des callbacks dus ; le port les
    /// collecte puis les appelle sur `self`).
    pub fn drain_idles(&mut self) {
        let cbs = self.platform.borrow_mut().take_pending_idles(self.id);
        for cb in cbs {
            cb(self);
        }
        // Resizes différés par les renderers externes (≙
        // `queueResize` amont — différé, modèle D) — tagués
        // `self.id` comme `pending_draws`.
        let resizes = {
            let mut a = self.arena.borrow_mut();
            let (mine, rest): (Vec<_>, Vec<_>) =
                a.pending_resizes.drain(..).partition(|e| e.0 == self.id);
            a.pending_resizes = rest;
            mine
        };
        for (_, w, ref_, extremes_changed) in resizes {
            self.widget_queue_resize(w, ref_, extremes_changed);
        }
        self.flush_pending_draws();
    }

    // ---- scroll (layout.cc:529-650) ----

    /// `Layout::scroll` — verbatim.
    pub fn scroll(&mut self, cmd: ScrollCommand) {
        if let Some(v) = &mut self.view {
            if v.uses_viewport() {
                v.scroll(cmd);
            }
        }
    }

    /// `Layout::scrollTo` — verbatim (`scrollingInterrupted =
    /// true`).
    pub fn scroll_to(
        &mut self,
        hpos: HPosition,
        vpos: VPosition,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        self.scroll_to0(hpos, vpos, x, y, width, height, true);
    }

    /// `Layout::scrollTo0` — verbatim layout.cc:545.
    #[allow(clippy::too_many_arguments)]
    fn scroll_to0(
        &mut self,
        hpos: HPosition,
        vpos: VPosition,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        scrolling_interrupted: bool,
    ) {
        if self.uses_viewport {
            self.scroll_target_hpos = hpos;
            self.scroll_target_vpos = vpos;
            self.scroll_target_x = x;
            self.scroll_target_y = y;
            self.scroll_target_width = width;
            self.scroll_target_height = height;
            if self.scroll_idle_id == -1 {
                self.scroll_idle_id = self
                    .platform
                    .borrow_mut()
                    .add_idle(self.id, Rc::new(Layout::scroll_idle));
                self.scroll_idle_not_interrupted = true;
            }
            self.scroll_idle_not_interrupted =
                self.scroll_idle_not_interrupted || !scrolling_interrupted;
        }
    }

    /// `Layout::scrollIdle` — verbatim layout.cc:570.
    fn scroll_idle(&mut self) {
        let mut x_changed = true;
        match self.scroll_target_hpos {
            HPosition::Left => self.scroll_x = self.scroll_target_x,
            HPosition::Center => {
                self.scroll_x = self.scroll_target_x
                    - (self.viewport_width
                        - self.curr_v_scrollbar_thickness()
                        - self.scroll_target_width)
                        / 2;
            }
            HPosition::Right => {
                self.scroll_x = self.scroll_target_x
                    - (self.viewport_width
                        - self.curr_v_scrollbar_thickness()
                        - self.scroll_target_width);
            }
            HPosition::IntoView => {
                let vp = self.viewport_width - self.curr_v_scrollbar_thickness();
                x_changed = Self::calc_scroll_into(
                    self.scroll_target_x,
                    self.scroll_target_width,
                    &mut self.scroll_x,
                    vp,
                );
            }
            HPosition::NoChange => x_changed = false,
        }

        let mut y_changed = true;
        match self.scroll_target_vpos {
            VPosition::Top => self.scroll_y = self.scroll_target_y,
            VPosition::Center => {
                self.scroll_y = self.scroll_target_y
                    - (self.viewport_height
                        - self.curr_h_scrollbar_thickness()
                        - self.scroll_target_height)
                        / 2;
            }
            VPosition::Bottom => {
                self.scroll_y = self.scroll_target_y
                    - (self.viewport_height
                        - self.curr_h_scrollbar_thickness()
                        - self.scroll_target_height);
            }
            VPosition::IntoView => {
                let vp = self.viewport_height - self.curr_h_scrollbar_thickness();
                y_changed = Self::calc_scroll_into(
                    self.scroll_target_y,
                    self.scroll_target_height,
                    &mut self.scroll_y,
                    vp,
                );
            }
            VPosition::NoChange => y_changed = false,
        }

        if x_changed || y_changed {
            self.adjust_scroll_pos();
            if let Some(v) = &mut self.view {
                v.scroll_to(self.scroll_x, self.scroll_y);
                if self.draw_after_scroll_req {
                    self.draw_after_scroll_req = false;
                    v.queue_draw_total();
                }
            }
        }

        self.scroll_idle_id = -1;
    }

    /// `Layout::adjustScrollPos` — verbatim.
    fn adjust_scroll_pos(&mut self) {
        self.scroll_x = self
            .scroll_x
            .min(self.canvas_width - (self.viewport_width - self.v_scrollbar_thickness));
        self.scroll_x = self.scroll_x.max(0);
        self.scroll_y = self.scroll_y.min(
            self.canvas_ascent + self.canvas_descent
                - (self.viewport_height - self.h_scrollbar_thickness),
        );
        self.scroll_y = self.scroll_y.max(0);
    }

    /// `Layout::calcScrollInto` — verbatim layout.cc:635
    /// (statique amont).
    fn calc_scroll_into(
        requested_value: i32,
        mut requested_size: i32,
        value: &mut i32,
        viewport_size: i32,
    ) -> bool {
        if requested_size > viewport_size {
            // Viewport plus petit que la région — visible → ne
            // rien changer, sinon montrer le bord haut/gauche.
            if *value >= requested_value
                && *value + viewport_size < requested_value + requested_size
            {
                return false;
            }
            requested_size = viewport_size;
        }
        if requested_value < *value {
            *value = requested_value;
            true
        } else if requested_value + requested_size > *value + viewport_size {
            *value = requested_value - viewport_size + requested_size;
            true
        } else {
            false
        }
    }

    // ---- events (layout.cc:1052-1357) ----

    /// `Layout::buttonEvent` — verbatim.
    #[allow(clippy::too_many_arguments)]
    pub fn button_event(
        &mut self,
        r#type: ButtonEventType,
        num_pressed: i32,
        x: i32,
        y: i32,
        state: i32,
        button: i32,
    ) -> bool {
        self.move_to_widget_at_point(x, y, state);
        let mut event = EventButton {
            x_canvas: x,
            y_canvas: y,
            state,
            button,
            num_pressed,
            x_widget: 0,
            y_widget: 0,
        };
        self.process_mouse_event_button(&mut event, r#type)
    }

    /// `Layout::motionNotify` — verbatim.
    pub fn motion_notify(&mut self, x: i32, y: i32, state: i32) -> bool {
        self.move_to_widget_at_point(x, y, state);
        let mut event = EventMotion {
            x_canvas: x,
            y_canvas: y,
            state,
            x_widget: 0,
            y_widget: 0,
        };
        self.process_mouse_event_motion(&mut event)
    }

    /// `Layout::enterNotify` — verbatim.
    pub fn enter_notify(&mut self, x: i32, y: i32, state: i32) {
        let last = self.widget_at_point;
        self.move_to_widget_at_point(x, y, state);
        if let Some(w) = self.widget_at_point {
            let event = EventCrossing {
                state,
                last_widget: last,
                current_widget: Some(w),
            };
            self.widget_enter_notify(w, &event);
        }
    }

    /// `Layout::leaveNotify` — verbatim (branche `#else`
    /// active : `moveOutOfView` seul).
    pub fn leave_notify(&mut self, state: i32) {
        self.move_out_of_view(state);
    }

    /// `moveToWidgetAtPoint` (inline amont).
    fn move_to_widget_at_point(&mut self, x: i32, y: i32, state: i32) {
        let w = self.get_widget_at_point(x, y);
        self.move_to_widget(w, state);
    }

    /// `moveOutOfView` (inline amont).
    fn move_out_of_view(&mut self, state: i32) {
        self.move_to_widget(None, state);
    }

    /// `Layout::getWidgetAtPoint` — verbatim.
    #[must_use]
    pub fn get_widget_at_point(&mut self, x: i32, y: i32) -> Option<WidgetId> {
        let top = self.top_level?;
        if !self.node(top, WidgetNode::was_allocated) {
            return None;
        }
        let mut context = GettingWidgetAtPointContext::default();
        self.w_get_widget_at_point(top, x, y, &mut context)
    }

    /// `Layout::moveToWidget` — verbatim layout.cc:1159 : piste
    /// ancêtre → leave puis enter le long du chemin.
    fn move_to_widget(&mut self, new_widget_at_point: Option<WidgetId>, state: i32) {
        if new_widget_at_point == self.widget_at_point {
            return;
        }
        let old = self.widget_at_point;
        let ancestor = match (new_widget_at_point, old) {
            (Some(a), Some(b)) => self.widget_nearest_common_ancestor(a, b),
            (Some(a), None) => Some(self.widget_top_level(a)),
            (None, Some(b)) => Some(self.widget_top_level(b)),
            (None, None) => None,
        };
        // Piste : [old..ancestor) + ancestor + [new..ancestor)
        // (ordre verbatim : leave old..ancestor, ancêtre selon
        // cas, enter descendants de new en ordre décroissant).
        let mut track: Vec<Option<WidgetId>> = Vec::new();
        if let Some(w) = old {
            let mut cur = Some(w);
            while cur != ancestor {
                match cur {
                    Some(c) => track.push(Some(c)),
                    None => break,
                }
                cur = cur.and_then(|c| self.node(c, |n| n.parent));
            }
        }
        let i_a = track.len();
        track.push(ancestor);
        if let Some(w) = new_widget_at_point {
            // seconde partie en ordre inverse (descendant→
            // ancestor devient ancestor→descendant après push
            // en fin — verbatim insère à la fin décroissante).
            let mut tail = Vec::new();
            let mut cur = Some(w);
            while cur != ancestor {
                match cur {
                    Some(c) => tail.push(c),
                    None => break,
                }
                cur = cur.and_then(|c| self.node(c, |n| n.parent));
            }
            for c in tail.iter().rev() {
                track.push(Some(*c));
            }
        }

        let track_len = track.len();
        for (i, t) in track.iter().enumerate() {
            let Some(w) = t else { continue };
            let event = EventCrossing {
                state,
                last_widget: old,
                current_widget: old,
            };
            match i.cmp(&i_a) {
                std::cmp::Ordering::Less => self.widget_leave_notify(*w, &event),
                std::cmp::Ordering::Equal => {
                    // ancêtre — verbatim : toucher seulement si
                    // départ/arrivée NULL ou ancêtre == new.
                    if i_a == track_len - 1 && new_widget_at_point.is_none() {
                        self.widget_leave_notify(*w, &event);
                    } else if (i_a == 0 && old.is_none())
                        || (i_a == track_len - 1 && new_widget_at_point.is_some())
                    {
                        self.widget_enter_notify(*w, &event);
                    }
                }
                std::cmp::Ordering::Greater => self.widget_enter_notify(*w, &event),
            }
        }

        self.widget_at_point = new_widget_at_point;
        self.update_cursor();
    }

    /// `Layout::processMouseEvent` — verbatim layout.cc:1258 :
    /// clamp dans la zone visible puis remontée par `parent`.
    fn process_mouse_event_common(
        &mut self,
        x_canvas: &mut i32,
        y_canvas: &mut i32,
    ) -> Option<WidgetId> {
        if *x_canvas < self.scroll_x {
            *x_canvas = self.scroll_x;
        } else {
            let max_x = self.scroll_x + self.viewport_width - self.curr_v_scrollbar_thickness() - 1;
            if *x_canvas > max_x {
                *x_canvas = max_x;
            }
        }
        if *y_canvas < self.scroll_y {
            *y_canvas = self.scroll_y;
        } else {
            let max_y = (self.scroll_y + self.viewport_height - self.curr_h_scrollbar_thickness())
                .min(self.canvas_ascent + self.canvas_descent)
                - 1;
            if *y_canvas > max_y {
                *y_canvas = max_y;
            }
        }
        self.get_widget_at_point(*x_canvas, *y_canvas)
    }

    /// `processMouseEvent` pour `EventButton` — verbatim.
    fn process_mouse_event_button(
        &mut self,
        event: &mut EventButton,
        r#type: ButtonEventType,
    ) -> bool {
        let mut x = event.x_canvas;
        let mut y = event.y_canvas;
        let mut widget = self.process_mouse_event_common(&mut x, &mut y);
        event.x_canvas = x;
        event.y_canvas = y;
        while let Some(w) = widget {
            if self.node(w, |n| n.button_sensitive) {
                let a = self.node(w, |n| n.allocation);
                event.x_widget = event.x_canvas - a.x;
                event.y_widget = event.y_canvas - a.y;
                return match r#type {
                    ButtonEventType::ButtonPress => self.widget_button_press(w, event),
                    ButtonEventType::ButtonRelease => self.widget_button_release(w, event),
                    ButtonEventType::MotionNotify => false,
                };
            }
            widget = self.node(w, |n| n.parent);
        }
        match r#type {
            ButtonEventType::ButtonPress => self.emit_link_press(None, -1, -1, -1, -1, event),
            ButtonEventType::ButtonRelease => self.emit_link_release(None, -1, -1, -1, -1, event),
            ButtonEventType::MotionNotify => false,
        }
    }

    /// `processMouseEvent` pour `EventMotion` — verbatim.
    fn process_mouse_event_motion(&mut self, event: &mut EventMotion) -> bool {
        let mut x = event.x_canvas;
        let mut y = event.y_canvas;
        let mut widget = self.process_mouse_event_common(&mut x, &mut y);
        event.x_canvas = x;
        event.y_canvas = y;
        while let Some(w) = widget {
            if self.node(w, |n| n.button_sensitive) {
                let a = self.node(w, |n| n.allocation);
                event.x_widget = event.x_canvas - a.x;
                event.y_widget = event.y_canvas - a.y;
                return self.widget_motion_notify(w, event);
            }
            widget = self.node(w, |n| n.parent);
        }
        false
    }

    /// `Layout::scrollPosChanged` — verbatim.
    pub fn scroll_pos_changed(&mut self, x: i32, y: i32) {
        if x != self.scroll_x || y != self.scroll_y {
            self.scroll_x = x;
            self.scroll_y = y;
            self.set_anchor(None);
            self.update_anchor();
        }
    }

    /// `Layout::viewportSizeChanged` — verbatim.
    pub fn viewport_size_changed(&mut self, width: i32, height: i32) {
        if self.viewport_width != width || self.viewport_height != height {
            self.canvas_height_greater = false;
            self.viewport_width = width;
            self.viewport_height = height;
            self.resize_counter = 0;
            self.container_size_changed();
        }
    }

    /// `Layout::containerSizeChanged` — verbatim : propage au
    /// top level + `queueResize(true)`.
    pub fn container_size_changed(&mut self) {
        if let Some(t) = self.top_level {
            self.widget_container_size_changed(t);
            self.queue_resize(true);
        }
    }

    // ---- signaux (layout.hh inline) ----

    /// `connectLink`.
    pub fn connect_link(&mut self, receiver: Rc<RefCell<dyn LinkReceiver>>) {
        self.link_receiver = Some(receiver);
    }
    /// `connect` (`LayoutReceiver`) ≙ `emitter.connect`.
    pub fn connect_layout_receiver(&mut self, receiver: &Rc<RefCell<dyn LayoutReceiver>>) {
        self.emitter.connect(receiver);
    }
    /// `unconnect`.
    pub fn unconnect_layout_receiver(&mut self, receiver: &Rc<RefCell<dyn LayoutReceiver>>) {
        self.emitter.unconnect(receiver);
    }

    /// `emitLinkEnter`.
    pub fn emit_link_enter(
        &mut self,
        w: Option<WidgetId>,
        link: i32,
        img: i32,
        x: i32,
        y: i32,
    ) -> bool {
        self.link_receiver
            .as_ref()
            .is_some_and(|r| r.borrow_mut().enter(w, link, img, x, y))
    }
    /// `emitLinkPress`.
    pub fn emit_link_press(
        &mut self,
        w: Option<WidgetId>,
        link: i32,
        img: i32,
        x: i32,
        y: i32,
        event: &EventButton,
    ) -> bool {
        self.link_receiver
            .as_ref()
            .is_some_and(|r| r.borrow_mut().pressed(w, link, img, x, y, event))
    }
    /// `emitLinkRelease`.
    pub fn emit_link_release(
        &mut self,
        w: Option<WidgetId>,
        link: i32,
        img: i32,
        x: i32,
        y: i32,
        event: &EventButton,
    ) -> bool {
        self.link_receiver
            .as_ref()
            .is_some_and(|r| r.borrow_mut().released(w, link, img, x, y, event))
    }
    /// `emitLinkClick`.
    pub fn emit_link_click(
        &mut self,
        w: Option<WidgetId>,
        link: i32,
        img: i32,
        x: i32,
        y: i32,
        event: &EventButton,
    ) -> bool {
        self.link_receiver
            .as_ref()
            .is_some_and(|r| r.borrow_mut().clicked(w, link, img, x, y, event))
    }

    // ---- sélection / findtext (glues verbatim) ----

    /// `copyCurrentSelection` (≙ `selectionState.copy`) —
    /// `mem::take` : les méthodes de l'état reçoivent `&mut Layout`
    /// (≙ champ `layout` amont).
    pub fn copy_current_selection(&mut self, selection: i32) {
        let mut st = std::mem::replace(&mut self.selection_state, SelectionState::new());
        st.copy(self, selection);
        self.selection_state = st;
    }

    /// `copySelection` ≙ `platform->copySelection` (inline
    /// `layout.hh:436`) — copie `text` vers `destination`
    /// (0 primary, 1 clipboard, 2 both).
    pub fn copy_selection(&mut self, text: &[u8], destination: i32) {
        self.platform.borrow_mut().copy_selection(text, destination);
    }

    /// `selectionState.handleEvent` — seam appelée par
    /// `Textblock::sendSelectionEvent` avec l'itérateur positionné
    /// (≙ `Iterator*` amont, consommé ici).
    pub(crate) fn selection_state_handle_event(
        &mut self,
        event_type: SelectionEvent,
        it: Box<dyn crate::iterator::DwIterator>,
        char_pos: i32,
        link: i32,
        event: crate::selection::SelectionMouseEvent<'_>,
    ) -> bool {
        let mut st = std::mem::replace(&mut self.selection_state, SelectionState::new());
        let r = st.handle_event(self, event_type, it, char_pos, link, event);
        self.selection_state = st;
        r
    }
    /// `search` ≙ `findtextState.search` — `mem::take` (l'état
    /// reçoit `&mut Layout` ≙ champ `layout` amont).
    pub fn search(&mut self, key: &[u8], case_sens: bool, backwards: bool) -> FindtextResult {
        let mut st = std::mem::replace(&mut self.findtext_state, FindtextState::new());
        let r = st.search(self, key, case_sens, backwards);
        self.findtext_state = st;
        r
    }
    /// `resetSearch` — `unhighlight` interne a besoin de
    /// `&mut Layout` → `mem::take`.
    pub fn reset_search(&mut self) {
        let mut st = std::mem::replace(&mut self.findtext_state, FindtextState::new());
        st.reset_search(self);
        self.findtext_state = st;
    }

    /// Destruction d'un sous-arbre (≙ `delete widget` amont —
    /// ~Widget sur tous les descendants puis le nœud).
    pub fn widget_delete_subtree(&mut self, w: WidgetId) {
        let children = self.widget_children_for_container_change(w);
        for c in children {
            self.widget_delete_subtree(c);
        }
        self.widget_delete(w);
    }
}

// ------------------------------------------------------------------
// Layout::LayoutImgRenderer — layout.hh:41-56 + layout.cc:37-92.
// ------------------------------------------------------------------

/// `Layout::LayoutImgRenderer` — verbatim : fond de layout.
/// Amont tient `Layout*` ; le port tient l'état synchronisé
/// (`sync_layout_img_renderer`) + `Weak` vers l'arène pour les
/// draws différés (modèle D — pas de `&mut Layout` dans un
/// callback renderer).
pub(crate) struct LayoutImgRenderer {
    /// `layout->canvasWidth`.
    canvas_width: i32,
    /// `canvasAscent + canvasDescent`.
    canvas_height: i32,
    /// `layout->viewportWidth`/`viewportHeight`.
    viewport_width: i32,
    viewport_height: i32,
    /// `layout->canvasHeightGreater`.
    canvas_height_greater: bool,
    /// `hScrollbarThickness`/`vScrollbarThickness`.
    h_scrollbar_thickness: i32,
    v_scrollbar_thickness: i32,
    /// `layout->bgImage`.
    bg_image: std::rc::Weak<RefCell<StyleImage>>,
    /// `layout->bgRepeat`/`bgAttachment`/`bgPosition{X,Y}`.
    bg_repeat: style::BackgroundRepeat,
    bg_attachment: style::BackgroundAttachment,
    bg_position_x: Length,
    bg_position_y: Length,
    /// `Layout::id` émetteur (tag `pending_draws`).
    layout_id: usize,
    /// Arène (draws différés — ≙ `layout->queueDraw` amont).
    pending_draws: std::rc::Weak<RefCell<Arena>>,
}

impl ImgRenderer for LayoutImgRenderer {
    /// `setBuffer` — rien (verbatim via helper amont).
    fn set_buffer(&mut self, _b: &Rc<dyn crate::imgbuf::Imgbuf>, _resize: bool) {}
    /// `drawRow` — verbatim (morte amont mais fidèle).
    fn draw_row(&mut self, row: i32) {
        style::external_img_renderer_draw_row(self, row);
    }
    /// `finish` — verbatim via helper.
    fn finish(&mut self) {
        style::external_img_renderer_finish(self);
    }
    /// `fatal` — rien.
    fn fatal(&mut self) {}
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl style::ExternalImgRenderer for LayoutImgRenderer {
    /// `readyToDraw` — `true` verbatim.
    fn ready_to_draw(&self) -> bool {
        true
    }
    /// `getBgArea` — `getRefArea` verbatim (TODO amont).
    fn get_bg_area(&self) -> (i32, i32, i32, i32) {
        self.get_ref_area()
    }
    /// `getRefArea` — verbatim : `max(viewport, canvas)`.
    fn get_ref_area(&self) -> (i32, i32, i32, i32) {
        (
            0,
            0,
            (self.viewport_width
                - if self.canvas_height_greater {
                    self.v_scrollbar_thickness
                } else {
                    0
                })
            .max(self.canvas_width),
            (self.viewport_height - self.h_scrollbar_thickness).max(self.canvas_height),
        )
    }
    /// `getBackgroundImage`.
    fn get_background_image(&self) -> Option<Rc<RefCell<StyleImage>>> {
        self.bg_image.upgrade()
    }
    /// `getBackgroundRepeat`.
    fn get_background_repeat(&self) -> style::BackgroundRepeat {
        self.bg_repeat
    }
    /// `getBackgroundAttachment`.
    fn get_background_attachment(&self) -> style::BackgroundAttachment {
        self.bg_attachment
    }
    /// `getBackgroundPositionX`.
    fn get_background_position_x(&self) -> Length {
        self.bg_position_x
    }
    /// `getBackgroundPositionY`.
    fn get_background_position_y(&self) -> Length {
        self.bg_position_y
    }
    /// `draw` — `layout->queueDraw` amont ≙ `pending_draws`
    /// (adaptation modèle D — différé au prochain
    /// `flush_pending_draws`).
    fn draw(&mut self, x: i32, y: i32, width: i32, height: i32) {
        if let Some(a) = self.pending_draws.upgrade() {
            // Coords canvas directes (figées ≙ `queueDraw` immédiat).
            a.borrow_mut().pending_draws.push((
                self.layout_id,
                Rectangle {
                    x,
                    y,
                    width,
                    height,
                },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::button_state;
    use crate::imgbuf::{Imgbuf, ImgbufType};
    use crate::platform::{IdleCallback, Platform};
    use crate::style::{Font, FontAttrs, Shading, Style, Tooltip};
    use crate::types::Point;
    use crate::widget::WidgetKind;

    // ---- mocks ----

    /// `Platform` mock : file d'idles drainée par le test.
    #[derive(Default)]
    struct MockPlatform {
        /// `(id, layout, callback)` en attente — ≙ file toolkit.
        pending: Vec<(i32, usize, IdleCallback)>,
        /// Prochain id (amont plateforme alloue).
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
            Rc::new(NullImgbuf {
                w,
                h,
                refs: std::cell::Cell::new(1),
                dou: std::cell::Cell::new(true),
            })
        }
        fn copy_selection(&mut self, _t: &[u8], _d: i32) {}
        fn get_resource_factory(&mut self) -> Option<Rc<dyn crate::ui::ResourceFactory>> {
            None
        }
    }

    /// `Imgbuf` stub pour `create_imgbuf`.
    struct NullImgbuf {
        w: i32,
        h: i32,
        /// `refCount` amont.
        refs: std::cell::Cell<i32>,
        /// `deleteOnUnref`.
        dou: std::cell::Cell<bool>,
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
            Rc::new(NullImgbuf {
                w,
                h,
                refs: std::cell::Cell::new(1),
                dou: std::cell::Cell::new(true),
            })
        }
        fn rgb_row(&self, _y: i32) -> Vec<u8> {
            #[allow(clippy::cast_sign_loss)]
            {
                vec![0; (3 * self.w).max(0) as usize]
            }
        }
        fn write_rgb_span(&self, _x: i32, _y: i32, _d: &[u8]) {}
        fn ref_(&self) {
            self.refs.set(self.refs.get() + 1);
        }
        fn unref(&self) {
            self.refs.set(self.refs.get() - 1);
        }
        fn last_reference(&self) -> bool {
            self.refs.get() == 1
        }
        fn set_delete_on_unref(&self, v: bool) {
            self.dou.set(v);
        }
        fn is_referred(&self) -> bool {
            self.refs.get() > 0
        }
    }

    /// `View` enregistreuse (viewport actif).
    #[derive(Default)]
    struct RecView {
        /// `queueDraw` reçus.
        draws: Vec<Rectangle>,
        /// `queueDrawTotal` reçus.
        total_draws: usize,
        /// `cancelQueueDraw` reçus.
        cancels: usize,
        /// `scrollTo` reçus.
        scrolls: Vec<(i32, i32)>,
        /// Dernier `setCanvasSize`.
        canvas: (i32, i32, i32),
        /// Dernier `setViewportSize`.
        viewport: (i32, i32, i32, i32),
        /// `usesViewport`.
        viewport_mode: bool,
        /// Nb d'ops de dessin reçues (sonde draw).
        ops: usize,
        /// Textes dessinés.
        texts: Vec<String>,
    }

    impl RecView {
        /// Vue viewport par défaut des tests.
        fn viewport() -> std::boxed::Box<dyn View> {
            std::boxed::Box::new(RecView {
                viewport_mode: true,
                ..RecView::default()
            })
        }
    }

    impl View for RecView {
        fn set_layout(&mut self, _l: Option<usize>) {}
        fn set_canvas_size(&mut self, w: i32, a: i32, d: i32) {
            self.canvas = (w, a, d);
        }
        fn set_cursor(&mut self, _c: Cursor) {}
        fn set_bg_color(&mut self, _c: &Color) {}
        fn uses_viewport(&mut self) -> bool {
            self.viewport_mode
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
        fn scroll_to(&mut self, x: i32, y: i32) {
            self.scrolls.push((x, y));
        }
        fn set_viewport_size(&mut self, w: i32, h: i32, ht: i32, vt: i32) {
            self.viewport = (w, h, ht, vt);
        }
        fn start_drawing(&mut self, _a: &Rectangle) {}
        fn finish_drawing(&mut self, _a: &Rectangle) {}
        fn queue_draw(&mut self, a: &Rectangle) {
            self.draws.push(*a);
        }
        fn queue_draw_total(&mut self) {
            self.total_draws += 1;
        }
        fn cancel_queue_draw(&mut self) {
            self.cancels += 1;
        }
        fn draw_point(&mut self, _c: &Color, _s: Shading, _x: i32, _y: i32) {}
        fn draw_line(&mut self, _c: &Color, _s: Shading, _x1: i32, _y1: i32, _x2: i32, _y2: i32) {}
        fn draw_typed_line(
            &mut self,
            _c: &Color,
            _s: Shading,
            _t: crate::style::LineType,
            _w: i32,
            _x1: i32,
            _y1: i32,
            _x2: i32,
            _y2: i32,
        ) {
        }
        fn draw_rectangle(
            &mut self,
            _c: &Color,
            _s: Shading,
            _f: bool,
            _x: i32,
            _y: i32,
            _w: i32,
            _h: i32,
        ) {
            self.ops += 1;
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
        fn draw_polygon(&mut self, _c: &Color, _s: Shading, _f: bool, _cv: bool, _p: &[Point]) {
            self.ops += 1;
        }
        fn draw_text(&mut self, _f: &Font, _c: &Color, _s: Shading, _x: i32, _y: i32, t: &[u8]) {
            self.ops += 1;
            self.texts.push(String::from_utf8_lossy(t).into_owned());
        }
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
            _x: i32,
            _y: i32,
            _w: i32,
            _h: i32,
        ) {
        }
        fn get_clipping_view(
            &mut self,
            _x: i32,
            _y: i32,
            _w: i32,
            _h: i32,
        ) -> std::boxed::Box<dyn View> {
            RecView::viewport()
        }
        fn merge_clipping_view(&mut self, _cv: std::boxed::Box<dyn View>) {}
        fn as_any(&self) -> Option<&dyn std::any::Any> {
            Some(self)
        }
    }

    /// Layout de test (limit activée).
    fn make_layout() -> Layout {
        Layout::new(
            Rc::new(RefCell::new(MockPlatform {
                pending: Vec::new(),
                next_id: 0,
            })),
            true,
        )
    }

    // ---- tests ----

    #[test]
    fn hierarchy_top_level_and_ancestors() {
        let mut l = make_layout();
        let parent = l.add_widget_node(WidgetKind::Container {
            children: Vec::new(),
        });
        let child = l.add_widget_node(WidgetKind::Base);
        let grand = l.add_widget_node(WidgetKind::Base);
        l.add_widget(parent);
        l.container_add_child(parent, child);
        l.container_add_child(child, grand);

        assert_eq!(l.get_top_level(), Some(parent));
        assert_eq!(l.widget_top_level(grand), parent);
        assert_eq!(l.widget_level(grand), 2);
        assert_eq!(l.widget_parent(grand), Some(child));
        // ancêtre commun child/grand = child.
        assert_eq!(l.widget_nearest_common_ancestor(grand, child), Some(child));
        // ancêtre de child et parent = parent.
        assert_eq!(
            l.widget_nearest_common_ancestor(child, parent),
            Some(parent)
        );
        // `container` = plus proche ancêtre possibleContainer —
        // `Base`/`Container` ne sont pas block-level → viewport.
        assert_eq!(l.widget_container(child), None);
    }

    #[test]
    fn queue_resize_flags_propagate_and_idle_allocates() {
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        let top = l.add_widget_node(WidgetKind::Base);
        let child = l.add_widget_node(WidgetKind::Base);
        l.add_widget(top);
        l.container_add_child(top, child);

        // queueResize sur l'enfant → flags propagés aux ancêtres.
        l.widget_queue_resize(child, 7, true);
        assert!(l.node(child, |n| n.resize_queued()
            && n.allocate_queued()
            && n.extremes_queued()));
        assert!(l.node(top, |n| n.resize_queued() && n.allocate_queued()));

        // l'idle transforme QUEUED→NEEDS puis alloue le top.
        l.drain_idles();
        assert!(l.node(top, |n| n.was_allocated() && !n.needs_allocate()));
        assert!(l.node(child, |n| n.needs_resize() && n.extremes_changed()));
    }

    #[test]
    fn remove_widget_resets_canvas_and_view() {
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        let top = l.add_widget_node(WidgetKind::Base);
        l.add_widget(top);
        l.widget_queue_resize(top, 0, false);
        l.drain_idles();

        l.remove_widget();
        assert_eq!(l.get_top_level(), None);
        assert_eq!(l.widget_at_point, None);
        assert_eq!(l.canvas_width, 0);
        assert_eq!(l.canvas_ascent + l.canvas_descent, 0);
    }

    #[test]
    fn scroll_to_into_view_clamps() {
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        // canvas 200x200, viewport 100x100 (champs pub(crate) —
        // posés comme le ferait viewportSizeChanged/setCanvasSize).
        l.viewport_width = 100;
        l.viewport_height = 100;
        l.canvas_width = 200;
        l.canvas_ascent = 200;
        l.canvas_descent = 0;
        l.uses_viewport = true;

        // région (150,150,10x10) → scroll 60,60 (into view).
        l.scroll_to(HPosition::IntoView, VPosition::IntoView, 150, 150, 10, 10);
        l.drain_idles();
        assert_eq!(l.scroll_x, 60);
        assert_eq!(l.scroll_y, 60);

        // région hors bornes → clamp canvas.
        l.scroll_to(HPosition::Left, VPosition::Top, 500, 500, 10, 10);
        l.drain_idles();
        assert_eq!(l.scroll_x, 100);
        assert_eq!(l.scroll_y, 100);
    }

    #[test]
    fn anchors_scroll_to_named_position() {
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        let w = l.add_widget_node(WidgetKind::Base);
        l.add_widget(w);
        // Le resize initial (verbatim `addWidget`) calcule le canvas
        // depuis la requête — on seed après, comme un resizeIdle
        // amont qui aurait déjà tourné.
        l.drain_idles();
        l.viewport_width = 100;
        l.viewport_height = 100;
        l.canvas_width = 100;
        l.canvas_ascent = 500;
        l.uses_viewport = true;

        assert!(l.add_anchor(w, b"top", 150).is_some());
        // doublon → None (verbatim `anchorsTable->contains`).
        assert!(l.add_anchor(w, b"top", 200).is_none());

        l.set_anchor(Some(b"top"));
        l.drain_idles();
        assert_eq!(l.scroll_y, 150);

        l.remove_anchor(b"top");
        l.set_anchor(Some(b"top"));
        l.drain_idles();
        assert_eq!(l.scroll_y, 150); // inchangé — anchor absente.
    }

    #[test]
    fn button_event_falls_back_to_link_receiver() {
        struct Rec {
            presses: usize,
            /// dernier `widget` vu — `None` ≙ NULL amont
            /// (la détection d'appel passe par `presses`).
            widget_seen: Option<WidgetId>,
        }
        impl LinkReceiver for Rec {
            fn pressed(
                &mut self,
                w: Option<WidgetId>,
                _l: i32,
                _i: i32,
                _x: i32,
                _y: i32,
                _e: &EventButton,
            ) -> bool {
                self.presses += 1;
                self.widget_seen = w;
                true
            }
        }

        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        let rec = Rc::new(RefCell::new(Rec {
            presses: 0,
            widget_seen: None,
        }));
        l.connect_link(rec.clone());

        // layout vide (pas de top level) → widget = None ≙ NULL.
        let handled = l.button_event(
            ButtonEventType::ButtonPress,
            1,
            10,
            10,
            button_state::BUTTON1_MASK,
            1,
        );
        assert!(handled);
        assert_eq!(rec.borrow().presses, 1);
        assert_eq!(rec.borrow().widget_seen, None);
    }

    #[test]
    fn widget_intersects_clips_to_allocations() {
        let mut l = make_layout();
        let parent = l.add_widget_node(WidgetKind::Base);
        let child = l.add_widget_node(WidgetKind::Base);
        l.add_widget(parent);
        l.container_add_child(parent, child);

        // allocations posées à la main (≙ sizeAllocate effectué).
        l.node_mut(parent, |n| {
            n.allocation = Allocation {
                x: 10,
                y: 10,
                width: 50,
                ascent: 50,
                descent: 0,
            };
            n.set_flags(crate::widget::WAS_ALLOCATED);
        });
        l.node_mut(child, |n| {
            n.allocation = Allocation {
                x: 20,
                y: 20,
                width: 60,
                ascent: 20,
                descent: 0,
            };
            n.set_flags(crate::widget::WAS_ALLOCATED);
        });

        // zone (0,0,30,30) en coords parent = (10,10)-(40,40)
        // canvas → intersection child (20,20)-(80,40) →
        // (20,20)-(40,40) ; en coords child : (0,0,20,20).
        let area = Rectangle {
            x: 0,
            y: 0,
            width: 30,
            height: 30,
        };
        let r = l.widget_intersects(child, parent, &area);
        assert_eq!(r, Some(Rectangle::new(0, 0, 20, 20)));

        // en dehors → None.
        let far = Rectangle {
            x: 0,
            y: 100,
            width: 10,
            height: 10,
        };
        assert_eq!(l.widget_intersects(child, parent, &far), None);
    }

    #[test]
    fn widget_get_bg_color_walks_ancestors() {
        let mut l = make_layout();
        let parent = l.add_widget_node(WidgetKind::Base);
        let child = l.add_widget_node(WidgetKind::Base);
        l.add_widget(parent);
        l.container_add_child(parent, child);

        l.widget_set_bg_color(parent, Some(Color::new(0x00aa_bbcc)));
        assert_eq!(
            l.widget_get_bg_color(child).map(|c| c.get_color()),
            Some(0x00aa_bbcc)
        );
        // style du child prioritaire.
        let attrs = crate::style::StyleAttrs {
            background_color: Some(Color::new(0x0011_2233)),
            ..crate::style::StyleAttrs::default()
        };
        l.widget_set_style(child, &Style::create(&attrs));
        assert_eq!(
            l.widget_get_bg_color(child).map(|c| c.get_color()),
            Some(0x0011_2233)
        );
        // fg : hérite du parent.
        assert_eq!(l.widget_get_fg_color(child), None);
    }

    #[test]
    fn size_request_caches_when_unchanged() {
        let mut l = make_layout();
        let w = l.add_widget_node(WidgetKind::Base);
        l.add_widget(w);
        let req = crate::types::Requisition {
            width: 5,
            ascent: 3,
            descent: 2,
        };
        // Premier appel : NEEDS_RESIZE → impl (no-op `Base`)
        // → cache = requisition par défaut + flags vidés.
        let mut out = crate::types::Requisition::default();
        l.widget_size_request(w, &mut out, 0, &[], &[], &[]);
        assert!(!l.node(w, WidgetNode::needs_resize));
        // Deuxième appel, mêmes params : valeur cachée servie
        // sans rappeler l'impl.
        l.node_mut(w, |n| n.requisition = req);
        l.widget_size_request(w, &mut out, 0, &[], &[], &[]);
        assert_eq!(out, req);
    }

    #[test]
    fn get_widget_at_point_descends_children() {
        let mut l = make_layout();
        let top = l.add_widget_node(WidgetKind::Container {
            children: Vec::new(),
        });
        let child = l.add_widget_node(WidgetKind::Base);
        l.add_widget(top);
        l.container_add_child(top, child);
        l.node_mut(top, |n| {
            n.allocation = Allocation {
                x: 0,
                y: 0,
                width: 100,
                ascent: 100,
                descent: 0,
            };
            n.set_flags(crate::widget::WAS_ALLOCATED);
        });
        l.node_mut(child, |n| {
            n.allocation = Allocation {
                x: 10,
                y: 10,
                width: 20,
                ascent: 20,
                descent: 0,
            };
            n.set_flags(crate::widget::WAS_ALLOCATED);
        });
        assert_eq!(l.get_widget_at_point(15, 15), Some(child));
        // point dans top mais hors child → top.
        assert_eq!(l.get_widget_at_point(50, 50), Some(top));
        // hors tout → None.
        assert_eq!(l.get_widget_at_point(200, 200), None);
    }

    // ---- A05 apply — lifecycle addWidget/setWidget/detached ----

    #[test]
    fn add_widget_queues_initial_resize_and_resets_chg() {
        // Verbatim `addWidget` (layout.cc:396-403) : reset
        // `canvasHeightGreater` puis `topLevel->queueResize(-1,false)`
        // — flags RESIZE_QUEUED/ALLOCATE_QUEUED + idle armé (A05-D2).
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        l.canvas_height_greater = true;

        let top = l.add_widget_node(WidgetKind::Base);
        l.add_widget(top);
        assert!(!l.canvas_height_greater);
        assert!(l.node(top, |n| n.resize_queued() && n.allocate_queued()));

        // l'idle file alors le premier sizeRequest/sizeAllocate.
        l.drain_idles();
        assert!(l.node(top, |n| n.was_allocated() && !n.needs_allocate()));
    }

    #[test]
    fn add_widget_emits_resize_queued() {
        // Le signal `resizeQueued` amont est émis via
        // `Widget::queueResize` → `Layout::queueResize` (A05-D2).
        struct Rec {
            resizes: usize,
        }
        impl LayoutReceiver for Rec {
            fn resize_queued(&mut self, _extremes_changed: bool) {
                self.resizes += 1;
            }
        }
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        let rec = Rc::new(RefCell::new(Rec { resizes: 0 }));
        let rec_dyn: Rc<RefCell<dyn LayoutReceiver>> = rec.clone();
        l.connect_layout_receiver(&rec_dyn);

        let top = l.add_widget_node(WidgetKind::Base);
        l.add_widget(top);
        assert_eq!(rec.borrow().resizes, 1);
    }

    #[test]
    fn set_widget_replace_resets_and_requeues() {
        // Verbatim `setWidget` : `topLevel=NULL` puis `delete` —
        // `~Widget` appelle `layout->removeWidget()` car
        // `layout != NULL` (≙ rattaché) — reset scroll/canvas/view
        // *pendant* la destruction (A05-D3), puis `addWidget` re-file
        // le nouveau top level.
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        let a = l.add_widget_node(WidgetKind::Base);
        l.set_widget(a);
        l.drain_idles();
        assert!(l.node(a, WidgetNode::was_allocated));
        l.scroll_x = 10;
        l.scroll_y = 20;

        let b = l.add_widget_node(WidgetKind::Base);
        l.set_widget(b);
        // `a` = tombe detached ; `removeWidget` a remis scroll à 0
        // et réémis `canvasSizeChanged(0,0,0)`.
        assert!(l.node_detached(a));
        assert_eq!((l.scroll_x, l.scroll_y), (0, 0));
        assert_eq!(l.canvas_width, 0);
        // `b` = top level attaché, resize re-filé (≙ addWidget).
        assert!(!l.node_detached(b));
        assert!(l.node(b, WidgetNode::resize_queued));
        l.drain_idles();
        assert!(l.node(b, WidgetNode::was_allocated));
    }

    /// `pending_draws` : les coords **canvas** sont figées à
    /// l'émission (≙ `queueDrawArea` → `queueDraw(x + alloc.x)`
    /// immédiat amont). Si le widget bouge entre push et flush
    /// (relayout pendant le rendu progressif), la zone endommagée
    /// ne doit pas être décalée — sinon les pixels peints à la
    /// nouvelle position ne sont jamais rafraîchis (vestiges).
    #[test]
    fn pending_draw_canvas_coords_frozen_at_emit() {
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        let w = l.add_widget_node(WidgetKind::Base);
        l.add_widget(w);
        l.drain_idles();

        // Émission d'un draw par un renderer externe — la rect est
        // déjà en coords canvas au push.
        let rect = Rectangle {
            x: 7,
            y: 9,
            width: 3,
            height: 4,
        };
        l.arena.borrow_mut().pending_draws.push((l.id, rect));

        // Déplacement du widget entre push et flush (relayout) —
        // sans effet sur le damage (figé ≙ amont).
        l.node_mut(w, |n| {
            n.allocation.x += 40;
            n.allocation.y += 50;
        });
        l.flush_pending_draws();

        let rv = l
            .view
            .as_ref()
            .and_then(|v| v.as_any())
            .and_then(|a| a.downcast_ref::<RecView>())
            .expect("recview");
        assert!(
            rv.draws.contains(&rect),
            "damage attendu aux coords d'émission: {:?}",
            rv.draws
        );
        assert!(
            !rv.draws.iter().any(|r| r.x == 47 && r.y == 59),
            "damage déplacé (alloc du flush) = bug vestige: {:?}",
            rv.draws
        );
    }

    #[test]
    fn queue_resize_unattached_sets_needs_without_idle() {
        // `if (layout) … else` amont : un widget pas encore rattaché
        // prend la branche NEEDS_*/EXTREMES_CHANGED — pas de
        // `queueResizeList`, pas d'idle, pas d'émission (A05-D4).
        struct Rec {
            resizes: usize,
        }
        impl LayoutReceiver for Rec {
            fn resize_queued(&mut self, _extremes_changed: bool) {
                self.resizes += 1;
            }
        }
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        let rec = Rc::new(RefCell::new(Rec { resizes: 0 }));
        let rec_dyn: Rc<RefCell<dyn LayoutReceiver>> = rec.clone();
        l.connect_layout_receiver(&rec_dyn);

        let w = l.add_widget_node(WidgetKind::Base);
        assert!(l.node_detached(w)); // ≙ `layout == NULL` amont.
        l.widget_queue_resize(w, 0, true);
        assert!(l.node(w, |n| n.needs_resize()
            && n.extremes_changed()
            && !n.resize_queued()));
        assert_eq!(rec.borrow().resizes, 0);

        // L'attache conserve les flags → le prochain drain alloue.
        l.add_widget(w);
        l.drain_idles();
        assert!(l.node(w, WidgetNode::was_allocated));
    }

    /// Régression : les overrides textblock rappelaient le
    /// dispatcher `w_*` au lieu de l'implémentation de base
    /// `Widget::*` (liaison statique amont) —
    /// `affectsSizeChangeContainerChild`, `getAvailWidthOfChild`,
    /// `getAvailHeightOfChild` → récursion infinie → stack
    /// overflow sur la première `Resized` réelle.
    #[test]
    fn container_size_change_no_self_dispatch_recursion() {
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        let top = l.add_widget_node(WidgetKind::Textblock {
            tb: Box::new(crate::textblock::TextblockData::new(false, false)),
        });
        l.add_widget(top);
        // Enfant in-flow du textblock (≙ `<div>` + `display:
        // inline-block` dans `about:splash`).
        let child = l.add_widget_node(WidgetKind::Textblock {
            tb: Box::new(crate::textblock::TextblockData::new(false, false)),
        });
        let style = Style::create(&crate::style::StyleAttrs::default());
        l.tb_add_widget(top, child, &style);

        // `viewportSizeChanged` → `containerSizeChanged` →
        // `affectsSizeChangeContainerChild` (top toplevel →
        // `usesMaxGeneratorWidth` → fallback base) — boucle
        // infinie avant le fix.
        l.viewport_size_changed(800, 600);
        l.drain_idles();
        assert!(l.node(child, WidgetNode::was_allocated));
    }

    /// Régression : un draw pendant `scrollIdle` diffère — le
    /// damage doit être re-queué sur la vue, sinon `take_damage`
    /// l'a déjà consommé et `scrollIdle` ne ré-émet
    /// `queueDrawTotal` que si la position bouge → le contenu
    /// n'est jamais repeint (canvas blanc après `set_scroll_xy`
    /// de fin de navigation — bug réel `about:splash`).
    #[test]
    fn deferred_draw_during_scroll_requeues_damage() {
        let mut l = make_layout();
        l.attach_view(RecView::viewport());
        let top = l.add_widget_node(WidgetKind::Textblock {
            tb: Box::new(crate::textblock::TextblockData::new(false, false)),
        });
        l.add_widget(top);
        l.viewport_size_changed(800, 600);
        l.drain_idles();

        // Scroll en attente ≙ `nav` → `set_scroll_xy` : l'idle
        // est posée, pas encore drainée → `scroll_idle_id != -1`.
        l.scroll_to(
            crate::types::HPosition::NoChange,
            crate::types::VPosition::Top,
            0,
            0,
            0,
            0,
        );
        assert_ne!(l.scroll_idle_id, -1);

        let mut v = RecView::viewport();
        let area = Rectangle {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        };
        l.draw(&mut *v, &area);
        let rv = v
            .as_any()
            .and_then(|a| a.downcast_ref::<RecView>())
            .expect("recview");
        assert!(
            rv.draws.contains(&area),
            "deferred draw lost the damage: {:?}",
            rv.draws
        );
        assert!(l.draw_after_scroll_req);

        // Une fois l'idle scroll résolu, le damage re-queué permet
        // le vrai draw.
        l.drain_idles();
        assert_eq!(l.scroll_idle_id, -1);
    }

    /// Régression : plateforme partagée entre layouts (onglets) —
    /// l'idle de A doit être routé vers A, jamais exécuté sur B
    /// (amont : `FltkPlatform` par onglet = file privée). Sinon le
    /// `scroll_idle_id` de A reste posé → draws différés à
    /// l'infini → canvas blanc (bug réel mode shell).
    #[test]
    fn idle_callbacks_route_to_owning_layout() {
        let platform: Rc<RefCell<dyn Platform>> = Rc::new(RefCell::new(MockPlatform {
            pending: Vec::new(),
            next_id: 0,
        }));
        let mut a = Layout::new(Rc::clone(&platform), true);
        let mut b = Layout::new(Rc::clone(&platform), true);
        a.attach_view(RecView::viewport());
        a.scroll_to(
            crate::types::HPosition::NoChange,
            crate::types::VPosition::Top,
            0,
            0,
            0,
            0,
        );
        assert_ne!(a.scroll_idle_id, -1);

        // B draine : l'idle de A ne doit pas se résoudre.
        b.drain_idles();
        assert_ne!(a.scroll_idle_id, -1);

        // A draine : son idle se résout.
        a.drain_idles();
        assert_eq!(a.scroll_idle_id, -1);
    }
}
