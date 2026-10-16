//! Harness golden-render headless — `tiny-skia` offscreen + diff.
//!
//! Prérequis de vérification de toute la chaîne de rendu (ROADMAP
//! phase 0) : un test dessine dans un `Pixmap` offscreen, puis le
//! compare pixel-à-pixel à une image de référence (golden PNG dans
//! `tests/golden/`).
//!
//! Conventions :
//! - les goldens se régénèrent via `DILLO_UPDATE_GOLDEN=1`
//!   (`assert_golden` écrit alors au lieu de comparer) ;
//! - le diff compte les pixels dont *un canal* diffère, avec la
//!   bbox englobante — les rapports restent lisibles quand une
//!   régression de layout décale tout ;
//! - la comparaison porte sur les bytes RGBA prémultipliés de
//!   `tiny-skia` — les goldens sont donc dépendants de la version
//!   de `tiny-skia` (pin workspace, cf. ADR-0002 fonte embarquée).

use std::fmt;
use std::path::{Path, PathBuf};

/// Rapport de diff pixel-à-pixel entre deux `Pixmap`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Diff {
    /// Taille différente → comparaison impossible (compte = None).
    pub size_mismatch: Option<((u32, u32), (u32, u32))>,
    /// Pixels dont au moins un canal RGBA diffère.
    pub diff_pixels: usize,
    /// Pixels comparés (0 si `size_mismatch`).
    pub total_pixels: usize,
    /// Plus grand delta de canal observé (0..=255).
    pub max_channel_delta: u8,
    /// Bbox des pixels différents `(x, y, w, h)`, `None` si identique.
    pub bbox: Option<(u32, u32, u32, u32)>,
}

impl Diff {
    /// Vrai si les deux images sont identiques.
    #[must_use]
    pub fn is_identical(&self) -> bool {
        self.size_mismatch.is_none() && self.diff_pixels == 0
    }
}

impl fmt::Display for Diff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((a, b)) = self.size_mismatch {
            return write!(f, "size mismatch: {a:?} vs {b:?}");
        }
        write!(
            f,
            "{}/{} px differ, max channel delta {}, bbox {:?}",
            self.diff_pixels, self.total_pixels, self.max_channel_delta, self.bbox
        )
    }
}

/// Compare deux `Pixmap` byte-à-byte (RGBA prémultiplié).
#[must_use]
pub fn diff_pixmaps(actual: &tiny_skia::Pixmap, expected: &tiny_skia::Pixmap) -> Diff {
    if actual.width() != expected.width() || actual.height() != expected.height() {
        return Diff {
            size_mismatch: Some((
                (actual.width(), actual.height()),
                (expected.width(), expected.height()),
            )),
            ..Diff::default()
        };
    }
    let (width, height) = (actual.width(), actual.height());
    let (pa, pb) = (actual.data(), expected.data());
    let mut diff = Diff {
        total_pixels: (width as usize) * (height as usize),
        ..Diff::default()
    };
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let mut differs = false;
            for c in 0..4 {
                let d = pa[i + c].abs_diff(pb[i + c]);
                diff.max_channel_delta = diff.max_channel_delta.max(d);
                differs |= d != 0;
            }
            if differs {
                diff.diff_pixels += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if diff.diff_pixels > 0 {
        diff.bbox = Some((min_x, min_y, max_x - min_x + 1, max_y - min_y + 1));
    }
    diff
}

/// Erreurs du harness golden (I/O, PNG, diff).
#[derive(Debug, thiserror::Error)]
pub enum GoldenError {
    /// I/O sur le fichier golden.
    #[error("golden io: {0}")]
    Io(#[from] std::io::Error),
    /// Décodage/encodage PNG.
    #[error("golden png: {0}")]
    Png(String),
    /// Le rendu diffère du golden (avec le rapport).
    #[error("golden diff {path}: {diff}")]
    Mismatch {
        /// Chemin du golden de référence.
        path: PathBuf,
        /// Rapport de diff.
        diff: Box<Diff>,
    },
}

/// Écrit un `Pixmap` en PNG RGBA.
///
/// # Errors
/// `GoldenError::Io`/`Png` sur échec disque/encodeur.
pub fn save_png(path: &Path, pixmap: &tiny_skia::Pixmap) -> Result<(), GoldenError> {
    let file = std::fs::File::create(path)?;
    let w = std::io::BufWriter::new(file);
    let mut enc = png::Encoder::new(w, pixmap.width(), pixmap.height());
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc
        .write_header()
        .map_err(|e| GoldenError::Png(e.to_string()))?;
    writer
        .write_image_data(pixmap.data())
        .map_err(|e| GoldenError::Png(e.to_string()))
}

/// Lit un PNG en `Pixmap` (RGBA8).
///
/// # Errors
/// `GoldenError::Io`/`Png` sur échec disque/décodeur ou format non RGBA8.
pub fn load_png(path: &Path) -> Result<tiny_skia::Pixmap, GoldenError> {
    let file = std::fs::File::open(path)?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .map_err(|e| GoldenError::Png(e.to_string()))?;
    let mut buf = vec![0; dec.output_buffer_size()];
    let info = dec
        .next_frame(&mut buf)
        .map_err(|e| GoldenError::Png(e.to_string()))?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(GoldenError::Png(format!(
            "expected RGBA8, got {:?}/{:?}",
            info.color_type, info.bit_depth
        )));
    }
    buf.truncate(info.buffer_size());
    tiny_skia::Pixmap::from_vec(
        buf,
        tiny_skia::IntSize::from_wh(info.width, info.height).unwrap_or_else(|| {
            // impossible : PNG valide a w/h > 0
            tiny_skia::IntSize::from_wh(1, 1).unwrap_or_else(|| unreachable!())
        }),
    )
    .ok_or_else(|| GoldenError::Png("pixmap size overflow".into()))
}

/// Compare `actual` au golden `path` ; si `DILLO_UPDATE_GOLDEN=1`,
/// écrit le golden à la place (mode régénération).
///
/// # Errors
/// `GoldenError::Mismatch` si le rendu diffère ; `Io`/`Png` sur erreur fichier.
pub fn assert_golden(actual: &tiny_skia::Pixmap, path: &Path) -> Result<(), GoldenError> {
    if std::env::var_os("DILLO_UPDATE_GOLDEN").is_some() {
        return save_png(path, actual);
    }
    let expected = load_png(path)?;
    let diff = diff_pixmaps(actual, &expected);
    if diff.is_identical() {
        Ok(())
    } else {
        Err(GoldenError::Mismatch {
            path: path.to_path_buf(),
            diff: Box::new(diff),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Paint, PathBuilder, Pixmap, Transform};

    fn scene(square_x: f32) -> Pixmap {
        let mut pm = Pixmap::new(64, 48).unwrap_or_else(|| unreachable!());
        pm.fill(tiny_skia::Color::WHITE);
        let rect = tiny_skia::Rect::from_xywh(8.0 + square_x, 8.0, 24.0, 24.0)
            .unwrap_or_else(|| unreachable!());
        let mut pb = PathBuilder::new();
        pb.push_rect(rect);
        let path = pb.finish().unwrap_or_else(|| unreachable!());
        let mut paint = Paint::default();
        paint.set_color_rgba8(0x20, 0x60, 0xa0, 0xff);
        pm.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
        pm
    }

    #[test]
    fn identical_scenes_match() {
        let d = diff_pixmaps(&scene(0.0), &scene(0.0));
        assert!(d.is_identical());
    }

    #[test]
    fn shifted_rect_detected_with_bbox() {
        let d = diff_pixmaps(&scene(0.0), &scene(4.0));
        assert!(!d.is_identical());
        assert!(d.diff_pixels > 0);
        assert!(d.bbox.is_some());
    }

    #[test]
    fn size_mismatch_reported() {
        let a = Pixmap::new(10, 10).unwrap_or_else(|| unreachable!());
        let b = Pixmap::new(20, 10).unwrap_or_else(|| unreachable!());
        assert!(diff_pixmaps(&a, &b).size_mismatch.is_some());
    }

    #[test]
    fn png_roundtrip() {
        let pm = scene(0.0);
        let dir = std::env::temp_dir().join("dillorust_golden_test");
        std::fs::create_dir_all(&dir).unwrap_or_else(|_| unreachable!());
        let p = dir.join("scene.png");
        save_png(&p, &pm).unwrap_or_else(|_| unreachable!());
        let back = load_png(&p).unwrap_or_else(|_| unreachable!());
        assert!(diff_pixmaps(&pm, &back).is_identical());
        let _ = std::fs::remove_file(&p);
    }
}
