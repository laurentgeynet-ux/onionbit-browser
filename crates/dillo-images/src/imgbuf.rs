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
//! scalés (`scaleMode == BEAUTIFUL_GAMMA` verbatim : moyenne de boîte
//! gamma-corrigée), `copiedRows` par
//! buffer, `setCMap` no-op verbatim, rawdata init `222` verbatim.
//! `root` est un `Rc` fort (≙ le jeu de refcounts amont gardant
//! la racine vivante tant qu'un fils existe — même durée de vie
//! observable).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
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

/// `ScaleMode` (fltkimgbuf.cc) — `SIMPLE`/`BEAUTIFUL`/`BEAUTIFUL_GAMMA`.
/// `BEAUTIFUL` = même box-average sans correction gamma (upstream ne
/// le distingue que par l'absence des tables).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // `SIMPLE`/`BEAUTIFUL` conservés pour parité (amont garde les 3 modes).
enum ScaleMode {
    /// `SIMPLE` — nearest-neighbor/copie (`scaleRowSimple`).
    Simple,
    /// `BEAUTIFUL` — box-average sans gamma.
    Beautiful,
    /// `BEAUTIFUL_GAMMA` — box-average gamma-corrigé (défaut amont).
    BeautifulGamma,
}

/// `scaleMode` amont — constante en dur `BEAUTIFUL_GAMMA`
/// (fltkimgbuf.cc:37, jamais modifiée ailleurs).
const SCALE_MODE: ScaleMode = ScaleMode::BeautifulGamma;

thread_local! {
    /// `gammaCorrectionTables` amont — cache global de tables
    /// `map[i] = 255*pow(i/255, gamma)`, lookup par gamma exact
    /// (`to_bits` ≙ égalité double amont).
    static GAMMA_TABLES: RefCell<HashMap<u64, [u8; 256]>> = RefCell::new(HashMap::new());
}

/// `findGammaCorrectionTable` — table de 256 entrées
/// `map[i] = (uchar)(255 * pow(i/255, gamma))` — troncature ≙ cast
/// `uchar` C (valeurs ∈ [0,255]).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn gamma_table(gamma: f64) -> [u8; 256] {
    GAMMA_TABLES.with(|tables| {
        if let Some(m) = tables.borrow().get(&gamma.to_bits()) {
            return *m;
        }
        let mut m = [0u8; 256];
        for (i, v) in m.iter_mut().enumerate() {
            *v = (255.0 * (i as f64 / 255.0).powf(gamma)) as u8;
        }
        tables.borrow_mut().insert(gamma.to_bits(), m);
        m
    })
}

/// `scaleBuffer` (fltkimgbuf.cc:290) — pour chaque pixel dest,
/// moyenne la boîte source `[xo1,xo2)×[yo1,yo2)` :
/// `xo1 = x*srcW/destW`, `xo2 = max((x+1)*srcW/destW, xo1+1)` (idem Y).
/// `BEAUTIFUL_GAMMA` : `map2` linéarise chaque composante source
/// (`v += gammaMap2[p]`), `map1` ré-applique le gamma sur la moyenne
/// (`v[i]/n`). `int` amont → `i32` wrapping (borne amont identique).
/// `bpp = 3` (RGB).
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
fn scale_buffer(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    dest: &mut [u8],
    dest_w: usize,
    dest_h: usize,
    gamma: f64,
) {
    const BPP: usize = 3;
    let map1 = gamma_table(gamma);
    let map2 = gamma_table(1.0 / gamma);
    for x in 0..dest_w {
        for y in 0..dest_h {
            let xo1 = x * src_w / dest_w;
            let xo2 = ((x + 1) * src_w / dest_w).max(xo1 + 1);
            let yo1 = y * src_h / dest_h;
            let yo2 = ((y + 1) * src_h / dest_h).max(yo1 + 1);
            let n = ((xo2 - xo1) * (yo2 - yo1)) as i32;
            let mut v = [0i32; BPP];
            for xo in xo1..xo2 {
                for yo in yo1..yo2 {
                    let ps = (yo * src_w + xo) * BPP;
                    for (i, acc) in v.iter_mut().enumerate() {
                        if let Some(&p) = src.get(ps + i) {
                            *acc = acc.wrapping_add(i32::from(map2[p as usize]));
                        }
                    }
                }
            }
            let pd = (y * dest_w + x) * BPP;
            for (i, acc) in v.iter().enumerate() {
                if let Some(d) = dest.get_mut(pd + i) {
                    *d = map1[acc.wrapping_div(n).clamp(0, 255) as usize];
                }
            }
        }
    }
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
    /// `gamma` — utilisée par `scaleBuffer` (`BEAUTIFUL_GAMMA`) pour
    /// linéariser avant moyenne (`gammaMap2 = table(1/gamma)`).
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

    /// `scaleRow` — `row` en coordonnées ROOT. `scaleMode` amont =
    /// `BEAUTIFUL_GAMMA` (constante en dur, fltkimgbuf.cc:37) —
    /// dispatch verbatim (`SIMPLE` gardé pour parité structurelle).
    fn scale_row(&self, row: u32, data: &[u8]) {
        let Some(root) = &self.root else {
            return;
        };
        if row >= root.height {
            return;
        }
        match SCALE_MODE {
            ScaleMode::Simple => self.scale_row_simple(row, data),
            ScaleMode::Beautiful | ScaleMode::BeautifulGamma => {
                self.scale_row_beautiful(row, data);
            }
        }
    }

    /// `scaledY` — `ySrc * height / root->height`.
    fn scaled_y(&self, y_src: u32) -> u32 {
        let root_h = self.root.as_ref().map_or(self.height, |r| r.height);
        y_src * self.height / root_h
    }

    /// `backscaledY` — `yScaled * root->height / height` (borne
    /// inférieure — pas l'inverse exacte de `scaledY`, verbatim).
    fn backscaled_y(&self, y_scaled: u32) -> u32 {
        let root_h = self.root.as_ref().map_or(self.height, |r| r.height);
        y_scaled * root_h / self.height
    }

    /// `scaleRowBeautiful` (fltkimgbuf.cc:245) — upscale : la ligne
    /// source arrivante alimente `sr2-sr1` lignes dest via
    /// `scaleBuffer(srcH=1)` ; downscale : attend que TOUTES les
    /// lignes racine de `[backscaledY(sr1), backscaledY(sr1+1))`
    /// soient copiées puis produit UNE ligne dest (`destH=1`).
    /// `copiedRows[sr1]` court-circuite le rescale (verbatim).
    fn scale_row_beautiful(&self, row: u32, data: &[u8]) {
        let Some(root) = &self.root else {
            return;
        };
        let sr1 = self.scaled_y(row);
        let sr2 = self.scaled_y(row + 1);
        if sr1 >= self.height {
            return;
        }
        if self.copied_rows.borrow().get_bit(sr1 as usize) != 0 {
            return;
        }
        let dw = (3 * self.width) as usize;
        if self.height > root.height {
            // Agrandissement vertical : 1 ligne source → sr2-sr1
            // lignes dest (bornées à `height`).
            let count = (sr2 - sr1).min(self.height - sr1).max(1);
            let mut out = vec![0u8; count as usize * dw];
            scale_buffer(
                data,
                root.width as usize,
                1,
                &mut out,
                self.width as usize,
                count as usize,
                self.gamma,
            );
            let off = sr1 as usize * dw;
            self.data.borrow_mut()[off..off + out.len()].copy_from_slice(&out);
            // `for (sr = sr1; sr < sr2 || sr == sr1; sr++)` verbatim —
            // marque au moins sr1 même si sr2 <= sr1.
            let mut cr = self.copied_rows.borrow_mut();
            let mut sr = sr1;
            while sr < sr2 || sr == sr1 {
                if sr < self.height {
                    cr.set_bit(sr as usize);
                }
                sr += 1;
            }
        } else {
            // Réduction : `assert(sr1==sr2 || sr1+1==sr2)` amont.
            let row1 = self.backscaled_y(sr1);
            let row2 = self.backscaled_y(sr1 + 1);
            // `for (r=row1; (allRootRows = root->copiedRows->get(r))
            //  && ++r < row2; )` — vrai si toutes les lignes
            // [row1, row2) sont arrivées.
            let mut r = row1;
            let all_root = loop {
                if root.copied_rows.borrow().get_bit(r as usize) == 0 {
                    break false;
                }
                r += 1;
                if r >= row2 {
                    break true;
                }
            };
            if all_root {
                let rw = (3 * root.width) as usize;
                let src_h = (row2 - row1) as usize;
                let off = row1 as usize * rw;
                let mut out = vec![0u8; dw];
                {
                    let root_data = root.data.borrow();
                    let Some(end) = off.checked_add(src_h * rw) else {
                        return;
                    };
                    if end > root_data.len() {
                        return;
                    }
                    scale_buffer(
                        &root_data[off..end],
                        root.width as usize,
                        src_h,
                        &mut out,
                        self.width as usize,
                        1,
                        self.gamma,
                    );
                }
                let dst = sr1 as usize * dw;
                self.data.borrow_mut()[dst..dst + dw].copy_from_slice(&out);
                self.copied_rows.borrow_mut().set_bit(sr1 as usize);
            }
        }
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
/// `BEAUTIFUL_GAMMA` — porté verbatim (`scaleRowBeautiful` +
/// `scaleBuffer` gamma-corrigé).
/// Casts u32↔i32 bornés par `MAX_DIM` (0x8000).
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
impl DwImgbuf for Imgbuf {
    /// `setCMap` — **vide amont** pour `FltkImgbuf` (RGB only — la
    /// cmap est appliquée dans `Imgbuf_rgb_line` côté dicache).
    fn set_cmap(&self, _colors: &[i32]) {}

    /// `copyRow` (≙ `assert(isRoot)` — sur un fils, délègue).
    fn copy_row(&self, row: i32, data: &[u8]) {
        if row >= 0 {
            Imgbuf::copy_row(self, row as u32, data);
        }
    }

    /// `newScan`.
    fn new_scan(&self) {
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
    fn write_rgb_span(&self, x: i32, y: i32, data: &[u8]) {
        if x < 0 || y < 0 || y >= self.height as i32 {
            return;
        }
        let off = (y as u32 * 3 * self.width + 3 * x as u32) as usize;
        let mut d = self.data.borrow_mut();
        let n = data.len().min(d.len().saturating_sub(off));
        d[off..off + n].copy_from_slice(&data[..n]);
    }

    /// `ref` — refcount manuel amont (init 1).
    fn ref_(&self) {
        self.ref_count.set(self.ref_count.get() + 1);
    }

    /// `unref` — amont supprime si 0 et conditions ; ici `Rc`
    /// gère la destruction — le compteur reste la sémantique
    /// observable (`lastReference`/`isReferred`).
    fn unref(&self) {
        self.ref_count.set(self.ref_count.get() - 1);
    }

    /// `lastReference` — `refCount==1 && pas de fils`.
    fn last_reference(&self) -> bool {
        self.ref_count.get() == 1 && self.live_children().is_empty()
    }

    /// `setDeleteOnUnref` (root only amont — muet sur fils).
    fn set_delete_on_unref(&self, delete_on_unref: bool) {
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

    /// Fils scalé enregistré sur la racine (≙ `getScaledBuf` +
    /// `scaledBuffers` amont, sans passer par le trait `DwImgbuf`).
    fn scaled_child(root: &Rc<Imgbuf>, w: u32, h: u32) -> Rc<Imgbuf> {
        let child = Rc::new(Imgbuf::with_root(
            ImgbufType::Rgb,
            w,
            h,
            root.gamma,
            Some(Rc::clone(root)),
        ));
        root.children.borrow_mut().push(Rc::downgrade(&child));
        child
    }

    /// A09-D8 — `scaleBuffer` box-average : 4→1 px large, gamma=1
    /// (tables identité) → moyenne arithmétique du footprint.
    #[test]
    fn scaled_downscale_box_average() {
        let root = Imgbuf::new(ImgbufType::Rgb, 4, 1, 1.0);
        let child = scaled_child(&root, 2, 1);
        root.copy_row(0, &[10, 0, 0, 20, 0, 0, 30, 0, 0, 40, 0, 0]);
        let d = child.data.borrow();
        // [10,20] → 15 ; [30,40] → 35 (moyenne entière amont `v/n`).
        assert_eq!(&d[..6], &[15, 0, 0, 35, 0, 0]);
    }

    /// A09-D8 — `BEAUTIFUL_GAMMA` : `map2=pow(1/gamma)` linéarise,
    /// `map1=pow(gamma)` ré-encode. gamma=2.0, source {0,255} :
    /// `v/n = (0+255)/2 = 127`, `map1[127] = 255*(127/255)² = 63`
    /// — ≠ moyenne naïve 127 (déviation pixel visible).
    #[test]
    fn scaled_gamma_corrected_average() {
        let root = Imgbuf::new(ImgbufType::Rgb, 2, 1, 2.0);
        let child = scaled_child(&root, 1, 1);
        root.copy_row(0, &[0, 0, 0, 255, 255, 255]);
        assert_eq!(&child.data.borrow()[..3], &[63, 63, 63]);
    }

    /// A09-D8 — réduction verticale : une ligne dest n'est produite
    /// que quand TOUTES les lignes racine de sa bande
    /// `[backscaledY(sr1), backscaledY(sr1+1))` sont arrivées
    /// (`allRootRows` amont) — progressive scanline préservé.
    #[test]
    fn scaled_downscale_waits_for_all_source_rows() {
        let root = Imgbuf::new(ImgbufType::Rgb, 1, 4, 1.0);
        let child = scaled_child(&root, 1, 2);
        root.copy_row(0, &[10, 10, 10]);
        // Ligne 1 racine manquante → bande incomplète → gris init 222.
        assert_eq!(&child.data.borrow()[..3], &[222, 222, 222]);
        root.copy_row(1, &[30, 30, 30]);
        // Bande [0,2) complète → moyenne (10+30)/2 = 20.
        assert_eq!(&child.data.borrow()[..3], &[20, 20, 20]);
    }

    /// A09-D8 — agrandissement : `scaleBuffer(srcH=1)` réplique la
    /// ligne source sur `sr2-sr1` lignes dest (pas d'interpolation).
    #[test]
    fn scaled_upscale_replicates_rows() {
        let root = Imgbuf::new(ImgbufType::Rgb, 1, 1, 1.0);
        let child = scaled_child(&root, 1, 3);
        root.copy_row(0, &[7, 8, 9]);
        assert_eq!(&child.data.borrow()[..], &[7, 8, 9, 7, 8, 9, 7, 8, 9]);
    }

    /// `copiedRows[sr1]` — ligne dest déjà produite → pas de rescale
    /// (verbatim : le no-op `scaleRow` sur ligne déjà copiée).
    #[test]
    fn scaled_row_not_rescaled() {
        let root = Imgbuf::new(ImgbufType::Rgb, 1, 4, 1.0);
        let child = scaled_child(&root, 1, 2);
        root.copy_row(0, &[10, 10, 10]);
        root.copy_row(1, &[30, 30, 30]);
        assert_eq!(&child.data.borrow()[..3], &[20, 20, 20]);
        // Réécriture de la ligne racine 0 : `copiedRows[0]` fils déjà
        // posé → scale_row retourne sans toucher la ligne dest.
        root.copy_row(0, &[200, 200, 200]);
        assert_eq!(&child.data.borrow()[..3], &[20, 20, 20]);
    }
}
