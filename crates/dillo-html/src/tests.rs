//! Tests de parité — `html.cc`/`entities`/`form.cc`/`plain.cc` :
//! state machine tag-soup, entités, attributs, formulaires et
//! découpage incrémental. Les attentes reflètent le comportement
//! observable amont (messages `BUG_MSG`, flags `InFlags`, reprises
//! de parsing malformé).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dillo_css::styleengine::{StyleEnginePrefs, StyleEngineSink};
use dillo_dw::style::StyleImage;

use dillo_dw::imgbuf::{Imgbuf, ImgbufType};
use dillo_dw::iterator::{DwIterator, EmptyIterator};
use dillo_dw::layout::Layout;
use dillo_dw::platform::{IdleCallback, Platform};
use dillo_dw::style::{Color, Font, FontAttrs, Tooltip};
use dillo_dw::types::Rectangle;
use dillo_dw::types::Requisition;
use dillo_dw::ui::{Resource, ResourceEmitters, ResourceFactory};
use dillo_dw::view::View;
use dillo_net::url::{self, DilloUrl};

use crate::common::{DocType, ImageHandle, in_flags};
use crate::entities::{EntityCtx, parse_entities};
use crate::form::{Method, generate_boundary, str_urlencode_append};
use crate::html::DilloHtml;
use crate::sink::{HtmlPrefs, HtmlSink};
use crate::tags;

// ------------------------------------------------------------------
// Mocks — verbatim des mocks `dillo-dw`/`dillo-css`.
// ------------------------------------------------------------------

struct MockPlatform;

impl Platform for MockPlatform {
    fn set_layout(&mut self, _l: usize) {}
    fn attach_view(&mut self, _v: &mut dyn View) {}
    fn detach_view(&mut self, _v: &mut dyn View) {}
    fn text_width(&mut self, _f: &Font, t: &[u8]) -> i32 {
        4 * i32::try_from(t.len()).unwrap_or_default()
    }
    fn text_to_upper(&mut self, t: &[u8]) -> Vec<u8> {
        t.to_vec()
    }
    fn text_to_lower(&mut self, t: &[u8]) -> Vec<u8> {
        t.to_vec()
    }
    fn next_glyph(&mut self, _t: &[u8], i: i32) -> i32 {
        i + 1
    }
    fn prev_glyph(&mut self, _t: &[u8], i: i32) -> i32 {
        i - 1
    }
    fn dpi_x(&mut self) -> f64 {
        96.0
    }
    fn dpi_y(&mut self) -> f64 {
        96.0
    }
    fn add_idle(&mut self, _l: usize, _cb: IdleCallback) -> i32 {
        0
    }
    fn remove_idle(&mut self, _id: i32) {}
    fn take_pending_idles(&mut self, _l: usize) -> Vec<IdleCallback> {
        Vec::new()
    }
    fn create_font(&mut self, attrs: &FontAttrs, _te: bool) -> Rc<Font> {
        Rc::new(Font {
            attrs: attrs.clone(),
            ascent: 10,
            descent: 2,
            space_width: 4,
            x_height: 5,
            zero_width: 5,
        })
    }
    fn font_exists(&mut self, _n: &[u8]) -> bool {
        true
    }
    fn create_color(&mut self, c: i32) -> Color {
        Color::new(c)
    }
    fn create_tooltip(&mut self, t: &[u8]) -> Rc<Tooltip> {
        Tooltip::create(t, None)
    }
    fn cancel_tooltip(&mut self) {}
    fn create_imgbuf(&mut self, _t: ImgbufType, w: i32, h: i32, _g: f64) -> Rc<dyn Imgbuf> {
        Rc::new(NullImgbuf {
            w,
            h,
            refs: Cell::new(1),
            dou: Cell::new(true),
        })
    }
    fn copy_selection(&mut self, _t: &[u8], _d: i32) {}
    fn get_resource_factory(&mut self) -> Option<Rc<dyn ResourceFactory>> {
        None
    }
}

struct NullImgbuf {
    w: i32,
    h: i32,
    refs: Cell<i32>,
    dou: Cell<bool>,
}

impl Imgbuf for NullImgbuf {
    fn set_cmap(&self, _c: &[i32]) {}
    fn copy_row(&self, _r: i32, _d: &[u8]) {}
    fn new_scan(&self) {}
    fn get_scaled_buf(self: Rc<Self>, _w: i32, _h: i32) -> Rc<dyn Imgbuf> {
        self
    }
    fn get_row_area(&self, _r: i32, _a: &mut Rectangle) {}
    fn get_root_width(&self) -> i32 {
        self.w
    }
    fn get_root_height(&self) -> i32 {
        self.h
    }
    fn width(&self) -> i32 {
        self.w
    }
    fn height(&self) -> i32 {
        self.h
    }
    fn create_similar_buf(&self, w: i32, h: i32) -> Rc<dyn Imgbuf> {
        Rc::new(NullImgbuf {
            w,
            h,
            refs: Cell::new(1),
            dou: Cell::new(true),
        })
    }
    fn rgb_row(&self, _y: i32) -> Vec<u8> {
        vec![0; usize::try_from((3 * self.w).max(0)).unwrap_or_default()]
    }
    fn write_rgb_span(&self, _x: i32, _y: i32, _d: &[u8]) {}
    fn ref_(&self) {
        self.refs.set(self.refs.get() + 1);
    }
    fn unref(&self) {
        self.refs.set(self.refs.get() - 1);
    }
    fn last_reference(&self) -> bool {
        self.refs.get() == 1
    }
    fn set_delete_on_unref(&self, v: bool) {
        self.dou.set(v);
    }
    fn is_referred(&self) -> bool {
        self.refs.get() > 0
    }
}

/// `Resource` minimal pour les tests de widgets embeddés.
struct MockRes {
    embed: Cell<Option<dillo_dw::WidgetId>>,
    emitters: ResourceEmitters,
}

impl MockRes {
    fn new() -> Self {
        MockRes {
            embed: Cell::new(None),
            emitters: ResourceEmitters::new(),
        }
    }
}

impl Resource for MockRes {
    fn as_resource(&self) -> &dyn Resource {
        self
    }
    fn embed(&self) -> Option<dillo_dw::WidgetId> {
        self.embed.get()
    }
    fn set_embed(&self, e: Option<dillo_dw::WidgetId>) {
        self.embed.set(e);
    }
    fn emitters(&self) -> &ResourceEmitters {
        &self.emitters
    }
    fn size_request(&self, _l: &mut Layout, r: &mut Requisition) {
        r.width = 10;
        r.ascent = 10;
        r.descent = 2;
    }
    fn iterator(&self, mask: i16, at_end: bool) -> Box<dyn DwIterator> {
        Box::new(EmptyIterator::new(
            self.embed.get().unwrap_or(0),
            mask,
            at_end,
        ))
    }
    fn is_enabled(&self) -> bool {
        true
    }
    fn set_enabled(&self, _e: bool) {}
}

/// `HtmlSink` mock — enregistre `bug_message`/`set_page_title`/
/// navigations ; le reste en no-op verbatim « interface absente ».
#[derive(Default)]
struct MockSink {
    bugs: Vec<String>,
    titles: Vec<Vec<u8>>,
    msgs: Vec<Vec<u8>>,
    opened: Vec<DilloUrl>,
    redirects: Vec<DilloUrl>,
    pending_css: i32,
    closed: Vec<i32>,
    images: Vec<DilloUrl>,
    removed: i32,
}

impl HtmlSink for MockSink {
    fn bug_message(&mut self, msg: &str) {
        self.bugs.push(msg.to_owned());
    }
    fn set_msg(&mut self, msg: &[u8]) {
        self.msgs.push(msg.to_vec());
    }
    fn set_page_title(&mut self, _url: &DilloUrl, title: &[u8]) {
        self.titles.push(title.to_vec());
    }
    fn open_url(&mut self, url: &DilloUrl) {
        self.opened.push(url.clone());
    }
    fn open_url_nw(&mut self, url: &DilloUrl) {
        self.opened.push(url.clone());
    }
    fn open_url_nt(&mut self, url: &DilloUrl, _f: bool) {
        self.opened.push(url.clone());
    }
    fn redirection0(&mut self, url: DilloUrl) {
        self.redirects.push(url);
    }
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
    fn close_client(&mut self, key: i32) {
        self.closed.push(key);
    }
    fn add_doc(&mut self) {}
    fn remove_doc(&mut self) {
        self.removed += 1;
    }
    fn num_pending_style_sheets(&self) -> i32 {
        self.pending_css
    }
    fn set_num_pending_style_sheets(&mut self, n: i32) {
        self.pending_css = n;
    }
    fn zoom(&self) -> f64 {
        1.0
    }
    fn load_image(&mut self, u: &DilloUrl, _r: Option<&DilloUrl>, _i: &ImageHandle) -> bool {
        self.images.push(u.clone());
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

impl StyleEngineSink for MockSink {
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

fn make_html() -> (Rc<RefCell<MockSink>>, DilloHtml<'static>) {
    let layout = Rc::new(RefCell::new(Layout::new(
        Rc::new(RefCell::new(MockPlatform)),
        true,
    )));
    let sink: Rc<RefCell<MockSink>> = Rc::new(RefCell::new(MockSink::default()));
    let prefs_rc = prefs();
    // `prefs: &'p HtmlPrefs` — le mock vit dans le test (leaked ok
    // en test : la durée de vie est celle du test).
    let prefs_ref: &'static HtmlPrefs = Box::leak(Box::new((*prefs_rc).clone()));
    let url = url::url_new(b"http://example.org/index.html", None).expect("test URL");
    let h = DilloHtml::new(
        layout,
        sink.clone(),
        &url,
        b"text/html",
        prefs_ref,
        prefs_rc,
    );
    (sink, h)
}

/// Régression — le widget toplevel possède le document
/// (`setDeleteCallback(Html_free, this)` amont) : sans
/// [`DilloHtml::arm_widget_delete`] le `Rc` du parseur mourait en fin
/// de fetch avec le cache client → `Weak` des receivers cassé →
/// clics sur liens / submits de forms en no-op silencieux (bug
/// interactif trouvé au live-test, invisible au corpus golden).
#[test]
fn widget_delete_callback_owns_doc() {
    let (sink, h) = make_html();
    let html = Rc::new(RefCell::new(h));
    DilloHtml::arm_widget_delete(&html);
    let (layout, dw, weak_shared) = {
        let b = html.borrow();
        (
            Rc::clone(&b.core.shared.borrow().layout),
            b.dw().expect("toplevel dw"),
            Rc::downgrade(&b.core.shared),
        )
    };
    // ≙ fin de fetch : la closure `call` du cache client est droppée.
    drop(html);
    // Le callback du widget retient le document → `HtmlShared` vivant.
    assert!(weak_shared.upgrade().is_some());
    // Destruction du toplevel (navigation) → `Html_free` ≙ callback
    // → `remove_doc` puis libération réelle du document.
    layout.borrow_mut().widget_delete(dw);
    assert_eq!(sink.borrow().removed, 1);
    assert!(weak_shared.upgrade().is_none());
}

/// Régression A03 passe-2 D7 — `HtmlInput::get_resource` empruntait
/// `layout.borrow()` ; invoqué depuis un receiver qui tourne sous
/// `layout.borrow_mut()` (dispatch `button_event`) → panic `RefCell`.
/// Le lookup passe désormais par l'`Arena` partagée — aucun emprunt
/// du `Layout` n'est requis.
#[test]
fn form_input_get_resource_under_layout_borrow_mut() {
    let (_sink, mut h) = make_html();
    let doc = Rc::new(b"<form><input type=text name=x value=v></form>".to_vec());
    h.write(&doc, true);
    h.finish_parsing(0);
    // MockPlatform n'a pas de ResourceFactory → l'input n'est pas
    // créé par le tag ; on pose le contrôle directement (≙
    // `Html_add_input` hors `<form>` → `inputs_outside_form`).
    let res: Rc<dyn Resource> = Rc::new(MockRes::new());
    let embed = h
        .core
        .shared
        .borrow()
        .layout
        .borrow_mut()
        .add_embed_widget(&res);
    crate::form::add_input(
        &mut h,
        crate::form::InputType::Text,
        Some(embed),
        Some(b"x".to_vec()),
        None,
        false,
        true,
    );
    let input = h.core.inputs_outside_form[0].clone();
    let layout = h.core.shared.borrow().layout.clone();
    // Simule le dispatch `Layout::button_event` : le RefCell du
    // layout est mut-emprunté pendant toute la durée du receiver.
    let _guard = layout.borrow_mut();
    assert!(input.borrow().get_resource().is_some());
}

fn with_ctx<R>(bugs: &mut Vec<String>, f: impl FnOnce(&mut EntityCtx) -> R) -> R {
    let mut report = |m: &str| bugs.push(m.to_owned());
    let mut ctx = EntityCtx {
        doc_type: DocType::Html,
        doc_type_version: 4.01,
        show_extra_warnings: true,
        bug: &mut report,
    };
    f(&mut ctx)
}

// ------------------------------------------------------------------
// tags.rs — `a_Html_tag_index` + table `Tags[]`.
// ------------------------------------------------------------------

#[test]
#[allow(clippy::too_many_lines)]
fn tag_index_known_and_unknown() {
    assert!(tags::tag_index(b"b") >= 0);
    assert!(tags::tag_index(b"BODY") >= 0, "case-insensitive");
    assert_eq!(tags::tag_index(b"zznope"), -1);
    assert_eq!(tags::tag_index(b""), -1);
    // verbatim : la table couvre tous les index `i_*` utilisés.
    for name in [
        "a",
        "abbr",
        "address",
        "area",
        "article",
        "aside",
        "audio",
        "b",
        "base",
        "big",
        "blockquote",
        "body",
        "br",
        "button",
        "center",
        "cite",
        "code",
        "dd",
        "del",
        "dfn",
        "dir",
        "div",
        "dl",
        "dt",
        "em",
        "embed",
        "figcaption",
        "figure",
        "font",
        "footer",
        "form",
        "frame",
        "frameset",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "header",
        "hr",
        "html",
        "i",
        "iframe",
        "img",
        "input",
        "ins",
        "isindex",
        "kbd",
        "label",
        "li",
        "link",
        "main",
        "map",
        "mark",
        "menu",
        "meta",
        "nav",
        "object",
        "ol",
        "optgroup",
        "option",
        "p",
        "pre",
        "q",
        "s",
        "samp",
        "script",
        "section",
        "select",
        "small",
        "source",
        "span",
        "strike",
        "strong",
        "style",
        "sub",
        "sup",
        "table",
        "tbody",
        "td",
        "textarea",
        "tfoot",
        "th",
        "thead",
        "title",
        "tr",
        "tt",
        "u",
        "ul",
        "var",
        "video",
        "wbr",
    ] {
        assert!(tags::tag_index(name.as_bytes()) >= 0, "tag {name} manquant");
    }
}

#[test]
fn tag_compare_verbatim() {
    // `dStrnAsciiCasecmp` tri : nom plus court < nom plus long.
    assert_eq!(
        tags::tag_index(b"blockquote"),
        tags::tag_index(b"BlockQuote")
    );
}

// ------------------------------------------------------------------
// entities.rs — `Html_parse_entity` / `a_Html_parse_entities`.
// ------------------------------------------------------------------

#[test]
fn entities_named_and_numeric() {
    let mut bugs = Vec::new();
    with_ctx(&mut bugs, |ctx| {
        assert_eq!(parse_entities(ctx, b"a &amp; b"), b"a & b");
        assert_eq!(parse_entities(ctx, b"&#65;"), b"A");
        assert_eq!(parse_entities(ctx, b"&#x41;"), b"A");
        assert_eq!(parse_entities(ctx, b"&#X41;"), b"A");
        // `&nbsp;` → U+00A0 (UTF-8 `0xC2 0xA0`).
        assert_eq!(parse_entities(ctx, b"&nbsp;"), b"\xc2\xa0");
    });
    assert_eq!(bugs, [] as [std::string::String; 0]);
}

#[test]
fn entities_literal_ampersand_and_undefined() {
    let mut bugs = Vec::new();
    with_ctx(&mut bugs, |ctx| {
        // `&` littéral → conservé + warning verbatim `Literal '&'`.
        assert_eq!(parse_entities(ctx, b"a & b"), b"a & b");
        // entité inconnue → token recopié tel quel + warning.
        assert_eq!(parse_entities(ctx, b"&nosuch;"), b"&nosuch;");
    });
    assert!(bugs.iter().any(|b| b.contains("Literal '&'")));
    assert!(
        bugs.iter()
            .any(|b| b.contains("Undefined character reference"))
    );
}

#[test]
fn entities_numeric_out_of_range() {
    let mut bugs = Vec::new();
    with_ctx(&mut bugs, |ctx| {
        // `&#0;` → verbatim : rejeté/warning, rien d'inséré.
        let _ = parse_entities(ctx, b"&#0;");
        let _ = parse_entities(ctx, b"&#x110000;");
    });
    assert_ne!(bugs, [] as [std::string::String; 0]);
}

#[test]
fn entities_no_semicolon_legacy() {
    // HTML ≤ 4.01 : `&amp` sans `;` est décodé quand le nom se
    // termine (espace suivant non consommé) — verbatim.
    let mut bugs = Vec::new();
    with_ctx(&mut bugs, |ctx| {
        assert_eq!(parse_entities(ctx, b"a&amp b"), b"a& b");
        // `&ampy` : le nom scanné est `ampy` (inconnu) → littéral.
        assert_eq!(parse_entities(ctx, b"x&ampy"), b"x&ampy");
    });
}

// ------------------------------------------------------------------
// form.rs — encodage query verbatim.
// ------------------------------------------------------------------

#[test]
fn urlencode_verbatim() {
    // `a_Url_encode_hex_str` : alnum + `-_.*` inchangés, espace → `+`.
    let mut d = Vec::new();
    str_urlencode_append(&mut d, b"a b+c/d~e");
    assert_eq!(d, b"a+b%2Bc%2Fd%7Ee");
    // `inputUrlencodeAppend` — `name=value&`.
    let mut d2 = Vec::new();
    super::form::input_urlencode_append(&mut d2, b"q", b"a b");
    assert_eq!(d2, b"q=a+b&");
    // nom vide → rien (verbatim `if (name)`).
    let mut d3 = Vec::new();
    super::form::input_urlencode_append(&mut d3, b"", b"v");
    assert_eq!(d3, [] as [u8; 0]);
}

#[test]
fn multipart_boundary_verbatim() {
    // 70 caractères `[a-zA-Z0-9]` — verbatim.
    let b = generate_boundary();
    assert_eq!(b.len(), 70);
    assert!(b.iter().all(u8::is_ascii_alphanumeric));
}

// ------------------------------------------------------------------
// html.rs — state machine tag-soup.
// ------------------------------------------------------------------

#[test]
fn soup_basic_document() {
    let (sink, mut h) = make_html();
    let doc = Rc::new(b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><html><body><p>hello <b>world</b></p></body></html>".to_vec());
    h.write(&doc, true);
    h.finish_parsing(0);
    // pas de bug pour un document bien formé.
    assert!(sink.borrow().bugs.is_empty(), "{:?}", sink.borrow().bugs);
    assert!(sink.borrow().closed.contains(&0));
}

#[test]
fn soup_implicit_p_close() {
    let (sink, mut h) = make_html();
    // `<p>` auto-fermé par le `<p>` suivant — verbatim, sans bug.
    let doc = Rc::new(
        b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><body><p>one<p>two<p>three".to_vec(),
    );
    h.write(&doc, true);
    h.finish_parsing(0);
    assert!(sink.borrow().bugs.is_empty(), "{:?}", sink.borrow().bugs);
}

#[test]
fn soup_cross_nesting_b_i() {
    let (sink, mut h) = make_html();
    // `</b>` sous `<i>` — verbatim : <i> est fermé implicitement
    // puis <b> fermé ; le `</i>` suivant déclenche un bug "unexpected".
    let doc = Rc::new(
        b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><body><b><i>x</b>y</i>".to_vec(),
    );
    h.write(&doc, true);
    h.finish_parsing(0);
    let bugs = &sink.borrow().bugs;
    assert!(bugs.iter().any(|b| b.contains("Unexpected")), "{bugs:?}");
}

#[test]
fn soup_forbidden_close_li() {
    let (sink, mut h) = make_html();
    // `</li>` hors liste → end_tag 'F' → bug verbatim.
    let doc = Rc::new(b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><body>x</li>".to_vec());
    h.write(&doc, true);
    h.finish_parsing(0);
    assert_ne!(sink.borrow().bugs, [] as [std::string::String; 0]);
}

#[test]
fn soup_li_implicit_close() {
    let (sink, mut h) = make_html();
    // `<li>` auto-fermé par le `<li>` suivant — verbatim.
    let doc = Rc::new(
        b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><body><ul><li>a<li>b</ul>".to_vec(),
    );
    h.write(&doc, true);
    h.finish_parsing(0);
    assert!(sink.borrow().bugs.is_empty(), "{:?}", sink.borrow().bugs);
}

#[test]
fn soup_title_stash() {
    let (sink, mut h) = make_html();
    // `<title>` → stash → `a_UIcmd_set_page_title` verbatim.
    let doc = Rc::new(b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><head><title>My &amp; Page</title></head>".to_vec());
    h.write(&doc, true);
    h.finish_parsing(0);
    assert_eq!(sink.borrow().titles, vec![b"My & Page".to_vec()]);
}

#[test]
fn soup_textarea_verbatim() {
    let (sink, mut h) = make_html();
    // verbatim : le contenu de `<textarea>` est literal — `<b>` ne
    // doit ni ouvrir un élément ni produire de bug.
    let doc = Rc::new(
        b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><body><form action=/x method=get><textarea cols=5 rows=2>a<b>c</textarea></form>".to_vec(),
    );
    h.write(&doc, true);
    h.finish_parsing(0);
    assert!(sink.borrow().bugs.is_empty(), "{:?}", sink.borrow().bugs);
    assert_eq!(h.core.in_flags & in_flags::TEXTAREA, 0);
}

#[test]
fn soup_incremental_token_split() {
    let (sink, mut h) = make_html();
    // découpe un tag entre deux write() — verbatim : le scan attend
    // le `>` dans le buffer suivant (parse incrémental).
    let buf: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(
        b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><body><b".to_vec(),
    ));
    {
        let shared = buf.borrow().clone();
        h.write(&Rc::new(shared), false);
    }
    // même buffer complet cumulé (verbatim Client->Buf).
    buf.borrow_mut().extend_from_slice(b">x</b>y");
    {
        let shared = buf.borrow().clone();
        h.write(&Rc::new(shared), true);
    }
    h.finish_parsing(0);
    assert!(sink.borrow().bugs.is_empty(), "{:?}", sink.borrow().bugs);
}

#[test]
fn soup_eof_closes_open_elements() {
    let (sink, mut h) = make_html();
    // verbatim `finishParsing` : force la fermeture des éléments
    // restés ouverts (`<b><i>` sans close → EOF les ferme).
    let doc =
        Rc::new(b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><body><b><i>x".to_vec());
    h.write(&doc, false);
    h.finish_parsing(0);
    assert!(sink.borrow().closed.contains(&0));
    assert_eq!(h.core.stack.len(), 1, "stack vidée à l'EOF verbatim");
}

#[test]
fn soup_doctype_html5() {
    let (sink, mut h) = make_html();
    // `<!DOCTYPE html>` → DocTypeVersion = 5.0 verbatim.
    let doc = Rc::new(b"<!DOCTYPE html><body>x".to_vec());
    h.write(&doc, true);
    h.finish_parsing(0);
    assert_eq!(h.core.doc_type, DocType::Html);
    assert!((h.core.doc_type_version - 5.0).abs() < f32::EPSILON);
    let _ = sink;
}

#[test]
fn soup_attribute_parsing() {
    let (_sink, mut h) = make_html();
    // quoted, unquoted, bare, entités dans les valeurs.
    assert_eq!(
        h.get_attr(b"<a href='x y' id=z disabled>", b"href"),
        Some(b"x y".to_vec())
    );
    assert_eq!(
        h.get_attr(b"<a href='x y' id=z>", b"id"),
        Some(b"z".to_vec())
    );
    assert_eq!(h.get_attr(b"<a disabled>", b"disabled"), Some(Vec::new()));
    assert_eq!(h.get_attr(b"<a>", b"href"), None);
    // entities décodées dans la valeur (verbatim).
    assert_eq!(
        h.get_attr(b"<a href='a&amp;b'>", b"href"),
        Some(b"a&b".to_vec())
    );
    // case-insensitive.
    assert_eq!(h.get_attr(b"<a HREF=x>", b"href"), Some(b"x".to_vec()));
}

#[test]
fn soup_meta_refresh() {
    let (sink, mut h) = make_html();
    // `<meta http-equiv=refresh content="0; url=...">` →
    // `a_UIcmd_redirection0` verbatim.
    let doc = Rc::new(
        b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><head><meta http-equiv=refresh content=\"0; url=/next.html\"></head>".to_vec(),
    );
    h.write(&doc, true);
    h.finish_parsing(0);
    assert_eq!(sink.borrow().redirects.len(), 1);
    let mut u = sink.borrow().redirects[0].clone();
    assert_eq!(u.str_(), b"http://example.org/next.html");
}

#[test]
fn soup_img_src_triggers_load() {
    let (sink, mut h) = make_html();
    // `<img src>` + `prefs.load_images` → `Html_load_image` via sink.
    let doc = Rc::new(
        b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><body><img src=/i.png alt='pic'>"
            .to_vec(),
    );
    h.write(&doc, true);
    h.finish_parsing(0);
    assert_eq!(sink.borrow().images.len(), 1, "{:?}", sink.borrow().images);
    let mut u = sink.borrow().images[0].clone();
    assert_eq!(u.str_(), b"http://example.org/i.png");
}

#[test]
fn soup_anchor_href_link() {
    let (sink, mut h) = make_html();
    // `<a href>` → lien enregistré ; `tb_set_link` — observable via
    // absence de bug (lien valide) et `name`→anchor.
    let doc = Rc::new(b"<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"><body><a href='/p2'>go</a><a name=t></a></body>".to_vec());
    h.write(&doc, true);
    h.finish_parsing(0);
    assert!(sink.borrow().bugs.is_empty(), "{:?}", sink.borrow().bugs);
}

// ------------------------------------------------------------------
// plain.rs — `DilloPlain`.
// ------------------------------------------------------------------

#[test]
fn plain_write_lines() {
    let layout = Rc::new(RefCell::new(Layout::new(
        Rc::new(RefCell::new(MockPlatform)),
        true,
    )));
    let sink: Rc<RefCell<dyn HtmlSink>> = Rc::new(RefCell::new(MockSink::default()));
    let prefs_rc = prefs();
    let mut plain = crate::plain::DilloPlain::new(sink, layout, &prefs_rc);
    // `buf` = document cumulatif (verbatim `Client->Buf`).
    let all = b"line one\nline two\n".to_vec();
    plain.write(&all, false);
    plain.write(&all, true);
    // vérifie qu'un textblock existe et le lien receiver connecté.
    assert_ne!(plain.dw, usize::MAX);
}

// ------------------------------------------------------------------
// Divers verbatim — `collapse_ws_entities`, `get_attr2` charset,
// `HtmlState`/`stack` invariants.
// ------------------------------------------------------------------

#[test]
fn stack_top_invariant() {
    let (_sink, h) = make_html();
    // `S_TOP` = stack[0] au départ (verbatim : jamais dépilé).
    assert_eq!(h.core.stack.len(), 1);
    assert_eq!(h.s_top().tag_idx, -1);
}

#[test]
fn method_enc_enums() {
    assert_ne!(Method::Get, Method::Post);
}
