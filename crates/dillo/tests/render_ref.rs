//! Golden render suite — `test/html/render/*` amont.
//!
//! Chaque paire `x.html` / `x.ref.html` doit produire un rendu
//! **pixel-identique** (sémantique upstream : le `.ref` est la
//! version de référence équivalente). On rend les deux via le
//! pipeline réel (`DilloHtml` → `StyleEngine` → `Layout` dw →
//! `SkiaView` offscreen tiny-skia) puis `diff_pixmaps`.
//!
//! Cas tolérés : `ALLOWED` liste les paires dont le rendu diffère
//! de façon connue/documentée (à investiguer — pas de `won't fix`
//! permanent sans note).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::case_sensitive_file_extension_comparisons
)] // tests : casts verbatim amont, fixtures en minuscules.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use dillo_css::styleengine::{StyleEnginePrefs, StyleEngineSink};
use dillo_dw::layout::Layout;
use dillo_dw::platform::Platform;
use dillo_dw::style::StyleImage;
use dillo_dw::types::Rectangle;
use dillo_html::common::ImageHandle;
use dillo_html::html::DilloHtml;
use dillo_html::sink::{HtmlPrefs, HtmlSink};
use dillo_images::dicache::{
    dicache_callback, dicache_image, dicache_init, dicache_set_factory, DIC_GIF, DIC_JPEG, DIC_PNG,
    DIC_SVG, DIC_WEBP,
};
use dillo_images::image::image_new;
use dillo_net::cache::{CacheClient, CA_CLOSE, CA_SEND};
use dillo_net::http::HttpWeb;
use dillo_net::url::{self, DilloUrl};
use dillo_platform::platform::SkiaPlatform;
use dillo_platform::view::{SharedView, SkiaView};

/// Canvas amont : `PREFS_GEOMETRY_DEFAULT_*` (prefs.h:22-23) —
/// fenêtre 780×580 ; les barres (15 px, `SCROLLBAR_THICKNESS`
/// fltkviewport.hh:40) sont gérées par `Layout` via
/// `curr_*_scrollbar_thickness` comme amont.
const W: u32 = 780;
const H: u32 = 580;

/// Sink minimal — tout no-op verbatim « interface absente »
/// (cf. `dillo-html/src/tests.rs` `MockSink`).
#[derive(Default)]
struct Sink {
    bugs: Vec<String>,
}

impl HtmlSink for Sink {
    fn bug_message(&mut self, msg: &str) {
        self.bugs.push(msg.to_owned());
    }
    fn set_msg(&mut self, _m: &[u8]) {}
    fn set_page_title(&mut self, _u: &DilloUrl, _t: &[u8]) {}
    fn open_url(&mut self, _u: &DilloUrl) {}
    fn open_url_nw(&mut self, _u: &DilloUrl) {}
    fn open_url_nt(&mut self, _u: &DilloUrl, _f: bool) {}
    fn redirection0(&mut self, _u: DilloUrl) {}
    fn repush(&mut self) {}
    fn page_popup(&mut self, _b: bool, _u: &[DilloUrl]) {}
    fn link_popup(&mut self, _u: &DilloUrl, _p: &DilloUrl) {}
    fn image_popup(&mut self, _u: &DilloUrl, _l: bool, _p: &DilloUrl, _k: Option<&DilloUrl>) {}
    fn form_popup(&mut self, _p: &DilloUrl, _i: usize, _s: bool) {}
    fn bw_id(&self) -> u32 {
        0
    }
    fn expecting(&self) -> bool {
        false
    }
    fn close_client(&mut self, _k: i32) {}
    fn add_doc(&mut self) {}
    fn remove_doc(&mut self) {}
    fn num_pending_style_sheets(&self) -> i32 {
        0
    }
    fn set_num_pending_style_sheets(&mut self, _n: i32) {}
    fn zoom(&self) -> f64 {
        1.0
    }
    /// `Html_load_image` version synchrone : les fixtures `file:` sont
    /// lues depuis `tests/data/render/` et décodées par la chaîne
    /// dicache réelle (≙ `a_Web_new` + `WEB_Image` + capi → dicache).
    fn load_image(&mut self, u: &DilloUrl, _r: Option<&DilloUrl>, i: &ImageHandle) -> bool {
        let Some(name) = u
            .path
            .as_ref()
            .and_then(|p| p.rsplit(|&c| c == b'/').find(|s| !s.is_empty()))
        else {
            return false;
        };
        let name = String::from_utf8_lossy(name);
        let Ok(bytes) = std::fs::read(data_dir().join(name.as_ref())) else {
            return false;
        };
        let (fmt, mime) = match name.rsplit('.').next() {
            Some("gif") => (DIC_GIF, "image/gif"),
            Some("png") => (DIC_PNG, "image/png"),
            Some("webp") => (DIC_WEBP, "image/webp"),
            Some("jpg" | "jpeg") => (DIC_JPEG, "image/jpeg"),
            Some("svg") => (DIC_SVG, "image/svg+xml"),
            _ => return false,
        };
        let (rndr, dpi, bg, fg) = {
            let im = i.borrow();
            let mut lay = im.layout.borrow_mut();
            (
                lay.image_img_renderer(im.widget),
                lay.dpi_x() as f32,
                im.bg_color,
                im.fg_color,
            )
        };
        let di = image_new(0, rndr, bg, fg, dpi);
        let web = Rc::new(HttpWeb::new(u.clone(), None, 0));
        web.bg_color.set(bg);
        *web.image.borrow_mut() = Some(di);
        if dicache_image(fmt, mime, &web).is_none() {
            return false;
        }
        let mut cl = CacheClient {
            key: 1,
            url: u.clone(),
            version: 0,
            buf: bytes,
            callback: None,
            web,
        };
        dicache_callback(CA_SEND, &mut cl);
        dicache_callback(CA_CLOSE, &mut cl);
        true
    }
    fn load_stylesheet(&mut self, _u: &DilloUrl, _r: &DilloUrl) -> bool {
        false
    }
    fn select_file(&mut self) -> Option<String> {
        None
    }
    fn dialog_choice(&mut self, _t: &str, _m: &str, _b: &[&str]) -> i32 {
        0
    }
    fn file_to_string(&mut self, _p: &str) -> Option<Vec<u8>> {
        None
    }
    fn as_style_engine_sink(&mut self) -> &mut dyn StyleEngineSink {
        self
    }
}

impl StyleEngineSink for Sink {
    fn start_background_image_load(
        &mut self,
        _u: &DilloUrl,
        _r: Option<&DilloUrl>,
        _i: &Rc<RefCell<StyleImage>>,
    ) {
    }
}

fn prefs() -> Rc<HtmlPrefs> {
    Rc::new(HtmlPrefs {
        limit_text_width: true,
        show_tooltip: true,
        show_extra_warnings: false,
        parse_embedded_css: true,
        load_stylesheets: true,
        load_images: true,
        mark_unloaded_images: false,
        contrast_visited_color: false,
        middle_click_opens_new_tab: false,
        focus_new_tab: false,
        enterpress_forces_submit: false,
        buffered_drawing: 2,
        style: StyleEnginePrefs {
            font_serif: b"serif".to_vec(),
            font_sans_serif: b"sans".to_vec(),
            font_cursive: b"cursive".to_vec(),
            font_fantasy: b"fantasy".to_vec(),
            font_monospace: b"mono".to_vec(),
            font_factor: 1.0,
            font_min_size: 6,
            font_max_size: 100,
            bg_color: 0x00ff_ffff,
            allow_white_bg: true,
            white_bg_replacement: 0x00d6_dcc0,
            load_background_images: false,
            parse_embedded_css: true,
        },
    })
}

/// Un rendu complet (garde tout vivant : `html`/`layout`/`view`
/// partagent des `Rc`).
struct Rendered {
    _html: DilloHtml<'static>,
    layout: Rc<RefCell<Layout>>,
    view: Rc<RefCell<SkiaView>>,
    _sink: Rc<RefCell<Sink>>,
}

fn render(doc: &[u8]) -> Rendered {
    // Cache d'images frais par rendu (dicache = global thread_local) +
    // décodeurs ≙ `a_Dicache_init` + `a_Mime_init` amont.
    dicache_init(None);
    dicache_set_factory(DIC_GIF, dillo_images::gif::gif_new);
    dicache_set_factory(DIC_PNG, dillo_images::png::png_new);
    dicache_set_factory(DIC_WEBP, dillo_images::webp::webp_new);
    dicache_set_factory(DIC_JPEG, dillo_images::jpeg::jpeg_new);
    dicache_set_factory(DIC_SVG, dillo_images::svg::svg_new);

    let plat_concrete = Rc::new(RefCell::new(SkiaPlatform::new()));
    let platform: Rc<RefCell<dyn Platform>> = plat_concrete.clone();
    let view = Rc::new(RefCell::new(SkiaView::new(&plat_concrete, W, H)));
    let layout = Rc::new(RefCell::new(Layout::new(platform, true)));
    layout
        .borrow_mut()
        .attach_view(Box::new(SharedView::new(&view)));

    let sink = Rc::new(RefCell::new(Sink::default()));
    let prefs_rc = prefs();
    let prefs_ref: &'static HtmlPrefs = Box::leak(Box::new((*prefs_rc).clone()));
    let url = url::url_new(b"file:///page.html", None).expect("test URL");
    let mut html = DilloHtml::new(
        layout.clone(),
        sink.clone(),
        &url,
        b"text/html",
        prefs_ref,
        prefs_rc,
    );
    html.write(&Rc::new(doc.to_vec()), true);
    let dw = html.dw().expect("html should produce a widget");
    {
        let mut l = layout.borrow_mut();
        l.set_widget(dw);
        l.viewport_size_changed(W as i32, H as i32);
        l.drain_idles();
    }
    {
        let mut v = view.borrow_mut();
        let full = Rectangle {
            x: 0,
            y: 0,
            width: W as i32,
            height: H as i32,
        };
        v.fill_bg(&full);
        layout.borrow_mut().draw(&mut *v, &full);
    }
    Rendered {
        _html: html,
        layout,
        view,
        _sink: sink,
    }
}

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/render")
}

/// Paires `x.html`/`x.ref.html` détectées dans le répertoire.
fn pairs() -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(data_dir()).expect("data dir") {
        let p = e.expect("entry").path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".html") || name.ends_with(".ref.html") {
            continue;
        }
        let stem = &name[..name.len() - 5];
        if data_dir().join(format!("{stem}.ref.html")).exists() {
            out.push(stem.to_string());
        }
    }
    out.sort();
    out
}

/// Paires connues pour diverger (chacune doit avoir une raison —
/// pas de `won't fix` silencieux).
const ALLOWED: &[&str] = &[
    // `XFAIL_TESTS` amont (`test/html/Makefile.am`, « To be
    // fixed ») — divergences assumées par Dillo lui-même
    // (margin:auto non supporté, max-width sur <html>, padding
    // inline, largeur % de cellule) ; notre port diverge pour les
    // mêmes raisons.
    "margin-auto",
    "max-width-html",
    "span-padding",
    "table-td-width-percent",
    // `currentColor` est correctement résolu (`fg_color` injecté —
    // svg.rs) ; le diff résiduel est l'antialiasing de bords du
    // rasterizer (resvg ≠ nanosvg embarqué amont — la référence est
    // un PNG pré-rastérisé, jamais bit-identique).
    "svg-current-color",
];

#[test]
fn render_pairs_pixel_identical() {
    let ps = pairs();
    assert!(
        !ps.is_empty(),
        "aucune paire trouvée dans tests/data/render"
    );
    let mut failures = Vec::new();
    for stem in &ps {
        let a_doc = std::fs::read(data_dir().join(format!("{stem}.html"))).unwrap();
        let b_doc = std::fs::read(data_dir().join(format!("{stem}.ref.html"))).unwrap();
        let a = render(&a_doc);
        let b = render(&b_doc);
        let (pa, pb) = (a.view.borrow(), b.view.borrow());
        let (pa, pb) = (pa.pixmap(), pb.pixmap());
        let (Some(pa), Some(pb)) = (pa, pb) else {
            failures.push(format!("{stem}: pixmap indisponible"));
            continue;
        };
        let diff = dillo_platform::golden::diff_pixmaps(&pa, &pb);
        if !diff.is_identical() && !ALLOWED.contains(&stem.as_str()) {
            // `DILLO_GOLDEN_DUMP=<dir>` → écrit `<stem>.got.png` /
            // `<stem>.want.png` pour inspection visuelle.
            if let Ok(dir) = std::env::var("DILLO_GOLDEN_DUMP") {
                let dir = PathBuf::from(dir);
                let _ = std::fs::create_dir_all(&dir);
                let _ = dillo_platform::golden::save_png(&dir.join(format!("{stem}.got.png")), &pa);
                let _ =
                    dillo_platform::golden::save_png(&dir.join(format!("{stem}.want.png")), &pb);
                // Dump de la structure textblock (diagnostic).
                for (tag, r) in [("got", &a), ("want", &b)] {
                    let l = r.layout.borrow();
                    if let Some(top) = l.get_top_level() {
                        let _ =
                            std::fs::write(dir.join(format!("{stem}.{tag}.txt")), l.tb_dump(top));
                    }
                }
            }
            failures.push(format!(
                "{stem}: diff {} px (bbox {:?})",
                diff.diff_pixels, diff.bbox
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "rendus divergents:\n{}",
        failures.join("\n")
    );
}
