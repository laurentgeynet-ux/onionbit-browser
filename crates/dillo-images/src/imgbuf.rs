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

use std::cell::RefCell;
use std::rc::Rc;

use dillo_common::bitvec::Bitvec;

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
        Rc::new(Self {
            buf_type,
            width,
            height,
            gamma,
            data: RefCell::new(vec![0; (3 * width * height) as usize]),
        })
    }

    /// `copyRow` — copie `3*width` octets décodés dans la ligne `y`
    /// (`dReturn_if_fail(y < height)` amont → muet).
    pub fn copy_row(&self, y: u32, buf: &[u8]) {
        if y >= self.height {
            return;
        }
        let w = (3 * self.width) as usize;
        let off = y as usize * w;
        let mut data = self.data.borrow_mut();
        let n = w.min(buf.len()).min(data.len() - off);
        data[off..off + n].copy_from_slice(&buf[..n]);
    }

    /// `newScan` — reset pour un nouveau scan (multi-scan/interlacé).
    /// Amont efface la back-buffer ; ici le buffer unique garde ses
    /// données (le redessin passe par `BitVec` côté `DilloImage`).
    pub fn new_scan(&self) {
        let _ = self;
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
