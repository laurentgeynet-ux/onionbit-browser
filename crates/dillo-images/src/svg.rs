//! Port de `src/svg.c` — décodeur SVG.
//!
//! Amont : nanosvg (embarqué) — `Svg_write` attend le littéral
//! `"</svg>"` dans le tampon complet, parse avec les unités `"px"`
//! et `Image->dpi`, rastérise en RGBA straight puis compose sur
//! `bg_color` avec `(c*a + bg*(255-a))/0xFF` (division entière).
//!
//! Ici : `resvg`/`usvg` (pur Rust, tiny-skia — déjà le moteur de
//! rendu du projet). Déviations documentées (à couvrir par
//! `audit-09-images`) :
//! - rasteriseur différent : antialiasing/résultat pixel non
//!   bit-identique à nanosvg (naturel — l'audit compare le
//!   comportement observable, pas les pixels) ;
//! - sortie tiny-skia **premultiplied** : on dé-premultiplie puis on
//!   applique la formule de composite amont exacte ;
//! - `nsvgParse(str, "px", dpi, currentColor)` : amont injecte
//!   `Image->fg_color` pour chaque token `currentColor`
//!   (`strncmp(value,"currentColor",12)` — préfixe littéral) ; ici
//!   le token est remplacé textuellement par `#rrggbb` avant le
//!   parse usvg — sémantique identique ;
//! - amont réessaie à chaque `CA_Send` tant que `"</svg>"` manque ;
//!   ici pareil — un SVG tronqué ne produit rien (identique).

use std::cell::RefCell;
use std::rc::Rc;

use dillo_net::cache::{CacheClient, CA_CLOSE, CA_SEND};
use dillo_net::url::DilloUrl;
use resvg::{tiny_skia, usvg};

use crate::dicache::{dicache_close, dicache_set_parms, dicache_write, ImageDecoder};
use crate::image::DilloImage;
use crate::imgbuf::ImgType;

/// `IMAGE_MAX_AREA` (image.hh).
const IMAGE_MAX_AREA: u64 = 6000 * 6000;

/// `DilloSvg` — état du décodeur SVG.
pub struct DilloSvg {
    /// `Image` — retenu jusqu'au `set_parms` puis `NULL` amont
    /// (amont ne le met pas à NULL — mène quirk que webp : les
    /// `set_parms` suivants no-op via `DicState`).
    image: Option<Rc<RefCell<DilloImage>>>,
    /// `url`/`version` — clés dicache.
    url: DilloUrl,
    /// `version`.
    version: i32,
    /// `bgcolor`/`fgcolor` — capturés à `a_Svg_new`.
    bgcolor: i32,
    /// `fgcolor` — ≙ `NSVG_RGB(fg_r, fg_g, fg_b)` : substitué à
    /// chaque token `currentColor` du document.
    fgcolor: i32,
    /// `dpi` — `Image->dpi` passé à `nsvgParse` ≙ `usvg::Options.dpi`.
    dpi: f32,
    /// Amont n'a pas d'état — une fois rendu on ne re-parse pas
    /// (`strstr` retrouverait `</svg>` et re-rasteriserait à chaque
    /// `CA_Send` — quirk amont qu'on ne reproduit pas : la ré-émission
    /// écrirait les mêmes lignes).
    done: bool,
}

impl DilloSvg {
    /// `a_Svg_new`.
    pub fn new(image: Rc<RefCell<DilloImage>>, url: DilloUrl, version: i32) -> Self {
        let (bgcolor, fgcolor, dpi) = {
            let img = image.borrow();
            (img.bg_color, img.fg_color, img.dpi)
        };
        Self {
            image: Some(image),
            url,
            version,
            bgcolor,
            fgcolor,
            dpi,
            done: false,
        }
    }

    /// `Svg_write` — attend `"</svg>"` dans le tampon complet.
    #[allow(clippy::many_single_char_names)]
    fn write(&mut self, buf: &[u8]) {
        if self.done || buf.is_empty() {
            return;
        }
        // `strstr(Buf, "</svg>")` — recherche octet-exacte.
        if !contains_subslice(buf, b"</svg>") {
            return;
        }
        self.done = true;
        // `nsvgParse` prenait `current_color = Image->fg_color` —
        // nanosvg substituait cette couleur à chaque occurrence du
        // token `currentColor` (fill, stroke, fallbacks) ; même
        // résultat par remplacement littéral avant le parse.
        let mut text = String::from_utf8_lossy(buf).into_owned();
        if text.contains("currentColor") {
            let hex = format!("#{:06x}", self.fgcolor & 0xff_ffff);
            text = text.replace("currentColor", &hex);
        }
        let opt = usvg::Options {
            dpi: self.dpi,
            ..usvg::Options::default()
        };
        let Ok(tree) = usvg::Tree::from_str(&text, &opt) else {
            return; // « cannot parse SVG image » amont
        };
        // `unsigned w = nimg->width` — troncature comme le cast C.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (w, h) = (tree.size().width() as u32, tree.size().height() as u32);
        if w == 0 || h == 0 || u64::from(w) > IMAGE_MAX_AREA / u64::from(h) {
            return; // « suspicious image size » amont
        }
        let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
            return;
        };
        // `nsvgRasterizeXY(r, img, 0,0, 1,1, ...)` ≙ transform identité.
        resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
        if let Some(img) = self.image.take() {
            dicache_set_parms(&self.url, self.version, &img, w, h, ImgType::Rgb, 1.0 / 2.2);
        }
        #[allow(clippy::cast_sign_loss)]
        let (br, bg, bb) = (
            ((self.bgcolor >> 16) & 0xff) as u32,
            ((self.bgcolor >> 8) & 0xff) as u32,
            (self.bgcolor & 0xff) as u32,
        );
        let data = pixmap.data(); // RGBA premultiplied
        let stride = w as usize * 4;
        #[allow(clippy::cast_possible_truncation)]
        for y in 0..h as usize {
            let mut line = Vec::with_capacity(3 * w as usize);
            for px in data[y * stride..(y + 1) * stride].chunks_exact(4) {
                // Dé-premultiply → straight, puis formule amont
                // `(c*a + bg*(255-a))/0xFF` (division entière).
                let a = u32::from(px[3]);
                let unmul = |c: u8| u32::from(c) * 255 / a.max(1);
                let (r, g, b) = if a == 0 {
                    (0, 0, 0)
                } else {
                    (unmul(px[0]), unmul(px[1]), unmul(px[2]))
                };
                let na = 0xff - a;
                line.extend_from_slice(&[
                    ((r * a + br * na) / 0xff) as u8,
                    ((g * a + bg * na) / 0xff) as u8,
                    ((b * a + bb * na) / 0xff) as u8,
                ]);
            }
            #[allow(clippy::cast_possible_truncation)]
            dicache_write(&self.url, self.version, &line, y as u32);
        }
    }
}

/// `strstr` octet-exact.
fn contains_subslice(hay: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || hay.len() >= needle.len() && hay.windows(needle.len()).any(|w| w == needle)
}

impl ImageDecoder for DilloSvg {
    /// `a_Svg_callback` : `CA_Send`→`Svg_write`,
    /// `CA_Close`→`Svg_close`, `CA_Abort`→`Svg_free` ≙ drop.
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

/// `a_Svg_new` — factory dicache (`DIC_SVG`).
pub fn svg_new(
    image: Rc<RefCell<DilloImage>>,
    url: DilloUrl,
    version: i32,
) -> Box<dyn ImageDecoder> {
    Box::new(DilloSvg::new(image, url, version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dicache::{
        dicache_callback, dicache_get_entry, dicache_image, dicache_set_factory,
        dicache_set_image_factory, DicEntryState, DIC_LAST, DIC_SVG,
    };
    use crate::image::{ImgRenderer, NullRenderer};
    use dillo_net::cache::{self, CacheClient};
    use dillo_net::http::HttpWeb;

    /// SVG 4×2 : rect rouge plein cadre.
    const SVG_RECT: &str = concat!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='4' height='2'>",
        "<rect width='4' height='2' fill='rgb(255,0,0)'/>",
        "</svg>"
    );

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
        dicache_set_factory(DIC_SVG, svg_new);
        let web = Rc::new(HttpWeb::new(
            dillo_net::url::url_new(url.as_bytes(), None).unwrap(),
            None,
            0,
        ));
        let _ = dicache_image(DIC_SVG, "image/svg+xml", &web).unwrap();
        (web, nr)
    }

    #[test]
    fn solid_rect_renders() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, nr) = setup("http://t/r.svg");
        let url = web.url.clone();
        let mut cl = client(1, &web, SVG_RECT.as_bytes());
        dicache_callback(CA_SEND, &mut cl);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        {
            let eb = e.borrow();
            assert_eq!(eb.state, DicEntryState::Write);
            let ib = eb.imgbuf.as_ref().unwrap();
            assert_eq!((ib.width, ib.height), (4, 2));
            // Rouge plein (antialiasing sur les bords seulement).
            let row = ib.row_copy(0);
            assert!(row.iter().step_by(3).all(|&r| r > 200));
            assert!(row.iter().skip(1).step_by(3).all(|&g| g < 60));
        }
        assert_eq!(nr.borrow().drawn, vec![0, 1]);
        dicache_callback(CA_CLOSE, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).map(|e| e.borrow().state),
            Some(DicEntryState::Close)
        );
    }

    #[test]
    fn waits_for_close_tag() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _nr) = setup("http://t/p.svg");
        let url = web.url.clone();
        // Sans `</svg>` → rien.
        let mut cl = client(2, &web, &SVG_RECT.as_bytes()[..SVG_RECT.len() - 6]);
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).unwrap().borrow().state,
            DicEntryState::Empty
        );
        cl.buf = SVG_RECT.as_bytes().to_vec();
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).unwrap().borrow().state,
            DicEntryState::Write
        );
    }

    #[test]
    fn malformed_svg_emits_nothing() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _nr) = setup("http://t/bad.svg");
        let url = web.url.clone();
        let mut cl = client(3, &web, b"<svg><<<<</svg>");
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(
            dicache_get_entry(&url, DIC_LAST).unwrap().borrow().state,
            DicEntryState::Empty
        );
    }
}
