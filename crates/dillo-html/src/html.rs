//! Port of `src/html.cc` — parseur HTML tag-soup tolérant.
//!
//! `DilloHtml` est la machine d'état verbatim : pile d'états
//! [`HtmlState`], modes `ParseMode`, entités, doctype, ancres,
//! images, liens et glue `StyleEngine`.
//!
//! Le navigateur (`BrowserWindow`) est remplacé par le trait
//! [`crate::sink::HtmlSink`] ; l'image (`DilloImage`) par
//! [`crate::common::ImageHandle`].
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)] // fonctions verbatim monolithiques.
#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

use std::cell::RefCell;
use std::rc::Rc;

use dillo_common::{colors, dlib, utf8};
use dillo_css::css::{
    CssLength, CssLengthType, CssOrigin, CssPropertyName as Prop, CssValueType, css_create_length,
};
use dillo_css::parser::{CssHtmlHooks, TagIndex};
use dillo_css::styleengine::StyleEngine;
use dillo_dw::WidgetId;
use dillo_dw::events::{EventButton, button_state};
use dillo_dw::image::ImageMapsList;
use dillo_dw::layout::{Layout, LinkReceiver};
use dillo_dw::style::Style;
use dillo_net::capi;
use dillo_net::url::{self, DilloUrl};

use crate::common::{
    DocType, HtmlImage, HtmlImageEntry, HtmlState, ImageHandle, ListMode, ParseMode, SGML_SPCDEL,
    TAB_SIZE, TableBorderMode, TableMode, in_flags,
};
use crate::entities::{self, EntityCtx};
use crate::sink::{HtmlPrefs, HtmlSink};
use crate::tags::{self, EndTag, TAGS};

// ------------------------------------------------------------------
// Structures
// ------------------------------------------------------------------

/// Données partagées avec `HtmlLinkReceiver` — verbatim des champs
/// `DilloHtml` lus par `linkReceiver.*` (html.cc:740).
pub struct HtmlShared {
    /// `bw`.
    pub sink: Rc<RefCell<dyn HtmlSink>>,
    /// `bw->render_layout` ≙ `HT2LT` — toutes les ops widget
    /// (`tb_*`, `tbl_*`, `widget_*`, `embed_*`) vivent dessus.
    pub layout: Rc<RefCell<Layout>>,
    /// `page_url`.
    pub page_url: DilloUrl,
    /// `base_url` — mutée par `<base href>` (verbatim).
    pub base_url: DilloUrl,
    /// `links`.
    pub links: Vec<DilloUrl>,
    /// `images`.
    pub images: Vec<HtmlImageEntry>,
    /// `cssUrls`.
    pub css_urls: Vec<DilloUrl>,
    /// `maps` — `Rc` ≙ `dwi->setUseMap(&html->maps, ...)` non possédé.
    pub maps: Rc<RefCell<ImageMapsList>>,
    /// `bw->num_page_bugs` — compteur incrémenté par `bug_message`
    /// (≙ `a_UIcmd_set_bug_prog(bw, ++bw->num_page_bugs)`).
    pub page_bugs: i32,
    /// `visited_color`.
    pub visited_color: i32,
    /// `prefs` partagées avec le receiver (≙ globaux amont).
    pub prefs: Rc<HtmlPrefs>,
}

/// État verbatim de `DilloHtml` hors `styleEngine`/`bw`-shared.
#[allow(clippy::struct_excessive_bools)] // verbatim des champs `DilloHtml`.
pub(crate) struct HtmlCore {
    /// `shared`.
    pub shared: Rc<RefCell<HtmlShared>>,
    /// `stack`.
    pub stack: Vec<HtmlState>,
    /// `dw` — widget courant (dupliqué de `S_TOP->textblock`).
    pub dw: Option<WidgetId>,
    /// `StopParser` — verbatim `stop_parser`.
    pub stop_parser: bool,
    /// `Start_Buf` — buffer complet (pour `getCurrLineNumber`).
    pub start_buf: Option<Rc<Vec<u8>>>,
    /// `Start_Ofs`.
    pub start_ofs: usize,
    /// `CurrOfs`/`OldOfs`/`OldLine`.
    pub curr_ofs: usize,
    /// `OldOfs`.
    pub old_ofs: usize,
    /// `OldLine`.
    pub old_line: usize,
    /// `DocType`.
    pub doc_type: DocType,
    /// `DocTypeVersion`.
    pub doc_type_version: f32,
    /// `InFlags`.
    pub in_flags: u32,
    /// `Stash`.
    pub stash: Vec<u8>,
    /// `StashSpace`.
    pub stash_space: bool,
    /// `pre_column`.
    pub pre_column: i32,
    /// `PreFirstChar`.
    pub pre_first_char: bool,
    /// `PrevWasCR`.
    pub prev_was_cr: bool,
    /// `PrevWasOpenTag` — verbatim (inutilisé amont mais champ présent).
    #[allow(dead_code)]
    pub prev_was_open_tag: bool,
    /// `InVisitedLink`.
    pub in_visited_link: bool,
    /// `ReqTagClose`.
    pub req_tag_close: bool,
    /// `TagSoup`.
    pub tag_soup: bool,
    /// `loadCssFromStash`.
    pub load_css_from_stash: bool,
    /// `PrevWasBodyClose`.
    pub prev_was_body_close: bool,
    /// `PrevWasHtmlClose`.
    pub prev_was_html_close: bool,
    /// `Num_HTML`/`Num_HEAD`/`Num_BODY`/`Num_TITLE` — 3 états {0,1,>1}.
    pub num_html: u8,
    /// `Num_HEAD`.
    pub num_head: u8,
    /// `Num_BODY`.
    pub num_body: u8,
    /// `Num_TITLE`.
    pub num_title: u8,
    /// `attr_data` — verbatim (scratch `Dstr*` amont ; le port
    /// retourne des `Vec<u8>` — champ conservé pour la parité).
    #[allow(dead_code)]
    pub attr_data: Vec<u8>,
    /// `non_css_link_color`.
    pub non_css_link_color: i32,
    /// `non_css_visited_color`.
    pub non_css_visited_color: i32,
    /// `content_type`.
    pub content_type: Vec<u8>,
    /// `charset` — `a_Misc_parse_content_type` (documenté ; utilisé
    /// par `Html_update_content_type` via capi amont).
    pub charset: Option<Vec<u8>>,
    /// `forms` — `Rc` ≙ partage avec `DilloHtmlReceiver` (form.cc).
    pub forms: Vec<Rc<RefCell<crate::form::HtmlForm>>>,
    /// `inputs_outside_form`.
    pub inputs_outside_form: Vec<Rc<RefCell<crate::form::HtmlInput>>>,
    /// Sentinel `S_TOP` — `stack[0]` n'est jamais dépilé amont ;
    /// fallback de dégradation (≙ UB release amont, jamais atteint).
    pub dummy_state: HtmlState,
}

/// `DilloHtml` — verbatim. `se` est séparé de `core` pour les
/// emprunts disjoints (`se.parse(Some(&mut core))` récursif sur
/// `@import`).
pub struct DilloHtml<'p> {
    /// `core` — tout l'état du parseur sauf `se`.
    pub(crate) core: HtmlCore,
    /// `styleEngine`.
    pub(crate) se: StyleEngine<'p>,
    /// `prefs`.
    pub(crate) prefs: &'p HtmlPrefs,
}

// ------------------------------------------------------------------
// HtmlTagIndex — `a_Html_tag_index` pour le `StyleEngine`.
// ------------------------------------------------------------------

/// `TagIndex` vers la table `Tags[]` (seam `a_Html_tag_index`).
pub struct HtmlTagIndex;

impl TagIndex for HtmlTagIndex {
    fn tag_index(&mut self, name: &[u8]) -> i32 {
        tags::tag_index(name)
    }
}

// ------------------------------------------------------------------
// Helpers verbatim de bas niveau
// ------------------------------------------------------------------

/// `dStrnAsciiCasecmp`.
fn strn_icmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    dlib::strn_ascii_casecmp(a, b, n)
}

/// `dStrAsciiCasecmp`.
fn str_icmp(a: &[u8], b: &[u8]) -> i32 {
    dlib::str_ascii_casecmp(a, b)
}

/// `dStriAsciiStr` — index de `needle` dans `haystack` (case-insensitive).
fn stri_str(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    dlib::stri_ascii_str(haystack, needle)
}

impl DilloHtml<'_> {
    /// `BUG_MSG` — `page_bugs` + `a_UIcmd_set_bug_prog` (préfixe
    /// `"HTML warning: line %d, "` verbatim).
    pub(crate) fn bug_message(&mut self, msg: &str) {
        let line = self.get_curr_line_number();
        let full = format!("HTML warning: line {line}, {msg}");
        let mut shared = self.core.shared.borrow_mut();
        shared.page_bugs += 1;
        shared.sink.borrow_mut().bug_message(&full);
    }

    /// `dReturn_val_if_fail` equivalent pour `getCurrLineNumber`.
    pub(crate) fn get_curr_line_number(&mut self) -> i32 {
        // `dReturn_val_if_fail(p != NULL, -1)` + META_HACK.
        if self.core.in_flags & in_flags::META_HACK != 0 {
            return -1;
        }
        let Some(buf) = self.core.start_buf.clone() else {
            return -1;
        };
        let ofs = self.core.curr_ofs.min(buf.len());
        let mut line = self.core.old_line;
        let mut i = self.core.old_ofs.min(ofs);
        while i < ofs {
            if buf[i] == b'\n' || (buf[i] == b'\r' && buf.get(i + 1) != Some(&b'\n')) {
                line += 1;
            }
            i += 1;
        }
        self.core.old_ofs = self.core.curr_ofs;
        self.core.old_line = line;
        line as i32
    }

    /// `S_TOP` — dernier élément de pile (`getRef(size-1)`).
    /// Amont : `stack[0]` n'est jamais dépilé — le fallback `dummy`
    /// reproduit la dégradation release (pas de UB, comportement
    /// défini mais inutilisé).
    #[inline]
    pub(crate) fn s_top(&self) -> &HtmlState {
        self.core.stack.last().unwrap_or(&self.core.dummy_state)
    }

    /// `S_TOP` mutable.
    #[inline]
    pub(crate) fn s_top_mut(&mut self) -> &mut HtmlState {
        self.core
            .stack
            .last_mut()
            .unwrap_or(&mut self.core.dummy_state)
    }

    /// `HT2LT` — le `Layout` du bw.
    pub(crate) fn layout(&self) -> Rc<RefCell<Layout>> {
        Rc::clone(&self.core.shared.borrow().layout)
    }

    /// `html->bw` — le sink.
    pub(crate) fn sink(&self) -> Rc<RefCell<dyn HtmlSink>> {
        Rc::clone(&self.core.shared.borrow().sink)
    }

    /// `html->style ()` — verbatim (`bw` passé pour les callbacks).
    pub(crate) fn style(&mut self) -> Option<Rc<Style>> {
        let layout_rc = self.layout();
        let mut layout = layout_rc.borrow_mut();
        let sink_rc = self.core.shared.borrow().sink.clone();
        let mut sink = sink_rc.borrow_mut();
        self.se
            .style(&mut layout, Some(sink.as_style_engine_sink()))
    }

    /// `html->wordStyle ()`.
    pub(crate) fn word_style(&mut self) -> Option<Rc<Style>> {
        let layout_rc = self.layout();
        let mut layout = layout_rc.borrow_mut();
        let sink_rc = self.core.shared.borrow().sink.clone();
        let mut sink = sink_rc.borrow_mut();
        self.se
            .word_style(&mut layout, Some(sink.as_style_engine_sink()))
    }

    /// `html->backgroundStyle ()`.
    pub(crate) fn background_style(&mut self) -> Option<Rc<Style>> {
        let layout_rc = self.layout();
        let mut layout = layout_rc.borrow_mut();
        let sink_rc = self.core.shared.borrow().sink.clone();
        let mut sink = sink_rc.borrow_mut();
        self.se
            .background_style(&mut layout, Some(sink.as_style_engine_sink()))
    }

    /// `html->restyle ()`.
    pub(crate) fn restyle(&mut self) {
        let layout_rc = self.layout();
        let mut layout = layout_rc.borrow_mut();
        let sink_rc = self.core.shared.borrow().sink.clone();
        let mut sink = sink_rc.borrow_mut();
        let mut bw = Some(sink.as_style_engine_sink());
        self.se.restyle(&mut layout, &mut bw);
    }

    /// `html->startElement (tag_idx)`.
    pub(crate) fn start_element(&mut self, tag: i32) {
        let layout_rc = self.layout();
        let mut layout = layout_rc.borrow_mut();
        let sink_rc = self.core.shared.borrow().sink.clone();
        let mut sink = sink_rc.borrow_mut();
        self.se
            .start_element(tag, &mut layout, Some(sink.as_style_engine_sink()));
    }

    /// `html->styleEngine->endElement` — verbatim privé
    /// (`Html_real_pop_tag` uniquement).
    pub(crate) fn end_element(&mut self) {
        self.se.end_element(self.s_top().tag_idx);
    }

    /// `a_Html_parse_entities` — BUG_MSG collectés puis émis via
    /// `bug_message` (même ligne — `curr_ofs` ne change pas pendant
    /// le parsing d'entités).
    pub(crate) fn parse_entities(&mut self, token: &[u8]) -> Vec<u8> {
        let (r, bugs) = {
            let mut bugs: Vec<String> = Vec::new();
            let mut ctx = EntityCtx {
                doc_type: self.core.doc_type,
                doc_type_version: self.core.doc_type_version,
                show_extra_warnings: self.prefs.show_extra_warnings,
                bug: &mut |m| bugs.push(m.to_string()),
            };
            (entities::parse_entities(&mut ctx, token), bugs)
        };
        for b in &bugs {
            self.bug_message(b);
        }
        r
    }

    /// `Html_parse_entity` (attributs) — même motif collecteur.
    pub(crate) fn parse_entity(&mut self, token: &[u8], is_attr: bool) -> Option<(Vec<u8>, usize)> {
        let (r, bugs) = {
            let mut bugs: Vec<String> = Vec::new();
            let mut ctx = EntityCtx {
                doc_type: self.core.doc_type,
                doc_type_version: self.core.doc_type_version,
                show_extra_warnings: self.prefs.show_extra_warnings,
                bug: &mut |m| bugs.push(m.to_string()),
            };
            (entities::parse_entity(&mut ctx, token, is_attr), bugs)
        };
        for b in &bugs {
            self.bug_message(b);
        }
        r
    }
}

// ------------------------------------------------------------------
// DilloHtml::new + cycle de vie (html.cc:432-662)
// ------------------------------------------------------------------

#[allow(clippy::elidable_lifetime_names)] // `'p` lie `prefs` au struct.
impl<'p> DilloHtml<'p> {
    /// `DilloHtml::DilloHtml` (html.cc:432) — `p_bw` ≙ `sink`,
    /// `HT2LT` ≙ `layout` partagé (le `BrowserWindow` du port est
    /// injecté via [`HtmlSink`]).
    #[allow(clippy::needless_pass_by_value)] // `prefs` ET `prefs_rc` verbatim.
    pub fn new(
        layout: Rc<RefCell<Layout>>,
        sink: Rc<RefCell<dyn HtmlSink>>,
        url: &DilloUrl,
        content_type: &[u8],
        prefs: &'p HtmlPrefs,
        prefs_rc: Rc<HtmlPrefs>,
    ) -> Self {
        // verbatim: page_url/base_url dup, connectLink, a_Bw_add_doc,
        // content_type/charset, stack[0], StyleEngine::new.
        let shared = Rc::new(RefCell::new(HtmlShared {
            sink: Rc::clone(&sink),
            layout: Rc::clone(&layout),
            page_url: url.clone(),
            base_url: url.clone(),
            links: Vec::with_capacity(64),
            images: Vec::with_capacity(16),
            css_urls: Vec::new(),
            maps: Rc::new(RefCell::new(ImageMapsList::new())),
            page_bugs: 0,
            visited_color: -1,
            prefs: prefs_rc,
        }));

        // `HT2LT(this)->connectLink (&linkReceiver)` — verbatim : le
        // receiver tient `Weak<RefCell<HtmlShared>>`.
        let rc = Rc::new(RefCell::new(HtmlLinkReceiver {
            shared: Rc::downgrade(&shared),
        }));
        layout.borrow_mut().connect_link(rc);

        sink.borrow_mut().add_doc();

        let mut tag_idx = HtmlTagIndex;
        let se = StyleEngine::new(
            &mut layout.borrow_mut(),
            Some(url.clone()),
            Some(url.clone()),
            sink.borrow().zoom(),
            &prefs.style,
            &mut tag_idx,
        );

        let mut stack = Vec::with_capacity(16);
        stack.push(HtmlState {
            parse_mode: ParseMode::Init,
            table_mode: TableMode::None,
            table_border_mode: TableBorderMode::Separate,
            cell_text_align_set: false,
            display_none: false,
            list_type: ListMode::None,
            list_number: 0,
            tag_idx: -1, // MUST not be used
            textblock: None,
            table: None,
            ref_list_item: None,
            hand_over_break: false,
        });

        let mut html = DilloHtml {
            core: HtmlCore {
                shared,
                stack,
                dw: None,
                stop_parser: false,
                start_buf: None,
                start_ofs: 0,
                curr_ofs: 0,
                old_ofs: 0,
                old_line: 1,
                doc_type: DocType::None,
                doc_type_version: 0.0,
                in_flags: in_flags::NONE,
                stash: Vec::new(),
                stash_space: false,
                pre_column: 0,
                pre_first_char: false,
                prev_was_cr: false,
                prev_was_open_tag: false,
                in_visited_link: false,
                req_tag_close: false,
                tag_soup: true,
                load_css_from_stash: false,
                prev_was_body_close: false,
                prev_was_html_close: false,
                num_html: 0,
                num_head: 0,
                num_body: 0,
                num_title: 0,
                attr_data: Vec::with_capacity(1024),
                non_css_link_color: -1,
                non_css_visited_color: -1,
                content_type: content_type.to_vec(),
                charset: dillo_common::misc::parse_content_type(Some(content_type)).charset,
                forms: Vec::new(),
                inputs_outside_form: Vec::new(),
                dummy_state: HtmlState {
                    parse_mode: ParseMode::Init,
                    table_mode: TableMode::None,
                    table_border_mode: TableBorderMode::Separate,
                    cell_text_align_set: false,
                    display_none: false,
                    list_type: ListMode::None,
                    list_number: 0,
                    tag_idx: -1,
                    textblock: None,
                    table: None,
                    ref_list_item: None,
                    hand_over_break: false,
                },
            },
            se,
            prefs,
        };

        // `initDw` — widget principal + `setDeleteCallback(Html_free)`.
        html.init_dw();
        html
    }

    /// `initDw` (html.cc:522) — `dw = new Textblock(limit_text_width)`
    /// + `num_page_bugs = 0` + `page_bugs = ""`.
    fn init_dw(&mut self) {
        debug_assert!(self.core.dw.is_none());
        let dw = self
            .core
            .shared
            .borrow()
            .layout
            .borrow_mut()
            .tb_new(self.prefs.limit_text_width, false);
        self.core.dw = Some(dw);
        self.s_top_mut().textblock = Some(dw);
        // `dw->setDeleteCallback(Html_free, this)` ≙ libère le DilloHtml
        // quand le widget est détruit — ici en deux phases : `init_dw`
        // pose un callback faible (le `Rc<DilloHtml>` n'existe pas
        // encore), `arm_widget_delete` le remplace par un callback qui
        // *possède* le document (verbatim : `Html_free` libère `this`).
        let shared = Rc::downgrade(&self.core.shared);
        self.core
            .shared
            .borrow()
            .layout
            .borrow_mut()
            .widget_set_delete_callback(
                dw,
                Some(Rc::new(move || {
                    if let Some(s) = shared.upgrade() {
                        s.borrow().sink.borrow_mut().remove_doc();
                    }
                })),
            );
    }

    /// `dw->setDeleteCallback(Html_free, this)` — phase forte : le
    /// widget toplevel *possède* le document. Amont `Html_free` libère
    /// le `DilloHtml` à la destruction du widget ; ici la closure
    /// retient le `Rc` (le `take` de `run_delete_callback` casse le
    /// cycle pile à la destruction). Sans cette rétention le
    /// `DilloHtml` meurt en fin de fetch avec le cache client → le
    /// `Weak` des receivers (liens, forms) casse et les clics
    /// deviennent des no-ops silencieux.
    pub fn arm_widget_delete(this: &Rc<RefCell<Self>>)
    where
        Self: 'static,
    {
        let (layout, dw) = {
            let h = this.borrow();
            (Rc::clone(&h.core.shared.borrow().layout), h.core.dw)
        };
        if let Some(dw) = dw {
            let doc = Rc::clone(this);
            layout.borrow_mut().widget_set_delete_callback(
                dw,
                Some(Rc::new(move || {
                    if let Ok(h) = doc.try_borrow() {
                        h.core.shared.borrow().sink.borrow_mut().remove_doc();
                    }
                })),
            );
        }
    }

    /// `html->dw` — le widget toplevel (créé par `initDw` au
    /// constructeur ; ≙ retour de `a_Html_text` amont).
    #[must_use]
    pub fn dw(&self) -> Option<WidgetId> {
        self.core.dw
    }

    /// `DilloHtml::write` (html.cc:578) — `buf` = buffer COMPLET du
    /// document jusqu'à présent (verbatim `Client->Buf`/`BufSize`).
    pub fn write(&mut self, buf: &Rc<Vec<u8>>, eof: bool) {
        // `Start_Buf = Buf` — utilisé même après arrêt du parseur.
        self.core.start_buf = Some(Rc::clone(buf));

        if self.core.dw.is_none() || self.core.stop_parser {
            return;
        }
        // `Client->Buf` est cumulatif amont ; un appelant qui passerait
        // un buffer plus court que `start_ofs` ne doit pas paniquer.
        let base = self.core.start_ofs.min(buf.len());
        let bufsize = buf.len() - base;
        let token_start = self.write_raw(&buf[base..base + bufsize], bufsize, eof);
        self.core.start_ofs += token_start;
    }

    /// `DilloHtml::finishParsing` (html.cc:641).
    pub fn finish_parsing(&mut self, client_key: i32) {
        if self.core.stop_parser {
            return;
        }
        self.core.in_flags |= in_flags::EOF;

        // force la fermeture des éléments restés ouverts.
        loop {
            let si = self.core.stack.len().saturating_sub(1);
            if si == 0 {
                break;
            }
            if self.core.stack[si].tag_idx >= 0 {
                self.tag_cleanup_at_close(self.core.stack[si].tag_idx);
            } else {
                // verbatim : boucle while ne sort que via size()==1 ;
                // tag_idx == -1 sans progrès → éviter boucle infinie.
                break;
            }
        }

        self.core.in_flags = in_flags::EOF;
        self.sink().borrow_mut().close_client(client_key);
    }

    // --------------------------------------------------------------
    // Pile d'états (html.cc:1346-1393)
    // --------------------------------------------------------------

    /// `Html_eventually_pop_dw` (html.cc:1346).
    fn eventually_pop_dw(&mut self, hand_over_break: bool) {
        let top_tb = self.s_top().textblock;
        if self.core.dw != top_tb {
            if let Some(dw) = self.core.dw {
                // styles d'abord (`style()` emprunte le layout).
                let style = if hand_over_break { self.style() } else { None };
                let arena_rc = self.core.shared.borrow().layout.clone();
                let mut arena = arena_rc.borrow_mut();
                if let Some(style) = &style {
                    arena.tb_hand_over_break(dw, style);
                }
                arena.tb_flush(dw);
            }
            self.core.dw = top_tb;
        }
    }

    /// `Html_push_tag` (html.cc:1359) — copie l'état du dessus,
    /// change `tag_idx`.
    pub(crate) fn push_tag(&mut self, tag_idx: i32) {
        let mut st = self.s_top().clone();
        st.tag_idx = tag_idx;
        self.core.stack.push(st);
        self.core.dw = self.s_top().textblock;
    }

    /// `Html_force_push_tag` (html.cc:1376).
    pub(crate) fn force_push_tag(&mut self, tag_idx: i32) {
        self.start_element(tag_idx);
        self.push_tag(tag_idx);
    }

    /// `Html_real_pop_tag` (html.cc:1385).
    pub(crate) fn real_pop_tag(&mut self) {
        self.end_element();
        let hand_over_break = self.s_top().hand_over_break;
        self.core.stack.pop();
        self.eventually_pop_dw(hand_over_break);
    }

    /// `a_Html_pop_tag` — verbatim (html.cc:1395+).
    pub fn pop_tag(&mut self, tag_idx: i32) {
        self.tag_cleanup_at_close(tag_idx);
    }

    /// `a_Html_stash_init` (html.cc:853).
    pub fn stash_init(&mut self) {
        self.s_top_mut().parse_mode = ParseMode::Stash;
        self.core.stash_space = false;
        self.core.stash.clear();
    }

    // --------------------------------------------------------------
    // Formulaires — seams `form.cc` (form.hh:140-160).
    // --------------------------------------------------------------

    /// `DilloHtml::formNew` (html.cc:1014) — `enabled` =
    /// `bw->NumPendingStyleSheets == 0` verbatim. Retour =
    /// `forms->size()`.
    pub fn form_new(
        &mut self,
        method: crate::form::Method,
        action: DilloUrl,
        enc: crate::form::Enc,
        charset: Option<Vec<u8>>,
    ) -> usize {
        let enabled = {
            let sh = self.core.shared.borrow();
            sh.sink.borrow().num_pending_style_sheets() == 0
        };
        let form = crate::form::HtmlForm::new(
            method,
            action,
            enc,
            charset,
            enabled,
            self.core.shared.clone(),
        );
        let nf = self.core.forms.len();
        form.borrow_mut().index = nf;
        self.core.forms.push(form);
        nf + 1
    }

    /// `DilloHtml::getCurrentForm` (html.hh:130) —
    /// `forms->get(forms->size()-1)` ≙ dernier formulaire.
    pub(crate) fn get_current_form(&self) -> Option<Rc<RefCell<crate::form::HtmlForm>>> {
        self.core.forms.last().cloned()
    }

    // --------------------------------------------------------------
    // Html_write_raw (html.cc:4341) — scan de tokens incrémental.
    // --------------------------------------------------------------

    /// `Html_write_raw` — retourne `token_start` (nb d'octets
    /// consommés). `buf` = sous-buffer depuis `Start_Ofs`.
    fn write_raw(&mut self, buf: &[u8], bufsize: usize, eof: bool) -> usize {
        let mut buf_index = 0usize;
        let mut token_start = 0usize;

        while buf_index < bufsize && !self.core.stop_parser {
            // invariant : buf_index == bufsize || token_start == buf_index

            if self.s_top().parse_mode == ParseMode::Verbatim {
                // Non-HTML : skip jusqu'à la balise fermante.
                loop {
                    let tag_name = TAGS
                        .get(self.s_top().tag_idx.max(0) as usize)
                        .map_or("", |t| t.name);
                    buf_index += strcspn(&buf[buf_index..bufsize], b"<");
                    if buf_index + tag_name.len() + 3 > bufsize {
                        buf_index = bufsize;
                    } else if buf.get(buf_index..buf_index + 2) == Some(b"</")
                        && tags::match_tag(
                            tag_name.as_bytes(),
                            &buf[buf_index + 2..],
                            tag_name.len() + 1,
                        )
                    {
                        // copie le VERBATIM dans le stash.
                        self.core
                            .stash
                            .extend_from_slice(&buf[token_start..buf_index]);
                        token_start = buf_index;
                        break;
                    } else {
                        buf_index += 1;
                    }
                    if buf_index >= bufsize {
                        break;
                    }
                }
                if buf_index == bufsize {
                    break;
                }
            }

            if buf.get(buf_index).is_some_and(|&c| dlib::is_space(c)) {
                // whitespace : grouper tous les espaces disponibles.
                buf_index += 1;
                while buf_index < bufsize && dlib::is_space(buf[buf_index]) {
                    buf_index += 1;
                }
                self.process_space(&buf[token_start..buf_index]);
                token_start = buf_index;
            } else if buf.get(buf_index) == Some(&b'<')
                && matches!(buf.get(buf_index + 1), Some(&c) if dlib::is_alpha(c) || b"/!?".contains(&c))
            {
                // Tag.
                if buf_index + 3 < bufsize && buf.get(buf_index..buf_index + 4) == Some(b"<!--") {
                    // Commentaire : cherche `-->` (saute tout sauf match).
                    let mut found = false;
                    while let Some(pos) = buf[buf_index..bufsize].iter().position(|&c| c == b'>') {
                        buf_index += pos + 1;
                        if buf_index >= 2
                            && buf.get(buf_index - 2) == Some(&b'-')
                            && buf.get(buf_index - 3) == Some(&b'-')
                        {
                            found = true;
                            break;
                        }
                    }
                    if found {
                        token_start = buf_index;
                    } else {
                        buf_index = bufsize;
                    }
                } else {
                    // Tag : cherche la fin en sautant les strings quotées.
                    self.core.curr_ofs = self.core.start_ofs + token_start;

                    while buf_index < bufsize {
                        buf_index += 1;
                        buf_index += strcspn(&buf[buf_index..bufsize], b">\"'<");
                        let ch = buf.get(buf_index).copied().unwrap_or(0);
                        if ch == b'>' {
                            break;
                        } else if ch == b'"' || ch == b'\'' {
                            // skip la chaîne quotée.
                            buf_index += 1;
                            let set: &[u8] = if ch == b'"' { b"\">" } else { b"'>" };
                            buf_index += strcspn(&buf[buf_index..bufsize], set);
                            if buf.get(buf_index) == Some(&b'>') {
                                // string non terminée ? lookahead :
                                // (< : non terminé, quote : terminé)
                                let mut offset = buf_index + 1;
                                let set2: &[u8] = if ch == b'"' { b"\"<" } else { b"'<" };
                                offset += strcspn(&buf[offset..bufsize], set2);
                                match buf.get(offset) {
                                    Some(&c) if c != ch && c != 0 => {
                                        self.bug_message("Attribute lacks closing quote.");
                                        break;
                                    }
                                    _ => {
                                        buf_index = offset;
                                    }
                                }
                            }
                        } else if ch == b'<' {
                            // tag non terminé détecté.
                            let p_end =
                                strcspn(&buf[(token_start + 1).min(bufsize)..bufsize], b" <\n\r\t");
                            let name = String::from_utf8_lossy(
                                &buf[token_start + 1..token_start + 1 + p_end],
                            );
                            self.bug_message(&format!("<{name}> lacks its closing '>'."));
                            buf_index = buf_index.saturating_sub(1);
                            break;
                        }
                    }
                    if buf_index < bufsize {
                        buf_index += 1;
                        self.process_tag(&buf[token_start..buf_index]);
                        token_start = buf_index;
                    }
                }
            } else {
                // Word : cherche whitespace ou '<' ouvrant.
                self.core.curr_ofs = self.core.start_ofs + token_start;

                buf_index += 1;
                while buf_index < bufsize {
                    buf_index += strcspn(&buf[buf_index..bufsize], b" <\n\r\t\x0c\x0b");
                    if buf.get(buf_index) == Some(&b'<')
                        && matches!(buf.get(buf_index + 1), Some(&c) if !dlib::is_alpha(c) && !b"/!?".contains(&c))
                    {
                        buf_index += 1;
                        continue;
                    }
                    break;
                }
                if buf_index < bufsize || eof {
                    self.process_word(&buf[token_start..buf_index]);
                    token_start = buf_index;
                }
            }
        }

        if let Some(dw) = self.core.dw {
            self.core.shared.borrow().layout.borrow_mut().tb_flush(dw);
        }
        token_start
    }
}

/// `strcspn` — longueur du préfixe sans caractère de `set`.
fn strcspn(s: &[u8], set: &[u8]) -> usize {
    s.iter().position(|c| set.contains(c)).unwrap_or(s.len())
}

// ------------------------------------------------------------------
// Nettoyage de pile (html.cc:3707-3860)
// ------------------------------------------------------------------

impl DilloHtml<'_> {
    /// `Html_tag_cleanup_to_idx` (html.cc:3707) — dépile jusqu'à
    /// `s_idx` **inclus**, en avertissant selon `fi`/`op`.
    ///
    /// - `s_idx` : index de pile cible.
    /// - `new_idx` : tag déclencheur.
    /// - `fi` : index du tag interdit (nesting croisé), `-1` sinon.
    /// - `op` : `'o'` open, `'c'` close.
    fn tag_cleanup_to_idx(&mut self, s_idx: usize, new_idx: i32, fi: i32, op: u8) {
        let ni = new_idx;
        loop {
            let s_top = self.core.stack.len().saturating_sub(1);
            if s_top < s_idx {
                break;
            }
            let toptag_idx = self.s_top().tag_idx;
            let Some(toptag) = TAGS.get(toptag_idx.max(0) as usize).copied() else {
                break;
            };

            if fi >= 0 {
                // nesting interdit
                if toptag_idx != fi {
                    self.bug_message(&format!(
                        "   Nesting cleanup - forcing close of open tag: <{}>.",
                        toptag.name
                    ));
                }
            } else if s_top == s_idx && op == b'c' {
                // tag cible — pas de bug en fermeture.
            } else if toptag.end_tag == EndTag::Optional {
                // fermeture optionnelle — OK.
            } else if (toptag.flags & 4 == 0
                && (TAGS
                    .get(ni.max(0) as usize)
                    .is_some_and(|t| t.flags & 4 != 0 || t.flags & 1 == 0)))
                || (TAGS
                    .get(ni.max(0) as usize)
                    .is_some_and(|t| t.flags & 1 != 0)
                    && toptag.flags & 2 == 0)
            {
                // block {element, container} dans non block container,
                // ou inline dans non inline container.
                let nname = TAGS.get(ni.max(0) as usize).map_or("?", |t| t.name);
                if op == b'o' {
                    self.bug_message(&format!(
                        "Bad nesting:  <{}> can't contain <{}>.  -- closing <{}>.",
                        toptag.name, nname, toptag.name
                    ));
                } else {
                    self.bug_message(&format!(
                        "<{}> needs to be closed before </{}>.  -- closing <{}>.",
                        toptag.name, nname, toptag.name
                    ));
                }
            } else {
                self.bug_message(&format!(
                    "<{}> should have been closed before </{}>.  -- closing <{}>.",
                    toptag.name,
                    TAGS.get(ni.max(0) as usize).map_or("?", |t| t.name),
                    toptag.name
                ));
            }

            if toptag_idx == *tags::I_BODY
                && !(self.core.in_flags & in_flags::EOF != 0 || self.core.req_tag_close)
            {
                // ne dépile {BODY,HTML} que sur EOF ou redondance.
                if s_idx == 1 {
                    self.core.prev_was_html_close = true;
                } else {
                    self.core.prev_was_body_close = true;
                }
                break;
            }
            if let Some(close) = toptag.close {
                close(self);
            }
            self.real_pop_tag();
        }
    }

    /// `Html_stack_cleanup_at_open` (html.cc:3759) — appelé avant
    /// l'ouverture/push d'un nouveau tag.
    fn stack_cleanup_at_open(&mut self, ni: i32) {
        if !self.core.tag_soup {
            return;
        }
        let s_top = self.core.stack.len().saturating_sub(1);
        let fi = tags::forbids_cross_nesting(self.core.in_flags, ni);
        let mut s_idx = s_top as i32;
        while s_idx > 0 {
            let ti = self
                .core
                .stack
                .get(s_idx as usize)
                .map_or(-1, |s| s.tag_idx);

            if fi >= 0 {
                // nesting croisé interdit trouvé
                if ti != fi {
                    s_idx -= 1;
                    continue; // interdit → fermer
                }
                s_idx -= 1;
                self.bug_message(&format!(
                    "Forbidden nesting: <{}> can't contain <{}>.  -- closing <{}>.",
                    TAGS.get(fi.max(0) as usize).map_or("?", |t| t.name),
                    TAGS.get(ni.max(0) as usize).map_or("?", |t| t.name),
                    TAGS.get(fi.max(0) as usize).map_or("?", |t| t.name)
                ));
            } else if self.core.in_flags & in_flags::PRE != 0 && ni == *tags::I_HR {
                break; // listes de répertoires Apache…
            } else if TAGS
                .get(ti.max(0) as usize)
                .is_some_and(|t| t.end_tag == EndTag::Optional)
            {
                if tags::triggers_optional_close(ti, ni) {
                    s_idx -= 1;
                    continue; // fermer
                }
            } else if TAGS
                .get(ni.max(0) as usize)
                .is_some_and(|t| t.flags & 1 == 0)
                && TAGS
                    .get(ti.max(0) as usize)
                    .is_some_and(|t| t.flags & 4 == 0)
            {
                // élément block au-dessus d'un NON block container
                if ti == *tags::I_A && self.core.doc_type_version >= 5.0 {
                    break;
                }
                s_idx -= 1;
                continue; // fermer
            }
            break;
        }

        if s_idx < s_top as i32 {
            self.tag_cleanup_to_idx((s_idx + 1).max(0) as usize, ni, fi, b'o');
        }
    }

    /// `Html_tag_cleanup_at_close` (html.cc:3811) — recherche du tag
    /// correspondant en fermant les optionnels/croisés/inline.
    ///
    /// Quirk verbatim : `Tags[stack_idx]` (index pile, pas `tag_idx`)
    /// pour le test "inline element or container" — bug amont
    /// reproduit (borne : `None` → pas inline → `expected`).
    pub(crate) fn tag_cleanup_at_close(&mut self, new_idx: i32) {
        let Some(new_tag) = TAGS.get(new_idx.max(0) as usize).copied() else {
            return;
        };
        let mut matched = false;
        let mut expected = false;
        let mut expected_tag_idx = 0i32;
        let mut stack_idx = self.core.stack.len();
        while {
            stack_idx = stack_idx.saturating_sub(1);
            stack_idx != 0
        } {
            let tag_idx = self.core.stack.get(stack_idx).map_or(-1, |s| s.tag_idx);
            if tag_idx == new_idx {
                matched = true;
                break;
            }
            // QUIRK verbatim sur le 3e test : `Tags[stack_idx]` amont
            // — index pile utilisé comme index Tags (hors borne →
            // None → pas inline → `expected`, dégradation définie).
            let skip = TAGS
                .get(tag_idx.max(0) as usize)
                .is_some_and(|t| t.end_tag == EndTag::Optional)
                || (new_idx == *tags::I_A && self.core.in_flags & in_flags::A != 0)
                || (new_idx == *tags::I_BUTTON && self.core.in_flags & in_flags::BUTTON != 0)
                || (new_idx == *tags::I_SELECT && self.core.in_flags & in_flags::SELECT != 0)
                || (new_idx == *tags::I_TEXTAREA && self.core.in_flags & in_flags::TEXTAREA != 0)
                || (new_tag.flags & 4 != 0
                    && TAGS.get(stack_idx).is_some_and(|t| t.flags & 3 != 0));
            if !skip {
                expected = true;
                expected_tag_idx = tag_idx;
                break;
            }
        }

        if matched {
            self.tag_cleanup_to_idx(stack_idx, new_idx, -1, b'c');
        } else if expected {
            self.bug_message(&format!(
                "Unexpected closing tag: </{}> -- expected </{}>.",
                new_tag.name,
                TAGS.get(expected_tag_idx.max(0) as usize)
                    .map_or("?", |t| t.name)
            ));
        } else {
            self.bug_message(&format!("Unexpected closing tag: </{}>.", new_tag.name));
        }
    }

    /// `Html_test_section` (html.cc:3850) — `<html>`/`<head>`/`<body>`
    /// à ouverture/fermeture optionnelles.
    fn test_section(&mut self, new_idx: i32, is_close_tag: bool) {
        if self.core.in_flags & in_flags::HTML == 0 && self.core.doc_type == DocType::None {
            self.bug_message("The required DOCTYPE declaration is missing. Handling as HTML4.");
        }

        if self.core.in_flags & in_flags::HTML == 0 {
            let tag_idx = tags::tag_index(b"html");
            if tag_idx != new_idx || is_close_tag {
                // open implicite
                self.force_push_tag(tag_idx);
                if let Some(t) = TAGS.get(tag_idx.max(0) as usize) {
                    (t.open)(self, b"<html>");
                }
            }
        }

        let Some(new_tag) = TAGS.get(new_idx.max(0) as usize).copied() else {
            return;
        };
        if new_tag.flags & 16 != 0 {
            // élément head
            if self.core.in_flags & in_flags::HEAD == 0 && self.core.num_head == 0 {
                let tag_idx = tags::tag_index(b"head");
                if tag_idx != new_idx || is_close_tag {
                    self.force_push_tag(tag_idx);
                    if let Some(t) = TAGS.get(tag_idx.max(0) as usize) {
                        (t.open)(self, b"<head>");
                    }
                }
            }
        } else if new_tag.flags & 8 != 0 {
            // élément body
            if self.core.in_flags & in_flags::HEAD != 0 {
                let tag_idx = tags::tag_index(b"head");
                self.tag_cleanup_at_close(tag_idx);
            }
            let tag_idx = tags::tag_index(b"body");
            if tag_idx != new_idx || is_close_tag {
                self.force_push_tag(tag_idx);
                if let Some(t) = TAGS.get(tag_idx.max(0) as usize) {
                    (t.open)(self, b"<body>");
                }
            }
        }
    }
}

// ------------------------------------------------------------------
// Parseur d'attributs (html.cc:4196-4315)
// ------------------------------------------------------------------

/// `HTML_LeftTrim` / `HTML_RightTrim` / `HTML_ParseEntities` —
/// `DilloHtmlTagParsingFlags` (html.hh).
pub mod tag_parsing_flags {
    /// `HTML_LeftTrim`.
    pub const LEFT_TRIM: u32 = 1;
    /// `HTML_RightTrim`.
    pub const RIGHT_TRIM: u32 = 2;
    /// `HTML_ParseEntities`.
    pub const PARSE_ENTITIES: u32 = 4;
}

/// États de `Html_get_attr2` (`DilloHtmlTagParsingState`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AttrState {
    /// `SEEK_ATTR_START`.
    SeekAttrStart,
    /// `MATCH_ATTR_NAME`.
    MatchAttrName,
    /// `SEEK_TOKEN_START`.
    SeekTokenStart,
    /// `SEEK_VALUE_START`.
    SeekValueStart,
    /// `SKIP_VALUE`.
    SkipValue,
    /// `GET_VALUE`.
    GetValue,
    /// `FINISHED`.
    Finished,
}

impl DilloHtml<'_> {
    /// `Html_get_attr2` (html.cc:4196) — machine à états verbatim.
    /// Retourne `Some(valeur)` (peut être vide) ou `None` si absent.
    /// `tag` inclut les chevrons ; `tagsize` = `tag.len()`.
    pub(crate) fn get_attr2(&mut self, tag: &[u8], attrname: &[u8], flags: u32) -> Option<Vec<u8>> {
        if attrname.is_empty() {
            return None;
        }
        let tagsize = tag.len();
        let mut buf: Vec<u8> = Vec::new();
        let mut found = false;
        let mut delimiter = 0u8;
        let mut attr_pos = 0usize;
        let mut state = AttrState::SeekAttrStart;

        let mut i = 1usize;
        while i < tagsize {
            match state {
                AttrState::SeekAttrStart => {
                    if dlib::is_space(tag[i]) {
                        state = AttrState::SeekTokenStart;
                    } else if tag[i] == b'=' {
                        state = AttrState::SeekValueStart;
                    }
                }
                AttrState::MatchAttrName => {
                    if attr_pos == attrname.len()
                        && (tag[i] == b'=' || dlib::is_space(tag[i]) || tag[i] == b'>')
                    {
                        found = true;
                        state = AttrState::SeekTokenStart;
                        i = i.saturating_sub(1);
                    } else if tag[i] == 0 {
                        // NUL interdit dans un nom d'attribut.
                        state = AttrState::SeekAttrStart;
                    } else {
                        if dlib::ascii_to_lower(tag[i])
                            != dlib::ascii_to_lower(attrname.get(attr_pos).copied().unwrap_or(0))
                        {
                            state = AttrState::SeekAttrStart;
                        }
                        attr_pos += 1;
                    }
                }
                AttrState::SeekTokenStart => {
                    if tag[i] == b'=' {
                        state = AttrState::SeekValueStart;
                    } else if !dlib::is_space(tag[i]) {
                        attr_pos = 0;
                        state = if found {
                            AttrState::Finished
                        } else {
                            AttrState::MatchAttrName
                        };
                        i = i.saturating_sub(1);
                    }
                }
                AttrState::SeekValueStart => {
                    if !dlib::is_space(tag[i]) {
                        delimiter = if tag[i] == b'"' || tag[i] == b'\'' {
                            tag[i]
                        } else {
                            b' '
                        };
                        if delimiter == b' ' {
                            i = i.saturating_sub(1);
                        }
                        state = if found {
                            AttrState::GetValue
                        } else {
                            AttrState::SkipValue
                        };
                    }
                }
                AttrState::SkipValue => {
                    if (delimiter == b' ' && dlib::is_space(tag[i])) || tag[i] == delimiter {
                        state = AttrState::SeekTokenStart;
                    }
                }
                AttrState::GetValue => {
                    if (delimiter == b' ' && (dlib::is_space(tag[i]) || tag[i] == b'>'))
                        || tag[i] == delimiter
                    {
                        state = AttrState::Finished;
                    } else if tag[i] == b'&' && flags & tag_parsing_flags::PARSE_ENTITIES != 0 {
                        if let Some((entstr, entsize)) = self.parse_entity(&tag[i..tagsize], true) {
                            buf.extend_from_slice(&entstr);
                            i += entsize.saturating_sub(1);
                        } else {
                            buf.push(tag[i]);
                        }
                    } else if tag[i] == b'\r' || tag[i] == b'\t' {
                        buf.push(b' ');
                    } else if tag[i] == b'\n' {
                        // ignoré
                    } else {
                        buf.push(tag[i]);
                    }
                }
                AttrState::Finished => {
                    i = tagsize;
                    continue;
                }
            }
            i += 1;
        }

        if flags & tag_parsing_flags::LEFT_TRIM != 0 {
            while buf.first().is_some_and(|&c| dlib::is_space(c)) {
                buf.remove(0);
            }
        }
        if flags & tag_parsing_flags::RIGHT_TRIM != 0 {
            while buf.last().is_some_and(|&c| dlib::is_space(c)) {
                buf.pop();
            }
        }

        if found { Some(buf) } else { None }
    }

    /// `a_Html_get_attr` (html.cc:4300) — `get_attr2` avec
    /// trim + entités. `attr_data` scratch ≙ retour `Vec<u8>`.
    pub fn get_attr(&mut self, tag: &[u8], attrname: &[u8]) -> Option<Vec<u8>> {
        self.get_attr2(
            tag,
            attrname,
            tag_parsing_flags::LEFT_TRIM
                | tag_parsing_flags::RIGHT_TRIM
                | tag_parsing_flags::PARSE_ENTITIES,
        )
    }

    /// `a_Html_get_attr_wdef` (html.cc:4318) — valeur ou `def`
    /// (`None` ≙ `NULL` amont : l'absence reste `None`).
    pub fn get_attr_wdef(
        &mut self,
        tag: &[u8],
        attrname: &[u8],
        def: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        self.get_attr(tag, attrname)
            .or_else(|| def.map(<[u8]>::to_vec))
    }

    /// `Html_check_name_val` (html.cc:1486) — vérifie `id`/`name`
    /// (`[A-Za-z][A-Za-z0-9:_.-]*` ou HTML5 non-vide-sans-espace).
    pub(crate) fn check_name_val(&mut self, val: &[u8], attrname: &str) -> i32 {
        if self.core.doc_type == DocType::Html && self.core.doc_type_version >= 5.0 {
            let valid = !val.is_empty() && !val.contains(&b' ');
            if !valid {
                self.bug_message(&format!(
                    "'{}' value \"{}\" must not be empty and must not contain spaces.",
                    attrname,
                    String::from_utf8_lossy(val)
                ));
            }
            i32::from(valid)
        } else {
            let mut i = 0usize;
            while i < val.len()
                && (dlib::is_ascii(val[i]) && (dlib::is_alnum(val[i]) || b":_.-".contains(&val[i])))
            {
                i += 1;
            }
            let ok = i == val.len()
                && val
                    .first()
                    .is_some_and(|&c| dlib::is_ascii(c) && dlib::is_alpha(c));
            if !ok {
                self.bug_message(&format!(
                    "{} attribute value \"{}\" is not of the form '[A-Za-z][A-Za-z0-9:_.-]*'.",
                    attrname,
                    String::from_utf8_lossy(val)
                ));
            }
            i32::from(ok)
        }
    }

    /// `Html_parse_common_attrs` (html.cc:3916) — `id`/`class`/`style`/
    /// `xml:lang`/`lang` → `StyleEngine`.
    fn parse_common_attrs(&mut self, tag: &[u8]) {
        let tagsize = tag.len();

        if tagsize >= 8
            && let Some(attrbuf) = self.get_attr(tag, b"id")
        {
            self.check_name_val(&attrbuf, "id");
            self.se.set_id(&attrbuf);
        }

        if tagsize >= 11 && (self.prefs.parse_embedded_css || self.prefs.load_stylesheets) {
            if let Some(attrbuf) = self.get_attr(tag, b"class") {
                self.se.set_class(&attrbuf);
            }
            if let Some(attrbuf) = self.get_attr(tag, b"style") {
                self.se.set_style(Some(&attrbuf));
            }
        }

        // "xml:lang" prévaut sur "lang" — 2 premiers chars (BCP 47).
        let mut lang = [0u8; 2];
        if tagsize >= 14
            && let Some(attrbuf) = self.get_attr(tag, b"xml:lang")
        {
            let n = attrbuf.len().min(2);
            lang[..n].copy_from_slice(&attrbuf[..n]);
        }
        if lang[0] == 0
            && tagsize >= 10
            && let Some(attrbuf) = self.get_attr(tag, b"lang")
        {
            let n = attrbuf.len().min(2);
            lang[..n].copy_from_slice(&attrbuf[..n]);
        }
        if lang[0] != 0 {
            let n = if lang[1] == 0 { 1 } else { 2 };
            self.se
                .set_non_css_hint_str(Prop::PropertyXLang, CssValueType::String, &lang[..n]);
        }
    }
}

// ------------------------------------------------------------------
// Doctype + helpers divers (html.cc:1530, 3085, 292-316, 393-424)
// ------------------------------------------------------------------

/// `EI_set[]` (html.cc:1455) — éléments exclus de `<pre>` pré-HTML5.
static EI_SET: std::sync::LazyLock<Vec<i32>> = std::sync::LazyLock::new(|| {
    [
        "img", "object", "applet", "big", "small", "sub", "sup", "font", "basefont",
    ]
    .iter()
    .map(|s| tags::tag_index(s.as_bytes()))
    .collect()
});

impl DilloHtml<'_> {
    /// `Html_parse_doctype` (html.cc:1530) — sanitization verbatim
    /// (collapse whitespace, `\n`/`\r` → `' '` dans les strings
    /// quotées), puis reconnaissance des signatures connues.
    fn parse_doctype(&mut self, tag: &[u8]) {
        const HTML_SGML_SIG: &[u8] = b"<!DOCTYPE HTML PUBLIC ";
        const HTML20: &[u8] = b"-//IETF//DTD HTML";
        const HTML32: &[u8] = b"-//W3C//DTD HTML 3.2";
        const HTML40: &[u8] = b"-//W3C//DTD HTML 4.0";
        const HTML401: &[u8] = b"-//W3C//DTD HTML 4.01";
        const HTML401_URL: &[u8] = b"http://www.w3.org/TR/html4/";
        const XHTML1: &[u8] = b"-//W3C//DTD XHTML 1.0";
        const XHTML1_URL: &[u8] = b"http://www.w3.org/TR/xhtml1/DTD/";
        const XHTML11: &[u8] = b"-//W3C//DTD XHTML 1.1";
        const XHTML11_URL: &[u8] = b"http://www.w3.org/TR/xhtml11/DTD/";

        // `dStrndup` ≙ tronque au premier NUL.
        let ntag_src = {
            let nul = tag.iter().position(|&c| c == 0).unwrap_or(tag.len());
            &tag[..nul]
        };
        // Tag sanitization : collapse whitespace entre tokens et
        // remplace '\n'/'\r' par ' ' dans les strings quotées.
        let mut ntag: Vec<u8> = Vec::with_capacity(ntag_src.len());
        let mut p = 0usize;
        while p < ntag_src.len() && ntag_src[p] != 0 {
            let c = ntag_src[p];
            if dlib::is_space(c) {
                ntag.push(b' ');
                while p + 1 < ntag_src.len() && dlib::is_space(ntag_src[p + 1]) {
                    p += 1;
                }
            } else if c == b'"' || c == b'\'' {
                let quote = c;
                ntag.push(c);
                p += 1;
                loop {
                    let q = if p < ntag_src.len() { ntag_src[p] } else { 0 };
                    ntag.push(q);
                    if q == 0 || q == quote {
                        break;
                    }
                    if q == b'\n' || q == b'\r' {
                        *ntag.last_mut().unwrap_or(&mut 0) = b' ';
                        if q == b'\r' && p + 1 < ntag_src.len() && ntag_src[p + 1] == b'\n' {
                            p += 1;
                        }
                    }
                    p += 1;
                }
                // upstream laisse `p` sur le caractère suivant le
                // quote de fermeture (`*p` relu en fin de boucle for).
                if p < ntag_src.len() && ntag_src[p] != 0 {
                    // le `for` amont incrémente `p` — déjà fait par le
                    // `p += 1` du prochain tour.
                    continue;
                }
                break;
            } else {
                ntag.push(c);
            }
            if p >= ntag_src.len() || ntag_src[p] == 0 {
                break;
            }
            p += 1;
        }
        // retire le NUL terminal éventuel poussé par la branche quote.
        while ntag.last() == Some(&0) {
            ntag.pop();
        }

        if self.core.doc_type != DocType::None {
            self.bug_message("Multiple DOCTYPE declarations.");
        }

        // Le type par défaut DT_NONE est TagSoup.
        if ntag.len() > HTML_SGML_SIG.len()
            && strn_icmp(&ntag, HTML_SGML_SIG, HTML_SGML_SIG.len()) == 0
        {
            let ppos = HTML_SGML_SIG.len() + 1;
            let p = if ppos < ntag.len() {
                &ntag[ppos..]
            } else {
                &[][..]
            };
            if p.starts_with(HTML401) && stri_str(&p[HTML401.len()..], HTML401_URL).is_some() {
                self.core.doc_type = DocType::Html;
                self.core.doc_type_version = 4.01;
            } else if p.starts_with(XHTML1) && stri_str(&p[XHTML1.len()..], XHTML1_URL).is_some() {
                self.core.doc_type = DocType::Xhtml;
                self.core.doc_type_version = 1.0;
            } else if p.starts_with(XHTML11) && stri_str(&p[XHTML11.len()..], XHTML11_URL).is_some()
            {
                self.core.doc_type = DocType::Xhtml;
                self.core.doc_type_version = 1.1;
            } else if p.starts_with(HTML40) {
                self.core.doc_type = DocType::Html;
                self.core.doc_type_version = 4.0;
            } else if p.starts_with(HTML32) {
                self.core.doc_type = DocType::Html;
                self.core.doc_type_version = 3.2;
            } else if p.starts_with(HTML20) {
                self.core.doc_type = DocType::Html;
                self.core.doc_type_version = 2.0;
            }
        } else if str_icmp(&ntag, b"<!DOCTYPE html>") == 0
            || str_icmp(&ntag, b"<!DOCTYPE html >") == 0
            || str_icmp(&ntag, b"<!DOCTYPE html SYSTEM \"about:legacy-compat\">") == 0
            || str_icmp(&ntag, b"<!DOCTYPE html SYSTEM 'about:legacy-compat'>") == 0
        {
            self.core.doc_type = DocType::Html;
            self.core.doc_type_version = 5.0;
        }
        if self.core.doc_type == DocType::None {
            self.core.doc_type = DocType::Unrecognized;
            self.bug_message(&format!(
                "DOCTYPE not recognized: ('{}').",
                String::from_utf8_lossy(&ntag)
            ));
        }
    }

    /// `Html_tag_pre_excludes` (html.cc:3085) — éléments interdits
    /// dans `<pre>` (sauf HTML5 qui ne dit rien).
    #[allow(clippy::float_cmp)] // verbatim `>= 5.0`/`== 5.0` amont.
    fn tag_pre_excludes(&self, tag_idx: i32) -> bool {
        if self.core.doc_type == DocType::Html && self.core.doc_type_version >= 5.0 {
            return false;
        }
        EI_SET.contains(&tag_idx)
    }

    /// `Html_set_link_coordinates` (html.cc:292) — `?x,y` ismap.
    pub fn set_link_coordinates(&mut self, link: i32, x: i32, y: i32) {
        if x != -1
            && let Some(u) = self
                .core
                .shared
                .borrow_mut()
                .links
                .get_mut(link.max(0) as usize)
        {
            let data = format!("?{x},{y}");
            u.set_ismap_coords(data.as_bytes());
        }
    }

    /// `Html_set_new_link` (html.cc:306) — push dans `links`,
    /// retourne l'index.
    pub(crate) fn set_new_link(&mut self, url: Option<DilloUrl>) -> i32 {
        let mut shared = self.core.shared.borrow_mut();
        let nl = shared.links.len();
        if let Some(u) = url {
            shared.links.push(u);
        }
        i32::try_from(nl).unwrap_or(i32::MAX)
    }

    /// `a_Html_url_new` (html.cc:172) — `a_Url_new` relatif à
    /// `base_url` + warnings bytes illégaux.
    pub fn url_new(
        &mut self,
        url_str: Option<&[u8]>,
        base_url: Option<&[u8]>,
        use_base_url: bool,
    ) -> Option<DilloUrl> {
        let Some(url_str) = url_str else {
            tracing::warn!("a_Html_url_new: URL is NULL");
            return None;
        };
        let base = if use_base_url {
            base_url.map(<[u8]>::to_vec)
        } else {
            Some(self.core.shared.borrow_mut().base_url.str_().to_vec())
        };
        let url = url::url_new(url_str, base.as_deref());
        let Some(url) = url else {
            self.bug_message(&format!(
                "URL is not valid '{}'.",
                String::from_utf8_lossy(url_str)
            ));
            return None;
        };
        let n_ic = url.illegal_chars;
        if n_ic != 0 {
            let suffix = if n_ic > 1 { "s" } else { "" };
            let n_ic_spc = url.illegal_chars_spc;
            if n_ic == n_ic_spc {
                self.bug_message(&format!(
                    "URL has {} illegal space{} ('{}').",
                    n_ic,
                    suffix,
                    String::from_utf8_lossy(url_str)
                ));
            } else if n_ic_spc == 0 {
                self.bug_message(&format!(
                    "URL has {} illegal byte{} in {{00-1F, 7F-FF}} range ('{}').",
                    n_ic,
                    suffix,
                    String::from_utf8_lossy(url_str)
                ));
            } else {
                self.bug_message(&format!(
                    "URL has {} illegal byte{}: {} space{} and {} in {{00-1F, 7F-FF}} range ('{}').",
                    n_ic, suffix, n_ic_spc,
                    if n_ic_spc > 1 { "s" } else { "" },
                    n_ic - n_ic_spc,
                    String::from_utf8_lossy(url_str)
                ));
            }
        }
        Some(url)
    }

    /// `Html_get_javascript_link` (html.cc:2666) — extrait la
    /// référence entre quotes d'un lien `javascript:...`.
    /// Verbatim : mutait `attr_data` ; le port prend l'attrbuf.
    pub(crate) fn get_javascript_link(&mut self, attrbuf: &[u8]) -> Vec<u8> {
        if strn_icmp(b"javascript", attrbuf, 10) == 0 {
            let i = attrbuf
                .iter()
                .position(|&c| c == b'\'' || c == b'"')
                .unwrap_or(attrbuf.len());
            let ch = attrbuf.get(i).copied().unwrap_or(0);
            if (ch == b'"' || ch == b'\'')
                && let Some(rel) = attrbuf[i + 1..].iter().position(|&c| c == ch)
            {
                self.bug_message("Link depends on javascript().");
                return attrbuf[i + 1..i + 1 + rel].to_vec();
            }
        }
        attrbuf.to_vec()
    }

    /// `Html_add_textblock` (html.cc:393).
    pub(crate) fn add_textblock(
        &mut self,
        add_breaks: bool,
        break_space: i32,
        add_break_opt: bool,
    ) {
        let ltw = self.prefs.limit_text_width;
        let style = if add_breaks {
            let Some(s) = self.style() else { return };
            let mut attrs = s.attrs.clone();
            attrs.display = dillo_dw::style::DisplayType::Block;
            Some(Style::create(&attrs))
        } else {
            self.style()
        };
        let Some(style) = style else { return };

        let Some(dw) = self.core.dw else { return };
        // styles d'abord (`word_style()`/`style()` empruntent le layout).
        let ws = if add_breaks { self.word_style() } else { None };
        let s2 = if add_break_opt { self.style() } else { None };
        let arena_rc = self.core.shared.borrow().layout.clone();
        let mut arena = arena_rc.borrow_mut();
        if let Some(ws) = &ws {
            arena.tb_add_parbreak(dw, break_space, ws);
        }
        let textblock = arena.tb_new(ltw, false);
        arena.tb_add_widget(dw, textblock, &style);
        if let Some(s2) = &s2 {
            arena.tb_add_break_option(dw, s2, false);
        }
        if let Some(ws) = &ws {
            arena.tb_add_parbreak(dw, break_space, ws);
        }
        drop(arena);
        self.s_top_mut().textblock = Some(textblock);
        self.core.dw = Some(textblock);
        if add_breaks {
            self.s_top_mut().hand_over_break = true;
        }
    }

    /// `Html_must_add_breaks` (html.cc:424).
    pub(crate) fn must_add_breaks(&mut self) -> bool {
        self.style()
            .is_some_and(|s| dillo_dw::oof::must_add_breaks(&s.attrs))
    }

    /// `Html_add_anchor` (html.cc:2689).
    pub(crate) fn add_anchor(&mut self, name: &[u8]) {
        let (Some(dw), Some(style)) = (self.core.dw, self.style()) else {
            return;
        };
        let ok = self
            .core
            .shared
            .borrow()
            .layout
            .borrow_mut()
            .tb_add_anchor(dw, name, &style);
        if !ok {
            self.bug_message(&format!(
                "Anchor names must be unique within the document (\"{}\").",
                String::from_utf8_lossy(name)
            ));
        }
    }
}

// ------------------------------------------------------------------
// Display helpers (html.cc:3989-4050)
// ------------------------------------------------------------------

impl DilloHtml<'_> {
    /// `Html_display_block`.
    fn display_block(&mut self) {
        let mab = self.must_add_breaks();
        self.add_textblock(mab, 0, false);
    }

    /// `Html_display_inline_block`.
    fn display_inline_block(&mut self) {
        self.add_textblock(false, 0, true);
    }

    /// `Html_display_listitem` (html.cc:4001) — le numéro et
    /// `ref_list_item` vivent dans l'état du PARENT (size-2).
    fn display_listitem(&mut self) {
        let Some(style) = self.style() else { return };
        let Some(word_style) = self.word_style() else {
            return;
        };
        let Some(dw) = self.core.dw else { return };

        // parent state (verbatim `stack->getRef(size()-2)`).
        let parent_idx = self.core.stack.len().saturating_sub(2);
        let ref_item = self
            .core
            .stack
            .get(parent_idx)
            .and_then(|s| s.ref_list_item);

        let ltw = self.prefs.limit_text_width;
        let arena_rc = self.core.shared.borrow().layout.clone();
        let mut arena = arena_rc.borrow_mut();
        arena.tb_add_parbreak(dw, 0, &word_style);
        let list_item = arena.listitem_new(ref_item, ltw);
        arena.tb_add_widget(dw, list_item, &style);
        arena.tb_add_parbreak(dw, 0, &word_style);
        drop(arena);

        if let Some(s) = self.core.stack.get_mut(parent_idx) {
            s.ref_list_item = Some(list_item);
        }
        self.s_top_mut().textblock = Some(list_item);
        self.core.dw = Some(list_item);

        let lt = style.attrs.list_style_type;
        let arena_rc = self.core.shared.borrow().layout.clone();
        let mut arena = arena_rc.borrow_mut();
        if lt == dillo_dw::style::ListStyleType::None {
            // none
        } else if lt >= dillo_dw::style::ListStyleType::Decimal {
            // ordered
            let num = self.core.stack.get_mut(parent_idx).map_or(0, |s| {
                let n = s.list_number;
                s.list_number += 1;
                n
            });
            let buf = dillo_dw::style::num_to_str(num, lt);
            arena.listitem_init_with_text(list_item, buf.as_bytes(), &word_style);
        } else {
            // unordered
            let bullet = arena.bullet_new();
            arena.listitem_init_with_widget(list_item, bullet, &word_style);
        }
    }

    /// `a_Html_should_display` (html.cc:4040).
    pub fn should_display(&mut self) -> bool {
        if self.s_top().display_none {
            return false;
        }
        !matches!(
            self.style().map(|s| s.attrs.display),
            Some(dillo_dw::style::DisplayType::None)
        )
    }
}

// ------------------------------------------------------------------
// Html_process_tag (html.cc:4056)
// ------------------------------------------------------------------

impl DilloHtml<'_> {
    /// `Html_process_tag` — dispatch open/close verbatim.
    /// `tag` inclut les chevrons (`<tag ...>` ou `</tag ...>`).
    fn process_tag(&mut self, tag: &[u8]) {
        if self.core.stop_parser {
            return;
        }
        let tagsize = tag.len();
        let start = 1usize; // skip '<'
        let is_close_tag = tag.get(start) == Some(&b'/');
        let name_start = start + usize::from(is_close_tag);

        let ni = tags::tag_index(&tag[name_start.min(tagsize)..]);
        if ni == -1 {
            // doctype parsing fuzzy — verbatim.
            if self.core.in_flags & in_flags::HTML == 0
                && tagsize > 9
                && strn_icmp(tag, b"<!doctype", 9) == 0
            {
                self.parse_doctype(tag);
            }
            // tags inconnus ignorés.
            return;
        }

        if !is_close_tag && self.core.doc_type == DocType::Html && self.core.doc_type_version >= 5.0
        {
            let mut bugs: Vec<String> = Vec::new();
            tags::check_html5_obsolete(ni, |m| bugs.push(m.to_string()));
            for b in &bugs {
                self.bug_message(b);
            }
        }

        let i = if self.core.prev_was_html_close {
            1
        } else if self.core.prev_was_body_close {
            2
        } else {
            0
        };
        if i == 1 || (i == 2 && ni != *tags::I_HTML) {
            self.bug_message(&format!(
                "Content after </{}> tag.",
                if i == 1 { "html" } else { "body" }
            ));
        }
        self.core.prev_was_html_close = false;
        self.core.prev_was_body_close = false;

        // HTML, HEAD et BODY — open/close optionnels.
        if self.core.in_flags & in_flags::BODY == 0 {
            self.test_section(ni, is_close_tag);
        }

        // Tag processing.
        let ti = self.s_top().tag_idx;
        if !is_close_tag {
            // open.
            if ti != -1 {
                self.stack_cleanup_at_open(ni);
            }

            // warning seulement — l'élément est autorisé (Apache IMG
            // dans PRE).
            if self.core.in_flags & in_flags::PRE != 0 && self.tag_pre_excludes(ni) {
                self.bug_message(&format!(
                    "<pre> is not allowed to contain <{}>.",
                    TAGS.get(ni.max(0) as usize).map_or("?", |t| t.name)
                ));
            }

            self.push_tag(ni);
            self.start_element(ni);
            self.parse_common_attrs(tag);

            if let Some(t) = TAGS.get(ni.max(0) as usize) {
                (t.open)(self, tag);
            }

            if !self.s_top().display_none {
                let disp = self.style().map(|s| s.attrs.display);
                let vloat = self.style().map(|s| s.attrs.vloat);
                match disp {
                    Some(dillo_dw::style::DisplayType::Block) => {
                        self.display_block();
                    }
                    Some(dillo_dw::style::DisplayType::InlineBlock) => {
                        self.display_inline_block();
                    }
                    Some(dillo_dw::style::DisplayType::ListItem) => {
                        self.display_listitem();
                    }
                    Some(dillo_dw::style::DisplayType::None) => {
                        self.s_top_mut().display_none = true;
                    }
                    Some(dillo_dw::style::DisplayType::Inline)
                        if !matches!(vloat, Some(dillo_dw::style::FloatType::None) | None) =>
                    {
                        self.display_block();
                    }
                    _ => {}
                }

                if !self.s_top().display_none
                    && let Some(content) = TAGS.get(ni.max(0) as usize).and_then(|t| t.content)
                {
                    content(self, tag);
                }
            }

            if !self.core.stop_parser {
                if self.s_top().parse_mode == ParseMode::Verbatim {
                    // ne change rien.
                } else if self.s_top().parse_mode != ParseMode::Pre
                    && self.style().is_some_and(|s| {
                        matches!(
                            s.attrs.white_space,
                            dillo_dw::style::WhiteSpace::Pre | dillo_dw::style::WhiteSpace::PreWrap
                        )
                    })
                {
                    self.s_top_mut().parse_mode = ParseMode::Pre;
                    self.core.pre_column = 0;
                    self.core.pre_first_char = true;
                }

                if let Some(id) = self.se.get_id().map(<[u8]>::to_vec) {
                    self.add_anchor(&id);
                }

                // `EndTag == 'F'` → fermeture immédiate requise.
                if TAGS
                    .get(ni.max(0) as usize)
                    .is_some_and(|t| t.end_tag == EndTag::Forbidden)
                {
                    self.core.req_tag_close = true;
                }
            }
        }

        // Close — `</x>`, ReqTagClose, `<x />` et `<x/>`.
        if !is_close_tag && !self.core.req_tag_close {
            // open tag sans demande de close — terminé.
            return;
        }
        let selfclose = tag.get(tagsize.wrapping_sub(2)) == Some(&b'/')
            && (tag
                .get(tagsize.wrapping_sub(3))
                .is_some_and(|&c| b" \"'".contains(&c))
                || tagsize == TAGS.get(ni.max(0) as usize).map_or(0, |t| t.name.len()) + 3);
        if is_close_tag || self.core.req_tag_close || selfclose {
            self.tag_cleanup_at_close(ni);
            self.core.req_tag_close = false;
        }
    }
}

// ==================================================================
// Tag functions — html.cc:1616-3480 (open/content/close dispatch)
// ==================================================================

/// `Html_tag_open_html` (html.cc:1616) — IN_HTML persiste jusqu'à
/// EOF ; >1 open → ReqTagClose.
pub fn tag_open_html(h: &mut DilloHtml, _tag: &[u8]) {
    if h.core.in_flags & in_flags::HTML == 0 {
        h.core.in_flags |= in_flags::HTML;
    }
    h.core.num_html = h.core.num_html.saturating_add(1);
    if h.core.num_html > 1 {
        h.bug_message("<html> was already open.");
        h.core.req_tag_close = true;
    }
}

/// `Html_tag_close_html` (html.cc:1635) — no-op verbatim (IN_HTML
/// ne se vide qu'à EOF).
pub fn tag_close_html(_h: &mut DilloHtml) {}

/// `Html_tag_open_head` (html.cc:1647).
pub fn tag_open_head(h: &mut DilloHtml, _tag: &[u8]) {
    if h.core.in_flags & in_flags::BODY != 0 {
        h.bug_message("<head> must go before the BODY section.");
        h.core.req_tag_close = true;
        return;
    }
    h.core.num_head = h.core.num_head.saturating_add(1);
    if h.core.in_flags & in_flags::HEAD != 0 {
        h.bug_message("<head> was already open.");
        h.core.req_tag_close = true;
    } else if h.core.num_head > 1 {
        h.bug_message("<head> already finished -- ignoring.");
        h.core.req_tag_close = true;
    } else {
        h.core.in_flags |= in_flags::HEAD;
    }
}

/// `Html_tag_close_head` (html.cc:1672) — head parsé une fois ;
/// charge les cssUrls collectées.
pub fn tag_close_head(h: &mut DilloHtml) {
    if h.core.in_flags & in_flags::HEAD != 0 {
        if h.core.num_head == 1 {
            if h.core.num_title == 0 {
                h.bug_message("<head> lacks <title>.");
            }
            h.core.in_flags &= !in_flags::HEAD;
            // charset déjà connu → charger les feuilles distantes.
            let urls: Vec<DilloUrl> = h.core.shared.borrow().css_urls.clone();
            for u in urls {
                h.load_stylesheet(&u);
            }
        } else if h.core.num_head > 1 {
            h.core.num_head -= 1;
        }
    }
}

/// `Html_tag_open_title` (html.cc:1698) — stash + Num_TITLE.
pub fn tag_open_title(h: &mut DilloHtml, _tag: &[u8]) {
    h.stash_init();
    if h.core.in_flags & in_flags::HEAD != 0 {
        h.core.num_title = h.core.num_title.saturating_add(1);
        if h.core.num_title > 1 {
            h.bug_message("Redundant <title>.");
        }
    } else {
        h.bug_message("<title> must be inside <head> -- ignoring.");
    }
}

/// `Html_tag_close_title` (html.cc:1718) — set_page_title si dans
/// HEAD et premier title ; titre vide ignoré.
pub fn tag_close_title(h: &mut DilloHtml) {
    if h.core.in_flags & in_flags::HEAD != 0 && h.core.num_title == 1 {
        if h.core.stash.is_empty() {
            return;
        }
        let title = h.core.stash.clone();
        let url = h.core.shared.borrow().page_url.clone();
        h.sink().borrow_mut().set_page_title(&url, &title);
    }
}

/// `Html_tag_open_script` (html.cc:1736) — stash VERBATIM.
pub fn tag_open_script(h: &mut DilloHtml, _tag: &[u8]) {
    h.stash_init();
    h.s_top_mut().parse_mode = ParseMode::Verbatim;
}

/// `Html_tag_close_script` (html.cc:1745) — no-op.
pub fn tag_close_script(_h: &mut DilloHtml) {}

/// `Html_tag_open_style` (html.cc:1754) — stash VERBATIM sauf
/// `type`/`media` incompatibles.
pub fn tag_open_style(h: &mut DilloHtml, tag: &[u8]) {
    h.core.load_css_from_stash = true;
    match h.get_attr(tag, b"type") {
        None => {
            if h.core.doc_type != DocType::Html || h.core.doc_type_version <= 4.01 {
                h.bug_message("<style> requires type attribute.");
            }
        }
        Some(attrbuf) => {
            if str_icmp(&attrbuf, b"text/css") != 0 {
                h.core.load_css_from_stash = false;
            }
        }
    }
    if let Some(attrbuf) = h.get_attr(tag, b"media")
        && str_icmp(&attrbuf, b"all") != 0
        && stri_str(&attrbuf, b"screen").is_none()
    {
        h.core.load_css_from_stash = false;
    }
    h.stash_init();
    h.s_top_mut().parse_mode = ParseMode::Verbatim;
}

/// `Html_tag_close_style` (html.cc:1783) — parse le stash en CSS
/// (origin AUTHOR, base_url).
pub fn tag_close_style(h: &mut DilloHtml) {
    if h.prefs.parse_embedded_css && h.core.load_css_from_stash {
        let base_url = h.core.shared.borrow().base_url.clone();
        let data = h.core.stash.clone();
        let mut ti = HtmlTagIndex;
        let mut hooks = HtmlCssHooks {
            core: &mut h.core,
            prefs: h.prefs,
        };
        h.se.parse(
            Some(&mut hooks),
            &mut ti,
            Some(&base_url),
            &data,
            CssOrigin::Author,
        );
    }
}

/// `Html_tag_open_body` (html.cc:1793) — flags + attrs non-CSS
/// (bgcolor/text/link/vlink) + couleur visited + parse_mode BODY.
pub fn tag_open_body(h: &mut DilloHtml, tag: &[u8]) {
    let html5 = h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0;
    if h.core.in_flags & in_flags::BODY == 0 {
        h.core.in_flags |= in_flags::BODY;
    }
    h.core.num_body = h.core.num_body.saturating_add(1);
    if h.core.num_body > 1 {
        h.bug_message("<body> was already open.");
        h.core.req_tag_close = true;
        return;
    }
    if h.core.in_flags & in_flags::HEAD != 0 {
        h.bug_message("Unclosed <head>.");
    }

    if let Some(attrbuf) = h.get_attr(tag, b"bgcolor") {
        let color = h.color_parse(&attrbuf, -1);
        if html5 {
            h.bug_message("<body> bgcolor attribute is obsolete.");
        }
        if color != -1 {
            h.se.set_non_css_hint_int(Prop::BackgroundColor, CssValueType::Color, color);
        }
    }
    if let Some(attrbuf) = h.get_attr(tag, b"text") {
        let color = h.color_parse(&attrbuf, -1);
        if html5 {
            h.bug_message("<body> text attribute is obsolete.");
        }
        if color != -1 {
            h.se.set_non_css_hint_int(Prop::Color, CssValueType::Color, color);
        }
    }

    h.restyle();

    if let Some(attrbuf) = h.get_attr(tag, b"link") {
        h.core.non_css_link_color = h.color_parse(&attrbuf, -1);
        if html5 {
            h.bug_message("<body> link attribute is obsolete.");
        }
    }
    if let Some(attrbuf) = h.get_attr(tag, b"vlink") {
        h.core.non_css_visited_color = h.color_parse(&attrbuf, -1);
        if html5 {
            h.bug_message("<body> vlink attribute is obsolete.");
        }
    }

    // `html->dw->setStyle (html->style ())`.
    if let (Some(dw), Some(style)) = (h.core.dw, h.style()) {
        h.core
            .shared
            .borrow()
            .layout
            .borrow_mut()
            .widget_set_style(dw, &style);
    }

    let bg_color = h.se.background_color();
    if let Some(c) = bg_color {
        h.layout().borrow_mut().set_bg_color(Some(c));
    }
    if let Some((img, rep, att, px, py)) = h.se.background_image() {
        h.layout()
            .borrow_mut()
            .set_bg_image(Some(img), rep, att, px, py);
    }

    // couleur visited — calculée une fois par page.
    h.start_element(*tags::I_A);
    h.se.set_pseudo_visited();
    if h.core.non_css_visited_color != -1 {
        h.se.set_non_css_hint_int(
            Prop::Color,
            CssValueType::Color,
            h.core.non_css_visited_color,
        );
    }
    let mut visited = h
        .style()
        .and_then(|s| s.attrs.color.map(|c| c.get_color()))
        .unwrap_or(-1);
    h.se.end_element(*tags::I_A);

    if h.prefs.contrast_visited_color {
        let text_c = h
            .style()
            .and_then(|s| s.attrs.color.map(|c| c.get_color()))
            .unwrap_or(-1);
        let bg_c = h
            .background_style()
            .and_then(|s| s.attrs.background_color.map(|c| c.get_color()))
            .unwrap_or(-1);
        visited = colors::color_vc(visited, text_c, h.core.non_css_link_color, bg_c);
    }
    h.core.shared.borrow_mut().visited_color = visited;

    h.s_top_mut().parse_mode = ParseMode::Body;
}

/// `Html_tag_close_body` (html.cc:1899) — no-op verbatim.
pub fn tag_close_body(_h: &mut DilloHtml) {}

/// `Html_tag_open_p` (html.cc:1913).
pub fn tag_open_p(h: &mut DilloHtml, tag: &[u8]) {
    h.tag_set_align_attr(tag);
}

/// `Html_tag_open_frame` + `Html_tag_open_iframe` (html.cc:1925).
pub fn tag_open_frame(h: &mut DilloHtml, tag: &[u8]) {
    let Some(attrbuf) = h.get_attr(tag, b"src") else {
        return;
    };
    let Some(url) = h.url_new(Some(&attrbuf), None, false) else {
        return;
    };
    if capi::capi_get_flags_with_redirection(&url) & capi::CAPI_IS_CACHED != 0 {
        h.se.set_pseudo_visited();
    } else {
        h.se.set_pseudo_link();
    }
    let nl = h.set_new_link(Some(url));
    h.se.set_non_css_hint_int(Prop::PropertyXLink, CssValueType::Integer, nl);
}

/// `Html_tag_content_frame` (html.cc:1948) — bullet + lien texte
/// ("IFRAME" forcé pour iframe).
pub fn tag_content_frame(h: &mut DilloHtml, tag: &[u8]) {
    let Some(attrbuf) = h.get_attr(tag, b"src") else {
        return;
    };
    let src = attrbuf;
    let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) else {
        return;
    };
    let arena_rc = h.core.shared.borrow().layout.clone();
    let mut arena = arena_rc.borrow_mut();
    arena.tb_add_parbreak(dw, 5, &ws);
    let bullet = arena.bullet_new();
    arena.tb_add_widget(dw, bullet, &ws);
    arena.tb_add_space(dw, &ws);
    if tag.get(1).map_or(0, |&c| dlib::ascii_to_lower(c)) == b'i' {
        arena.tb_add_text(dw, b"IFRAME", 6, &ws);
    } else if let Some(name) = h.get_attr(tag, b"name") {
        arena.tb_add_text(dw, &name, name.len(), &ws);
    } else {
        arena.tb_add_text(dw, &src, src.len(), &ws);
    }
    arena.tb_add_parbreak(dw, 5, &ws);
}

/// `Html_tag_content_frameset` (html.cc:1995).
pub fn tag_content_frameset(h: &mut DilloHtml, _tag: &[u8]) {
    let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) else {
        return;
    };
    let arena_rc = h.core.shared.borrow().layout.clone();
    let mut arena = arena_rc.borrow_mut();
    arena.tb_add_parbreak(dw, 9, &ws);
    arena.tb_add_text(dw, b"--FRAME--", 9, &ws);
    drop(arena);
    h.add_textblock(true, 5, false);
}

/// `Html_tag_open_h` (html.cc:2006) — h1..h6 : align + stash-and-body.
pub fn tag_open_h(h: &mut DilloHtml, tag: &[u8]) {
    h.tag_set_align_attr(tag);
    h.stash_init();
    h.s_top_mut().parse_mode = ParseMode::StashAndBody;
}

/// `Html_tag_content_br` (html.cc:2018).
pub fn tag_content_br(h: &mut DilloHtml, _tag: &[u8]) {
    let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) else {
        return;
    };
    h.core
        .shared
        .borrow()
        .layout
        .borrow_mut()
        .tb_add_linebreak(dw, &ws);
}

/// `Html_tag_open_font` (html.cc:2026) — color + face → non-CSS hints.
pub fn tag_open_font(h: &mut DilloHtml, tag: &[u8]) {
    if let Some(attrbuf) = h.get_attr(tag, b"color") {
        let color = if h.prefs.contrast_visited_color && h.core.in_visited_link {
            h.core.shared.borrow().visited_color
        } else {
            h.color_parse(&attrbuf, -1)
        };
        if color != -1 {
            h.se.set_non_css_hint_int(Prop::Color, CssValueType::Color, color);
        }
    }
    if let Some(attrbuf) = h.get_attr(tag, b"face") {
        h.se.set_non_css_hint_str(Prop::FontFamily, CssValueType::Symbol, &attrbuf);
    }
}

/// `Html_tag_open_abbr` (html.cc:2057) — inherit bg + title tooltip.
pub fn tag_open_abbr(h: &mut DilloHtml, tag: &[u8]) {
    h.se.inherit_background_color();
    if h.prefs.show_tooltip
        && let Some(attrbuf) = h.get_attr(tag, b"title")
    {
        h.se.set_non_css_hint_str(Prop::PropertyXTooltip, CssValueType::String, &attrbuf);
    }
}

/// `Html_tag_open_default` (html.cc:3437).
pub fn tag_open_default(h: &mut DilloHtml, _tag: &[u8]) {
    h.se.inherit_background_color();
}

/// `Html_tag_open_span` (html.cc:3443).
pub fn tag_open_span(h: &mut DilloHtml, tag: &[u8]) {
    h.se.inherit_background_color();
    if h.prefs.show_tooltip
        && let Some(attrbuf) = h.get_attr(tag, b"title")
    {
        h.se.set_non_css_hint_str(Prop::PropertyXTooltip, CssValueType::String, &attrbuf);
    }
}

/// `Html_tag_open_sectioning` (html.cc:3458) — article/aside/nav/
/// section/header/footer/main.
pub fn tag_open_sectioning(h: &mut DilloHtml, tag: &[u8]) {
    if h.prefs.show_tooltip
        && let Some(attrbuf) = h.get_attr(tag, b"title")
    {
        h.se.set_non_css_hint_str(Prop::PropertyXTooltip, CssValueType::String, &attrbuf);
    }
}

/// `Html_tag_open_div` (html.cc:3473).
pub fn tag_open_div(h: &mut DilloHtml, tag: &[u8]) {
    h.tag_set_align_attr(tag);
    tag_open_sectioning(h, tag);
}

/// `Html_tag_close_par` (html.cc:3481) — parbreak 9.
pub fn tag_close_par(h: &mut DilloHtml) {
    let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) else {
        return;
    };
    h.core
        .shared
        .borrow()
        .layout
        .borrow_mut()
        .tb_add_parbreak(dw, 9, &ws);
}

/// `Html_tag_content_wbr` (html.cc:3490) — break option forcé.
pub fn tag_content_wbr(h: &mut DilloHtml, _tag: &[u8]) {
    let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) else {
        return;
    };
    h.core
        .shared
        .borrow()
        .layout
        .borrow_mut()
        .tb_add_break_option(dw, &ws, true);
}

// ==================================================================
// Helpers de style/attributs + récepteur de liens (html.cc:720-1529)
// ==================================================================

impl DilloHtml<'_> {
    /// `a_Html_tag_set_align_attr` (html.cc:318) — `align` →
    /// `text-align` non-CSS hint.
    pub fn tag_set_align_attr(&mut self, tag: &[u8]) {
        if let Some(align) = self.get_attr(tag, b"align") {
            use dillo_dw::style::TextAlignType as TA;
            let mut ta = TA::Left;
            if self.core.doc_type == DocType::Html && self.core.doc_type_version >= 5.0 {
                self.bug_message("The align attribute is obsolete in HTML5.");
            }
            if str_icmp(&align, b"left") == 0 {
                ta = TA::Left;
            } else if str_icmp(&align, b"right") == 0 {
                ta = TA::Right;
            } else if str_icmp(&align, b"center") == 0 {
                ta = TA::Center;
            } else if str_icmp(&align, b"justify") == 0 {
                ta = TA::Justify;
            }
            self.se
                .set_non_css_hint_int(Prop::TextAlign, CssValueType::Enum, ta as i32);
        }
    }

    /// `a_Html_tag_set_valign_attr` (html.cc:356) — `valign` →
    /// `vertical-align`. Retourne `true` si posé.
    pub fn tag_set_valign_attr(&mut self, tag: &[u8]) -> bool {
        if let Some(attr) = self.get_attr(tag, b"valign") {
            use dillo_dw::style::VAlignType as VA;
            if self.core.doc_type == DocType::Html && self.core.doc_type_version >= 5.0 {
                self.bug_message("The valign attribute is obsolete in HTML5.");
            }
            let valign = if str_icmp(&attr, b"top") == 0 {
                VA::Top
            } else if str_icmp(&attr, b"bottom") == 0 {
                VA::Bottom
            } else if str_icmp(&attr, b"baseline") == 0 {
                VA::Baseline
            } else {
                VA::Middle
            };
            self.se
                .set_non_css_hint_int(Prop::VerticalAlign, CssValueType::Enum, valign as i32);
            return true;
        }
        false
    }

    /// `a_Html_color_parse` (html.cc:1468) — `a_Color_parse` + BUG_MSG.
    pub fn color_parse(&mut self, s: &[u8], default_color: i32) -> i32 {
        let (color, err) = colors::color_parse(s, default_color);
        if err != 0 {
            self.bug_message(&format!(
                "Color \"{}\" is not in \"#RRGGBB\" format.",
                String::from_utf8_lossy(s)
            ));
        }
        color
    }

    /// `Html_parse_length_or_multi_length` (html.cc:1404) —
    /// `%`/`px`/`*`. Retourne `(CssLength, end_offset)`.
    pub(crate) fn parse_length_or_multi_length(attr: &[u8]) -> (CssLength, usize) {
        let (v, mut end) = dlib::strtod(attr);
        let l = match attr.get(end).copied().unwrap_or(0) {
            b'%' => {
                end += 1;
                css_create_length(v as f32 / 100.0, CssLengthType::Percentage)
            }
            b'*' => {
                end += 1;
                css_create_length(v as f32, CssLengthType::Relative)
            }
            _ => css_create_length(v as f32, CssLengthType::Px),
        };
        (l, end)
    }

    /// `a_Html_parse_length` (html.cc:1443) — `RELATIVE` interdit ;
    /// garbage après la valeur → AUTO + BUG_MSG.
    pub fn parse_length(&mut self, attr: &[u8]) -> CssLength {
        let (l, end) = Self::parse_length_or_multi_length(attr);
        if l.ty == CssLengthType::Relative {
            // RELATIVE — interdit comme &Length;
            return css_create_length(0.0, CssLengthType::Auto);
        }
        if attr.get(end).is_some_and(|&c| !dlib::is_space(c) && c != 0) {
            self.bug_message(&format!(
                "Garbage after length: '{}'.",
                String::from_utf8_lossy(attr)
            ));
            return css_create_length(0.0, CssLengthType::Auto);
        }
        l
    }

    /// `DilloHtml::loadImages` (html.cc:702) — chargement différé
    /// des images (pattern = `None` → toutes, requester = page_url).
    pub fn load_images(&mut self, pattern: Option<&DilloUrl>) {
        if !self.sink().borrow().expecting() {
            return;
        }
        let requester = match pattern {
            None => Some(self.core.shared.borrow().page_url.clone()),
            Some(_) => None,
        };
        let n = self.core.shared.borrow().images.len();
        for i in 0..n {
            // clone des handles pour casser l'emprunt pendant load_image
            let (img_url, img) = {
                let shared = self.core.shared.borrow();
                let e = &shared.images[i];
                (e.url.clone(), e.image.clone())
            };
            if let Some(handle) = img {
                let matches = match pattern {
                    None => true,
                    Some(p) => img_url.url_cmp(p) == 0,
                };
                if matches
                    && self
                        .sink()
                        .borrow_mut()
                        .load_image(&img_url, requester.as_ref(), &handle)
                {
                    // web possède la réf maintenant.
                    self.core.shared.borrow_mut().images[i].image = None;
                }
            }
        }
    }

    /// `a_Html_load_stylesheet` (html.cc:3234) — cached → parse
    /// inline ; sinon fetch réseau + `NumPendingStyleSheets++`.
    pub fn load_stylesheet(&mut self, url: &DilloUrl) {
        if !self.prefs.load_stylesheets {
            return;
        }
        if capi::capi_get_flags_with_redirection(url) & capi::CAPI_COMPLETED != 0
            && let Some(mut data) = capi::capi_get_buf(url)
        {
            // sniffing `@charset` (verbatim : ≤ 40 chars IANA,
            // `endq - data <= 51`).
            if data.starts_with(b"@charset \"")
                && let Some(rel) = data[10..].iter().position(|&c| c == b'"')
            {
                let endq = 10 + rel;
                if endq - 10 <= 51 {
                    let charset = data[10..endq].to_vec();
                    capi::capi_unref_buf(url);
                    let ct = [b"text/css; charset=".as_ref(), &charset].concat();
                    let _ = capi::capi_set_content_type(url, &ct, "meta");
                    if let Some(d2) = capi::capi_get_buf(url) {
                        data = d2;
                    }
                }
            }
            let base_url = url.clone();
            let mut ti = HtmlTagIndex;
            let mut hooks = HtmlCssHooks {
                core: &mut self.core,
                prefs: self.prefs,
            };
            self.se.parse(
                Some(&mut hooks),
                &mut ti,
                Some(&base_url),
                &data,
                CssOrigin::Author,
            );
        } else {
            // fetch réseau — verbatim a_Web_new + capi_open_url +
            // NumPendingStyleSheets++ (le repush arrive à 0).
            let page_url = self.core.shared.borrow().page_url.clone();
            let sink = self.sink();
            let mut s = sink.borrow_mut();
            if s.load_stylesheet(url, &page_url) {
                let n = s.num_pending_style_sheets() + 1;
                s.set_num_pending_style_sheets(n);
            }
        }
    }
}

/// `HtmlCssHooks` — vue disjointe `(&mut HtmlCore, &HtmlPrefs)`
/// implémentant `CssHtmlHooks` (le `&mut se` de l'appelant reste
/// libre — ≙ `DilloHtml*` passé au parser amont).
pub(crate) struct HtmlCssHooks<'a> {
    /// `html` hors-`se`.
    pub core: &'a mut HtmlCore,
    /// `prefs`.
    pub prefs: &'a HtmlPrefs,
}

impl HtmlCssHooks<'_> {
    /// `BUG_MSG` sur core (sans `se`) — verbatim `++num_page_bugs`.
    fn bug(&mut self, msg: &str) {
        let line = core_line_number(self.core);
        let full = format!("HTML warning: line {line}, {msg}");
        let mut shared = self.core.shared.borrow_mut();
        shared.page_bugs += 1;
        shared.sink.borrow_mut().bug_message(&full);
    }

    /// `a_Html_url_new` — même logique que `DilloHtml::url_new`
    /// mais sur `HtmlCore` seul.
    fn url_new_core(
        &mut self,
        url_str: Option<&[u8]>,
        base_url: Option<&[u8]>,
        use_base_url: bool,
    ) -> Option<DilloUrl> {
        let url_str = url_str?;
        let base = if use_base_url {
            base_url.map(<[u8]>::to_vec)
        } else {
            let mut u = self.core.shared.borrow().base_url.clone();
            Some(u.str_().to_vec())
        };
        match url::url_new(url_str, base.as_deref()) {
            None => {
                self.bug(&format!(
                    "URL is not valid '{}'.",
                    String::from_utf8_lossy(url_str)
                ));
                None
            }
            ok => ok,
        }
    }
}

impl CssHtmlHooks for HtmlCssHooks<'_> {
    fn resolve_url(
        &mut self,
        url_str: &[u8],
        base_url: Option<&[u8]>,
        use_base: bool,
    ) -> Option<DilloUrl> {
        self.url_new_core(Some(url_str), base_url, use_base)
    }

    /// `a_Html_load_stylesheet` (vue hooks) — `Some(data)` = cache
    /// hit (parse inline) ; sinon fetch réseau + `None`.
    fn load_stylesheet(&mut self, url: DilloUrl) -> Option<Vec<u8>> {
        if !self.prefs.load_stylesheets {
            return None;
        }
        if capi::capi_get_flags_with_redirection(&url) & capi::CAPI_COMPLETED != 0
            && let Some(mut data) = capi::capi_get_buf(&url)
        {
            if data.starts_with(b"@charset \"")
                && let Some(rel) = data[10..].iter().position(|&c| c == b'"')
            {
                let endq = 10 + rel;
                if endq - 10 <= 51 {
                    let charset = data[10..endq].to_vec();
                    capi::capi_unref_buf(&url);
                    let ct = [b"text/css; charset=".as_ref(), &charset].concat();
                    let _ = capi::capi_set_content_type(&url, &ct, "meta");
                    if let Some(d2) = capi::capi_get_buf(&url) {
                        data = d2;
                    }
                }
            }
            return Some(data);
        }
        let page_url = self.core.shared.borrow().page_url.clone();
        let sink = self.core.shared.borrow().sink.clone();
        let mut s = sink.borrow_mut();
        if s.load_stylesheet(&url, &page_url) {
            let n = s.num_pending_style_sheets() + 1;
            s.set_num_pending_style_sheets(n);
        }
        None
    }
}

/// `getCurrLineNumber` sur `HtmlCore` seul (≙ `DilloHtml::`).
fn core_line_number(core: &mut HtmlCore) -> i32 {
    if core.in_flags & in_flags::META_HACK != 0 {
        return -1;
    }
    let Some(buf) = core.start_buf.clone() else {
        return -1;
    };
    let ofs = core.curr_ofs.min(buf.len());
    let mut line = core.old_line;
    let mut i = core.old_ofs.min(ofs);
    while i < ofs {
        if buf[i] == b'\n' || (buf[i] == b'\r' && buf.get(i + 1) != Some(&b'\n')) {
            line += 1;
        }
        i += 1;
    }
    core.old_ofs = core.curr_ofs;
    core.old_line = line;
    line as i32
}

// ------------------------------------------------------------------
// HtmlLinkReceiver (html.cc:735-815) — verbatim des handlers
// ------------------------------------------------------------------

/// `DilloHtml::HtmlLinkReceiver` — `Weak<HtmlShared>` ≙ pointeur
/// `html` amont (le receiver survit au parseur via `Rc` cycles
/// évités — verbatim `linkReceiver.html = this` non-possédant).
pub(crate) struct HtmlLinkReceiver {
    /// `html->bw`/état partagé.
    pub shared: std::rc::Weak<RefCell<HtmlShared>>,
}

impl HtmlLinkReceiver {
    /// `html->links->get(link)` — `-1`/OOB ≙ `NULL` amont.
    fn link_at(shared: &HtmlShared, link: i32) -> Option<DilloUrl> {
        if link < 0 {
            return None;
        }
        shared.links.get(link as usize).cloned()
    }

    /// `Html_set_link_coordinates` (html.cc:292) — `?x,y` sur le lien.
    fn set_link_coords(shared: &mut HtmlShared, link: i32, x: i32, y: i32) {
        if x != -1
            && link >= 0
            && let Some(u) = shared.links.get_mut(link as usize)
        {
            let data = format!("?{x},{y}");
            u.set_ismap_coords(data.as_bytes());
        }
    }
}

impl LinkReceiver for HtmlLinkReceiver {
    /// `enter` — status bar message (vide au leave).
    fn enter(&mut self, _widget: Option<WidgetId>, link: i32, _img: i32, x: i32, y: i32) -> bool {
        let Some(sh) = self.shared.upgrade() else {
            return false;
        };
        let mut shared = sh.borrow_mut();
        if link == -1 {
            shared.sink.borrow_mut().set_msg(b"");
        } else {
            Self::set_link_coords(&mut shared, link, x, y);
            let msg = Self::link_at(&shared, link)
                .map(|mut u| u.str_().to_vec())
                .unwrap_or_default();
            shared.sink.borrow_mut().set_msg(&msg);
        }
        true
    }

    /// `press` — bouton 3 → popups image/lien/page.
    fn pressed(
        &mut self,
        _widget: Option<WidgetId>,
        link: i32,
        img: i32,
        _x: i32,
        _y: i32,
        event: &EventButton,
    ) -> bool {
        if event.button != 3 {
            return false;
        }
        let Some(sh) = self.shared.upgrade() else {
            return false;
        };
        let shared = sh.borrow_mut();
        if img != -1 {
            // menu image — `loaded` ≙ `image == NULL`.
            let (img_url, loaded) = shared.images.get(img.max(0) as usize).map_or_else(
                || (shared.page_url.clone(), true),
                |e| (e.url.clone(), e.image.is_none()),
            );
            let linkurl = Self::link_at(&shared, link);
            let page_url = shared.page_url.clone();
            shared
                .sink
                .borrow_mut()
                .image_popup(&img_url, loaded, &page_url, linkurl.as_ref());
            true
        } else if link == -1 {
            let has_bugs = shared.page_bugs != 0;
            let css_urls = shared.css_urls.clone();
            shared.sink.borrow_mut().page_popup(has_bugs, &css_urls);
            true
        } else {
            let url = Self::link_at(&shared, link);
            let page_url = shared.page_url.clone();
            if let Some(u) = url {
                shared.sink.borrow_mut().link_popup(&u, &page_url);
            }
            true
        }
    }

    /// `click` — bouton 1/2 : navigation + visited color + lazy-load.
    #[allow(clippy::many_single_char_names)] // verbatim (x, y, img, u, h, m…).
    fn clicked(
        &mut self,
        widget: Option<WidgetId>,
        link: i32,
        img: i32,
        x: i32,
        y: i32,
        event: &EventButton,
    ) -> bool {
        let Some(sh) = self.shared.upgrade() else {
            return false;
        };
        // click sur une image non chargée → load toutes ses instances.
        if img != -1 {
            let pending = shared_pending_image(&sh, img);
            if pending.is_some() && event.button == 1 {
                let pattern = pending;
                // loadImages(pattern) — verbatim requester = NULL.
                let n = sh.borrow().images.len();
                for i in 0..n {
                    let (u, h) = {
                        let s = sh.borrow();
                        (s.images[i].url.clone(), s.images[i].image.clone())
                    };
                    if let Some(handle) = h {
                        let m = match &pattern {
                            None => true,
                            Some(p) => u.url_cmp(p) == 0,
                        };
                        if m && sh.borrow().sink.borrow_mut().load_image(&u, None, &handle) {
                            sh.borrow_mut().images[i].image = None;
                        }
                    }
                }
                return true;
            }
        }

        if link != -1 {
            let mut shared = sh.borrow_mut();
            Self::set_link_coords(&mut shared, link, x, y);
            let url = Self::link_at(&shared, link);
            if let Some(u) = url {
                let prefs = Rc::clone(&shared.prefs);
                let mut sink = shared.sink.borrow_mut();
                if event.button == 1 {
                    if event.state & button_state::CONTROL_MASK != 0 {
                        if prefs.middle_click_opens_new_tab {
                            let mut focus = i32::from(prefs.focus_new_tab);
                            if event.state & button_state::SHIFT_MASK != 0 {
                                focus = i32::from(focus == 0);
                            }
                            sink.open_url_nt(&u, focus != 0);
                        } else {
                            sink.open_url_nw(&u);
                        }
                    } else {
                        sink.open_url(&u);
                    }
                } else if event.button == 2 {
                    if prefs.middle_click_opens_new_tab {
                        let mut focus = i32::from(prefs.focus_new_tab);
                        if event.state & button_state::SHIFT_MASK != 0 {
                            focus = i32::from(focus == 0);
                        }
                        sink.open_url_nt(&u, focus != 0);
                    } else {
                        sink.open_url_nw(&u);
                    }
                } else {
                    return false;
                }
                // feedback "visited" — différé via le sink : le
                // `Layout` est déjà sous `borrow_mut` (emission
                // pendant `button_event`), la maj part en `GlueOp`.
                let visited = shared.visited_color;
                sink.link_visited(widget, link, visited);
                drop(sink);
            }
        }
        true
    }
}

/// `images->get(img)->image` — `Some(handle)` si non chargée.
fn shared_pending_image(sh: &Rc<RefCell<HtmlShared>>, img: i32) -> Option<DilloUrl> {
    sh.borrow()
        .images
        .get(img.max(0) as usize)
        .filter(|e| e.image.is_some())
        .map(|e| e.url.clone())
}

// ==================================================================
// Tag functions, partie C — liens, images, listes, média, meta
// (html.cc:2200-3300)
// ==================================================================

/// `object::String` amont ≙ clé `ImageMapKey` portant la chaîne.
struct UrlMapKey {
    /// `String::chars`.
    s: Vec<u8>,
}

impl dillo_dw::image::ImageMapKey for UrlMapKey {
    fn key_equals(&self, other: &Rc<dyn dillo_dw::image::ImageMapKey>) -> bool {
        (other as &dyn std::any::Any)
            .downcast_ref::<UrlMapKey>()
            .is_some_and(|o| o.s == self.s)
    }

    fn key_hash(&self) -> i32 {
        self.s
            .iter()
            .fold(0i32, |h, &b| h.wrapping_mul(31).wrapping_add(i32::from(b)))
    }
}

impl DilloHtml<'_> {
    /// `a_Html_common_image_attrs` (html.cc:2073) — `title`,
    /// `width`/`height` (avec garde `IMAGE_MAX_AREA`) et `x_img`.
    pub(crate) fn common_image_attrs(&mut self, tag: &[u8]) {
        use crate::common::IMAGE_MAX_AREA;
        let mut l_w = css_create_length(0.0, CssLengthType::Auto);
        let mut l_h = css_create_length(0.0, CssLengthType::Auto);
        let mut w: i64 = 0;
        let mut h: i64 = 0;

        if self.prefs.show_tooltip
            && let Some(attrbuf) = self.get_attr(tag, b"title")
        {
            self.se
                .set_non_css_hint_str(Prop::PropertyXTooltip, CssValueType::String, &attrbuf);
        }
        let width_ptr = self.get_attr(tag, b"width");
        let height_ptr = self.get_attr(tag, b"height");
        // Check for malicious values.
        if let Some(wp) = &width_ptr {
            l_w = self.parse_length(wp);
            w = if l_w.ty == CssLengthType::Px {
                i64::from(l_w.i)
            } else {
                0
            };
        }
        if let Some(hp) = &height_ptr {
            l_h = self.parse_length(hp);
            h = if l_h.ty == CssLengthType::Px {
                i64::from(l_h.i)
            } else {
                0
            };
        }
        let max = IMAGE_MAX_AREA;
        if w < 0 || h < 0 || w > max || h > max || (h > 0 && w > max / h) {
            tracing::info!(
                "a_Html_common_image_attrs: suspicious image size request {} x {}",
                w,
                h
            );
        } else {
            if l_w.ty != CssLengthType::Auto {
                self.se
                    .set_non_css_hint_len(Prop::Width, CssValueType::LengthPercentage, l_w);
            }
            if l_h.ty != CssLengthType::Auto {
                self.se
                    .set_non_css_hint_len(Prop::Height, CssValueType::LengthPercentage, l_h);
            }
        }

        // `x_img` — index dans `images` (le prochain `image_new`
        // pousse à `len`).
        let idx = self.core.shared.borrow().images.len() as i32;
        self.se
            .set_non_css_hint_int(Prop::PropertyXImg, CssValueType::Integer, idx);
    }

    /// `a_Html_image_new` (html.cc:2143) — widget `dw::Image` +
    /// entrée `images` + requête cache éventuelle.
    pub(crate) fn image_new(&mut self, tag: &[u8]) -> Option<ImageHandle> {
        let attrbuf = self.get_attr(tag, b"src")?;
        let url = self.url_new(Some(&attrbuf), None, false)?;

        // alt : absent/vide → "[IMG]", sinon "[IMG] <alt>".
        let alt = match self.get_attr(tag, b"alt") {
            None => b"[IMG]".to_vec(),
            Some(a) if a.is_empty() => b"[IMG]".to_vec(),
            Some(a) => [b"[IMG] ".as_ref(), &a].concat(),
        };

        let shared = Rc::clone(&self.core.shared);
        let dw_img = {
            let layout = shared.borrow().layout.clone();
            layout
                .borrow_mut()
                .image_new(Some(alt), self.prefs.mark_unloaded_images)
        };

        let (bg, fg) = {
            let arena = shared.borrow().layout.clone();
            let arena = arena.borrow();
            let tb = self.core.dw;
            (
                tb.and_then(|t| arena.widget_get_bg_color(t))
                    .map(|c| c.get_color()),
                tb.and_then(|t| arena.widget_get_fg_color(t))
                    .map(|c| c.get_color()),
            )
        };

        let image: ImageHandle = Rc::new(RefCell::new(HtmlImage {
            widget: dw_img,
            layout: shared.borrow().layout.clone(),
            bg_color: bg.unwrap_or(0),
            fg_color: fg.unwrap_or(0),
            width: 0,
            height: 0,
        }));

        // `a_Image_ref` ≙ l'entrée `images` partage le handle.
        {
            let mut sh = shared.borrow_mut();
            sh.images.push(HtmlImageEntry {
                url: url.clone(),
                image: None,
            });
        }

        let load_now = self.prefs.load_images
            || url
                .scheme
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(b"data"))
            || capi::capi_get_flags_with_redirection(&url) & capi::CAPI_IS_CACHED != 0;

        let page_url = shared.borrow().page_url.clone();
        let ok = load_now
            && self
                .sink()
                .borrow_mut()
                .load_image(&url, Some(&page_url), &image);
        if !ok {
            // sinon la référence reste dans `html->images` (verbatim).
            if let Some(e) = shared.borrow_mut().images.last_mut() {
                e.image = Some(image.clone());
            }
        }
        Some(image)
    }
}

/// `Html_tag_open_img` (html.cc:2220) — hspace/vspace/border +
/// `a_Html_common_image_attrs`.
pub fn tag_open_img(h: &mut DilloHtml, tag: &[u8]) {
    h.common_image_attrs(tag);

    // espaces horizontaux
    if let Some(attrbuf) = h.get_attr(tag, b"hspace") {
        let (space, _) = dlib::strtol(&attrbuf, 10);
        if space > 0 {
            let len = css_create_length(space as f32, CssLengthType::Px);
            h.se.set_non_css_hint_len(Prop::MarginLeft, CssValueType::LengthPercentage, len);
            h.se.set_non_css_hint_len(Prop::MarginRight, CssValueType::LengthPercentage, len);
        }
    }

    // espaces verticaux
    if let Some(attrbuf) = h.get_attr(tag, b"vspace") {
        let (space, _) = dlib::strtol(&attrbuf, 10);
        if space > 0 {
            let len = css_create_length(space as f32, CssLengthType::Px);
            h.se.set_non_css_hint_len(Prop::MarginTop, CssValueType::LengthPercentage, len);
            h.se.set_non_css_hint_len(Prop::MarginBottom, CssValueType::LengthPercentage, len);
        }
    }

    // bordure
    if let Some(attrbuf) = h.get_attr(tag, b"border") {
        let (border, _) = dlib::strtol(&attrbuf, 10);
        if border >= 0 {
            let b = css_create_length(border as f32, CssLengthType::Px);
            h.se.set_non_css_hint_len(Prop::BorderTopWidth, CssValueType::LengthPercentage, b);
            h.se.set_non_css_hint_len(Prop::BorderBottomWidth, CssValueType::LengthPercentage, b);
            h.se.set_non_css_hint_len(Prop::BorderLeftWidth, CssValueType::LengthPercentage, b);
            h.se.set_non_css_hint_len(Prop::BorderRightWidth, CssValueType::LengthPercentage, b);
            let solid = dillo_dw::style::BorderStyle::Solid as i32;
            h.se.set_non_css_hint_int(Prop::BorderTopStyle, CssValueType::Enum, solid);
            h.se.set_non_css_hint_int(Prop::BorderBottomStyle, CssValueType::Enum, solid);
            h.se.set_non_css_hint_int(Prop::BorderLeftStyle, CssValueType::Enum, solid);
            h.se.set_non_css_hint_int(Prop::BorderRightStyle, CssValueType::Enum, solid);
        }
    }
}

/// `Html_tag_content_img` (html.cc:2283) — SpamSafe court-circuite ;
/// `usemap`/`ismap` verbatim.
pub fn tag_content_img(h: &mut DilloHtml, tag: &[u8]) {
    // visualisation de mail suspect → pas d'images.
    if h.core.shared.borrow().base_url.flags & url::flags::SPAM_SAFE != 0 {
        return;
    }

    let Some(image) = h.image_new(tag) else {
        return;
    };

    let usemap_url = h
        .get_attr(tag, b"usemap")
        .and_then(|a| h.url_new(Some(&a), None, false));

    let shared = Rc::clone(&h.core.shared);
    let (dw_img, layout, arena) = {
        let sh = shared.borrow();
        (image.borrow().widget, sh.layout.clone(), sh.layout.clone())
    };
    let Some(style) = h.style() else { return };
    if let Some(dw) = h.core.dw {
        arena.borrow_mut().tb_add_widget(dw, dw_img, &style);
        arena.borrow_mut().tb_add_break_option(dw, &style, false);
    }

    // cartes image
    if h.get_attr(tag, b"ismap").is_some() {
        layout.borrow_mut().image_set_is_map(dw_img);
    }

    if let Some(mut um) = usemap_url {
        let key = Rc::new(UrlMapKey {
            s: um.str_().to_vec(),
        });
        let maps = shared.borrow().maps.clone();
        layout.borrow_mut().image_set_use_map(dw_img, maps, key);
    }
}

/// `Html_tag_content_map` (html.cc:2329).
pub fn tag_content_map(h: &mut DilloHtml, tag: &[u8]) {
    if h.core.in_flags & in_flags::MAP != 0 {
        h.bug_message("Nested <map>.");
    } else if let Some(attrbuf) = h.get_attr(tag, b"name") {
        h.core.in_flags |= in_flags::MAP;
        let hash_name = [b"#".as_ref(), &attrbuf].concat();
        // url NULL amont ≙ `if let` (sécurité ; BUG déjà émis).
        if let Some(mut url) = h.url_new(Some(&hash_name), None, false) {
            let key = Rc::new(UrlMapKey {
                s: url.str_().to_vec(),
            });
            h.core.shared.borrow().maps.borrow_mut().start_new_map(key);
        }
    } else {
        h.bug_message("<map> requires name attribute.");
    }
}

/// `Html_tag_close_map` (html.cc:2354) — forceMapRedraw des images
/// non chargées puis IN_MAP retiré.
pub fn tag_close_map(h: &mut DilloHtml) {
    let shared = h.core.shared.borrow();
    let layout = shared.layout.clone();
    for e in &shared.images {
        if let Some(img) = &e.image {
            let w = img.borrow().widget;
            layout.borrow_mut().image_force_map_redraw(w);
        }
    }
    drop(shared);
    h.core.in_flags &= !in_flags::MAP;
}

/// `Html_read_coords` (html.cc:2380) — `strtol` en boucle,
/// séparateur `,` obligatoire.
pub(crate) fn read_coords(h: &mut DilloHtml, s: &[u8]) -> Vec<i32> {
    let mut coords = Vec::with_capacity(4);
    let mut tail = s;
    loop {
        let (coord, n) = dlib::strtol(tail, 10);
        if coord == 0 && n == 0 {
            break;
        }
        coords.push(coord);
        let mut i = n;
        while tail.get(i).is_some_and(|&c| dlib::is_space(c)) {
            i += 1;
        }
        if i >= tail.len() {
            break;
        }
        if tail[i] != b',' {
            h.bug_message("<area> coords must be integers separated by commas.");
        }
        tail = &tail[i + 1..];
    }
    coords
}

/// `Html_tag_content_area` (html.cc:2410) — formes d'imagemap.
#[allow(clippy::items_after_statements)] // `enum AreaType` local verbatim.
pub fn tag_content_area(h: &mut DilloHtml, tag: &[u8]) {
    use dillo_dw::types::{Circle, Polygon, Rectangle, Shape};

    if h.core.in_flags & in_flags::MAP == 0 {
        h.bug_message("<area> not inside <map>.");
        return;
    }

    #[derive(PartialEq)]
    enum AreaType {
        Unknown,
        RectangleT,
        CircleT,
        PolygonT,
        Background,
    }
    let attrbuf = h.get_attr(tag, b"shape");
    let ty = match attrbuf.as_deref() {
        None | Some(b"") => AreaType::RectangleT,
        Some(s) if str_icmp(s, b"rect") == 0 => AreaType::RectangleT,
        Some(s) if str_icmp(s, b"default") == 0 => AreaType::Background,
        Some(s) if str_icmp(s, b"circle") == 0 => AreaType::CircleT,
        Some(s) if strn_icmp(s, b"poly", 4) == 0 => AreaType::PolygonT,
        Some(s) => {
            h.bug_message(&format!(
                "<area> unknown shape: '{}'.",
                String::from_utf8_lossy(s)
            ));
            AreaType::Unknown
        }
    };

    let mut shape: Option<Rc<dyn Shape>> = None;
    if ty == AreaType::RectangleT || ty == AreaType::CircleT || ty == AreaType::PolygonT {
        if let Some(attrbuf) = h.get_attr(tag, b"coords") {
            let coords = read_coords(h, &attrbuf);
            if ty == AreaType::RectangleT {
                if coords.len() != 4 {
                    h.bug_message("<area> rectangle must have four coordinate values.");
                }
                if coords.len() >= 4 {
                    shape = Some(Rc::new(Rectangle::new(
                        coords[0],
                        coords[1],
                        coords[2] - coords[0],
                        coords[3] - coords[1],
                    )));
                }
            } else if ty == AreaType::CircleT {
                if coords.len() != 3 {
                    h.bug_message("<area> circle must have three coordinate values.");
                }
                if coords.len() >= 3 {
                    shape = Some(Rc::new(Circle::new(coords[0], coords[1], coords[2])));
                }
            } else {
                if coords.len() % 2 != 0 {
                    h.bug_message("<area> polygon with odd number of coordinates.");
                }
                let mut poly = Polygon::new();
                for i in 0..coords.len() / 2 {
                    poly.add_point(coords[2 * i], coords[2 * i + 1]);
                }
                shape = Some(Rc::new(poly));
            }
        }
    }

    if shape.is_some() || ty == AreaType::Background {
        let mut link = -1;
        if let Some(attrbuf) = h.get_attr(tag, b"href") {
            let url = h.url_new(Some(&attrbuf), None, false);
            link = h.set_new_link(url);
        }
        let shared = h.core.shared.borrow();
        let mut maps = shared.maps.borrow_mut();
        if ty == AreaType::Background {
            maps.set_current_map_default_link(link);
        } else if let Some(sh) = shape {
            maps.add_shape_to_current_map(sh, link);
        }
    }
}

// ------------------------------------------------------------------
// <OBJECT> / <EMBED> / <VIDEO> / <AUDIO> / <SOURCE>
// ------------------------------------------------------------------

/// `Html_tag_open_object` (html.cc:2484) — lien si `data`
/// téléchargeable (codebase = base).
pub fn tag_open_object(h: &mut DilloHtml, tag: &[u8]) {
    let base_url = h
        .get_attr(tag, b"codebase")
        .and_then(|a| h.url_new(Some(&a), None, false));

    if let Some(attrbuf) = h.get_attr(tag, b"data") {
        let base_str = base_url.clone().map(|mut u| u.str_().to_vec());
        let Some(url) = h.url_new(Some(&attrbuf), base_str.as_deref(), base_url.is_some()) else {
            return;
        };
        if capi::capi_get_flags_with_redirection(&url) & capi::CAPI_IS_CACHED != 0 {
            h.se.set_pseudo_visited();
        } else {
            h.se.set_pseudo_link();
        }
        let link = h.set_new_link(Some(url));
        h.se.set_non_css_hint_int(Prop::PropertyXLink, CssValueType::Integer, link);
    }
}

/// `Html_tag_content_object` (html.cc:2510).
pub fn tag_content_object(h: &mut DilloHtml, tag: &[u8]) {
    if h.get_attr(tag, b"data").is_some()
        && let (Some(dw), Some(ws)) = (h.core.dw, h.word_style())
    {
        h.layout().borrow_mut().tb_add_text(dw, b"[OBJECT]", 8, &ws);
    }
}

/// `Html_tag_open_video` (html.cc:2525).
pub fn tag_open_video(h: &mut DilloHtml, tag: &[u8]) {
    if h.core.in_flags & in_flags::MEDIA != 0 {
        tracing::warn!("<video> not handled when already inside a media element.");
        return;
    }
    if let Some(attrbuf) = h.get_attr(tag, b"src") {
        // `dReturn_if_fail(url != NULL)` — IN_MEDIA non posé.
        let Some(url) = h.url_new(Some(&attrbuf), None, false) else {
            return;
        };
        if capi::capi_get_flags_with_redirection(&url) & capi::CAPI_IS_CACHED != 0 {
            h.se.set_pseudo_visited();
        } else {
            h.se.set_pseudo_link();
        }
        let link = h.set_new_link(Some(url));
        h.se.set_non_css_hint_int(Prop::PropertyXLink, CssValueType::Integer, link);
        if let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) {
            h.layout().borrow_mut().tb_add_text(dw, b"[VIDEO]", 7, &ws);
        }
    }
    h.core.in_flags |= in_flags::MEDIA;
}

/// `Html_tag_open_audio` (html.cc:2557).
pub fn tag_open_audio(h: &mut DilloHtml, tag: &[u8]) {
    if h.core.in_flags & in_flags::MEDIA != 0 {
        tracing::warn!("<audio> not handled when already inside a media element.");
        return;
    }
    if let Some(attrbuf) = h.get_attr(tag, b"src") {
        let Some(url) = h.url_new(Some(&attrbuf), None, false) else {
            return;
        };
        if capi::capi_get_flags_with_redirection(&url) & capi::CAPI_IS_CACHED != 0 {
            h.se.set_pseudo_visited();
        } else {
            h.se.set_pseudo_link();
        }
        let link = h.set_new_link(Some(url));
        h.se.set_non_css_hint_int(Prop::PropertyXLink, CssValueType::Integer, link);
        if let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) {
            h.layout().borrow_mut().tb_add_text(dw, b"[AUDIO]", 7, &ws);
        }
    }
    h.core.in_flags |= in_flags::MEDIA;
}

/// `Html_tag_open_source` (html.cc:2590).
pub fn tag_open_source(h: &mut DilloHtml, tag: &[u8]) {
    if h.core.in_flags & in_flags::MEDIA == 0 {
        // `<source>` peut aussi être dans <picture> (verbatim : pas
        // de BUG_MSG — commentaire amont).
        return;
    }
    let Some(attrbuf) = h.get_attr(tag, b"src") else {
        h.bug_message("<source> requires src attribute.");
        return;
    };
    let Some(url) = h.url_new(Some(&attrbuf), None, false) else {
        return;
    };
    if capi::capi_get_flags_with_redirection(&url) & capi::CAPI_IS_CACHED != 0 {
        h.se.set_pseudo_visited();
    } else {
        h.se.set_pseudo_link();
    }
    let link = h.set_new_link(Some(url));
    h.se.set_non_css_hint_int(Prop::PropertyXLink, CssValueType::Integer, link);
}

/// `Html_tag_content_source` (html.cc:2618).
pub fn tag_content_source(h: &mut DilloHtml, tag: &[u8]) {
    if h.core.in_flags & in_flags::MEDIA != 0
        && h.get_attr(tag, b"src").is_some()
        && let (Some(dw), Some(ws)) = (h.core.dw, h.word_style())
    {
        h.layout()
            .borrow_mut()
            .tb_add_text(dw, b"[MEDIA SOURCE]", 14, &ws);
    }
}

/// `Html_tag_close_media` (html.cc:2628).
pub fn tag_close_media(h: &mut DilloHtml) {
    h.core.in_flags &= !in_flags::MEDIA;
}

/// `Html_tag_open_embed` (html.cc:2637).
pub fn tag_open_embed(h: &mut DilloHtml, tag: &[u8]) {
    if let Some(attrbuf) = h.get_attr(tag, b"src")
        && let Some(url) = h.url_new(Some(&attrbuf), None, false)
    {
        if capi::capi_get_flags_with_redirection(&url) & capi::CAPI_IS_CACHED != 0 {
            h.se.set_pseudo_visited();
        } else {
            h.se.set_pseudo_link();
        }
        let link = h.set_new_link(Some(url));
        h.se.set_non_css_hint_int(Prop::PropertyXLink, CssValueType::Integer, link);
    }
}

/// `Html_tag_content_embed` (html.cc:2657).
pub fn tag_content_embed(h: &mut DilloHtml, tag: &[u8]) {
    if h.get_attr(tag, b"src").is_some()
        && let (Some(dw), Some(ws)) = (h.core.dw, h.word_style())
    {
        h.layout().borrow_mut().tb_add_text(dw, b"[EMBED]", 7, &ws);
    }
}

// ------------------------------------------------------------------
// <A>, <Q>, listes, <HR>, <DL>/<DT>/<DD>, <PRE>, <BLOCKQUOTE>
// ------------------------------------------------------------------

/// `Html_tag_open_a` (html.cc:2708) — href (javascript extrait),
/// pseudo link/visited + couleurs non-CSS, `x_link`, title→tooltip,
/// name→ancre (comparée à `id` url-décodée).
pub fn tag_open_a(h: &mut DilloHtml, tag: &[u8]) {
    h.core.in_flags |= in_flags::A;
    if h.core.in_flags & in_flags::MAP != 0 {
        tag_content_area(h, tag);
    }

    if let Some(mut attrbuf) = h.get_attr(tag, b"href") {
        // lien javascript → extraire la référence.
        if attrbuf
            .first()
            .is_some_and(|&c| c.eq_ignore_ascii_case(&b'j'))
        {
            attrbuf = h.get_javascript_link(&attrbuf);
        }
        let url = h.url_new(Some(&attrbuf), None, false);
        if let Some(url) = url {
            if capi::capi_get_flags_with_redirection(&url) & capi::CAPI_IS_CACHED != 0 {
                h.core.in_visited_link = true;
                h.se.set_pseudo_visited();
                if h.core.non_css_visited_color != -1 {
                    h.se.set_non_css_hint_int(
                        Prop::Color,
                        CssValueType::Color,
                        h.core.non_css_visited_color,
                    );
                }
            } else {
                h.se.set_pseudo_link();
                if h.core.non_css_link_color != -1 {
                    h.se.set_non_css_hint_int(
                        Prop::Color,
                        CssValueType::Color,
                        h.core.non_css_link_color,
                    );
                }
            }
            let link = h.set_new_link(Some(url));
            h.se.set_non_css_hint_int(Prop::PropertyXLink, CssValueType::Integer, link);
        }
    }
    if h.prefs.show_tooltip
        && let Some(attrbuf) = h.get_attr(tag, b"title")
    {
        h.se.set_non_css_hint_str(Prop::PropertyXTooltip, CssValueType::String, &attrbuf);
    }

    h.se.inherit_background_color();

    if let Some(attrbuf) = h.get_attr(tag, b"name") {
        if h.prefs.show_extra_warnings {
            h.check_name_val(&attrbuf, "name");
        }
        let name_val = url::decode_hex_str(&attrbuf);
        // comparaison `id` vs `name` décodé (verbatim).
        let id = h.se.get_id().map(<[u8]>::to_vec);
        if id.as_deref() != Some(name_val.as_slice()) {
            if let Some(id) = &id {
                h.bug_message(&format!(
                    "In <a>, id ('{}') and name ('{}') attributes differ.",
                    String::from_utf8_lossy(id),
                    String::from_utf8_lossy(&name_val)
                ));
            }
            h.add_anchor(&name_val);
        }
    }
}

/// `Html_tag_close_a` (html.cc:2768).
pub fn tag_close_a(h: &mut DilloHtml) {
    h.core.in_flags &= !in_flags::A;
    h.core.in_visited_link = false;
}

/// `Html_tag_open_blockquote` (html.cc:2778).
pub fn tag_open_blockquote(h: &mut DilloHtml, _tag: &[u8]) {
    h.add_textblock(true, 9, false);
}

/// `Html_tag_open_q` (html.cc:2786) — “.
pub fn tag_open_q(h: &mut DilloHtml, _tag: &[u8]) {
    h.se.inherit_background_color();
    if let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) {
        h.layout()
            .borrow_mut()
            .tb_add_text(dw, "\u{201c}".as_bytes(), 3, &ws);
    }
}

/// `Html_tag_close_q` (html.cc:2799) — ”.
pub fn tag_close_q(h: &mut DilloHtml) {
    if let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) {
        h.layout()
            .borrow_mut()
            .tb_add_text(dw, "\u{201d}".as_bytes(), 3, &ws);
    }
}

/// `Html_tag_open_ul` (html.cc:2809) — `type` → list-style-type.
pub fn tag_open_ul(h: &mut DilloHtml, tag: &[u8]) {
    use dillo_dw::style::ListStyleType as LST;
    if let Some(attrbuf) = h.get_attr(tag, b"type") {
        let lst = if str_icmp(&attrbuf, b"disc") == 0 {
            LST::Disc
        } else if str_icmp(&attrbuf, b"circle") == 0 {
            LST::Circle
        } else if str_icmp(&attrbuf, b"square") == 0 {
            LST::Square
        } else {
            LST::Disc
        };
        h.se.set_non_css_hint_int(Prop::ListStyleType, CssValueType::Enum, lst as i32);
        if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
            h.bug_message("<ul> type attribute is obsolete.");
        }
    }
    let t = h.s_top_mut();
    t.list_type = ListMode::Unordered;
    t.list_number = 0;
    t.ref_list_item = None;
}

/// `Html_tag_open_dir` (html.cc:2845) — obsolète ≈ `<ul>`.
pub fn tag_open_dir(h: &mut DilloHtml, _tag: &[u8]) {
    h.se.inherit_background_color();
    if let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) {
        h.layout().borrow_mut().tb_add_parbreak(dw, 9, &ws);
    }
    let t = h.s_top_mut();
    t.list_type = ListMode::Unordered;
    t.list_number = 0;
    t.ref_list_item = None;
    if h.prefs.show_extra_warnings {
        h.bug_message("Obsolete list type; use <ul> instead.");
    }
}

/// `Html_tag_open_menu` (html.cc:2868) — `<menu>` HTML5 ≠ ancien
/// sens : ignoré en HTML5.
pub fn tag_open_menu(h: &mut DilloHtml, tag: &[u8]) {
    if !(h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0) {
        tag_open_dir(h, tag);
    }
}

/// `Html_tag_open_ol` (html.cc:2886) — `type` + `start`.
#[allow(clippy::match_same_arms)] // verbatim : `'1'` explicite + `default` → Decimal.
pub fn tag_open_ol(h: &mut DilloHtml, tag: &[u8]) {
    use dillo_dw::style::ListStyleType as LST;
    if let Some(attrbuf) = h.get_attr(tag, b"type") {
        let lst = match attrbuf.first().copied().unwrap_or(0) {
            b'1' => LST::Decimal,
            b'a' => LST::LowerAlpha,
            b'A' => LST::UpperAlpha,
            b'i' => LST::LowerRoman,
            b'I' => LST::UpperRoman,
            _ => LST::Decimal,
        };
        h.se.set_non_css_hint_int(Prop::ListStyleType, CssValueType::Enum, lst as i32);
    }

    h.s_top_mut().list_type = ListMode::Ordered;

    let mut n = 1i32;
    if let Some(attrbuf) = h.get_attr(tag, b"start") {
        let (v, _) = dlib::strtol(&attrbuf, 10);
        n = v;
        if n < 0 {
            h.bug_message("Illegal '-' character in START attribute; Starting from 0.");
            n = 0;
        }
    }
    let t = h.s_top_mut();
    t.list_number = n;
    t.ref_list_item = None;
}

/// `Html_tag_open_li` (html.cc:2920) — warning hors liste ;
/// `value` écrit dans le `list_number` du parent (stack[len-2]).
pub fn tag_open_li(h: &mut DilloHtml, tag: &[u8]) {
    if h.s_top().list_type == ListMode::None
        && !(h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0)
    {
        // HTML5 (WHATWG/5.1) autorise <li> dans <menu> toolbar.
        h.bug_message("<li> outside <ul> or <ol>.");
    }

    h.core.in_flags |= in_flags::LI;

    // `list_number` de l'état parent (verbatim `size()-2`).
    let parent_idx = h.core.stack.len().saturating_sub(2);
    if h.style()
        .is_some_and(|s| s.attrs.list_style_type >= dillo_dw::style::ListStyleType::Decimal)
        && let Some(attrbuf) = h.get_attr(tag, b"value")
    {
        let (v, _) = dlib::strtol(&attrbuf, 10);
        if v < 0 {
            h.bug_message("Illegal negative list value attribute; Starting from 0.");
            if let Some(p) = h.core.stack.get_mut(parent_idx) {
                p.list_number = 0;
            }
        } else if let Some(p) = h.core.stack.get_mut(parent_idx) {
            p.list_number = v;
        }
    }
}

/// `Html_tag_close_li` (html.cc:2950).
pub fn tag_close_li(h: &mut DilloHtml) {
    h.core.in_flags &= !in_flags::LI;
    if let Some(dw) = h.core.dw {
        h.layout().borrow_mut().tb_flush(dw);
    }
}

/// `Html_tag_open_hr` (html.cc:2959) — width/size/align/noshade.
pub fn tag_open_hr(h: &mut DilloHtml, tag: &[u8]) {
    let mut size: i32 = 0;

    if let Some(width_ptr) = h.get_attr(tag, b"width") {
        if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
            h.bug_message("<hr> width attribute is obsolete.");
        }
        let l = h.parse_length(&width_ptr);
        h.se.set_non_css_hint_len(Prop::Width, CssValueType::LengthPercentage, l);
    }

    if let Some(attrbuf) = h.get_attr(tag, b"size") {
        let (v, _) = dlib::strtol(&attrbuf, 10);
        size = v;
        if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
            h.bug_message("<hr> size attribute is obsolete.");
        }
    }

    h.tag_set_align_attr(tag);

    if h.get_attr(tag, b"noshade").is_some() {
        if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
            h.bug_message("<hr> noshade attribute is obsolete.");
        }
        let solid = dillo_dw::style::BorderStyle::Solid as i32;
        h.se.set_non_css_hint_int(Prop::BorderTopStyle, CssValueType::Enum, solid);
        h.se.set_non_css_hint_int(Prop::BorderBottomStyle, CssValueType::Enum, solid);
        h.se.set_non_css_hint_int(Prop::BorderLeftStyle, CssValueType::Enum, solid);
        h.se.set_non_css_hint_int(Prop::BorderRightStyle, CssValueType::Enum, solid);
        if size <= 0 {
            size = 1;
        }
    }

    if size > 0 {
        let size_top = css_create_length(((size + 1) / 2) as f32, CssLengthType::Px);
        let size_bottom = css_create_length((size / 2) as f32, CssLengthType::Px);
        h.se.set_non_css_hint_len(
            Prop::BorderTopWidth,
            CssValueType::LengthPercentage,
            size_top,
        );
        h.se.set_non_css_hint_len(
            Prop::BorderLeftWidth,
            CssValueType::LengthPercentage,
            size_top,
        );
        h.se.set_non_css_hint_len(
            Prop::BorderBottomWidth,
            CssValueType::LengthPercentage,
            size_bottom,
        );
        h.se.set_non_css_hint_len(
            Prop::BorderRightWidth,
            CssValueType::LengthPercentage,
            size_bottom,
        );
    }
}

/// `Html_tag_content_hr` (html.cc:3020) — widget `Ruler`.
pub fn tag_content_hr(h: &mut DilloHtml, _tag: &[u8]) {
    let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) else {
        return;
    };
    let Some(style) = h.style() else { return };
    let layout = h.layout();
    let mut arena = layout.borrow_mut();
    arena.tb_add_parbreak(dw, 5, &ws);
    let ruler = arena.ruler_new();
    arena.tb_add_widget(dw, ruler, &style);
    arena.tb_add_parbreak(dw, 5, &ws);
}

/// `Html_tag_open_dl` (html.cc:3031).
pub fn tag_open_dl(h: &mut DilloHtml, _tag: &[u8]) {
    h.se.inherit_background_color();
    if let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) {
        h.layout().borrow_mut().tb_add_parbreak(dw, 9, &ws);
    }
}

/// `Html_tag_open_dt` (html.cc:3041).
pub fn tag_open_dt(h: &mut DilloHtml, _tag: &[u8]) {
    h.se.inherit_background_color();
    if let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) {
        h.layout().borrow_mut().tb_add_parbreak(dw, 9, &ws);
    }
}

/// `Html_tag_open_dd` (html.cc:3057).
pub fn tag_open_dd(h: &mut DilloHtml, _tag: &[u8]) {
    h.add_textblock(true, 9, false);
}

/// `Html_tag_open_pre` (html.cc:3066).
pub fn tag_open_pre(h: &mut DilloHtml, _tag: &[u8]) {
    h.se.inherit_background_color();
    if let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) {
        h.layout().borrow_mut().tb_add_parbreak(dw, 9, &ws);
    }
    h.core.in_flags |= in_flags::PRE;
}

/// `Html_tag_close_pre` (html.cc:3077).
pub fn tag_close_pre(h: &mut DilloHtml) {
    h.core.in_flags &= !in_flags::PRE;
}

// ------------------------------------------------------------------
// <META>, <LINK>, <BASE>
// ------------------------------------------------------------------

impl DilloHtml<'_> {
    /// `Html_update_content_type` (html.cc:3106) — pousse le type
    /// META dans le cache ; changement → stop_parser + repush.
    fn update_content_type(&mut self, content: &[u8]) {
        let page_url = self.core.shared.borrow().page_url.clone();
        let new_content = capi::capi_set_content_type(&page_url, content, "meta");
        // verbatim `a_Misc_content_type_cmp` — != → buffer caduc.
        if !dillo_common::misc::content_type_cmp(
            Some(self.core.content_type.as_slice()),
            new_content.as_deref(),
        ) {
            self.core.stop_parser = true;
            self.sink().borrow_mut().repush();
        }
    }
}

/// `Html_tag_open_meta` (html.cc:3127) — refresh / content-type /
/// charset verbatim (refresh 0 seulement).
#[allow(clippy::float_cmp)] // verbatim `doc_type_version == 5.0` amont.
pub fn tag_open_meta(h: &mut DilloHtml, tag: &[u8]) {
    const META_TEMPLATE: &str = "<table width='100%'><tr><td bgcolor='#ee0000'>Warning:</td>\n <td bgcolor='#8899aa' width='100%'>\n This page uses the NON-STANDARD meta refresh tag.<br> The HTML 4.01 SPEC\n (sec 7.4.4) recommends explicitly to avoid it.</td></tr>\n <tr><td bgcolor='#a0a0a0' colspan='2'>The author wanted you to go\n <a href='{}'>here</a>{}</td></tr></table><br>\n";

    // valide seulement dans HEAD.
    if h.core.in_flags & in_flags::HEAD == 0 {
        let itemprop_ok = h.core.doc_type == DocType::Html
            && h.core.doc_type_version >= 5.0
            && h.get_attr(tag, b"itemprop").is_some();
        if !itemprop_ok {
            h.bug_message("This <meta> element must be inside the HEAD section.");
        }
        return;
    }

    if let Some(equiv) = h.get_attr(tag, b"http-equiv") {
        if str_icmp(&equiv, b"refresh") == 0
            && let Some(content) = h.get_attr(tag, b"content")
        {
            // délai éventuel → message (verbatim `strtol` base 0).
            let (delay, _) = dlib::strtol(&content, 0);
            let delay_str = if delay != 0 {
                format!(
                    " after {} second{}.",
                    delay,
                    if delay > 1 { "s" } else { "" }
                )
            } else {
                ".".to_string()
            };
            // saute jusqu'à après "URL=" ou ";" si "URL=" absent.
            let mut has_url = true;
            let mut rest: &[u8] = &content;
            if let Some(p) = dlib::stri_ascii_str(&content, b"url=") {
                rest = &content[p + 4..];
            } else if let Some(p) = content.iter().position(|&c| c == b';') {
                rest = &content[p + 1..];
            } else {
                has_url = false;
            }

            let mr_url: Vec<u8> = if has_url {
                // URL éventuellement quotée.
                let q = rest.first().copied().unwrap_or(0);
                if q == b'"' || q == b'\'' {
                    match rest[1..].iter().position(|&c| c == q) {
                        Some(p) => rest[1..=p].to_vec(),
                        None => rest[1..].to_vec(),
                    }
                } else {
                    rest.to_vec()
                }
            } else {
                Vec::new()
            };

            // verbatim : `a_Url_cmp(base_url, new_url) == 0` →
            // boucle/URL vide (BUG `*mr_url ? loop : no target`).
            let Some(mut new_url) = h.url_new(Some(&mr_url), None, false) else {
                return;
            };
            if h.core.shared.borrow().base_url.url_cmp(&new_url) == 0 {
                h.bug_message(&format!(
                    "<meta> refresh: {}.",
                    if mr_url.is_empty() {
                        "no target URL"
                    } else {
                        "redirection loop"
                    }
                ));
            } else if delay == 0 {
                // redirection immédiate.
                h.core.stop_parser = true;
                let spam_safe = h.core.shared.borrow().base_url.flags & url::flags::SPAM_SAFE != 0;
                if spam_safe {
                    h.sink()
                        .borrow_mut()
                        .set_msg(b"WARNING: local URL with META refresh.  Aborting.");
                } else {
                    let bw = h.sink().borrow().bw_id();
                    if capi::capi_dpi_verify_request(bw, &new_url) {
                        h.sink().borrow_mut().redirection0(new_url);
                    }
                }
            } else {
                // message HTML spécial (verbatim : hack IN_BODY +
                // META_HACK pour écrire le bloc).
                let url_s = String::from_utf8_lossy(new_url.str_()).into_owned();
                let msg = META_TEMPLATE
                    .replacen("{}", &url_s, 1)
                    .replacen("{}", &delay_str, 1);
                let o_flags = h.core.in_flags;
                let o_soup = h.core.tag_soup;
                h.core.in_flags = in_flags::BODY + in_flags::META_HACK;
                h.core.tag_soup = false;
                h.write_raw(msg.as_bytes(), msg.len(), false);
                h.core.tag_soup = o_soup;
                h.core.in_flags = o_flags;
            }
        } else if str_icmp(&equiv, b"content-type") == 0
            && let Some(content) = h.get_attr(tag, b"content")
        {
            h.update_content_type(&content);
        }
    } else if h.core.doc_type == DocType::Html
        && h.core.doc_type_version == 5.0
        && let Some(charset) = h.get_attr(tag, b"charset")
    {
        let content = [b"text/html; charset=".as_ref(), &charset].concat();
        h.update_content_type(&content);
    }
}

/// `Html_tag_open_link` (html.cc:3313) — `rel=stylesheet` dans HEAD
/// → `addCssUrl`.
pub fn tag_open_link(h: &mut DilloHtml, tag: &[u8]) {
    // SpamSafe → ne pas charger LINK (mails suspects).
    if h.core.shared.borrow().base_url.flags & url::flags::SPAM_SAFE != 0 {
        return;
    }

    // LINK hors HEAD ignoré (sauf itemprop HTML5).
    if h.core.in_flags & in_flags::HEAD == 0 {
        let itemprop_ok = h.core.doc_type == DocType::Html
            && h.core.doc_type_version >= 5.0
            && h.get_attr(tag, b"itemprop").is_some();
        if !itemprop_ok {
            h.bug_message("This <link> element must be inside the HEAD section.");
        }
        return;
    }
    if !h.prefs.load_stylesheets {
        return;
    }
    // feuille CSS seulement.
    match h.get_attr(tag, b"rel") {
        None => return,
        Some(rel) if str_icmp(&rel, b"stylesheet") != 0 => return,
        _ => {}
    }

    // attributs `type`/`media` restrictifs → ignorés.
    if (h
        .get_attr(tag, b"type")
        .is_some_and(|t| str_icmp(&t, b"text/css") != 0))
        || (h.get_attr(tag, b"media").is_some_and(|m| {
            dlib::stri_ascii_str(&m, b"screen").is_none() && str_icmp(&m, b"all") != 0
        }))
    {
        return;
    }

    let Some(attrbuf) = h.get_attr(tag, b"href") else {
        return;
    };
    let Some(url) = h.url_new(Some(&attrbuf), None, false) else {
        return;
    };

    // `addCssUrl`.
    h.core.shared.borrow_mut().css_urls.push(url);
}

/// `Html_tag_open_base` (html.cc:3363) — document base URI.
pub fn tag_open_base(h: &mut DilloHtml, tag: &[u8]) {
    if h.core.in_flags & in_flags::HEAD != 0 {
        if let Some(attrbuf) = h.get_attr(tag, b"href") {
            let html5 = h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0;
            let base = if html5 {
                h.url_new(Some(&attrbuf), None, false)
            } else {
                h.url_new(Some(&attrbuf), Some(b""), true)
            };
            let Some(mut base) = base else { return };
            if html5 || base.scheme.is_some() {
                // passe le flag SpamSafe au nouveau base_url.
                let old_flags = h.core.shared.borrow().base_url.flags;
                base.flags = old_flags & url::flags::SPAM_SAFE;
                h.core.shared.borrow_mut().base_url = base;
            } else {
                h.bug_message("<base> URI is relative (it MUST be absolute).");
            }
        }
    } else {
        h.bug_message("<base> not inside HEAD section.");
    }
}

// ------------------------------------------------------------------
// Html_process_space / Html_process_word (html.cc:1114-1322)
// ------------------------------------------------------------------

impl DilloHtml<'_> {
    /// `Html_process_space_pre_line` (html.cc:1118).
    fn process_space_pre_line(&mut self, space: &[u8]) {
        let mut break_cnt = 0;
        for &c in space {
            // support "\r", "\n" et "\r\n".
            if c == b'\r' || (c == b'\n' && !self.core.prev_was_cr) {
                break_cnt += 1;
                self.core.prev_was_cr = c == b'\r';
                let Some(dw) = self.core.dw else { continue };
                let Some(ws) = self.word_style() else {
                    continue;
                };
                self.core
                    .shared
                    .borrow()
                    .layout
                    .borrow_mut()
                    .tb_add_linebreak(dw, &ws);
            }
        }
        if break_cnt == 0
            && let (Some(dw), Some(ws)) = (self.core.dw, self.word_style())
        {
            self.core
                .shared
                .borrow()
                .layout
                .borrow_mut()
                .tb_add_space(dw, &ws);
        }
    }

    /// `Html_process_space` (html.cc:1140).
    fn process_space(&mut self, space: &[u8]) {
        let parse_mode = self.s_top().parse_mode;

        if self.s_top().display_none {
            // do nothing
        } else if parse_mode == ParseMode::Stash {
            self.core.stash_space = !self.core.stash.is_empty();
        } else if parse_mode == ParseMode::Verbatim {
            self.core.stash.extend_from_slice(space);
        } else if parse_mode == ParseMode::Pre {
            let mut space_cnt = 0i32;
            for &c in space {
                // "\r", "\n" et "\r\n" (skips le premier — PreFirstChar).
                if !self.core.pre_first_char
                    && (c == b'\r' || (c == b'\n' && !self.core.prev_was_cr))
                {
                    if space_cnt > 0 {
                        let spc = vec![b' '; space_cnt as usize];
                        if let (Some(dw), Some(ws)) = (self.core.dw, self.word_style()) {
                            self.core.shared.borrow().layout.borrow_mut().tb_add_text(
                                dw,
                                &spc,
                                spc.len(),
                                &ws,
                            );
                        }
                        space_cnt = 0;
                    }
                    if let (Some(dw), Some(ws)) = (self.core.dw, self.word_style()) {
                        self.core
                            .shared
                            .borrow()
                            .layout
                            .borrow_mut()
                            .tb_add_linebreak(dw, &ws);
                    }
                    self.core.pre_column = 0;
                }
                self.core.pre_first_char = false;

                // cr et lf ne se rendent pas — ils deviennent un break.
                match c {
                    b'\r' | b'\n' => {}
                    b'\t' => {
                        if self.prefs.show_extra_warnings {
                            self.bug_message("TAB character inside <pre>.");
                        }
                        let offset = TAB_SIZE - self.core.pre_column % TAB_SIZE;
                        space_cnt += offset;
                        self.core.pre_column += offset;
                    }
                    _ => {
                        space_cnt += 1;
                        self.core.pre_column += 1;
                    }
                }
                self.core.prev_was_cr = c == b'\r';
            }
            if space_cnt > 0 {
                // break possibility pour white-space: pre-wrap.
                let Some(dw) = self.core.dw else { return };
                let Some(ws) = self.word_style() else { return };
                let spc = vec![b' '; space_cnt as usize];
                let arena_rc = self.core.shared.borrow().layout.clone();
                let mut arena = arena_rc.borrow_mut();
                arena.tb_add_break_option(dw, &ws, false);
                arena.tb_add_text(dw, &spc, spc.len(), &ws);
            }
        } else {
            if SGML_SPCDEL {
                // SGML_SPCDEL ignore les espaces après une balise ouvrante.
            } else if self
                .word_style()
                .is_some_and(|ws| ws.white_space == dillo_dw::style::WhiteSpace::PreLine)
            {
                self.process_space_pre_line(space);
            } else if let (Some(dw), Some(ws)) = (self.core.dw, self.word_style()) {
                self.core
                    .shared
                    .borrow()
                    .layout
                    .borrow_mut()
                    .tb_add_space(dw, &ws);
            }
            if parse_mode == ParseMode::StashAndBody {
                self.core.stash_space = !self.core.stash.is_empty();
            }
        }
    }

    /// `Html_process_word` (html.cc:1226).
    fn process_word(&mut self, word: &[u8]) {
        let parse_mode = self.s_top().parse_mode;

        if self.s_top().display_none {
            return;
        }
        let i = if self.core.prev_was_html_close {
            1
        } else if self.core.prev_was_body_close {
            2
        } else {
            0
        };
        if i != 0 {
            self.bug_message(&format!(
                "Content after </{}> tag.",
                if i == 1 { "html" } else { "body" }
            ));
            self.core.prev_was_html_close = false;
            self.core.prev_was_body_close = false;
        }

        if parse_mode == ParseMode::Stash || parse_mode == ParseMode::StashAndBody {
            if self.core.stash_space {
                self.core.stash.push(b' ');
                self.core.stash_space = false;
            }
            let pword = self.parse_entities(word);
            self.core.stash.extend_from_slice(&pword);
        } else if parse_mode == ParseMode::Verbatim {
            self.core.stash.extend_from_slice(word);
        }

        if parse_mode == ParseMode::Stash || parse_mode == ParseMode::Verbatim {
            // skip jusqu'aux instructions de fermeture.
        } else if parse_mode == ParseMode::Pre {
            // overhead pour attraper les entités whitespace.
            let pword = self.parse_entities(word);
            let mut i = 0usize;
            while i < pword.len() {
                let start = i;
                if dlib::is_space(pword[i]) {
                    i += 1;
                    while i < pword.len() && dlib::is_space(pword[i]) {
                        i += 1;
                    }
                    self.process_space(&pword[start..i]);
                } else {
                    i += 1;
                    while i < pword.len() && !dlib::is_space(pword[i]) {
                        i += 1;
                    }
                    if let (Some(dw), Some(ws)) = (self.core.dw, self.word_style()) {
                        self.core.shared.borrow().layout.borrow_mut().tb_add_text(
                            dw,
                            &pword[start..i],
                            i - start,
                            &ws,
                        );
                    }
                    self.core.pre_column += (i - start) as i32;
                    self.core.pre_first_char = false;
                }
            }
        } else {
            let owned: Vec<u8>;
            let word2: &[u8] = if word.contains(&b'&') {
                // collapse les entités whitespace (sauf &nbsp;) puis
                // " \t\f\n\r" adjacents en un espace — verbatim de
                // la boucle in-place.
                owned = collapse_ws_entities(&self.parse_entities(word));
                &owned
            } else {
                word
            };
            let beyond = word2.len();
            let mut i = 0usize;
            while i < word2.len() {
                let start = i;
                if dlib::is_space(word2[i]) {
                    i += 1;
                    while i < word2.len() && dlib::is_space(word2[i]) {
                        i += 1;
                    }
                    self.process_space(&word2[start..i]);
                } else if word2[i..].starts_with(utf8::UTF8_ZERO_WIDTH_SPACE) {
                    i += 3;
                    if let (Some(dw), Some(ws)) = (self.core.dw, self.word_style()) {
                        self.core
                            .shared
                            .borrow()
                            .layout
                            .borrow_mut()
                            .tb_add_break_option(dw, &ws, false);
                    }
                } else {
                    let (ideo, len) = utf8::ideographic(word2, i, beyond);
                    if ideo {
                        i += len;
                        if let (Some(dw), Some(ws)) = (self.core.dw, self.word_style()) {
                            self.core.shared.borrow().layout.borrow_mut().tb_add_text(
                                dw,
                                &word2[start..i],
                                i - start,
                                &ws,
                            );
                        }
                        if let (Some(dw), Some(ws)) = (self.core.dw, self.word_style()) {
                            self.core
                                .shared
                                .borrow()
                                .layout
                                .borrow_mut()
                                .tb_add_break_option(dw, &ws, false);
                        }
                    } else {
                        // mot normal : scan jusqu'à espace/zwsp/ideographic.
                        i += len;
                        while i < word2.len()
                            && !dlib::is_space(word2[i])
                            && !word2[i..].starts_with(utf8::UTF8_ZERO_WIDTH_SPACE)
                        {
                            let (is_ideo, l) = utf8::ideographic(word2, i, beyond);
                            if is_ideo {
                                break;
                            }
                            i += l.max(1);
                        }
                        if let (Some(dw), Some(ws)) = (self.core.dw, self.word_style()) {
                            self.core.shared.borrow().layout.borrow_mut().tb_add_text(
                                dw,
                                &word2[start..i],
                                i - start,
                                &ws,
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Boucle de collapsing verbatim `Pword` (html.cc:1285) —
/// `" \t\f\n\r"` adjacents → un espace (`&nbsp;` produit `0xC2 0xA0`,
/// pas dans le set → conservé).
fn collapse_ws_entities(pword: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pword.len());
    let mut j = 0usize;
    while j < pword.len() {
        let c = pword[j];
        if b" \t\x0c\n\r".contains(&c) {
            if out.last() != Some(&b' ') {
                out.push(b' ');
            }
            // skip tous les whitespace suivants.
            while j + 1 < pword.len() && b" \t\x0c\n\r".contains(&pword[j + 1]) {
                j += 1;
            }
        } else {
            out.push(c);
        }
        j += 1;
    }
    out
}
