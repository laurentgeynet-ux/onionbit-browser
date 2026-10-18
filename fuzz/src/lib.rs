//! Harnesses de fuzzing DilloRust — partagés entre les cibles
//! `fuzz_target!` (libFuzzer, nécessite nightly + sanitizer rt) et le
//! runner `smoke` (stable, mutateur déterministe).
//!
//! Propriété invariante testée : **jamais de panic** — Dillo est un
//! logiciel tolérant, toute entrée doit dégrader.

use std::cell::RefCell;
use std::rc::Rc;

// ── HTML ─────────────────────────────────────────────────────────

use dillo_css::styleengine::{StyleEnginePrefs, StyleEngineSink};
use dillo_dw::layout::Layout;
use dillo_dw::platform::Platform;
use dillo_dw::style::StyleImage;
use dillo_html::common::ImageHandle;
use dillo_html::html::DilloHtml;
use dillo_html::sink::{HtmlPrefs, HtmlSink};
use dillo_net::url::{self, DilloUrl};
use dillo_platform::platform::SkiaPlatform;

#[derive(Default)]
struct Sink;

impl HtmlSink for Sink {
    fn bug_message(&mut self, _msg: &str) {}
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
    fn load_image(&mut self, _u: &DilloUrl, _r: Option<&DilloUrl>, _i: &ImageHandle) -> bool {
        false
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

/// `html.cc` via la vraie pipeline `DilloHtml::write` — chunks
/// ≤ 64 o ≙ livraison incrémentale amont.
pub fn html(data: &[u8]) {
    if data.is_empty() {
        return;
    }
    let prefs_rc = Rc::new(HtmlPrefs {
        limit_text_width: true,
        show_tooltip: true,
        show_extra_warnings: false,
        parse_embedded_css: true,
        load_stylesheets: true,
        load_images: false,
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
    });
    let platform: Rc<RefCell<dyn Platform>> = Rc::new(RefCell::new(SkiaPlatform::new()));
    let layout = Rc::new(RefCell::new(Layout::new(platform, true)));
    let sink: Rc<RefCell<dyn HtmlSink>> = Rc::new(RefCell::new(Sink));
    let Some(url) = url::url_new(b"http://fuzz.local/", None) else {
        return;
    };
    let mut html = DilloHtml::new(layout, sink, &url, b"text/html", &prefs_rc, prefs_rc.clone());
    // `Client->Buf` amont est le buffer CUMULATIF du document — on
    // ré-embarque le préfixe complet à chaque write (≙ livraison
    // incrémentale par chunks ≤ 64 o).
    let mut accum: Vec<u8> = Vec::new();
    for chunk in data.chunks(64) {
        accum.extend_from_slice(chunk);
        html.write(&Rc::new(accum.clone()), false);
    }
    html.write(&Rc::new(accum), true);
}

// ── CSS ──────────────────────────────────────────────────────────

use dillo_css::css::{CssContext, CssOrigin, CssPropertyList};
use dillo_css::parser::{CssParser, NullTagIndex};

/// `cssparser.cc` — feuille complète + declaration-block `style=""`.
pub fn css(data: &[u8]) {
    let mut ctx = CssContext::new();
    let mut tags = NullTagIndex;
    CssParser::parse(None, &mut tags, None, &mut ctx, data, CssOrigin::Author);
    let mut props = CssPropertyList::new(true);
    let mut props_i = CssPropertyList::new(true);
    CssParser::parse_declaration_block(None, data, &mut props, &mut props_i);
}

// ── Decode ───────────────────────────────────────────────────────

use dillo_net::decode::{Decode, DecodeTransfer};

const CONTENT_FORMATS: &[&str] = &["gzip", "x-gzip", "deflate", "br"];
const CHARSETS: &[&str] = &[
    "utf-8",
    "iso-8859-1",
    "windows-1251",
    "us-ascii",
    "koi8-r",
    "bogus-charset",
];

/// `decode.c` — transfer `chunked`, content gzip/deflate/br, charset.
pub fn decode(data: &[u8]) {
    let feed = |f: &mut dyn FnMut(&[u8])| {
        for c in data.chunks(37) {
            f(c);
        }
    };
    if let Some(mut d) = DecodeTransfer::new(Some("chunked")) {
        feed(&mut |c| {
            let _ = d.process(c);
        });
    }
    for fmt in CONTENT_FORMATS {
        if let Some(mut d) = Decode::content_init(Some(fmt)) {
            feed(&mut |c| {
                let _ = d.process(c);
            });
        }
    }
    for cs in CHARSETS {
        if let Some(mut d) = Decode::charset_init(Some(cs)) {
            feed(&mut |c| {
                let _ = d.process(c);
            });
        }
    }
}

// ── dpip ─────────────────────────────────────────────────────────

use dillo_net::dpip::{build_cmd, get_attr, get_attr_l};

/// `dpip.c` — parseur de tag wire byte-exact + round-trip
/// `build_cmd`/`get_attr` (couvre l'échappement).
pub fn dpip(data: &[u8]) {
    for attr in [b"cmd".as_slice(), b"msg", b"url", b"a"] {
        let _ = get_attr_l(data, attr);
    }
    if let Ok(s) = std::str::from_utf8(data) {
        for attr in ["cmd", "msg", "url", "cookie", "host"] {
            let _ = get_attr(s, attr);
        }
    }
    let frag = String::from_utf8_lossy(data);
    if let Some(cmd) = build_cmd("cmd=%s arg=%s", &[&frag, "x"]) {
        let _ = get_attr(&cmd, "cmd");
        let _ = get_attr(&cmd, "arg");
    }
}

// ── Fichiers partagés ────────────────────────────────────────────

use dillo_net::cookies::{cookies_freeall, cookies_get_query, cookies_init};
use dillo_net::domain::{domain_freeall, domain_parse, domain_permit};
use dillo_net::hsts::{hsts_freeall, hsts_init, hsts_require_https, hsts_set};
use dillo_net::url::url_new;

/// `cookiesrc` + requête cookie, `hsts` preload + header STS,
/// `domainrc` + permit — formats ligne partagés avec C Dillo.
pub fn files(data: &[u8]) {
    let text = String::from_utf8_lossy(data);

    let _ = cookies_init(Some(&text));
    if let Some(mut u) = url_new(b"http://a.b/", None) {
        let mut noop = |_cmd: &str| -> Option<String> { None };
        let _ = cookies_get_query(&mut u, None, true, &mut noop);
    }
    cookies_freeall();

    hsts_init(true, Some(&text));
    if let Some(mut u) = url_new(b"https://a.b/", None) {
        hsts_set(&text, &mut u, true);
        let _ = hsts_require_https("a.b");
    }
    hsts_freeall();

    domain_parse(&text);
    if let (Some(mut a), Some(mut b)) =
        (url_new(b"http://a.b/", None), url_new(b"http://c.d/", None))
    {
        let _ = domain_permit(&mut a, &mut b);
    }
    domain_freeall();

    // dillorc — parseur tolérant (commentaires, whitespace, clés
    // inconnues ignorées, override PREFS_STRINGS à la 1re occurrence).
    let mut prefs = dillo_browser::prefs::Prefs::default();
    let _ = prefs.parse_dillorc(&mut std::io::Cursor::new(data.to_vec()));

    // keysrc — parseur tolérant (mods `<Ctrl|Shift|…>`, commandes,
    // `nop`, lignes malformées).
    let mut keys = dillo_browser::keys::Keys::new();
    keys.parse(&text);
}

// ── Images ───────────────────────────────────────────────────────

use dillo_browser::ui::xpm_decode;
use dillo_images::dicache::{
    dicache_callback, dicache_freeall, dicache_image, dicache_init, dicache_set_factory,
    dicache_set_image_factory, ImageFactory, DIC_GIF, DIC_JPEG, DIC_PNG, DIC_SVG, DIC_WEBP,
};
use dillo_images::gif::gif_new;
use dillo_images::image::{ImgRenderer, NullRenderer};
use dillo_images::imgbuf::{Imgbuf, ImgbufType};
use dillo_images::jpeg::jpeg_new;
use dillo_images::png::png_new;
use dillo_images::svg::svg_new;
use dillo_images::webp::webp_new;
use dillo_net::cache::{cache_init, CacheClient, CA_CLOSE, CA_SEND};
use dillo_net::http::HttpWeb;

struct FuzzImageFactory;

impl ImageFactory for FuzzImageFactory {
    fn new_image(&mut self, _web: &HttpWeb) -> (Rc<RefCell<dyn ImgRenderer>>, f32) {
        (Rc::new(RefCell::new(NullRenderer::default())), 96.0)
    }
}

/// `gif.c`/`png.c`/`jpeg.c`/`webp.c`/`svg.c` via la vraie boucle
/// `dicache_callback` — feed cumulatif `Client->Buf` par chunks ≤ 64 o
/// (≙ `CA_Send` amont) puis `CA_Close`. Plus `xpm_decode` (icônes
/// embarquées `ui.cc`) et les blits `Imgbuf`.
pub fn images(data: &[u8]) {
    if data.is_empty() {
        return;
    }
    cache_init();
    dicache_init(None);
    dicache_set_image_factory(Box::new(FuzzImageFactory));
    dicache_set_factory(DIC_GIF, gif_new);
    dicache_set_factory(DIC_PNG, png_new);
    dicache_set_factory(DIC_WEBP, webp_new);
    dicache_set_factory(DIC_JPEG, jpeg_new);
    dicache_set_factory(DIC_SVG, svg_new);

    for (fmt, mime) in [
        (DIC_GIF, "image/gif"),
        (DIC_PNG, "image/png"),
        (DIC_WEBP, "image/webp"),
        (DIC_JPEG, "image/jpeg"),
        (DIC_SVG, "image/svg+xml"),
    ] {
        let Some(url) = url_new(format!("http://fuzz.local/i{fmt}.bin").as_bytes(), None)
        else {
            continue;
        };
        let web = Rc::new(HttpWeb::new(url.clone(), None, 0));
        if dicache_image(fmt, mime, &web).is_none() {
            continue;
        }
        let mut client = CacheClient {
            key: 1,
            url,
            version: 0,
            buf: Vec::new(),
            callback: None,
            web,
        };
        let mut accum: Vec<u8> = Vec::new();
        for chunk in data.chunks(64) {
            accum.extend_from_slice(chunk);
            client.buf = accum.clone();
            dicache_callback(CA_SEND, &mut client);
        }
        dicache_callback(CA_CLOSE, &mut client);
    }
    dicache_freeall();

    // `Imgbuf` — blits ligne arbitraires (dims bornées).
    let w = u32::from(data[0]) % 64;
    let h = u32::from(data.get(1).copied().unwrap_or(0)) % 64;
    if w > 0 && h > 0 {
        let ib = Imgbuf::new(ImgbufType::Rgb, w, h, 1.0);
        for y in 0..h.min(8) {
            ib.copy_row(y, &data[2.min(data.len())..]);
        }
        ib.new_scan();
        let _ = ib.data();
    }

    // `xpm_decode` (icônes `ui.cc`) — lignes texte.
    let text = String::from_utf8_lossy(data);
    let lines: Vec<&str> = text.lines().take(64).collect();
    let _ = xpm_decode(&lines);
}

// ── URL ──────────────────────────────────────────────────────────

use dillo_net::url::{decode_hex_str, encode_hex_str};

/// `url.c` — parse lenient, résolution relative, percent-decode
/// tolérant `strtol`, encodage, `url_cmp`.
pub fn url(data: &[u8]) {
    let _ = decode_hex_str(data);
    let _ = encode_hex_str(data);
    let base_bytes: &[u8] = b"http://a.b/dir/page?x=1#frag";
    let Some(base) = url_new(base_bytes, None) else {
        return;
    };
    if let Some(mut u) = url_new(data, None) {
        let _ = u.url_str();
        let _ = u.hostname();
        let _ = u.url_cmp(&base);
    }
    let _ = url_new(data, Some(base_bytes));
}
