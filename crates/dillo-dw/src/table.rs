//! `dw/table.cc` + `dw/table.hh` + `dw/table_iterator.cc` +
//! `dw/tablecell.cc`/`tablecell.hh` — `dw::Table` : algorithme de
//! dimensionnement de colonnes **maison** de Dillo (pas le modèle
//! CSS — verbatim), cellules `SimpleTableCell`/`AlignedTableCell`
//! (sous-classes `Textblock` ≙ `cell` dans `TextblockData`) et
//! `TableIterator`.
//!
//! Structure amont : `children` est un `SimpleVector<Child*>` de
//! `numRows * numCols` cases, `NULL` ou `CELL` (cellule racine) /
//! `SPAN_SPACE` (recouvert par un span). Le port garde
//! `Vec<Option<Child>>` à plat — indices `row * num_cols + col`
//! verbatim.

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::doc_markdown)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::layout::Layout;
use crate::oof::{
    get_parent_ref_in_flow_sub_ref, handled_by_stacking_context_mgr, make_parent_ref_in_flow, sl,
    OofAwareData, OofAwareIterator, OofInFlow,
};
use crate::style::{multiply_with_per_length, Length, Style, LENGTH_AUTO};
use crate::types::{content_type, Allocation, Content, ContentData, Extremes};
use crate::widget::WidgetKind;
use crate::WidgetId;

/// `static bool Table::adjustTableMinWidth` (table.cc:26) —
/// globale réglée par les prefs (`setAdjustTableMinWidth`,
/// appelée depuis `dillo.cc` ≙ glue navigateur).
static ADJUST_TABLE_MIN_WIDTH: AtomicBool = AtomicBool::new(true);

/// `Table::setAdjustTableMinWidth` / `getAdjustTableMinWidth`
/// (table.cc:25-27 + table.hh:412-416).
pub fn set_adjust_table_min_width(v: bool) {
    ADJUST_TABLE_MIN_WIDTH.store(v, Ordering::Relaxed);
}

/// `Table::getAdjustTableMinWidth` (table.hh:415-416) —
/// accesseur statique ; `tablecell::getAdjustMinWidth`
/// (tablecell.cc:31-34) y délègue.
#[must_use]
pub fn adjust_table_min_width() -> bool {
    ADJUST_TABLE_MIN_WIDTH.load(Ordering::Relaxed)
}

/// `enum ExtrMod` (table.hh:381) — sélecteur de membre
/// `Extremes`/`Vec` pour les passes d'apportionnement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtrMod {
    /// `MIN` — `minWidth`.
    Min,
    /// `MIN_INTR` — `minWidthIntrinsic`.
    MinIntr,
    /// `MIN_MIN` — `min (minWidth, minWidthIntrinsic)`.
    MinMin,
    /// `MAX_MIN` — `max (minWidth, minWidthIntrinsic)`.
    MaxMin,
    /// `MAX` — `maxWidth`.
    Max,
    /// `MAX_INTR` — `maxWidthIntrinsic`.
    MaxIntr,
    /// `DATA` — lit/écrit `extrData` (`SimpleVector<int>*`).
    Data,
}

/// `struct Cell` (table.hh:330-333).
#[derive(Debug, Clone)]
pub struct CellData {
    /// `widget`.
    pub widget: WidgetId,
    /// `colspanOrig`.
    pub colspan_orig: i32,
    /// `colspanEff`.
    pub colspan_eff: i32,
    /// `rowspan`.
    pub rowspan: i32,
}

/// `struct Child` (table.hh:335-346) — `NULL` ≙ `None`.
#[derive(Debug, Clone)]
pub enum Child {
    /// `Child::CELL` — racine d'une cellule.
    Cell(CellData),
    /// `Child::SPAN_SPACE` — case recouverte par un span.
    SpanSpace {
        /// `startCol` — colonne de la cellule racine.
        start_col: i32,
        /// `startRow` — rangée de la cellule racine.
        start_row: i32,
    },
}

/// `dw::Table` — état verbatim `table.hh:325-517`
/// ([`WidgetKind::Table`] ≙ sous-classe `OOFAwareWidget`).
#[derive(Debug)]
pub struct TableData {
    /// `bool limitTextWidth`.
    pub limit_text_width: bool,
    /// `bool rowClosed`.
    pub row_closed: bool,
    /// `int numRows`.
    pub num_rows: i32,
    /// `int numCols`.
    pub num_cols: i32,
    /// `int curRow`.
    pub cur_row: i32,
    /// `int curCol`.
    pub cur_col: i32,
    /// `SimpleVector<Child*> *children` — `numRows * numCols`
    /// cases, `None` ≙ `NULL`.
    pub children: Vec<Option<Child>>,
    /// `int redrawX`.
    pub redraw_x: i32,
    /// `int redrawY`.
    pub redraw_y: i32,
    /// `SimpleVector<Extremes> *colExtremes`.
    pub col_extremes: Vec<Extremes>,
    /// `SimpleVector<bool> *colWidthSpecified`.
    pub col_width_specified: Vec<bool>,
    /// `int numColWidthSpecified`.
    pub num_col_width_specified: i32,
    /// `SimpleVector<bool> *colWidthPercentage`.
    pub col_width_percentage: Vec<bool>,
    /// `int numColWidthPercentage`.
    pub num_col_width_percentage: i32,
    /// `SimpleVector<int> *colWidths`.
    pub col_widths: Vec<i32>,
    /// `SimpleVector<int> *cumHeight` — `numRows + 1` cases.
    pub cum_height: Vec<i32>,
    /// `SimpleVector<int> *rowSpanCells`.
    pub row_span_cells: Vec<i32>,
    /// `SimpleVector<int> *baseline` — jamais rempli amont
    /// (`\bug` verbatim).
    pub baseline: Vec<i32>,
    /// `SimpleVector<Style*> *rowStyle` — `Rc` ≙ refcount amont.
    pub row_style: Vec<Option<Rc<Style>>>,
    /// `bool colWidthsUpToDateWidthColExtremes`.
    pub col_widths_up_to_date_width_col_extremes: bool,
    /// Partie `OOFAwareWidget` (≙ héritage amont).
    pub oof: OofAwareData,
}

impl TableData {
    /// `Table (limitTextWidth)` (table.cc:29-72).
    pub(crate) fn new(limit_text_width: bool) -> Self {
        TableData {
            limit_text_width,
            row_closed: false,
            num_rows: 0,
            num_cols: 0,
            cur_row: -1,
            cur_col: 0,
            children: Vec::new(),
            redraw_x: 0,
            redraw_y: 0,
            col_extremes: Vec::new(),
            col_width_specified: Vec::new(),
            num_col_width_specified: 0,
            col_width_percentage: Vec::new(),
            num_col_width_percentage: 0,
            col_widths: Vec::new(),
            cum_height: Vec::new(),
            row_span_cells: Vec::new(),
            baseline: Vec::new(),
            row_style: Vec::new(),
            col_widths_up_to_date_width_col_extremes: true,
            oof: OofAwareData::new(),
        }
    }

    /// `childDefined` (table.hh:383-387) — verbatim.
    fn child_defined(&self, n: i32) -> bool {
        usize::try_from(n).is_ok_and(|i| matches!(self.children.get(i), Some(Some(Child::Cell(_)))))
    }

    /// `child` — accès verbatim `children->get(n)`
    /// (`None` ≙ `NULL` ou hors bornes).
    fn child(&self, n: i32) -> Option<&Child> {
        usize::try_from(n)
            .ok()
            .and_then(|i| self.children.get(i))
            .and_then(Option::as_ref)
    }

    /// `setCumHeight` (table.hh:404-410) — verbatim : borne
    /// `redrawY` avant écriture.
    fn set_cum_height(&mut self, row: usize, value: i32) {
        if Some(&value) != self.cum_height.get(row) {
            self.redraw_y = self.redraw_y.min(value);
            if let Some(slot) = self.cum_height.get_mut(row) {
                *slot = value;
            }
        }
    }
}

/// `getExtreme` (table.cc:651-683) — verbatim.
fn get_extreme(extremes: &Extremes, m: ExtrMod) -> i32 {
    match m {
        ExtrMod::Min => extremes.min_width,
        ExtrMod::MinIntr => extremes.min_width_intrinsic,
        ExtrMod::MinMin => extremes.min_width.min(extremes.min_width_intrinsic),
        ExtrMod::MaxMin => extremes.min_width.max(extremes.min_width_intrinsic),
        ExtrMod::Max => extremes.max_width,
        ExtrMod::MaxIntr => extremes.max_width_intrinsic,
        ExtrMod::Data => 0, // `assertNotReached` amont.
    }
}

/// `setExtreme` (table.cc:685-716) — verbatim (`MIN_MIN`/
/// `MAX_MIN` non supportés amont → ignorés).
fn set_extreme(extremes: &mut Extremes, m: ExtrMod, value: i32) {
    match m {
        ExtrMod::Min => extremes.min_width = value,
        ExtrMod::MinIntr => extremes.min_width_intrinsic = value,
        ExtrMod::Max => extremes.max_width = value,
        ExtrMod::MaxIntr => extremes.max_width_intrinsic = value,
        // `assertNotReached` amont pour le reste.
        ExtrMod::MinMin | ExtrMod::MaxMin | ExtrMod::Data => {}
    }
}

impl TableData {
    /// `getColExtreme` (table.cc:700-710) — `DATA` lit
    /// `extr_data` (`SimpleVector<int>*` amont).
    fn get_col_extreme(&self, col: i32, m: ExtrMod, extr_data: Option<&[i32]>) -> i32 {
        match m {
            ExtrMod::Data => extr_data
                .and_then(|d| usize::try_from(col).ok().and_then(|i| d.get(i)))
                .copied()
                .unwrap_or(0),
            _ => usize::try_from(col)
                .ok()
                .and_then(|i| self.col_extremes.get(i))
                .map_or(0, |e| get_extreme(e, m)),
        }
    }

    /// `setColExtreme` (table.cc:712-722) — verbatim : `DATA`
    /// écrit dans `extr_data` *puis* (fallthrough amont) dans
    /// `colExtremes` via `setExtreme`.
    fn set_col_extreme(
        &mut self,
        col: i32,
        m: ExtrMod,
        extr_data: Option<&mut Vec<i32>>,
        value: i32,
    ) {
        let Ok(i) = usize::try_from(col) else { return };
        if m == ExtrMod::Data {
            if let Some(d) = extr_data {
                if let Some(slot) = d.get_mut(i) {
                    *slot = value;
                }
            }
        }
        if let Some(e) = self.col_extremes.get_mut(i) {
            set_extreme(e, m, value);
        }
    }
}

impl Layout {
    /// `new Table (limitTextWidth)` — crée le nœud
    /// [`WidgetKind::Table`] + [`TableData`].
    pub fn table_new(&mut self, limit_text_width: bool) -> WidgetId {
        self.add_widget_node(WidgetKind::Table {
            tbl: Box::new(TableData::new(limit_text_width)),
        })
    }

    /// Accès [`TableData`] — `node(w, |n| ..)` extracteur
    /// `mem::replace` (≙ `node_tb`, modèle D).
    pub(crate) fn node_tbl<T>(&self, w: WidgetId, f: impl FnOnce(&TableData) -> T) -> Option<T> {
        self.node(w, |n| match &n.kind {
            WidgetKind::Table { tbl } => Some(f(tbl)),
            _ => None,
        })
    }

    /// [`node_tbl`] mutable.
    pub(crate) fn node_tbl_mut<T>(
        &mut self,
        w: WidgetId,
        f: impl FnOnce(&mut TableData) -> T,
    ) -> Option<T> {
        self.node_mut(w, |n| match &mut n.kind {
            WidgetKind::Table { tbl } => Some(f(tbl)),
            _ => None,
        })
    }

    // --------------------------------------------------------------
    // Dimensionnement — verbatim table.cc.
    // --------------------------------------------------------------

    /// `Table::sizeRequestSimpl` (table.cc:107-129) — verbatim :
    /// `forceCalcCellSizes(true)`, largeur = boxDiff + spacings +
    /// Σ colWidths ; `ascent` = boxDiff + cumHeight[numRows] +
    /// vBorderSpacing ; corrections base + OOF (ordre ≙ textblock).
    pub(crate) fn tbl_size_request_simpl(
        &mut self,
        w: WidgetId,
        requisition: &mut crate::types::Requisition,
    ) {
        self.tbl_force_calc_cell_sizes(w, true);

        let (num_cols, total, ascent) = self
            .node_tbl(w, |t| {
                (
                    t.num_cols,
                    t.col_widths
                        .iter()
                        .take(t.num_cols.max(0) as usize)
                        .sum::<i32>(),
                    t.cum_height
                        .get(t.num_rows.max(0) as usize)
                        .copied()
                        .unwrap_or(0),
                )
            })
            .unwrap_or((0, 0, 0));
        let (h_sp, v_sp) = self.node(w, |n| {
            (
                n.style.as_ref().map_or(0, |s| s.attrs.h_border_spacing),
                n.style.as_ref().map_or(0, |s| s.attrs.v_border_spacing),
            )
        });

        requisition.width =
            self.node(w, crate::widget::WidgetNode::box_diff_width) + (num_cols + 1) * h_sp + total;
        requisition.ascent =
            self.node(w, crate::widget::WidgetNode::box_diff_height) + ascent + v_sp;
        requisition.descent = 0;

        self.widget_correct_requisition(
            w,
            requisition,
            crate::widget::split_height_preserve_descent,
            true,
            false,
        );
        self.correct_requisition_by_oof(
            w,
            requisition,
            crate::widget::split_height_preserve_descent,
        );
    }

    /// `Table::getExtremesSimpl` (table.cc:131-163) — verbatim.
    pub(crate) fn tbl_get_extremes_simpl(&mut self, w: WidgetId, extremes: &mut Extremes) {
        let num_cols = self.node_tbl(w, |t| t.num_cols).unwrap_or(0);
        let (h_sp, bdw) = self.node(w, |n| {
            (
                n.style.as_ref().map_or(0, |s| s.attrs.h_border_spacing),
                n.box_diff_width(),
            )
        });
        if num_cols == 0 {
            let v = bdw;
            extremes.min_width = v;
            extremes.min_width_intrinsic = v;
            extremes.max_width = v;
            extremes.max_width_intrinsic = v;
            extremes.adjustment_width = v;
        } else {
            self.tbl_force_calc_column_extremes(w);
            let sums = self
                .node_tbl(w, |t| {
                    t.col_extremes.iter().take(t.num_cols.max(0) as usize).fold(
                        (0i32, 0i32, 0i32, 0i32, 0i32),
                        |(a, b, c, d, e), x| {
                            (
                                a + x.min_width,
                                b + x.min_width_intrinsic,
                                c + x.max_width,
                                d + x.max_width_intrinsic,
                                e + x.adjustment_width,
                            )
                        },
                    )
                })
                .unwrap_or_default();
            let base = (num_cols + 1) * h_sp + bdw;
            extremes.min_width = base + sums.0;
            extremes.min_width_intrinsic = base + sums.1;
            extremes.max_width = base + sums.2;
            extremes.max_width_intrinsic = base + sums.3;
            extremes.adjustment_width = base + sums.4;
        }

        self.widget_correct_extremes(w, extremes, true);
        self.correct_extremes_by_oof(w, extremes);
    }

    /// `Table::sizeAllocateImpl` (table.cc:165-214) — verbatim :
    /// allocation colonne×rangée ; `ascent` = req ascent,
    /// `descent` = hauteur de span − vSpacing − ascent.
    pub(crate) fn tbl_size_allocate_impl(&mut self, w: WidgetId, allocation: &Allocation) {
        self.oof_size_allocate_start(w, allocation);
        self.tbl_calc_cell_sizes(w, true);

        let (num_cols, num_rows, h_sp, v_sp) = self.node(w, |n| {
            let (nc, nr) = self
                .node_tbl(w, |t| (t.num_cols, t.num_rows))
                .unwrap_or((0, 0));
            (
                nc,
                nr,
                n.style.as_ref().map_or(0, |s| s.attrs.h_border_spacing),
                n.style.as_ref().map_or(0, |s| s.attrs.v_border_spacing),
            )
        });
        let offy = allocation.y + self.node(w, crate::widget::WidgetNode::box_offset_y) + v_sp;
        let mut x = allocation.x + self.node(w, crate::widget::WidgetNode::box_offset_x) + h_sp;

        for col in 0..num_cols {
            for row in 0..num_rows {
                let n = row * num_cols + col;
                let cell = self.node_tbl(w, |t| {
                    if t.child_defined(n) {
                        match t.child(n) {
                            Some(Child::Cell(c)) => Some(c.clone()),
                            _ => None,
                        }
                    } else {
                        None
                    }
                });
                let Some(Some(cell)) = cell else { continue };

                let width = self
                    .node_tbl(w, |t| {
                        (cell.colspan_eff - 1) * h_sp
                            + (0..cell.colspan_eff)
                                .filter_map(|i| t.col_widths.get((col + i) as usize))
                                .sum::<i32>()
                    })
                    .unwrap_or(0);

                let mut child_requisition = crate::types::Requisition::default();
                self.widget_size_request(cell.widget, &mut child_requisition, 0, &[], &[], &[]);

                let (cum_row, cum_end) = self
                    .node_tbl(w, |t| {
                        (
                            t.cum_height.get(row as usize).copied().unwrap_or(0),
                            t.cum_height
                                .get((row + cell.rowspan) as usize)
                                .copied()
                                .unwrap_or(0),
                        )
                    })
                    .unwrap_or((0, 0));
                let child_allocation = Allocation {
                    x,
                    y: cum_row + offy,
                    width,
                    ascent: child_requisition.ascent,
                    descent: cum_end - cum_row - v_sp - child_requisition.ascent,
                };
                self.widget_size_allocate(cell.widget, &child_allocation);
            }
            x += self
                .node_tbl(w, |t| t.col_widths.get(col as usize).copied().unwrap_or(0))
                .unwrap_or(0)
                + h_sp;
        }

        self.oof_size_allocate_end(w);
    }

    /// `Table::resizeDrawImpl` (table.cc:216-222) — verbatim :
    /// invalidation à droite de `redrawX` et sous `redrawY`.
    pub(crate) fn tbl_resize_draw_impl(&mut self, w: WidgetId) {
        let (rx, ry, aw, h) = self.node(w, |n| {
            (
                self.node_tbl(w, |t| t.redraw_x).unwrap_or(0),
                self.node_tbl(w, |t| t.redraw_y).unwrap_or(0),
                n.allocation.width,
                n.get_height(),
            )
        });
        self.widget_queue_draw_area(w, rx, 0, aw - rx, h);
        self.widget_queue_draw_area(w, 0, ry, aw, h - ry);
        self.node_tbl_mut(w, |t| {
            t.redraw_x = aw;
            t.redraw_y = h;
        });
    }

    /// `Table::getAvailWidthOfChild` (table.cc:224-265) —
    /// verbatim : délégué au mgr OOF si géré ; sinon
    /// `calcAvailWidthForDescendant` (forceValue) / -1 — les
    /// largeurs de colonnes ne sont **pas** recalculées ici
    /// (commentaire amont verbatim).
    pub(crate) fn tbl_get_avail_width_of_child(
        &mut self,
        w: WidgetId,
        child: WidgetId,
        force_value: bool,
    ) -> i32 {
        if self.widget_is_oof(child) {
            if let Some(mgr) = self.widget_out_of_flow_mgr(w, child) {
                if mgr.borrow().dealing_with_size_of_child(child) {
                    return mgr
                        .borrow_mut()
                        .get_avail_width_of_child(self, child, force_value);
                }
            }
        }
        if force_value {
            self.tbl_calc_avail_width_for_descendant(w, child)
        } else {
            -1
        }
    }

    /// `Table::calcAvailWidthForDescendant` (table.cc:267-314) —
    /// verbatim : remonte à l'enfant direct, décode `parentRef`
    /// (indice `children`), somme `colWidths` du span ; pour un
    /// descendant non-direct, `calcFinalWidth` borne la largeur.
    fn tbl_calc_avail_width_for_descendant(&mut self, w: WidgetId, child: WidgetId) -> i32 {
        let mut actual_child = Some(child);
        while actual_child.is_some_and(|a| self.node(a, |n| n.parent) != Some(w)) {
            actual_child = actual_child.and_then(|a| self.node(a, |n| n.parent));
        }
        let Some(actual) = actual_child else {
            return -1;
        };

        let child_no = get_parent_ref_in_flow_sub_ref(self.node(actual, |n| n.parent_ref));
        let (num_cols, colspan_eff, h_sp) = self
            .node_tbl(w, |t| {
                let cs = match t.child(child_no) {
                    Some(Child::Cell(c)) => c.colspan_eff,
                    _ => 1,
                };
                (
                    t.num_cols,
                    cs,
                    self.node(w, |n| {
                        n.style.as_ref().map_or(0, |s| s.attrs.h_border_spacing)
                    }),
                )
            })
            .unwrap_or((0, 1, 0));
        let col = if num_cols > 0 { child_no % num_cols } else { 0 };

        let mut width = self
            .node_tbl(w, |t| {
                (colspan_eff - 1) * h_sp
                    + (0..colspan_eff)
                        .filter_map(|i| t.col_widths.get((col + i) as usize))
                        .sum::<i32>()
            })
            .unwrap_or(0)
            .max(0);

        if child != actual {
            // Pour les descendants non-directs, le `width` CSS est
            // regardé ici (verbatim).
            let mut corr_width = width;
            if let Some(style_) = self.widget_style(child) {
                self.widget_calc_final_width(child, &style_, -1, Some(w), 0, true, &mut corr_width);
            }
            width = width.min(corr_width);
        }
        width
    }

    /// `Table::applyPerWidth` / `applyPerHeight` (table.cc:316-324)
    /// — `multiplyWithPerLength` **sans** `boxDiff*` (verbatim :
    /// diffère de `Widget::applyPerWidth`).
    #[allow(clippy::unused_self)]
    pub(crate) fn tbl_apply_per_width(&self, container_width: i32, per_width: Length) -> i32 {
        multiply_with_per_length(container_width, per_width)
    }

    /// [`tbl_apply_per_width`] pour la hauteur.
    #[allow(clippy::unused_self)]
    pub(crate) fn tbl_apply_per_height(&self, container_height: i32, per_height: Length) -> i32 {
        multiply_with_per_length(container_height, per_height)
    }

    /// `Table::containerSizeChangedForChildren` (table.cc:326-341)
    /// — verbatim : toutes les cellules définies + OOF.
    pub(crate) fn tbl_container_size_changed_for_children(&mut self, w: WidgetId) {
        let cells: Vec<WidgetId> = self
            .node_tbl(w, |t| {
                t.children
                    .iter()
                    .filter_map(|c| match c {
                        Some(Child::Cell(cell)) => Some(cell.widget),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        for c in cells {
            self.widget_container_size_changed(c);
        }
        self.oof_container_size_changed_for_children(w);
    }

    /// `Table::drawLevel` (table.cc:377-423) — verbatim :
    /// `SL_IN_FLOW` dessine les cellules (hors stacking mgr),
    /// autres niveaux délégués `OOFAwareWidget::drawLevel`.
    pub(crate) fn tbl_draw_level(
        &mut self,
        w: WidgetId,
        view: &mut dyn crate::view::View,
        area: &crate::types::Rectangle,
        level: i32,
        context: &mut crate::types::DrawingContext,
    ) {
        if level == sl::IN_FLOW {
            let cells: Vec<WidgetId> = self
                .node_tbl(w, |t| {
                    t.children
                        .iter()
                        .filter_map(|c| match c {
                            Some(Child::Cell(cell)) => Some(cell.widget),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            for child in cells {
                if handled_by_stacking_context_mgr(self, child) {
                    continue;
                }
                if let Some(child_area) = self.widget_intersects(child, w, area) {
                    self.w_draw(child, view, &child_area, context);
                }
            }
        } else {
            self.oof_draw_level(w, view, area, level, context);
        }
    }

    /// `Table::getWidgetAtPointLevel` (table.cc:425-454) —
    /// verbatim : `SL_IN_FLOW` en ordre inverse, sinon délégué.
    pub(crate) fn tbl_get_widget_at_point_level(
        &mut self,
        w: WidgetId,
        x: i32,
        y: i32,
        level: i32,
        context: &mut crate::types::GettingWidgetAtPointContext,
    ) -> Option<WidgetId> {
        if level == sl::IN_FLOW {
            let cells: Vec<WidgetId> = self
                .node_tbl(w, |t| {
                    t.children
                        .iter()
                        .rev()
                        .filter_map(|c| match c {
                            Some(Child::Cell(cell)) => Some(cell.widget),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            for child in cells {
                if handled_by_stacking_context_mgr(self, child) {
                    continue;
                }
                if let Some(found) = self.w_get_widget_at_point(child, x, y, context) {
                    return Some(found);
                }
            }
            None
        } else {
            self.oof_get_widget_at_point_level(w, x, y, level, context)
        }
    }

    /// `Table::removeChild` (table.cc:457-460) — `\bug` amont :
    /// non implémenté → no-op verbatim.
    #[allow(clippy::unused_self)]
    pub(crate) fn tbl_remove_child(&self, _w: WidgetId, _child: WidgetId) {}

    /// `~Table` (table.cc:74-106) — verbatim : détruit les
    /// cellules (`Child::CELL`) ; `SPAN_SPACE` libérés avec le
    /// vecteur (≙ drop), `rowStyle->unref` ≙ drop des `Rc`.
    pub(crate) fn tbl_drop(&mut self, w: WidgetId) {
        let cells: Vec<WidgetId> = self
            .node_tbl(w, |t| {
                t.children
                    .iter()
                    .filter_map(|c| match c {
                        Some(Child::Cell(cell)) => Some(cell.widget),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        for c in cells {
            self.widget_delete(c);
        }
    }

    /// `Table::addCell` (table.cc:467-563) — verbatim :
    /// `maxspan` 100, rangée implicite si vide/`rowClosed`,
    /// `colspan 0` → `colspanEff = max(numCols-curCol, 1)` +
    /// `rowClosed`, remplissage `SPAN_SPACE`, `parentRef` =
    /// `makeParentRefInFlow(n)`, `setBgColor` rangée,
    /// `queueResize(0, true)`.
    pub fn tbl_add_cell(&mut self, w: WidgetId, widget: WidgetId, colspan: i32, rowspan: i32) {
        const MAXSPAN: i32 = 100;
        let mut colspan = colspan;
        let mut rowspan = rowspan;
        if !(0..=MAXSPAN).contains(&colspan) {
            // `MSG_WARN` amont — pas d'effet.
            colspan = MAXSPAN;
        }
        if rowspan > MAXSPAN || rowspan <= 0 {
            rowspan = MAXSPAN;
        }

        if self.node_tbl(w, |t| t.num_rows == 0).unwrap_or(true) {
            // `MSG` amont — rangée implicite (verbatim).
            self.tbl_add_row(w, None);
        }
        if self.node_tbl(w, |t| t.row_closed).unwrap_or(true) {
            // `MSG_WARN` amont — colspan=0 → nouvelle rangée.
            self.tbl_add_row(w, None);
        }

        let (num_cols, cur_row, cur_col) = self
            .node_tbl(w, |t| (t.num_cols, t.cur_row, t.cur_col))
            .unwrap_or((0, -1, 0));
        let colspan_eff = if colspan == 0 {
            self.node_tbl_mut(w, |t| t.row_closed = true);
            (num_cols - cur_col).max(1)
        } else {
            colspan
        };

        // Prochaine case libre (verbatim).
        let mut cur_col = cur_col;
        while cur_col < num_cols
            && matches!(
                self.node_tbl(w, |t| t.child(cur_row * num_cols + cur_col).cloned()),
                Some(Some(Child::SpanSpace { .. }))
            )
        {
            cur_col += 1;
        }
        self.node_tbl_mut(w, |t| t.cur_col = cur_col);

        if cur_row + rowspan > self.node_tbl(w, |t| t.num_rows).unwrap_or(0) {
            let nr = cur_row + rowspan;
            self.tbl_realloc_children(w, num_cols, nr);
        }
        if cur_col + colspan_eff > self.node_tbl(w, |t| t.num_cols).unwrap_or(0) {
            let nc = cur_col + colspan_eff;
            let nr = self.node_tbl(w, |t| t.num_rows).unwrap_or(0);
            self.tbl_realloc_children(w, nc, nr);
        }

        let num_cols = self.node_tbl(w, |t| t.num_cols).unwrap_or(0);

        // Remplissage `SPAN_SPACE` (verbatim).
        self.node_tbl_mut(w, |t| {
            for c in 0..colspan_eff {
                for r in 0..rowspan {
                    if !(c == 0 && r == 0) {
                        let i = ((cur_row + r) * num_cols + cur_col + c) as usize;
                        if i < t.children.len() {
                            t.children[i] = Some(Child::SpanSpace {
                                start_col: cur_col,
                                start_row: cur_row,
                            });
                        }
                    }
                }
            }
            let n = (cur_row * num_cols + cur_col) as usize;
            if n < t.children.len() {
                t.children[n] = Some(Child::Cell(CellData {
                    widget,
                    colspan_orig: colspan,
                    colspan_eff,
                    rowspan,
                }));
            }
            t.cur_col = cur_col + colspan_eff;
        });

        // `parentRef` ≙ position dans `children` (verbatim —
        // utilisé par `calcAvailWidthForDescendant`).
        self.node_mut(widget, |n| {
            n.parent_ref = make_parent_ref_in_flow(cur_row * num_cols + cur_col);
        });
        self.widget_set_parent(widget, w);
        let bg = self
            .node_tbl(w, |t| {
                usize::try_from(cur_row)
                    .ok()
                    .and_then(|i| t.row_style.get(i))
                    .and_then(Option::as_ref)
                    .and_then(|s| s.attrs.background_color)
            })
            .flatten();
        if bg.is_some() {
            self.widget_set_bg_color(widget, bg);
        }
        self.widget_queue_resize(w, 0, true);
    }

    /// `Table::addRow` (table.cc:565-579) — verbatim.
    pub fn tbl_add_row(&mut self, w: WidgetId, style: Option<Rc<Style>>) {
        self.node_tbl_mut(w, |t| t.cur_row += 1);
        if self
            .node_tbl(w, |t| t.cur_row >= t.num_rows)
            .unwrap_or(true)
        {
            let (nc, nr) = self
                .node_tbl(w, |t| (t.num_cols, t.cur_row + 1))
                .unwrap_or((0, 1));
            self.tbl_realloc_children(w, nc, nr);
        }
        self.node_tbl_mut(w, |t| {
            let i = t.cur_row as usize;
            if i >= t.row_style.len() {
                t.row_style.resize(i + 1, None);
            }
            t.row_style[i] = style;
            t.cur_col = 0;
            t.row_closed = false;
        });
    }

    /// `Table::getCellRef` (table.cc:581-594) — verbatim :
    /// première cellule `AlignedTableCell` (≙ `cell == Aligned`
    /// dans `TextblockData`).
    #[must_use]
    pub fn tbl_get_cell_ref(&self, w: WidgetId) -> Option<WidgetId> {
        let (num_cols, cur_col, num_rows) =
            self.node_tbl(w, |t| (t.num_cols, t.cur_col, t.num_rows))?;
        for row in 0..=num_rows {
            let n = cur_col + row * num_cols;
            if self.node_tbl(w, |t| t.child_defined(n)).unwrap_or(false) {
                if let Some(Some(Child::Cell(cell))) = self.node_tbl(w, |t| t.child(n).cloned()) {
                    if self
                        .node_tb(cell.widget, |tb| {
                            matches!(tb.cell, Some(TableCellKind::Aligned { .. }))
                        })
                        .unwrap_or(false)
                    {
                        return Some(cell.widget);
                    }
                }
            }
        }
        None
    }

    /// `Table::reallocChildren` (table.cc:724-826) — verbatim :
    /// agrandit la grille (recopie colonne par colonne, suivi des
    /// `colspanOrig == 0`, remplissage `SPAN_SPACE`), recalcule
    /// `parentRef`, initialise `colWidths` à 100.
    fn tbl_realloc_children(&mut self, w: WidgetId, new_num_cols: i32, new_num_rows: i32) {
        debug_assert!(new_num_cols >= self.node_tbl(w, |t| t.num_cols).unwrap_or(0));
        debug_assert!(new_num_rows >= self.node_tbl(w, |t| t.num_rows).unwrap_or(0));

        let (num_cols, num_rows) = self
            .node_tbl(w, |t| (t.num_cols, t.num_rows))
            .unwrap_or((0, 0));
        let mut children = self
            .node_tbl_mut(w, |t| {
                t.children
                    .resize((new_num_cols * new_num_rows).max(0) as usize, None);
                std::mem::take(&mut t.children)
            })
            .unwrap_or_default();

        if new_num_cols > num_cols {
            // Cas compliqué : la grille s'élargit aussi (verbatim).
            for row in (0..new_num_rows).rev() {
                let mut colspan0_col = -1i32;
                let mut colspan0_row = -1i32;
                for col in (0..num_cols).rev() {
                    let n = (row * new_num_cols + col) as usize;
                    children[n] = children[(row * num_cols + col) as usize].clone();
                    match &children[n] {
                        Some(Child::Cell(cell)) => {
                            if cell.colspan_orig == 0 {
                                colspan0_col = col;
                                colspan0_row = row;
                                if let Some(Child::Cell(c)) = &mut children[n] {
                                    c.colspan_eff = new_num_cols - col;
                                }
                            }
                        }
                        Some(Child::SpanSpace {
                            start_col,
                            start_row,
                        }) => {
                            let (sc, sr) = (*start_col, *start_row);
                            let root = children[(sr * num_cols + sc) as usize].clone();
                            if matches!(root, Some(Child::Cell(ref c)) if c.colspan_orig == 0) {
                                colspan0_col = sc;
                                colspan0_row = sr;
                            }
                        }
                        _ => {}
                    }
                }
                for col in num_cols..new_num_cols {
                    let i = (row * new_num_cols + col) as usize;
                    children[i] = if colspan0_col == -1 {
                        None
                    } else {
                        Some(Child::SpanSpace {
                            start_col: colspan0_col,
                            start_row: colspan0_row,
                        })
                    };
                }
            }
        }

        // Partie basse de la grille (verbatim).
        for row in num_rows..new_num_rows {
            for col in 0..new_num_cols {
                children[(row * new_num_cols + col) as usize] = None;
            }
        }

        // `parentRef` recalculé (verbatim — voir `addCell`).
        let mut parent_refs: Vec<(WidgetId, i32)> = Vec::new();
        if new_num_cols > num_cols {
            for row in 1..new_num_rows {
                for col in 0..new_num_cols {
                    let n = (row * new_num_cols + col) as usize;
                    if let Some(Child::Cell(cell)) = &children[n] {
                        parent_refs.push((cell.widget, make_parent_ref_in_flow(n as i32)));
                    }
                }
            }
        }

        self.node_tbl_mut(w, |t| {
            t.children = children;
            t.row_style.resize(new_num_rows.max(0) as usize, None);
            t.num_cols = new_num_cols;
            t.num_rows = new_num_rows;
            // `colWidths` init à 100 (valeur arbitraire verbatim —
            // largeur dispo définie avant le vrai calcul).
            t.col_widths.resize(t.num_cols as usize, 100);
        });
        for (widget, pr) in parent_refs {
            self.node_mut(widget, |n| n.parent_ref = pr);
        }
    }

    /// `Table::calcCellSizes` (table.cc:832-845) — verbatim.
    fn tbl_calc_cell_sizes(&mut self, w: WidgetId, calc_heights: bool) {
        let (size_changed, extremes_changet, uptodate) = self.node(w, |n| {
            (
                n.needs_resize() || n.resize_queued(),
                n.extremes_changed() || n.extremes_queued(),
                self.node_tbl(w, |t| t.col_widths_up_to_date_width_col_extremes)
                    .unwrap_or(true),
            )
        });
        if if calc_heights {
            extremes_changet || size_changed
        } else {
            extremes_changet || !uptodate
        } {
            self.tbl_force_calc_cell_sizes(w, calc_heights);
        }
    }

    /// `Table::forceCalcCellSizes` (table.cc:847-898) — verbatim :
    /// calcule puis notifie `containerSizeChanged` aux cellules
    /// des colonnes élargies (span : recherche de la cellule
    /// racine à gauche sur la même rangée).
    fn tbl_force_calc_cell_sizes(&mut self, w: WidgetId, calc_heights: bool) {
        let old_col_widths = self
            .node_tbl(w, |t| t.col_widths.clone())
            .unwrap_or_default();

        self.tbl_actually_calc_cell_sizes(w, calc_heights);

        let (num_cols, num_rows) = self
            .node_tbl(w, |t| (t.num_cols, t.num_rows))
            .unwrap_or((0, 0));
        for col in 0..num_cols {
            let new_w = self
                .node_tbl(w, |t| t.col_widths.get(col as usize).copied().unwrap_or(0))
                .unwrap_or(0);
            if old_col_widths.get(col as usize).copied().unwrap_or(0) == new_w {
                continue;
            }
            for row in 0..num_rows {
                let n = row * num_cols + col;
                let cell = self.node_tbl(w, |t| match t.child(n) {
                    Some(Child::Cell(c)) => Some(c.widget),
                    Some(Child::SpanSpace { .. }) => {
                        // Cellule multi-colonnes : cherche la
                        // racine à gauche (verbatim — les spans
                        // multi-rangées ont déjà été traités).
                        let mut found = None;
                        let mut col2 = col - 1;
                        while col2 >= 0 && found.is_none() {
                            if let Some(Child::Cell(c2)) = t.child(row * num_cols + col2) {
                                found = Some(c2.widget);
                            }
                            col2 -= 1;
                        }
                        found
                    }
                    None => None,
                });
                if let Some(Some(cw)) = cell {
                    self.widget_container_size_changed(cw);
                }
            }
        }
    }

    /// `Table::actuallyCalcCellSizes` (table.cc:900-1223) —
    /// verbatim : cas 1a/1b/2a/2b/3 d'apportionnement des
    /// colonnes + hauteurs de rangées + `apportionRowSpan`.
    #[allow(clippy::too_many_lines)] // verbatim actuallyCalcCellSizes (table.cc:903).
    fn tbl_actually_calc_cell_sizes(&mut self, w: WidgetId, calc_heights: bool) {
        // `getExtremes` appelle `forceCalcColumnExtremes` si
        // requis (verbatim).
        let mut extremes = Extremes::default();
        self.widget_get_extremes(w, &mut extremes, 0, &[], &[], &[]);

        let avail_width = self.widget_get_avail_width(w, true);
        // `adjustTableMinWidth` (verbatim).
        let corr_width = if adjust_table_min_width() {
            extremes.adjustment_width
        } else {
            0
        };
        let (num_cols, h_sp) = self.node(w, |n| {
            (
                self.node_tbl(w, |t| t.num_cols).unwrap_or(0),
                n.style.as_ref().map_or(0, |s| s.attrs.h_border_spacing),
            )
        });
        let total_width = avail_width.max(corr_width)
            - ((num_cols + 1) * h_sp + self.node(w, crate::widget::WidgetNode::box_diff_width));

        self.node_tbl_mut(w, |t| {
            t.cum_height.clear();
            t.cum_height.resize((t.num_rows + 1).max(0) as usize, 0);
            t.row_span_cells.clear();
            t.baseline.resize(t.num_rows.max(0) as usize, 0);
        });

        let old_col_widths = self
            .node_tbl_mut(w, |t| {
                std::mem::replace(&mut t.col_widths, vec![0; t.num_cols.max(0) as usize])
            })
            .unwrap_or_default();

        let (min_width, _min_width_intrinsic, max_width) = self
            .node_tbl(w, |t| {
                t.col_extremes
                    .iter()
                    .fold((0i32, 0i32, 0i32), |(a, b, c), e| {
                        (a + e.min_width, b + e.min_width_intrinsic, c + e.max_width)
                    })
            })
            .unwrap_or_default();

        // `width` CSS défini et effectif ? (verbatim — astuce
        // `correctRequisition` sur une req factice).
        let mut total_width_specified = false;
        if self.node(w, |n| {
            n.style
                .as_ref()
                .is_some_and(|s| s.attrs.width != LENGTH_AUTO)
        }) {
            let mut test_req = crate::types::Requisition {
                width: -1,
                ascent: -1,
                descent: -1,
            };
            self.widget_correct_requisition(
                w,
                &mut test_req,
                crate::widget::split_height_preserve_descent,
                true,
                false,
            );
            if test_req.width != -1 {
                total_width_specified = true;
            }
        }

        if min_width > total_width {
            // Cas 1 : somme des minima > largeur dispo.
            let (n_per, extr, per_flags, min_intr_sum) = self
                .node_tbl(w, |t| {
                    (
                        t.num_col_width_percentage,
                        t.col_extremes.clone(),
                        t.col_width_percentage.clone(),
                        t.col_extremes
                            .iter()
                            .map(|e| e.min_width_intrinsic)
                            .sum::<i32>(),
                    )
                })
                .unwrap_or_default();
            if n_per == 0 || min_intr_sum >= total_width {
                // Cas 1a : apportionnement simple.
                self.tbl_apportion2_collect(
                    w,
                    total_width,
                    0,
                    num_cols - 1,
                    ExtrMod::MinMin,
                    ExtrMod::MaxMin,
                    None,
                );
            } else {
                // Cas 1b : traitement spécial des pourcentages
                // (mimique Firefox/Chromium — verbatim).
                let mut width_part_per = total_width;
                let mut min_width_intrinsic_per = 0;
                for (col, e) in extr.iter().enumerate() {
                    if per_flags.get(col).copied().unwrap_or(false) {
                        min_width_intrinsic_per += e.min_width_intrinsic;
                    } else {
                        width_part_per -= e.min_width_intrinsic;
                    }
                }
                let mut new_widths = Vec::with_capacity(extr.len());
                for (col, e) in extr.iter().enumerate() {
                    if per_flags.get(col).copied().unwrap_or(false) {
                        let mut col_width = e.min_width;
                        let min_intr = e.min_width_intrinsic;
                        min_width_intrinsic_per -= min_intr;
                        if col_width > width_part_per - min_width_intrinsic_per {
                            col_width = width_part_per - min_width_intrinsic_per;
                        }
                        width_part_per -= col_width;
                        new_widths.push(col_width);
                    } else {
                        new_widths.push(e.min_width_intrinsic);
                    }
                }
                self.node_tbl_mut(w, |t| t.col_widths = new_widths);
            }
        } else if total_width_specified && total_width > max_width {
            // Cas 2 : largeur forcée > somme des maxima.
            let (n_spec, extr, spec_flags) = self
                .node_tbl(w, |t| {
                    (
                        t.num_col_width_specified,
                        t.col_extremes.clone(),
                        t.col_width_specified.clone(),
                    )
                })
                .unwrap_or_default();
            if n_spec == 0 || n_spec == num_cols {
                // Sous-cas 2a.
                self.tbl_apportion2_collect(
                    w,
                    total_width,
                    0,
                    num_cols - 1,
                    ExtrMod::Max,
                    ExtrMod::Max,
                    None,
                );
            } else {
                // Sous-cas 2b : colonnes spécifiées exclues de
                // l'apportionnement (verbatim).
                let num_not_specified = extr.len() as i32 - n_spec;
                let mut widths_not_specified = Vec::new();
                let mut total_not_specified = total_width;
                for (col, e) in extr.iter().enumerate() {
                    if spec_flags.get(col).copied().unwrap_or(false) {
                        total_not_specified -= e.max_width;
                    } else {
                        widths_not_specified.push(e.max_width);
                    }
                }
                let data = RefCell::new(widths_not_specified);
                let mut dest = Vec::new();
                self.tbl_apportion2(
                    w,
                    total_not_specified,
                    0,
                    num_not_specified - 1,
                    ExtrMod::Data,
                    ExtrMod::Data,
                    Some(&data),
                    &mut dest,
                    0,
                );
                self.node_tbl_mut(w, |t| {
                    let mut idx = 0usize;
                    for (col, e) in extr.iter().enumerate() {
                        if spec_flags.get(col).copied().unwrap_or(false) {
                            if col < t.col_widths.len() {
                                t.col_widths[col] = e.max_width;
                            }
                        } else {
                            if col < t.col_widths.len() {
                                t.col_widths[col] = dest.get(idx).copied().unwrap_or(0);
                            }
                            idx += 1;
                        }
                    }
                });
            }
        } else {
            // Cas 3 : apportionnement normal.
            let width = if total_width_specified {
                total_width
            } else {
                total_width.min(max_width)
            };
            self.tbl_apportion2_collect(
                w,
                width,
                0,
                num_cols - 1,
                ExtrMod::Min,
                ExtrMod::Max,
                None,
            );
        }

        // `redrawX` (verbatim — TODO amont conservé).
        self.node_tbl_mut(w, |t| {
            for col in 0..t.col_extremes.len() {
                if t.col_widths.get(col) != old_col_widths.get(col) {
                    t.redraw_x = t.redraw_x.min(t.col_widths[col]);
                }
            }
            t.col_widths_up_to_date_width_col_extremes = true;
        });

        // Notify `containerSizeChanged` (verbatim).
        let num_cols = self.node_tbl(w, |t| t.num_cols).unwrap_or(0);
        let num_rows = self.node_tbl(w, |t| t.num_rows).unwrap_or(0);
        for col in 0..num_cols {
            let changed = self
                .node_tbl(w, |t| {
                    col as usize >= old_col_widths.len()
                        || col as usize >= t.col_widths.len()
                        || old_col_widths[col as usize] != t.col_widths[col as usize]
                })
                .unwrap_or(false);
            if changed {
                for row in 0..num_rows {
                    let n = row * num_cols + col;
                    // TODO amont : spans multi-rangées seulement
                    // via la 1re colonne (verbatim).
                    if self.node_tbl(w, |t| t.child_defined(n)).unwrap_or(false) {
                        if let Some(Some(Child::Cell(c))) =
                            self.node_tbl(w, |t| t.child(n).cloned())
                        {
                            self.widget_container_size_changed(c.widget);
                        }
                    }
                }
            }
        }

        if calc_heights {
            self.node_tbl_mut(w, |t| t.set_cum_height(0, 0));
            for row in 0..num_rows {
                // `\bug` amont : `baseline` jamais rempli.
                let mut row_height = 0;
                for col in 0..num_cols {
                    let n = row * num_cols + col;
                    if self.node_tbl(w, |t| t.child_defined(n)).unwrap_or(false) {
                        if let Some(Some(Child::Cell(c))) =
                            self.node_tbl(w, |t| t.child(n).cloned())
                        {
                            let mut req = crate::types::Requisition::default();
                            self.widget_size_request(c.widget, &mut req, 0, &[], &[], &[]);
                            let child_height = req.ascent + req.descent;
                            if c.rowspan == 1 {
                                row_height = row_height.max(child_height);
                            } else {
                                self.node_tbl_mut(w, |t| t.row_span_cells.push(n));
                            }
                        }
                    }
                }
                let v_sp = self.node(w, |n| {
                    n.style.as_ref().map_or(0, |s| s.attrs.v_border_spacing)
                });
                let prev = self
                    .node_tbl(w, |t| t.cum_height.get(row as usize).copied().unwrap_or(0))
                    .unwrap_or(0);
                self.node_tbl_mut(w, |t| {
                    t.set_cum_height(row as usize + 1, prev + row_height + v_sp);
                });
            }
            self.tbl_apportion_row_span(w);
        }
    }

    /// `Table::apportionRowSpan` (table.cc:1225-1284) — verbatim :
    /// redistribution des hauteurs pour les cellules `rowspan>1`
    /// trop petites.
    fn tbl_apportion_row_span(&mut self, w: WidgetId) {
        let span_cells = self
            .node_tbl(w, |t| t.row_span_cells.clone())
            .unwrap_or_default();
        let mut row_height: Option<Vec<i32>> = None;

        for n in span_cells {
            let (row, rs, widget, v_sp, num_rows) = self.node(w, |no| {
                let t = self.node_tbl(w, |t| {
                    let num_cols = t.num_cols.max(1);
                    let row = n / num_cols;
                    let rs = match t.child(n) {
                        Some(Child::Cell(c)) => c.rowspan,
                        _ => 1,
                    };
                    let widget = match t.child(n) {
                        Some(Child::Cell(c)) => Some(c.widget),
                        _ => None,
                    };
                    (row, rs, widget, t.num_rows)
                });
                let (r, s, wid, nr) = t.unwrap_or((0, 1, None, 0));
                (
                    r,
                    s,
                    wid,
                    no.style.as_ref().map_or(0, |st| st.attrs.v_border_spacing),
                    nr,
                )
            });
            let Some(widget) = widget else { continue };
            let sum_rows = self
                .node_tbl(w, |t| {
                    t.cum_height.get((row + rs) as usize).copied().unwrap_or(0)
                        - t.cum_height.get(row as usize).copied().unwrap_or(0)
                })
                .unwrap_or(0);
            let mut req = crate::types::Requisition::default();
            self.widget_size_request(widget, &mut req, 0, &[], &[], &[]);
            let span_height = req.ascent + req.descent + v_sp;
            if sum_rows >= span_height {
                continue;
            }

            // Cellule trop petite (verbatim).
            if row_height.is_none() {
                row_height = Some(
                    self.node_tbl(w, |t| {
                        (0..num_rows)
                            .map(|i| {
                                t.cum_height.get((i + 1) as usize).copied().unwrap_or(0)
                                    - t.cum_height.get(i as usize).copied().unwrap_or(0)
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                );
            }
            let Some(rh) = row_height.as_mut() else {
                continue;
            };

            // Nouvelles hauteurs pour ce span (verbatim — cast
            // `(int)((float)...)` ≙ `as i32` tronque vers 0).
            let mut cum_h_new_i = 0;
            let mut cumh_i = 0;
            for i in row..row + rs {
                let iu = i as usize;
                let h_new_i = if sum_rows == 0 {
                    ((span_height - cum_h_new_i) as f32 / (row + rs - i) as f32) as i32
                } else if sum_rows - cumh_i <= 0 {
                    0
                } else {
                    ((span_height - cum_h_new_i) as f32 * rh.get(iu).copied().unwrap_or(0) as f32
                        / (sum_rows - cumh_i) as f32) as i32
                };
                cum_h_new_i += h_new_i;
                cumh_i += rh.get(iu).copied().unwrap_or(0);
                if iu < rh.len() {
                    rh[iu] = h_new_i;
                }
            }
            // Mise à jour `cumHeight` (verbatim).
            for i in 0..num_rows {
                let (prev, h) = self
                    .node_tbl(w, |t| {
                        (
                            t.cum_height.get(i as usize).copied().unwrap_or(0),
                            rh.get(i as usize).copied().unwrap_or(0),
                        )
                    })
                    .unwrap_or((0, 0));
                self.node_tbl_mut(w, |t| t.set_cum_height(i as usize + 1, prev + h));
            }
        }
    }

    /// `Table::forceCalcColumnExtremes` (table.cc:1290-1418) —
    /// verbatim : passe 1 (colspan=1) puis passe 2 (spans), puis
    /// recompte `numColWidth{Specified,Percentage}` — **bug
    /// amont conservé verbatim** : `numColWidthPercentage` n'est
    /// jamais remis à 0 (double reset de `numColWidthSpecified`).
    #[allow(clippy::too_many_lines)] // verbatim forceCalcColumnExtremes (table.cc:1295).
    fn tbl_force_calc_column_extremes(&mut self, w: WidgetId) {
        let num_cols = self.node_tbl(w, |t| t.num_cols).unwrap_or(0);
        let num_rows = self.node_tbl(w, |t| t.num_rows).unwrap_or(0);
        if num_cols > 0 {
            let mut col_span_cells: Vec<i32> = Vec::new();
            self.node_tbl_mut(w, |t| {
                t.col_extremes
                    .resize(num_cols.max(0) as usize, Extremes::default());
                t.col_width_specified
                    .resize(num_cols.max(0) as usize, false);
                t.col_width_percentage
                    .resize(num_cols.max(0) as usize, false);
            });

            // 1. cellules colspan = 1 (verbatim).
            for col in 0..num_cols {
                self.node_tbl_mut(w, |t| {
                    let i = col as usize;
                    t.col_width_specified[i] = false;
                    t.col_width_percentage[i] = false;
                    t.col_extremes[i] = Extremes::default();
                });
                for row in 0..num_rows {
                    let n = row * num_cols + col;
                    let (cell, colspan_eff) = self
                        .node_tbl(w, |t| match t.child(n) {
                            Some(Child::Cell(c)) => (Some(c.clone()), c.colspan_eff),
                            _ => (None, 0),
                        })
                        .unwrap_or((None, 0));
                    let Some(cell) = cell else { continue };
                    if colspan_eff == 1 {
                        let mut cell_extremes = Extremes::default();
                        self.widget_get_extremes(cell.widget, &mut cell_extremes, 0, &[], &[], &[]);
                        self.node_tbl_mut(w, |t| {
                            let i = col as usize;
                            let ce = &mut t.col_extremes[i];
                            ce.min_width_intrinsic = ce
                                .min_width_intrinsic
                                .max(cell_extremes.min_width_intrinsic);
                            ce.max_width_intrinsic = ce
                                .min_width_intrinsic
                                .max(ce.max_width_intrinsic)
                                .max(cell_extremes.max_width_intrinsic);
                            ce.min_width = ce.min_width.max(cell_extremes.min_width);
                            ce.max_width =
                                ce.min_width.max(ce.max_width).max(cell_extremes.max_width);
                            ce.adjustment_width =
                                ce.adjustment_width.max(cell_extremes.adjustment_width);
                        });
                        let child_width = self.widget_style(cell.widget).map(|s| s.attrs.width);
                        if child_width.is_some_and(|l| l != LENGTH_AUTO) {
                            self.node_tbl_mut(w, |t| {
                                t.col_width_specified[col as usize] = true;
                                if crate::style::is_per_length(child_width.unwrap_or(LENGTH_AUTO)) {
                                    t.col_width_percentage[col as usize] = true;
                                }
                            });
                        }
                    } else {
                        col_span_cells.push(n);
                    }
                }
            }

            // 2. cellules colspan > 1 (verbatim).
            for n in col_span_cells {
                let (col, cs, cell) = self
                    .node_tbl(w, |t| {
                        let c = n % t.num_cols;
                        match t.child(n) {
                            Some(Child::Cell(cell)) => (c, cell.colspan_eff, cell.clone()),
                            _ => (
                                c,
                                1,
                                CellData {
                                    widget: w,
                                    colspan_orig: 0,
                                    colspan_eff: 1,
                                    rowspan: 1,
                                },
                            ),
                        }
                    })
                    .unwrap_or((
                        n % num_cols.max(1),
                        1,
                        CellData {
                            widget: w,
                            colspan_orig: 0,
                            colspan_eff: 1,
                            rowspan: 1,
                        },
                    ));
                let mut cell_extremes = Extremes::default();
                self.widget_get_extremes(cell.widget, &mut cell_extremes, 0, &[], &[], &[]);

                self.tbl_calc_extremes_span_multi_cols(
                    w,
                    col,
                    cs,
                    &cell_extremes,
                    ExtrMod::Min,
                    ExtrMod::Max,
                    None,
                );
                self.tbl_calc_extremes_span_multi_cols(
                    w,
                    col,
                    cs,
                    &cell_extremes,
                    ExtrMod::MinIntr,
                    ExtrMod::MaxIntr,
                    None,
                );
                self.tbl_calc_adjustment_width_span_multi_cols(w, col, cs, &cell_extremes);

                let child_width = self.widget_style(cell.widget).map(|s| s.attrs.width);
                if child_width.is_some_and(|l| l != LENGTH_AUTO) {
                    self.node_tbl_mut(w, |t| {
                        for j in 0..cs {
                            let i = (col + j) as usize;
                            if i < t.col_width_specified.len() {
                                t.col_width_specified[i] = true;
                            }
                        }
                        if crate::style::is_per_length(child_width.unwrap_or(LENGTH_AUTO)) {
                            for j in 0..cs {
                                let i = (col + j) as usize;
                                if i < t.col_width_percentage.len() {
                                    t.col_width_percentage[i] = true;
                                }
                            }
                        }
                    });
                }
            }
        }

        self.node_tbl_mut(w, |t| {
            t.num_col_width_specified = 0;
            // `\bug` amont : `numColWidthSpecified = 0` deux fois,
            // `numColWidthPercentage` jamais remis à zéro —
            // verbatim (dérive observable du cumul conservée).
            t.num_col_width_specified = 0;
            for i in 0..t.col_extremes.len() {
                if t.col_width_specified.get(i).copied().unwrap_or(false) {
                    t.num_col_width_specified += 1;
                }
                if t.col_width_percentage.get(i).copied().unwrap_or(false) {
                    t.num_col_width_percentage += 1;
                }
            }
            t.col_widths_up_to_date_width_col_extremes = false;
        });
    }

    /// `Table::calcExtremesSpanMultiCols` (table.cc:1450-1499) —
    /// verbatim : si le min/max de la cellule dépasse la somme
    /// des colonnes, il est apportionné.
    #[allow(clippy::too_many_arguments)] // signature verbatim amont.
    fn tbl_calc_extremes_span_multi_cols(
        &mut self,
        w: WidgetId,
        col: i32,
        cs: i32,
        cell_extremes: &Extremes,
        min_extr_mod: ExtrMod,
        max_extr_mod: ExtrMod,
        extr_data: Option<&RefCell<Vec<i32>>>,
    ) {
        let cell_min = get_extreme(cell_extremes, min_extr_mod);
        let cell_max = get_extreme(cell_extremes, max_extr_mod);
        let (min_sum, max_sum) = self
            .node_tbl(w, |t| {
                let mut a = 0;
                let mut b = 0;
                for j in 0..cs {
                    let d = extr_data.map(|r| r.borrow());
                    let dref = d.as_deref().map(Vec::as_slice);
                    a += t.get_col_extreme(col + j, min_extr_mod, dref);
                    b += t.get_col_extreme(col + j, max_extr_mod, dref);
                }
                (a, b)
            })
            .unwrap_or((0, 0));

        let change_min = cell_min > min_sum;
        let change_max = cell_max > max_sum;
        if change_min || change_max {
            let mut new_min = Vec::new();
            let mut new_max = Vec::new();
            if change_min {
                self.tbl_apportion2(
                    w,
                    cell_min,
                    col,
                    col + cs - 1,
                    ExtrMod::Min,
                    ExtrMod::Max,
                    None,
                    &mut new_min,
                    0,
                );
            }
            if change_max {
                self.tbl_apportion2(
                    w,
                    cell_max,
                    col,
                    col + cs - 1,
                    ExtrMod::Min,
                    ExtrMod::Max,
                    None,
                    &mut new_max,
                    0,
                );
            }
            self.node_tbl_mut(w, |t| {
                for j in 0..cs {
                    let mut ed = extr_data.map(|r| r.borrow_mut());
                    if change_min {
                        t.set_col_extreme(
                            col + j,
                            min_extr_mod,
                            ed.as_deref_mut(),
                            new_min.get(j as usize).copied().unwrap_or(0),
                        );
                    }
                    if change_max {
                        t.set_col_extreme(
                            col + j,
                            max_extr_mod,
                            ed.as_deref_mut(),
                            new_max.get(j as usize).copied().unwrap_or(0),
                        );
                    }
                    // Cas confus : max ≥ min (verbatim).
                    let mn =
                        t.get_col_extreme(col + j, min_extr_mod, ed.as_deref().map(Vec::as_slice));
                    let mx =
                        t.get_col_extreme(col + j, max_extr_mod, ed.as_deref().map(Vec::as_slice));
                    t.set_col_extreme(col + j, max_extr_mod, ed.as_deref_mut(), mn.max(mx));
                }
            });
        }
    }

    /// `Table::calcAdjustmentWidthSpanMultiCols` (table.cc:1501-1522)
    /// — verbatim.
    fn tbl_calc_adjustment_width_span_multi_cols(
        &mut self,
        w: WidgetId,
        col: i32,
        cs: i32,
        cell_extremes: &Extremes,
    ) {
        let sum = self
            .node_tbl(w, |t| {
                (0..cs)
                    .map(|j| {
                        t.col_extremes
                            .get((col + j) as usize)
                            .map_or(0, |e| e.adjustment_width)
                    })
                    .sum::<i32>()
            })
            .unwrap_or(0);
        if cell_extremes.adjustment_width > sum {
            let mut new_adj = Vec::new();
            self.tbl_apportion2(
                w,
                cell_extremes.adjustment_width,
                col,
                col + cs - 1,
                ExtrMod::Min,
                ExtrMod::Max,
                None,
                &mut new_adj,
                0,
            );
            self.node_tbl_mut(w, |t| {
                for j in 0..cs {
                    if let Some(e) = t.col_extremes.get_mut((col + j) as usize) {
                        e.adjustment_width = new_adj.get(j as usize).copied().unwrap_or(0);
                    }
                }
            });
        }
    }

    /// `Table::apportion2` (table.cc:1527-1643) — verbatim :
    /// 3 cas (normal / totalMin≠0 / totalMin=0) avec répartition
    /// cumulative anti-arrondi (`rounding-errors.doc`).
    #[allow(clippy::too_many_arguments)]
    fn tbl_apportion2(
        &mut self,
        w: WidgetId,
        total_width: i32,
        first_col: i32,
        last_col: i32,
        min_extr_mod: ExtrMod,
        max_extr_mod: ExtrMod,
        extr_data: Option<&RefCell<Vec<i32>>>,
        dest: &mut Vec<i32>,
        dest_offset: usize,
    ) {
        if last_col >= first_col {
            dest.resize(dest_offset + (last_col - first_col + 1).max(0) as usize, 0);

            let (total_min, total_max) = self
                .node_tbl(w, |t| {
                    let mut a = 0;
                    let mut b = 0;
                    for col in first_col..=last_col {
                        let d = extr_data.map(|r| r.borrow());
                        let dref = d.as_deref().map(Vec::as_slice);
                        a += t.get_col_extreme(col, min_extr_mod, dref);
                        b += t.get_col_extreme(col, max_extr_mod, dref);
                    }
                    (a, b)
                })
                .unwrap_or((0, 0));

            let total_diff_extr = total_max - total_min;
            if total_diff_extr != 0 {
                // Cas normal (rounding-errors.doc).
                let total_diff_width = total_width - total_min;
                let mut cum_diff_extr = 0;
                let mut cum_diff_width = 0;
                for col in first_col..=last_col {
                    let (min, max) = self
                        .node_tbl(w, |t| {
                            let d = extr_data.map(|r| r.borrow());
                            let dref = d.as_deref().map(Vec::as_slice);
                            (
                                t.get_col_extreme(col, min_extr_mod, dref),
                                t.get_col_extreme(col, max_extr_mod, dref),
                            )
                        })
                        .unwrap_or((0, 0));
                    let diff_extr = max - min;
                    cum_diff_extr += diff_extr;
                    let diff_width =
                        (cum_diff_extr * total_diff_width) / total_diff_extr - cum_diff_width;
                    cum_diff_width += diff_width;
                    let i = dest_offset + (col - first_col) as usize;
                    if i < dest.len() {
                        dest[i] = diff_width + min;
                    }
                }
            } else if total_min != 0 {
                // Cas spécial 1 : répartition selon les minima.
                let mut cum_min = 0;
                let mut cum_width = 0;
                for col in first_col..=last_col {
                    let min = self
                        .node_tbl(w, |t| {
                            let d = extr_data.map(|r| r.borrow());
                            t.get_col_extreme(col, min_extr_mod, d.as_deref().map(Vec::as_slice))
                        })
                        .unwrap_or(0);
                    cum_min += min;
                    let width = (cum_min * total_width) / total_min - cum_width;
                    cum_width += width;
                    let i = dest_offset + (col - first_col) as usize;
                    if i < dest.len() {
                        dest[i] = width;
                    }
                }
            } else {
                // Cas spécial 2 : parts égales (x[i] = 1).
                let mut cum_width = 0;
                let n = last_col - first_col + 1;
                for col in first_col..=last_col {
                    let i = col - first_col + 1;
                    let width = (i * total_width) / n - cum_width;
                    cum_width += width;
                    let i2 = dest_offset + (col - first_col) as usize;
                    if i2 < dest.len() {
                        dest[i2] = width;
                    }
                }
            }
        }
    }

    /// `apportion2` avec destination `colWidths` (≙ passage
    /// `dest = colWidths, destOffset = 0` amont).
    #[allow(clippy::too_many_arguments)] // signature verbatim amont.
    fn tbl_apportion2_collect(
        &mut self,
        w: WidgetId,
        total_width: i32,
        first_col: i32,
        last_col: i32,
        min_extr_mod: ExtrMod,
        max_extr_mod: ExtrMod,
        extr_data: Option<&RefCell<Vec<i32>>>,
    ) {
        let mut dest = Vec::new();
        self.tbl_apportion2(
            w,
            total_width,
            first_col,
            last_col,
            min_extr_mod,
            max_extr_mod,
            extr_data,
            &mut dest,
            0,
        );
        self.node_tbl_mut(w, |t| {
            for (i, v) in dest.iter().enumerate() {
                if i < t.col_widths.len() {
                    t.col_widths[i] = *v;
                }
            }
        });
    }
}

// ------------------------------------------------------------------
// `tablecell::` (tablecell.cc) — helpers partagés par
// `SimpleTableCell`/`AlignedTableCell` (≙ namespace `tablecell`).
// ------------------------------------------------------------------

/// `TableCellKind` — sous-classe `Textblock` ≙ champ `cell` de
/// [`crate::textblock::TextblockData`] : `Simple` (≙
/// `SimpleTableCell`) ou `Aligned` (≙ `AlignedTableCell` +
/// `AlignedTextblock`).
#[derive(Debug, Clone, Copy)]
pub enum TableCellKind {
    /// `dw::SimpleTableCell`.
    Simple,
    /// `dw::AlignedTableCell` — `charWordIndex`/`charWordPos`
    /// pour l'alignement sur `textAlignChar` (verbatim).
    Aligned {
        /// `int charWordIndex` — `-1` tant que le caractère
        /// d'alignement n'est pas trouvé.
        char_word_index: i32,
        /// `int charWordPos` — position dans le mot + 1.
        char_word_pos: i32,
    },
}

impl Layout {
    /// `tablecell::correctAvailWidthOfChild` (tablecell.cc:44-72)
    /// — verbatim : borne la largeur à `availWidth - boxDiffWidth`
    /// de la cellule.
    pub(crate) fn cell_correct_avail_width_of_child(
        &mut self,
        w: WidgetId,
        width: i32,
        force_value: bool,
    ) -> i32 {
        if width == -1 {
            return width;
        }
        let this_width = self.widget_get_avail_width(w, force_value);
        if this_width == -1 {
            return width;
        }
        (width.min(this_width - self.node(w, crate::widget::WidgetNode::box_diff_width))).max(0)
    }

    /// `tablecell::correctCorrectedRequisitionOfChild`
    /// (tablecell.cc:81-107) — verbatim : clamp `width` à
    /// `availWidth - boxDiffWidth` (min respecté si
    /// `!allowDecreaseWidth`).
    pub(crate) fn cell_correct_corrected_requisition_of_child(
        &mut self,
        w: WidgetId,
        requisition: &mut crate::types::Requisition,
        allow_decrease_width: bool,
    ) {
        let this_width = self.widget_get_avail_width(w, true);
        let new_width = (requisition
            .width
            .min(this_width - self.node(w, crate::widget::WidgetNode::box_diff_width)))
        .max(0);
        requisition.width = if allow_decrease_width {
            new_width
        } else {
            requisition.width.max(new_width)
        };
    }

    /// `SimpleTableCell` ctor — `Textblock(limitTextWidth)` +
    /// `cell = Simple` (simpletablecell.cc:29-36).
    pub fn simple_table_cell_new(&mut self, limit_text_width: bool) -> WidgetId {
        let w = self.tb_new(limit_text_width, false);
        self.node_tb_mut(w, |tb| tb.cell = Some(TableCellKind::Simple));
        w
    }

    /// `AlignedTableCell` ctor (alignedtablecell.cc:30-45) —
    /// `AlignedTextblock(limitTextWidth)` + `setRefTextblock(ref)`
    /// + `charWordIndex = -1` + `buttonSensitive = true` (verbatim).
    pub fn aligned_table_cell_new(
        &mut self,
        ref_cell: Option<WidgetId>,
        limit_text_width: bool,
    ) -> WidgetId {
        let w = self.tb_new(limit_text_width, false);
        self.node_tb_mut(w, |tb| {
            tb.cell = Some(TableCellKind::Aligned {
                char_word_index: -1,
                char_word_pos: 0,
            });
        });
        self.widget_set_button_sensitive(w, true);
        self.tb_set_ref_textblock(w, ref_cell);
        w
    }

    /// `AlignedTableCell::getValue` (alignedtablecell.cc:181-211)
    /// — verbatim : somme des largeurs des mots avant
    /// `charWordIndex` + `textWidth(text, charWordPos)` ; sans
    /// caractère trouvé → somme totale (dernier mot sans espace).
    pub(crate) fn cell_get_value(&mut self, w: WidgetId) -> i32 {
        self.node_tb(w, |tb| {
            let (cwi, cwp) = match tb.cell {
                Some(TableCellKind::Aligned {
                    char_word_index,
                    char_word_pos,
                }) => (char_word_index, char_word_pos),
                _ => (-1, 0),
            };
            let word_index = if cwi == -1 {
                tb.words.len() as i32 - 1
            } else {
                cwi
            };
            let mut wsum = 0;
            for i in 0..word_index.max(0) as usize {
                if let Some(word) = tb.words.get(i) {
                    wsum += word.size.width + i32::from(word.orig_space);
                }
            }
            if cwi == -1 {
                if let Some(word) = tb.words.last() {
                    wsum += word.size.width;
                }
            } else if let Some(word) = tb.words.get(cwi as usize) {
                // `layout->textWidth (font, text, charWordPos)`
                // verbatim — mesure brute, sans text-transform.
                if let (Some(font), ContentData::Text(text)) =
                    (&word.style.attrs.font, &word.content.data)
                {
                    let len = text.len().min(cwp.max(0) as usize);
                    wsum += self.platform.borrow_mut().text_width(font, &text[..len]);
                }
            }
            wsum
        })
        .unwrap_or(0)
    }

    /// `AlignedTableCell::setMaxValue` (alignedtablecell.cc:213-218)
    /// — verbatim : `line1Offset = maxValue - value`,
    /// `queueResize(makeParentRefInFlow(0), true)`.
    pub(crate) fn cell_set_max_value(&mut self, w: WidgetId, max_value: i32, value: i32) {
        self.node_tb_mut(w, |tb| tb.line1_offset = max_value - value);
        self.widget_queue_resize(w, make_parent_ref_in_flow(0), true);
    }

    /// `AlignedTableCell::wordWrap` (alignedtablecell.cc:152-179)
    /// — verbatim : détecte `textAlignChar` dans le mot courant
    /// (`strchr` ≙ `iter().position`), ou l'espace quand
    /// `textAlignChar == ' '` ; `updateValue` quand
    /// `wordIndex == charWordIndex`.
    pub(crate) fn cell_word_wrap_after(&mut self, w: WidgetId, word_index: i32) {
        let needs_update = self.node_tb_mut(w, |tb| {
            let Some(TableCellKind::Aligned {
                char_word_index, ..
            }) = tb.cell
            else {
                return false;
            };
            if char_word_index == -1 {
                if let Some(word) = tb.words.get(word_index as usize) {
                    if word.content.r#type == content_type::TEXT {
                        let align_char = word.style.attrs.text_align_char;
                        let pos = if let ContentData::Text(text) = &word.content.data {
                            text.iter().position(|&b| b == align_char)
                        } else {
                            None
                        };
                        if let Some(pos) = pos {
                            tb.cell = Some(TableCellKind::Aligned {
                                char_word_index: word_index,
                                char_word_pos: pos as i32 + 1,
                            });
                        } else if align_char == b' ' && word.content.space {
                            tb.cell = Some(TableCellKind::Aligned {
                                char_word_index: word_index + 1,
                                char_word_pos: 0,
                            });
                        }
                    }
                }
            }
            matches!(
                tb.cell,
                Some(TableCellKind::Aligned { char_word_index: cwi, .. }) if word_index == cwi
            )
        });
        if needs_update.unwrap_or(false) {
            self.tb_update_value(w);
        }
    }
}

// ------------------------------------------------------------------
// `table_iterator.cc` — `Table::TableIterator`.
// ------------------------------------------------------------------

/// `new TableIterator` (table_iterator.cc:28-32) —
/// `OofAwareIterator` avec `numParts = children->size()`.
#[must_use]
pub fn table_iterator(layout: &Layout, w: WidgetId, mask: i16, at_end: bool) -> OofAwareIterator {
    let n = layout.node_tbl(w, |t| t.children.len() as i32).unwrap_or(0);
    OofAwareIterator::new(
        w,
        mask,
        at_end,
        Box::new(TableInFlow { widget: w }),
        layout,
        n,
    )
}

/// Section en-flux de `TableIterator` — verbatim
/// `table_iterator.cc:34-83`.
pub struct TableInFlow {
    /// La table (≙ `getWidget()` amont).
    widget: WidgetId,
}

impl Clone for TableInFlow {
    fn clone(&self) -> Self {
        TableInFlow {
            widget: self.widget,
        }
    }
}

impl OofInFlow for TableInFlow {
    /// `numContentsInFlow` (table_iterator.cc:60-63) —
    /// `children->size()`.
    fn num_contents_in_flow(&self, layout: &Layout) -> i32 {
        layout
            .node_tbl(self.widget, |t| t.children.len() as i32)
            .unwrap_or(0)
    }

    /// `getContentInFlow` (table_iterator.cc:65-77) — verbatim :
    /// `CELL` → `WIDGET_IN_FLOW`, autre → `INVALID`.
    fn get_content_in_flow(&self, layout: &Layout, index: i32, content: &mut Content) {
        *content = layout
            .node_tbl(self.widget, |t| match t.child(index) {
                Some(Child::Cell(c)) => Content {
                    r#type: content_type::WIDGET_IN_FLOW,
                    data: ContentData::Widget(c.widget),
                    space: false,
                },
                _ => Content {
                    r#type: content_type::INVALID,
                    data: ContentData::None,
                    space: false,
                },
            })
            .unwrap_or(Content {
                r#type: content_type::INVALID,
                data: ContentData::None,
                space: false,
            });
    }

    fn clone_box(&self) -> Box<dyn OofInFlow> {
        Box::new(self.clone())
    }

    /// `TableIterator::highlight` in-flow (table_iterator.cc:38-44)
    /// — verbatim : `/** todo Needs this an implementation? */`
    /// → no-op.
    fn highlight_in_flow(
        &self,
        _layout: &mut Layout,
        _index: i32,
        _start: i32,
        _end: i32,
        _layer: crate::types::HighlightLayer,
    ) {
    }

    /// `TableIterator::unhighlight` in-flow
    /// (table_iterator.cc:46-52) — verbatim `// ???` → no-op.
    fn unhighlight_in_flow(
        &self,
        _layout: &mut Layout,
        _index: i32,
        _direction: i32,
        _layer: crate::types::HighlightLayer,
    ) {
    }

    /// `TableIterator::getAllocation` in-flow
    /// (table_iterator.cc:54-60) — verbatim `\bug Not
    /// implemented.` → no-op.
    fn get_allocation_in_flow(
        &self,
        _layout: &Layout,
        _index: i32,
        _start: i32,
        _end: i32,
        _allocation: &mut Allocation,
    ) {
    }
}

// ------------------------------------------------------------------
// Tests — régression A07 (construction grille, spans, extrêmes,
// apportionnement, itérateur).
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imgbuf::{Imgbuf, ImgbufType};
    use crate::iterator::DwIterator;
    use crate::platform::{IdleCallback, Platform};
    use crate::style::{
        create_abs_length, create_per_length, Color, Font, FontAttrs, StyleAttrs, Tooltip,
    };
    use crate::types::content_type;
    use crate::view::View;

    // ---- Platform mock (verbatim du mock ui.rs) ----

    /// `Platform` mock — file d'idles interne.
    #[derive(Default)]
    struct MockPlatform {
        /// `(id, callback)` en attente.
        pending: Vec<(i32, IdleCallback)>,
        /// Prochain id.
        next_id: i32,
    }

    impl Platform for MockPlatform {
        fn set_layout(&mut self, _l: usize) {}
        fn attach_view(&mut self, _v: &mut dyn View) {}
        fn detach_view(&mut self, _v: &mut dyn View) {}
        fn text_width(&mut self, _f: &Font, t: &[u8]) -> i32 {
            // 1 px / octet — mesures déterministes.
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
        fn add_idle(&mut self, cb: IdleCallback) -> i32 {
            self.next_id += 1;
            self.pending.push((self.next_id, cb));
            self.next_id
        }
        fn remove_idle(&mut self, id: i32) {
            self.pending.retain(|(i, _)| *i != id);
        }
        fn take_pending_idles(&mut self) -> Vec<IdleCallback> {
            std::mem::take(&mut self.pending)
                .into_iter()
                .map(|(_, cb)| cb)
                .collect()
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
        fn get_row_area(&self, _r: i32, _a: &mut crate::types::Rectangle) {}
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
            false
        }
        fn set_delete_on_unref(&self, _delete_on_unref: bool) {}
        fn is_referred(&self) -> bool {
            false
        }
    }

    /// Layout de test.
    fn make_layout() -> Layout {
        Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true)
    }

    /// Cellule simple stylée `width` px (≙ `new SimpleTableCell`
    /// + `setStyle` amont).
    fn cell(l: &mut Layout, width: i32) -> WidgetId {
        let c = l.simple_table_cell_new(true);
        let attrs = StyleAttrs {
            width: create_abs_length(width),
            ..StyleAttrs::default()
        };
        l.widget_set_style(c, &Style::create(&attrs));
        c
    }

    /// Cellule stylée `width` × `height` px.
    fn cell_wh(l: &mut Layout, width: i32, height: i32) -> WidgetId {
        let c = l.simple_table_cell_new(true);
        let attrs = StyleAttrs {
            width: create_abs_length(width),
            height: create_abs_length(height),
            ..StyleAttrs::default()
        };
        l.widget_set_style(c, &Style::create(&attrs));
        c
    }

    /// Cellule simple stylée `width` en pourcentage.
    fn cell_per(l: &mut Layout, per: f64) -> WidgetId {
        let c = l.simple_table_cell_new(true);
        let attrs = StyleAttrs {
            width: create_per_length(per),
            ..StyleAttrs::default()
        };
        l.widget_set_style(c, &Style::create(&attrs));
        c
    }

    // ---- construction de la grille ----

    #[test]
    fn add_cell_creates_implicit_row() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c = cell(&mut l, 10);
        l.tbl_add_cell(t, c, 1, 1);
        let (nr, nc) = l.node_tbl(t, |t| (t.num_rows, t.num_cols)).unwrap();
        assert_eq!((nr, nc), (1, 1));
        assert!(matches!(
            l.node_tbl(t, |t| t.child(0).cloned()).unwrap(),
            Some(Child::Cell(_))
        ));
    }

    #[test]
    fn grid_raster_order() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c1 = cell(&mut l, 0);
        let c2 = cell(&mut l, 0);
        l.tbl_add_cell(t, c1, 1, 1);
        l.tbl_add_cell(t, c2, 1, 1);
        l.tbl_add_row(t, None);
        let c3 = cell(&mut l, 0);
        l.tbl_add_cell(t, c3, 1, 1);
        let (nr, nc) = l.node_tbl(t, |t| (t.num_rows, t.num_cols)).unwrap();
        assert_eq!((nr, nc), (2, 2));
        let ids: Vec<WidgetId> = l
            .node_tbl(t, |t| {
                t.children
                    .iter()
                    .filter_map(|c| match c {
                        Some(Child::Cell(c)) => Some(c.widget),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap();
        assert_eq!(ids, vec![c1, c2, c3]);
        // La case (1,1) reste `None` — rangée courte (verbatim).
        assert!(l.node_tbl(t, |t| t.child(3).is_none()).unwrap());
    }

    #[test]
    fn colspan_fills_span_space() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c = cell(&mut l, 0);
        l.tbl_add_cell(t, c, 2, 1);
        assert_eq!(l.node_tbl(t, |t| t.num_cols).unwrap(), 2);
        assert!(matches!(
            l.node_tbl(t, |t| t.child(1).cloned()).unwrap(),
            Some(Child::SpanSpace {
                start_col: 0,
                start_row: 0
            })
        ));
        assert_eq!(l.node_tbl(t, |t| t.cur_col).unwrap(), 2);
    }

    #[test]
    fn rowspan_fills_below() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c = cell(&mut l, 0);
        l.tbl_add_cell(t, c, 1, 2);
        assert_eq!(l.node_tbl(t, |t| t.num_rows).unwrap(), 2);
        // (1,0) est recouvert par le span.
        assert!(matches!(
            l.node_tbl(t, |t| t.child(t.num_cols).cloned()).unwrap(),
            Some(Child::SpanSpace {
                start_col: 0,
                start_row: 0
            })
        ));
    }

    #[test]
    fn colspan_zero_closes_row() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c1 = cell(&mut l, 0);
        let c2 = cell(&mut l, 0);
        l.tbl_add_cell(t, c1, 1, 1);
        // `colspan == 0` → `colspanEff = max(numCols-curCol, 1)`
        // + `rowClosed` (verbatim).
        l.tbl_add_cell(t, c2, 0, 1);
        assert!(l.node_tbl(t, |t| t.row_closed).unwrap());
        let c3 = cell(&mut l, 0);
        // Prochain `addCell` → nouvelle rangée implicite.
        l.tbl_add_cell(t, c3, 1, 1);
        assert_eq!(l.node_tbl(t, |t| t.num_rows).unwrap(), 2);
        assert_eq!(l.node_tbl(t, |t| t.cur_row).unwrap(), 1);
    }

    #[test]
    fn malformed_spans_clamped_to_maxspan() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c1 = cell(&mut l, 0);
        // `colspan > MAXSPAN` → borné à 100 (verbatim).
        l.tbl_add_cell(t, c1, 200, 1);
        assert_eq!(l.node_tbl(t, |t| t.num_cols).unwrap(), 100);
        l.tbl_add_row(t, None);
        let c2 = cell(&mut l, 0);
        // `rowspan <= 0` → borné à 100 (verbatim).
        l.tbl_add_cell(t, c2, 1, 0);
        assert_eq!(l.node_tbl(t, |t| t.num_rows).unwrap(), 101);
    }

    #[test]
    fn empty_table_extremes_zero() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let mut e = Extremes::default();
        l.widget_get_extremes(t, &mut e, 0, &[], &[], &[]);
        // `numCols == 0` → tous les extrêmes = boxDiff (≙ 0 sans
        // style — verbatim `getExtremesSimpl`).
        assert_eq!(e.min_width, 0);
        assert_eq!(e.max_width, 0);
        let mut req = crate::types::Requisition::default();
        l.widget_size_request(t, &mut req, 0, &[], &[], &[]);
        assert_eq!(req.width, 0);
    }

    // ---- extrêmes de colonnes ----

    #[test]
    fn col_extremes_from_styled_cells() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c1 = cell(&mut l, 50);
        let c2 = cell(&mut l, 30);
        l.tbl_add_cell(t, c1, 1, 1);
        l.tbl_add_cell(t, c2, 1, 1);
        l.tbl_force_calc_column_extremes(t);
        let extr = l.node_tbl(t, |t| t.col_extremes.clone()).unwrap();
        assert_eq!(extr.len(), 2);
        assert_eq!(extr[0].min_width, 50);
        assert_eq!(extr[0].max_width, 50);
        assert_eq!(extr[1].min_width, 30);
        assert_eq!(extr[1].max_width, 30);
        // Sommes aux extrêmes de la table (verbatim sommation).
        let mut e = Extremes::default();
        l.widget_get_extremes(t, &mut e, 0, &[], &[], &[]);
        assert_eq!(e.min_width, 80);
        assert_eq!(e.max_width, 80);
    }

    #[test]
    fn col_extremes_multicol_span_apportioned() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        // Colonnes : c1 (1 col, 20px), span (2 col, 100px).
        let c1 = cell(&mut l, 20);
        let c2 = cell(&mut l, 100);
        l.tbl_add_cell(t, c1, 1, 1);
        l.tbl_add_cell(t, c2, 2, 1);
        l.tbl_force_calc_column_extremes(t);
        let extr = l.node_tbl(t, |t| t.col_extremes.clone()).unwrap();
        assert_eq!(extr.len(), 3);
        // La cellule 100px est répartie sur les 2 dernières
        // colonnes (verbatim `calcExtremesSpanMultiCols` —
        // répartition en parts égales : 50/50).
        assert_eq!(extr[0].min_width, 20);
        assert_eq!(extr[1].min_width + extr[2].min_width, 100);
        assert_eq!(extr[1].min_width, 50);
        assert_eq!(extr[2].min_width, 50);
    }

    #[test]
    fn percentage_column_flagged() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c = cell_per(&mut l, 0.5);
        l.tbl_add_cell(t, c, 1, 1);
        l.tbl_force_calc_column_extremes(t);
        assert_eq!(l.node_tbl(t, |t| t.num_col_width_percentage).unwrap(), 1);
        assert!(l.node_tbl(t, |t| t.col_width_percentage[0]).unwrap());
    }

    // ---- apportionnement ----

    #[test]
    fn size_request_sums_col_widths() {
        let mut l = make_layout();
        l.viewport_width = 500;
        let t = l.table_new(true);
        l.add_widget(t);
        let c1 = cell(&mut l, 40);
        let c2 = cell(&mut l, 60);
        l.tbl_add_cell(t, c1, 1, 1);
        l.tbl_add_cell(t, c2, 1, 1);
        let mut req = crate::types::Requisition::default();
        l.widget_size_request(t, &mut req, 0, &[], &[], &[]);
        // `minWidth`=100 ≤ `totalWidth`=500 et pas de `width`
        // spécifié → cas 3 amont : `width = min(500, max=100)`
        // → apportionnement aux maxima (verbatim).
        assert_eq!(req.width, 100);
        let widths = l.node_tbl(t, |t| t.col_widths.clone()).unwrap();
        assert_eq!(widths, vec![40, 60]);
    }

    #[test]
    fn apportion2_cumulative_integer_rounding() {
        // Vérifie la distribution entière cumulative de
        // `apportion2` (verbatim — pas de dérive d'arrondi).
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c1 = cell(&mut l, 0);
        l.tbl_add_cell(t, c1, 1, 1);
        // Force 3 colonnes min=10,max=10 via `col_extremes`.
        l.node_tbl_mut(t, |t| {
            t.num_cols = 3;
            t.col_widths = vec![0; 3];
            t.col_extremes = vec![
                Extremes {
                    min_width: 10,
                    max_width: 10,
                    min_width_intrinsic: 10,
                    max_width_intrinsic: 10,
                    adjustment_width: 10,
                },
                Extremes {
                    min_width: 20,
                    max_width: 20,
                    min_width_intrinsic: 20,
                    max_width_intrinsic: 20,
                    adjustment_width: 20,
                },
                Extremes {
                    min_width: 30,
                    max_width: 30,
                    min_width_intrinsic: 30,
                    max_width_intrinsic: 30,
                    adjustment_width: 30,
                },
            ];
        });
        let mut dest = vec![0; 3];
        l.tbl_apportion2(t, 100, 0, 2, ExtrMod::Min, ExtrMod::Max, None, &mut dest, 0);
        // `totalDiffExtr == 0` (min == max) → cas spécial 1
        // amont : répartition du total selon les minima, en
        // arithmétique cumulative — cumMin {10,30,60} →
        // (10·100)/60=16, (30·100)/60−16=34, 100−50=50
        // (table.cc:1593-1610).
        assert_eq!(dest, vec![16, 34, 50]);
        assert_eq!(dest.iter().sum::<i32>(), 100);
    }

    #[test]
    fn apportion2_equal_parts_when_no_min() {
        // `totalMin == 0` → cas spécial 2 amont : parts égales,
        // reliquat d'arrondi sur la dernière colonne
        // (table.cc:1611-1640).
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c1 = cell(&mut l, 0);
        l.tbl_add_cell(t, c1, 1, 1);
        l.node_tbl_mut(t, |t| {
            t.num_cols = 3;
            t.col_extremes = vec![
                Extremes {
                    min_width: 0,
                    max_width: 0,
                    min_width_intrinsic: 0,
                    max_width_intrinsic: 0,
                    adjustment_width: 0,
                },
                Extremes {
                    min_width: 0,
                    max_width: 0,
                    min_width_intrinsic: 0,
                    max_width_intrinsic: 0,
                    adjustment_width: 0,
                },
                Extremes {
                    min_width: 0,
                    max_width: 0,
                    min_width_intrinsic: 0,
                    max_width_intrinsic: 0,
                    adjustment_width: 0,
                },
            ];
        });
        let mut dest = vec![0; 3];
        l.tbl_apportion2(t, 100, 0, 2, ExtrMod::Min, ExtrMod::Max, None, &mut dest, 0);
        // cumul : (1·100)/3=33, (2·100)/3−33=33, 100−66=34.
        assert_eq!(dest, vec![33, 33, 34]);
    }

    // ---- itérateur ----

    #[test]
    fn table_iterator_raster_order() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c1 = cell(&mut l, 0);
        let c2 = cell(&mut l, 0);
        l.tbl_add_cell(t, c1, 1, 1);
        l.tbl_add_cell(t, c2, 2, 1);
        l.tbl_add_row(t, None);
        let c3 = cell(&mut l, 0);
        l.tbl_add_cell(t, c3, 1, 1);
        let mut it = table_iterator(&l, t, content_type::WIDGET_IN_FLOW, false);
        // Raster order : racines `CELL` seulement — `SPAN_SPACE`
        // ≙ `InvalidIterator` amont (sauté).
        assert_eq!(it.content().r#type, content_type::START);
        let mut got = Vec::new();
        while it.next(&l) {
            if let Some(w) = it.content().get_widget() {
                got.push(w);
            }
        }
        assert_eq!(got, vec![c1, c2, c3]);
    }

    // ---- tables imbriquées ----

    #[test]
    fn nested_table_extremes_propagate() {
        let mut l = make_layout();
        // Amont, le toplevel est toujours un `Textblock` (seul
        // conteneur FLOATS possible — `isPossibleOOFContainer` :
        // `Table` retourne `oofmIndex != OOFM_FLOATS`).
        let top = l.tb_new(true, false);
        l.add_widget(top);
        let outer = l.table_new(true);
        l.tb_add_widget(top, outer, &Style::create(&StyleAttrs::default()));
        let inner = l.table_new(true);
        let ic = cell(&mut l, 70);
        l.tbl_add_cell(inner, ic, 1, 1);
        l.tbl_add_cell(outer, inner, 1, 1);
        let mut e = Extremes::default();
        l.widget_get_extremes(outer, &mut e, 0, &[], &[], &[]);
        // L'extrême de la cellule ≙ extrême de la table interne.
        assert_eq!(e.min_width, 70);
        assert_eq!(e.max_width, 70);
    }

    // ---- hauteurs ----

    #[test]
    fn cum_height_from_cell_requisitions() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c1 = cell_wh(&mut l, 10, 30);
        l.tbl_add_cell(t, c1, 1, 1);
        l.tbl_add_row(t, None);
        let c2 = cell_wh(&mut l, 10, 20);
        l.tbl_add_cell(t, c2, 1, 1);
        l.tbl_force_calc_cell_sizes(t, true);
        let cum = l.node_tbl(t, |t| t.cum_height.clone()).unwrap();
        assert_eq!(cum.len(), 3);
        assert_eq!(cum[0], 0);
        // `height` stylée → requête de la cellule ≈ hauteur forcée.
        assert_eq!(cum[1], 30);
        assert_eq!(cum[2], 50);
    }

    #[test]
    fn rowspan_height_redistributed() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        // Cellule rowspan=2 de 100px de haut (stylée) à côté
        // d'une cellule courte.
        let tall = l.simple_table_cell_new(true);
        l.widget_set_style(
            tall,
            &Style::create(&StyleAttrs {
                width: create_abs_length(10),
                height: create_abs_length(100),
                ..StyleAttrs::default()
            }),
        );
        let short = cell(&mut l, 10);
        l.tbl_add_cell(t, tall, 1, 2);
        l.tbl_add_cell(t, short, 1, 1);
        l.tbl_force_calc_cell_sizes(t, true);
        let cum = l.node_tbl(t, |t| t.cum_height.clone()).unwrap();
        // `apportionRowSpan` : les 2 rangées recouvertes
        // totalisent ≥ 100 (verbatim — redistribution du dépassement).
        assert!(cum[2] >= 100);
    }

    // ---- cellule alignée ----

    #[test]
    fn aligned_cell_value_without_char_is_sum() {
        let mut l = make_layout();
        let t = l.table_new(true);
        l.add_widget(t);
        let c = l.aligned_table_cell_new(None, true);
        let attrs = StyleAttrs {
            font: Some(Rc::new(Font {
                attrs: FontAttrs {
                    name: Vec::new(),
                    size: 10,
                    weight: 400,
                    letter_spacing: 0,
                    font_variant: crate::style::FontVariant::Normal,
                    style: crate::style::FontStyle::Normal,
                },
                ascent: 10,
                descent: 2,
                space_width: 4,
                x_height: 5,
                zero_width: 5,
            })),
            ..StyleAttrs::default()
        };
        let st = Style::create(&attrs);
        l.widget_set_style(c, &st);
        l.tbl_add_cell(t, c, 1, 1);
        // Sans caractère `textAlignChar` trouvé → valeur = somme
        // des largeurs des mots (verbatim getValue, cwi == -1).
        l.tb_add_text(c, b"abcd", 4, &st);
        // Forcer le wrap pour que les tailles soient calculées
        // (≙ `Textblock::flush` amont → `wordWrap`).
        l.tb_flush(c);
        let v = l.cell_get_value(c);
        // 4 caractères × 1px + mot unique → 4.
        assert_eq!(v, 4);
    }
}
