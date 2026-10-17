//! Port of `src/plain.cc` — décodeur `text/plain` vers `Textblock`.
//!
//! `DilloPlain` verbatim : FSM ligne à ligne (`ST_SeekingEol`/
//! `ST_Eol`/`ST_Eof`), `addLine` éclate en mots de ≤ 128 octets via
//! `a_Misc_expand_tabs`, ancres `L<n>` par ligne, `parbreak` final.
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

use std::cell::RefCell;
use std::rc::Rc;

use dillo_common::misc;
use dillo_css::parser::NullTagIndex;
use dillo_css::styleengine::StyleEngine;
use dillo_dw::WidgetId;
use dillo_dw::events::EventButton;
use dillo_dw::layout::Layout;
use dillo_dw::layout::LinkReceiver;
use dillo_dw::style::Style;

use crate::sink::{HtmlPrefs, HtmlSink};
use crate::tags;

/* FSM states */
/// `ST_SeekingEol`.
const ST_SEEKING_EOL: i32 = 0;
/// `ST_Eol`.
const ST_EOL: i32 = 1;

/// `PlainLinkReceiver` — clic droit → `a_UIcmd_page_popup`.
struct PlainLinkReceiver {
    /// `plain->bw` ≙ sink.
    sink: Rc<RefCell<dyn HtmlSink>>,
}

impl LinkReceiver for PlainLinkReceiver {
    /// `PlainLinkReceiver::press` (plain.cc:125).
    fn pressed(
        &mut self,
        _widget: Option<WidgetId>,
        _link: i32,
        _img: i32,
        _x: i32,
        _y: i32,
        event: &EventButton,
    ) -> bool {
        tracing::trace!("DilloPlain::PlainLinkReceiver::buttonPress");
        if event.button == 3 {
            self.sink.borrow_mut().page_popup(false, &[]);
            return true;
        }
        false
    }
}

/// `DilloPlain` — document `text/plain`.
pub struct DilloPlain {
    /// `bw` ≙ sink.
    sink: Rc<RefCell<dyn HtmlSink>>,
    /// `dw` — le `Textblock` racine (≙ valeur de retour de
    /// `a_Plain_text`).
    pub dw: WidgetId,
    /// `widgetStyle`.
    widget_style: Rc<Style>,
    /// `Start_Ofs` — offset de lecture du prochain chunk.
    pub start_ofs: usize,
    /// `state`.
    state: i32,
    /// `currentLine`.
    current_line: i64,
    /// `HT2LT` (≙ `bw->render_layout` — porte l'arène partagée).
    layout: Rc<RefCell<Layout>>,
}

impl DilloPlain {
    /// `DilloPlain (bw)` + `a_Plain_text` (plain.cc:89, 218) —
    /// construit le textblock, le style `<body><pre>`, connecte le
    /// receiver et renvoie `(plain, widget)`.
    pub fn new(
        sink: Rc<RefCell<dyn HtmlSink>>,
        layout: Rc<RefCell<Layout>>,
        prefs: &Rc<HtmlPrefs>,
    ) -> Self {
        let dw = layout.borrow_mut().tb_new(prefs.limit_text_width, false);

        let widget_style = {
            let zoom = sink.borrow().zoom();
            let mut lay = layout.borrow_mut();
            let mut ti = NullTagIndex;
            let mut se = StyleEngine::new(&mut lay, None, None, zoom, &prefs.style, &mut ti);
            se.start_element(*tags::I_BODY, &mut lay, None);
            se.start_element(*tags::I_PRE, &mut lay, None);
            se.word_style(&mut lay, None)
        };
        /* `wordStyle` amont est non-nul après `startElement` —
         * fallback dégradé : `StyleAttrs` par défaut. */
        let widget_style =
            widget_style.unwrap_or_else(|| Style::create(&dillo_dw::style::StyleAttrs::default()));

        /* The context menu */
        layout
            .borrow_mut()
            .connect_link(Rc::new(RefCell::new(PlainLinkReceiver {
                sink: sink.clone(),
            })));

        /* Hook destructor to the dw delete call */
        layout
            .borrow_mut()
            .widget_set_delete_callback(dw, Some(Rc::new(|| tracing::trace!("a_Plain_free!"))));

        Self::make(sink, layout, dw, widget_style)
    }

    /// aide constructeur.
    fn make(
        sink: Rc<RefCell<dyn HtmlSink>>,
        layout: Rc<RefCell<Layout>>,
        dw: WidgetId,
        style: Rc<Style>,
    ) -> Self {
        DilloPlain {
            sink,
            dw,
            widget_style: style,
            start_ofs: 0,
            state: ST_SEEKING_EOL,
            current_line: 0,
            layout,
        }
    }

    /// `addLine` (plain.cc:140).
    fn add_line(&mut self, buf: &[u8]) {
        self.current_line += 1; /* Start at 1 */
        let anchor = format!("L{}", self.current_line);
        let mut arena = self.layout.borrow_mut();
        arena.tb_add_anchor(self.dw, anchor.as_bytes(), &self.widget_style);

        if buf.is_empty() {
            // Add dummy word for empty lines - otherwise the parbreak
            // is ignored.
            arena.tb_add_text(self.dw, b"", 0, &self.widget_style);
        } else {
            // Limit word length to avoid X11 coordinate
            // overflow with extremely long lines.
            let mut chunk: &[u8] = buf;
            let mut word = [0u8; 129];
            loop {
                let len = misc::expand_tabs(&mut chunk, &mut word[..128]);
                if len == 0 {
                    break;
                }
                arena.tb_add_text(self.dw, &word[..len], len, &self.widget_style);
            }
        }

        arena.tb_add_parbreak(self.dw, 0, &self.widget_style);
    }

    /// `write` (plain.cc:164) — FSM incrémental verbatim.
    pub fn write(&mut self, buf: &[u8], eof: bool) {
        tracing::trace!("DilloPlain::write Eof={}", eof);

        let start = &buf[self.start_ofs..];
        let max_bytes = start.len();
        let mut i = 0usize;
        let mut len = 0usize;
        while i < max_bytes {
            match self.state {
                ST_SEEKING_EOL => {
                    if start[i] == b'\n' || start[i] == b'\r' {
                        self.state = ST_EOL;
                    } else {
                        i += 1;
                        len += 1;
                    }
                }
                ST_EOL => {
                    let line = start[i - len..i].to_vec();
                    self.add_line(&line);
                    if start[i] == b'\r' && start.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                    if i < max_bytes {
                        i += 1;
                    }
                    self.state = ST_SEEKING_EOL;
                    len = 0;
                }
                _ => {}
            }
        }
        self.start_ofs += i - len;
        if eof && len > 0 {
            let line = start[i - len..i].to_vec();
            self.add_line(&line);
            self.start_ofs += len;
        }

        self.layout.borrow_mut().tb_flush(self.dw);
    }

    /// `Plain_callback` (plain.cc:230) — client de cache.
    pub fn callback(&mut self, op: bool, buf: &[u8], key: i32) {
        if op {
            /* Do the last line: */
            self.write(buf, true);
            /* remove this client from our active list */
            self.sink.borrow_mut().close_client(key);
        } else {
            self.write(buf, false);
        }
    }
}
