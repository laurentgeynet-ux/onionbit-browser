//! Port de `src/imgbuf.cc` + `imgbuf.hh` + la face buffer de
//! `dw/imgbuf.hh`/`fltkimgbuf` — buffer RGB racine d'une image.
//!
//! Amont `Imgbuf` (`dw::core::Imgbuf`/`fltkimgbuf`) est un buffer
//! `3*w*h` RGB avec `copyRow`/`newScan` et refcount manuel partagé entre
//! l'entrée dicache et le widget `dw::Image`. Ici :
//! [`Imgbuf`] = buffer + `Rc` (le refcount amont ≙ `Rc::strong_count` —
//! `last_reference` ≙ `Rc::strong_count(imgbuf) == 1`), `gamma` gardée
//! pour le backend de rendu (phase platform).
//!
//! `Imgbuf_rgb_line` est porté verbatim — y compris le traitement
//! `CMYK_INV` « RGBW » (`c*white/0x100`, quirk Adobe amont) et le buffer
//! de ligne partagé `linebuf` (ici : réallocé par appel — taille bornée
//! par `3*width`, même borne qu'amont).
//!
//! Face `dw::core::Imgbuf` : ce buffer implémente le trait
//! [`dillo_dw::imgbuf::Imgbuf`] (≙ `FltkImgbuf`) — arbre de buffers
//! scalés (`scaleMode == SIMPLE` ; le défaut amont
//! `BEAUTIFUL_GAMMA` est une déviation documentée, correction
//! gamma à venir avec la couche platform), `copiedRows` par
//! buffer, `setCMap` no-op verbatim, rawdata init `222` verbatim.
//! `root` est un `Rc` fort (≙ le jeu de refcounts amont gardant
//! la racine vivante tant qu'un fils existe — même durée de vie
//! observable).

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use dillo_common::bitvec::Bitvec;
use dillo_dw::imgbuf::Imgbuf as DwImgbuf;
use dillo_dw::types::Rectangle;

/// `Imgbuf::RGB` — seul type utilisé amont (`I_RGB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImgbufType {
    /// RGB 3 octets/pixel.
    Rgb,
}

/// `dw::core::Imgbuf` (face buffer de `fltkimgbuf`) — données RGB
/// organisées en lignes `3*width`.
#[derive(Debug)]
pub struct Imgbuf {
    /// Type (`Imgbuf::RGB` amont — seul utilisé).
    pub buf_type: ImgbufType,
    /// Dimensions.
    pub width: u32,
    /// `height`.
    pub height: u32,
    /// `gamma` (passée au backend — pas de correction dans le buffer).
    pub gamma: f64,
    /// Données RGB (`root buffer` amont) — `3*width*height`. Partagé
    /// entre entrée dicache et widget via `Rc` → `RefCell` (mutations
    /// depuis le décodeur, thread principal).
    data: RefCell<Vec<u8>>,
    /// `root` — `Some` pour un buffer scalé (`Rc` fort ≙ refcounts
    /// amont gardant la racine vivante).
    root: Option<Rc<Imgbuf>>,
    /// `scaledBuffers` — buffers fils (≙ `List<FltkImgbuf>` ; les
    /// `Weak` morts sont purgés — ≙ `detachScaledBuf`).
    children: RefCell<Vec<Weak<Imgbuf>>>,
    /// `copiedRows` — lignes déjà copiées (taille = `height`
    /// propre, comme amont).
    copied_rows: RefCell<Bitvec>,
    /// `refCount` amont (`ref`/`unref` manuels côté trait —
    /// les clients `Rc` n'y touchent pas).
    ref_count: Cell<i64>,
    /// `deleteOnUnref` amont (init `true`).
    delete_on_unref: Cell<bool>,
}

impl Imgbuf {
    /// `createImgbuf(RGB, w, h, gamma)` → `FltkImgbuf::init` — buffer
    /// zéroé. Cascade amont dans l'ordre : dimensions excessives
    /// (`w<=0 || h<=0 || w > IMAGE_MAX_AREA/h`) → 1×1, `w>0x8000` →
    /// `MAX_WIDTH`, `h>0x8000` → `MAX_HEIGHT`, `gamma<=0` → 1
    /// (récursion amont ≙ boucle ici — les clamps restent idempotents).
    #[must_use]
    pub fn new(buf_type: ImgbufType, mut width: u32, mut height: u32, mut gamma: f64) -> Rc<Self> {
        /// `MAX_WIDTH`/`MAX_HEIGHT` (fltkimgbuf.cc).
        const MAX_DIM: u32 = 0x8000;
        /// `IMAGE_MAX_AREA`.
        const MAX_AREA: u64 = 6000 * 6000;
        loop {
            if width == 0 || height == 0 || u64::from(width) > MAX_AREA / u64::from(height) {
                width = 1;
                height = 1;
            } else if width > MAX_DIM {
                width = MAX_DIM;
            } else if height > MAX_DIM {
                height = MAX_DIM;
            } else if gamma <= 0.0 {
                gamma = 1.0;
            } else {
                break;
            }
        }
        Rc::new(Self::with_root(buf_type, width, height, gamma, None))
    }

    /// `init` amont — construit le buffer (racine ou fils).
    fn with_root(
        buf_type: ImgbufType,
        width: u32,
        height: u32,
        gamma: f64,
        root: Option<Rc<Imgbuf>>,
    ) -> Self {
        Imgbuf {
            buf_type,
            width,
            height,
            gamma,
            // `memset(rawdata, 222, ...)` — gris clair pendant le
            // chargement progressif (quirk amont).
            data: RefCell::new(vec![222; (3 * width * height) as usize]),
            root,
            children: RefCell::new(Vec::new()),
            copied_rows: RefCell::new(Bitvec::new(height as usize)),
            ref_count: Cell::new(1),
            delete_on_unref: Cell::new(true),
        }
    }

    /// `copyRow` — copie `3*width` octets décodés dans la ligne `y`
    /// (`dReturn_if_fail(y < height)` amont → muet), marque
    /// `copiedRows[y]` et propage `scaleRow` aux buffers scalés
    /// (appelée sur le ROOT amont — sur un fils, délègue à root).
    pub fn copy_row(&self, y: u32, buf: &[u8]) {
        if let Some(root) = &self.root {
            root.copy_row(y, buf);
            return;
        }
        if y >= self.height {
            return;
        }
        let w = (3 * self.width) as usize;
        let off = y as usize * w;
        let n = w.min(buf.len()).min(self.data.borrow().len() - off);
        self.data.borrow_mut()[off..off + n].copy_from_slice(&buf[..n]);
        self.copied_rows.borrow_mut().set_bit(y as usize);
        // Update all the scaled buffers of this root image.
        for child in self.live_children() {
            child.scale_row(y, buf);
        }
    }

    /// `scaledBuffers` vivants (purge ≙ `detachScaledBuf`).
    fn live_children(&self) -> Vec<Rc<Imgbuf>> {
        let mut children = self.children.borrow_mut();
        children.retain(|w| w.strong_count() > 0);
        children.iter().filter_map(Weak::upgrade).collect()
    }

    /// `scaleRow` — `row` en coordonnées ROOT (`scaleMode` amont =
    /// `BEAUTIFUL_GAMMA` ; ici `SIMPLE` — déviation documentée :
    /// pixels scalés non gamma-corrigés).
    fn scale_row(&self, row: u32, data: &[u8]) {
        let Some(root) = &self.root else {
            return;
        };
        if row >= root.height {
            return;
        }
        self.scale_row_simple(row, data);
    }

    /// `scaledY` — `ySrc * height / root->height`.
    fn scaled_y(&self, y_src: u32) -> u32 {
        let root_h = self.root.as_ref().map_or(self.height, |r| r.height);
        y_src * self.height / root_h
    }

    /// `scaleRowSimple` — nearest-neighbor verbatim (sr1
    /// resamplé horizontalement, sr>sr1 copie la ligne sr1,
    /// `copiedRows` anti-multi-passes).
    fn scale_row_simple(&self, row: u32, data: &[u8]) {
        let Some(root) = &self.root else {
            return;
        };
        let sr1 = self.scaled_y(row);
        let sr2 = self.scaled_y(row + 1);
        for sr in sr1..sr2 {
            if sr >= self.height {
                break;
            }
            // Avoid multiple passes.
            if self.copied_rows.borrow().get_bit(sr as usize) != 0 {
                continue;
            }
            self.copied_rows.borrow_mut().set_bit(sr as usize);
            let w = (3 * self.width) as usize;
            if sr == sr1 {
                let mut data_b = self.data.borrow_mut();
                for px in 0..root.width {
                    let px1 = px * self.width / root.width;
                    let px2 = (px + 1) * self.width / root.width;
                    for sp in px1..px2 {
                        let dst = (sr * self.width + sp) as usize * 3;
                        let src = (3 * px) as usize;
                        if dst + 2 < data_b.len() && src + 2 < data.len() {
                            data_b[dst..dst + 3].copy_from_slice(&data[src..src + 3]);
                        }
                    }
                }
            } else {
                // memcpy ligne sr1 → sr
                let (lo, hi) = (
                    (sr1 * self.width) as usize * 3,
                    (sr * self.width) as usize * 3,
                );
                let mut data_b = self.data.borrow_mut();
                if hi + w <= data_b.len() && lo + w <= data_b.len() {
                    data_b.copy_within(lo..lo + w, hi);
                }
            }
        }
    }

    /// `newScan` — reset pour un nouveau scan (multi-scan/interlacé).
    /// Amont efface `copiedRows` des buffers SCALÉS seulement —
    /// pas celui du root (quirk verbatim).
    pub fn new_scan(&self) {
        if self.root.is_none() {
            for child in self.live_children() {
                child.copied_rows.borrow_mut().clear();
            }
        }
    }

    /// `lastReference` — `Rc::strong_count == 1` ≙ dernier détenteur.
    #[must_use]
    pub fn last_reference(buf: &Rc<Self>) -> bool {
        Rc::strong_count(buf) == 1
    }

    /// Données RGB complètes (pour le backend de rendu).
    #[must_use]
    pub fn data(&self) -> std::cell::Ref<'_, Vec<u8>> {
        self.data.borrow()
    }

    /// Ligne `y` du buffer RGB (`y >= height` → vide).
    #[must_use]
    pub fn row_copy(&self, y: u32) -> Vec<u8> {
        if y >= self.height {
            return Vec::new();
        }
        let w = (3 * self.width) as usize;
        let off = y as usize * w;
        self.data.borrow()[off..off + w].to_vec()
    }
}

/// `linebuf` amont — buffer de ligne partagé, grossi à `3*width` max
/// (`dRealloc`). Ici : alloué par appel à la bonne taille — mêmes
/// octets produits, sans état global mutable.
#[must_use]
pub fn rgb_line(buf: &[u8], img_type: ImgType, cmap: Option<&[u8]>, width: u32) -> Vec<u8> {
    let w = width as usize;
    let mut linebuf = vec![0u8; 3 * w];
    match img_type {
        ImgType::Indexed => {
            if let Some(cmap) = cmap {
                for x in 0..w {
                    let i = buf.get(x).copied().unwrap_or(0) as usize;
                    let src = 3 * i;
                    if src + 2 < cmap.len() {
                        linebuf[3 * x..3 * x + 3].copy_from_slice(&cmap[src..src + 3]);
                    }
                }
            }
            // else: MSG_WARN "image lacks a color map" — ligne noire.
        }
        ImgType::Gray => {
            for x in 0..w {
                linebuf[3 * x..3 * x + 3].fill(buf.get(x).copied().unwrap_or(0));
            }
        }
        ImgType::CmykInv => {
            // CMYK traité comme « RGBW » (quirk amont — voir imgbuf.cc).
            for x in 0..w {
                let white = u32::from(buf.get(4 * x + 3).copied().unwrap_or(0));
                for c in 0..3 {
                    // `v * white / 0x100` ≤ 254 — pas de troncature.
                    let v = u32::from(buf.get(4 * x + c).copied().unwrap_or(0)) * white / 0x100;
                    linebuf[3 * x + c] = u8::try_from(v).unwrap_or(0);
                }
            }
        }
        ImgType::Rgb => {
            // Pas de conversion — `return (uchar_t*)buf` amont.
            let n = (3 * w).min(buf.len());
            linebuf[..n].copy_from_slice(&buf[..n]);
        }
        ImgType::NotSet => {
            // MSG_ERR amont — ligne noire.
        }
    }
    linebuf
}

/// `DilloImgType` (image.hh) — type des données brutes fournies par le
/// décodeur (avant conversion RGB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImgType {
    /// `DILLO_IMG_TYPE_INDEXED` — indices dans `cmap`.
    Indexed,
    /// `DILLO_IMG_TYPE_RGB` — triplets RGB directs.
    Rgb,
    /// `DILLO_IMG_TYPE_GRAY` — un octet par pixel.
    Gray,
    /// `DILLO_IMG_TYPE_CMYK_INV` — 4 octets « RGBW ».
    CmykInv,
    /// `DILLO_IMG_TYPE_NOTSET` — valeur initiale.
    NotSet,
}

impl ImgType {
    /// `dReturn_if_fail(y < height)` — ligne valide.
    #[must_use]
    pub const fn is_set(self) -> bool {
        !matches!(self, Self::NotSet)
    }
}

/// Réexport pour les appelants qui décodent via `Bitvec` de lignes.
pub type RowBits = Bitvec;

/// `excessiveImageDimensions` — `w<=0 || h<=0 || w > MAX/h`.
fn excessive_image_dimensions(width: u32, height: u32) -> bool {
    const MAX_AREA: u64 = 6000 * 6000;
    width == 0 || height == 0 || u64::from(width) > MAX_AREA / u64::from(height)
}

/// Face `dw::core::Imgbuf` (≙ `FltkImgbuf`). `scaleMode` amont =
/// `BEAUTIFUL_GAMMA` — ici `SIMPLE` (déviation documentée :
/// pixels scalés non gamma-corrigés ; `BEAUTIFUL` arrivera avec
/// `fltkimgbuf` complet en phase platform).
/// Casts u32↔i32 bornés par `MAX_DIM` (0x8000).
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
impl DwImgbuf for Imgbuf {
    /// `setCMap` — **vide amont** pour `FltkImgbuf` (RGB only — la
    /// cmap est appliquée dans `Imgbuf_rgb_line` côté dicache).
    fn set_cmap(&mut self, _colors: &[i32]) {}

    /// `copyRow` (≙ `assert(isRoot)` — sur un fils, délègue).
    fn copy_row(&mut self, row: i32, data: &[u8]) {
        if row >= 0 {
            Imgbuf::copy_row(self, row as u32, data);
        }
    }

    /// `newScan`.
    fn new_scan(&mut self) {
        Imgbuf::new_scan(self);
    }

    /// `getScaledBuf` — verbatim (clamp `>0x8000`, same-size →
    /// `this`, match enfant existant, `excessive` → `this`,
    /// sinon nouveau fils scalé depuis les lignes déjà copiées).
    #[allow(clippy::cast_sign_loss)]
    fn get_scaled_buf(self: Rc<Self>, width: i32, height: i32) -> Rc<dyn DwImgbuf> {
        /// `MAX_WIDTH`/`MAX_HEIGHT` (fltkimgbuf.cc).
        const MAX_DIM: i32 = 0x8000;
        if let Some(root) = &self.root {
            return Rc::clone(root).get_scaled_buf(width, height);
        }
        if width > MAX_DIM {
            return self.get_scaled_buf(MAX_DIM, height);
        }
        if height > MAX_DIM {
            return self.get_scaled_buf(width, MAX_DIM);
        }
        if width == self.width as i32 && height == self.height as i32 {
            return self;
        }
        for child in self.live_children() {
            if child.width == width as u32 && child.height == height as u32 {
                return child;
            }
        }
        if width <= 0 || height <= 0 || excessive_image_dimensions(width as u32, height as u32) {
            // Suspicious size → unscaled (verbatim).
            return self;
        }
        // Nouveau buffer fils — scalé depuis les lignes déjà copiées.
        let child = Rc::new(Imgbuf::with_root(
            self.buf_type,
            width as u32,
            height as u32,
            self.gamma,
            Some(Rc::clone(&self)),
        ));
        for row in 0..self.height {
            if self.copied_rows.borrow().get_bit(row as usize) != 0 {
                let row_data = self.row_copy(row);
                child.scale_row(row, &row_data);
            }
        }
        self.children.borrow_mut().push(Rc::downgrade(&child));
        child
    }

    /// `getRowArea` — root `(0,row,w,1)` ; fils : `row >
    /// root->height` (strict `>` verbatim) → zéros, sinon bande
    /// `scaledY`.
    #[allow(clippy::cast_sign_loss)]
    fn get_row_area(&self, row: i32, area: &mut Rectangle) {
        match &self.root {
            None => {
                area.x = 0;
                area.y = row;
                area.width = self.width as i32;
                area.height = 1;
            }
            Some(root) => {
                if row > root.height as i32 || row < 0 {
                    area.x = 0;
                    area.y = 0;
                    area.width = 0;
                    area.height = 0;
                } else {
                    let sr1 = self.scaled_y(row as u32) as i32;
                    let sr2 = self.scaled_y((row + 1) as u32) as i32;
                    area.x = 0;
                    area.y = sr1;
                    area.width = self.width as i32;
                    area.height = sr2 - sr1;
                }
            }
        }
    }

    /// `getRootWidth` — `root ? root->width : width`.
    fn get_root_width(&self) -> i32 {
        self.root.as_ref().map_or(self.width, |r| r.width) as i32
    }

    /// `getRootHeight`.
    fn get_root_height(&self) -> i32 {
        self.root.as_ref().map_or(self.height, |r| r.height) as i32
    }

    /// `width` propre (≙ champ `FltkImgbuf::width`).
    fn width(&self) -> i32 {
        self.width as i32
    }

    /// `height` propre.
    fn height(&self) -> i32 {
        self.height as i32
    }

    /// `createSimilarBuf` — nouvelle racine, mêmes type/gamma.
    #[allow(clippy::cast_sign_loss)]
    fn create_similar_buf(&self, width: i32, height: i32) -> Rc<dyn DwImgbuf> {
        Rc::new(Imgbuf::with_root(
            self.buf_type,
            width.max(1) as u32,
            height.max(1) as u32,
            self.gamma,
            None,
        ))
    }

    /// `rawdata + y*width*bpp` amont.
    fn rgb_row(&self, y: i32) -> Vec<u8> {
        if y < 0 {
            return Vec::new();
        }
        self.row_copy(y as u32)
    }

    /// Écriture `rawdata` à `(x,y)` (≙ `copyTo` amont via
    /// `rawdata` direct).
    fn write_rgb_span(&mut self, x: i32, y: i32, data: &[u8]) {
        if x < 0 || y < 0 || y >= self.height as i32 {
            return;
        }
        let off = (y as u32 * 3 * self.width + 3 * x as u32) as usize;
        let mut d = self.data.borrow_mut();
        let n = data.len().min(d.len().saturating_sub(off));
        d[off..off + n].copy_from_slice(&data[..n]);
    }

    /// `ref` — refcount manuel amont (init 1).
    fn ref_(&mut self) {
        self.ref_count.set(self.ref_count.get() + 1);
    }

    /// `unref` — amont supprime si 0 et conditions ; ici `Rc`
    /// gère la destruction — le compteur reste la sémantique
    /// observable (`lastReference`/`isReferred`).
    fn unref(&mut self) {
        self.ref_count.set(self.ref_count.get() - 1);
    }

    /// `lastReference` — `refCount==1 && pas de fils`.
    fn last_reference(&self) -> bool {
        self.ref_count.get() == 1 && self.live_children().is_empty()
    }

    /// `setDeleteOnUnref` (root only amont — muet sur fils).
    fn set_delete_on_unref(&mut self, delete_on_unref: bool) {
        if self.root.is_none() {
            self.delete_on_unref.set(delete_on_unref);
        }
    }

    /// `isReferred` — `refCount!=0 || fils restants`.
    fn is_referred(&self) -> bool {
        self.ref_count.get() != 0 || !self.live_children().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_uses_cmap() {
        let cmap = [255, 0, 0, 0, 255, 0]; // 2 couleurs
        let line = rgb_line(&[1, 0, 1], ImgType::Indexed, Some(&cmap), 3);
        assert_eq!(line, vec![0, 255, 0, 255, 0, 0, 0, 255, 0]);
        // Sans cmap → noir (warn amont).
        let line = rgb_line(&[1], ImgType::Indexed, None, 1);
        assert_eq!(line, vec![0, 0, 0]);
    }

    #[test]
    fn gray_replicates() {
        let line = rgb_line(&[7, 128], ImgType::Gray, None, 2);
        assert_eq!(line, vec![7, 7, 7, 128, 128, 128]);
    }

    #[test]
    fn cmyk_inv_math_verbatim() {
        // amont : `buf[x*4+c] * white / 0x100` (0x100 ≠ 0xff !).
        let line = rgb_line(&[200, 100, 50, 128], ImgType::CmykInv, None, 1);
        assert_eq!(line[0], u8::try_from(200 * 128 / 0x100).unwrap());
        assert_eq!(line[1], u8::try_from(100 * 128 / 0x100).unwrap());
        assert_eq!(line[2], u8::try_from(50 * 128 / 0x100).unwrap());
    }

    #[test]
    fn rgb_passthrough() {
        let line = rgb_line(&[1, 2, 3, 4, 5, 6], ImgType::Rgb, None, 2);
        assert_eq!(line, vec![1, 2, 3, 4, 5, 6]);
    }
}
