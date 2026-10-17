//! Port de `dw/types.hh` et `dw/types.cc`.
//!
//! Géométrie (`Point`, `Rectangle`, `Circle`, `Polygon`,
//! `Region`), structs de layout (`Allocation`, `Requisition`,
//! `Extremes`), `Content`/`WidgetReference` (avec `WidgetId` en
//! lieu de `Widget*`), et contextes de stacking
//! (`DrawingContext`, `GettingWidgetAtPointContext`).
//!
//! `Shape::draw` dépend de `core::View` + `style->color` : le
//! trait [`crate::view::View`] porte les trois primitives de
//! dessin utilisées ici ; le `Style*` amont se réduit au
//! `color: i32` extrait par l'appelant.

use std::collections::HashSet;

use crate::style::{Color, Shading};
use crate::view::View;
use crate::WidgetId;

/// `dw::core::HPosition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum HPosition {
    /// `HPOS_LEFT`.
    Left,
    /// `HPOS_CENTER`.
    Center,
    /// `HPOS_RIGHT`.
    Right,
    /// `HPOS_INTO_VIEW` — scroll only.
    IntoView,
    /// `HPOS_NO_CHANGE`.
    NoChange,
}

/// `dw::core::VPosition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VPosition {
    /// `VPOS_TOP`.
    Top,
    /// `VPOS_CENTER`.
    Center,
    /// `VPOS_BOTTOM`.
    Bottom,
    /// `VPOS_INTO_VIEW` — scroll only.
    IntoView,
    /// `VPOS_NO_CHANGE`.
    NoChange,
}

/// `dw::core::ScrollCommand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ScrollCommand {
    /// `SCREEN_UP_CMD`.
    ScreenUp,
    /// `SCREEN_DOWN_CMD`.
    ScreenDown,
    /// `SCREEN_LEFT_CMD`.
    ScreenLeft,
    /// `SCREEN_RIGHT_CMD`.
    ScreenRight,
    /// `LINE_UP_CMD`.
    LineUp,
    /// `LINE_DOWN_CMD`.
    LineDown,
    /// `LEFT_CMD`.
    Left,
    /// `RIGHT_CMD`.
    Right,
    /// `TOP_CMD`.
    Top,
    /// `BOTTOM_CMD`.
    Bottom,
    /// `NONE_CMD`.
    None,
}

/// `dw::core::HighlightLayer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum HighlightLayer {
    /// `HIGHLIGHT_SELECTION`.
    Selection,
    /// `HIGHLIGHT_FINDTEXT`.
    Findtext,
}

/// `HIGHLIGHT_NUM_LAYERS`.
pub const HIGHLIGHT_NUM_LAYERS: usize = 2;

/// `dw::core::Point`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Point {
    /// `x`.
    pub x: i32,
    /// `y`.
    pub y: i32,
}

/// `dw::core::Shape` — interface pour les formes de sélection
/// (liens, ancres).
pub trait Shape {
    /// `isPointWithin`.
    fn is_point_within(&self, x: i32, y: i32) -> bool;
    /// `draw` — `style` amont réduit à `style->color`.
    fn draw(&self, view: &mut dyn View, color: &Color, x: i32, y: i32);
}

/// `dw::core::Rectangle`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rectangle {
    /// `x`.
    pub x: i32,
    /// `y`.
    pub y: i32,
    /// `width`.
    pub width: i32,
    /// `height`.
    pub height: i32,
}

impl Rectangle {
    /// `Rectangle (x, y, width, height)`.
    #[must_use]
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Rectangle {
            x,
            y,
            width,
            height,
        }
    }

    /// `intersectsWith` — retourne l'intersection si elle existe
    /// (amont écrit `dest` ; `None` ≡ zéro).
    #[must_use]
    pub fn intersects_with(&self, other: &Rectangle) -> Option<Rectangle> {
        let do_intersect = self.x < other.x + other.width
            && self.y < other.y + other.height
            && other.x < self.x + self.width
            && other.y < self.y + self.height;

        if do_intersect {
            let x = self.x.max(other.x);
            let y = self.y.max(other.y);
            Some(Rectangle {
                x,
                y,
                width: (self.x + self.width).min(other.x + other.width) - x,
                height: (self.y + self.height).min(other.y + other.height) - y,
            })
        } else {
            None
        }
    }

    /// `isSubsetOf`.
    #[must_use]
    pub fn is_subset_of(&self, other: &Rectangle) -> bool {
        self.x >= other.x
            && self.y >= other.y
            && self.x + self.width <= other.x + other.width
            && self.y + self.height <= other.y + other.height
    }

    /// `isEmpty`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }
}

impl Shape for Rectangle {
    /// `isPointWithin`.
    fn is_point_within(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    /// `Rectangle::draw` — `filled = false` verbatim.
    fn draw(&self, view: &mut dyn View, color: &Color, x: i32, y: i32) {
        view.draw_rectangle(
            color,
            Shading::Normal,
            false,
            x + self.x,
            y + self.y,
            self.width,
            self.height,
        );
    }
}

/// `dw::core::Circle`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Circle {
    /// `x`.
    pub x: i32,
    /// `y`.
    pub y: i32,
    /// `radius`.
    pub radius: i32,
}

impl Circle {
    /// `Circle (x, y, radius)`.
    #[must_use]
    pub fn new(x: i32, y: i32, radius: i32) -> Self {
        Circle { x, y, radius }
    }
}

impl Shape for Circle {
    /// `isPointWithin`.
    fn is_point_within(&self, x: i32, y: i32) -> bool {
        (x - self.x) * (x - self.x) + (y - self.y) * (y - self.y) <= self.radius * self.radius
    }

    /// `Circle::draw` — `filled = false`, arc 0..360.
    fn draw(&self, view: &mut dyn View, color: &Color, x: i32, y: i32) {
        view.draw_arc(
            color,
            Shading::Normal,
            false,
            x + self.x,
            y + self.y,
            2 * self.radius,
            2 * self.radius,
            0,
            360,
        );
    }
}

/// `dw::core::Polygon`.
#[derive(Debug, Clone, Default)]
pub struct Polygon {
    points: Vec<Point>,
    minx: i32,
    miny: i32,
    maxx: i32,
    maxy: i32,
}

impl Polygon {
    /// `Polygon ()` — bornes ±`0xff_ffff` comme amont.
    #[must_use]
    pub fn new() -> Self {
        Polygon {
            points: Vec::with_capacity(8),
            minx: 0xff_ffff,
            miny: 0xff_ffff,
            maxx: -0xff_ffff,
            maxy: -0xff_ffff,
        }
    }

    /// `addPoint`.
    pub fn add_point(&mut self, x: i32, y: i32) {
        self.points.push(Point { x, y });
        self.minx = self.minx.min(x);
        self.miny = self.miny.min(y);
        self.maxx = self.maxx.max(x);
        self.maxy = self.maxy.max(y);
    }

    /// `zOfVectorProduct` — z du produit vectoriel en 2D.
    fn z_of_vector_product(x1: i32, y1: i32, x2: i32, y2: i32) -> i32 {
        x1 * y2 - x2 * y1
    }

    /// `linesCross0` — le segment `a` croise la droite `b`.
    #[allow(clippy::too_many_arguments)]
    fn lines_cross0(
        ax1: i32,
        ay1: i32,
        ax2: i32,
        ay2: i32,
        bx1: i32,
        by1: i32,
        bx2: i32,
        by2: i32,
    ) -> bool {
        let z1 = Self::z_of_vector_product(ax1 - bx1, ay1 - by1, bx2 - bx1, by2 - by1);
        let z2 = Self::z_of_vector_product(ax2 - bx1, ay2 - by1, bx2 - bx1, by2 - by1);

        (z1 <= 0 && z2 >= 0) || (z1 >= 0 && z2 <= 0)
    }

    /// `linesCross` — croisement des deux segments.
    #[allow(clippy::too_many_arguments)]
    fn lines_cross(
        ax1: i32,
        ay1: i32,
        ax2: i32,
        ay2: i32,
        bx1: i32,
        by1: i32,
        bx2: i32,
        by2: i32,
    ) -> bool {
        Self::lines_cross0(ax1, ay1, ax2, ay2, bx1, by1, bx2, by2)
            && Self::lines_cross0(bx1, by1, bx2, by2, ax1, ay1, ax2, ay2)
    }
}

impl Shape for Polygon {
    /// `isPointWithin` — ray-casting depuis `(minx-1, miny-1)`,
    /// verbatim (borne `y >= maxy` asymétrique amont).
    fn is_point_within(&self, x: i32, y: i32) -> bool {
        if self.points.len() < 3
            || (x < self.minx || x > self.maxx || y < self.miny || y >= self.maxy)
        {
            return false;
        }
        let mut num_crosses = 0;
        for i in 0..self.points.len() - 1 {
            if Self::lines_cross(
                self.minx - 1,
                self.miny - 1,
                x,
                y,
                self.points[i].x,
                self.points[i].y,
                self.points[i + 1].x,
                self.points[i + 1].y,
            ) {
                num_crosses += 1;
            }
        }
        if Self::lines_cross(
            self.minx - 1,
            self.miny - 1,
            x,
            y,
            self.points[self.points.len() - 1].x,
            self.points[self.points.len() - 1].y,
            self.points[0].x,
            self.points[0].y,
        ) {
            num_crosses += 1;
        }
        num_crosses % 2 == 1
    }

    /// `Polygon::draw` — `filled = false`, `convex = false`.
    fn draw(&self, view: &mut dyn View, color: &Color, x: i32, y: i32) {
        if !self.points.is_empty() {
            let pts: Vec<Point> = self
                .points
                .iter()
                .map(|p| Point {
                    x: x + p.x,
                    y: y + p.y,
                })
                .collect();
            view.draw_polygon(color, Shading::Normal, false, false, &pts);
        }
    }
}

/// `dw::core::Region` — ensemble de rectangles disjoints
/// (fusion agressive, ≤16 rect comme amont).
#[derive(Debug, Clone, Default)]
pub struct Region {
    rectangles: Vec<Rectangle>,
}

impl Region {
    /// `Region ()`.
    #[must_use]
    pub fn new() -> Self {
        Region {
            rectangles: Vec::new(),
        }
    }

    /// `clear`.
    pub fn clear(&mut self) {
        self.rectangles.clear();
    }

    /// `addRectangle` — fusionne si la couverture n'augmente
    /// pas, ou dès que la liste atteint 16 (quirk amont : la
    /// fusion forcée produit aussi un rectangle englobant).
    pub fn add_rectangle(&mut self, rect: &Rectangle) {
        let mut r = *rect;
        let mut i = 0;
        while i < self.rectangles.len() {
            let own = self.rectangles[i];
            let combined_height = (r.y + r.height).max(own.y + own.height) - r.y.min(own.y);
            let combined_width = (r.x + r.width).max(own.x + own.width) - r.x.min(own.x);

            if self.rectangles.len() >= 16
                || combined_width * combined_height <= own.width * own.height + r.width * r.height
            {
                r.x = r.x.min(own.x);
                r.y = r.y.min(own.y);
                r.width = combined_width;
                r.height = combined_height;
                self.rectangles.remove(i);
            } else {
                i += 1;
            }
        }
        self.rectangles.push(r);
    }

    /// `rectangles ()` — itérateur amont → slice.
    #[must_use]
    pub fn rectangles(&self) -> &[Rectangle] {
        &self.rectangles
    }
}

/// `dw::core::Allocation` — position et taille réelles d'un widget.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Allocation {
    /// `x`.
    pub x: i32,
    /// `y`.
    pub y: i32,
    /// `width`.
    pub width: i32,
    /// `ascent`.
    pub ascent: i32,
    /// `descent`.
    pub descent: i32,
}

/// `dw::core::Requisition` — taille demandée par un widget.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Requisition {
    /// `width`.
    pub width: i32,
    /// `ascent`.
    pub ascent: i32,
    /// `descent`.
    pub descent: i32,
}

/// `dw::core::Extremes` — largeurs minimales/maximales.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Extremes {
    /// `minWidth`.
    pub min_width: i32,
    /// `maxWidth`.
    pub max_width: i32,
    /// `minWidthIntrinsic`.
    pub min_width_intrinsic: i32,
    /// `maxWidthIntrinsic`.
    pub max_width_intrinsic: i32,
    /// `adjustmentWidth`.
    pub adjustment_width: i32,
}

/// `dw::core::WidgetReference` — référence à un widget OOF ;
/// `parentRef` = index de référence parent (-1 = absent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetReference {
    /// `widget` (≙ `Widget*`).
    pub widget: WidgetId,
    /// `parentRef`.
    pub parent_ref: i32,
}

impl WidgetReference {
    /// `WidgetReference (widget)` — `parentRef = -1`.
    #[must_use]
    pub fn new(widget: WidgetId) -> Self {
        WidgetReference {
            widget,
            parent_ref: -1,
        }
    }
}

/// `dw::core::Content` — contenu d'un mot/itérateur de
/// textblock. L'union amont (`text|widget|widgetReference|
/// breakSpace`) devient une enum taguée ; `type` garde la
/// sémantique bitmask `short`.
#[derive(Debug, Clone, PartialEq)]
pub struct Content {
    /// `type` — un seul bit de `content_type` positionné.
    pub r#type: i16,
    /// `space` — espace après ce contenu.
    pub space: bool,
    /// Payload (variant active ≡ union amont).
    pub data: ContentData,
}

/// `dw::core::Content::Type` — bits (≙ enum amont `short`).
pub mod content_type {
    /// `START`.
    pub const START: i16 = 1 << 0;
    /// `END`.
    pub const END: i16 = 1 << 1;
    /// `TEXT`.
    pub const TEXT: i16 = 1 << 2;
    /// `WIDGET_IN_FLOW` — `this` = conteneur et générateur.
    pub const WIDGET_IN_FLOW: i16 = 1 << 3;
    /// `WIDGET_OOF_CONT` — `this` = conteneur seulement.
    pub const WIDGET_OOF_CONT: i16 = 1 << 4;
    /// `WIDGET_OOF_REF` — `this` = générateur seulement.
    pub const WIDGET_OOF_REF: i16 = 1 << 5;
    /// `BREAK`.
    pub const BREAK: i16 = 1 << 6;
    /// `INVALID` — interne, jamais exposé.
    pub const INVALID: i16 = 1 << 7;
    /// `ALL`.
    pub const ALL: i16 = 0xff;
    /// `REAL_CONTENT`.
    pub const REAL_CONTENT: i16 = ALL ^ (START | END);
    /// `SELECTION_CONTENT` — `WIDGET_*` s'y ajoute.
    pub const SELECTION_CONTENT: i16 = TEXT | BREAK;
    /// `ANY_WIDGET`.
    pub const ANY_WIDGET: i16 = WIDGET_IN_FLOW | WIDGET_OOF_CONT | WIDGET_OOF_REF;
}

/// Union `Content` amont : `text|widget|widgetReference|
/// breakSpace`.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentData {
    /// `const char *text` — copie possédée (amont pointe dans le
    /// buffer interne du textblock).
    Text(Vec<u8>),
    /// `Widget *widget`.
    Widget(WidgetId),
    /// `WidgetReference *widgetReference`.
    WidgetReference(WidgetReference),
    /// `int breakSpace`.
    BreakSpace(i32),
    /// Variante inerte (≙ union non initialisée pour
    /// `START`/`END`/`INVALID`).
    None,
}

impl Content {
    /// `maskForSelection`.
    #[must_use]
    pub fn mask_for_selection(follow_references: bool) -> i16 {
        let widget_mask = if follow_references {
            content_type::WIDGET_IN_FLOW | content_type::WIDGET_OOF_REF
        } else {
            content_type::WIDGET_IN_FLOW | content_type::WIDGET_OOF_CONT
        };
        content_type::SELECTION_CONTENT | widget_mask
    }

    /// `getWidget` — `widgetReference->widget` pour `OOF_REF`,
    /// `widget` sinon. `None` si le type n'est pas un widget
    /// (amont `assert`).
    #[must_use]
    pub fn get_widget(&self) -> Option<WidgetId> {
        if self.r#type & content_type::ANY_WIDGET == 0 {
            return None;
        }
        if self.r#type == content_type::WIDGET_OOF_REF {
            match self.data {
                ContentData::WidgetReference(r) => Some(r.widget),
                _ => None,
            }
        } else {
            match self.data {
                ContentData::Widget(w) => Some(w),
                _ => None,
            }
        }
    }

    /// `maskIntoStringBuffer` — `st:en:tx:wf:Wr:Wc:br`.
    #[must_use]
    pub fn mask_into_string(mask: i16) -> String {
        let mut s = String::new();
        let flag = |b: i16| mask & b != 0;
        s.push_str(if flag(content_type::START) {
            "st"
        } else {
            "--"
        });
        s.push(':');
        s.push_str(if flag(content_type::END) { "en" } else { "--" });
        s.push(':');
        s.push_str(if flag(content_type::TEXT) { "tx" } else { "--" });
        s.push(':');
        s.push_str(if flag(content_type::WIDGET_IN_FLOW) {
            "wf"
        } else {
            "--"
        });
        s.push(':');
        s.push_str(if flag(content_type::WIDGET_OOF_REF) {
            "Wr"
        } else {
            "--"
        });
        s.push(':');
        s.push_str(if flag(content_type::WIDGET_OOF_CONT) {
            "Wc"
        } else {
            "--"
        });
        s.push(':');
        s.push_str(if flag(content_type::BREAK) {
            "br"
        } else {
            "--"
        });
        s
    }
}

/// `dw::core::StackingProcessingContext` — ensemble des widgets
/// déjà traités en interruption (`HashSet<TypedPointer>` →
/// `HashSet<WidgetId>`).
#[derive(Debug, Clone, Default)]
pub struct StackingProcessingContext {
    /// `widgetsProcessedAsInterruption`.
    widgets_processed_as_interruption: HashSet<WidgetId>,
}

impl StackingProcessingContext {
    /// `StackingProcessingContext ()`.
    #[must_use]
    pub fn new() -> Self {
        StackingProcessingContext::default()
    }

    /// `hasWidgetBeenProcessedAsInterruption`.
    #[must_use]
    pub fn has_widget_been_processed_as_interruption(&self, widget: WidgetId) -> bool {
        self.widgets_processed_as_interruption.contains(&widget)
    }

    /// `addWidgetProcessedAsInterruption`.
    pub fn add_widget_processed_as_interruption(&mut self, widget: WidgetId) {
        self.widgets_processed_as_interruption.insert(widget);
    }
}

/// `dw::core::DrawingContext` — posé au sommet du dessin.
#[derive(Debug, Clone, Default)]
pub struct DrawingContext {
    /// Contexte de stacking hérité.
    pub stacking: StackingProcessingContext,
    /// `toplevelArea`.
    pub toplevel_area: Rectangle,
}

impl DrawingContext {
    /// `DrawingContext (toplevelArea)`.
    #[must_use]
    pub fn new(toplevel_area: Rectangle) -> Self {
        DrawingContext {
            stacking: StackingProcessingContext::new(),
            toplevel_area,
        }
    }
}

/// `dw::core::GettingWidgetAtPointContext`.
#[derive(Debug, Clone, Default)]
pub struct GettingWidgetAtPointContext {
    /// Contexte de stacking hérité.
    pub stacking: StackingProcessingContext,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_intersection() {
        let a = Rectangle::new(0, 0, 10, 10);
        let b = Rectangle::new(5, 5, 10, 10);
        assert_eq!(a.intersects_with(&b), Some(Rectangle::new(5, 5, 5, 5)));
        let c = Rectangle::new(20, 20, 5, 5);
        assert_eq!(a.intersects_with(&c), None);
        // Bords tangents : pas d'intersection amont (strict `<`).
        let d = Rectangle::new(10, 0, 5, 5);
        assert_eq!(a.intersects_with(&d), None);
    }

    #[test]
    fn rectangle_subset_and_point() {
        let outer = Rectangle::new(0, 0, 100, 100);
        let inner = Rectangle::new(10, 10, 20, 20);
        assert!(inner.is_subset_of(&outer));
        assert!(!outer.is_subset_of(&inner));
        assert!(inner.is_point_within(15, 15));
        assert!(!inner.is_point_within(30, 15)); // x < x+w strict
        assert!(!inner.is_point_within(10, 30));
    }

    #[test]
    fn polygon_point_within() {
        let mut p = Polygon::new();
        p.add_point(0, 0);
        p.add_point(10, 0);
        p.add_point(10, 10);
        p.add_point(0, 10);
        // Le rayon (-1,-1)→(7,5) ne traverse qu'une arête → dedans.
        assert!(p.is_point_within(7, 5));
        assert!(!p.is_point_within(15, 5));
        // Quirk amont : un rayon passant par le sommet (0,0) compte
        // ce sommet deux fois (arêtes adjacentes) → pair → dehors.
        assert!(!p.is_point_within(5, 5));
        // < 3 points → false
        let mut tri = Polygon::new();
        tri.add_point(0, 0);
        tri.add_point(10, 0);
        assert!(!tri.is_point_within(5, 0));
    }

    #[test]
    fn region_merges_overlapping() {
        let mut r = Region::new();
        r.add_rectangle(&Rectangle::new(0, 0, 10, 10));
        // (0,0,10,10) + (2,2,4,4) : union 100 <= 116 → fusionne
        r.add_rectangle(&Rectangle::new(2, 2, 4, 4));
        assert_eq!(r.rectangles().len(), 1);
        assert_eq!(r.rectangles()[0], Rectangle::new(0, 0, 10, 10));
        r.clear();
        r.add_rectangle(&Rectangle::new(0, 0, 10, 10));
        // (5,5,10,10) : union 15×15=225 > 100+100 → pas de fusion
        r.add_rectangle(&Rectangle::new(5, 5, 10, 10));
        assert_eq!(r.rectangles().len(), 2);
        // Disjoint évidemment : pas de fusion non plus
        r.clear();
        r.add_rectangle(&Rectangle::new(0, 0, 10, 10));
        r.add_rectangle(&Rectangle::new(50, 50, 10, 10));
        assert_eq!(r.rectangles().len(), 2);
    }

    #[test]
    fn region_forces_merge_at_16() {
        let mut r = Region::new();
        // 16 rect disjoints puis un 17e → fusion forcée
        for i in 0..16 {
            r.add_rectangle(&Rectangle::new(i * 20, 0, 10, 10));
        }
        assert_eq!(r.rectangles().len(), 16);
        r.add_rectangle(&Rectangle::new(400, 0, 10, 10));
        // Le 17e rectangle fusionne tout par engulfing → reste ≤ 16
        assert!(r.rectangles().len() <= 16);
    }

    #[test]
    fn content_mask_and_get_widget() {
        use content_type::*;
        assert_eq!(
            Content::mask_for_selection(false),
            TEXT | BREAK | WIDGET_IN_FLOW | WIDGET_OOF_CONT
        );
        assert_eq!(
            Content::mask_for_selection(true),
            TEXT | BREAK | WIDGET_IN_FLOW | WIDGET_OOF_REF
        );

        let mut c = Content {
            r#type: WIDGET_OOF_REF,
            space: false,
            data: ContentData::WidgetReference(WidgetReference::new(42)),
        };
        assert_eq!(c.get_widget(), Some(42));
        c.r#type = WIDGET_IN_FLOW;
        c.data = ContentData::Widget(7);
        assert_eq!(c.get_widget(), Some(7));
        c.r#type = TEXT;
        c.data = ContentData::Text(b"hi".to_vec());
        assert_eq!(c.get_widget(), None);
    }

    #[test]
    fn mask_string_verbatim() {
        use content_type::*;
        assert_eq!(
            Content::mask_into_string(TEXT | WIDGET_IN_FLOW),
            "--:--:tx:wf:--:--:--"
        );
        assert_eq!(Content::mask_into_string(ALL), "st:en:tx:wf:Wr:Wc:br");
    }

    #[test]
    fn stacking_context_tracks_processed() {
        let mut sc = StackingProcessingContext::new();
        assert!(!sc.has_widget_been_processed_as_interruption(3));
        sc.add_widget_processed_as_interruption(3);
        assert!(sc.has_widget_been_processed_as_interruption(3));
        assert!(!sc.has_widget_been_processed_as_interruption(4));
    }
}
