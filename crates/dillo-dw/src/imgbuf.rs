//! Port de `dw/imgbuf.hh` — interface platform-agnostique des
//! buffers d'image.
//!
//! Le `Imgbuf*` amont devient `Rc<dyn Imgbuf>` ; `ref`/`unref`
//! restent dans le trait (les impls ont des sémantiques
//! spécifiques — `lastReference`, `deleteOnUnref`, buffers
//! fils scalés gardant la racine vivante).
//!
//! L'implémentation concrète vit dans `dillo-images::imgbuf`
//! (≙ `dw::fltk::FltkImgbuf` côté amont).

use std::rc::Rc;

use crate::types::Rectangle;

/// `dw::core::Imgbuf::Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ImgbufType {
    /// `RGB` — 3 octets/px.
    Rgb,
    /// `RGBA` — 4 octets/px.
    Rgba,
    /// `GRAY` — 1 octet/px.
    Gray,
    /// `INDEXED` — 1 octet/px + colormap `0xrrbbgg`.
    Indexed,
    /// `INDEXED_ALPHA` — 1 octet/px + colormap `0xaarrggbb`.
    IndexedAlpha,
}

/// `dw::core::Imgbuf` — interface buffer d'image.
pub trait Imgbuf {
    /*
     * Appelés depuis le décodage d'image
     */

    /// `setCMap (colors, num_colors)`.
    fn set_cmap(&mut self, colors: &[i32]);
    /// `copyRow (row, data)`.
    fn copy_row(&mut self, row: i32, data: &[u8]);
    /// `newScan` — nouveau passage (progressif/interlacé).
    fn new_scan(&mut self);

    /*
     * Appelés depuis dw::Image
     */

    /// `getScaledBuf (width, height)` — buffer scalé (ou le
    /// root si même taille).
    fn get_scaled_buf(&self, width: i32, height: i32) -> Rc<dyn Imgbuf>;
    /// `getRowArea (row, area)` — aire canvas couverte par `row`.
    fn get_row_area(&self, row: i32, area: &mut Rectangle);
    /// `getRootWidth`.
    fn get_root_width(&self) -> i32;
    /// `getRootHeight`.
    fn get_root_height(&self) -> i32;

    /// `createSimilarBuf (width, height)` — mêmes paramètres
    /// (type, gamma), autre taille.
    fn create_similar_buf(&self, width: i32, height: i32) -> Rc<dyn Imgbuf>;

    /// `copyTo (dest, xDestRoot, yDestRoot, xSrc, ySrc, w, h)`.
    #[allow(clippy::too_many_arguments)]
    fn copy_to(
        &mut self,
        dest: &mut dyn Imgbuf,
        x_dest_root: i32,
        y_dest_root: i32,
        x_src: i32,
        y_src: i32,
        width_src: i32,
        height_src: i32,
    );

    /*
     * Comptage de références (sémantiques amont, voir doc du
     * header : buffers fils gardant la racine vivante)
     */

    /// `ref`.
    fn ref_(&mut self);
    /// `unref`.
    fn unref(&mut self);
    /// `lastReference`.
    fn last_reference(&self) -> bool;
    /// `setDeleteOnUnref`.
    fn set_delete_on_unref(&mut self, delete_on_unref: bool);
    /// `isReferred`.
    fn is_referred(&self) -> bool;
}
