//! Port de `dw/widget.hh` + `dw/widget.cc` — nœud de base de
//! l'arbre de widgets.
//!
//! Modèle ADR-0005 (option D) : `Widget*` amont → `WidgetId`
//! (index dans l'arène `Layout.arena`). Toutes les méthodes amont
//! `widget->foo(...)` deviennent `layout.widget_foo(w, ...)` ;
//! les helpers qui ne touchent que le nœud sont des méthodes
//! `WidgetNode`. Les méthodes virtuelles amont sont dispatchées
//! sur `WidgetKind` (seul `Base` pour l'instant — les impls par
//! défaut sont celles de `dw::core::Widget` amont, verbatim).
//!
//! `layout` champ amont : implicite (les nœuds vivent dans
//! l'arène du layout ; `layout == NULL` amont ≙ nœud pas encore
//! rattaché — cf. `parent`/`top_level`).
//!
//! `misc::notImplemented` amont (imprime puis continue) →
//! no-op + note ; `MSG_*` diagnostiques élidés (sans effet
//! comportemental).
#![allow(clippy::unused_self)]
// Les méthodes virtuelles amont sont majoritairement des
// impls par défaut no-op (`Widget::fooImpl`) — verbatim.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::events::{EventButton, EventCrossing, EventMotion};
use crate::layout::Layout;
use crate::style::{
    self, abs_length_val, is_abs_length, is_per_length, multiply_with_per_length, Color, Style,
    LENGTH_AUTO,
};
use crate::textblock::is_textblock_kind;
use crate::tools::SizeParams;
use crate::types::{
    content_type, Allocation, ContentData, DrawingContext, Extremes, GettingWidgetAtPointContext,
    HPosition, Requisition, VPosition, WidgetReference,
};
use crate::view::View;
use crate::WidgetId;

// ------------------------------------------------------------------
// Flags — widget.hh:38-66, verbatim.
// ------------------------------------------------------------------

/// `Widget::RESIZE_QUEUED`.
pub(crate) const RESIZE_QUEUED: u32 = 1 << 0;
/// `Widget::EXTREMES_QUEUED`.
pub(crate) const EXTREMES_QUEUED: u32 = 1 << 1;
/// `Widget::NEEDS_RESIZE`.
pub(crate) const NEEDS_RESIZE: u32 = 1 << 2;
/// `Widget::NEEDS_ALLOCATE`.
pub(crate) const NEEDS_ALLOCATE: u32 = 1 << 3;
/// `Widget::ALLOCATE_QUEUED`.
pub(crate) const ALLOCATE_QUEUED: u32 = 1 << 4;
/// `Widget::EXTREMES_CHANGED`.
pub(crate) const EXTREMES_CHANGED: u32 = 1 << 5;
/// `Widget::WAS_ALLOCATED`.
pub(crate) const WAS_ALLOCATED: u32 = 1 << 6;

/// `Widget::adjustMinWidth` — statique de classe amont (init
/// `true`, `setAdjustMinWidth` pour les tests).
static ADJUST_MIN_WIDTH: AtomicBool = AtomicBool::new(true);

/// `Widget::setAdjustMinWidth`.
pub fn set_adjust_min_width(v: bool) {
    ADJUST_MIN_WIDTH.store(v, Ordering::Relaxed);
}

/// `PASS_INCREASE`/`PASS_DECREASE`/`PASS_KEEP` de
/// `correctReqAspectRatio` (widget.cc:30).
const PASS_INCREASE: i32 = 0;
const PASS_DECREASE: i32 = 1;
const PASS_KEEP: i32 = 2;

/// `IMPL_POS` (core.hh:16) — `false` amont : le bloc
/// `StackingContextMgr` de `Layout::addWidget` est inactif
/// verbatim (positioning est une phase ultérieure — la const
/// reste pour référence jusqu'à la phase OOF).
#[allow(dead_code)]
pub(crate) const IMPL_POS: bool = false;

// ------------------------------------------------------------------
// WidgetKind — dispatch des méthodes virtuelles.
// ------------------------------------------------------------------

/// `enum`-dispatch du kind de widget (ADR-0005).
///
/// `Base` = `dw::core::Widget` : les impls virtuelles par défaut
/// amont sont reproduites verbatim — `sizeRequestSimpl`/
/// `getExtremesSimpl` appellent `notImplemented` amont (trace +
/// retour de valeurs initiales ; ici : `size_request_simpl`
/// no-op laissant les champs passés).
///
/// `Container` = seam d'itération des enfants pour
/// `containerSizeChangedForChildren`/`getWidgetAtPoint`
/// (amont passe par `iterator` — module `iterator.cc` encore
/// en attente ; les kinds concrets exposeront leurs enfants via
/// la seam `widget_children_for_container_change`).
#[derive(Debug, Default)]
pub enum WidgetKind {
    /// `dw::core::Widget` (impls virtuelles par défaut).
    #[default]
    Base,
    /// Conteneur à enfants directs (`children` — seam en
    /// attendant les kinds concrets Textblock/Table qui
    /// porteront leurs propres structures de contenu).
    Container {
        /// Enfants ordonnés (≙ contenu `WIDGET_IN_FLOW`).
        children: Vec<WidgetId>,
    },
    /// Feuille texte d'un mot — ≙ l'usage `TextIterator`
    /// anticipé mais commenté amont (bullet/image/ruler) ;
    /// seam de test pour `DeepIterator`/`CharIterator` en
    /// attendant `Textblock`.
    Text {
        /// Le mot (`const char *text` amont).
        text: Rc<[u8]>,
    },
    /// `dw::oof::OOFAwareWidget` — widget pouvant servir de
    /// container et de générateur de widgets hors-flux
    /// (oof.rs — `RegardingBorder`/Textblock/Table/Ruler
    /// embarqueront le même `OofAwareData`).
    OofAware {
        /// Champs `OOFAwareWidget` (`oofContainer`,
        /// `outOfFlowMgr`, `requisitionWithoutOOF`).
        oof: crate::oof::OofAwareData,
    },
    /// `dw::Textblock` — bloc de texte/lignes/widgets
    /// (textblock.cc) ; hérite `RegardingBorder` amont ≙
    /// `OofAwareData` embarqué dans `TextblockData`.
    Textblock {
        /// État verbatim `textblock.hh` (`words`/`lines`/
        /// `paragraphs`/`anchors` + partie `OOFAwareWidget`) —
        /// boxed (variant le plus volumineux, ≙ sous-classe
        /// amont allouée à part).
        tb: Box<crate::textblock::TextblockData>,
    },
    /// `dw::Bullet` — puce de liste (bullet.cc), `Widget`
    /// direct amont, leaf sans état.
    Bullet,
    /// `dw::Image` — widget image (image.cc), `Widget` direct
    /// amont ; `img` ≙ champs verbatim.
    Image {
        /// Champs verbatim `image.hh`.
        img: crate::image::ImageData,
    },
    /// `dw::Ruler` — `<hr>` (ruler.cc) ; `Ruler : RegardingBorder`
    /// amont ≙ `OofAwareData` embarqué.
    Ruler {
        /// Champs `OOFAwareWidget`.
        oof: crate::oof::OofAwareData,
    },
    /// `dw::core::ui::Embed` — widget hôte d'une `Resource` de
    /// formulaire (ui.cc:36) ; `Rc` partagé car la machinerie du
    /// layout interne (`ComplexButtonResource`) re-entre dans le
    /// même objet pendant `size_request`.
    Embed {
        /// `resource` (≙ `Resource*` possédé amont).
        res: Rc<dyn crate::ui::Resource>,
    },
    /// `dw::Table` — table (table.cc) ; `Table : OOFAwareWidget`
    /// amont ≙ `OofAwareData` embarqué dans `TableData` — boxed
    /// (variant volumineux).
    Table {
        /// Champs verbatim `table.hh` (grille `children`,
        /// `colExtremes`, `colWidths`, `cumHeight`, `rowStyle`…).
        tbl: Box<crate::table::TableData>,
    },
}

// ------------------------------------------------------------------
// WidgetNode — champs verbatim widget.hh:124-219.
// ------------------------------------------------------------------

/// Un widget : nœud d'arène. Tous les liens sont des
/// `Option<WidgetId>` ≙ pointeurs amont (`NULL` → `None`).
#[allow(clippy::struct_excessive_bools)]
pub struct WidgetNode {
    /// `flags` (combinaison des consts ci-dessus).
    pub(crate) flags: u32,
    /// `parent`.
    pub(crate) parent: Option<WidgetId>,
    /// `quasiParent`.
    pub(crate) quasi_parent: Option<WidgetId>,
    /// `generator`.
    pub(crate) generator: Option<WidgetId>,
    /// `container` (containing block CSS ; `None` ≙ viewport).
    pub(crate) container: Option<WidgetId>,
    /// `widgetReference`.
    pub(crate) widget_reference: Option<WidgetReference>,
    /// `style` (`Rc` ≙ `style->ref()`).
    pub(crate) style: Option<Rc<Style>>,
    /// `requisition` — dernier résultat de `sizeRequestImpl`.
    pub(crate) requisition: Requisition,
    /// `requisitionParams`.
    pub(crate) requisition_params: SizeParams,
    /// `extremes`.
    pub(crate) extremes: Extremes,
    /// `extremesParams`.
    pub(crate) extremes_params: SizeParams,
    /// `bgColor` (fond « derrière », ≙ `setBgColor`).
    pub(crate) bg_color: Option<Color>,
    /// `buttonSensitive`.
    pub(crate) button_sensitive: bool,
    /// `buttonSensitiveSet`.
    pub(crate) button_sensitive_set: bool,
    /// `parentRef` — valeur définie par le parent pour le
    /// redimensionnement incrémental.
    pub parent_ref: i32,
    /// `allocation` — position/taille courantes (canvas).
    pub(crate) allocation: Allocation,
    /// `ratio` — ratio préféré `width/height` (0 = aucun).
    pub(crate) ratio: f32,
    /// `extraSpace` — espace autour de la margin box.
    pub(crate) extra_space: crate::style::Box,
    /// `stackingContextMgr != NULL` (le gestionnaire arrive avec
    /// la phase OOF/stacking — flag conservé verbatim).
    pub(crate) has_stacking_context_mgr: bool,
    /// `stackingContextWidget` — plus bas ancêtre avec SCM.
    pub(crate) stacking_context_widget: Option<WidgetId>,
    /// `widgetImgRenderer != NULL` (l'instance vit dans la
    /// `StyleImage` ; flag ≙ pointeur non nul amont).
    pub(crate) widget_img_renderer: bool,
    /// `deleteCallback{Func,Data}` ≙ `Rc<dyn Fn()>`.
    pub(crate) delete_callback: Option<Rc<dyn Fn()>>,
    /// `layout == NULL` amont (≙ `detachWidget`) — porté par
    /// [`crate::layout::Arena::detached`] (flag d'arène, pas du
    /// nœud : `node_detached` le consulte).
    /// Dispatch des méthodes virtuelles.
    pub(crate) kind: WidgetKind,
}

impl WidgetNode {
    /// `Widget()` — verbatim (allocation init `(-1,-1,1,1+0)`).
    #[must_use]
    pub fn new(kind: WidgetKind) -> Self {
        WidgetNode {
            flags: NEEDS_RESIZE | EXTREMES_CHANGED,
            parent: None,
            quasi_parent: None,
            generator: None,
            container: None,
            widget_reference: None,
            style: None,
            requisition: Requisition::default(),
            requisition_params: SizeParams::default(),
            extremes: Extremes::default(),
            extremes_params: SizeParams::default(),
            bg_color: None,
            button_sensitive: true,
            button_sensitive_set: false,
            parent_ref: 0,
            allocation: Allocation {
                x: -1,
                y: -1,
                width: 1,
                ascent: 1,
                descent: 0,
            },
            ratio: 0.0,
            extra_space: crate::style::Box::default(),
            has_stacking_context_mgr: false,
            stacking_context_widget: None,
            widget_img_renderer: false,
            delete_callback: None,
            kind,
        }
    }

    /// `~Widget` partie 1/2 — callback + retrait du renderer
    /// externe (le reste : `Layout::remove_widget`/`remove_child`).
    pub(crate) fn run_delete_callback(&mut self) {
        if let Some(cb) = self.delete_callback.take() {
            cb();
        }
    }

    // ---- flags (verbatim inline) ----

    /// `resizeQueued`.
    #[must_use]
    pub(crate) fn resize_queued(&self) -> bool {
        self.flags & RESIZE_QUEUED != 0
    }
    /// `extremesQueued`.
    #[must_use]
    pub(crate) fn extremes_queued(&self) -> bool {
        self.flags & EXTREMES_QUEUED != 0
    }
    /// `needsResize`.
    #[must_use]
    pub(crate) fn needs_resize(&self) -> bool {
        self.flags & NEEDS_RESIZE != 0
    }
    /// `needsAllocate`.
    #[must_use]
    pub(crate) fn needs_allocate(&self) -> bool {
        self.flags & NEEDS_ALLOCATE != 0
    }
    /// `allocateQueued`.
    #[must_use]
    pub(crate) fn allocate_queued(&self) -> bool {
        self.flags & ALLOCATE_QUEUED != 0
    }
    /// `extremesChanged`.
    #[must_use]
    pub(crate) fn extremes_changed(&self) -> bool {
        self.flags & EXTREMES_CHANGED != 0
    }
    /// `wasAllocated`.
    #[must_use]
    pub(crate) fn was_allocated(&self) -> bool {
        self.flags & WAS_ALLOCATED != 0
    }
    /// `setFlags`.
    pub(crate) fn set_flags(&mut self, f: u32) {
        self.flags |= f;
    }
    /// `unsetFlags`.
    pub(crate) fn unset_flags(&mut self, f: u32) {
        self.flags &= !f;
    }

    /// `getHeight` — `ascent + descent`.
    #[must_use]
    pub(crate) fn get_height(&self) -> i32 {
        self.allocation.ascent + self.allocation.descent
    }

    /// `getGenerator` — fallback `parent` (widget.hh:600).
    #[must_use]
    pub(crate) fn get_generator(&self) -> Option<WidgetId> {
        self.generator.or(self.parent)
    }

    /// `inAllocation (x, y)` — canvas coords.
    #[must_use]
    pub(crate) fn in_allocation(&self, x: i32, y: i32) -> bool {
        self.was_allocated()
            && x >= self.allocation.x
            && y >= self.allocation.y
            && x <= self.allocation.x + self.allocation.width
            && y <= self.allocation.y + self.get_height()
    }

    /// Style courant (`&StyleAttrs` — amont déréférence
    /// `style` sans garde ; ici `None` → attributs par défaut
    /// via [`Layout::node_style`]).
    #[must_use]
    pub(crate) fn style_attrs(&self) -> Option<&style::StyleAttrs> {
        self.style.as_deref().map(|s| &s.attrs)
    }

    /// `boxOffsetX` — `extraSpace.left + style->boxOffsetX()`.
    /// Widget sans style : `style` part `None` → 0 pour la
    /// partie style (amont crasherait — le port reste sain).
    #[must_use]
    pub(crate) fn box_offset_x(&self) -> i32 {
        self.extra_space.left
            + self
                .style_attrs()
                .map_or(0, style::StyleAttrs::box_offset_x)
    }
    /// `boxRestWidth`.
    #[must_use]
    pub(crate) fn box_rest_width(&self) -> i32 {
        self.extra_space.right
            + self
                .style_attrs()
                .map_or(0, style::StyleAttrs::box_rest_width)
    }
    /// `boxDiffWidth`.
    #[must_use]
    pub(crate) fn box_diff_width(&self) -> i32 {
        self.box_offset_x() + self.box_rest_width()
    }
    /// `boxOffsetY`.
    #[must_use]
    pub(crate) fn box_offset_y(&self) -> i32 {
        self.extra_space.top
            + self
                .style_attrs()
                .map_or(0, style::StyleAttrs::box_offset_y)
    }
    /// `boxRestHeight`.
    #[must_use]
    pub(crate) fn box_rest_height(&self) -> i32 {
        self.extra_space.bottom
            + self
                .style_attrs()
                .map_or(0, style::StyleAttrs::box_rest_height)
    }
    /// `boxDiffHeight`.
    #[must_use]
    pub(crate) fn box_diff_height(&self) -> i32 {
        self.box_offset_y() + self.box_rest_height()
    }
    /// `getContentWidth` (inline amont) — utilisée par les
    /// kinds conteneurs à venir.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn get_content_width(&self) -> i32 {
        self.allocation.width - self.box_diff_width()
    }
    /// `getContentHeight` — même note que `get_content_width`.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn get_content_height(&self) -> i32 {
        self.get_height() - self.box_diff_height()
    }

    /// `getMarginArea` — verbatim.
    #[must_use]
    pub(crate) fn get_margin_area(&self) -> (i32, i32, i32, i32) {
        (
            self.allocation.x + self.extra_space.left,
            self.allocation.y + self.extra_space.top,
            self.allocation.width - (self.extra_space.left + self.extra_space.right),
            self.get_height() - (self.extra_space.top + self.extra_space.bottom),
        )
    }

    /// `getBorderArea` — margin area + margin (verbatim ; widget
    /// sans style → marges 0).
    #[must_use]
    pub(crate) fn get_border_area(&self) -> (i32, i32, i32, i32) {
        let (mut x, mut y, mut w, mut h) = self.get_margin_area();
        if let Some(s) = self.style_attrs() {
            x += s.margin.left;
            y += s.margin.top;
            w -= s.margin.left + s.margin.right;
            h -= s.margin.top + s.margin.bottom;
        }
        (x, y, w, h)
    }

    /// `getPaddingArea` — border area + border widths (verbatim).
    #[must_use]
    pub(crate) fn get_padding_area(&self) -> (i32, i32, i32, i32) {
        let (mut x, mut y, mut w, mut h) = self.get_border_area();
        if let Some(s) = self.style_attrs() {
            x += s.border_width.left;
            y += s.border_width.top;
            w -= s.border_width.left + s.border_width.right;
            h -= s.border_width.top + s.border_width.bottom;
        }
        (x, y, w, h)
    }
}

// ------------------------------------------------------------------
// splitHeight — verbatim widget.cc:2261-2283.
// ------------------------------------------------------------------

/// `splitHeightPreserveAscent`.
pub fn split_height_preserve_ascent(height: i32, ascent: &mut i32, descent: &mut i32) {
    *descent = height - *ascent;
    if *descent < 0 {
        *descent = 0;
        *ascent = height;
    }
}

/// `splitHeightPreserveDescent`.
pub fn split_height_preserve_descent(height: i32, ascent: &mut i32, descent: &mut i32) {
    *ascent = height - *descent;
    if *ascent < 0 {
        *ascent = 0;
        *descent = height;
    }
}

/// `SplitHeightFun` amont (pointeur de fonction).
pub type SplitHeightFun = fn(i32, &mut i32, &mut i32);

// ------------------------------------------------------------------
// Méthodes Widget sur Layout — la suite du port.
// ------------------------------------------------------------------

impl Layout {
    /// Accès lecture à un nœud (helper interne — emprunt court,
    /// jamais tenu pendant un appel récursif).
    #[inline]
    pub(crate) fn node<T>(&self, w: WidgetId, f: impl FnOnce(&WidgetNode) -> T) -> T {
        f(&self.arena.borrow().nodes[w])
    }

    /// Accès mutation à un nœud (même discipline).
    #[inline]
    pub(crate) fn node_mut<T>(&self, w: WidgetId, f: impl FnOnce(&mut WidgetNode) -> T) -> T {
        f(&mut self.arena.borrow_mut().nodes[w])
    }

    /// `Widget::setDeleteCallback`.
    pub fn widget_set_delete_callback(&mut self, w: WidgetId, cb: Option<Rc<dyn Fn()>>) {
        self.node_mut(w, |n| n.delete_callback = cb);
    }

    /// `Widget::getWidgetReference`.
    #[must_use]
    pub fn widget_reference(&self, w: WidgetId) -> Option<WidgetReference> {
        self.node(w, |n| n.widget_reference)
    }

    /// `Widget::setWidgetReference`.
    pub fn widget_set_widget_reference(&mut self, w: WidgetId, wr: Option<WidgetReference>) {
        self.node_mut(w, |n| n.widget_reference = wr);
    }

    /// `Widget::getStyle` — `Option` ≙ `style == NULL` amont.
    #[must_use]
    pub fn widget_style(&self, w: WidgetId) -> Option<Rc<Style>> {
        self.node(w, |n| n.style.clone())
    }

    /// `Widget::getAllocation`.
    #[must_use]
    pub fn widget_allocation(&self, w: WidgetId) -> Allocation {
        self.node(w, |n| n.allocation)
    }

    /// `Widget::getParent` / `getQuasiParent` / `getContainer`.
    #[must_use]
    pub fn widget_parent(&self, w: WidgetId) -> Option<WidgetId> {
        self.node(w, |n| n.parent)
    }
    /// `getQuasiParent`.
    #[must_use]
    pub fn widget_quasi_parent(&self, w: WidgetId) -> Option<WidgetId> {
        self.node(w, |n| n.quasi_parent)
    }
    /// `w->instanceOf (TEXTBLOCK)` — pour le walk de linkReceiver
    /// amont (`html.cc` `HtmlLinkReceiver::link`).
    #[must_use]
    pub fn widget_is_textblock_kind(&self, w: WidgetId) -> bool {
        self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind))
    }
    /// `getContainer`.
    #[must_use]
    pub fn widget_container(&self, w: WidgetId) -> Option<WidgetId> {
        self.node(w, |n| n.container)
    }
    /// `getGenerator` — fallback parent verbatim.
    #[must_use]
    pub fn widget_generator(&self, w: WidgetId) -> Option<WidgetId> {
        self.node(w, WidgetNode::get_generator)
    }

    /// `Widget::setGenerator`.
    pub fn widget_set_generator(&mut self, w: WidgetId, gen: Option<WidgetId>) {
        self.node_mut(w, |n| n.generator = gen);
    }

    /// `Widget::setQuasiParent` — verbatim (simple affectation).
    pub fn widget_set_quasi_parent(&mut self, w: WidgetId, qp: Option<WidgetId>) {
        self.node_mut(w, |n| n.quasi_parent = qp);
    }

    /// `Embed::getResource` — `None` si `w` n'est pas un `Embed`.
    /// `Rc` cloné : les méthodes `Resource` sont `&self` +
    /// ré-entrantes (ui.rs — modèle D).
    #[must_use]
    pub fn embed_resource(&self, w: WidgetId) -> Option<Rc<dyn crate::ui::Resource>> {
        self.node(w, |n| match &n.kind {
            WidgetKind::Embed { res } => Some(res.clone()),
            _ => None,
        })
    }

    /// `new Embed (resource)` — verbatim ui.cc:36 : nœud +
    /// `resource->setEmbed(this)`.
    pub fn add_embed_widget(&self, res: &Rc<dyn crate::ui::Resource>) -> WidgetId {
        let w = self.add_widget_node(WidgetKind::Embed { res: res.clone() });
        res.set_embed(Some(w));
        w
    }

    /// `Widget::getTopLevel` — remonte `parent` jusqu'à la racine.
    #[must_use]
    pub fn widget_top_level(&self, w: WidgetId) -> WidgetId {
        let mut cur = w;
        while let Some(p) = self.node(cur, |n| n.parent) {
            cur = p;
        }
        cur
    }

    /// `Widget::getLevel` — profondeur (racine = 0).
    #[must_use]
    pub fn widget_level(&self, w: WidgetId) -> i32 {
        let mut level = 0;
        let mut cur = w;
        while let Some(p) = self.node(cur, |n| n.parent) {
            level += 1;
            cur = p;
        }
        level
    }

    /// `Widget::getGeneratorLevel` — profondeur via générateurs.
    #[must_use]
    pub fn widget_generator_level(&self, w: WidgetId) -> i32 {
        let mut level = 0;
        let mut cur = w;
        while let Some(g) = self.node(cur, WidgetNode::get_generator) {
            level += 1;
            cur = g;
        }
        level
    }

    /// `Widget::getNearestCommonAncestor` — verbatim (aligne les
    /// niveaux puis remonte en parallèle ; arbres disjoints →
    /// `None` ≙ `NULL` + `MSG_WARN` amont).
    #[must_use]
    pub fn widget_nearest_common_ancestor(&self, w1: WidgetId, w2: WidgetId) -> Option<WidgetId> {
        let mut a = w1;
        let mut b = w2;
        let mut la = self.widget_level(a);
        let mut lb = self.widget_level(b);
        while la > lb {
            a = self.node(a, |n| n.parent).unwrap_or(a);
            la -= 1;
        }
        while lb > la {
            b = self.node(b, |n| n.parent).unwrap_or(b);
            lb -= 1;
        }
        while a != b {
            a = self.node(a, |n| n.parent)?;
            b = self.node(b, |n| n.parent).unwrap_or(b);
        }
        Some(a)
    }

    /// `Widget::setParent` — verbatim widget.cc:218 :
    /// layout hérité, `buttonSensitive` hérité si non fixé,
    /// `container` = plus proche ancêtre `isPossibleContainer`,
    /// stacking : SCM → `addChildSCWidget` (phase OOF —
    /// `has_stacking_context_mgr` remonte le widget SCM sinon
    /// hérite du parent), `notifySetParent`.
    pub fn widget_set_parent(&mut self, w: WidgetId, parent: WidgetId) {
        // Attacher = le widget a un `layout` amont (A05-D4).
        self.arena.borrow_mut().detached.remove(&w);
        self.node_mut(w, |n| n.parent = Some(parent));
        if !self.node(w, |n| n.button_sensitive_set) {
            let ps = self.node(parent, |n| n.button_sensitive);
            self.node_mut(w, |n| n.button_sensitive = ps);
        }

        // `container` — ancêtre `isPossibleContainer` le plus proche.
        let mut container = None;
        let mut cur = Some(parent);
        while let Some(c) = cur {
            if self.w_is_possible_container(c) {
                container = Some(c);
                break;
            }
            cur = self.node(c, |n| n.parent);
        }
        self.node_mut(w, |n| n.container = container);

        // Stacking (verbatim) — `stackingContextMgr` amont non
        // porté : `addChildSCWidget` est différé à la phase OOF ;
        // la remontée/héritage du widget SCM est fidèle.
        if self.node(w, |n| n.has_stacking_context_mgr) {
            let mut scw = Some(parent);
            while let Some(s) = scw {
                if self.node(s, |n| n.has_stacking_context_mgr) {
                    break;
                }
                scw = self.node(s, |n| n.parent);
            }
            // ≙ addChildSCWidget(this) — phase OOF.
            self.node_mut(w, |n| n.stacking_context_widget = scw);
        } else {
            let scw = self.node(parent, |n| n.stacking_context_widget);
            self.node_mut(w, |n| n.stacking_context_widget = scw);
        }

        self.w_notify_set_parent(w);
    }

    /// `Widget::scrollTo` — délègue à `Layout::scroll_to` en
    /// coords canvas (widget.cc:1630).
    #[allow(clippy::too_many_arguments)]
    pub fn widget_scroll_to(
        &mut self,
        w: WidgetId,
        hpos: HPosition,
        vpos: VPosition,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        let a = self.node(w, |n| n.allocation);
        self.scroll_to(hpos, vpos, x + a.x, y + a.y, width, height);
    }

    /// `Widget::setButtonSensitive`.
    pub fn widget_set_button_sensitive(&mut self, w: WidgetId, v: bool) {
        self.node_mut(w, |n| {
            n.button_sensitive = v;
            n.button_sensitive_set = true;
        });
    }

    /// `Widget::isButtonSensitive`.
    #[must_use]
    pub fn widget_is_button_sensitive(&self, w: WidgetId) -> bool {
        self.node(w, |n| n.button_sensitive)
    }

    /// `Widget::intersects` — verbatim widget.cc:146 : intersection
    /// de `area` (coords `ref_widget`) avec les allocations
    /// visibles de `w` à `ref_widget`, retournée en coords `w`.
    /// `ref_widget` doit être un ancêtre.
    #[must_use]
    pub fn widget_intersects(
        &self,
        w: WidgetId,
        ref_widget: WidgetId,
        area: &crate::types::Rectangle,
    ) -> Option<crate::types::Rectangle> {
        if !self.node(w, WidgetNode::was_allocated) {
            return None;
        }
        let ref_alloc = self.node(ref_widget, |n| n.allocation);
        let mut intersection = crate::types::Rectangle {
            x: area.x + ref_alloc.x,
            y: area.y + ref_alloc.y,
            width: area.width,
            height: area.height,
        };
        let mut cur = w;
        let mut ok = true;
        while ok && cur != ref_widget {
            let wa = self.node(cur, |n| crate::types::Rectangle {
                x: n.allocation.x,
                y: n.allocation.y,
                width: n.allocation.width,
                height: n.get_height(),
            });
            match intersection.intersects_with(&wa) {
                Some(i) => intersection = i,
                None => ok = false,
            }
            match self.node(cur, |n| n.parent) {
                Some(p) => cur = p,
                None => break,
            }
        }
        if ok {
            let alloc = self.node(w, |n| n.allocation);
            intersection.x -= alloc.x;
            intersection.y -= alloc.y;
            Some(intersection)
        } else {
            None
        }
    }
}

// ------------------------------------------------------------------
// queueDraw / queueResize / mark* / containerSizeChanged —
// widget.cc:270-528, verbatim.
// ------------------------------------------------------------------

impl Layout {
    /// `Widget::queueDraw` inline — zone entière du widget.
    pub fn widget_queue_draw(&mut self, w: WidgetId) {
        let (w2, h) = self.node(w, |n| (n.allocation.width, n.get_height()));
        self.widget_queue_draw_area(w, 0, 0, w2, h);
    }

    /// `Widget::queueDrawArea` — `x..` coords widget → canvas,
    /// délègue `Layout::queueDraw` (widget.cc:270).
    pub fn widget_queue_draw_area(&mut self, w: WidgetId, x: i32, y: i32, width: i32, height: i32) {
        let a = self.node(w, |n| n.allocation);
        self.queue_draw(x + a.x, y + a.y, width, height);
    }

    /// `Widget::queueResize (ref, extremesChanged)` — public.
    pub fn widget_queue_resize(&mut self, w: WidgetId, ref_: i32, extremes_changed: bool) {
        self.widget_queue_resize_inner(w, ref_, extremes_changed, false);
    }

    /// `Widget::queueResizeFast` — depuis `viewportSizeChanged`
    /// (ignore les ancêtres et ne déclenche pas l'idle).
    pub fn widget_queue_resize_fast(&mut self, w: WidgetId, ref_: i32, extremes_changed: bool) {
        self.widget_queue_resize_inner(w, ref_, extremes_changed, true);
    }

    /// `Widget::queueResize (ref, extremesChanged, fast)` —
    /// verbatim widget.cc:288.
    fn widget_queue_resize_inner(
        &mut self,
        w: WidgetId,
        ref_: i32,
        extremes_changed: bool,
        fast: bool,
    ) {
        self.enter_queue_resize();

        // `if (layout)` amont ≙ `!detached` : pas encore rattaché
        // (`add_widget_node`/`set_parent`/`add_widget`) ou détaché
        // (`detach_widget`, destruction) → branche NEEDS_*/
        // EXTREMES_CHANGED, sans liste ni idle (verbatim — A05-D4).
        let detached = self.node_detached(w);
        let (resize_flag, extremes_flag) = if detached {
            (NEEDS_RESIZE, EXTREMES_CHANGED)
        } else {
            if !self.node(w, WidgetNode::resize_queued) {
                self.queue_resize_list_put(w);
            }
            (RESIZE_QUEUED, EXTREMES_QUEUED)
        };

        self.node_mut(w, |n| {
            n.set_flags(resize_flag);
            n.set_flags(ALLOCATE_QUEUED);
        });
        self.w_mark_size_change(w, ref_);

        let mut total_flags = resize_flag;
        if extremes_changed {
            total_flags |= extremes_flag;
            self.node_mut(w, |n| n.set_flags(extremes_flag));
            self.w_mark_extremes_change(w, ref_);
        }

        if fast {
            if let Some(p) = self.node(w, |n| n.parent) {
                let parent_ref = self.node(w, |n| n.parent_ref);
                if self.node(p, |n| n.needs_resize() || n.resize_queued()) {
                    self.w_mark_size_change(p, parent_ref);
                }
                if self.node(p, |n| n.extremes_changed() || n.extremes_queued()) {
                    self.w_mark_extremes_change(p, parent_ref);
                }
            }
        } else {
            let mut child = w;
            let mut cur = self.node(w, |n| n.parent);
            while let Some(w2) = cur {
                if !detached && !self.node(w2, WidgetNode::resize_queued) {
                    self.queue_resize_list_put(w2);
                }
                let child_parent_ref = self.node(child, |n| n.parent_ref);
                // Ordre verbatim : `resizeFlag` → `markSizeChange` →
                // `ALLOCATE_QUEUED` (widget.cc:382-386 — A05-D5).
                self.node_mut(w2, |n| n.set_flags(resize_flag));
                self.w_mark_size_change(w2, child_parent_ref);
                self.node_mut(w2, |n| n.set_flags(ALLOCATE_QUEUED));
                if extremes_changed {
                    self.node_mut(w2, |n| n.set_flags(extremes_flag));
                    self.w_mark_extremes_change(w2, child_parent_ref);
                }
                if let Some(gp) = self.node(w2, |n| n.parent) {
                    if self.node(gp, |n| n.flags & total_flags == total_flags) {
                        let gp_ref = self.node(w2, |n| n.parent_ref);
                        self.w_mark_size_change(gp, gp_ref);
                        if extremes_changed {
                            self.w_mark_extremes_change(gp, gp_ref);
                        }
                        break;
                    }
                }
                child = w2;
                cur = self.node(w2, |n| n.parent);
            }
            if !detached {
                self.queue_resize(extremes_changed);
            }
        }

        self.leave_queue_resize();
    }

    /// `Widget::containerSizeChanged` — verbatim widget.cc:420 :
    /// propage si `container` nul (viewport) ou déjà marqué.
    pub fn widget_container_size_changed(&mut self, w: WidgetId) {
        let container = self.node(w, |n| n.container);
        let go = match container {
            None => true,
            Some(c) => self.node(c, |n| {
                n.needs_resize() || n.resize_queued() || n.extremes_changed() || n.extremes_queued()
            }),
        };
        if go {
            if self.w_affected_by_container_size_change(w) {
                self.widget_queue_resize_fast(w, 0, true);
            }
            self.w_container_size_changed_for_children(w);
        }
    }
}

// ------------------------------------------------------------------
// sizeRequest / getExtremes / calcExtraSpace / sizeAllocate —
// widget.cc:539-1244, verbatim.
// ------------------------------------------------------------------

impl Layout {
    /// `Widget::sizeRequest` — wrapper de `sizeRequestImpl` avec
    /// cache + `SizeParams` équivalence (widget.cc:539).
    pub fn widget_size_request(
        &mut self,
        w: WidgetId,
        requisition: &mut Requisition,
        num_pos: usize,
        references: &[WidgetId],
        x: &[i32],
        y: &[i32],
    ) {
        self.enter_size_request();

        if self.node(w, WidgetNode::resize_queued) {
            self.node_mut(w, |n| {
                n.set_flags(NEEDS_RESIZE);
                n.unset_flags(RESIZE_QUEUED);
            });
        }

        let new_params = SizeParams::with(
            &references[..num_pos.min(references.len())],
            &x[..num_pos.min(x.len())],
            &y[..num_pos.min(y.len())],
        );
        let call_impl = if self.node(w, WidgetNode::needs_resize) {
            true
        } else {
            !self.node(w, |n| new_params.is_equivalent(&n.requisition_params))
        };
        self.node_mut(w, |n| n.requisition_params = new_params.clone());

        if call_impl {
            self.widget_calc_extra_space(w, num_pos, references, x, y);
            self.w_size_request_impl(w, requisition, num_pos, references, x, y);
            self.node_mut(w, |n| {
                n.requisition = *requisition;
                n.unset_flags(NEEDS_RESIZE);
            });
        } else {
            *requisition = self.node(w, |n| n.requisition);
        }

        self.leave_size_request();
    }

    /// `Widget::getExtremes` — wrapper de `getExtremesImpl`
    /// (widget.cc:1134).
    pub fn widget_get_extremes(
        &mut self,
        w: WidgetId,
        extremes: &mut Extremes,
        num_pos: usize,
        references: &[WidgetId],
        x: &[i32],
        y: &[i32],
    ) {
        self.enter_get_extremes();

        if self.node(w, WidgetNode::extremes_queued) {
            self.node_mut(w, |n| {
                n.set_flags(EXTREMES_CHANGED);
                n.unset_flags(EXTREMES_QUEUED);
            });
        }

        let call_impl = if self.node(w, WidgetNode::extremes_changed) {
            true
        } else {
            let new_params = SizeParams::with(
                &references[..num_pos.min(references.len())],
                &x[..num_pos.min(x.len())],
                &y[..num_pos.min(y.len())],
            );
            if self.node(w, |n| new_params.is_equivalent(&n.extremes_params)) {
                false
            } else {
                self.node_mut(w, |n| n.extremes_params = new_params);
                true
            }
        };

        if call_impl {
            // Compat amont part 1/2 : intrinsic = -1.
            extremes.min_width_intrinsic = -1;
            extremes.max_width_intrinsic = -1;
            self.w_get_extremes_impl(w, extremes, num_pos, references, x, y);
            // part 2/2.
            if extremes.min_width_intrinsic == -1 {
                extremes.min_width_intrinsic = extremes.min_width;
            }
            if extremes.max_width_intrinsic == -1 {
                extremes.max_width_intrinsic = extremes.max_width;
            }
            self.node_mut(w, |n| {
                n.extremes = *extremes;
                n.unset_flags(EXTREMES_CHANGED);
            });
        } else {
            *extremes = self.node(w, |n| n.extremes);
        }

        self.leave_get_extremes();
    }

    /// `Widget::calcExtraSpace` — reset puis `calcExtraSpaceImpl`
    /// (widget.cc:1159).
    pub fn widget_calc_extra_space(
        &mut self,
        w: WidgetId,
        num_pos: usize,
        references: &[WidgetId],
        x: &[i32],
        y: &[i32],
    ) {
        self.node_mut(w, |n| n.extra_space.set_val(0));
        self.w_calc_extra_space_impl(w, num_pos, references, x, y);
    }

    /// `Widget::sizeAllocate` — verbatim widget.cc:1197 :
    /// queueDrawExcept ancienne/nouvelle zone, `sizeAllocateImpl`,
    /// flags, `resizeDrawImpl`.
    pub fn widget_size_allocate(&mut self, w: WidgetId, allocation: &Allocation) {
        debug_assert!(self.resize_idle_entered());
        self.enter_size_allocate();

        let old = self.node(w, |n| n.allocation);
        let changed = self.node(w, WidgetNode::needs_allocate)
            || allocation.x != old.x
            || allocation.y != old.y
            || allocation.width != old.width
            || allocation.ascent != old.ascent
            || allocation.descent != old.descent;

        if changed {
            if self.node(w, WidgetNode::was_allocated) {
                self.queue_draw_except(
                    old.x,
                    old.y,
                    old.width,
                    old.ascent + old.descent,
                    allocation.x,
                    allocation.y,
                    allocation.width,
                    allocation.ascent + allocation.descent,
                );
            }
            self.w_size_allocate_impl(w, allocation);
            self.node_mut(w, |n| {
                n.allocation = *allocation;
                n.unset_flags(NEEDS_ALLOCATE);
                n.set_flags(WAS_ALLOCATED);
            });
            self.w_resize_draw_impl(w);
        }

        self.leave_size_allocate();
    }

    /// `Widget::getMinWidth` — verbatim widget.cc:584 : si
    /// `adjustMinWidth` actif, `extremes.adjustmentWidth` (0
    /// si `getExtremes` non appelable / pas de extremes).
    #[must_use]
    pub fn widget_get_min_width(
        &mut self,
        w: WidgetId,
        extremes: Option<&Extremes>,
        force_value: bool,
    ) -> i32 {
        if !self.w_get_adjust_min_width(w) {
            return 0;
        }
        if let Some(e) = extremes {
            return e.adjustment_width;
        }
        if force_value {
            let mut e2 = Extremes::default();
            self.widget_get_extremes(w, &mut e2, 0, &[], &[], &[]);
            e2.adjustment_width
        } else {
            0
        }
    }

    /// `Widget::getAvailWidth` — verbatim widget.cc:616 :
    /// viewport / parent / quasiParent.
    #[must_use]
    pub fn widget_get_avail_width(&mut self, w: WidgetId, force_value: bool) -> i32 {
        let (parent, quasi) = self.node(w, |n| (n.parent, n.quasi_parent));
        if parent.is_none() && quasi.is_none() {
            // Viewport.
            let viewport_width = self.viewport_width
                - if self.canvas_height_greater {
                    self.v_scrollbar_thickness
                } else {
                    0
                };
            let mut width = viewport_width;
            let style = self.node(w, |n| n.style.clone());
            if let Some(s) = &style {
                self.widget_calc_final_width(
                    w,
                    s,
                    viewport_width,
                    None,
                    0,
                    force_value,
                    &mut width,
                );
            }
            width
        } else if let Some(p) = parent {
            self.w_get_avail_width_of_child(p, w, force_value)
        } else {
            let qp = quasi.unwrap_or(w);
            self.w_get_avail_width_of_child(qp, w, force_value)
        }
    }

    /// `Widget::getAvailHeight` — verbatim widget.cc:660.
    #[must_use]
    pub fn widget_get_avail_height(&mut self, w: WidgetId, force_value: bool) -> i32 {
        let (parent, quasi) = self.node(w, |n| (n.parent, n.quasi_parent));
        if parent.is_none() && quasi.is_none() {
            let style = self.node(w, |n| n.style.clone());
            if let Some(s) = &style {
                if is_abs_length(s.height) {
                    abs_length_val(s.height) + self.node(w, WidgetNode::box_diff_height)
                } else if is_per_length(s.height) {
                    // `hScrollbarThickness` non considéré (verbatim).
                    self.w_apply_per_height(w, self.viewport_height, s.height)
                } else if force_value {
                    self.viewport_height
                } else {
                    -1
                }
            } else if force_value {
                self.viewport_height
            } else {
                -1
            }
        } else if let Some(p) = parent {
            self.w_get_avail_height_of_child(p, w, force_value)
        } else {
            let qp = quasi.unwrap_or(w);
            self.w_get_avail_height_of_child(qp, w, force_value)
        }
    }

    /// `Widget::correctRequisitionViewport` — verbatim
    /// widget.cc:724 (top-level : correction par le viewport).
    pub fn widget_correct_requisition_viewport(
        &mut self,
        w: WidgetId,
        requisition: &mut Requisition,
        split_height_fun: SplitHeightFun,
        allow_decrease_width: bool,
        allow_decrease_height: bool,
    ) {
        let mut limit_min_width = self.widget_get_min_width(w, None, true);
        if !allow_decrease_width && limit_min_width < requisition.width {
            limit_min_width = requisition.width;
        }
        let viewport_width = self.viewport_width
            - if self.canvas_height_greater {
                self.v_scrollbar_thickness
            } else {
                0
            };
        let style = self.node(w, |n| n.style.clone());
        let Some(s) = style else { return };
        self.widget_calc_final_width(
            w,
            &s,
            viewport_width,
            None,
            limit_min_width,
            false,
            &mut requisition.width,
        );

        let mut height =
            self.widget_calc_height(w, s.height, false, self.viewport_height, None, false);
        Self::adjust_height(
            &mut height,
            allow_decrease_height,
            requisition.ascent,
            requisition.descent,
        );
        let mut min_height =
            self.widget_calc_height(w, s.min_height, false, self.viewport_height, None, false);
        let mut max_height =
            self.widget_calc_height(w, s.max_height, false, self.viewport_height, None, false);
        if min_height != -1 && max_height != -1 && max_height < min_height {
            max_height = min_height;
        }
        Self::adjust_height(
            &mut min_height,
            allow_decrease_height,
            requisition.ascent,
            requisition.descent,
        );
        Self::adjust_height(
            &mut max_height,
            allow_decrease_height,
            requisition.ascent,
            requisition.descent,
        );
        if height != -1 {
            split_height_fun(height, &mut requisition.ascent, &mut requisition.descent);
        }
        if min_height != -1 && requisition.ascent + requisition.descent < min_height {
            split_height_fun(
                min_height,
                &mut requisition.ascent,
                &mut requisition.descent,
            );
        }
        if max_height != -1 && requisition.ascent + requisition.descent > max_height {
            split_height_fun(
                max_height,
                &mut requisition.ascent,
                &mut requisition.descent,
            );
        }
    }

    /// `Widget::correctRequisition` — verbatim widget.cc:776 :
    /// viewport (3 passes aspect-ratio) / parent / quasiParent.
    pub fn widget_correct_requisition(
        &mut self,
        w: WidgetId,
        requisition: &mut Requisition,
        split_height_fun: SplitHeightFun,
        allow_decrease_width: bool,
        allow_decrease_height: bool,
    ) {
        let (parent, quasi) = self.node(w, |n| (n.parent, n.quasi_parent));
        if parent.is_none() && quasi.is_none() {
            for pass in 0..3 {
                self.widget_correct_requisition_viewport(
                    w,
                    requisition,
                    split_height_fun,
                    allow_decrease_width,
                    allow_decrease_height,
                );
                let changed = self.widget_correct_req_aspect_ratio(
                    pass,
                    w,
                    w,
                    requisition,
                    allow_decrease_width,
                    allow_decrease_height,
                    split_height_fun,
                );
                if !changed {
                    break;
                }
            }
        } else if let Some(p) = parent {
            self.w_correct_requisition_of_child(
                p,
                w,
                requisition,
                split_height_fun,
                allow_decrease_width,
                allow_decrease_height,
            );
        } else {
            let qp = quasi.unwrap_or(w);
            self.w_correct_requisition_of_child(
                qp,
                w,
                requisition,
                split_height_fun,
                allow_decrease_width,
                allow_decrease_height,
            );
        }
    }

    /// `Widget::correctExtremes` — verbatim widget.cc:818.
    pub fn widget_correct_extremes(
        &mut self,
        w: WidgetId,
        extremes: &mut Extremes,
        use_adjustment_width: bool,
    ) {
        let (parent, quasi, container) = self.node(w, |n| (n.parent, n.quasi_parent, n.container));
        if container.is_none() && quasi.is_none() {
            // Viewport (verbatim : `container == NULL`, pas parent).
            let limit_min_width = if use_adjustment_width {
                self.widget_get_min_width(w, Some(extremes), false)
            } else {
                0
            };
            let viewport_width = self.viewport_width
                - if self.canvas_height_greater {
                    self.v_scrollbar_thickness
                } else {
                    0
                };
            let style = self.node(w, |n| n.style.clone());
            if let Some(s) = &style {
                let width = self.widget_calc_width(
                    w,
                    s.width,
                    viewport_width,
                    None,
                    limit_min_width,
                    false,
                );
                let min_width = self.widget_calc_width(
                    w,
                    s.min_width,
                    viewport_width,
                    None,
                    limit_min_width,
                    false,
                );
                let max_width = self.widget_calc_width(
                    w,
                    s.max_width,
                    viewport_width,
                    None,
                    limit_min_width,
                    false,
                );
                if width != -1 {
                    extremes.min_width = width;
                    extremes.max_width = width;
                }
                if min_width != -1 {
                    extremes.min_width = min_width;
                }
                if max_width != -1 {
                    extremes.max_width = max_width;
                }
            }
        } else if let Some(p) = parent {
            self.w_correct_extremes_of_child(p, w, extremes, use_adjustment_width);
        } else if let Some(qp) = quasi {
            self.w_correct_extremes_of_child(qp, w, extremes, use_adjustment_width);
        }
        if extremes.max_width < extremes.min_width {
            extremes.max_width = extremes.min_width;
        }
    }

    /// `Widget::calcWidth` — verbatim widget.cc:881.
    #[must_use]
    pub fn widget_calc_width(
        &mut self,
        w: WidgetId,
        css_value: style::Length,
        ref_width: i32,
        ref_widget: Option<WidgetId>,
        limit_min_width: i32,
        force_value: bool,
    ) -> i32 {
        debug_assert!(ref_width != -1 || ref_widget.is_some());
        if is_abs_length(css_value) {
            (abs_length_val(css_value) + self.node(w, WidgetNode::box_diff_width))
                .max(limit_min_width)
        } else if is_per_length(css_value) {
            if ref_width == -1 {
                let rw = ref_widget.unwrap_or(w);
                let avail_width = self.widget_get_avail_width(rw, force_value);
                if avail_width == -1 {
                    -1
                } else {
                    let container_width = avail_width - self.node(rw, WidgetNode::box_diff_width);
                    self.w_apply_per_width(w, container_width, css_value)
                        .max(limit_min_width)
                }
            } else {
                self.w_apply_per_width(w, ref_width, css_value)
                    .max(limit_min_width)
            }
        } else {
            -1
        }
    }

    /// `Widget::calcFinalWidth` — verbatim widget.cc:933 :
    /// width + clamps min/max (postcondition `w != -1` préservée
    /// : si `*final_width` initial != -1, le résultat != -1).
    #[allow(clippy::too_many_arguments)]
    pub fn widget_calc_final_width(
        &mut self,
        w: WidgetId,
        style_: &Style,
        ref_width: i32,
        ref_widget: Option<WidgetId>,
        limit_min_width: i32,
        force_value: bool,
        final_width: &mut i32,
    ) {
        let mut wid = *final_width;
        let width = self.widget_calc_width(
            w,
            style_.width,
            ref_width,
            ref_widget,
            limit_min_width,
            force_value,
        );
        if width != -1 {
            wid = width;
        }
        if wid != -1 {
            let min_width = self.widget_calc_width(
                w,
                style_.min_width,
                ref_width,
                ref_widget,
                limit_min_width,
                force_value,
            );
            let mut max_width = self.widget_calc_width(
                w,
                style_.max_width,
                ref_width,
                ref_widget,
                limit_min_width,
                force_value,
            );
            if min_width != -1 && max_width != -1 && max_width < min_width {
                max_width = min_width;
            }
            if min_width != -1 && wid < min_width {
                wid = min_width;
            }
            if max_width != -1 && wid > max_width {
                wid = max_width;
            }
        }
        debug_assert!(*final_width == -1 || wid != -1);
        *final_width = wid;
    }

    /// `Widget::calcHeight` — verbatim widget.cc:993.
    #[must_use]
    pub fn widget_calc_height(
        &mut self,
        w: WidgetId,
        css_value: style::Length,
        use_percentage: bool,
        ref_height: i32,
        ref_widget: Option<WidgetId>,
        force_value: bool,
    ) -> i32 {
        debug_assert!(ref_height != -1 || ref_widget.is_some());
        if is_abs_length(css_value) {
            (abs_length_val(css_value) + self.node(w, WidgetNode::box_diff_height)).max(0)
        } else if is_per_length(css_value) {
            if !use_percentage {
                -1
            } else if ref_height == -1 {
                let rw = ref_widget.unwrap_or(w);
                let avail_height = self.widget_get_avail_height(rw, force_value);
                if avail_height == -1 {
                    -1
                } else {
                    let container_height =
                        avail_height - self.node(rw, WidgetNode::box_diff_height);
                    self.w_apply_per_height(w, container_height, css_value)
                        .max(0)
                }
            } else {
                self.w_apply_per_height(w, ref_height, css_value).max(0)
            }
        } else {
            -1
        }
    }

    /// `Widget::adjustHeight` — verbatim widget.cc:1039.
    pub fn adjust_height(height: &mut i32, allow_decrease_height: bool, ascent: i32, descent: i32) {
        if !allow_decrease_height && *height != -1 && *height < ascent + descent {
            *height = ascent + descent;
        }
    }

    /// `Widget::correctReqAspectRatio` — verbatim
    /// widget.cc:1960 (passes increase/decrease/keep, 3 passes
    /// max orchestrées par l'appelant).
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn widget_correct_req_aspect_ratio(
        &mut self,
        pass: i32,
        _caller: WidgetId,
        child: WidgetId,
        requisition: &mut Requisition,
        allow_decrease_width: bool,
        allow_decrease_height: bool,
        split_height_fun: SplitHeightFun,
    ) -> bool {
        let w_req0 = requisition.width;
        let h_req0 = requisition.ascent + requisition.descent;
        let size_set = w_req0 > 0 && h_req0 > 0;
        let ratio = self.node(child, |n| n.ratio);
        let mut changed = false;

        let mut w_req = w_req0 - self.node(child, WidgetNode::box_diff_width);
        let mut h_req = h_req0 - self.node(child, WidgetNode::box_diff_height);

        if pass != PASS_KEEP && ratio > 0.0 && size_set {
            #[allow(clippy::cast_precision_loss)]
            let cur_ratio = w_req as f32 / h_req as f32;
            if cur_ratio < ratio {
                if pass == PASS_INCREASE {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        w_req = (h_req as f32 * ratio) as i32;
                    }
                    w_req += self.node(child, WidgetNode::box_diff_width);
                    requisition.width = w_req;
                    changed = true;
                } else if pass == PASS_DECREASE && allow_decrease_height {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        h_req = (w_req as f32 / ratio) as i32;
                    }
                    h_req += self.node(child, WidgetNode::box_diff_height);
                    split_height_fun(h_req, &mut requisition.ascent, &mut requisition.descent);
                    changed = true;
                }
            } else if cur_ratio > ratio {
                if pass == PASS_INCREASE {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        h_req = (w_req as f32 / ratio) as i32;
                    }
                    h_req += self.node(child, WidgetNode::box_diff_height);
                    split_height_fun(h_req, &mut requisition.ascent, &mut requisition.descent);
                    changed = true;
                } else if pass == PASS_DECREASE && allow_decrease_width {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        w_req = (h_req as f32 * ratio) as i32;
                    }
                    w_req += self.node(child, WidgetNode::box_diff_width);
                    requisition.width = w_req;
                    changed = true;
                }
            }
        }
        changed
    }

    /// `Widget::correctReqWidthOfChild` — verbatim
    /// widget.cc:2077 (`self` = parent/`effContainer`).
    pub fn widget_correct_req_width_of_child(
        &mut self,
        parent: WidgetId,
        child: WidgetId,
        requisition: &mut Requisition,
        allow_decrease_width: bool,
    ) {
        let mut limit_min_width = self.widget_get_min_width(child, None, true);
        if !allow_decrease_width && limit_min_width < requisition.width {
            limit_min_width = requisition.width;
        }
        let style = self.node(child, |n| n.style.clone());
        if let Some(s) = &style {
            self.widget_calc_final_width(
                child,
                s,
                -1,
                Some(parent),
                limit_min_width,
                true,
                &mut requisition.width,
            );
        }
    }

    /// `Widget::correctReqHeightOfChild` — verbatim
    /// widget.cc:2108.
    pub fn widget_correct_req_height_of_child(
        &mut self,
        parent: WidgetId,
        child: WidgetId,
        requisition: &mut Requisition,
        split_height_fun: SplitHeightFun,
        allow_decrease_height: bool,
    ) {
        let style = self.node(child, |n| n.style.clone());
        let Some(s) = style else { return };
        let mut height = self.widget_calc_height(child, s.height, true, -1, Some(parent), false);
        Self::adjust_height(
            &mut height,
            allow_decrease_height,
            requisition.ascent,
            requisition.descent,
        );
        let mut min_height =
            self.widget_calc_height(child, s.min_height, true, -1, Some(parent), false);
        let mut max_height =
            self.widget_calc_height(child, s.max_height, true, -1, Some(parent), false);
        if min_height != -1 && max_height != -1 && max_height < min_height {
            max_height = min_height;
        }
        Self::adjust_height(
            &mut min_height,
            allow_decrease_height,
            requisition.ascent,
            requisition.descent,
        );
        Self::adjust_height(
            &mut max_height,
            allow_decrease_height,
            requisition.ascent,
            requisition.descent,
        );
        if height != -1 {
            split_height_fun(height, &mut requisition.ascent, &mut requisition.descent);
        }
        if min_height != -1 && requisition.ascent + requisition.descent < min_height {
            split_height_fun(
                min_height,
                &mut requisition.ascent,
                &mut requisition.descent,
            );
        }
        if max_height != -1 && requisition.ascent + requisition.descent > max_height {
            split_height_fun(
                max_height,
                &mut requisition.ascent,
                &mut requisition.descent,
            );
        }
    }
}

// ------------------------------------------------------------------
// Dispatch des méthodes virtuelles amont — `WidgetKind`.
// Impls par défaut = celles de `dw::core::Widget` (verbatim).
// ------------------------------------------------------------------

impl Layout {
    /// Seam enfants pour `containerSizeChangedForChildren` et
    /// `detachWidget` — ≙ itération `iterator(WIDGET_IN_FLOW |
    /// WIDGET_OOF_CONT)` (iterator.cc en attente ; les kinds
    /// concrets exposeront leur contenu enfant).
    pub(crate) fn widget_children_for_container_change(&self, w: WidgetId) -> Vec<WidgetId> {
        self.node(w, |n| match &n.kind {
            WidgetKind::Container { children } => children.clone(),
            // `Textblock` : les enfants in-flow sont les widgets
            // des mots `WIDGET_IN_FLOW` (≙ itérateur textblock,
            // masque `WIDGET_IN_FLOW`).
            WidgetKind::Textblock { tb } => tb
                .words
                .iter()
                .filter_map(|word| {
                    if word.content.r#type == content_type::WIDGET_IN_FLOW {
                        if let ContentData::Widget(c) = word.content.data {
                            return Some(c);
                        }
                    }
                    None
                })
                .collect(),
            // `OofAware` : les kinds OOF-aware notifient les
            // enfants OOF via `containerSizeChangedForChildrenOOF`
            // (contrat oofawarewidget.hh:33) — ici in-flow
            // seulement (vide pour le kind nu).
            WidgetKind::Base
            | WidgetKind::Text { .. }
            | WidgetKind::OofAware { .. }
            | WidgetKind::Bullet
            | WidgetKind::Image { .. }
            | WidgetKind::Ruler { .. }
            // `Embed` : pas d'enfants Widget (`childWidget` vit
            // dans le layout interne — ui.cc).
            | WidgetKind::Embed { .. } => Vec::new(),
            // `Table` : les enfants in-flow sont les widgets des
            // cellules `Child::CELL` (≙ itérateur table, masque
            // `WIDGET_IN_FLOW`).
            WidgetKind::Table { tbl } => tbl
                .children
                .iter()
                .filter_map(|c| match c {
                    Some(crate::table::Child::Cell(cell)) => Some(cell.widget),
                    _ => None,
                })
                .collect(),
        })
    }

    /// Seam enfants pour `detachWidget`/`widget_delete` — ≙
    /// itération amont `WIDGET_IN_FLOW | WIDGET_OOF_CONT`
    /// (sections OOF de l'`OOFAwareWidgetIterator` incluses).
    pub(crate) fn widget_children_for_detach(&self, w: WidgetId) -> Vec<WidgetId> {
        let mut out = self.widget_children_for_container_change(w);
        if let Some(oof) = self.node_oof(w, |o| {
            let mut widgets = Vec::new();
            for mgr in o.out_of_flow_mgr.iter().flatten() {
                for i in 0..mgr.borrow().get_num_widgets() {
                    if let Some(c) = mgr.borrow().get_widget(i) {
                        widgets.push(c);
                    }
                }
            }
            widgets
        }) {
            out.extend(oof);
        }
        out
    }

    /// `Widget::iterator` — dispatch `WidgetKind` :
    /// `Base` → `EmptyIterator` (≙ widget sans contenu amont),
    /// `Container` → `ChildIterator` sur `children` (seam —
    /// les kinds concrets exposeront leur itérateur dédié).
    #[must_use]
    pub fn w_iterator(
        &self,
        w: WidgetId,
        mask: i16,
        at_end: bool,
    ) -> Box<dyn crate::iterator::DwIterator> {
        enum Dispatch {
            Child(Vec<WidgetId>),
            Text(Rc<[u8]>),
            OofAware,
            Textblock,
            Table,
            Embed(Rc<dyn crate::ui::Resource>),
            Empty,
        }
        match self.node(w, |n| match &n.kind {
            WidgetKind::Container { children } => Dispatch::Child(children.clone()),
            WidgetKind::Text { text } => Dispatch::Text(text.clone()),
            WidgetKind::OofAware { .. } => Dispatch::OofAware,
            // `Textblock` : `TextblockIterator` verbatim —
            // `OofAwareIterator` + `TbInFlow` (section en-flux =
            // mots — textblock_iterator.cc).
            WidgetKind::Textblock { .. } => Dispatch::Textblock,
            // `Bullet::iterator` — `EmptyIterator` verbatim
            // (`\bug Not implemented` amont) ;
            // `Ruler::iterator` — `EmptyIterator` (`\todo
            // TextIterator?` amont) ; `Image::iterator` —
            // `EmptyIterator` (`\bug Not implemented`,
            // image.cc:497-502).
            // `Embed::iterator` — `resource->iterator` verbatim
            // (ui.cc:137) ; les impls amont retournent toutes
            // `EmptyIterator` (`ug Not implemented`).
            WidgetKind::Embed { res } => Dispatch::Embed(res.clone()),
            // `Table::iterator` — `TableIterator` verbatim
            // (`OofAwareIterator` + section en-flux = cases
            // `children`, cellules `CELL` → `WIDGET_IN_FLOW`,
            // table_iterator.cc).
            WidgetKind::Table { .. } => Dispatch::Table,
            WidgetKind::Base
            | WidgetKind::Bullet
            | WidgetKind::Image { .. }
            | WidgetKind::Ruler { .. } => Dispatch::Empty,
        }) {
            Dispatch::Child(children) => Box::new(crate::iterator::ChildIterator::new(
                w,
                mask,
                at_end,
                children.into(),
            )),
            Dispatch::Text(text) => {
                Box::new(crate::iterator::TextIterator::new(w, mask, at_end, &text))
            }
            Dispatch::OofAware => Box::new(crate::oof::OofAwareIterator::new(
                w,
                mask,
                at_end,
                Box::new(crate::oof::EmptyInFlow),
                self,
                0,
            )),
            Dispatch::Textblock => Box::new(crate::textblock::iter::textblock_iterator(
                self, w, mask, at_end,
            )),
            Dispatch::Table => Box::new(crate::table::table_iterator(self, w, mask, at_end)),
            Dispatch::Embed(res) => res.iterator(mask, at_end),
            Dispatch::Empty => Box::new(crate::iterator::EmptyIterator::new(w, mask, at_end)),
        }
    }

    /// `Widget::sizeRequestImpl` — délègue à la variante simple
    /// (widget.cc:1693 verbatim). `OofAware` : +
    /// `correctRequisitionByOOF` (contrat oofawarewidget.hh:28 ;
    /// split « tout en ascent » pour le kind nu — seam).
    pub(crate) fn w_size_request_impl(
        &mut self,
        w: WidgetId,
        requisition: &mut Requisition,
        num_pos: usize,
        references: &[WidgetId],
        x: &[i32],
        y: &[i32],
    ) {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            let _ = self.with_tb(w, |l, tb| {
                l.tb_size_request_impl(w, tb, requisition, references, x, y);
            });
            return;
        }
        let _ = (num_pos, references, x, y);
        self.w_size_request_simpl(w, requisition);
        if self.node(w, |n| crate::oof::is_oof_aware_kind(&n.kind)) {
            self.correct_requisition_by_oof(w, requisition, |h, a, d| {
                *a = h;
                *d = 0;
            });
        }
    }

    /// `Widget::sizeRequestSimpl` — `notImplemented` amont
    /// (`MSG_ERR` + retour) → no-op documenté : `Base` ne doit pas
    /// être mesuré. `Bullet`/`Ruler` : `*Simpl` verbatim.
    pub(crate) fn w_size_request_simpl(&mut self, w: WidgetId, requisition: &mut Requisition) {
        // `Embed::sizeRequestSimpl` — verbatim ui.cc:50 :
        // `resource->sizeRequest` + `correctRequisition
        // (preserveAscent, true, true)`.
        if let Some(res) = self.embed_resource(w) {
            res.size_request(self, requisition);
            self.widget_correct_requisition(
                w,
                requisition,
                split_height_preserve_ascent,
                true,
                true,
            );
            return;
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Bullet)) {
            self.bullet_size_request_simpl(w, requisition);
        } else if self.node(w, |n| matches!(&n.kind, WidgetKind::Ruler { .. })) {
            self.ruler_size_request_simpl(w, requisition);
        } else if self.node(w, |n| matches!(&n.kind, WidgetKind::Image { .. })) {
            self.img_size_request_simpl(w, requisition);
        } else if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            self.tbl_size_request_simpl(w, requisition);
        }
    }

    /// `Widget::getExtremesImpl` — délègue simple (widget.cc:1706).
    pub(crate) fn w_get_extremes_impl(
        &mut self,
        w: WidgetId,
        extremes: &mut Extremes,
        num_pos: usize,
        references: &[WidgetId],
        x: &[i32],
        y: &[i32],
    ) {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            // `Textblock::getExtremesSimpl` fait lui-meme
            // `correctExtremes` + `correctExtremesByOOF`
            // (textblock.cc:510-517, verbatim).
            self.tb_get_extremes_simpl(w, extremes);
            return;
        }
        let _ = (num_pos, references, x, y);
        self.w_get_extremes_simpl(w, extremes);
        if self.node(w, |n| crate::oof::is_oof_aware_kind(&n.kind)) {
            self.correct_extremes_by_oof(w, extremes);
        }
    }

    /// `Widget::getExtremesSimpl` — `notImplemented` amont →
    /// no-op (même note que `sizeRequestSimpl`). `Bullet`/`Ruler` :
    /// `*Simpl` verbatim.
    pub(crate) fn w_get_extremes_simpl(&mut self, w: WidgetId, extremes: &mut Extremes) {
        // `Embed::getExtremesSimpl` — verbatim ui.cc:58 :
        // `resource->getExtremes` + `correctExtremes(false)` +
        // `adjustmentWidth = min(minWidthIntrinsic, minWidth)`.
        if let Some(res) = self.embed_resource(w) {
            res.get_extremes(self, extremes);
            self.widget_correct_extremes(w, extremes, false);
            extremes.adjustment_width = extremes.min_width_intrinsic.min(extremes.min_width);
            return;
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Bullet)) {
            self.bullet_get_extremes_simpl(w, extremes);
        } else if self.node(w, |n| matches!(&n.kind, WidgetKind::Ruler { .. })) {
            self.ruler_get_extremes_simpl(w, extremes);
        } else if self.node(w, |n| matches!(&n.kind, WidgetKind::Image { .. })) {
            self.img_get_extremes_simpl(w, extremes);
        } else if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            self.tbl_get_extremes_simpl(w, extremes);
        }
    }

    /// `Widget::calcExtraSpaceImpl` — vide amont (widget.cc:1735).
    pub(crate) fn w_calc_extra_space_impl(
        &mut self,
        w: WidgetId,
        num_pos: usize,
        references: &[WidgetId],
        x: &[i32],
        y: &[i32],
    ) {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            let _ = num_pos;
            let _ = self.with_tb(w, |l, tb| {
                l.tb_calc_extra_space_impl(w, tb, references, x, y);
            });
            return;
        }
        let _ = (w, num_pos, references, x, y);
    }

    /// `Widget::sizeAllocateImpl` — vide amont (widget.cc:1726).
    /// `OofAware` : `sizeAllocateStart`/`End` autour (contrat
    /// oofawarewidget.hh:30 — le kind nu n'a pas de contenu
    /// propre entre les deux).
    pub(crate) fn w_size_allocate_impl(&mut self, w: WidgetId, allocation: &Allocation) {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            self.tb_size_allocate_impl(w, allocation);
            return;
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Image { .. })) {
            self.img_size_allocate_impl(w, allocation);
            return;
        }
        // `Table::sizeAllocateImpl` — allocation colonnes/rangées
        // verbatim (table.cc:165).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            self.tbl_size_allocate_impl(w, allocation);
            return;
        }
        // `Embed::sizeAllocateImpl` — `resource->sizeAllocate`
        // verbatim (ui.cc:65).
        if let Some(res) = self.embed_resource(w) {
            res.size_allocate(self, allocation);
            return;
        }
        if self.node(w, |n| crate::oof::is_oof_aware_kind(&n.kind)) {
            self.oof_size_allocate_start(w, allocation);
            self.oof_size_allocate_end(w);
            return;
        }
        let _ = (w, allocation);
    }

    /// `Widget::resizeDrawImpl` — `queueDraw()` amont.
    pub(crate) fn w_resize_draw_impl(&mut self, w: WidgetId) {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            self.tb_resize_draw_impl(w);
            return;
        }
        // `Table::resizeDrawImpl` — verbatim table.cc:216
        // (redrawX/redrawY).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            self.tbl_resize_draw_impl(w);
            return;
        }
        self.widget_queue_draw(w);
    }

    /// `Widget::markSizeChange` / `markExtremesChange` — vides
    /// amont (widget.cc:1746/1750).
    pub(crate) fn w_mark_size_change(&mut self, w: WidgetId, ref_: i32) {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            self.tb_mark_size_change(w, ref_);
            return;
        }
        let _ = (w, ref_);
    }
    /// `markExtremesChange`.
    pub(crate) fn w_mark_extremes_change(&mut self, w: WidgetId, ref_: i32) {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            self.tb_mark_extremes_change(w, ref_);
            return;
        }
        let _ = (w, ref_);
    }

    /// `Widget::notifySetAsTopLevel` — vide amont ;
    /// `OOFAwareWidget` : containing blocks = soi-même
    /// (oofawarewidget.cc:86).
    pub(crate) fn w_notify_set_as_top_level(&mut self, w: WidgetId) {
        if self.node(w, |n| crate::oof::is_oof_aware_kind(&n.kind)) {
            self.oof_notify_set_as_top_level(w);
        }
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            self.tb_notify_set_as_top_level(w);
        }
    }
    /// `notifySetParent` — vide amont ; `OOFAwareWidget` :
    /// recherche des containing blocks (oofawarewidget.cc:185).
    pub(crate) fn w_notify_set_parent(&mut self, w: WidgetId) {
        if self.node(w, |n| crate::oof::is_oof_aware_kind(&n.kind)) {
            self.oof_notify_set_parent(w);
        }
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            self.tb_notify_set_parent(w);
        }
    }

    /// `Widget::numSizeRequestReferences` — 0 amont (seam —
    /// les conteneurs à références, ex. tables, la
    /// surchargeront).
    #[allow(dead_code)]
    pub(crate) fn w_num_size_request_references(&self, w: WidgetId) -> usize {
        if let Some(n) = self.node_tb(w, |tb| self.tb_num_size_request_references(tb)) {
            return n;
        }
        let _ = w;
        0
    }
    /// `Widget::sizeRequestReference` — `notImplemented` amont →
    /// `None` (même seam).
    #[allow(dead_code)]
    pub(crate) fn w_size_request_reference(&self, w: WidgetId, index: usize) -> Option<WidgetId> {
        if let Some(r) = self.node_tb(w, |tb| self.tb_size_request_reference(tb, index)) {
            return r;
        }
        let _ = (w, index);
        None
    }
    /// `numGetExtremesReferences` — 0 (même seam).
    #[allow(dead_code)]
    pub(crate) fn w_num_get_extremes_references(&self, w: WidgetId) -> usize {
        if let Some(n) = self.node_tb(w, |tb| self.tb_num_get_extremes_references(tb)) {
            return n;
        }
        let _ = w;
        0
    }
    /// `getExtremesReference` — `None` ≙ `notImplemented`
    /// (même seam).
    #[allow(dead_code)]
    pub(crate) fn w_get_extremes_reference(&self, w: WidgetId, index: usize) -> Option<WidgetId> {
        if let Some(r) = self.node_tb(w, |tb| self.tb_get_extremes_reference(tb, index)) {
            return r;
        }
        let _ = (w, index);
        None
    }

    /// `Widget::getAdjustMinWidth` — statique amont.
    /// `tablecell::getAdjustMinWidth` → `Table::adjustTableMinWidth`
    /// (tablecell.cc:31-34) ; `Table::getAdjustMinWidth` idem
    /// (table.hh:408-410).
    pub(crate) fn w_get_adjust_min_width(&self, w: WidgetId) -> bool {
        // `Image::getAdjustMinWidth` → `false` (image.hh:180 —
        // verbatim : largeur minimale mal définie, correction
        // sans intérêt).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Image { .. })) {
            return false;
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. }))
            || self.node_tb(w, |tb| tb.cell.is_some()).unwrap_or(false)
        {
            return crate::table::adjust_table_min_width();
        }
        ADJUST_MIN_WIDTH.load(Ordering::Relaxed)
    }

    /// `Widget::usesAvailWidth` / `usesAvailHeight` — `false`.
    /// `Ruler::usesAvailWidth` → `true` (ruler.cc:77-79).
    pub(crate) fn w_uses_avail_width(&self, w: WidgetId) -> bool {
        if self.node(w, |n| {
            is_textblock_kind(&n.kind)
                || matches!(&n.kind, WidgetKind::Ruler { .. })
                || matches!(&n.kind, WidgetKind::Table { .. })
        }) {
            return true;
        }
        let _ = w;
        false
    }
    /// `usesAvailHeight`.
    pub(crate) fn w_uses_avail_height(&self, w: WidgetId) -> bool {
        let _ = w;
        false
    }

    /// `Widget::applyPerWidth` — verbatim widget.cc:1754
    /// (`multiplyWithPerLength + boxDiffWidth`).
    /// `Table`/`tablecell` : `multiplyWithPerLength` **sans**
    /// `boxDiff` (table.cc:316, tablecell.cc:122-128 — verbatim).
    pub(crate) fn w_apply_per_width(
        &mut self,
        w: WidgetId,
        container_width: i32,
        per_width: style::Length,
    ) -> i32 {
        if self.node(w, |n| {
            matches!(&n.kind, WidgetKind::Table { .. })
                || self.node_tb(w, |tb| tb.cell.is_some()).unwrap_or(false)
        }) {
            return self.tbl_apply_per_width(container_width, per_width);
        }
        multiply_with_per_length(container_width, per_width)
            + self.node(w, WidgetNode::box_diff_width)
    }

    /// `Widget::applyPerHeight` — verbatim (idem `applyPerWidth`
    /// pour `Table`/`tablecell`).
    pub(crate) fn w_apply_per_height(
        &mut self,
        w: WidgetId,
        container_height: i32,
        per_height: style::Length,
    ) -> i32 {
        if self.node(w, |n| {
            matches!(&n.kind, WidgetKind::Table { .. })
                || self.node_tb(w, |tb| tb.cell.is_some()).unwrap_or(false)
        }) {
            return self.tbl_apply_per_height(container_height, per_height);
        }
        multiply_with_per_length(container_height, per_height)
            + self.node(w, WidgetNode::box_diff_height)
    }

    /// `Widget::isBlockLevel` — `false` amont.
    /// `Ruler::isBlockLevel` → `true` (ruler.cc:62-65) ;
    /// `Table::isBlockLevel` → `true` (table.cc:358-361) ;
    /// `tablecell::isBlockLevel` → `false` (tablecell.cc:36-39 —
    /// les cellules sont inline vis-à-vis du parent).
    pub(crate) fn w_is_block_level(&self, w: WidgetId) -> bool {
        if self.node_tb(w, |tb| tb.cell.is_some()).unwrap_or(false) {
            return false;
        }
        if self.node(w, |n| {
            is_textblock_kind(&n.kind)
                || matches!(&n.kind, WidgetKind::Ruler { .. })
                || matches!(&n.kind, WidgetKind::Table { .. })
        }) {
            return true;
        }
        let _ = w;
        false
    }

    /// `Widget::isPossibleContainer` — `isBlockLevel()` amont.
    pub(crate) fn w_is_possible_container(&self, w: WidgetId) -> bool {
        self.w_is_block_level(w)
    }

    /// `Widget::containerSizeChangedForChildren` — verbatim :
    /// itération `WIDGET_IN_FLOW | WIDGET_OOF_CONT` (seam
    /// `children_for_container_change` tant qu'iterator.cc
    /// n'est pas porté). `OofAware` : contrat amont —
    /// in-flow (vide pour le kind nu) +
    /// `containerSizeChangedForChildrenOOF`.
    pub(crate) fn w_container_size_changed_for_children(&mut self, w: WidgetId) {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            self.tb_container_size_changed_for_children(w);
            return;
        }
        // `Table::containerSizeChangedForChildren` — verbatim
        // table.cc:326 (cellules + OOF).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            self.tbl_container_size_changed_for_children(w);
            return;
        }
        // `Embed::containerSizeChangedForChildren` —
        // `resource->containerSizeChangedForChildren` verbatim
        // (ui.cc:99 — ne descend PAS dans les enfants Widget).
        if let Some(res) = self.embed_resource(w) {
            res.container_size_changed_for_children(self);
            return;
        }
        // `Bullet::containerSizeChangedForChildren` /
        // `Ruler::containerSizeChangedForChildren` —
        // « Nothing to do » verbatim (bullet.cc:54-58,
        // ruler.cc:67-71) ; le `Ruler` court-circuite la
        // variante `OOFAwareWidget`. Idem `Image`
        // (image.cc:309-313).
        if self.node(w, |n| {
            matches!(
                &n.kind,
                WidgetKind::Bullet | WidgetKind::Ruler { .. } | WidgetKind::Image { .. }
            )
        }) {
            return;
        }
        if self.node(w, |n| crate::oof::is_oof_aware_kind(&n.kind)) {
            self.oof_container_size_changed_for_children(w);
            return;
        }
        for c in self.widget_children_for_container_change(w) {
            self.widget_container_size_changed(c);
        }
    }

    /// `Widget::affectedByContainerSizeChange` — verbatim
    /// widget.cc:444 (règle abs/per/auto + `usesAvail*`).
    pub(crate) fn w_affected_by_container_size_change(&mut self, w: WidgetId) -> bool {
        let container = self.node(w, |n| n.container);
        match container {
            None => {
                let Some(s) = self.node(w, |n| n.style.clone()) else {
                    return false;
                };
                if is_abs_length(s.width) && is_abs_length(s.height) {
                    false
                } else if is_per_length(s.width) || is_per_length(s.height) {
                    true
                } else {
                    (s.width == LENGTH_AUTO && self.w_uses_avail_width(w))
                        || (s.height == LENGTH_AUTO && self.w_uses_avail_height(w))
                }
            }
            Some(c) => self.w_affects_size_change_container_child(c, w),
        }
    }

    /// `Widget::affectsSizeChangeContainerChild` — verbatim
    /// widget.cc:485 (même règle, point de vue conteneur).
    pub(crate) fn w_affects_size_change_container_child(
        &mut self,
        parent: WidgetId,
        child: WidgetId,
    ) -> bool {
        if self.node(parent, |n| is_textblock_kind(&n.kind)) {
            return self.tb_affects_size_change_container_child(parent, child);
        }
        // `Table::affectsSizeChangeContainerChild` — `true`
        // verbatim (table.cc:343-355 : tracking coûteux →
        // toujours vrai).
        if self.node(parent, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            return true;
        }
        self.w_affects_size_change_container_child_base(child)
    }

    /// `Widget::affectsSizeChangeContainerChild` — implémentation
    /// de base (règle abs/per/auto + `usesAvail*`, widget.cc:479).
    /// Amont les overrides appellent la méthode de base par liaison
    /// statique (`Widget::affectsSizeChangeContainerChild`) — passer
    /// ici, PAS par le dispatcher `w_*` (récursion infinie sinon).
    pub(crate) fn w_affects_size_change_container_child_base(&mut self, child: WidgetId) -> bool {
        let Some(s) = self.node(child, |n| n.style.clone()) else {
            return false;
        };
        if is_abs_length(s.width) && is_abs_length(s.height) {
            false
        } else if is_per_length(s.width) || is_per_length(s.height) {
            true
        } else {
            (s.width == LENGTH_AUTO && self.w_uses_avail_width(child))
                || (s.height == LENGTH_AUTO && self.w_uses_avail_height(child))
        }
    }

    /// `Widget::getAvailWidthOfChild` — verbatim widget.cc:1774
    /// (impl par défaut « convenable pour tous les conteneurs »).
    pub(crate) fn w_get_avail_width_of_child(
        &mut self,
        w: WidgetId,
        child: WidgetId,
        force_value: bool,
    ) -> i32 {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            return self.tb_get_avail_width_of_child(w, child, force_value);
        }
        // `Table::getAvailWidthOfChild` — verbatim table.cc:224
        // (mgr OOF ou `calcAvailWidthForDescendant`/`-1`).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            return self.tbl_get_avail_width_of_child(w, child, force_value);
        }
        // `Embed::getAvailWidthOfChild` — délègue au resource
        // verbatim (ui.cc:72).
        if let Some(res) = self.embed_resource(w) {
            return res.get_avail_width_of_child(self, child, force_value);
        }
        self.w_get_avail_width_of_child_base(w, child, force_value)
    }

    /// `Widget::getAvailWidthOfChild` — implémentation de base
    /// (widget.cc:1774). Amont les overrides appellent la méthode
    /// de base par liaison statique (`Widget::getAvailWidthOfChild`)
    /// — passer ici, PAS par le dispatcher `w_*` (récursion
    /// infinie sinon).
    pub(crate) fn w_get_avail_width_of_child_base(
        &mut self,
        w: WidgetId,
        child: WidgetId,
        force_value: bool,
    ) -> i32 {
        let style = self.node(child, |n| n.style.clone());
        let Some(s) = style else { return -1 };
        if s.width == LENGTH_AUTO {
            if !force_value {
                return -1;
            }
            let mut width = (self.widget_get_avail_width(w, true)
                - self.node(w, WidgetNode::box_diff_width))
            .max(0);
            if width != -1 {
                let max_width = self.widget_calc_width(child, s.max_width, -1, Some(w), -1, false);
                if max_width != -1 && width > max_width {
                    width = max_width;
                }
                let min_width = self.widget_calc_width(child, s.min_width, -1, Some(w), -1, false);
                if min_width != -1 && width < min_width {
                    width = min_width;
                }
            }
            width
        } else {
            let eff = self.node(child, |n| n.quasi_parent.or(n.container).or(n.parent));
            if eff == Some(w) {
                let mut width = -1;
                self.widget_calc_final_width(child, &s, -1, Some(w), 0, force_value, &mut width);
                width
            } else {
                let e = eff.unwrap_or(w);
                self.w_get_avail_width_of_child(e, child, force_value)
            }
        }
    }

    /// `Widget::getAvailHeightOfChild` — verbatim widget.cc:1823.
    pub(crate) fn w_get_avail_height_of_child(
        &mut self,
        w: WidgetId,
        child: WidgetId,
        force_value: bool,
    ) -> i32 {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            return self.tb_get_avail_height_of_child(w, child, force_value);
        }
        // `Embed::getAvailHeightOfChild` — verbatim ui.cc:77.
        if let Some(res) = self.embed_resource(w) {
            return res.get_avail_height_of_child(self, child, force_value);
        }
        self.w_get_avail_height_of_child_base(w, child, force_value)
    }

    /// `Widget::getAvailHeightOfChild` — implémentation de base
    /// (widget.cc:1823). Même règle de liaison statique que
    /// [`Self::w_get_avail_width_of_child_base`].
    pub(crate) fn w_get_avail_height_of_child_base(
        &mut self,
        w: WidgetId,
        child: WidgetId,
        force_value: bool,
    ) -> i32 {
        let style = self.node(child, |n| n.style.clone());
        let Some(s) = style else { return -1 };
        if s.height == LENGTH_AUTO {
            if !force_value {
                return -1;
            }
            (self.widget_get_avail_height(w, true) - self.node(w, WidgetNode::box_diff_height))
                .max(0)
        } else {
            let eff = self.node(child, |n| n.quasi_parent.or(n.container).or(n.parent));
            if eff == Some(w) {
                if is_abs_length(s.height) {
                    (abs_length_val(s.height) + self.node(child, WidgetNode::box_diff_height))
                        .max(0)
                } else {
                    debug_assert!(is_per_length(s.height));
                    let avail_height = self.widget_get_avail_height(w, force_value);
                    if avail_height == -1 {
                        -1
                    } else {
                        self.w_apply_per_height(
                            child,
                            avail_height - self.node(w, WidgetNode::box_diff_height),
                            s.height,
                        )
                        .max(0)
                    }
                }
            } else {
                let e = eff.unwrap_or(w);
                self.w_get_avail_height_of_child(e, child, force_value)
            }
        }
    }

    /// `Widget::correctRequisitionOfChild` — verbatim
    /// widget.cc:1894 (effContainer + 3 passes).
    pub(crate) fn w_correct_requisition_of_child(
        &mut self,
        w: WidgetId,
        child: WidgetId,
        requisition: &mut Requisition,
        split_height_fun: SplitHeightFun,
        allow_decrease_width: bool,
        allow_decrease_height: bool,
    ) {
        // `Embed::correctRequisitionOfChild` — délègue au resource
        // verbatim (ui.cc:82).
        if let Some(res) = self.embed_resource(w) {
            res.correct_requisition_of_child(
                self,
                child,
                requisition,
                split_height_fun,
                allow_decrease_width,
                allow_decrease_height,
            );
            return;
        }
        let eff = self.node(child, |n| n.quasi_parent.or(n.container).or(n.parent));
        if eff == Some(w) {
            for pass in 0..3 {
                self.widget_correct_req_width_of_child(w, child, requisition, allow_decrease_width);
                self.widget_correct_req_height_of_child(
                    w,
                    child,
                    requisition,
                    split_height_fun,
                    allow_decrease_height,
                );
                let changed = self.widget_correct_req_aspect_ratio(
                    pass,
                    w,
                    child,
                    requisition,
                    allow_decrease_width,
                    allow_decrease_height,
                    split_height_fun,
                );
                if !changed {
                    break;
                }
            }
            // `SimpleTableCell`/`AlignedTableCell::
            // correctRequisitionOfChild` = base + `tablecell::
            // correctCorrectedRequisitionOfChild` (clamp à la
            // largeur de cellule — verbatim).
            if self.node_tb(w, |tb| tb.cell.is_some()).unwrap_or(false) {
                self.cell_correct_corrected_requisition_of_child(
                    w,
                    requisition,
                    allow_decrease_width,
                );
            }
        } else {
            let e = eff.unwrap_or(w);
            self.w_correct_requisition_of_child(
                e,
                child,
                requisition,
                split_height_fun,
                allow_decrease_width,
                allow_decrease_height,
            );
        }
    }

    /// `Widget::correctExtremesOfChild` — verbatim widget.cc:2164.
    pub(crate) fn w_correct_extremes_of_child(
        &mut self,
        w: WidgetId,
        child: WidgetId,
        extremes: &mut Extremes,
        use_adjustment_width: bool,
    ) {
        if let Some(res) = self.embed_resource(w) {
            // `Embed::correctExtremesOfChild` — délègue au resource
            // verbatim (ui.cc:93).
            res.correct_extremes_of_child(self, child, extremes, use_adjustment_width);
            return;
        }
        let eff = self.node(child, |n| n.quasi_parent.or(n.container).or(n.parent));
        if eff == Some(w) {
            self.w_correct_extremes_of_child_base(w, child, extremes, use_adjustment_width);
        } else {
            let e = eff.unwrap_or(w);
            self.w_correct_extremes_of_child(e, child, extremes, use_adjustment_width);
        }
    }

    /// `Widget::correctExtremesOfChild` — impl de **base**
    /// (non-virtuel ≙ `correctExtremesOfChildNoRec` de `ui.hh:278`,
    /// appelé par `ComplexButtonResource` en court-circuitant le
    /// dispatch `Embed`).
    pub(crate) fn w_correct_extremes_of_child_base(
        &mut self,
        w: WidgetId,
        child: WidgetId,
        extremes: &mut Extremes,
        use_adjustment_width: bool,
    ) {
        let limit_min_width = if use_adjustment_width {
            self.widget_get_min_width(child, Some(extremes), false)
        } else {
            0
        };
        let style = self.node(child, |n| n.style.clone());
        if let Some(s) = &style {
            let width = self.widget_calc_width(child, s.width, -1, Some(w), limit_min_width, false);
            let min_width =
                self.widget_calc_width(child, s.min_width, -1, Some(w), limit_min_width, false);
            let max_width =
                self.widget_calc_width(child, s.max_width, -1, Some(w), limit_min_width, false);
            if width != -1 {
                extremes.min_width = width;
                extremes.max_width = width;
            }
            if min_width != -1 {
                extremes.min_width = min_width;
            }
            if max_width != -1 {
                extremes.max_width = max_width;
            }
        }
    }

    /// `Widget::buttonPressImpl` — `Textblock` ≙
    /// `sendSelectionEvent (BUTTON_PRESS)` (textblock.cc:944) ;
    /// défaut `false`.
    pub(crate) fn w_button_press_impl(&mut self, w: WidgetId, event: &EventButton) -> bool {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            return self.tb_button_press_impl(w, event);
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Image { .. })) {
            return self.img_button_press_impl(w, event);
        }
        // `Embed::buttonPressImpl` — verbatim ui.cc:113 :
        // bouton 3 → `resource->emitClicked`, `handled = true`.
        if let Some(res) = self.embed_resource(w) {
            if event.button == 3 {
                res.emit_clicked(event);
                return true;
            }
            return false;
        }
        let _ = (w, event);
        false
    }
    /// `buttonReleaseImpl` — `Textblock` ≙
    /// `sendSelectionEvent (BUTTON_RELEASE)` (textblock.cc:949).
    pub(crate) fn w_button_release_impl(&mut self, w: WidgetId, event: &EventButton) -> bool {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            return self.tb_button_release_impl(w, event);
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Image { .. })) {
            return self.img_button_release_impl(w, event);
        }
        let _ = (w, event);
        false
    }
    /// `motionNotifyImpl` — `Textblock` ≙ motion sélection/hover
    /// (textblock.cc:959) ; défaut `false`.
    pub(crate) fn w_motion_notify_impl(&mut self, w: WidgetId, event: &EventMotion) -> bool {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            return self.tb_motion_notify_impl(w, event);
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Image { .. })) {
            return self.img_motion_notify_impl(w, event);
        }
        let _ = (w, event);
        false
    }

    /// `Widget::enterNotifyImpl` — `Textblock` ≙ `hoverLink = -2`
    /// (textblock.cc:1001) ; défaut `tooltip->onEnter()` amont.
    pub(crate) fn w_enter_notify_impl(&mut self, w: WidgetId, event: &EventCrossing) {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            return self.tb_enter_notify_impl(w, event);
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Image { .. })) {
            return self.img_enter_notify_impl(w, event);
        }
        // `Embed::enterNotifyImpl` — verbatim ui.cc:104 :
        // `resource->emitEnter()` + base `Widget::enterNotifyImpl`.
        if let Some(res) = self.embed_resource(w) {
            res.emit_enter();
        }
        let _ = event;
        let tooltip = self.node(w, |n| {
            n.style.as_ref().and_then(|s| s.attrs.x_tooltip.clone())
        });
        if let Some(t) = tooltip {
            t.on_enter();
        }
    }

    /// `Widget::leaveNotifyImpl` — `Textblock` ≙ LinkEnter(-1) +
    /// onLeave tooltip (textblock.cc:1008) ; défaut
    /// `tooltip->onLeave()`.
    pub(crate) fn w_leave_notify_impl(&mut self, w: WidgetId, event: &EventCrossing) {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            return self.tb_leave_notify_impl(w, event);
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Image { .. })) {
            return self.img_leave_notify_impl(w, event);
        }
        // `Embed::leaveNotifyImpl` — verbatim ui.cc:110 :
        // `resource->emitLeave()` + base.
        if let Some(res) = self.embed_resource(w) {
            res.emit_leave();
        }
        let _ = event;
        let tooltip = self.node(w, |n| {
            n.style.as_ref().and_then(|s| s.attrs.x_tooltip.clone())
        });
        if let Some(t) = tooltip {
            t.on_leave();
        }
    }

    /// `Widget::draw` — pure virtual amont ; `Base` no-op (les
    /// kinds concrets dessineront).
    pub(crate) fn w_draw(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        area: &crate::types::Rectangle,
        context: &mut DrawingContext,
    ) {
        // `Bullet::draw` / `Ruler::draw` — verbatim ; `Ruler`
        // court-circuite `OOFAwareWidget::draw` (ruler.cc:87).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Bullet)) {
            self.bullet_draw(w, view, area, context);
            return;
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Image { .. })) {
            self.img_draw(w, view, area, context);
            return;
        }
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Ruler { .. })) {
            self.ruler_draw(w, view, area, context);
            return;
        }
        // `Embed::draw` — verbatim ui.cc:126 :
        // `drawWidgetBox(false)` + `resource->draw`.
        if let Some(res) = self.embed_resource(w) {
            self.widget_draw_widget_box(w, view, area, false);
            res.draw(view, area, context);
            return;
        }
        if self.node(w, |n| crate::oof::is_oof_aware_kind(&n.kind)) {
            self.oof_draw(w, view, area, context);
        }
    }

    /// `Widget::getWidgetAtPoint` — verbatim widget.cc:207 :
    /// widgets simples sans enfants → `self` si `inAllocation`.
    /// `Container` : descend dans les enfants (seam — les kinds
    /// concrets affineront leurs règles de hit-test).
    /// `OofAware` : niveaux `SL_*` verbatim
    /// (oofawarewidget.cc:456).
    pub(crate) fn w_get_widget_at_point(
        &mut self,
        w: WidgetId,
        x: i32,
        y: i32,
        context: &mut GettingWidgetAtPointContext,
    ) -> Option<WidgetId> {
        // `Ruler::getWidgetAtPoint` remplace la version complexe
        // de `OOFAwareWidget` — `inAllocation ? this : NULL`
        // verbatim (ruler.cc:93-102).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Ruler { .. })) {
            let _ = context;
            return if self.node(w, |n| n.in_allocation(x, y)) {
                Some(w)
            } else {
                None
            };
        }
        if self.node(w, |n| crate::oof::is_oof_aware_kind(&n.kind)) {
            return self.oof_get_widget_at_point(w, x, y, context);
        }
        let _ = context;
        if !self.node(w, |n| n.in_allocation(x, y)) {
            return None;
        }
        // Descente dans les enfants, dernier au premier
        // (ordre de dessin — les kinds affineront).
        for c in self.widget_children_for_container_change(w).iter().rev() {
            if let Some(found) = self.w_get_widget_at_point(*c, x, y, context) {
                return Some(found);
            }
        }
        Some(w)
    }

    /// `Widget::removeChild` — `notImplemented` amont ; pour
    /// `Container`, retrait verbatim attendu par `~Widget`.
    /// `OofAware` : assert `isWidgetOOF` verbatim
    /// (oofawarewidget.cc:540 — corps vide).
    pub(crate) fn w_remove_child(&mut self, w: WidgetId, child: WidgetId) {
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            // `Textblock::removeChild` — `\bug Not implemented`
            // amont (textblock.cc:1172-1175, verbatim no-op).
            self.tb_remove_child(w, child);
            return;
        }
        // `Table::removeChild` — `\bug Not implemented` amont
        // (table.cc:457-460, verbatim no-op — n'atteint pas la
        // version `OOFAwareWidget`).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            self.tbl_remove_child(w, child);
            return;
        }
        if self.node(w, |n| crate::oof::is_oof_aware_kind(&n.kind)) {
            self.oof_remove_child(w, child);
            return;
        }
        self.node_mut(w, |n| {
            if let WidgetKind::Container { children } = &mut n.kind {
                children.retain(|c| *c != child);
            }
        });
    }
}

// ------------------------------------------------------------------
// Wrappers publics d'événements (widget.cc:1247-1273).
// ------------------------------------------------------------------

impl Layout {
    /// `Widget::buttonPress`.
    pub fn widget_button_press(&mut self, w: WidgetId, event: &EventButton) -> bool {
        self.w_button_press_impl(w, event)
    }
    /// `Widget::buttonRelease`.
    pub fn widget_button_release(&mut self, w: WidgetId, event: &EventButton) -> bool {
        self.w_button_release_impl(w, event)
    }
    /// `Widget::motionNotify`.
    pub fn widget_motion_notify(&mut self, w: WidgetId, event: &EventMotion) -> bool {
        self.w_motion_notify_impl(w, event)
    }
    /// `Widget::enterNotify`.
    pub fn widget_enter_notify(&mut self, w: WidgetId, event: &EventCrossing) {
        self.w_enter_notify_impl(w, event);
    }
    /// `Widget::leaveNotify`.
    pub fn widget_leave_notify(&mut self, w: WidgetId, event: &EventCrossing) {
        self.w_leave_notify_impl(w, event);
    }
}

// ------------------------------------------------------------------
// Style / couleurs — widget.cc:1283-1452.
// ------------------------------------------------------------------

impl Layout {
    /// `Widget::setStyle` — verbatim widget.cc:1283 :
    /// retire le renderer externe de l'ancienne image de fond,
    /// `sizeDiffs` → `queueResize` sinon `queueDraw`, pose le
    /// `WidgetImgRenderer` sur la nouvelle image de fond.
    /// `&Rc<Style>` ≙ `Style*` emprunté amont.
    pub fn widget_set_style(&mut self, w: WidgetId, style_: &Rc<Style>) {
        // `Embed::setStyle` — `resource->setStyle` + `Widget::setStyle`
        // verbatim (ui.cc:142).
        if let Some(res) = self.embed_resource(w) {
            res.set_style(style_);
        }
        let old = self.node(w, |n| n.style.clone());
        let size_changed = match &old {
            Some(o) => {
                let has_renderer = self.node(w, |n| n.widget_img_renderer);
                if has_renderer {
                    if let Some(bi) = &o.attrs.background_image {
                        let r = self.widget_img_renderer_rc(w);
                        bi.borrow().remove_external_img_renderer(&r);
                    }
                }
                o.size_diffs(&style_.attrs)
            }
            None => true,
        };

        self.node_mut(w, |n| n.style = Some(style_.clone()));

        if let Some(bi) = &style_.attrs.background_image {
            if !self.node(w, |n| n.widget_img_renderer) {
                self.node_mut(w, |n| n.widget_img_renderer = true);
            }
            let r = self.widget_img_renderer_rc(w);
            bi.borrow().put_external_img_renderer(&r);
        }

        self.update_cursor();

        // `stackingContextMgr` — verbatim : établi seulement si
        // `layout == NULL` amont ; ici `detached` ≙ pas encore
        // rattaché. La création du SCM est phase OOF — le flag
        // `has_stacking_context_mgr` et `stacking_context_widget =
        // self` sont posés si le style établit un contexte
        // (`StackingContextMgr::isEstablishingStackingContext`
        // → seam `style_establishes_stacking_context`).
        if self.node_detached(w) && self.style_establishes_stacking_context(style_) {
            self.node_mut(w, |n| {
                n.has_stacking_context_mgr = true;
                n.stacking_context_widget = Some(w);
            });
        }

        if size_changed {
            self.widget_queue_resize(w, 0, true);
        } else {
            self.widget_queue_draw(w);
        }
    }

    /// `StackingContextMgr::isEstablishingStackingContext` — seam :
    /// le critère (`position != STATIC`/`zIndex`) arrive avec la
    /// phase OOF ; `false` ≙ `IMPL_POS = false` amont (layout.cc:380).
    pub(crate) fn style_establishes_stacking_context(&self, _style: &Style) -> bool {
        false
    }

    /// `Rc` vers le `WidgetImgRenderer` du widget — créé à la
    /// demande et mémorisé dans `widget_img_renderers` (≙ le
    /// champ `widgetImgRenderer` amont, conservé entre styles).
    fn widget_img_renderer_rc(
        &mut self,
        w: WidgetId,
    ) -> Rc<RefCell<dyn crate::imgrenderer::ImgRenderer>> {
        if let Some(r) = self.widget_img_renderers.get(&w) {
            return r.clone();
        }
        let arena = Rc::downgrade(&self.arena);
        let r: Rc<RefCell<dyn crate::imgrenderer::ImgRenderer>> =
            Rc::new(RefCell::new(WidgetImgRenderer {
                arena,
                layout_id: self.id,
                widget: w,
            }));
        self.widget_img_renderers.insert(w, r.clone());
        r
    }

    /// `Widget::setBgColor` — verbatim (affectation simple).
    pub fn widget_set_bg_color(&mut self, w: WidgetId, bg_color: Option<Color>) {
        self.node_mut(w, |n| n.bg_color = bg_color);
    }

    /// `Widget::getBgColor` — verbatim widget.cc:1381 : remonte
    /// `style->backgroundColor` puis `bgColor`, fallback
    /// `layout->getBgColor()`.
    #[must_use]
    pub fn widget_get_bg_color(&self, w: WidgetId) -> Option<Color> {
        let mut cur = Some(w);
        while let Some(c) = cur {
            let found = self.node(c, |n| {
                n.style
                    .as_ref()
                    .and_then(|s| s.attrs.background_color)
                    .or(n.bg_color)
            });
            if found.is_some() {
                return found;
            }
            cur = self.node(c, |n| n.parent);
        }
        self.bg_color
    }

    /// `Widget::getFgColor` — verbatim widget.cc:1399 : remonte
    /// `style->color`, `None` si aucun.
    #[must_use]
    pub fn widget_get_fg_color(&self, w: WidgetId) -> Option<Color> {
        let mut cur = Some(w);
        while let Some(c) = cur {
            let found = self.node(c, |n| n.style.as_ref().and_then(|s| s.attrs.color));
            if found.is_some() {
                return found;
            }
            cur = self.node(c, |n| n.parent);
        }
        None
    }
}

// ------------------------------------------------------------------
// drawBox / drawWidgetBox / drawSelected — widget.cc:1415-1564.
// ------------------------------------------------------------------

impl Layout {
    /// `Widget::drawBox` — verbatim widget.cc:1415 : bordure puis
    /// fond de la sous-zone `(x,y,w,h)` (coords widget), `area`
    /// en coords widget.
    #[allow(clippy::too_many_arguments)]
    pub fn widget_draw_box(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        style_: &Style,
        area: &crate::types::Rectangle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        inverse: bool,
    ) {
        let alloc = self.node(w, |n| n.allocation);
        let canvas_area = crate::types::Rectangle {
            x: area.x + alloc.x,
            y: area.y + alloc.y,
            width: area.width,
            height: area.height,
        };

        style::draw_border(
            view,
            &canvas_area,
            alloc.x + x,
            alloc.y + y,
            width,
            height,
            &style_.attrs,
            inverse,
        );

        let (x_pad, y_pad, width_pad, height_pad) = self.node(w, WidgetNode::get_padding_area);
        let s = &style_.attrs;
        let (lbg_c, lbg_i) = self.layout_bg_refs();
        style::draw_background(
            view,
            lbg_c.as_ref(),
            lbg_i.as_ref(),
            &canvas_area,
            alloc.x + x + s.margin.left + s.border_width.left,
            alloc.y + y + s.margin.top + s.border_width.top,
            width - s.margin.left - s.border_width.left - s.margin.right - s.border_width.right,
            height - s.margin.top - s.border_width.top - s.margin.bottom - s.border_width.bottom,
            x_pad,
            y_pad,
            width_pad,
            height_pad,
            &style_.attrs,
            s.background_color.as_ref(),
            inverse,
            false,
        );
    }

    /// `Widget::drawWidgetBox` — verbatim widget.cc:1467 : boîte
    /// complète du widget (margin/border/padding areas), fond
    /// hérité des ancêtres en inverse, `atTop = parent == NULL`.
    pub fn widget_draw_widget_box(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        area: &crate::types::Rectangle,
        inverse: bool,
    ) {
        let alloc = self.node(w, |n| n.allocation);
        let canvas_area = crate::types::Rectangle {
            x: area.x + alloc.x,
            y: area.y + alloc.y,
            width: area.width,
            height: area.height,
        };
        let (x_mar, y_mar, width_mar, height_mar) = self.node(w, WidgetNode::get_margin_area);
        let style = self.node(w, |n| n.style.clone());
        let Some(s) = style else { return };

        style::draw_border(
            view,
            &canvas_area,
            x_mar,
            y_mar,
            width_mar,
            height_mar,
            &s.attrs,
            inverse,
        );

        let (x_pad, y_pad, width_pad, height_pad) = self.node(w, WidgetNode::get_padding_area);

        // Fond inverse : première couleur d'ancêtre définie.
        let bg_color: Option<Color> = if inverse && s.attrs.background_color.is_none() {
            let mut cur = Some(w);
            let mut found = None;
            while let Some(c) = cur {
                let bc = self.node(c, |n| {
                    n.style.as_ref().and_then(|st| st.attrs.background_color)
                });
                if bc.is_some() {
                    found = bc;
                    break;
                }
                cur = self.node(c, |n| n.parent);
            }
            found.or(self.bg_color)
        } else {
            s.attrs.background_color
        };

        let (lbg_c, lbg_i) = self.layout_bg_refs();
        let at_top = self.node(w, |n| n.parent).is_none();
        style::draw_background(
            view,
            lbg_c.as_ref(),
            lbg_i.as_ref(),
            &canvas_area,
            x_pad,
            y_pad,
            width_pad,
            height_pad,
            x_pad,
            y_pad,
            width_pad,
            height_pad,
            &s.attrs,
            bg_color.as_ref(),
            inverse,
            at_top,
        );
    }

    /// `Widget::drawSelected` — vide amont (`\bug Not
    /// implemented`, widget.cc:1560).
    pub fn widget_draw_selected(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        area: &crate::types::Rectangle,
    ) {
        let _ = (w, view, area);
    }

    /// `Widget::drawInterruption` — verbatim widget.cc:195 :
    /// dessine la partie visible si `topLevel` intersecte.
    pub fn widget_draw_interruption(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        _area: &crate::types::Rectangle,
        context: &mut DrawingContext,
    ) {
        let Some(top) = self.top_level else {
            context.stacking.add_widget_processed_as_interruption(w);
            return;
        };
        let toplevel_area = context.toplevel_area;
        if let Some(this_area) = self.widget_intersects(w, top, &toplevel_area) {
            self.w_draw(w, view, &this_area, context);
        }
        context.stacking.add_widget_processed_as_interruption(w);
    }

    /// `Widget::getWidgetAtPointInterrupted` — verbatim
    /// widget.cc:215.
    pub fn widget_get_widget_at_point_interrupted(
        &mut self,
        w: WidgetId,
        x: i32,
        y: i32,
        context: &mut GettingWidgetAtPointContext,
    ) -> Option<WidgetId> {
        let found = self.w_get_widget_at_point(w, x, y, context);
        context.stacking.add_widget_processed_as_interruption(w);
        found
    }
}

// ------------------------------------------------------------------
// Widget::WidgetImgRenderer — widget.cc:39-62, verbatim.
// ------------------------------------------------------------------

/// `Widget::WidgetImgRenderer` — renderer externe de fond de
/// widget. Amont tient `Widget*` ; le port tient `Weak<RefCell<
/// Arena>>` + `WidgetId` (modèle D — les callbacks image ne
/// reçoivent pas `&mut Layout`).
struct WidgetImgRenderer {
    /// Arène faible (≙ `widget` amont).
    arena: Weak<RefCell<crate::layout::Arena>>,
    /// `Layout::id` émetteur (tag `pending_draws` — arène
    /// partageable entre layouts).
    layout_id: usize,
    /// Le widget concerné.
    widget: WidgetId,
}

impl WidgetImgRenderer {
    /// Zone padding du widget (≙ `getPaddingArea`).
    fn padding_area(&self) -> (i32, i32, i32, i32) {
        self.arena.upgrade().map_or((0, 0, 0, 0), |a| {
            a.borrow().nodes[self.widget].get_padding_area()
        })
    }
}

impl crate::imgrenderer::ImgRenderer for WidgetImgRenderer {
    /// `setBuffer` — rien (≙ `external_img_renderer_set_buffer`).
    fn set_buffer(&mut self, _b: &Rc<dyn crate::imgbuf::Imgbuf>, _resize: bool) {}

    /// `drawRow` — verbatim via helper (code mort amont).
    fn draw_row(&mut self, row: i32) {
        style::external_img_renderer_draw_row(self, row);
    }

    /// `finish` — verbatim via helper : `draw` sur la zone fond.
    fn finish(&mut self) {
        style::external_img_renderer_finish(self);
    }

    /// `fatal` — rien.
    fn fatal(&mut self) {}
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl style::ExternalImgRenderer for WidgetImgRenderer {
    /// `readyToDraw` — `widget->wasAllocated()` (widget.cc:39).
    fn ready_to_draw(&self) -> bool {
        self.arena
            .upgrade()
            .is_some_and(|a| a.borrow().nodes[self.widget].was_allocated())
    }

    /// `getBgArea` — padding area (widget.cc:45).
    fn get_bg_area(&self) -> (i32, i32, i32, i32) {
        self.padding_area()
    }

    /// `getRefArea` — padding area (widget.cc:52).
    fn get_ref_area(&self) -> (i32, i32, i32, i32) {
        self.padding_area()
    }

    /// `getBackgroundImage` — `style ? backgroundImage : NULL`.
    fn get_background_image(&self) -> Option<Rc<RefCell<style::StyleImage>>> {
        self.arena.upgrade().and_then(|a| {
            a.borrow().nodes[self.widget]
                .style
                .as_ref()
                .and_then(|s| s.attrs.background_image.clone())
        })
    }

    /// `getBackgroundRepeat` — fallback `BACKGROUND_REPEAT`.
    fn get_background_repeat(&self) -> style::BackgroundRepeat {
        self.arena
            .upgrade()
            .map_or(style::BackgroundRepeat::Repeat, |a| {
                a.borrow().nodes[self.widget]
                    .style
                    .as_ref()
                    .map_or(style::BackgroundRepeat::Repeat, |s| {
                        s.attrs.background_repeat
                    })
            })
    }

    /// `getBackgroundAttachment` — fallback `SCROLL`.
    fn get_background_attachment(&self) -> style::BackgroundAttachment {
        self.arena
            .upgrade()
            .map_or(style::BackgroundAttachment::Scroll, |a| {
                a.borrow().nodes[self.widget]
                    .style
                    .as_ref()
                    .map_or(style::BackgroundAttachment::Scroll, |s| {
                        s.attrs.background_attachment
                    })
            })
    }

    /// `getBackgroundPositionX` — fallback `createPerLength(0)`.
    fn get_background_position_x(&self) -> style::Length {
        self.arena.upgrade().map_or_else(
            || style::create_per_length(0.0),
            |a| {
                a.borrow().nodes[self.widget].style.as_ref().map_or_else(
                    || style::create_per_length(0.0),
                    |s| s.attrs.background_position_x,
                )
            },
        )
    }

    /// `getBackgroundPositionY`.
    fn get_background_position_y(&self) -> style::Length {
        self.arena.upgrade().map_or_else(
            || style::create_per_length(0.0),
            |a| {
                a.borrow().nodes[self.widget].style.as_ref().map_or_else(
                    || style::create_per_length(0.0),
                    |s| s.attrs.background_position_y,
                )
            },
        )
    }

    /// `draw` — `widget->queueDrawArea (x-alloc.x, y-alloc.y, ...)`
    /// (widget.cc:59). Le renderer n'a pas `&mut Layout` :
    /// `queue_draw_area` enfile dans `arena.pending_draws`, vidé
    /// vers `view->queueDraw` par `Layout::flush_pending_draws`
    /// (adaptation documentée — différé vs immédiat amont).
    fn draw(&mut self, x: i32, y: i32, width: i32, height: i32) {
        if let Some(a) = self.arena.upgrade() {
            let mut a = a.borrow_mut();
            let alloc = a.nodes[self.widget].allocation;
            a.pending_draws.push((
                self.layout_id,
                Some(self.widget),
                crate::types::Rectangle {
                    x: x - alloc.x,
                    y: y - alloc.y,
                    width,
                    height,
                },
            ));
        }
    }
}

// ------------------------------------------------------------------
// Destruction — ~Widget (widget.cc:110-134).
// ------------------------------------------------------------------

impl Layout {
    /// `~Widget` — callback de suppression, retrait du renderer
    /// externe, `removeChild`/`removeWidget` (verbatim). Le nœud
    /// reste dans l'arène sous forme de tombe `Base` (modèle D —
    /// les `WidgetId` restants ne sont pas réutilisés).
    pub fn widget_delete(&mut self, w: WidgetId) {
        // `~Textblock` (textblock.cc:277-303) : tooltip +
        // ancres + cleanupWord, *avant* `~OOFAwareWidget`
        // (verbatim ordre de destruction).
        if self.node(w, |n| is_textblock_kind(&n.kind)) {
            self.tb_drop(w);
        }
        // `~Table` (table.cc:74-106) : détruit les cellules
        // `Child::CELL` (verbatim ordre — corps avant
        // `~OOFAwareWidget` ≙ section OOF ci-dessous).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            self.tbl_drop(w);
        }
        // `~OOFAwareWidget` (oofawarewidget.cc:55) : les widgets
        // OOF sont détruits avec le container (avant le callback
        // `~Widget` — verbatim ordre d'exécution amont : corps
        // de `~OOFAwareWidget` puis `~Widget`).
        if let Some(oof_children) = self.node_oof(w, |o| {
            let mut out = Vec::new();
            for mgr in o.out_of_flow_mgr.iter().flatten() {
                for i in 0..mgr.borrow().get_num_widgets() {
                    if let Some(c) = mgr.borrow().get_widget(i) {
                        out.push(c);
                    }
                }
            }
            out
        }) {
            for c in oof_children {
                self.widget_delete(c);
            }
        }
        self.node_mut(w, WidgetNode::run_delete_callback);

        // Renderer externe + style (verbatim ~Widget:120-129).
        if self.node(w, |n| n.widget_img_renderer) {
            let style = self.node(w, |n| n.style.clone());
            if let Some(s) = &style {
                if let Some(bi) = &s.attrs.background_image {
                    let r = self.widget_img_renderer_rc(w);
                    bi.borrow().remove_external_img_renderer(&r);
                }
            }
        }

        // `if (parent) … else if (layout) layout->removeWidget()`
        // (widget.cc:133-135) — `layout != NULL` ≙ rattaché : tout
        // widget sans parent encore attaché est le top level
        // (A05-D3 — `set_widget` vide `top_level` *avant* le delete,
        // verbatim amont).
        if let Some(p) = self.node(w, |n| n.parent) {
            self.w_remove_child(p, w);
        } else if !self.node_detached(w) {
            self.remove_widget();
        }
        self.node_mut(w, |n| {
            n.style = None;
            n.kind = WidgetKind::Base;
            n.parent = None;
        });
        // Tombe ≙ objet détruit — `layout == NULL` désormais.
        self.arena.borrow_mut().detached.insert(w);
    }
}

// ------------------------------------------------------------------
// Helpers kinds conteneurs (seam — les kinds amont ajoutent leurs
// enfants par leurs propres méthodes `addWidget`/`addParbreak`…
// qui font `setParent` + bookkeeping ; pour `WidgetKind::
// Container` le bookkeeping est le `children` du kind).
// ------------------------------------------------------------------

impl Layout {
    /// Ajout d'enfant sur `WidgetKind::Container` —
    /// `children.push` + `setParent` (verbatim l'ordre amont :
    /// le widget est rangé puis reçoit son parent).
    pub fn container_add_child(&mut self, parent: WidgetId, child: WidgetId) {
        self.node_mut(parent, |n| {
            if let WidgetKind::Container { children } = &mut n.kind {
                children.push(child);
            }
        });
        self.widget_set_parent(child, parent);
    }
}
