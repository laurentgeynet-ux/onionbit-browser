//! Port de `dw/view.hh` — trait `dw::core::View` complet
//! (dessin + scrolling + clipping, plateforme-agnostique).
//!
//! Différences de modèle (documentées) :
//! - `setLayout(Layout*)` → `set_layout(Option<LayoutPtr>)` où
//!   `LayoutPtr` est un handle opaque (`usize`) — la vue n'a pas
//!   besoin de l'objet `Layout` en modèle D, seulement de pouvoir
//!   lui renvoyer `expose`/`buttonPress`/`scrollPosChanged`/`viewport
//!   SizeChanged` via la plateforme.
//! - `drawImage(Imgbuf*, ...)` → `Rc<dyn Imgbuf>` comme partout.
//! - `getClippingView` retourne `Box<dyn View>` (la vue clippée
//!   appartient à l'appelant, ≙ `mergeClippingView` qui la rend).
//! - Les `style::Color*`/`style::Font*` C++ deviennent `&Color`/`&Font`
//!   (les ressources vivent dans les pools du layout).

use crate::imgbuf::Imgbuf;
use crate::style::{Color, Cursor, Font, LineType, Shading};
use crate::types::{Point, Rectangle, ScrollCommand};
use std::rc::Rc;

/// `dw::core::View` — interface de dessin complète (view.hh).
/// Signatures verbatim amont → `too_many_arguments` toléré.
#[allow(clippy::too_many_arguments)]
pub trait View {
    // --------------------------------------------------
    //    Operations on the view
    // --------------------------------------------------

    /// `setLayout` — notification d'attachement (`None` =
    /// détachement). Handle opaque : la vue rappelle le layout
    /// via `platform`/`dw-fltk` (`dw-fltkviewbase` amont tient un
    /// pointeur — le handle opaque est un `usize` ici car `Layout`
    /// n'a pas d'adresse stable exposée en safe Rust).
    fn set_layout(&mut self, layout: Option<usize>);

    /// `setCanvasSize` — taille du monde (ascent+descent = hauteur).
    fn set_canvas_size(&mut self, width: i32, ascent: i32, descent: i32);

    /// `setCursor`.
    fn set_cursor(&mut self, cursor: Cursor);

    /// `setBgColor`.
    fn set_bg_color(&mut self, color: &Color);

    // --------------------------------------------------
    //    Scrolling — si `usesViewport` == false, les autres
    //    méthodes de cette section ne sont jamais appelées
    //    (verbatim view.hh:54).
    // --------------------------------------------------

    /// `usesViewport`.
    fn uses_viewport(&mut self) -> bool;

    /// `getHScrollbarThickness` — épaisseur barre horizontale
    /// quand elle est visible.
    fn get_h_scrollbar_thickness(&mut self) -> i32;

    /// `getVScrollbarThickness`.
    fn get_v_scrollbar_thickness(&mut self) -> i32;

    /// `getScrollbarOnLeft` — booléen amont `int`.
    fn get_scrollbar_on_left(&mut self) -> i32;

    /// `scrollTo (x, y)`.
    fn scroll_to(&mut self, x: i32, y: i32);

    /// `scroll (ScrollCommand)` — impl. vide par défaut amont.
    fn scroll(&mut self, _cmd: ScrollCommand) {}

    /// `setViewportSize (w, h, hThick, vThick)` — w/h incluent
    /// les épaisseurs de barres.
    fn set_viewport_size(
        &mut self,
        width: i32,
        height: i32,
        h_scrollbar_thickness: i32,
        v_scrollbar_thickness: i32,
    );

    // --------------------------------------------------
    //    Drawing
    // --------------------------------------------------

    /// `startDrawing` — borne avant une passe de dessin
    /// (backing pixmap amont).
    fn start_drawing(&mut self, area: &Rectangle);

    /// `finishDrawing`.
    fn finish_drawing(&mut self, area: &Rectangle);

    /// `queueDraw` — enfile une région (coordonnées *canvas*)
    /// pour un dessin ultérieur.
    fn queue_draw(&mut self, area: &Rectangle);

    /// `queueDrawTotal`.
    fn queue_draw_total(&mut self);

    /// `cancelQueueDraw` — annule la file si pas encore dessinée.
    fn cancel_queue_draw(&mut self);

    /// `drawPoint (color, shading, x, y)`.
    fn draw_point(&mut self, color: &Color, shading: Shading, x: i32, y: i32);

    /// `drawLine (color, shading, x1, y1, x2, y2)`.
    fn draw_line(&mut self, color: &Color, shading: Shading, x1: i32, y1: i32, x2: i32, y2: i32);

    /// `drawTypedLine (color, shading, type, width, x1,y1,x2,y2)`.
    fn draw_typed_line(
        &mut self,
        color: &Color,
        shading: Shading,
        line_type: LineType,
        width: i32,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    );

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

    /// `drawArc (color, shading, filled, centerX, centerY, w, h,
    /// angle1, angle2)` — angles en 1/64°.
    fn draw_arc(
        &mut self,
        color: &Color,
        shading: Shading,
        filled: bool,
        center_x: i32,
        center_y: i32,
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

    /// `drawText (font, color, shading, x, y, text, len)`.
    fn draw_text(
        &mut self,
        font: &Font,
        color: &Color,
        shading: Shading,
        x: i32,
        y: i32,
        text: &[u8],
    );

    /// `drawSimpleWrappedText (font, color, shading, x,y,w,h, text)`.
    fn draw_simple_wrapped_text(
        &mut self,
        font: &Font,
        color: &Color,
        shading: Shading,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        text: &[u8],
    );

    /// `drawImage (imgbuf, xRoot, yRoot, x, y, w, h)` — `xRoot`/
    /// `yRoot` = origine de l'image root dans le canvas, `(x,y)`
    /// = offset dans l'image, `(w,h)` = taille du blit.
    fn draw_image(
        &mut self,
        imgbuf: &Rc<dyn Imgbuf>,
        x_root: i32,
        y_root: i32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    );

    // --------------------------------------------------
    //    Clipping — vues imbriquées, fusionnées à la demande
    // --------------------------------------------------

    /// `getClippingView (x, y, w, h)` — sous-vue clippée.
    fn get_clipping_view(&mut self, x: i32, y: i32, width: i32, height: i32) -> Box<dyn View>;

    /// `mergeClippingView` — réinjecte la sous-vue dans le parent.
    fn merge_clipping_view(&mut self, clipping_view: Box<dyn View>);

    /// Downcast plateforme — ≙ les casts `FltkView*` amont
    /// (`FltkResource::draw` caste `View*` → `FltkView*`). Les
    /// impls concrètes retournent `Some(self)` ; `None` défaut.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }
}
