//! Port de `dw/outofflowmgr.hh` + `dw/oofawarewidget.cc/.hh` +
//! `dw/oofawarewidget_iterator.cc` + `dw/regardingborder.*` +
//! `dw/ooffloatsmgr.cc/.hh` — couche « out of flow ».
//!
//! `IMPL_POS = false` verbatim (`core.hh:16`) : les
//! positionnements abs/rel/fixed sont morts amont — seuls les
//! **floats** sont actifs. Les mgrs positionnés amont sont
//! instanciés vides et inaccessibles (`getOOFMIndex` ne peut
//! pas les atteindre) → le port les représente par `None`
//! (identique observablement, documenté) jusqu'à la phase OOF
//! positionné.
//!
//! Modèle D : `OOFAwareWidget*`/`Widget*` → [`WidgetId`],
//! `outOfFlowMgr` → `Rc<RefCell<dyn OofMgr>>` dans
//! `OofAwareData` (embarqué par les kinds OOF-aware), les
//! float/TBInfo en arènes internes au mgr (les listes GB/CB
//! partagent les mêmes `Float` amont → ids + arène `floats`).
//!
//! `RegardingBorder` (tag interface) → `is_regarding_border`
//! sur le kind (Textblock/Ruler = `true`).

#![allow(clippy::cast_possible_truncation)] // `int` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::iterator::DwIterator;
use crate::layout::Layout;
use crate::style::{ClearType, FloatType, Position, StyleAttrs};
use crate::types::{
    content_type, Allocation, Content, ContentData, Extremes, GettingWidgetAtPointContext,
    Requisition,
};
use crate::view::View;
use crate::widget::WidgetKind;
use crate::WidgetId;

// ------------------------------------------------------------------
// Consts (oofawarewidget.hh) — verbatim.
// ------------------------------------------------------------------

/// `OOFM_FLOATS`.
pub const OOFM_FLOATS: usize = 0;
/// `OOFM_ABSOLUTE`.
pub const OOFM_ABSOLUTE: usize = 1;
/// `OOFM_RELATIVE`.
pub const OOFM_RELATIVE: usize = 2;
/// `OOFM_FIXED`.
pub const OOFM_FIXED: usize = 3;
/// `NUM_OOFM`.
pub const NUM_OOFM: usize = 4;
/// `OOFM_NAME`.
pub const OOFM_NAME: [&str; NUM_OOFM] = ["FLOATS", "ABSOLUTE", "RELATIVE", "FIXED"];

/// `IMPL_POS` (`core.hh:16`) — positionnement non implémenté
/// amont : les `testStyle*Positioned` sont constants `false`.
pub const IMPL_POS: bool = false;

/// `SL_*` — niveaux de dessin OOF-aware.
pub mod sl {
    /// `SL_START`.
    pub const START: i32 = 0;
    /// `SL_BACKGROUND`.
    pub const BACKGROUND: i32 = 1;
    /// `SL_SC_BOTTOM`.
    pub const SC_BOTTOM: i32 = 2;
    /// `SL_IN_FLOW`.
    pub const IN_FLOW: i32 = 3;
    /// `SL_OOF_REF`.
    pub const OOF_REF: i32 = 4;
    /// `SL_OOF_CONT`.
    pub const OOF_CONT: i32 = 5;
    /// `SL_SC_TOP`.
    pub const SC_TOP: i32 = 6;
    /// `SL_END`.
    pub const END: i32 = 7;
}

/// `stackingLevelText` — verbatim.
#[must_use]
pub fn stacking_level_text(level: i32) -> &'static str {
    match level {
        sl::START => "START",
        sl::BACKGROUND => "BACKGROUND",
        sl::SC_BOTTOM => "SC_BOTTOM",
        sl::IN_FLOW => "IN_FLOW",
        sl::OOF_REF => "OOF_REF",
        sl::OOF_CONT => "OOF_CONT",
        sl::SC_TOP => "SC_TOP",
        sl::END => "END",
        _ => "???",
    }
}

// ------------------------------------------------------------------
// parentRef helpers (oofawarewidget.hh) — verbatim.
// ------------------------------------------------------------------

/// `PARENT_REF_OOFM_BITS`.
pub const PARENT_REF_OOFM_BITS: i32 = 3;
/// `PARENT_REF_OOFM_MASK`.
pub const PARENT_REF_OOFM_MASK: i32 = (1 << PARENT_REF_OOFM_BITS) - 1;

/// `isParentRefOOF`.
#[must_use]
pub fn is_parent_ref_oof(parent_ref: i32) -> bool {
    parent_ref != -1 && (parent_ref & PARENT_REF_OOFM_MASK) != 0
}
/// `makeParentRefInFlow`.
#[must_use]
pub fn make_parent_ref_in_flow(in_flow_sub_ref: i32) -> i32 {
    in_flow_sub_ref << PARENT_REF_OOFM_BITS
}
/// `getParentRefInFlowSubRef` — `-1`/OOF hors contrat (assert amont).
#[must_use]
pub fn get_parent_ref_in_flow_sub_ref(parent_ref: i32) -> i32 {
    parent_ref >> PARENT_REF_OOFM_BITS
}
/// `makeParentRefOOF`.
#[must_use]
pub fn make_parent_ref_oof(oofm_index: usize, oofm_sub_ref: i32) -> i32 {
    (oofm_sub_ref << PARENT_REF_OOFM_BITS) | (oofm_index as i32 + 1)
}
/// `getParentRefOOFSubRef`.
#[must_use]
pub fn get_parent_ref_oof_sub_ref(parent_ref: i32) -> i32 {
    parent_ref >> PARENT_REF_OOFM_BITS
}
/// `getParentRefOOFIndex`.
#[must_use]
pub fn get_parent_ref_oof_index(parent_ref: i32) -> i32 {
    (parent_ref & PARENT_REF_OOFM_MASK) - 1
}

// ------------------------------------------------------------------
// testStyle* / testWidget* helpers — verbatim oofawarewidget.hh.
// ------------------------------------------------------------------

/// `testStyleFloat`.
#[must_use]
pub fn test_style_float(attrs: &StyleAttrs) -> bool {
    attrs.vloat != FloatType::None
}
/// `testStyleAbsolutelyPositioned` — `IMPL_POS` → `false`.
#[must_use]
pub fn test_style_absolutely_positioned(attrs: &StyleAttrs) -> bool {
    IMPL_POS && attrs.position == Position::Absolute
}
/// `testStyleFixedlyPositioned`.
#[must_use]
pub fn test_style_fixedly_positioned(attrs: &StyleAttrs) -> bool {
    IMPL_POS && attrs.position == Position::Fixed
}
/// `testStyleRelativelyPositioned`.
#[must_use]
pub fn test_style_relatively_positioned(attrs: &StyleAttrs) -> bool {
    IMPL_POS && attrs.position == Position::Relative
}
/// `testStylePositioned`.
#[must_use]
pub fn test_style_positioned(attrs: &StyleAttrs) -> bool {
    test_style_absolutely_positioned(attrs)
        || test_style_relatively_positioned(attrs)
        || test_style_fixedly_positioned(attrs)
}
/// `testStyleOutOfFlow`.
#[must_use]
pub fn test_style_out_of_flow(attrs: &StyleAttrs) -> bool {
    test_style_float(attrs)
        || test_style_absolutely_positioned(attrs)
        || test_style_relatively_positioned(attrs)
        || test_style_fixedly_positioned(attrs)
}
/// `mustAddBreaks` (textblock.hh inline).
#[must_use]
pub fn must_add_breaks(attrs: &StyleAttrs) -> bool {
    !test_style_out_of_flow(attrs) || test_style_relatively_positioned(attrs)
}

/// `isEstablishingStackingContext` — `IMPL_POS` → `false`
/// verbatim (stackingcontextmgr.hh).
#[must_use]
pub fn is_establishing_stacking_context(attrs: &StyleAttrs) -> bool {
    IMPL_POS && attrs.position != Position::Static && attrs.z_index != -1
}

// ------------------------------------------------------------------
// OofAwareData — ≙ champs OOFAwareWidget (oofContainer,
// outOfFlowMgr, requisitionWithoutOOF).
// ------------------------------------------------------------------

/// Champs `OOFAwareWidget` — embarqués par les kinds OOF-aware
/// (`WidgetKind::OofAware`, `Textblock`, `Table`, `Ruler`).
///
/// `Debug` manuel : les mgrs sont des trait objects.
pub struct OofAwareData {
    /// `oofContainer[NUM_OOFM]` — containing blocks par rôle.
    pub oof_container: [Option<WidgetId>; NUM_OOFM],
    /// `outOfFlowMgr[NUM_OOFM]` — mgrs du container ≙ champ
    /// amont (`Rc<RefCell<..>>` : accédés sous `&mut Layout` pendant
    /// que le nœud est emprunté).
    pub out_of_flow_mgr: [Option<Rc<RefCell<dyn OofMgr>>>; NUM_OOFM],
    /// `requisitionWithoutOOF`.
    pub requisition_without_oof: Requisition,
}

impl OofAwareData {
    /// Ctor verbatim : tous `NULL`.
    #[must_use]
    pub fn new() -> Self {
        OofAwareData {
            oof_container: [None; NUM_OOFM],
            out_of_flow_mgr: [None, None, None, None],
            requisition_without_oof: Requisition::default(),
        }
    }
}

impl Default for OofAwareData {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for OofAwareData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OofAwareData")
            .field("oof_container", &self.oof_container)
            .field(
                "out_of_flow_mgr",
                &self
                    .out_of_flow_mgr
                    .iter()
                    .map(|m| m.as_ref().map(|_| "mgr"))
                    .collect::<Vec<_>>(),
            )
            .field("requisition_without_oof", &self.requisition_without_oof)
            .finish()
    }
}

/// `instanceOf (OOFAwareWidget)` ≙ kind porteur d'`OofAwareData`.
#[must_use]
pub fn is_oof_aware_kind(kind: &WidgetKind) -> bool {
    match kind {
        WidgetKind::OofAware { .. }
        | WidgetKind::Textblock { .. }
        | WidgetKind::Ruler { .. }
        | WidgetKind::Table { .. } => true,
        WidgetKind::Base
        | WidgetKind::Container { .. }
        | WidgetKind::Text { .. }
        | WidgetKind::Bullet
        | WidgetKind::Image { .. }
        | WidgetKind::Embed { .. } => false,
    }
}

/// `instanceOf (RegardingBorder)` — `Textblock` et `Ruler`
/// héritent `RegardingBorder` amont ; `OofAware` nu n'en est pas.
#[must_use]
pub fn is_regarding_border_kind(kind: &WidgetKind) -> bool {
    matches!(
        kind,
        WidgetKind::Textblock { .. } | WidgetKind::Ruler { .. }
    )
}

// ------------------------------------------------------------------
// OofMgr — ≙ OutOfFlowMgr (outofflowmgr.hh).
// ------------------------------------------------------------------

/// `dw::oof::OutOfFlowMgr` — trait object. Widgets en
/// [`WidgetId`] ; `&mut Layout` partout où amont mute l'arbre
/// (requêtes de taille comprises — cache requisition).
pub trait OofMgr {
    /// `sizeAllocateStart`.
    fn size_allocate_start(
        &mut self,
        layout: &mut Layout,
        caller: WidgetId,
        allocation: &Allocation,
    );
    /// `sizeAllocateEnd`.
    fn size_allocate_end(&mut self, layout: &mut Layout, caller: WidgetId);
    /// `containerSizeChangedForChildren`.
    fn container_size_changed_for_children(&mut self, layout: &mut Layout);
    /// `draw`.
    fn draw(
        &mut self,
        layout: &mut Layout,
        view: &mut dyn View,
        area: &crate::types::Rectangle,
        context: &mut crate::types::DrawingContext,
    );
    /// `markSizeChange`.
    fn mark_size_change(&mut self, layout: &mut Layout, ref_: i32);
    /// `markExtremesChange`.
    fn mark_extremes_change(&mut self, layout: &mut Layout, ref_: i32);
    /// `getWidgetAtPoint`.
    fn get_widget_at_point(
        &mut self,
        layout: &mut Layout,
        x: i32,
        y: i32,
        context: &mut GettingWidgetAtPointContext,
    ) -> Option<WidgetId>;
    /// `addWidgetInFlow`.
    fn add_widget_in_flow(
        &mut self,
        layout: &mut Layout,
        widget: WidgetId,
        parent: Option<WidgetId>,
        external_index: i32,
    );
    /// `addWidgetOOF` → sub-ref.
    fn add_widget_oof(
        &mut self,
        layout: &mut Layout,
        widget: WidgetId,
        generator: WidgetId,
        external_index: i32,
    ) -> i32;
    /// `calcWidgetRefSize`.
    fn calc_widget_ref_size(
        &mut self,
        layout: &mut Layout,
        widget: WidgetId,
        size: &mut Requisition,
    );
    /// `moveExternalIndices`.
    fn move_external_indices(
        &mut self,
        layout: &mut Layout,
        generator: WidgetId,
        old_start_index: i32,
        diff: i32,
    );
    /// `tellPosition1`.
    fn tell_position1(&mut self, layout: &mut Layout, widget: WidgetId, x: i32, y: i32);
    /// `tellPosition2`.
    fn tell_position2(&mut self, layout: &mut Layout, widget: WidgetId, x: i32, y: i32);
    /// `tellIncompletePosition1`.
    fn tell_incomplete_position1(
        &mut self,
        layout: &mut Layout,
        generator: WidgetId,
        widget: WidgetId,
        x: i32,
        y: i32,
    );
    /// `tellIncompletePosition2`.
    fn tell_incomplete_position2(
        &mut self,
        layout: &mut Layout,
        generator: WidgetId,
        widget: WidgetId,
        x: i32,
        y: i32,
    );
    /// `getSize`.
    fn get_size(
        &mut self,
        layout: &mut Layout,
        container_req: &Requisition,
        oof_width: &mut i32,
        oof_height: &mut i32,
    );
    /// `containerMustAdjustExtraSpace`.
    fn container_must_adjust_extra_space(&self) -> bool;
    /// `getExtremes`.
    fn get_extremes(
        &mut self,
        layout: &mut Layout,
        container_extr: &Extremes,
        oof_min_width: &mut i32,
        oof_max_width: &mut i32,
    );
    /// `getLeftBorder`.
    fn get_left_border(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32;
    /// `getRightBorder`.
    fn get_right_border(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32;
    /// `hasFloatLeft`.
    fn has_float_left(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> bool;
    /// `hasFloatRight`.
    fn has_float_right(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> bool;
    /// `getLeftFloatHeight`.
    fn get_left_float_height(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32;
    /// `getRightFloatHeight`.
    fn get_right_float_height(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32;
    /// `affectsLeftBorder`.
    fn affects_left_border(&self, layout: &Layout, widget: WidgetId) -> bool;
    /// `affectsRightBorder`.
    fn affects_right_border(&self, layout: &Layout, widget: WidgetId) -> bool;
    /// `mayAffectBordersAtAll`.
    fn may_affect_borders_at_all(&self) -> bool;
    /// `getClearPosition`.
    fn get_clear_position(&mut self, layout: &mut Layout, widget: WidgetId) -> i32;
    /// `dealingWithSizeOfChild`.
    fn dealing_with_size_of_child(&self, child: WidgetId) -> bool;
    /// `getAvailWidthOfChild`.
    fn get_avail_width_of_child(
        &mut self,
        layout: &mut Layout,
        child: WidgetId,
        force_value: bool,
    ) -> i32;
    /// `getAvailHeightOfChild`.
    fn get_avail_height_of_child(
        &mut self,
        layout: &mut Layout,
        child: WidgetId,
        force_value: bool,
    ) -> i32;
    /// `getNumWidgets`.
    fn get_num_widgets(&self) -> usize;
    /// `getWidget`.
    fn get_widget(&self, i: usize) -> Option<WidgetId>;
}

// ------------------------------------------------------------------
// Accès `OofAwareData` dans l'arène (≙ `OOFAwareWidget*`).
// ------------------------------------------------------------------

impl Layout {
    /// Lecture des données OOF-aware d'un nœud (`None` si le
    /// kind n'est pas OOF-aware ≙ `instanceOf` faux amont).
    pub(crate) fn node_oof<T>(&self, w: WidgetId, f: impl FnOnce(&OofAwareData) -> T) -> Option<T> {
        self.node(w, |n| match &n.kind {
            WidgetKind::OofAware { oof } | WidgetKind::Ruler { oof } => Some(f(oof)),
            WidgetKind::Textblock { tb } => Some(f(&tb.oof)),
            WidgetKind::Table { tbl } => Some(f(&tbl.oof)),
            _ => None,
        })
    }

    /// Mutation des données OOF-aware.
    pub(crate) fn node_oof_mut<T>(
        &self,
        w: WidgetId,
        f: impl FnOnce(&mut OofAwareData) -> T,
    ) -> Option<T> {
        self.node_mut(w, |n| match &mut n.kind {
            WidgetKind::OofAware { oof } | WidgetKind::Ruler { oof } => Some(f(oof)),
            WidgetKind::Textblock { tb } => Some(f(&mut tb.oof)),
            WidgetKind::Table { tbl } => Some(f(&mut tbl.oof)),
            _ => None,
        })
    }

    /// `isWidgetOOF (widget)` — `isParentRefOOF (widget->parentRef)`.
    #[must_use]
    pub fn widget_is_oof(&self, w: WidgetId) -> bool {
        is_parent_ref_oof(self.node(w, |n| n.parent_ref))
    }

    /// `getWidgetInFlowSubRef` — hors contrat si OOF.
    #[must_use]
    pub fn widget_in_flow_sub_ref(&self, w: WidgetId) -> i32 {
        get_parent_ref_in_flow_sub_ref(self.node(w, |n| n.parent_ref))
    }

    /// `getWidgetOOFSubRef`.
    #[must_use]
    pub fn widget_oof_sub_ref(&self, w: WidgetId) -> i32 {
        get_parent_ref_oof_sub_ref(self.node(w, |n| n.parent_ref))
    }

    /// `getWidgetOOFIndex` → `OOFM_*`.
    #[must_use]
    pub fn widget_oof_index(&self, w: WidgetId) -> i32 {
        get_parent_ref_oof_index(self.node(w, |n| n.parent_ref))
    }

    /// `searchOutOfFlowMgr` — mgr du containing block, `None` si
    /// pas encore créé.
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn search_out_of_flow_mgr(
        &self,
        w: WidgetId,
        oofm_index: usize,
    ) -> Option<Rc<RefCell<dyn OofMgr>>> {
        self.node_oof(w, |o| o.oof_container[oofm_index])
            .flatten()
            .and_then(|c| {
                self.node_oof(c, |o| o.out_of_flow_mgr[oofm_index].clone())
                    .flatten()
            })
    }

    /// `getParentRefOutOfFlowMgr` — `outOfFlowMgr[getParentRefOOFIndex
    /// (parentRef)]` sur *ce* widget (containing block).
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn parent_ref_out_of_flow_mgr(
        &self,
        w: WidgetId,
        parent_ref: i32,
    ) -> Option<Rc<RefCell<dyn OofMgr>>> {
        if !is_parent_ref_oof(parent_ref) {
            return None;
        }
        let i = get_parent_ref_oof_index(parent_ref) as usize;
        if i >= NUM_OOFM {
            return None;
        }
        self.node_oof(w, |o| o.out_of_flow_mgr[i].clone()).flatten()
    }

    /// `getWidgetOutOfFlowMgr` — mgr qui gère `widget` OOF (≙
    /// `getParentRefOutOfFlowMgr (widget->parentRef)` appelé sur
    /// le containing block — amont l'appelle sur le parent OOF-
    /// aware qui détient les mgrs).
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn widget_out_of_flow_mgr(
        &self,
        container: WidgetId,
        widget: WidgetId,
    ) -> Option<Rc<RefCell<dyn OofMgr>>> {
        self.parent_ref_out_of_flow_mgr(container, self.node(widget, |n| n.parent_ref))
    }
}

// ------------------------------------------------------------------
// OOFAwareWidget — verbatim oofawarewidget.cc.
// ------------------------------------------------------------------

impl Layout {
    /// `OOFAwareWidget::notifySetAsTopLevel` — tous les
    /// containing blocks = soi-même (oofawarewidget.cc:86).
    pub(crate) fn oof_notify_set_as_top_level(&mut self, w: WidgetId) {
        let _ = self.node_oof_mut(w, |o| {
            o.oof_container = [Some(w); NUM_OOFM];
        });
    }

    /// `OOFAwareWidget::getOOFMIndex` — verbatim : float →
    /// `OOFM_FLOATS`, sinon positionné (mort, `IMPL_POS`) ;
    /// `assertNotReached` → `debug_assert` + `OOFM_FLOATS`
    /// (oofawarewidget.cc:94).
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn get_oofm_index(&self, w: WidgetId) -> usize {
        let attrs = self.node(w, |n| n.style.as_ref().map(|s| s.attrs.clone()));
        let Some(attrs) = attrs else {
            debug_assert!(false, "getOOFMIndex: widget sans style");
            return OOFM_FLOATS;
        };
        if test_style_float(&attrs) {
            OOFM_FLOATS
        } else if test_style_absolutely_positioned(&attrs) {
            OOFM_ABSOLUTE
        } else if test_style_relatively_positioned(&attrs) {
            OOFM_RELATIVE
        } else if test_style_fixedly_positioned(&attrs) {
            OOFM_FIXED
        } else {
            debug_assert!(false, "getOOFMIndex: widget non OOF");
            OOFM_FLOATS
        }
    }

    /// `OOFAwareWidget::isOOFContainer` — verbatim
    /// (oofawarewidget.cc:127). Les kinds `isPossibleOOFContainer*`
    /// sont dispatchés via [`Layout::w_is_possible_oof_container`].
    #[must_use]
    pub(crate) fn is_oof_container(&self, w: WidgetId, oofm_index: usize) -> bool {
        match oofm_index {
            OOFM_FLOATS => {
                let attrs = self.node(w, |n| n.style.as_ref().map(|s| s.attrs.clone()));
                let oof_aware = self.node(w, |n| is_oof_aware_kind(&n.kind));
                let parent = self.widget_parent(w);
                oof_aware
                    && (self.w_is_possible_oof_container(w, OOFM_FLOATS)
                        && (parent.is_none()
                            || !parent.is_some_and(|p| {
                                self.node(p, |n| is_oof_aware_kind(&n.kind))
                                    && self.w_is_possible_oof_container_parent(p, OOFM_FLOATS)
                            })
                            || attrs.as_ref().is_some_and(|a| {
                                a.display == crate::style::DisplayType::InlineBlock
                                    || a.overflow != crate::style::Overflow::Visible
                                    || test_style_out_of_flow(a)
                            })))
            }
            OOFM_RELATIVE | OOFM_ABSOLUTE => {
                self.node(w, |n| is_oof_aware_kind(&n.kind))
                    && (self.widget_parent(w).is_none()
                        || self
                            .node(w, |n| n.style.as_ref().map(|s| s.attrs.clone()))
                            .is_some_and(|a| test_style_positioned(&a)))
            }
            OOFM_FIXED => self.widget_parent(w).is_none(),
            _ => {
                debug_assert!(false, "isOOFContainer: oofmIndex invalide");
                false
            }
        }
    }

    /// `OOFAwareWidget::notifySetParent` — recherche des
    /// containing blocks en remontant les parents
    /// (oofawarewidget.cc:185).
    pub(crate) fn oof_notify_set_parent(&mut self, w: WidgetId) {
        if self.node_oof(w, |_| ()).is_none() {
            return;
        }
        for oofm_index in 0..NUM_OOFM {
            let mut container = None;
            let mut cur = Some(w);
            while let Some(widget) = cur {
                if container.is_some() {
                    break;
                }
                if self.is_oof_container(widget, oofm_index) {
                    container = Some(widget);
                }
                cur = self.widget_parent(widget);
            }
            debug_assert!(
                container.is_some(),
                "notifySetParent: pas de containing block OOF"
            );
            let _ = self.node_oof_mut(w, |o| o.oof_container[oofm_index] = container);
        }
    }

    /// `OOFAwareWidget::initOutOfFlowMgrs` — verbatim
    /// (oofawarewidget.cc:205) : crée les mgrs sur les
    /// containing blocks si absents. Les mgrs positionnés
    /// (`IMPL_POS`) ne sont jamais instanciés dans un code
    /// atteignable amont — le port les laisse `None` (aucun
    /// accès possible : `getOOFMIndex` ne peut pas les
    /// produire). Le floats-mgr reste réel. Appelé par les
    /// kinds générateurs (textblock.cc:2494 — seam, pas encore
    /// de consommateur dans le port).
    #[allow(dead_code)]
    pub(crate) fn init_out_of_flow_mgrs(&mut self, w: WidgetId) {
        let Some(container) = self.node_oof(w, |o| o.oof_container[OOFM_FLOATS]).flatten() else {
            return;
        };
        let has_mgr = self
            .node_oof(container, |o| o.out_of_flow_mgr[OOFM_FLOATS].is_some())
            .unwrap_or(false);
        if !has_mgr {
            let mgr: Rc<RefCell<dyn OofMgr>> = Rc::new(RefCell::new(OofFloatsMgr::new(
                self,
                container,
                OOFM_FLOATS,
            )));
            let _ = self.node_oof_mut(container, |o| o.out_of_flow_mgr[OOFM_FLOATS] = Some(mgr));
        }
    }

    /// `OOFAwareWidget::correctRequisitionByOOF` — verbatim
    /// (oofawarewidget.cc:236). `split_height` ≙ le funpointeur
    /// amont (`Textblock::splitHeightPreservingLineHeight`…).
    pub(crate) fn correct_requisition_by_oof(
        &mut self,
        w: WidgetId,
        requisition: &mut Requisition,
        split_height: impl Fn(i32, &mut i32, &mut i32),
    ) {
        let mgrs = self
            .node_oof(w, |o| o.out_of_flow_mgr.clone())
            .unwrap_or([None, None, None, None]);
        let mut req_without = *requisition;
        for mgr in mgrs.iter().flatten() {
            let mut oof_width = 0;
            let mut oof_height = 0;
            mgr.borrow_mut()
                .get_size(self, requisition, &mut oof_width, &mut oof_height);
            let adjust = mgr.borrow().container_must_adjust_extra_space()
                && self.w_adjust_extra_space_when_correcting_requisition_by_oof(w);
            if oof_width > requisition.width && adjust {
                let () = self.node_mut(w, |n| {
                    n.extra_space.right = n.extra_space.right.max(oof_width - requisition.width);
                });
            }
            if oof_width > requisition.width {
                requisition.width = oof_width;
            }
            if oof_height > requisition.ascent + requisition.descent {
                if adjust {
                    let () = self.node_mut(w, |n| {
                        n.extra_space.bottom = n
                            .extra_space
                            .bottom
                            .max(oof_height - (requisition.ascent + requisition.descent));
                    });
                }
                split_height(
                    oof_height,
                    &mut requisition.ascent,
                    &mut requisition.descent,
                );
            }
            if !self.w_adjust_extra_space_when_correcting_requisition_by_oof(w) {
                req_without.width = req_without.width.max(oof_width);
                if oof_height > req_without.ascent + req_without.descent {
                    split_height(
                        oof_height,
                        &mut req_without.ascent,
                        &mut req_without.descent,
                    );
                }
            }
        }
        let _ = self.node_oof_mut(w, |o| o.requisition_without_oof = req_without);
    }

    /// `OOFAwareWidget::correctExtremesByOOF` — verbatim
    /// (oofawarewidget.cc:306), y compris le probable
    /// copier-coller amont `maxWidthIntrinsic` vs `oofMinWidth`.
    pub(crate) fn correct_extremes_by_oof(&mut self, w: WidgetId, extremes: &mut Extremes) {
        let mgrs = self
            .node_oof(w, |o| o.out_of_flow_mgr.clone())
            .unwrap_or([None, None, None, None]);
        for mgr in mgrs.iter().flatten() {
            let mut oof_min_width = 0;
            let mut oof_max_width = 0;
            mgr.borrow_mut()
                .get_extremes(self, extremes, &mut oof_min_width, &mut oof_max_width);
            extremes.min_width = extremes.min_width.max(oof_min_width);
            extremes.min_width_intrinsic = extremes.min_width_intrinsic.max(oof_min_width);
            extremes.max_width = extremes.max_width.max(oof_max_width);
            // Verbatim amont : `oofMinWidth` (pas `oofMaxWidth`)
            // — probable copier-coller amont conservé.
            extremes.max_width_intrinsic = extremes.max_width_intrinsic.max(oof_min_width);
            extremes.adjustment_width = extremes.adjustment_width.max(oof_min_width);
        }
    }

    /// `OOFAwareWidget::sizeAllocateStart` — verbatim
    /// (oofawarewidget.cc:342) : notifie les mgrs *des
    /// containing blocks* (pas les siens).
    pub(crate) fn oof_size_allocate_start(&mut self, w: WidgetId, allocation: &Allocation) {
        for i in 0..NUM_OOFM {
            if let Some(mgr) = self
                .node_oof(w, |o| o.oof_container[i])
                .flatten()
                .and_then(|c| self.node_oof(c, |o| o.out_of_flow_mgr[i].clone()).flatten())
            {
                mgr.borrow_mut().size_allocate_start(self, w, allocation);
            }
        }
    }

    /// `OOFAwareWidget::sizeAllocateEnd` — verbatim
    /// (oofawarewidget.cc:350).
    pub(crate) fn oof_size_allocate_end(&mut self, w: WidgetId) {
        for i in 0..NUM_OOFM {
            if let Some(mgr) = self
                .node_oof(w, |o| o.oof_container[i])
                .flatten()
                .and_then(|c| self.node_oof(c, |o| o.out_of_flow_mgr[i].clone()).flatten())
            {
                mgr.borrow_mut().size_allocate_end(self, w);
            }
        }
    }

    /// `OOFAwareWidget::containerSizeChangedForChildrenOOF` —
    /// verbatim (oofawarewidget.cc:357) : notifie *ses propres*
    /// mgrs.
    pub(crate) fn oof_container_size_changed_for_children(&mut self, w: WidgetId) {
        let mgrs = self
            .node_oof(w, |o| o.out_of_flow_mgr.clone())
            .unwrap_or([None, None, None, None]);
        for mgr in mgrs.iter().flatten() {
            mgr.borrow_mut().container_size_changed_for_children(self);
        }
    }

    /// `OOFAwareWidget::doesWidgetOOFInterruptDrawing` —
    /// `IMPL_POS` → `false` verbatim (oofawarewidget.cc:364).
    /// Consommé par le dessin textblock (seam).
    #[allow(dead_code, clippy::unused_self)]
    #[must_use]
    pub(crate) fn does_widget_oof_interrupt_drawing(&self, _w: WidgetId) -> bool {
        // Le code amont derrière `IMPL_POS` compare les niveaux
        // de stacking context ; mort tant que `IMPL_POS` vaut
        // `false` (verbatim).
        false
    }

    /// `OOFAwareWidget::draw` — boucle `SL_START+1 .. SL_END`
    /// (oofawarewidget.cc:386).
    pub(crate) fn oof_draw(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        area: &crate::types::Rectangle,
        context: &mut crate::types::DrawingContext,
    ) {
        for level in (sl::START + 1)..sl::END {
            self.w_draw_level(w, view, area, level, context);
        }
    }

    /// Dispatch `drawLevel` — `Textblock` surcharge
    /// (textblock.cc:1731 : `SL_IN_FLOW`/`SL_OOF_REF`), sinon base
    /// `OOFAwareWidget`.
    pub(crate) fn w_draw_level(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        area: &crate::types::Rectangle,
        level: i32,
        context: &mut crate::types::DrawingContext,
    ) {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            self.tb_draw_level(w, view, area, level, context);
            return;
        }
        // `Table::drawLevel` — `SL_IN_FLOW` dessine les cellules
        // verbatim (table.cc:377).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            self.tbl_draw_level(w, view, area, level, context);
            return;
        }
        self.oof_draw_level(w, view, area, level, context);
    }

    /// `OOFAwareWidget::drawLevel` — verbatim
    /// (oofawarewidget.cc:397). `SL_IN_FLOW`/`SL_OOF_REF` →
    /// sous-classes : kind `OofAware` nu = no-op comme amont.
    #[allow(clippy::match_same_arms)]
    pub(crate) fn oof_draw_level(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        area: &crate::types::Rectangle,
        level: i32,
        context: &mut crate::types::DrawingContext,
    ) {
        match level {
            sl::BACKGROUND => self.widget_draw_widget_box(w, view, area, false),
            // `SC_BOTTOM`/`SC_TOP` : `stackingContextMgr` —
            // seam (A08 : mgr non encore porté ; amont teste
            // != NULL → no-op).
            sl::SC_BOTTOM | sl::SC_TOP | sl::IN_FLOW | sl::OOF_REF | sl::START | sl::END => {}
            sl::OOF_CONT => self.oof_draw_oof(w, view, area, context),
            _ => {}
        }
    }

    /// `OOFAwareWidget::drawOOF` — verbatim
    /// (oofawarewidget.cc:447).
    pub(crate) fn oof_draw_oof(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        area: &crate::types::Rectangle,
        context: &mut crate::types::DrawingContext,
    ) {
        let mgrs = self
            .node_oof(w, |o| o.out_of_flow_mgr.clone())
            .unwrap_or([None, None, None, None]);
        for mgr in mgrs.iter().flatten() {
            mgr.borrow_mut().draw(self, view, area, context);
        }
    }

    /// `OOFAwareWidget::getWidgetAtPoint` — verbatim
    /// (oofawarewidget.cc:456) : parcourt les niveaux du haut
    /// (`SL_END-1`) vers le bas (`SL_START+1`).
    pub(crate) fn oof_get_widget_at_point(
        &mut self,
        w: WidgetId,
        x: i32,
        y: i32,
        context: &mut GettingWidgetAtPointContext,
    ) -> Option<WidgetId> {
        if !self.node(w, |n| n.in_allocation(x, y)) {
            return None;
        }
        for level in ((sl::START + 1)..sl::END).rev() {
            if let Some(found) = self.w_get_widget_at_point_level(w, x, y, level, context) {
                return Some(found);
            }
        }
        None
    }

    /// Dispatch `getWidgetAtPointLevel` — `Textblock` surcharge
    /// (textblock.cc:2891 : `SL_IN_FLOW`/`SL_OOF_REF`), sinon base
    /// `OOFAwareWidget`.
    pub(crate) fn w_get_widget_at_point_level(
        &mut self,
        w: WidgetId,
        x: i32,
        y: i32,
        level: i32,
        context: &mut GettingWidgetAtPointContext,
    ) -> Option<WidgetId> {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            return self.tb_get_widget_at_point_level(w, x, y, level, context);
        }
        // `Table::getWidgetAtPointLevel` — verbatim
        // (table.cc:425).
        if self.node(w, |n| matches!(&n.kind, WidgetKind::Table { .. })) {
            return self.tbl_get_widget_at_point_level(w, x, y, level, context);
        }
        self.oof_get_widget_at_point_level(w, x, y, level, context)
    }

    /// `OOFAwareWidget::getWidgetAtPointLevel` — verbatim
    /// (oofawarewidget.cc:473).
    #[allow(clippy::match_same_arms)]
    pub(crate) fn oof_get_widget_at_point_level(
        &mut self,
        w: WidgetId,
        x: i32,
        y: i32,
        level: i32,
        context: &mut GettingWidgetAtPointContext,
    ) -> Option<WidgetId> {
        match level {
            sl::BACKGROUND => {
                if self.node(w, |n| n.in_allocation(x, y)) {
                    Some(w)
                } else {
                    None
                }
            }
            // `SC_BOTTOM`/`SC_TOP` : `stackingContextMgr` seam ;
            // `IN_FLOW` : `assertNotReached` amont pour la base
            // (kind `OofAware` nu → `None` ; textblock
            // surchargera) ; `OOF_REF` : sous-classes.
            sl::SC_BOTTOM | sl::SC_TOP | sl::IN_FLOW | sl::OOF_REF => None,
            sl::OOF_CONT => self.oof_get_widget_oof_at_point(w, x, y, context),
            _ => None,
        }
    }

    /// `OOFAwareWidget::getWidgetOOFAtPoint` — verbatim
    /// (oofawarewidget.cc:526) : mgrs de `NUM_OOFM-1` vers 0.
    pub(crate) fn oof_get_widget_oof_at_point(
        &mut self,
        w: WidgetId,
        x: i32,
        y: i32,
        context: &mut GettingWidgetAtPointContext,
    ) -> Option<WidgetId> {
        let mgrs = self
            .node_oof(w, |o| o.out_of_flow_mgr.clone())
            .unwrap_or([None, None, None, None]);
        for mgr in mgrs.iter().rev().flatten() {
            if let Some(found) = mgr.borrow_mut().get_widget_at_point(self, x, y, context) {
                return Some(found);
            }
        }
        None
    }

    /// `OOFAwareWidget::removeChild` — atteint seulement depuis
    /// `~OOFAwareWidget` pour des widgets OOF (assert amont
    /// (oofawarewidget.cc:540) → `debug_assert`).
    pub(crate) fn oof_remove_child(&mut self, _w: WidgetId, child: WidgetId) {
        debug_assert!(self.widget_is_oof(child));
    }
}

// ------------------------------------------------------------------
// Virtuels OOFAwareWidget — dispatch kind (défauts verbatim
// oofawarewidget.cc:548-614).
// ------------------------------------------------------------------

#[allow(clippy::unused_self)] // signatures verbatim amont.
impl Layout {
    /// `OOFAwareWidget::updateReference` — `notImplemented`
    /// amont → no-op (le textblock surchargera).
    pub(crate) fn w_update_reference(&mut self, w: WidgetId, ref_: i32) {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            self.tb_update_reference(w, ref_);
            return;
        }
        let _ = (w, ref_);
    }

    /// `OOFAwareWidget::widgetRefSizeChanged` — `notImplemented`
    /// amont → no-op (seam — textblock surchargera).
    #[allow(dead_code)]
    pub(crate) fn w_widget_ref_size_changed(&mut self, w: WidgetId, external_index: i32) {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            self.tb_widget_ref_size_changed(w, external_index);
            return;
        }
        let _ = (w, external_index);
    }

    /// `OOFAwareWidget::oofSizeChanged` — verbatim
    /// (oofawarewidget.cc:558) : `queueResize (-1, ext)` +
    /// `containerSizeChanged` si extrêmes (seam — appelée par
    /// les mgrs depuis `sizeAllocateEnd` des kinds concrets).
    #[allow(dead_code)]
    pub(crate) fn w_oof_size_changed(&mut self, w: WidgetId, extremes_changed: bool) {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            self.tb_oof_size_changed(w, extremes_changed);
            return;
        }
        self.widget_queue_resize(w, -1, extremes_changed);
        if extremes_changed {
            self.widget_container_size_changed(w);
        }
    }

    /// `getGeneratorX` — override `Textblock`
    /// (textblock.cc:3113) ; défaut `OOFAwareWidget` =
    /// `notImplemented` → 0.
    pub(crate) fn w_get_generator_x(&mut self, w: WidgetId, oofm_index: usize) -> i32 {
        if let Some(x) = self.node_tb(w, |tb| self.tb_get_generator_x(w, tb, oofm_index)) {
            return x;
        }
        let _ = (w, oofm_index);
        0
    }

    /// `getGeneratorY` — override `Textblock`
    /// (textblock.cc:3131) ; défaut `notImplemented` → 0.
    pub(crate) fn w_get_generator_y(&mut self, w: WidgetId, oofm_index: usize) -> i32 {
        if let Some(y) = self.node_tb(w, |tb| self.tb_get_generator_y(w, tb, oofm_index)) {
            return y;
        }
        let _ = (w, oofm_index);
        0
    }

    /// `getGeneratorRest` — override `Textblock`
    /// (textblock.cc:3147) ; défaut `OOFAwareWidget` = absent
    /// amont (seul Textblock l'implémente) → 0.
    pub(crate) fn w_get_generator_rest(&mut self, w: WidgetId, oofm_index: usize) -> i32 {
        if let Some(r) = self.with_tb(w, |l, tb| l.tb_get_generator_rest(w, tb, oofm_index)) {
            return r;
        }
        let _ = (w, oofm_index);
        0
    }

    /// `getGeneratorWidth` — override `Textblock`
    /// (textblock.cc:3170) ; défaut `notImplemented` → 0.
    pub(crate) fn w_get_generator_width(&mut self, w: WidgetId) -> i32 {
        if let Some(v) = self.with_tb(w, |l, tb| l.tb_get_generator_width(w, tb)) {
            return v;
        }
        let _ = w;
        0
    }

    /// `getMaxGeneratorWidth` — override `Textblock`
    /// (textblock.cc:3215) ; défaut `notImplemented` → 0.
    pub(crate) fn w_get_max_generator_width(&mut self, w: WidgetId) -> i32 {
        if let Some(v) = self.node_tb(w, |tb| self.tb_get_max_generator_width(tb)) {
            return v;
        }
        let _ = w;
        0
    }

    /// `usesMaxGeneratorWidth` — override `Textblock`
    /// (textblock.cc:3223) ; défaut `notImplemented` → `false`.
    pub(crate) fn w_uses_max_generator_width(&mut self, w: WidgetId) -> bool {
        if let Some(v) = self.node_tb(w, |tb| self.tb_uses_max_generator_width(w, tb)) {
            return v;
        }
        let _ = w;
        false
    }

    /// `isPossibleOOFContainer` — `Textblock` retourne `true`
    /// verbatim (textblock.cc:3253) ; défaut :
    /// `oofmIndex != OOFM_FLOATS` (oofawarewidget.cc:601).
    #[allow(clippy::unused_self)]
    pub(crate) fn w_is_possible_oof_container(&self, w: WidgetId, oofm_index: usize) -> bool {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            return true;
        }
        oofm_index != OOFM_FLOATS
    }

    /// `isPossibleOOFContainerParent` — `Textblock` retourne
    /// `true` verbatim (textblock.cc:3257) ; défaut
    /// (oofawarewidget.cc:606).
    #[allow(clippy::unused_self)]
    pub(crate) fn w_is_possible_oof_container_parent(
        &self,
        w: WidgetId,
        oofm_index: usize,
    ) -> bool {
        if self.node(w, |n| crate::textblock::is_textblock_kind(&n.kind)) {
            return true;
        }
        oofm_index != OOFM_FLOATS
    }

    /// `OOFAwareWidget::adjustExtraSpaceWhenCorrectingRequisitionByOOF`
    /// — verbatim : `true` (oofawarewidget.cc:611) ;
    /// `tablecell::` → `false` (tablecell.hh:37).
    #[allow(clippy::unused_self)]
    pub(crate) fn w_adjust_extra_space_when_correcting_requisition_by_oof(
        &self,
        w: WidgetId,
    ) -> bool {
        if self.node_tb(w, |tb| tb.cell.is_some()).unwrap_or(false) {
            return false;
        }
        true
    }
}

// ------------------------------------------------------------------
// OofFloatsMgr — ≙ dw::oof::OOFFloatsMgr (ooffloatsmgr.cc).
// Arènes internes : `floats` partagée par les listes GB/CB
// (amont : objets Float* partagés) ; `tb_infos` idem pour
// TBInfo (chaîne `parent` par ids).
// ------------------------------------------------------------------

/// `Side` — `enum { LEFT, RIGHT }` (ooffloatsmgr.hh:22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    /// `LEFT`.
    Left,
    /// `RIGHT`.
    Right,
}

/// `OOFFloatsMgr::Float` — id d'arène = position dans
/// [`OofFloatsMgr::floats`]. `widget`/`generator` `None` ≙
/// `NULL` amont (clés de recherche).
#[derive(Debug, Clone)]
struct Float {
    /// `getWidget ()`.
    widget: Option<WidgetId>,
    /// `generator` — bloc générateur (`OOFAwareWidget`).
    generator: Option<WidgetId>,
    /// `externalIndex`.
    external_index: i32,
    /// `index` — position dans la liste de son côté.
    index: i32,
    /// `yReq` — relatif au container.
    y_req: i32,
    /// `yReal` — relatif au container.
    y_real: i32,
    /// `sideSpanningIndex`.
    side_spanning_index: i32,
    /// `size`.
    size: Requisition,
    /// `dirty`.
    dirty: bool,
}

/// `OOFFloatsMgr::TBInfo` — id = position dans `tb_infos`.
#[derive(Debug)]
struct TbInfo {
    /// `getOOFAwareWidget ()` — `None` ≙ clé de recherche.
    widget: Option<WidgetId>,
    /// `index` — position dans `tb_infos`.
    index: i32,
    /// `y` — utilisé pour le tri (clés de recherche).
    y: i32,
    /// `parent`.
    parent: Option<usize>,
    /// `parentExtIndex`.
    parent_ext_index: i32,
    /// `leftFloats` — floats définis par ce générateur, côté
    /// gauche (ordre de définition ; ids d'arène `floats`).
    left_floats: Vec<usize>,
    /// `rightFloats`.
    right_floats: Vec<usize>,
}

/// `dw::oof::OOFFloatsMgr` — mgr de floats (le seul actif,
/// `IMPL_POS` excluant les positionnés).
pub struct OofFloatsMgr {
    /// `container`.
    container: WidgetId,
    /// `oofmIndex`.
    oofm_index: usize,
    /// `containerAllocation`.
    container_allocation: Allocation,
    /// Arène `Float` partagée (≙ ownership `left/rightFloats`
    /// amont + références `tbInfos`/`floatsByWidget`).
    floats: Vec<Float>,
    /// `leftFloats` — `SortedFloatsVector`, ids d'arène.
    left_floats: Vec<usize>,
    /// `rightFloats`.
    right_floats: Vec<usize>,
    /// `floatsByWidget`.
    floats_by_widget: HashMap<WidgetId, usize>,
    /// `tbInfos`.
    tb_infos: Vec<TbInfo>,
    /// `tbInfosByOOFAwareWidget`.
    tb_infos_by_widget: HashMap<WidgetId, usize>,
    /// `lastLeftTBIndex`/`lastRightTBIndex`/`leftFloatsMark`/
    /// `rightFloatsMark` — champs amont déclarés, non lus
    /// (verbatim conservés pour parité d'état).
    #[allow(dead_code)]
    last_left_tb_index: i32,
    /// Voir [`Self::last_left_tb_index`].
    #[allow(dead_code)]
    last_right_tb_index: i32,
    /// Voir [`Self::last_left_tb_index`].
    #[allow(dead_code)]
    left_floats_mark: i32,
    /// Voir [`Self::last_left_tb_index`].
    #[allow(dead_code)]
    right_floats_mark: i32,
    /// `SizeChanged`.
    size_changed: bool,
}

/// `bsearch` amont (`lout/container.cc:227`) : retourne
/// l'index exact si trouvé, sinon la position d'insertion
/// (`start..=end`, `mustExist=false`).
fn bsearch<T>(list: &[T], start: i32, end: i32, mut compare: impl FnMut(&T) -> i32) -> i32 {
    if start > end {
        return start;
    }
    let mut low = start;
    let mut high = end;
    loop {
        let index = (low + high) / 2;
        let c = compare(&list[index as usize]);
        if c == 0 {
            return index;
        }
        if low >= high {
            return if c > 0 { index + 1 } else { index };
        }
        if c < 0 {
            high = index - 1;
        } else {
            low = index + 1;
        }
    }
}

impl OofFloatsMgr {
    /// `OOFFloatsMgr (container, oofmIndex)` — verbatim
    /// (ooffloatsmgr.cc:320) : `SizeChanged = true`, allocation
    /// du container capturée, `addWidgetInFlow (container)`.
    #[must_use]
    pub fn new(layout: &mut Layout, container: WidgetId, oofm_index: usize) -> Self {
        let mut mgr = OofFloatsMgr {
            container,
            oofm_index,
            container_allocation: layout.node(container, |n| n.allocation),
            floats: Vec::new(),
            left_floats: Vec::new(),
            right_floats: Vec::new(),
            floats_by_widget: HashMap::new(),
            tb_infos: Vec::new(),
            tb_infos_by_widget: HashMap::new(),
            last_left_tb_index: 0,
            last_right_tb_index: 0,
            left_floats_mark: 0,
            right_floats_mark: 0,
            size_changed: true,
        };
        mgr.add_widget_in_flow(layout, container, None, 0);
        mgr
    }

    /// `getOOFAwareWidgetWhenRegistered` → id `TbInfo`.
    fn tb_info_id(&self, widget: WidgetId) -> Option<usize> {
        self.tb_infos_by_widget.get(&widget).copied()
    }

    /// `getOOFAwareWidget` — assert amont → `Option` (les
    /// appelants `debug_assert` + fallback sain).
    fn tb_info(&self, widget: WidgetId) -> Option<&TbInfo> {
        self.tb_info_id(widget).map(|i| &self.tb_infos[i])
    }

    /// `findFloatByWidget` → id d'arène `floats`.
    fn find_float_by_widget(&self, widget: WidgetId) -> Option<usize> {
        self.floats_by_widget.get(&widget).copied()
    }

    /// Liste d'un côté (≙ `SortedFloatsVector*`).
    fn side_list(&self, side: Side) -> &Vec<usize> {
        match side {
            Side::Left => &self.left_floats,
            Side::Right => &self.right_floats,
        }
    }

    /// `getFloatsListsAndSide` — verbatim
    /// (ooffloatsmgr.cc:925).
    fn floats_side(&self, layout: &Layout, vloat: usize) -> Side {
        let widget = self.floats[vloat].widget;
        match widget.and_then(|w| layout.node(w, |n| n.style.as_ref().map(|s| s.attrs.vloat))) {
            Some(FloatType::Right) => Side::Right,
            _ => Side::Left,
        }
    }

    /// `Float::covers` — verbatim (ooffloatsmgr.cc:106).
    fn float_covers(&mut self, layout: &mut Layout, vloat: usize, y: i32, h: i32) -> bool {
        self.ensure_float_size(layout, vloat);
        let f = &self.floats[vloat];
        f.y_real + f.size.ascent + f.size.descent > y && f.y_real < y + h
    }

    /// `Float::CompareGBAndExtIndex::compare` — verbatim
    /// (ooffloatsmgr.cc:129) : `compare ((g1, e1), (g2, e2))` —
    /// même générateur → différence d'`externalIndex` ; sinon
    /// remontée de la chaîne `TBInfo` ; fallback `index`.
    fn compare_gb(&self, g1: Option<WidgetId>, e1: i32, g2: Option<WidgetId>, e2: i32) -> i32 {
        if g1 == g2 {
            e1 - e2
        } else {
            let t1 = g1.and_then(|g| self.tb_info(g));
            let t2 = g2.and_then(|g| self.tb_info(g));
            let (Some(t1), Some(t2)) = (t1, t2) else {
                debug_assert!(false, "CompareGBAndExtIndex: TBInfo manquant");
                return 0;
            };
            let t1_id = t1.index as usize;
            let t2_id = t2.index as usize;
            // (b) t2 ancêtre de t1.
            let mut cur = Some(t1_id);
            while let Some(t) = cur {
                if self.tb_infos[t].parent == Some(t2_id) {
                    return self.tb_infos[t].parent_ext_index - e2;
                }
                cur = self.tb_infos[t].parent;
            }
            // (c) t1 ancêtre de t2.
            let mut cur = Some(t2_id);
            while let Some(t) = cur {
                if self.tb_infos[t].parent == Some(t1_id) {
                    return e1 - self.tb_infos[t].parent_ext_index;
                }
                cur = self.tb_infos[t].parent;
            }
            // (d) fallback : index des TBInfo.
            t1.index - t2.index
        }
    }

    /// `SortedFloatsVector::findFloatIndex` — verbatim
    /// (ooffloatsmgr.cc:184) : clé `(lastGB, lastExtIndex)`.
    fn find_float_index(
        &self,
        list: &[usize],
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32 {
        let i = bsearch(list, 0, list.len() as i32 - 1, |&f| {
            let f = &self.floats[f];
            self.compare_gb(last_gb, last_ext_index, f.generator, f.external_index)
        });
        if i == list.len() as i32 {
            i - 1
        } else {
            // Amont : `comparator.compare (f, &key) == 0` → i.
            let f = &self.floats[list[i as usize]];
            if self.compare_gb(f.generator, f.external_index, last_gb, last_ext_index) == 0 {
                i
            } else {
                i - 1
            }
        }
    }

    /// `SortedFloatsVector::find` — verbatim
    /// (ooffloatsmgr.cc:229) : recherche par `yReal` dans
    /// `start..end` (indices de la liste).
    fn find_in_list(&self, list: &[usize], y: i32, start: i32, end: i32) -> i32 {
        bsearch(list, start, end, |&f| y - self.floats[f].y_real)
    }

    /// `SortedFloatsVector::findFirst` — verbatim
    /// (ooffloatsmgr.cc:242).
    fn find_first(
        &mut self,
        layout: &mut Layout,
        list: &[usize],
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> (i32, i32) {
        let last_index = self.find_float_index(list, last_gb, last_ext_index);
        debug_assert!(last_index < list.len() as i32);
        let i = self.find_in_list(list, y, 0, last_index);
        let first = if i > 0 && self.float_covers(layout, list[(i - 1) as usize], y, h) {
            i - 1
        } else if i <= last_index && self.float_covers(layout, list[i as usize], y, h) {
            i
        } else {
            -1
        };
        (first, last_index)
    }

    /// `SortedFloatsVector::findLastBeforeSideSpanningIndex` —
    /// verbatim (ooffloatsmgr.cc:278).
    fn find_last_before_side_spanning_index(&self, list: &[usize], ssi: i32) -> i32 {
        bsearch(list, 0, list.len() as i32 - 1, |&f| {
            ssi - self.floats[f].side_spanning_index
        }) - 1
    }

    /// `SortedFloatsVector::put` — verbatim
    /// (ooffloatsmgr.cc:287) : `index = size() - 1`.
    fn put_float(&mut self, side: Side, vloat: usize) {
        let list = match side {
            Side::Left => &mut self.left_floats,
            Side::Right => &mut self.right_floats,
        };
        list.push(vloat);
        self.floats[vloat].index = list.len() as i32 - 1;
    }

    /// `Float::createSubRefLeftFloat`/`RightFloat` —
    /// ooffloatsmgr.hh:213.
    #[must_use]
    pub fn create_sub_ref_left_float(index: i32) -> i32 {
        index << 1
    }
    /// `createSubRefRightFloat`.
    #[must_use]
    pub fn create_sub_ref_right_float(index: i32) -> i32 {
        (index << 1) | 1
    }
    /// `isSubRefLeftFloat`.
    #[must_use]
    pub fn is_sub_ref_left_float(ref_: i32) -> bool {
        ref_ != -1 && (ref_ & 1) == 0
    }
    /// `isSubRefRightFloat`.
    #[must_use]
    pub fn is_sub_ref_right_float(ref_: i32) -> bool {
        ref_ != -1 && (ref_ & 1) == 1
    }
    /// `getFloatIndexFromSubRef`.
    #[must_use]
    pub fn get_float_index_from_sub_ref(ref_: i32) -> i32 {
        if ref_ == -1 {
            ref_
        } else {
            ref_ >> 1
        }
    }

    /// `ensureFloatSize` — verbatim (ooffloatsmgr.cc:1321).
    fn ensure_float_size(&mut self, layout: &mut Layout, vloat: usize) {
        if self.floats[vloat].dirty {
            if let Some(w) = self.floats[vloat].widget {
                let mut size = Requisition::default();
                layout.widget_size_request(w, &mut size, 0, &[], &[], &[]);
                self.floats[vloat].size = size;
            }
            self.floats[vloat].dirty = false;
        }
    }

    /// `sizeAllocateFloats` — verbatim (ooffloatsmgr.cc:404).
    fn size_allocate_floats(&mut self, layout: &mut Layout, side: Side) {
        let ids = self.side_list(side).clone();
        for vloat in ids {
            self.ensure_float_size(layout, vloat);
            let x = self.calc_float_x(layout, vloat);
            let f = &self.floats[vloat];
            let child_allocation = Allocation {
                x: self.container_allocation.x + x,
                y: self.container_allocation.y + f.y_real,
                width: f.size.width,
                ascent: f.size.ascent,
                descent: f.size.descent,
            };
            if let Some(w) = f.widget {
                layout.widget_size_allocate(w, &child_allocation);
            }
        }
    }

    /// `calcFloatX` — verbatim (ooffloatsmgr.cc:431) : position
    /// relative au container.
    fn calc_float_x(&mut self, layout: &mut Layout, vloat: usize) -> i32 {
        self.ensure_float_size(layout, vloat);
        let (widget, generator) = {
            let f = &self.floats[vloat];
            (f.widget, f.generator)
        };
        let (Some(widget), Some(generator)) = (widget, generator) else {
            return 0;
        };
        let vloat_side = layout
            .node(widget, |n| n.style.as_ref().map(|s| s.attrs.vloat))
            .unwrap_or(FloatType::Left);
        match vloat_side {
            FloatType::Left => {
                let mut x = layout.w_get_generator_x(generator, self.oofm_index)
                    + layout.node(generator, |n| {
                        n.style.as_ref().map_or(0, |s| s.attrs.box_offset_x())
                    });
                // Correction « dépasse la largeur de ligne du
                // container » — verbatim (pas à gauche du container).
                let f_width = self.floats[vloat].size.width;
                let max_gen_w = layout.w_get_max_generator_width(self.container);
                if x + f_width > max_gen_w {
                    x = (max_gen_w - f_width).max(0);
                }
                x
            }
            FloatType::Right => {
                let eff_generator_width = if layout.w_uses_max_generator_width(generator) {
                    layout.w_get_max_generator_width(generator)
                } else {
                    (layout.w_get_generator_width(generator) + self.floats[vloat].size.width)
                        .min(layout.w_get_max_generator_width(generator))
                };
                let gen_x = layout.w_get_generator_x(generator, self.oofm_index);
                let box_rest = layout.node(generator, |n| {
                    n.style.as_ref().map_or(0, |s| s.attrs.box_rest_width())
                });
                (gen_x + eff_generator_width - self.floats[vloat].size.width - box_rest).max(0)
            }
            FloatType::None => {
                debug_assert!(false, "calcFloatX: float sans vloat");
                0
            }
        }
    }

    /// `drawFloats` — verbatim (ooffloatsmgr.cc:508).
    fn draw_floats(
        &mut self,
        layout: &mut Layout,
        list: &[usize],
        view: &mut dyn View,
        area: &crate::types::Rectangle,
        context: &mut crate::types::DrawingContext,
    ) {
        for &vloat in list {
            let Some(child) = self.floats[vloat].widget else {
                continue;
            };
            if context
                .stacking
                .has_widget_been_processed_as_interruption(child)
            {
                continue;
            }
            if handled_by_stacking_context_mgr(layout, child) {
                continue;
            }
            if let Some(child_area) = layout.widget_intersects(child, self.container, area) {
                layout.w_draw(child, view, &child_area, context);
            }
        }
    }

    /// `getFloatWidgetAtPoint` — verbatim (ooffloatsmgr.cc:733)
    /// : itération en ordre inverse.
    fn get_float_widget_at_point(
        &mut self,
        layout: &mut Layout,
        list: &[usize],
        x: i32,
        y: i32,
        context: &mut GettingWidgetAtPointContext,
    ) -> Option<WidgetId> {
        for &vloat in list.iter().rev() {
            let Some(child) = self.floats[vloat].widget else {
                continue;
            };
            if context
                .stacking
                .has_widget_been_processed_as_interruption(child)
                || handled_by_stacking_context_mgr(layout, child)
            {
                continue;
            }
            if let Some(found) = layout.w_get_widget_at_point(child, x, y, context) {
                return Some(found);
            }
        }
        None
    }

    /// `updateGenerators` — verbatim (ooffloatsmgr.cc:666) :
    /// le premier générateur via `parentRef` de la référence
    /// widget, les suivants via `updateReference (0)`.
    fn update_generators(&mut self, layout: &mut Layout, vloat: usize) {
        let widget = self.floats[vloat].widget;
        debug_assert!(widget.and_then(|w| layout.widget_reference(w)).is_some());
        let first = self.floats[vloat]
            .generator
            .and_then(|g| self.tb_info(g))
            .map_or(0, |t| t.index as usize);
        if self.size_changed {
            if let Some(first_widget) = self.tb_infos.get(first).and_then(|t| t.widget) {
                let parent_ref = widget
                    .and_then(|w| layout.widget_reference(w))
                    .map_or(-1, |r| r.parent_ref);
                layout.w_update_reference(first_widget, parent_ref);
            }
        }
        for i in (first + 1)..self.tb_infos.len() {
            if let Some(w) = self.tb_infos[i].widget {
                layout.w_update_reference(w, 0);
            }
        }
        self.size_changed = false;
    }

    /// `findTBInfo` — verbatim (ooffloatsmgr.cc:695) :
    /// `bsearch` → dernier `<= y`. Mort amont aussi (défini,
    /// jamais appelé) — conservé pour parité.
    #[allow(dead_code)]
    fn find_tb_info(&mut self, layout: &mut Layout, y: i32) -> i32 {
        let oofm_index = self.oofm_index;
        let index = bsearch(&self.tb_infos, 0, self.tb_infos.len() as i32 - 1, |t| {
            // compare (key.y, t) : y - t.getGeneratorY().
            match t.widget {
                Some(w) => y - layout.w_get_generator_y(w, oofm_index),
                None => y - t.y,
            }
        });
        if index > 0 {
            index - 1
        } else {
            index
        }
    }

    /// `collidesV` — verbatim (ooffloatsmgr.cc:870).
    fn collides_v(&mut self, layout: &mut Layout, vloat: usize, other: usize) -> (bool, i32) {
        self.ensure_float_size(layout, other);
        let other_bottom_gb = {
            let o = &self.floats[other];
            o.y_real + o.size.ascent + o.size.descent
        };
        if self.floats[vloat].y_real < other_bottom_gb {
            (true, other_bottom_gb)
        } else {
            (false, self.floats[vloat].y_real)
        }
    }

    /// `collidesH` — verbatim (ooffloatsmgr.cc:902).
    fn collides_h(&mut self, layout: &mut Layout, vloat: usize, other: usize) -> bool {
        let vloat_x = self.calc_float_x(layout, vloat);
        let other_x = self.calc_float_x(layout, other);
        let vloat_widget = self.floats[vloat].widget;
        let is_left = vloat_widget
            .and_then(|w| layout.node(w, |n| n.style.as_ref().map(|s| s.attrs.vloat)))
            == Some(FloatType::Left);
        if is_left {
            self.ensure_float_size(layout, vloat);
            vloat_x + self.floats[vloat].size.width > other_x
        } else {
            self.ensure_float_size(layout, other);
            other_x + self.floats[other].size.width > vloat_x
        }
    }

    /// `getFloatsSize` — verbatim (ooffloatsmgr.cc:975).
    fn get_floats_size(
        &mut self,
        layout: &mut Layout,
        side: Side,
        width: &mut i32,
        height: &mut i32,
    ) {
        *width = 0;
        *height = 0;
        let ids = self.side_list(side).clone();
        for vloat in ids {
            self.ensure_float_size(layout, vloat);
            let x = self.calc_float_x(layout, vloat);
            let f = &self.floats[vloat];
            *width = (*width).max(x + f.size.width);
            *height = (*height).max(f.y_real + f.size.ascent + f.size.descent);
        }
    }

    /// `getFloatsExtremes` — verbatim (ooffloatsmgr.cc:1030).
    fn get_floats_extremes(
        &mut self,
        layout: &mut Layout,
        side: Side,
        min_width: &mut i32,
        max_width: &mut i32,
    ) {
        *min_width = 0;
        *max_width = 0;
        let ids = self.side_list(side).clone();
        for vloat in ids {
            let Some(w) = self.floats[vloat].widget else {
                continue;
            };
            let mut extr = Extremes::default();
            layout.widget_get_extremes(w, &mut extr, 0, &[], &[], &[]);
            *min_width = (*min_width).max(extr.min_width);
            // Bordures du côté opposé : verbatim — la différence
            // container/générateur entre dans `maxWidth`.
            let generator = self.floats[vloat].generator;
            let box_diff = generator.map_or(0, |g| {
                layout.node(g, |n| {
                    n.style.as_ref().map_or(0, |s| s.attrs.box_diff_width())
                })
            });
            let gen_delta = generator.map_or(0, |g| {
                (layout.w_get_generator_width(self.container) - layout.w_get_generator_width(g))
                    .max(0)
            });
            *max_width = (*max_width).max(extr.max_width + box_diff + gen_delta);
        }
    }

    /// `getBorder` — verbatim (ooffloatsmgr.cc:1118).
    fn get_border(
        &mut self,
        layout: &mut Layout,
        side: Side,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32 {
        let ids = self.side_list(side).clone();
        let (first, last) = self.find_first(layout, &ids, y, h, last_gb, last_ext_index);
        let mut border = 0;
        if first != -1 {
            let mut covers = true;
            let mut i = first;
            while covers && i <= last {
                let vloat = ids[i as usize];
                covers = self.float_covers(layout, vloat, y, h);
                if covers {
                    let d = match side {
                        Side::Left => {
                            let g = self.floats[vloat].generator;
                            g.map_or(0, |g| {
                                layout.w_get_generator_x(g, self.oofm_index)
                                    + layout.node(g, |n| {
                                        n.style.as_ref().map_or(0, |s| s.attrs.box_offset_x())
                                    })
                            })
                        }
                        Side::Right => {
                            let g = self.floats[vloat].generator;
                            g.map_or(0, |g| {
                                layout.w_get_max_generator_width(self.container)
                                    - (layout.w_get_generator_x(g, self.oofm_index)
                                        + layout.w_get_max_generator_width(g))
                                    + layout.node(g, |n| {
                                        n.style.as_ref().map_or(0, |s| s.attrs.box_rest_width())
                                    })
                            })
                        }
                    };
                    let this_border = self.floats[vloat].size.width + d;
                    border = border.max(this_border);
                }
                i += 1;
            }
        }
        border
    }

    /// `hasFloat` — verbatim (ooffloatsmgr.cc:1201).
    fn has_float(
        &mut self,
        layout: &mut Layout,
        side: Side,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> bool {
        let ids = self.side_list(side).clone();
        let (first, _) = self.find_first(layout, &ids, y, h, last_gb, last_ext_index);
        first != -1
    }

    /// `getFloatHeight` — verbatim (ooffloatsmgr.cc:1227).
    fn get_float_height(
        &mut self,
        layout: &mut Layout,
        side: Side,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32 {
        let ids = self.side_list(side).clone();
        let (first, _) = self.find_first(layout, &ids, y, h, last_gb, last_ext_index);
        debug_assert!(first != -1);
        if first == -1 {
            return 0;
        }
        let vloat = ids[first as usize];
        let y_rel_to_float = y - self.floats[vloat].y_real;
        self.ensure_float_size(layout, vloat);
        let f = &self.floats[vloat];
        f.size.ascent + f.size.descent - y_rel_to_float
    }

    /// `getClearPosition (widget, side)` — verbatim
    /// (ooffloatsmgr.cc:1292) : dernier float avant `widget`.
    fn get_clear_position_side(
        &mut self,
        layout: &mut Layout,
        widget: WidgetId,
        side: Side,
    ) -> i32 {
        let ids = self.side_list(side).clone();
        let i = self.find_float_index(&ids, Some(widget), -1);
        if i < 0 {
            return 0;
        }
        let vloat = ids[i as usize];
        debug_assert_ne!(self.floats[vloat].generator, Some(widget));
        self.ensure_float_size(layout, vloat);
        let y_rel = layout.w_get_generator_y(widget, self.oofm_index);
        let f = &self.floats[vloat];
        (f.y_real + f.size.ascent + f.size.descent - y_rel).max(0)
    }
}

/// `StackingContextMgr::handledByStackingContextMgr` — verbatim
/// (stackingcontextmgr.hh:42) : parent non nul + contexte
/// établi (`IMPL_POS` → `false`).
pub(crate) fn handled_by_stacking_context_mgr(layout: &Layout, w: WidgetId) -> bool {
    layout.widget_parent(w).is_some()
        && layout
            .node(w, |n| n.style.as_ref().map(|s| s.attrs.clone()))
            .is_some_and(|a| is_establishing_stacking_context(&a))
}

impl OofMgr for OofFloatsMgr {
    /// `sizeAllocateStart` — verbatim (ooffloatsmgr.cc:366).
    fn size_allocate_start(
        &mut self,
        _layout: &mut Layout,
        caller: WidgetId,
        allocation: &Allocation,
    ) {
        if caller == self.container {
            self.container_allocation = *allocation;
        }
    }

    /// `sizeAllocateEnd` — verbatim (ooffloatsmgr.cc:380).
    fn size_allocate_end(&mut self, layout: &mut Layout, caller: WidgetId) {
        if caller == self.container {
            self.size_allocate_floats(layout, Side::Left);
            self.size_allocate_floats(layout, Side::Right);
        }
    }

    /// `containerSizeChangedForChildren` — verbatim
    /// (ooffloatsmgr.cc:392).
    fn container_size_changed_for_children(&mut self, layout: &mut Layout) {
        let mut ids = self.left_floats.clone();
        ids.extend_from_slice(&self.right_floats);
        for vloat in ids {
            if let Some(w) = self.floats[vloat].widget {
                layout.widget_container_size_changed(w);
            }
        }
    }

    /// `draw` — verbatim (ooffloatsmgr.cc:497).
    fn draw(
        &mut self,
        layout: &mut Layout,
        view: &mut dyn View,
        area: &crate::types::Rectangle,
        context: &mut crate::types::DrawingContext,
    ) {
        let left = self.left_floats.clone();
        let right = self.right_floats.clone();
        self.draw_floats(layout, &left, view, area, context);
        self.draw_floats(layout, &right, view, area, context);
    }

    /// `markSizeChange` — verbatim (ooffloatsmgr.cc:646).
    fn mark_size_change(&mut self, layout: &mut Layout, ref_: i32) {
        let index = Self::get_float_index_from_sub_ref(ref_);
        let vloat = if Self::is_sub_ref_left_float(ref_) {
            self.left_floats.get(index as usize).copied()
        } else {
            self.right_floats.get(index as usize).copied()
        };
        let Some(vloat) = vloat else {
            debug_assert!(false, "markSizeChange: subRef {ref_} inconnu");
            return;
        };
        self.floats[vloat].dirty = true;
        self.update_generators(layout, vloat);
    }

    /// `markExtremesChange` — vide verbatim
    /// (ooffloatsmgr.cc:716).
    fn mark_extremes_change(&mut self, _layout: &mut Layout, _ref_: i32) {}

    /// `getWidgetAtPoint` — verbatim (ooffloatsmgr.cc:721) :
    /// droite d'abord, puis gauche.
    fn get_widget_at_point(
        &mut self,
        layout: &mut Layout,
        x: i32,
        y: i32,
        context: &mut GettingWidgetAtPointContext,
    ) -> Option<WidgetId> {
        let right = self.right_floats.clone();
        if let Some(found) = self.get_float_widget_at_point(layout, &right, x, y, context) {
            return Some(found);
        }
        let left = self.left_floats.clone();
        self.get_float_widget_at_point(layout, &left, x, y, context)
    }

    /// `addWidgetInFlow` — verbatim (ooffloatsmgr.cc:527).
    fn add_widget_in_flow(
        &mut self,
        _layout: &mut Layout,
        widget: WidgetId,
        parent: Option<WidgetId>,
        external_index: i32,
    ) {
        let parent_id = parent.and_then(|p| self.tb_info_id(p));
        let tb_info = TbInfo {
            widget: Some(widget),
            index: self.tb_infos.len() as i32,
            y: 0,
            parent: parent_id,
            parent_ext_index: external_index,
            left_floats: Vec::new(),
            right_floats: Vec::new(),
        };
        self.tb_infos.push(tb_info);
        self.tb_infos_by_widget
            .insert(widget, self.tb_infos.len() - 1);
    }

    /// `addWidgetOOF` — verbatim (ooffloatsmgr.cc:545) →
    /// sub-ref (`createSubRef*`).
    fn add_widget_oof(
        &mut self,
        layout: &mut Layout,
        widget: WidgetId,
        generator: WidgetId,
        external_index: i32,
    ) -> i32 {
        let tb_id = self.tb_info_id(generator);
        debug_assert!(tb_id.is_some());
        let vloat_id = self.floats.len();
        self.floats.push(Float {
            widget: Some(widget),
            generator: Some(generator),
            external_index,
            index: -1,
            y_req: 0,
            y_real: 0,
            side_spanning_index: 0,
            size: Requisition::default(),
            dirty: true,
        });
        let vloat_side = layout
            .node(widget, |n| n.style.as_ref().map(|s| s.attrs.vloat))
            .unwrap_or(FloatType::None);
        let sub_ref = match vloat_side {
            FloatType::Left => {
                self.put_float(Side::Left, vloat_id);
                if let Some(t) = tb_id {
                    self.tb_infos[t].left_floats.push(vloat_id);
                }
                Self::create_sub_ref_left_float(self.left_floats.len() as i32 - 1)
            }
            FloatType::Right => {
                self.put_float(Side::Right, vloat_id);
                if let Some(t) = tb_id {
                    self.tb_infos[t].right_floats.push(vloat_id);
                }
                Self::create_sub_ref_right_float(self.right_floats.len() as i32 - 1)
            }
            FloatType::None => {
                debug_assert!(false, "addWidgetOOF: widget non-float");
                // Amont : assertNotReached — le float est tout de
                // même enregistré côté gauche (état sain).
                self.put_float(Side::Left, vloat_id);
                if let Some(t) = tb_id {
                    self.tb_infos[t].left_floats.push(vloat_id);
                }
                Self::create_sub_ref_left_float(self.left_floats.len() as i32 - 1)
            }
        };
        self.floats[vloat_id].side_spanning_index =
            (self.left_floats.len() + self.right_floats.len()) as i32 - 1;
        self.floats_by_widget.insert(widget, vloat_id);
        sub_ref
    }

    /// `calcWidgetRefSize` — tailles nulles verbatim
    /// (ooffloatsmgr.cc:598).
    fn calc_widget_ref_size(
        &mut self,
        _layout: &mut Layout,
        _widget: WidgetId,
        size: &mut Requisition,
    ) {
        *size = Requisition::default();
    }

    /// `moveExternalIndices` — verbatim (ooffloatsmgr.cc:603).
    fn move_external_indices(
        &mut self,
        _layout: &mut Layout,
        generator: WidgetId,
        old_start_index: i32,
        diff: i32,
    ) {
        if let Some(t) = self.tb_info_id(generator) {
            let mut ids = self.tb_infos[t].left_floats.clone();
            ids.extend_from_slice(&self.tb_infos[t].right_floats);
            for vloat in ids {
                if self.floats[vloat].external_index >= old_start_index {
                    self.floats[vloat].external_index += diff;
                }
            }
        }
    }

    /// `tellPosition1` — verbatim (ooffloatsmgr.cc:751) :
    /// position demandée puis collisions même côté (float
    /// précédent seul) puis côté opposé (boucle).
    fn tell_position1(&mut self, layout: &mut Layout, widget: WidgetId, _x: i32, y: i32) {
        debug_assert!(y >= 0);
        let Some(vloat) = self.find_float_by_widget(widget) else {
            debug_assert!(false, "tellPosition1: float inconnu");
            return;
        };
        let side = self.floats_side(layout, vloat);
        let list_same = self.side_list(side).clone();
        let list_opp = self
            .side_list(match side {
                Side::Left => Side::Right,
                Side::Right => Side::Left,
            })
            .clone();
        self.ensure_float_size(layout, vloat);
        let old_y_real = self.floats[vloat].y_real;
        self.floats[vloat].y_req = y;
        self.floats[vloat].y_real = y;

        // Collisions même côté — float précédent seul.
        let idx = self.floats[vloat].index;
        if idx >= 1 {
            let other = list_same[(idx - 1) as usize];
            let (collides, y_real_new) = self.collides_v(layout, vloat, other);
            if collides {
                self.floats[vloat].y_real = y_real_new;
            }
        }

        // Collisions côté opposé — boucle tant que collision
        // verticale, arrêt à la première collision horizontale.
        let mut opp_index = self.find_last_before_side_spanning_index(
            &list_opp,
            self.floats[vloat].side_spanning_index,
        );
        let mut found_coll = false;
        while !found_coll && opp_index >= 0 {
            let other = list_opp[opp_index as usize];
            let (collides, y_real_new) = self.collides_v(layout, vloat, other);
            if !collides {
                break;
            }
            if self.collides_h(layout, vloat, other) {
                self.floats[vloat].y_real = y_real_new;
                found_coll = true;
            }
            opp_index -= 1;
        }

        if self.floats[vloat].y_real != old_y_real {
            self.update_generators(layout, vloat);
        }
    }

    /// `tellPosition2` — vide verbatim (ooffloatsmgr.cc:853).
    fn tell_position2(&mut self, _layout: &mut Layout, _w: WidgetId, _x: i32, _y: i32) {}

    /// `tellIncompletePosition1/2` — `notImplemented` amont →
    /// no-op documenté.
    fn tell_incomplete_position1(
        &mut self,
        _l: &mut Layout,
        _g: WidgetId,
        _w: WidgetId,
        _x: i32,
        _y: i32,
    ) {
    }
    /// `tellIncompletePosition2`.
    fn tell_incomplete_position2(
        &mut self,
        _l: &mut Layout,
        _g: WidgetId,
        _w: WidgetId,
        _x: i32,
        _y: i32,
    ) {
    }

    /// `getSize` — verbatim (ooffloatsmgr.cc:948) : + restes de
    /// boîte du container ; `SizeChanged = true`.
    fn get_size(
        &mut self,
        layout: &mut Layout,
        _cb_req: &Requisition,
        oof_width: &mut i32,
        oof_height: &mut i32,
    ) {
        let mut wl = 0;
        let mut hl = 0;
        let mut wr = 0;
        let mut hr = 0;
        self.get_floats_size(layout, Side::Left, &mut wl, &mut hl);
        self.get_floats_size(layout, Side::Right, &mut wr, &mut hr);
        let (rest_w, rest_h) = layout.node(self.container, |n| {
            n.style.as_ref().map_or((0, 0), |s| {
                (s.attrs.box_rest_width(), s.attrs.box_rest_height())
            })
        });
        *oof_width = wl.max(wr) + rest_w;
        *oof_height = hl.max(hr) + rest_h;
        self.size_changed = true;
    }

    /// `containerMustAdjustExtraSpace` — `false` verbatim
    /// (ooffloatsmgr.cc:1005).
    fn container_must_adjust_extra_space(&self) -> bool {
        false
    }

    /// `getExtremes` — verbatim (ooffloatsmgr.cc:1010).
    fn get_extremes(
        &mut self,
        layout: &mut Layout,
        _cb_extr: &Extremes,
        oof_min_width: &mut i32,
        oof_max_width: &mut i32,
    ) {
        let mut min_l = 0;
        let mut max_l = 0;
        let mut min_r = 0;
        let mut max_r = 0;
        self.get_floats_extremes(layout, Side::Left, &mut min_l, &mut max_l);
        self.get_floats_extremes(layout, Side::Right, &mut min_r, &mut max_r);
        *oof_min_width = min_l.max(min_r);
        *oof_max_width = max_l.max(max_r);
    }

    /// `getLeftBorder`/`getRightBorder` — verbatim
    /// (ooffloatsmgr.cc:1100).
    fn get_left_border(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32 {
        self.get_border(layout, Side::Left, y, h, last_gb, last_ext_index)
    }
    /// `getRightBorder`.
    fn get_right_border(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32 {
        self.get_border(layout, Side::Right, y, h, last_gb, last_ext_index)
    }

    /// `hasFloatLeft`/`hasFloatRight` — verbatim
    /// (ooffloatsmgr.cc:1183).
    fn has_float_left(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> bool {
        self.has_float(layout, Side::Left, y, h, last_gb, last_ext_index)
    }
    /// `hasFloatRight`.
    fn has_float_right(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> bool {
        self.has_float(layout, Side::Right, y, h, last_gb, last_ext_index)
    }

    /// `getLeftFloatHeight`/`getRightFloatHeight` — verbatim
    /// (ooffloatsmgr.cc:1215).
    fn get_left_float_height(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32 {
        self.get_float_height(layout, Side::Left, y, h, last_gb, last_ext_index)
    }
    /// `getRightFloatHeight`.
    fn get_right_float_height(
        &mut self,
        layout: &mut Layout,
        y: i32,
        h: i32,
        last_gb: Option<WidgetId>,
        last_ext_index: i32,
    ) -> i32 {
        self.get_float_height(layout, Side::Right, y, h, last_gb, last_ext_index)
    }

    /// `affectsLeftBorder`/`affectsRightBorder` — verbatim
    /// (ooffloatsmgr.cc:1277).
    fn affects_left_border(&self, layout: &Layout, widget: WidgetId) -> bool {
        layout.node(widget, |n| n.style.as_ref().map(|s| s.attrs.vloat)) == Some(FloatType::Left)
    }
    /// `affectsRightBorder`.
    fn affects_right_border(&self, layout: &Layout, widget: WidgetId) -> bool {
        layout.node(widget, |n| n.style.as_ref().map(|s| s.attrs.vloat)) == Some(FloatType::Right)
    }

    /// `mayAffectBordersAtAll` — `true` verbatim
    /// (ooffloatsmgr.cc:1287).
    fn may_affect_borders_at_all(&self) -> bool {
        true
    }

    /// `getClearPosition` — verbatim (ooffloatsmgr.cc:1252) :
    /// `clear` du style → max des deux côtés.
    fn get_clear_position(&mut self, layout: &mut Layout, widget: WidgetId) -> i32 {
        let clear = layout.node(widget, |n| n.style.as_ref().map(|s| s.attrs.clear));
        match clear {
            Some(ClearType::Left) => self.get_clear_position_side(layout, widget, Side::Left),
            Some(ClearType::Right) => self.get_clear_position_side(layout, widget, Side::Right),
            Some(ClearType::Both) => self
                .get_clear_position_side(layout, widget, Side::Left)
                .max(self.get_clear_position_side(layout, widget, Side::Right)),
            Some(ClearType::None) | None => 0,
        }
    }

    /// `dealingWithSizeOfChild` — `false` verbatim
    /// (ooffloatsmgr.cc:1353).
    fn dealing_with_size_of_child(&self, _child: WidgetId) -> bool {
        false
    }

    /// `getAvailWidthOfChild`/`getAvailHeightOfChild` —
    /// `notImplemented` amont → 0.
    fn get_avail_width_of_child(&mut self, _l: &mut Layout, _c: WidgetId, _force: bool) -> i32 {
        0
    }
    /// `getAvailHeightOfChild`.
    fn get_avail_height_of_child(&mut self, _l: &mut Layout, _c: WidgetId, _force: bool) -> i32 {
        0
    }

    /// `getNumWidgets`/`getWidget` — verbatim
    /// (ooffloatsmgr.cc:1370) : gauche puis droite.
    fn get_num_widgets(&self) -> usize {
        self.left_floats.len() + self.right_floats.len()
    }
    /// `getWidget`.
    fn get_widget(&self, i: usize) -> Option<WidgetId> {
        if i < self.left_floats.len() {
            self.floats[self.left_floats[i]].widget
        } else {
            self.floats
                .get(*self.right_floats.get(i - self.left_floats.len())?)
                .and_then(|f| f.widget)
        }
    }
}

// ------------------------------------------------------------------
// OOFAwareWidgetIterator — verbatim oofawarewidget_iterator.cc.
// `numContentsInFlow`/`getContentInFlow` (+ highlight/
// unhighlight/getAllocation in-flow) sont les virtuels de
// sous-classe amont → trait [`OofInFlow`].
// ------------------------------------------------------------------

/// `NUM_SECTIONS = NUM_OOFM + 1` — section 0 = en flux,
/// `section > 0` = `OOFM` index + 1.
const NUM_SECTIONS: i32 = NUM_OOFM as i32 + 1;

/// Virtuels « in flow » de `OOFAwareWidgetIterator` +
/// `highlight`/`unhighlight`/`getAllocation` appelés quand
/// `inFlow()` (`TextblockIterator` amont).
pub trait OofInFlow {
    /// `numContentsInFlow`.
    fn num_contents_in_flow(&self, layout: &Layout) -> i32;
    /// `getContentInFlow`.
    fn get_content_in_flow(&self, layout: &Layout, index: i32, content: &mut Content);
    /// Clone polymorphe (≙ `cloneIterator` sur la sous-classe).
    fn clone_box(&self) -> Box<dyn OofInFlow>;
    /// `highlight` in-flow (sous-classe amont).
    fn highlight_in_flow(
        &self,
        layout: &mut Layout,
        index: i32,
        start: i32,
        end: i32,
        layer: crate::types::HighlightLayer,
    );
    /// `unhighlight` in-flow.
    fn unhighlight_in_flow(
        &self,
        layout: &mut Layout,
        index: i32,
        direction: i32,
        layer: crate::types::HighlightLayer,
    );
    /// `getAllocation` in-flow.
    fn get_allocation_in_flow(
        &self,
        layout: &Layout,
        index: i32,
        start: i32,
        end: i32,
        allocation: &mut Allocation,
    );
}

/// `OofInFlow` vide — kind `OofAware` nu (≙ sous-classe amont
/// sans contenu : `numContentsInFlow` = 0).
pub struct EmptyInFlow;

impl OofInFlow for EmptyInFlow {
    fn num_contents_in_flow(&self, _layout: &Layout) -> i32 {
        0
    }
    fn get_content_in_flow(&self, _layout: &Layout, _index: i32, content: &mut Content) {
        content.r#type = content_type::INVALID;
        content.data = ContentData::None;
    }
    fn clone_box(&self) -> Box<dyn OofInFlow> {
        Box::new(EmptyInFlow)
    }
    fn highlight_in_flow(
        &self,
        _l: &mut Layout,
        _i: i32,
        _s: i32,
        _e: i32,
        _layer: crate::types::HighlightLayer,
    ) {
    }
    fn unhighlight_in_flow(
        &self,
        _l: &mut Layout,
        _i: i32,
        _d: i32,
        _layer: crate::types::HighlightLayer,
    ) {
    }
    fn get_allocation_in_flow(&self, _l: &Layout, _i: i32, _s: i32, _e: i32, _a: &mut Allocation) {}
}

/// `dw::oof::OOFAwareWidget::OOFAwareWidgetIterator` — sections
/// `0` (en flux) puis `1..=NUM_OOFM` (widgets OOF).
pub struct OofAwareIterator {
    /// `getWidget`.
    widget: WidgetId,
    /// `getMask`.
    mask: i16,
    /// `content`.
    content: Content,
    /// `sectionIndex` — 0 = en flux, sinon `OOFM` index + 1.
    section_index: i32,
    /// `index`.
    index: i32,
    /// Virtuels in-flow (≙ sous-classe amont).
    in_flow: Box<dyn OofInFlow>,
}

impl OofAwareIterator {
    /// `OOFAwareWidgetIterator (widget, mask, atEnd,
    /// numContentsInFlow)` — verbatim
    /// (`oofawarewidget_iterator.cc:36`) : `numContentsInFlow`
    /// passé pour éviter l'appel virtuel au ctor ; `layout`
    /// donne accès aux mgrs (≙ lecture directe du widget
    /// amont).
    #[must_use]
    pub fn new(
        widget: WidgetId,
        mask: i16,
        at_end: bool,
        in_flow: Box<dyn OofInFlow>,
        layout: &Layout,
        num_contents_in_flow: i32,
    ) -> Self {
        let mut it = OofAwareIterator {
            widget,
            mask,
            content: Content {
                r#type: if at_end {
                    content_type::END
                } else {
                    content_type::START
                },
                space: false,
                data: ContentData::None,
            },
            section_index: 0,
            index: -1,
            in_flow,
        };
        if at_end {
            it.section_index = NUM_SECTIONS - 1;
            while it.section_index >= 0
                && it.num_parts_opt(layout, it.section_index, num_contents_in_flow) == 0
            {
                it.section_index -= 1;
            }
            it.index = it.num_parts_opt(layout, it.section_index, num_contents_in_flow);
        } else {
            it.section_index = 0;
            it.index = -1;
        }
        it
    }

    /// `setValues` — verbatim (`oofawarewidget_iterator.cc:55`).
    /// Appelé par `cloneValues` amont — seam pour
    /// `TextblockIterator` (phase suivante).
    pub(crate) fn set_values(&mut self, layout: &Layout, section_index: i32, index: i32) {
        self.section_index = section_index;
        self.index = index;
        if section_index < 0 || index < 0 {
            self.content.r#type = content_type::START;
            self.content.data = ContentData::None;
        } else if section_index >= NUM_SECTIONS
            || index >= self.num_parts_opt(layout, section_index, -1)
        {
            self.content.r#type = content_type::END;
            self.content.data = ContentData::None;
        } else {
            self.get_part(layout, section_index, index);
        }
    }

    /// `numParts` — verbatim (`oofawarewidget_iterator.cc:69`) :
    /// `numContentsInFlow == -1` → virtuel in-flow ; OOF →
    /// `getNumWidgets` du mgr `section - 1`. (Verbatim y compris
    /// la borne `> NUM_SECTIONS` — `>=` serait strictement
    /// correct.)
    fn num_parts_opt(&self, layout: &Layout, section_index: i32, n_in_flow: i32) -> i32 {
        if !(0..=NUM_SECTIONS).contains(&section_index) {
            return 0;
        }
        if section_index == 0 {
            if n_in_flow == -1 {
                self.in_flow.num_contents_in_flow(layout)
            } else {
                n_in_flow
            }
        } else {
            layout
                .node_oof(self.widget, |o| {
                    o.out_of_flow_mgr[(section_index - 1) as usize].clone()
                })
                .flatten()
                .map_or(0, |m| m.borrow().get_num_widgets() as i32)
        }
    }

    /// `numParts (sectionIndex)` — raccourci `-1`.
    fn num_parts(&self, layout: &Layout, section_index: i32) -> i32 {
        self.num_parts_opt(layout, section_index, -1)
    }

    /// `getPart` — verbatim (`oofawarewidget_iterator.cc:94`).
    fn get_part(&mut self, layout: &Layout, section_index: i32, index: i32) {
        if section_index == 0 {
            self.in_flow
                .get_content_in_flow(layout, index, &mut self.content);
        } else {
            self.content.r#type = content_type::WIDGET_OOF_CONT;
            let widget = layout
                .node_oof(self.widget, |o| {
                    o.out_of_flow_mgr[(section_index - 1) as usize].clone()
                })
                .flatten()
                .and_then(|m| m.borrow().get_widget(index as usize));
            self.content.data = if let Some(w) = widget {
                ContentData::Widget(w)
            } else {
                self.content.r#type = content_type::INVALID;
                ContentData::None
            };
        }
    }

    /// `inFlow`/`getInFlowIndex`.
    #[must_use]
    pub fn in_flow(&self) -> bool {
        self.section_index == 0
    }
    /// `getInFlowIndex`.
    #[must_use]
    pub fn in_flow_index(&self) -> i32 {
        self.index
    }
    /// `sectionIndex`/`index` — accès pour `compare` et
    /// sous-classes.
    #[must_use]
    pub fn values(&self) -> (i32, i32) {
        (self.section_index, self.index)
    }
}

impl DwIterator for OofAwareIterator {
    fn widget(&self) -> WidgetId {
        self.widget
    }
    fn mask(&self) -> i16 {
        self.mask
    }
    fn content(&self) -> &Content {
        &self.content
    }
    fn set_content(&mut self, content: Content) {
        self.content = content;
    }
    fn clone_box(&self) -> Box<dyn DwIterator> {
        Box::new(OofAwareIterator {
            widget: self.widget,
            mask: self.mask,
            content: self.content.clone(),
            section_index: self.section_index,
            index: self.index,
            in_flow: self.in_flow.clone_box(),
        })
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// `compareTo` — verbatim (`oofawarewidget_iterator.cc:118`) :
    /// `sectionIndex` puis `index`.
    fn compare(&self, other: &dyn DwIterator) -> i32 {
        match other.as_any().downcast_ref::<OofAwareIterator>() {
            Some(o) => {
                if self.section_index == o.section_index {
                    self.index - o.index
                } else {
                    self.section_index - o.section_index
                }
            }
            None => 0,
        }
    }

    /// `next` — verbatim (`oofawarewidget_iterator.cc:128`).
    fn next(&mut self, layout: &Layout) -> bool {
        let mut found = false;
        if self.content.r#type != content_type::END {
            while !found {
                self.index += 1;
                if self.section_index >= 0
                    && self.section_index < NUM_SECTIONS
                    && self.index >= 0
                    && self.index < self.num_parts(layout, self.section_index)
                {
                    self.get_part(layout, self.section_index, self.index);
                } else {
                    loop {
                        self.section_index += 1;
                        if self.section_index >= NUM_SECTIONS
                            || self.num_parts(layout, self.section_index) != 0
                        {
                            break;
                        }
                    }
                    if self.section_index >= NUM_SECTIONS {
                        self.content.r#type = content_type::END;
                        self.content.data = ContentData::None;
                        break;
                    }
                    self.index = -1;
                    continue;
                }
                found = (self.content.r#type & self.mask) != 0;
            }
        }
        found
    }

    /// `prev` — verbatim (`oofawarewidget_iterator.cc:177`).
    fn prev(&mut self, layout: &Layout) -> bool {
        let mut found = false;
        if self.content.r#type != content_type::START {
            while !found {
                self.index -= 1;
                if self.section_index >= 0
                    && self.section_index < NUM_SECTIONS
                    && self.index >= 0
                    && self.index < self.num_parts(layout, self.section_index)
                {
                    self.get_part(layout, self.section_index, self.index);
                } else {
                    loop {
                        self.section_index -= 1;
                        if self.section_index < 0 || self.num_parts(layout, self.section_index) != 0
                        {
                            break;
                        }
                    }
                    if self.section_index < 0 {
                        self.content.r#type = content_type::START;
                        self.content.data = ContentData::None;
                        break;
                    }
                    self.index = self.num_parts(layout, self.section_index);
                    continue;
                }
                found = (self.content.r#type & self.mask) != 0;
            }
        }
        found
    }

    /// `highlight` — in-flow → virtuel de sous-classe ; OOF →
    /// `highlightOOF` no-op verbatim
    /// (`oofawarewidget_iterator.cc:225`).
    fn highlight(
        &mut self,
        layout: &mut Layout,
        start: i32,
        end: i32,
        layer: crate::types::HighlightLayer,
    ) {
        if self.in_flow() {
            let index = self.index;
            self.in_flow
                .highlight_in_flow(layout, index, start, end, layer);
        }
    }

    /// `unhighlight` — `unhighlightOOF` no-op verbatim.
    fn unhighlight(
        &mut self,
        layout: &mut Layout,
        direction: i32,
        layer: crate::types::HighlightLayer,
    ) {
        if self.in_flow() {
            let index = self.index;
            self.in_flow
                .unhighlight_in_flow(layout, index, direction, layer);
        }
    }

    /// `getAllocation` — OOF : allocation du widget OOF verbatim
    /// (`oofawarewidget_iterator.cc:238`) ; in-flow → sous-classe.
    fn get_allocation(&self, layout: &Layout, start: i32, end: i32, allocation: &mut Allocation) {
        if self.in_flow() {
            self.in_flow
                .get_allocation_in_flow(layout, self.index, start, end, allocation);
        } else {
            let widget = layout
                .node_oof(self.widget, |o| {
                    o.out_of_flow_mgr[(self.section_index - 1) as usize].clone()
                })
                .flatten()
                .and_then(|m| m.borrow().get_widget(self.index as usize));
            if let Some(w) = widget {
                *allocation = layout.node(w, |n| n.allocation);
            }
        }
    }
}

// ------------------------------------------------------------------
// Tests — couche OOF (codec parentRef, container search,
// OOFFloatsMgr verbatim, itérateur OOF-aware).
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imgbuf::{Imgbuf, ImgbufType};
    use crate::layout::IdleCallback;
    use crate::platform::Platform;
    use crate::style::{Font, FontAttrs, Style, Tooltip};
    use crate::types::WidgetReference;
    use crate::view::View;

    /// `Platform` minimale (≙ mock de `iterator::tests`).
    #[derive(Default)]
    struct MockPlatform {
        /// Ids alloués.
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
                ascent: 10,
                descent: 2,
                space_width: 4,
                x_height: 5,
                zero_width: 5,
            })
        }
        fn font_exists(&mut self, _n: &[u8]) -> bool {
            false
        }
        fn create_color(&mut self, c: i32) -> crate::style::Color {
            crate::style::Color::new(c)
        }
        fn create_tooltip(&mut self, t: &[u8]) -> Rc<Tooltip> {
            Tooltip::create(t, None)
        }
        fn cancel_tooltip(&mut self) {}
        fn create_imgbuf(&mut self, _t: ImgbufType, _w: i32, _h: i32, _g: f64) -> Rc<dyn Imgbuf> {
            unreachable!("imgbuf non utilisé par les tests oof")
        }
        fn copy_selection(&mut self, _t: &[u8], _d: i32) {}
        fn get_resource_factory(&mut self) -> Option<Rc<dyn crate::ui::ResourceFactory>> {
            None
        }
    }

    fn make_layout() -> Layout {
        Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true)
    }

    /// Toplevel `OofAware` (≙ `Layout::addWidget` →
    /// `notifySetAsTopLevel` → containing blocks = soi-même).
    fn oof_toplevel(l: &mut Layout) -> WidgetId {
        let w = l.add_widget_node(WidgetKind::OofAware {
            oof: OofAwareData::new(),
        });
        l.add_widget(w);
        w
    }

    /// Widget `Base` stylé float + parent OOF-aware (≙ chemin
    /// amont : textblock attache le float à son containing block
    /// et enregistre `parentRef = makeParentRefOOF`).
    fn float_widget(l: &mut Layout, container: WidgetId, vloat: FloatType) -> WidgetId {
        let w = l.add_widget_node(WidgetKind::Base);
        l.node_mut(w, |n| n.parent = Some(container));
        let attrs = StyleAttrs {
            vloat,
            ..StyleAttrs::default()
        };
        l.widget_set_style(w, &Style::create(&attrs));
        // ≙ `word->content.widgetReference` amont : assert dans
        // `updateGenerators` (verbatim — toujours posée pour un
        // widget OOF).
        l.widget_set_widget_reference(w, Some(WidgetReference::new(w)));
        w
    }

    /// `OofFloatsMgr` direct (≙ `initOutOfFlowMgrs` : le mgr est
    /// posé sur le container ; ici instancié à la main pour
    /// inspection des champs privés).
    fn floats_mgr(l: &mut Layout, container: WidgetId) -> OofFloatsMgr {
        OofFloatsMgr::new(l, container, OOFM_FLOATS)
    }

    // ---- codec parentRef (oofawarewidget.hh) ----

    #[test]
    fn parent_ref_in_flow_round_trip() {
        for sub in [0, 1, 42, 1000] {
            let r = make_parent_ref_in_flow(sub);
            assert!(!is_parent_ref_oof(r));
            assert_eq!(get_parent_ref_in_flow_sub_ref(r), sub);
        }
        assert!(!is_parent_ref_oof(-1));
    }

    #[test]
    fn parent_ref_oof_round_trip() {
        for (idx, sub) in [(OOFM_FLOATS, 0), (OOFM_FLOATS, 7), (OOFM_ABSOLUTE, 3)] {
            let r = make_parent_ref_oof(idx, sub);
            assert!(is_parent_ref_oof(r));
            assert_eq!(get_parent_ref_oof_index(r) as usize, idx);
            assert_eq!(get_parent_ref_oof_sub_ref(r), sub);
        }
        // Les 3 bits bas portent l'index + 1 : FLOATS → reste 1.
        assert_eq!(make_parent_ref_oof(OOFM_FLOATS, 5), (5 << 3) | 1);
        assert_eq!(make_parent_ref_oof(OOFM_ABSOLUTE, 5), (5 << 3) | 2);
    }

    #[test]
    fn float_sub_ref_codecs() {
        assert_eq!(OofFloatsMgr::create_sub_ref_left_float(3), 6);
        assert_eq!(OofFloatsMgr::create_sub_ref_right_float(3), 7);
        assert!(OofFloatsMgr::is_sub_ref_left_float(6));
        assert!(OofFloatsMgr::is_sub_ref_right_float(7));
        assert!(!OofFloatsMgr::is_sub_ref_left_float(-1));
        assert_eq!(OofFloatsMgr::get_float_index_from_sub_ref(7), 3);
        assert_eq!(OofFloatsMgr::get_float_index_from_sub_ref(-1), -1);
    }

    // ---- helpers testStyle* ----

    #[test]
    fn test_style_helpers() {
        let mut attrs = StyleAttrs {
            vloat: FloatType::Left,
            position: Position::Absolute,
            ..StyleAttrs::default()
        };
        attrs.vloat = FloatType::None;
        attrs.position = Position::Static;
        assert!(!test_style_float(&attrs));
        assert!(!test_style_out_of_flow(&attrs));
        attrs.vloat = FloatType::Left;
        assert!(test_style_float(&attrs));
        assert!(test_style_out_of_flow(&attrs));
        // `IMPL_POS` → positionnements morts même si position
        // non-statique (verbatim).
        attrs.position = Position::Absolute;
        assert!(!test_style_absolutely_positioned(&attrs));
        assert!(!test_style_positioned(&attrs));
        assert!(!must_add_breaks(&attrs));
        attrs.position = Position::Relative;
        assert!(!test_style_relatively_positioned(&attrs));
        // float + relative : `!outOfFlow || relPos` = false amont.
        assert!(!must_add_breaks(&attrs));
        attrs.vloat = FloatType::None;
        // non-OOF → `!outOfFlow` = true verbatim.
        assert!(must_add_breaks(&attrs));
    }

    // ---- oofContainer ----

    #[test]
    fn toplevel_containers_are_self() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let containers = l.node_oof(top, |o| o.oof_container).unwrap();
        assert_eq!(containers, [Some(top); NUM_OOFM]);
    }

    #[test]
    fn is_oof_container_fixed_absolute() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        // Toplevel : container pour tous les rôles positionnés
        // (parent == NULL verbatim).
        assert!(l.is_oof_container(top, OOFM_FIXED));
        assert!(l.is_oof_container(top, OOFM_ABSOLUTE));
        assert!(l.is_oof_container(top, OOFM_RELATIVE));
        // FLOATS : `isPossibleOOFContainer` = false pour le kind
        // nu (verbatim base — Textblock retournera true).
        assert!(!l.is_oof_container(top, OOFM_FLOATS));
        // Enfant (parent fixé directement, ≙ widget non-toplevel)
        // : FIXED/ABSOLUTE → false sans `position`.
        let child = l.add_widget_node(WidgetKind::OofAware {
            oof: OofAwareData::new(),
        });
        l.node_mut(child, |n| n.parent = Some(top));
        assert!(!l.is_oof_container(child, OOFM_FIXED));
        assert!(!l.is_oof_container(child, OOFM_ABSOLUTE));
    }

    // `oof_notify_set_parent` sur kinds nus : l'assert FLOATS
    /// amont (oofawarewidget.cc:201) est verbatim `debug_assert`
    /// — non testable en build debug (le kind `Textblock`
    /// rendra le chemin valide). `is_oof_container` couvre la
    /// logique de recherche.

    // ---- OofFloatsMgr ----

    #[test]
    fn mgr_ctor_registers_container() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mgr = floats_mgr(&mut l, top);
        assert_eq!(mgr.tb_infos.len(), 1);
        assert_eq!(mgr.tb_infos[0].widget, Some(top));
        assert_eq!(mgr.tb_infos[0].index, 0);
        assert!(mgr.size_changed);
        assert_eq!(mgr.get_num_widgets(), 0);
    }

    #[test]
    fn add_widget_oof_left_and_right() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let fl = float_widget(&mut l, top, FloatType::Left);
        let fr = float_widget(&mut l, top, FloatType::Right);
        let sl = mgr.add_widget_oof(&mut l, fl, top, 10);
        let sr = mgr.add_widget_oof(&mut l, fr, top, 11);
        assert!(OofFloatsMgr::is_sub_ref_left_float(sl));
        assert!(OofFloatsMgr::is_sub_ref_right_float(sr));
        assert_eq!(mgr.left_floats.len(), 1);
        assert_eq!(mgr.right_floats.len(), 1);
        // sideSpanningIndex global (gauche+droite - 1).
        assert_eq!(mgr.floats[0].side_spanning_index, 0);
        assert_eq!(mgr.floats[1].side_spanning_index, 1);
        // tbInfos[0] (container) détient les deux.
        assert_eq!(mgr.tb_infos[0].left_floats, vec![0]);
        assert_eq!(mgr.tb_infos[0].right_floats, vec![1]);
        // getWidget : gauche d'abord puis droite.
        assert_eq!(mgr.get_num_widgets(), 2);
        assert_eq!(mgr.get_widget(0), Some(fl));
        assert_eq!(mgr.get_widget(1), Some(fr));
        // Lookup par widget.
        assert_eq!(mgr.find_float_by_widget(fl), Some(0));
        assert_eq!(mgr.find_float_by_widget(fr), Some(1));
    }

    #[test]
    fn tell_position_pushes_below_previous_float() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        let f2 = float_widget(&mut l, top, FloatType::Left);
        let s1 = mgr.add_widget_oof(&mut l, f1, top, 0);
        let s2 = mgr.add_widget_oof(&mut l, f2, top, 1);
        // Tailles semées (≙ sizeRequest — `dirty = false`).
        let (i1, i2) = (
            mgr.find_float_by_widget(f1).unwrap(),
            mgr.find_float_by_widget(f2).unwrap(),
        );
        mgr.floats[i1].size = Requisition {
            width: 20,
            ascent: 10,
            descent: 5,
        };
        mgr.floats[i1].dirty = false;
        mgr.floats[i2].size = Requisition {
            width: 10,
            ascent: 8,
            descent: 2,
        };
        mgr.floats[i2].dirty = false;
        // f1 à y=0 → yReal = 0 (premier, pas de collision).
        mgr.tell_position1(&mut l, f1, 0, 0);
        assert_eq!(mgr.floats[i1].y_real, 0);
        // f2 demandé à y=3 < bas de f1 (15) → poussé à 15.
        mgr.tell_position1(&mut l, f2, 0, 3);
        assert_eq!(mgr.floats[i2].y_req, 3);
        assert_eq!(mgr.floats[i2].y_real, 15);
        let _ = (s1, s2);
    }

    #[test]
    fn opposite_side_collision_requires_h_overlap() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        let f2 = float_widget(&mut l, top, FloatType::Right);
        mgr.add_widget_oof(&mut l, f1, top, 0);
        mgr.add_widget_oof(&mut l, f2, top, 1);
        let (i1, i2) = (
            mgr.find_float_by_widget(f1).unwrap(),
            mgr.find_float_by_widget(f2).unwrap(),
        );
        // f1 gauche large 20 @ x gen 0 ; f2 droit large 10 —
        // conteneur getMaxGeneratorWidth = 0 (kind nu) → x2
        // clampé à 0 → chevauchement horizontal.
        mgr.floats[i1].size = Requisition {
            width: 20,
            ascent: 10,
            descent: 5,
        };
        mgr.floats[i1].dirty = false;
        mgr.floats[i2].size = Requisition {
            width: 10,
            ascent: 8,
            descent: 2,
        };
        mgr.floats[i2].dirty = false;
        mgr.tell_position1(&mut l, f1, 0, 0);
        // f2 demandé à y=3 : collision verticale avec f1 + f1
        // s'étend horizontalement (20 > x2=0) → poussé à 15.
        mgr.tell_position1(&mut l, f2, 0, 3);
        assert_eq!(mgr.floats[i2].y_real, 15);
    }

    #[test]
    fn borders_and_float_height() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        mgr.add_widget_oof(&mut l, f1, top, 0);
        let i1 = mgr.find_float_by_widget(f1).unwrap();
        mgr.floats[i1].size = Requisition {
            width: 30,
            ascent: 10,
            descent: 5,
        };
        mgr.floats[i1].dirty = false;
        mgr.tell_position1(&mut l, f1, 0, 5);
        // Ligne y=5..12 couverte par le float (yReal=5, h=15).
        assert!(mgr.has_float_left(&mut l, 5, 7, Some(top), 0));
        // Bord gauche : width + (genX + boxOffsetX) = 30 + 0.
        assert_eq!(mgr.get_left_border(&mut l, 5, 7, Some(top), 0), 30);
        // Hauteur de float vue d'une ligne à y=8 : 15 - (8-5) = 12.
        assert_eq!(mgr.get_left_float_height(&mut l, 8, 7, Some(top), 0), 12);
        // Sous le float : rien.
        assert!(!mgr.has_float_left(&mut l, 20, 5, Some(top), 0));
        assert_eq!(mgr.get_left_border(&mut l, 20, 5, Some(top), 0), 0);
    }

    #[test]
    fn clear_position_below_floats() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        let cleared = l.add_widget_node(WidgetKind::Base);
        l.node_mut(cleared, |n| n.parent = Some(top));
        let attrs = StyleAttrs {
            clear: ClearType::Both,
            ..StyleAttrs::default()
        };
        l.widget_set_style(cleared, &Style::create(&attrs));
        mgr.add_widget_oof(&mut l, f1, top, 0);
        let i1 = mgr.find_float_by_widget(f1).unwrap();
        mgr.floats[i1].size = Requisition {
            width: 30,
            ascent: 10,
            descent: 5,
        };
        mgr.floats[i1].dirty = false;
        mgr.tell_position1(&mut l, f1, 0, 4);
        // Le widget « cleared » est un générateur TBInfo ?
        // amont `findFloatIndex (widget, -1)` cherche le dernier
        // float avant ce générateur — `cleared` n'est pas
        // générateur de float : le dernier float de la liste est
        // avant lui (compare par index TBInfo → f1 < cleared).
        // → pos = yReal + h - genY = 4 + 15 - 0 = 19.
        mgr.add_widget_in_flow(&mut l, cleared, Some(top), 3);
        let pos = mgr.get_clear_position(&mut l, cleared);
        assert_eq!(pos, 19);
    }

    #[test]
    fn move_external_indices_shifts() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        let f2 = float_widget(&mut l, top, FloatType::Right);
        mgr.add_widget_oof(&mut l, f1, top, 5);
        mgr.add_widget_oof(&mut l, f2, top, 9);
        let (i1, i2) = (
            mgr.find_float_by_widget(f1).unwrap(),
            mgr.find_float_by_widget(f2).unwrap(),
        );
        mgr.move_external_indices(&mut l, top, 7, 3);
        // f1 (5 < 7) inchangé ; f2 (9 >= 7) → 12.
        assert_eq!(mgr.floats[i1].external_index, 5);
        assert_eq!(mgr.floats[i2].external_index, 12);
    }

    #[test]
    fn mark_size_change_dirty_and_generators() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        let sub = mgr.add_widget_oof(&mut l, f1, top, 0);
        let i1 = mgr.find_float_by_widget(f1).unwrap();
        mgr.floats[i1].dirty = false;
        mgr.mark_size_change(&mut l, sub);
        assert!(mgr.floats[i1].dirty);
        // `SizeChanged` consommé par `updateGenerators`.
        assert!(!mgr.size_changed);
    }

    #[test]
    fn get_size_and_extremes_contribution() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        mgr.add_widget_oof(&mut l, f1, top, 0);
        let i1 = mgr.find_float_by_widget(f1).unwrap();
        mgr.floats[i1].size = Requisition {
            width: 30,
            ascent: 10,
            descent: 5,
        };
        mgr.floats[i1].dirty = false;
        mgr.tell_position1(&mut l, f1, 0, 2);
        let mut w = 0;
        let mut h = 0;
        mgr.get_size(&mut l, &Requisition::default(), &mut w, &mut h);
        // x = 0 (gen nu) + 30 ; y = 2 + 15 ; container boxRest = 0.
        assert_eq!(w, 30);
        assert_eq!(h, 17);
        assert!(mgr.size_changed);
        // Extremes : extr du widget Base = 0 → min 0 ; max garde
        // les deltas générateurs (verbatim bornes).
        let mut min = 0;
        let mut max = 0;
        mgr.get_extremes(&mut l, &Extremes::default(), &mut min, &mut max);
        assert_eq!(min, 0);
        let _ = max;
    }

    // ---- OofAwareIterator ----

    #[test]
    fn oof_iterator_walks_sections() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        let f2 = float_widget(&mut l, top, FloatType::Right);
        mgr.add_widget_oof(&mut l, f1, top, 0);
        mgr.add_widget_oof(&mut l, f2, top, 1);
        // Le mgr vit hors du nœud : `node_oof` lit les mgrs du
        // nœud — injecter le mgr dans le kind (≙ initOutOfFlowMgrs).
        let mgr_rc: Rc<RefCell<dyn OofMgr>> = Rc::new(RefCell::new(mgr));
        l.node_oof_mut(top, |o| {
            o.out_of_flow_mgr[OOFM_FLOATS] = Some(mgr_rc.clone());
        });

        let mut it = l.w_iterator(top, content_type::ANY_WIDGET, false);
        assert_eq!(it.content().r#type, content_type::START);
        // Section 0 vide (EmptyInFlow) → passe à la section OOF.
        assert!(it.next(&l));
        assert_eq!(it.content().r#type, content_type::WIDGET_OOF_CONT);
        assert_eq!(it.content().get_widget(), Some(f1));
        assert!(it.next(&l));
        assert_eq!(it.content().get_widget(), Some(f2));
        assert!(!it.next(&l));
        assert_eq!(it.content().r#type, content_type::END);
        // `prev` depuis END → f2.
        assert!(it.prev(&l));
        assert_eq!(it.content().get_widget(), Some(f2));
        assert!(it.prev(&l));
        assert_eq!(it.content().get_widget(), Some(f1));
        assert!(!it.prev(&l));
        assert_eq!(it.content().r#type, content_type::START);
        // `getAllocation` OOF = allocation du widget.
        l.node_mut(f1, |n| {
            n.allocation = Allocation {
                x: 3,
                y: 4,
                width: 10,
                ascent: 6,
                descent: 2,
            }
        });
        let mut it2 = l.w_iterator(top, content_type::ANY_WIDGET, false);
        it2.next(&l);
        let mut alloc = Allocation::default();
        it2.get_allocation(&l, 0, 0, &mut alloc);
        assert_eq!(alloc.x, 3);
        assert_eq!(alloc.width, 10);
        // mask TEXT → aucun widget → END direct.
        let mut it3 = l.w_iterator(top, content_type::TEXT, false);
        assert!(!it3.next(&l));
        assert_eq!(it3.content().r#type, content_type::END);
    }

    #[test]
    fn oof_iterator_compare() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        mgr.add_widget_oof(&mut l, f1, top, 0);
        let mgr_rc: Rc<RefCell<dyn OofMgr>> = Rc::new(RefCell::new(mgr));
        l.node_oof_mut(top, |o| o.out_of_flow_mgr[OOFM_FLOATS] = Some(mgr_rc));
        let mut a = l.w_iterator(top, content_type::ANY_WIDGET, false);
        let mut b = a.clone_box();
        a.next(&l);
        assert!(a.compare(&*b) > 0); // (1,0) vs (0,-1)
        b.next(&l);
        assert!(a.equals(&*b));
    }

    // ---- containerSizeChangedForChildren / delete ----

    #[test]
    fn detach_marks_oof_children() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        let sub = mgr.add_widget_oof(&mut l, f1, top, 0);
        let mgr_rc: Rc<RefCell<dyn OofMgr>> = Rc::new(RefCell::new(mgr));
        l.node_oof_mut(top, |o| o.out_of_flow_mgr[OOFM_FLOATS] = Some(mgr_rc));
        l.node_mut(f1, |n| n.parent_ref = make_parent_ref_oof(OOFM_FLOATS, sub));
        l.detach_widget(top);
        // `WIDGET_OOF_CONT` ≙ itération detach amont : le float
        // est marqué `detached` avec son container.
        assert!(l.node_detached(top));
        assert!(l.node_detached(f1));
    }

    #[test]
    fn delete_container_deletes_oof_children() {
        let mut l = make_layout();
        let top = oof_toplevel(&mut l);
        let mut mgr = floats_mgr(&mut l, top);
        let f1 = float_widget(&mut l, top, FloatType::Left);
        let sub = mgr.add_widget_oof(&mut l, f1, top, 0);
        let mgr_rc: Rc<RefCell<dyn OofMgr>> = Rc::new(RefCell::new(mgr));
        l.node_oof_mut(top, |o| o.out_of_flow_mgr[OOFM_FLOATS] = Some(mgr_rc));
        l.node_mut(f1, |n| n.parent_ref = make_parent_ref_oof(OOFM_FLOATS, sub));
        // `~OOFAwareWidget` détruit les enfants OOF → tombe `Base`.
        l.widget_delete(top);
        assert!(l.node(f1, |n| matches!(&n.kind, WidgetKind::Base)));
    }
}
