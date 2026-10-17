//! Port de `src/image.cc` + `image.hh` — `DilloImage`, transfert des
//! données décompressées vers le widget image.
//!
//! Amont : `DilloImage` porte `layout`/`img_rndr` (`void*`) — ici
//! `img_rndr` = [`ImgRenderer`] seam (le `dw::Image` porté implémentera
//! `set_buffer`/`draw_row`/`finish`/`fatal` ≙ `I2IR(Image)->…`),
//! `layout` ≙ handle opaque passé à `a_Imgbuf_new` (notre `Imgbuf`
//! n'a pas besoin du layout — `dpi` est gardé).
//!
//! `RefCount` amont ≙ `Rc` — `a_Image_ref` = clone du `Rc`,
//! `a_Image_unref` = drop (la dernière `Rc` libère, comme amont) ; le
//! champ `ref_count` est conservé pour la sémantique des messages.

use std::cell::RefCell;
use std::rc::Rc;

use dillo_common::bitvec::Bitvec;

use crate::imgbuf::{ImgType, Imgbuf};

/// `IMAGE_MAX_AREA` (image.hh) — borne anti image-crafting.
pub const IMAGE_MAX_AREA: u32 = 6000 * 6000;

/// `ImageState` — état de traitement de l'image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImageState {
    /// `IMG_Empty` — créée.
    Empty,
    /// `IMG_SetParms` — paramètres posés.
    SetParms,
    /// `IMG_SetCmap` — colormap posée.
    SetCmap,
    /// `IMG_Write` — lignes en cours.
    Write,
    /// `IMG_Close` — image complète.
    Close,
    /// `IMG_Abort` — transfert avorté.
    Abort,
}

/// `dw::core::ImgRenderer` — le trait réel porté dans
/// `dillo-dw` (`I2IR(Image)` amont — implémentée par le futur
/// `dw::Image` ; le no-op sert les tests).
pub use dillo_dw::imgrenderer::ImgRenderer;

/// Buffer côté trait (`dw::core::Imgbuf` ≙ `Rc<dyn Imgbuf>`).
use dillo_dw::imgbuf::Imgbuf as DwImgbuf;

/// Renderer muet (tests / pas de dw encore).
#[derive(Default)]
pub struct NullRenderer {
    /// Lignes dessinées (traçabilité tests).
    pub drawn: Vec<i32>,
    /// `finish` vu.
    pub finished: bool,
    /// `fatal` vu.
    pub fataled: bool,
    /// Buffer courant.
    pub buf: Option<Rc<dyn DwImgbuf>>,
}

impl ImgRenderer for NullRenderer {
    fn set_buffer(&mut self, buf: &Rc<dyn DwImgbuf>, _resize: bool) {
        self.buf = Some(buf.clone());
    }
    fn draw_row(&mut self, y: i32) {
        self.drawn.push(y);
    }
    fn finish(&mut self) {
        self.finished = true;
    }
    fn fatal(&mut self) {
        self.fataled = true;
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// `DilloImage` — l'image vue par une requête web.
pub struct DilloImage {
    /// `Image->layout` — handle layout amont (≙ `void*` ; notre
    /// `Imgbuf` ne l'utilise pas, conservé pour `dpi`).
    pub layout: u32,
    /// `Image->img_rndr` — le widget/renderer (`I2IR` amont) ; partagé
    /// avec `a_Image_get_dw` (le HTML l'embarque comme widget).
    pub img_rndr: Rc<RefCell<dyn ImgRenderer>>,
    /// Paramètres depuis les données image.
    pub width: u32,
    /// `height`.
    pub height: u32,
    /// `dpi` — `layout->dpiX()` amont.
    pub dpi: f32,
    /// `bg_color`.
    pub bg_color: i32,
    /// `fg_color`.
    pub fg_color: i32,
    /// `BitVec` — lignes déjà dessinées.
    pub bitvec: Option<Bitvec>,
    /// `ScanNumber` — scan courant (multi-scan).
    pub scan_number: u32,
    /// `State`.
    pub state: ImageState,
    /// `RefCount` — compteur logique ≙ `Rc` (les deux avancent
    /// ensemble ; champ gardé pour les logs amont).
    pub ref_count: i32,
}

impl DilloImage {
    /// `a_Image_new` — `layout` ≙ handle opaque (dpi résolu par
    /// l'appelant ≙ `layout->dpiX()`).
    #[must_use]
    pub fn new(
        layout: u32,
        img_rndr: Rc<RefCell<dyn ImgRenderer>>,
        bg_color: i32,
        fg_color: i32,
        dpi: f32,
    ) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            layout,
            img_rndr,
            width: 0,
            height: 0,
            dpi,
            bg_color,
            fg_color,
            bitvec: None,
            scan_number: 0,
            state: ImageState::Empty,
            ref_count: 0,
        }))
    }
}

/// `a_Image_new` ≙ [`DilloImage::new`].
#[must_use]
pub fn image_new(
    layout: u32,
    img_rndr: Rc<RefCell<dyn ImgRenderer>>,
    bg_color: i32,
    fg_color: i32,
    dpi: f32,
) -> Rc<RefCell<DilloImage>> {
    DilloImage::new(layout, img_rndr, bg_color, fg_color, dpi)
}

/// `a_Image_get_dw` — le renderer comme handle widget (`Rc` partagé
/// ≙ cast `dw::Image*` amont).
#[must_use]
pub fn image_get_dw(image: &Rc<RefCell<DilloImage>>) -> Rc<RefCell<dyn ImgRenderer>> {
    image.borrow().img_rndr.clone()
}

/// `a_Image_ref` — clone du `Rc` (compteur logique ajusté).
#[must_use]
pub fn image_ref(image: &Rc<RefCell<DilloImage>>) -> Rc<RefCell<DilloImage>> {
    image.borrow_mut().ref_count += 1;
    image.clone()
}

/// `a_Image_unref` — décrémente le compteur logique ; le `Rc` droppé
/// par l'appelant libère la dernière référence (≙ free à 0 amont).
pub fn image_unref(image: &Rc<RefCell<DilloImage>>) {
    let mut i = image.borrow_mut();
    if i.ref_count > 0 {
        i.ref_count -= 1;
    }
}

/// `a_Image_set_parms` — `setBuffer(resize)` + init `BitVec` + dims.
pub fn image_set_parms(
    image: &Rc<RefCell<DilloImage>>,
    imgbuf: Rc<Imgbuf>,
    width: u32,
    height: u32,
) {
    let (rndr, needs_bv) = {
        let mut i = image.borrow_mut();
        let rndr = i.img_rndr.clone();
        let needs_bv = i.bitvec.is_none();
        if needs_bv {
            i.bitvec = Some(Bitvec::new(height as usize));
        }
        i.width = width;
        i.height = height;
        i.state = ImageState::SetParms;
        (rndr, needs_bv)
    };
    let _ = needs_bv;
    // `I2IR(Image)->setBuffer((Imgbuf*)v_imgbuf, resize = true)` —
    // hors borrow (le renderer peut réentrer).
    let buf: Rc<dyn DwImgbuf> = imgbuf;
    rndr.borrow_mut().set_buffer(&buf, true);
}

/// `a_Image_write` — `drawRow(y)` + bit (`dReturn_if_fail(y<height)`).
pub fn image_write(image: &Rc<RefCell<DilloImage>>, y: u32) {
    let rndr = {
        let mut i = image.borrow_mut();
        if y >= i.height {
            return;
        }
        if let Some(bv) = &mut i.bitvec {
            bv.set_bit(y as usize);
        }
        i.state = ImageState::Write;
        i.img_rndr.clone()
    };
    #[allow(clippy::cast_possible_wrap)]
    rndr.borrow_mut().draw_row(y as i32);
}

/// `a_Image_close` — `I2IR->finish()`.
pub fn image_close(image: &Rc<RefCell<DilloImage>>) {
    let rndr = {
        let mut i = image.borrow_mut();
        i.state = ImageState::Close;
        i.img_rndr.clone()
    };
    rndr.borrow_mut().finish();
}

/// `a_Image_abort` — `I2IR->fatal()`.
pub fn image_abort(image: &Rc<RefCell<DilloImage>>) {
    let rndr = {
        let mut i = image.borrow_mut();
        i.state = ImageState::Abort;
        i.img_rndr.clone()
    };
    rndr.borrow_mut().fatal();
}

/// Type brut porté par les décodeurs vers `set_parms`/`write` —
/// réexport [`ImgType`] ≙ `DilloImgType`.
pub type DilloImgType = ImgType;

#[cfg(test)]
mod tests {
    use super::*;

    fn mk() -> (Rc<RefCell<DilloImage>>, Rc<RefCell<NullRenderer>>) {
        let r = Rc::new(RefCell::new(NullRenderer::default()));
        let i = image_new(0, r.clone(), 0, 0, 96.0);
        (i, r)
    }

    #[test]
    fn lifecycle() {
        let (img, r) = mk();
        let buf = Imgbuf::new(crate::imgbuf::ImgbufType::Rgb, 4, 4, 1.0);
        image_set_parms(&img, buf, 4, 4);
        assert_eq!(img.borrow().state, ImageState::SetParms);
        image_write(&img, 2);
        assert_eq!(r.borrow().drawn, vec![2]);
        // y hors bornes → muet.
        image_write(&img, 9);
        assert_eq!(r.borrow().drawn, vec![2]);
        image_close(&img);
        assert!(r.borrow().finished);
    }
}
