//! Port de `dw/imgbuf.hh` — interface platform-agnostique des
//! buffers d'image.
//!
//! Le `Imgbuf*` amont devient `Rc<dyn Imgbuf>` ; `ref`/`unref`
//! restent dans le trait (les impls ont des sémantiques
//! spécifiques — `lastReference`, `deleteOnUnref`, buffers
//! fils scalés gardant la racine vivante).
//!
//! L'implémentation concrète vit dans `dillo-images::imgbuf`
//! (≙ `dw::fltk::FltkImgbuf` côté amont — la création via
//! `Platform::createImgbuf` arrive avec `dw/platform.hh`).

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
///
/// `rgb_row`/`write_rgb_span` sont les deux accesseurs bas niveau
/// permettant à [`Imgbuf::copy_to`] d'être une méthode par défaut
/// (≙ l'accès direct `rawdata` amont, qui supposait les deux
/// buffers de la même classe — `assert(bpp == fDest->bpp)`).
pub trait Imgbuf {
    /*
     * Appelés depuis le décodage d'image
     */

    /// `setCMap (colors, num_colors)` — receveur `&self` :
    /// amont appelle ces méthodes via `Imgbuf*` partagés ; les
    /// impls utilisent la mutabilité intérieure.
    fn set_cmap(&self, colors: &[i32]);
    /// `copyRow (row, data)` — `row` en coordonnées ROOT.
    fn copy_row(&self, row: i32, data: &[u8]);
    /// `newScan` — nouveau passage (progressif/interlacé).
    fn new_scan(&self);

    /*
     * Appelés depuis dw::Image
     */

    /// `getScaledBuf (width, height)` — buffer scalé (ou le
    /// root si même taille). Receveur `Rc` ≙ `ref()` + retour
    /// `this` amont.
    fn get_scaled_buf(self: Rc<Self>, width: i32, height: i32) -> Rc<dyn Imgbuf>;
    /// `getRowArea (row, area)` — aire canvas couverte par `row`
    /// (root : `(0,row,w,1)` ; scalé : bande `scaledY`).
    fn get_row_area(&self, row: i32, area: &mut Rectangle);
    /// `getRootWidth`.
    fn get_root_width(&self) -> i32;
    /// `getRootHeight`.
    fn get_root_height(&self) -> i32;

    /// Largeur propre du buffer (≠ root pour un buffer scalé —
    /// ≙ `FltkImgbuf::width`, utilisé par `copyTo` amont qui
    /// accède aux champs de la même classe).
    fn width(&self) -> i32;
    /// Hauteur propre du buffer.
    fn height(&self) -> i32;

    /// `createSimilarBuf (width, height)` — mêmes paramètres
    /// (type, gamma), autre taille, nouvelle racine.
    fn create_similar_buf(&self, width: i32, height: i32) -> Rc<dyn Imgbuf>;

    /// Ligne RGB `y` du buffer (`3*width` octets, coordonnées
    /// du buffer — ≙ `rawdata + y*width*bpp` amont).
    fn rgb_row(&self, y: i32) -> Vec<u8>;

    /// Écrit `data` à la position `(x, y)` du buffer (≙ écriture
    /// directe `rawdata` — hors limites : muet comme amont via
    /// clipping de `copyTo`).
    fn write_rgb_span(&self, x: i32, y: i32, data: &[u8]);

    /// `copyTo (dest, xDestRoot, yDestRoot, xSrc, ySrc, w, h)` —
    /// verbatim : clip `xSrc2/ySrc2` contre `dest` puis blit
    /// pixel-à-pixel (`bpp` identique = 3). Casts bornés par le
    /// clip ci-dessus (`x_src2 > x_src ≥ 0` garanti).
    #[allow(clippy::too_many_arguments, clippy::cast_sign_loss)]
    fn copy_to(
        &self,
        dest: &dyn Imgbuf,
        x_dest_root: i32,
        y_dest_root: i32,
        x_src: i32,
        y_src: i32,
        width_src: i32,
        height_src: i32,
    ) {
        // `xSrc2 = min(xSrc + widthSrc, fDest->width - xDestRoot)`
        let x_src2 = (x_src + width_src).min(dest.width() - x_dest_root);
        let y_src2 = (y_src + height_src).min(dest.height() - y_dest_root);

        if x_src2 <= x_src {
            return;
        }
        for y in y_src..y_src2 {
            let row = self.rgb_row(y);
            let lo = (3 * x_src) as usize;
            let hi = (3 * x_src2) as usize;
            if hi <= row.len() {
                dest.write_rgb_span(x_dest_root + x_src, y_dest_root + y, &row[lo..hi]);
            }
        }
    }

    /*
     * Comptage de références (sémantiques amont, voir doc du
     * header : buffers fils gardant la racine vivante)
     */

    /// `ref`.
    fn ref_(&self);
    /// `unref`.
    fn unref(&self);
    /// `lastReference`.
    fn last_reference(&self) -> bool;
    /// `setDeleteOnUnref`.
    fn set_delete_on_unref(&self, delete_on_unref: bool);
    /// `isReferred`.
    fn is_referred(&self) -> bool;
}
