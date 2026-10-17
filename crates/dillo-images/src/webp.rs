//! Port de `src/webp.c` — décodeur WebP.
//!
//! Amont : libwebp **incrémentale** (`WebPINewDecoder` +
//! `WebPIUpdate` — les lignes sont émises au fur et à mesure que
//! `last_y` avance). Ici : `image-webp` décode l'image complète —
//! la condition « tout est là » est la taille RIFF du fichier
//! (`len >= 8 + riff_size`), puis une seule passe d'émission.
//!
//! Déviations documentées (à couvrir par `audit-09-images`) :
//! - pas d'affichage progressif des lignes pendant le transfert ;
//! - erreur de `WebPIUpdate` amont : warning + état conservé (les
//!   écritures suivantes réessayent) ; ici une erreur de décodage
//!   fige le décodeur (équivalent observable : rien d'émis).

use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;

use dillo_net::cache::{CacheClient, CA_CLOSE, CA_SEND};
use dillo_net::url::DilloUrl;
use image_webp::WebPDecoder;

use crate::dicache::{dicache_close, dicache_set_parms, dicache_write, ImageDecoder};
use crate::image::DilloImage;
use crate::imgbuf::ImgType;

/// `IMAGE_MAX_AREA` (image.hh) — le check taille amont est fait par
/// `a_Dicache_set_parms` (`Imgbuf` clamp) ; ici même borne explicite
/// que les autres décodeurs.
const IMAGE_MAX_AREA: u64 = 6000 * 6000;

/// `enum prog_state` : `IS_init`.
const IS_INIT: i32 = 1;
/// `IS_nextdata`.
const IS_NEXTDATA: i32 = 2;
/// `IS_finished`.
const IS_FINISHED: i32 = 0;

/// `DilloWebp` — état du décodeur WebP.
pub struct DilloWebp {
    /// `Image` — retenu jusqu'au `set_parms` puis `NULL` amont…
    /// (amont ne le met PAS à NULL ici — quirk : `set_parms` des
    /// appels suivants no-op par le `DicState` interne).
    image: Option<Rc<RefCell<DilloImage>>>,
    /// `url`/`version` — clés dicache.
    url: DilloUrl,
    /// `version`.
    version: i32,
    /// `bgcolor` — `Image->bg_color` capturé à `a_Webp_new`.
    bgcolor: i32,
    /// `state`.
    state: i32,
    /// `y` — `webp->y` : lignes déjà émises (pour ne pas ré-émettre
    /// à chaque `CA_Send` supplémentaire).
    y: u32,
}

impl DilloWebp {
    /// `a_Webp_new`.
    pub fn new(image: Rc<RefCell<DilloImage>>, url: DilloUrl, version: i32) -> Self {
        let bgcolor = image.borrow().bg_color;
        Self {
            image: Some(image),
            url,
            version,
            bgcolor,
            state: IS_INIT,
            y: 0,
        }
    }

    /// `a_Webp_callback` `CA_Send` — `WebPGetFeatures` ≙ parse header
    /// (échec ≙ données partielles → `return` en restant `IS_init`),
    /// `WebPIUpdate` ≙ `read_image` quand le fichier est complet.
    fn write(&mut self, buf: &[u8]) {
        if self.state == IS_FINISHED {
            return;
        }
        if self.state == IS_INIT {
            let Ok(dec) = WebPDecoder::new(Cursor::new(buf)) else {
                return; // header partiel ≙ VP8_STATUS pas OK → return
            };
            let (w, h) = dec.dimensions();
            // Même borne « suspicious size » que les autres décodeurs.
            if w == 0 || h == 0 || u64::from(w) > IMAGE_MAX_AREA / u64::from(h) {
                self.state = IS_FINISHED;
                return;
            }
            if let Some(img) = self.image.take() {
                dicache_set_parms(&self.url, self.version, &img, w, h, ImgType::Rgb, 1.0 / 2.2);
            }
            self.state = IS_NEXTDATA;
        }
        // RIFF : le fichier est complet quand `len >= 8 + riff_size`.
        let complete = buf.len() >= 8
            && u64::from(u32::from_le_bytes(
                buf[4..8].try_into().unwrap_or([0, 0, 0, 0]),
            )) + 8
                <= buf.len() as u64;
        if !complete {
            return;
        }
        self.decode(buf);
    }

    /// `WebPIUpdate` + `WebPIDecGetRGB` + boucle d'émission — version
    /// « fichier entier » : toutes les lignes `[y, height)` d'un coup.
    #[allow(clippy::many_single_char_names)]
    fn decode(&mut self, buf: &[u8]) {
        let Ok(mut dec) = WebPDecoder::new(Cursor::new(buf)) else {
            self.state = IS_FINISHED;
            return;
        };
        let Some(nbytes) = dec.output_buffer_size() else {
            self.state = IS_FINISHED;
            return;
        };
        let has_alpha = dec.has_alpha();
        let (w, h) = dec.dimensions();
        let mut data = vec![0u8; nbytes];
        if dec.read_image(&mut data).is_err() {
            self.state = IS_FINISHED;
            return;
        }
        // `output_buffer` : RGBA (3*? — image-webp émet RGB ou RGBA
        // selon alpha ; stride = w * bpp).
        let bpp = if has_alpha { 4usize } else { 3usize };
        let stride = w as usize * bpp;
        if data.len() < h as usize * stride {
            self.state = IS_FINISHED;
            return;
        }
        #[allow(clippy::cast_sign_loss)]
        let (br, bg, bb) = (
            ((self.bgcolor >> 16) & 0xff) as u32,
            ((self.bgcolor >> 8) & 0xff) as u32,
            (self.bgcolor & 0xff) as u32,
        );
        #[allow(clippy::cast_possible_truncation)]
        while self.y < h {
            let row = &data[self.y as usize * stride..(self.y as usize + 1) * stride];
            if bpp == 4 {
                // Composite amont : `(c*a + bg*(255-a))/0xFF`
                // (division entière, PAS png_composite — quirk).
                let mut line = Vec::with_capacity(3 * w as usize);
                for px in row.chunks_exact(4) {
                    let (r, g, b, a) = (
                        u32::from(px[0]),
                        u32::from(px[1]),
                        u32::from(px[2]),
                        u32::from(px[3]),
                    );
                    let na = 0xff - a;
                    line.extend_from_slice(&[
                        ((r * a + br * na) / 0xff) as u8,
                        ((g * a + bg * na) / 0xff) as u8,
                        ((b * a + bb * na) / 0xff) as u8,
                    ]);
                }
                dicache_write(&self.url, self.version, &line, self.y);
            } else {
                dicache_write(&self.url, self.version, row, self.y);
            }
            self.y += 1;
        }
        self.state = IS_FINISHED;
    }
}

impl ImageDecoder for DilloWebp {
    /// `a_Webp_callback` : `CA_Send`→corps callback,
    /// `CA_Close`→`Webp_close`, `CA_Abort`→`Webp_free` ≙ drop.
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

/// `a_Webp_new` — factory dicache (`DIC_WEBP`).
pub fn webp_new(
    image: Rc<RefCell<DilloImage>>,
    url: DilloUrl,
    version: i32,
) -> Box<dyn ImageDecoder> {
    Box::new(DilloWebp::new(image, url, version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dicache::{
        dicache_callback, dicache_get_entry, dicache_image, dicache_set_factory,
        dicache_set_image_factory, DicEntryState, DIC_LAST, DIC_WEBP,
    };
    use crate::image::{ImgRenderer, NullRenderer};
    use dillo_net::cache::{self, CacheClient};
    use dillo_net::http::HttpWeb;
    use image_webp::{ColorType as WebpCt, WebPEncoder};

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
        dicache_set_factory(DIC_WEBP, webp_new);
        let web = Rc::new(HttpWeb::new(
            dillo_net::url::url_new(url.as_bytes(), None).unwrap(),
            None,
            0,
        ));
        let _ = dicache_image(DIC_WEBP, "image/webp", &web).unwrap();
        (web, nr)
    }

    /// WebP 2×2 lossless RGB.
    fn make_webp() -> Vec<u8> {
        let mut out = Vec::new();
        WebPEncoder::new(&mut out)
            .encode(
                &[255, 0, 0, 0, 255, 0, 0, 0, 255, 10, 20, 30],
                2,
                2,
                WebpCt::Rgb8,
            )
            .unwrap();
        out
    }

    #[test]
    fn lossless_webp_decodes() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, nr) = setup("http://t/p.webp");
        let url = web.url.clone();
        let data = make_webp();
        let mut cl = client(1, &web, &data);
        dicache_callback(CA_SEND, &mut cl);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        {
            let eb = e.borrow();
            assert_eq!(eb.state, DicEntryState::Write);
            let ib = eb.imgbuf.as_ref().unwrap();
            assert_eq!((ib.width, ib.height), (2, 2));
            // VP8L lossless → pixels exacts.
            assert_eq!(ib.row_copy(0), vec![255, 0, 0, 0, 255, 0]);
            assert_eq!(ib.row_copy(1), vec![0, 0, 255, 10, 20, 30]);
        }
        assert_eq!(nr.borrow().drawn, vec![0, 1]);
        dicache_callback(CA_CLOSE, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).map(|e| e.borrow().state),
            Some(DicEntryState::Close)
        );
    }

    #[test]
    fn waits_until_riff_complete() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _nr) = setup("http://t/p2.webp");
        let url = web.url.clone();
        let data = make_webp();
        let half = data.len() / 2;
        let mut cl = client(2, &web, &data[..half]);
        dicache_callback(CA_SEND, &mut cl);
        // Header parsé (set_parms) mais pas de lignes — image-webp a
        // lu le header → set_parms a eu lieu ou pas ; dans les deux
        // cas aucune ligne n'est écrite.
        {
            let e = dicache_get_entry(&url, DIC_LAST).unwrap();
            let eb = e.borrow();
            let ib = eb.imgbuf.as_ref();
            let written = ib.map_or(0, |b| {
                (0..b.height)
                    .filter(|&y| b.row_copy(y).iter().any(|&v| v != 0))
                    .count()
            });
            assert_eq!(written, 0);
        }
        cl.buf = data;
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).unwrap().borrow().state,
            DicEntryState::Write
        );
    }
}
