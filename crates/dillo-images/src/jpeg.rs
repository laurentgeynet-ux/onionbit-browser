//! Port de `src/jpeg.c` — décodeur JPEG.
//!
//! Amont : libjpeg avec *source manager* suspendable — décodage par
//! scanline au fil du réseau, et pour les JPEG progressifs multi-scan
//! (`jpeg_has_multiple_scans` + `buffered_image`) raffinement image
//! par image avec `a_Dicache_new_scan` entre les scans.
//!
//! Ici : `jpeg-decoder` (pur Rust) décode l'image **complète** — on
//! attend le marqueur EOI (`FF D9`) comme amont attend un flux
//! complet, puis une seule passe d'émission. Déviations documentées
//! (à couvrir par `audit-09-images`) :
//! - pas d'affichage progressif des scanlines pendant le transfert ;
//! - JPEG progressifs : une écriture finale par ligne au lieu du
//!   raffinement multi-scan (`new_scan` inutilisé) ;
//! - JPEG tronqué (jamais d'EOI) : amont affiche les lignes déjà
//!   décodées ; on tente un décodage final à `CA_Close` mais un
//!   échec n'émet rien ;
//! - `jpeg-decoder` ne produit pas les warnings libjpeg
//!   (« YCCK colors wrong? », « No adobe marker ») — le traitement
//!   CMYK/YCCK reste celui du flux Adobe via `imgbuf` `CmykInv`.

use std::cell::RefCell;
use std::io::ErrorKind;
use std::rc::Rc;

use dillo_net::cache::{CacheClient, CA_CLOSE, CA_SEND};
use dillo_net::url::DilloUrl;
use jpeg_decoder::{Decoder, Error, PixelFormat};

use crate::dicache::{dicache_close, dicache_set_parms, dicache_write, ImageDecoder};
use crate::image::DilloImage;
use crate::imgbuf::ImgType;

/// `IMAGE_MAX_AREA` (image.hh).
const IMAGE_MAX_AREA: u64 = 6000 * 6000;

/// Marqueur EOI — libjpeg tolère le remplissage terminal ; on attend
/// ce marqueur comme condition « flux complet » avant de décoder.
const EOI: [u8; 2] = [0xff, 0xd9];

/// `DilloJpegState` : `DILLO_JPEG_INIT` (rien émis),
/// `DILLO_JPEG_DONE`, `DILLO_JPEG_ERROR` — les états intermédiaires
/// n'existent plus (décodage monolithique).
#[derive(PartialEq)]
enum JpegState {
    /// `DILLO_JPEG_INIT`.
    Init,
    /// `DILLO_JPEG_DONE`.
    Done,
    /// `DILLO_JPEG_ERROR`.
    Error,
}

/// `DilloJpeg` — état du décodeur JPEG.
pub struct DilloJpeg {
    /// `Image` — retenu jusqu'au `set_parms` puis `NULL` amont.
    image: Option<Rc<RefCell<DilloImage>>>,
    /// `url`/`version` — clés dicache.
    url: DilloUrl,
    /// `version`.
    version: i32,
    /// `state`.
    state: JpegState,
}

impl DilloJpeg {
    /// `a_Jpeg_new`.
    pub fn new(image: Rc<RefCell<DilloImage>>, url: DilloUrl, version: i32) -> Self {
        Self {
            image: Some(image),
            url,
            version,
            state: JpegState::Init,
        }
    }

    /// `Jpeg_write` — `Buf` = tampon réseau complet ; décode quand
    /// l'EOI est vu (ou sur `CA_Close` pour les flux tronqués —
    /// `fill_input_buffer` amont retourne FALSE = suspend).
    fn write(&mut self, buf: &[u8], final_attempt: bool) {
        if self.state != JpegState::Init {
            return;
        }
        if !final_attempt && !buf.ends_with(&EOI) {
            return;
        }
        match Self::decode(buf) {
            Ok(Some((w, h, img_type, data, bpp))) => {
                if let Some(img) = self.image.take() {
                    dicache_set_parms(
                        &self.url,
                        self.version,
                        &img,
                        w,
                        h,
                        img_type,
                        1.0 / 2.2, // gamma amont fixe (« \todo Gamma for JPEG? »)
                    );
                }
                let stride = w as usize * bpp;
                for y in 0..h as usize {
                    let row = &data[y * stride..(y + 1) * stride];
                    #[allow(clippy::cast_possible_truncation)]
                    dicache_write(&self.url, self.version, row, y as u32);
                }
                self.state = JpegState::Done;
            }
            Ok(None) => {} // données partielles — attendre la suite
            Err(()) => self.state = JpegState::Error,
        }
    }

    /// ≙ `jpeg_read_header` + `jpeg_start_decompress` +
    /// `jpeg_read_scanlines` (retourne `None` si tronqué).
    #[allow(clippy::type_complexity)]
    fn decode(buf: &[u8]) -> Result<Option<(u32, u32, ImgType, Vec<u8>, usize)>, ()> {
        let mut dec = Decoder::new(buf);
        // `jpeg_read_header` : info() après read_info pour le check
        // `IMAGE_MAX_AREA` *avant* l'émission (ordre amont).
        if let Err(e) = dec.read_info() {
            return Self::classify_err(e);
        }
        let Some(info) = dec.info() else {
            return Err(());
        };
        let (w, h) = (u32::from(info.width), u32::from(info.height));
        if w == 0 || h == 0 || u64::from(w) > IMAGE_MAX_AREA / u64::from(h) {
            return Err(()); // « suspicious image size » → DILLO_JPEG_ERROR
        }
        let data = match dec.decode() {
            Ok(d) => d,
            Err(e) => return Self::classify_err(e),
        };
        // num_components amont : 1→GRAY, 3→RGB, 4→CMYK_INV.
        #[allow(clippy::match_wildcard_for_single_variants)]
        let (img_type, bpp) = match info.pixel_format {
            PixelFormat::L8 => (ImgType::Gray, 1),
            PixelFormat::RGB24 => (ImgType::Rgb, 3),
            PixelFormat::CMYK32 => (ImgType::CmykInv, 4),
            _ => return Err(()), // non_exhaustive — inconnu
        };
        Ok(Some((w, h, img_type, data, bpp)))
    }

    /// `Io(UnexpectedEof)` ≙ flux partiel ; le reste ≙
    /// `Jpeg_errorexit` (longjmp → `DILLO_JPEG_ERROR`).
    fn classify_err<T>(e: Error) -> Result<Option<T>, ()> {
        match e {
            Error::Io(io) if io.kind() == ErrorKind::UnexpectedEof => Ok(None),
            // jpeg-decoder rapporte certaines troncatures en Format
            // (« unexpected end of data ») — traité comme fatal :
            // l'EOI est la condition d'entrée du décodage.
            _ => Err(()),
        }
    }
}

impl ImageDecoder for DilloJpeg {
    /// `a_Jpeg_callback` : `CA_Send`→`Jpeg_write`,
    /// `CA_Close`→`Jpeg_close`, `CA_Abort`→`Jpeg_free` ≙ drop.
    fn callback(&mut self, op: i32, client: &mut CacheClient) {
        if op == CA_SEND {
            let buf = std::mem::take(&mut client.buf);
            self.write(&buf, false);
            client.buf = buf;
        } else if op == CA_CLOSE {
            // Dernière chance pour un flux sans EOI propre.
            let buf = std::mem::take(&mut client.buf);
            self.write(&buf, true);
            client.buf = buf;
            dicache_close(&self.url, self.version, client);
        }
    }
}

/// `a_Jpeg_new` — factory dicache (`DIC_JPEG`).
pub fn jpeg_new(
    image: Rc<RefCell<DilloImage>>,
    url: DilloUrl,
    version: i32,
) -> Box<dyn ImageDecoder> {
    Box::new(DilloJpeg::new(image, url, version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dicache::{
        dicache_callback, dicache_get_entry, dicache_image, dicache_set_factory,
        dicache_set_image_factory, DicEntryState, DIC_JPEG, DIC_LAST,
    };
    use crate::image::{ImgRenderer, NullRenderer};
    use dillo_net::cache::{self, CacheClient};
    use dillo_net::http::HttpWeb;
    use jpeg_encoder::{ColorType as Jct, Encoder as JEncoder};

    struct FakeFactory(Rc<RefCell<NullRenderer>>);
    impl crate::dicache::ImageFactory for FakeFactory {
        fn new_image(&mut self, _w: &HttpWeb) -> (Rc<RefCell<dyn ImgRenderer>>, f32) {
            (self.0.clone() as _, 96.0)
        }
    }

    fn client(key: i32, web: &Rc<HttpWeb>, buf: &[u8]) -> CacheClient {
        CacheClient {
            key,
            url: web.url.clone(),
            version: 0,
            buf: buf.to_vec(),
            callback: None,
            web: web.clone(),
        }
    }

    fn setup(url: &str) -> (Rc<HttpWeb>, Rc<RefCell<NullRenderer>>) {
        let nr = Rc::new(RefCell::new(NullRenderer::default()));
        dicache_set_image_factory(Box::new(FakeFactory(nr.clone())));
        dicache_set_factory(DIC_JPEG, jpeg_new);
        let web = Rc::new(HttpWeb::new(
            dillo_net::url::url_new(url.as_bytes(), None).unwrap(),
            None,
            0,
        ));
        let _ = dicache_image(DIC_JPEG, "image/jpeg", &web).unwrap();
        (web, nr)
    }

    /// JPEG 8×8 uniforme — la compression rend les pixels approximatifs,
    /// on teste dimensions + dominante de couleur.
    fn make_jpeg() -> Vec<u8> {
        let mut data = Vec::new();
        let enc = JEncoder::new(&mut data, 90);
        enc.encode(&[200u8, 30, 40].repeat(64), 8, 8, Jct::Rgb)
            .unwrap();
        data
    }

    #[test]
    fn baseline_jpeg_decodes() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, nr) = setup("http://t/pic.jpg");
        let url = web.url.clone();
        let jpg = make_jpeg();
        assert!(jpg.ends_with(&[0xff, 0xd9]));
        let mut cl = client(1, &web, &jpg);
        dicache_callback(CA_SEND, &mut cl);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        {
            let eb = e.borrow();
            assert_eq!(eb.state, DicEntryState::Write);
            let ib = eb.imgbuf.as_ref().unwrap();
            assert_eq!((ib.width, ib.height), (8, 8));
            let row = ib.row_copy(0);
            // Rouge dominant (tolérance compression).
            assert!(row[0] > 150, "r={}", row[0]);
            assert!(row[1] < 90, "g={}", row[1]);
        }
        assert_eq!(nr.borrow().drawn.len(), 8);
        dicache_callback(CA_CLOSE, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).map(|e| e.borrow().state),
            Some(DicEntryState::Close)
        );
    }

    #[test]
    fn waits_for_eoi_marker() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _nr) = setup("http://t/part.jpg");
        let url = web.url.clone();
        let jpg = make_jpeg();
        // Flux sans EOI → pas de décodage.
        let mut cl = client(2, &web, &jpg[..jpg.len() / 2]);
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).unwrap().borrow().state,
            DicEntryState::Empty
        );
        cl.buf = jpg;
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).unwrap().borrow().state,
            DicEntryState::Write
        );
    }

    #[test]
    fn garbage_never_decodes() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _nr) = setup("http://t/garbage.jpg");
        let url = web.url.clone();
        // « EOF » trompeur à la fin → tentative → erreur fatale.
        let mut data = vec![0u8; 64];
        data.extend_from_slice(&[0xff, 0xd9]);
        let mut cl = client(3, &web, &data);
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).unwrap().borrow().state,
            DicEntryState::Empty
        );
    }
}
