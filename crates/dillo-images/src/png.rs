//! Port de `src/png.c` — décodeur PNG.
//!
//! Amont : libpng *progressive* (`png_process_data` + callbacks par
//! ligne — les lignes apparaissent au fil du téléchargement, et les
//! images Adam7 émettent chaque ligne plusieurs fois en rafinant,
//! avec `a_Dicache_new_scan` à chaque retour en arrière de
//! `row_num`). Ici : crate `png` (pull API) — on accumule le tampon
//! réseau et on tente un décodage **complet** à chaque `write` ;
//! `IoError(UnexpectedEof)` ≙ « attendre plus de données ». Quand le
//! décodage réussit, toutes les lignes sont émises d'un coup.
//!
//! Déviations documentées (à couvrir par `audit-09-images`) :
//! - pas d'affichage progressif des lignes pendant le transfert ;
//! - interlaced : une écriture finale par ligne au lieu d'écritures
//!   multi-passes raffinées (`new_scan` inutilisé) ;
//! - gamma : `png_set_gamma(2.2, file_gamma)` amont est reproduit par
//!   une LUT 8-bit après strip-16 (amont corrige en 16-bit avant
//!   strip → différence de précision infime) ;
//! - libpng ignore certains CRC/erreurs bénignes que la crate rejette
//!   (et inversement : la crate est plus stricte sur les chunks
//!   critiques malformés).

use std::cell::RefCell;
use std::io::{Cursor, ErrorKind};
use std::rc::Rc;

use dillo_net::cache::{CacheClient, CA_CLOSE, CA_SEND};
use dillo_net::url::DilloUrl;
use png::{BitDepth, ColorType, Decoder, DecodingError, Transformations};

use crate::dicache::{dicache_close, dicache_set_parms, dicache_write, ImageDecoder};
use crate::image::DilloImage;
use crate::imgbuf::ImgType;

/// `IMAGE_MAX_AREA` (image.hh).
const IMAGE_MAX_AREA: u64 = 6000 * 6000;

/// Signature PNG (8 octets — `png_sig_cmp` amont compare tout le
/// préfixe disponible, borné à 8).
const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// `enum prog_state` : `IS_init`.
const IS_INIT: i32 = 1;
/// `IS_nextdata`.
const IS_NEXTDATA: i32 = 2;
/// `IS_finished`.
const IS_FINISHED: i32 = 0;

/// `DilloPng` — état du décodeur PNG.
pub struct DilloPng {
    /// `Image` — retenu jusqu'au `set_parms` puis `NULL` amont.
    image: Option<Rc<RefCell<DilloImage>>>,
    /// `url`/`version` — clés dicache.
    url: DilloUrl,
    /// `version`.
    version: i32,
    /// `bgcolor` — fond du widget parent pour la composition alpha
    /// (`Image->bg_color` lu dans `a_Png_new`).
    bgcolor: i32,
    /// `state` — FSM `prog_state`.
    state: i32,
}

impl DilloPng {
    /// `a_Png_new` — capture `bg_color` dès la création comme amont.
    pub fn new(image: Rc<RefCell<DilloImage>>, url: DilloUrl, version: i32) -> Self {
        let bgcolor = image.borrow().bg_color;
        Self {
            image: Some(image),
            url,
            version,
            bgcolor,
            state: IS_INIT,
        }
    }

    /// `Png_write` — `Buf` = tampon complet accumulé (≙ `Client->Buf`,
    /// `ipbufstart` implicite : tout est « consommé » comme amont qui
    /// passe `DATASIZE` à `png_process_data`).
    fn write(&mut self, buf: &[u8]) {
        while self.state != IS_FINISHED && !buf.is_empty() {
            match self.state {
                IS_INIT => {
                    if buf.len() < 8 {
                        return; // « need MORE data » amont
                    }
                    if buf[..8] != PNG_SIG[..] {
                        // « "url" is not a PNG file. » → IS_finished
                        self.state = IS_FINISHED;
                        break;
                    }
                    self.state = IS_NEXTDATA;
                }
                IS_NEXTDATA => {
                    self.try_decode(buf);
                    // Amont fait `ipbufstart += DATASIZE` : tout est
                    // consommé (décodeur interne retient le partiel).
                    // Le nôtre re-tente sur `client.buf` complet.
                    break;
                }
                _ => {
                    // « bad state » — amont abort() ; on fige.
                    self.state = IS_FINISHED;
                }
            }
        }
    }

    /// ≙ `png_process_data` + `Png_datainfo_callback` +
    /// `Png_datarow_callback` + `Png_dataend_callback` une fois toutes
    /// les données présentes.
    fn try_decode(&mut self, buf: &[u8]) {
        match self.decode(buf) {
            Ok(Some((width, height, file_gamma, rgb_rows))) => {
                // `Png_datainfo_callback` : `a_Dicache_set_parms` puis
                // `Image = NULL` (safeguard amont).
                if let Some(img) = self.image.take() {
                    dicache_set_parms(
                        &self.url,
                        self.version,
                        &img,
                        width,
                        height,
                        ImgType::Rgb,
                        f64::from(file_gamma),
                    );
                }
                for (y, row) in rgb_rows.iter().enumerate() {
                    #[allow(clippy::cast_possible_truncation)]
                    dicache_write(&self.url, self.version, row, y as u32);
                }
                self.state = IS_FINISHED;
            }
            Ok(None) => {
                // `UnexpectedEof` — attendre plus de données.
            }
            Err(()) => {
                // `Png_error_handling` : error=1, state=finished.
                self.state = IS_FINISHED;
            }
        }
    }

    /// Tente un décodage complet : `Ok(Some)` = image, `Ok(None)` =
    /// tronqué (plus de données à venir), `Err` = erreur fatale
    /// (longjmp amont).
    #[allow(clippy::type_complexity)]
    fn decode(&mut self, buf: &[u8]) -> Result<Option<(u32, u32, f32, Vec<Vec<u8>>)>, ()> {
        let mut decoder = Decoder::new(Cursor::new(buf));
        // ≙ `png_set_expand` (palette/gray<8/tRNS) + `png_set_strip_16`.
        // `png_set_gray_to_rgb` amont ≙ conversion gris→RGB manuelle
        // (GRAY_TO_RGB non exposé par la crate).
        decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
        let mut reader = match decoder.read_info() {
            Ok(r) => r,
            Err(e) => return Self::classify_err(e),
        };
        let file_gamma = reader
            .info()
            .gama_chunk
            .map_or(1.0 / 2.2, png::ScaledFloat::into_value);
        let (width, height) = {
            let i = reader.info();
            (i.width, i.height)
        };
        // Check amont `Png_datainfo_callback`.
        if width == 0 || height == 0 || u64::from(width) > IMAGE_MAX_AREA / u64::from(height) {
            return Err(());
        }
        let mut out = vec![0u8; reader.output_buffer_size()];
        let oinfo = match reader.next_frame(&mut out) {
            Ok(o) => o,
            Err(e) => return Self::classify_err(e),
        };
        if oinfo.bit_depth != BitDepth::Eight {
            return Err(()); // post-EXPAND|STRIP_16 : toujours 8-bit
        }
        let stride = oinfo.line_size;
        // LUT gamma ≙ `png_set_gamma(2.2, file_gamma)` — libpng
        // n'applique la correction que si le chunk gAMA existe.
        let lut = if reader.info().gama_chunk.is_some() {
            Some(gamma_lut(file_gamma))
        } else {
            None
        };
        #[allow(clippy::cast_sign_loss)]
        let (br, bg, bb) = (
            ((self.bgcolor >> 16) & 0xff) as u8,
            ((self.bgcolor >> 8) & 0xff) as u8,
            (self.bgcolor & 0xff) as u8,
        );
        let mut rows = Vec::with_capacity(height as usize);
        for y in 0..height as usize {
            let src = &out[y * stride..(y + 1) * stride];
            rows.push(Self::row_to_rgb(
                src,
                width,
                oinfo.color_type,
                lut.as_ref(),
                (br, bg, bb),
            ));
        }
        Ok(Some((width, height, file_gamma, rows)))
    }

    /// `UnexpectedEof` ≙ données partielles (réseau) ; le reste ≙
    /// `Png_error_handling` (error fatal → longjmp).
    fn classify_err<T>(e: DecodingError) -> Result<Option<T>, ()> {
        match e {
            DecodingError::IoError(io) if io.kind() == ErrorKind::UnexpectedEof => Ok(None),
            _ => Err(()),
        }
    }

    /// `Png_datarow_callback` : channels 3 → copie directe ;
    /// channels 4 → `png_composite` sur `bgcolor` ; gris (post-expand
    /// sans `GRAY_TO_RGB`) → réplication ≙ `png_set_gray_to_rgb`.
    fn row_to_rgb(
        src: &[u8],
        width: u32,
        ct: ColorType,
        lut: Option<&[u8; 256]>,
        bg: (u8, u8, u8),
    ) -> Vec<u8> {
        let w = width as usize;
        let g = |v: u8| lut.map_or(v, |t| t[v as usize]);
        let mut row = Vec::with_capacity(3 * w);
        match ct {
            ColorType::Rgb => {
                for px in src.chunks_exact(3).take(w) {
                    row.extend_from_slice(&[g(px[0]), g(px[1]), g(px[2])]);
                }
            }
            ColorType::Rgba => {
                for px in src.chunks_exact(4).take(w) {
                    let (a, br, bgr, bb) = (px[3], bg.0, bg.1, bg.2);
                    row.extend_from_slice(&[
                        png_composite(g(px[0]), a, br),
                        png_composite(g(px[1]), a, bgr),
                        png_composite(g(px[2]), a, bb),
                    ]);
                }
            }
            ColorType::Grayscale => {
                for &v in src.iter().take(w) {
                    let c = g(v);
                    row.extend_from_slice(&[c, c, c]);
                }
            }
            ColorType::GrayscaleAlpha => {
                for px in src.chunks_exact(2).take(w) {
                    let c = g(px[0]);
                    row.extend_from_slice(&[
                        png_composite(c, px[1], bg.0),
                        png_composite(c, px[1], bg.1),
                        png_composite(c, px[1], bg.2),
                    ]);
                }
            }
            ColorType::Indexed => {
                // Inatteignable post-EXPAND.
                row.resize(3 * w, 0);
            }
        }
        row
    }
}

/// `png_composite` libpng : `t = fg*a + bg*(255-a) + 128 ; (t + t>>8)>>8`.
#[allow(clippy::cast_possible_truncation)]
fn png_composite(fg: u8, a: u8, bg: u8) -> u8 {
    if a == 255 {
        return fg;
    }
    if a == 0 {
        return bg;
    }
    let t = u32::from(fg) * u32::from(a) + u32::from(bg) * (255 - u32::from(a)) + 128;
    ((t + (t >> 8)) >> 8) as u8
}

/// LUT `out = round(255 * (in/255)^(1/(file_gamma*2.2)))` — formule
/// `png_gamma_8bit_correct` de libpng (`screen_gamma` = 2.2 amont).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn gamma_lut(file_gamma: f32) -> [u8; 256] {
    let mut t = [0u8; 256];
    let expo = 1.0 / (f64::from(file_gamma) * 2.2);
    for (i, e) in t.iter_mut().enumerate() {
        *e = ((i as f64 / 255.0).powf(expo) * 255.0 + 0.5) as u8;
    }
    t
}

impl ImageDecoder for DilloPng {
    /// `a_Png_callback` : `CA_Send`→`Png_write`,
    /// `CA_Close`→`Png_close` (`a_Dicache_close` + free ≙ drop),
    /// `CA_Abort`→`Png_free` ≙ drop.
    fn callback(&mut self, op: i32, client: &mut CacheClient) {
        if op == CA_SEND {
            let buf = std::mem::take(&mut client.buf);
            self.write(&buf);
            client.buf = buf;
        } else if op == CA_CLOSE {
            dicache_close(&self.url, self.version, client);
        }
    }
}

/// `a_Png_new` — factory dicache (`DIC_PNG`).
pub fn png_new(
    image: Rc<RefCell<DilloImage>>,
    url: DilloUrl,
    version: i32,
) -> Box<dyn ImageDecoder> {
    Box::new(DilloPng::new(image, url, version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dicache::{
        dicache_callback, dicache_get_entry, dicache_image, dicache_set_factory,
        dicache_set_image_factory, DicEntryState, DIC_LAST, DIC_PNG,
    };
    use crate::image::{ImgRenderer, NullRenderer};
    use dillo_net::cache::{self, CacheClient};
    use dillo_net::http::HttpWeb;
    use png::{BitDepth, ColorType as PngCt, Encoder};

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

    /// Encode un PNG RGB ou RGBA 8-bit via la crate (données de test).
    fn encode_png(w: u32, h: u32, ct: PngCt, raw: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        {
            let mut enc = Encoder::new(&mut data, w, h);
            enc.set_color(ct);
            enc.set_depth(BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            writer.write_image_data(raw).unwrap();
        }
        data
    }

    fn setup(url: &str) -> (Rc<HttpWeb>, Rc<RefCell<NullRenderer>>) {
        let nr = Rc::new(RefCell::new(NullRenderer::default()));
        dicache_set_image_factory(Box::new(FakeFactory(nr.clone())));
        dicache_set_factory(DIC_PNG, png_new);
        let web = Rc::new(HttpWeb::new(
            dillo_net::url::url_new(url.as_bytes(), None).unwrap(),
            None,
            0,
        ));
        let _ = dicache_image(DIC_PNG, "image/png", &web).unwrap();
        (web, nr)
    }

    #[test]
    fn rgb_png_decodes_rows() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, nr) = setup("http://t/rgb.png");
        let url = web.url.clone();
        // 2×2 : row0 = rouge, vert ; row1 = bleu, noir.
        let raw = [
            255, 0, 0, 0, 255, 0, // row 0
            0, 0, 255, 0, 0, 0, // row 1
        ];
        let png = encode_png(2, 2, PngCt::Rgb, &raw);
        let mut cl = client(1, &web, &png);
        dicache_callback(CA_SEND, &mut cl);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        {
            let eb = e.borrow();
            assert_eq!(eb.state, DicEntryState::Write);
            let ib = eb.imgbuf.as_ref().unwrap();
            assert_eq!((ib.width, ib.height), (2, 2));
            assert_eq!(ib.row_copy(0), vec![255, 0, 0, 0, 255, 0]);
            assert_eq!(ib.row_copy(1), vec![0, 0, 255, 0, 0, 0]);
        }
        assert_eq!(nr.borrow().drawn, vec![0, 1]);
        dicache_callback(CA_CLOSE, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).map(|e| e.borrow().state),
            Some(DicEntryState::Close)
        );
    }

    #[test]
    fn truncated_waits_then_decodes() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _nr) = setup("http://t/tr.png");
        let url = web.url.clone();
        let png = encode_png(1, 1, PngCt::Rgb, &[9, 8, 7]);
        // Moitié du fichier → pas encore décodé.
        let mut cl = client(2, &web, &png[..png.len() / 2]);
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).unwrap().borrow().state,
            DicEntryState::Empty
        );
        cl.buf = png;
        dicache_callback(CA_SEND, &mut cl);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        assert_eq!(e.borrow().state, DicEntryState::Write);
        assert_eq!(
            e.borrow().imgbuf.as_ref().unwrap().row_copy(0),
            vec![9, 8, 7]
        );
    }

    #[test]
    fn bad_signature_finishes_silently() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _nr) = setup("http://t/bad.png");
        let url = web.url.clone();
        let mut cl = client(3, &web, b"NOPE1234567890abcdef");
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).unwrap().borrow().state,
            DicEntryState::Empty
        );
    }

    #[test]
    fn rgba_composites_over_bgcolor() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _nr) = setup("http://t/a.png");
        let url = web.url.clone();
        // 1×2 : pixel opaque rouge ; pixel semi-transparent a=128 blanc.
        let raw = [
            255, 0, 0, 255, // opaque → (255,0,0)
            255, 255, 255, 128, // a=128 → composite(255,128,0/255/0)
        ];
        let png = encode_png(2, 1, PngCt::Rgba, &raw);
        let mut cl = client(4, &web, &png);
        dicache_callback(CA_SEND, &mut cl);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        let row = e.borrow().imgbuf.as_ref().unwrap().row_copy(0);
        assert_eq!(&row[..3], &[255, 0, 0]);
        // bg=0 par défaut → composite(255,128,0)=128 sur les 3 canaux.
        // t=255*128+0*127+128=32768 ; (32768+128)>>8=128.
        assert_eq!(&row[3..6], &[128, 128, 128]);
    }

    #[test]
    fn grayscale_replicated_to_rgb() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _nr) = setup("http://t/g.png");
        let url = web.url.clone();
        let png = encode_png(2, 1, PngCt::Grayscale, &[0x11, 0xee]);
        let mut cl = client(5, &web, &png);
        dicache_callback(CA_SEND, &mut cl);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        assert_eq!(
            e.borrow().imgbuf.as_ref().unwrap().row_copy(0),
            vec![0x11, 0x11, 0x11, 0xee, 0xee, 0xee]
        );
    }
}
