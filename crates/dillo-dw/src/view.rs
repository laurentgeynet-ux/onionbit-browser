//! Port partiel de `dw/view.hh` — l'interface `core::View`
//! (dessin platform-agnostique).
//!
//! Ce trait ne contient pour l'instant que les primitives
//! utilisées par `types.rs` (`Shape::draw`) :
//! `drawRectangle`/`drawArc`/`drawPolygon`. Le reste de
//! l'interface (`setLayout`, scrolling, `drawText`,
//! `drawImage`, curseurs, vues texte…) arrive avec le lot
//! `view.hh`/`layout.cc` complet.

use crate::style::{Color, Shading};
use crate::types::Point;

/// `dw::core::View` — interface de dessin (extraits).
/// Signatures verbatim amont → `too_many_arguments` toléré.
#[allow(clippy::too_many_arguments)]
pub trait View {
    /// `drawRectangle (color, shading, filled, x, y, w, h)`.
    fn draw_rectangle(
        &mut self,
        color: &Color,
        shading: Shading,
        filled: bool,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    );

    /// `drawArc (color, shading, filled, x, y, w, h, angle1,
    /// angle2)` — angles en 1/64° comme amont.
    fn draw_arc(
        &mut self,
        color: &Color,
        shading: Shading,
        filled: bool,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        angle1: i32,
        angle2: i32,
    );

    /// `drawPolygon (color, shading, filled, convex, points)`.
    fn draw_polygon(
        &mut self,
        color: &Color,
        shading: Shading,
        filled: bool,
        convex: bool,
        points: &[Point],
    );
}
