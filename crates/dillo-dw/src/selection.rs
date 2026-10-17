//! `dw/selection.cc` — `dw::core::SelectionState` : machine à états
//! de la sélection et de l'activation des liens (verbatim).
//!
//! Adaptations modèle arène : l'état vit dans `Layout` et les
//! méthodes reçoivent `&mut Layout` (≙ champ `layout` amont) ;
//! `handleEvent` amont recaste `MousePositionEvent*` selon
//! `eventType` — le port passe l'événement typé via
//! [`SelectionMouseEvent`]. `strndup`/`StringBuffer` → `Vec<u8>`.

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use crate::events::{EventButton, EventMotion, MousePositionEvent};
use crate::iterator::{DeepIterator, DwIterator};
use crate::layout::{Layout, SelectionEvent};
use crate::types::{content_type, ContentData, HighlightLayer};

/// `core::SelectionState::END_OF_WORD` (selection.hh:201).
pub(crate) const END_OF_WORD: i32 = 1 << 30;

/// `SelectionState::selectionState` (selection.hh:207-212).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sel {
    /// `NONE`.
    None,
    /// `SELECTING`.
    Selecting,
    /// `SELECTED`.
    Selected,
}

/// `SelectionState::linkState` (selection.hh:223).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkSel {
    /// `LINK_NONE`.
    None,
    /// `LINK_PRESSED`.
    Pressed,
}

/// Événement typé pour `handle_event` — ≙ le recast
/// `(EventButton*)event`/`(EventMotion*)event` amont selon
/// `eventType` (le `MousePositionEvent` port ne conserve pas
/// `button`).
#[derive(Debug, Clone, Copy)]
pub enum SelectionMouseEvent<'a> {
    /// `BUTTON_PRESS`/`BUTTON_RELEASE` → `EventButton`.
    Button(&'a EventButton),
    /// `BUTTON_MOTION` → `EventMotion`.
    Motion(&'a EventMotion),
}

impl SelectionMouseEvent<'_> {
    /// Le `MousePositionEvent` équivalent (champs communs).
    #[must_use]
    pub fn position_event(&self) -> MousePositionEvent {
        match self {
            SelectionMouseEvent::Button(e) => MousePositionEvent::from(*e),
            SelectionMouseEvent::Motion(e) => MousePositionEvent::from(*e),
        }
    }
}

/// `dw::core::SelectionState` (selection.hh) — verbatim.
#[derive(Debug)]
pub(crate) struct SelectionState {
    /// `selectionState`.
    state: Sel,
    /// `linkState`.
    link_state: LinkSel,
    /// `from`.
    from: Option<DeepIterator>,
    /// `to`.
    to: Option<DeepIterator>,
    /// `fromChar`/`toChar`.
    from_char: i32,
    to_char: i32,
    /// `link`.
    link: Option<DeepIterator>,
    /// `linkButton`.
    link_button: i32,
    /// `linkChar`/`linkNumber`.
    link_char: i32,
    link_number: i32,
}

#[allow(clippy::similar_names)]
impl SelectionState {
    /// `SelectionState ()`.
    pub(crate) const fn new() -> Self {
        SelectionState {
            state: Sel::None,
            link_state: LinkSel::None,
            from: None,
            to: None,
            from_char: 0,
            to_char: 0,
            link: None,
            link_button: 0,
            link_char: 0,
            link_number: 0,
        }
    }

    /// `~SelectionState` → `reset`.
    pub(crate) fn reset(&mut self) {
        self.reset_selection();
        self.reset_link();
    }

    /// `resetSelection`.
    fn reset_selection(&mut self) {
        self.from = None;
        self.to = None;
        self.state = Sel::None;
    }

    /// `resetLink`.
    fn reset_link(&mut self) {
        self.link = None;
        self.link_state = LinkSel::None;
    }

    /// `buttonPress` (selection.cc:81-161) — verbatim.
    fn button_press(
        &mut self,
        layout: &mut Layout,
        it: Box<dyn DwIterator>,
        char_pos: i32,
        link_no: i32,
        event: &EventButton,
    ) -> bool {
        let it_widget = Some(it.widget());
        let mut ret = false;

        if event.button == 3 {
            // Popup menu.
            layout.emit_link_press(it_widget, link_no, -1, -1, -1, event);
            ret = true;
        } else if link_no != -1 {
            // Lien.
            let _ = layout.emit_link_press(it_widget, link_no, -1, -1, -1, event);
            self.reset_link();
            self.link_state = LinkSel::Pressed;
            self.link_button = event.button;
            let new_link = DeepIterator::new(layout, it);
            if new_link.is_empty() {
                self.reset_link();
            } else {
                let mut new_link = new_link;
                // L'utilisateur a pu presser sur quelque chose
                // d'activable (linkNo != -1) sans contenu — ex.
                // image sans texte ALT (verbatim).
                self.link_char = Self::correct_char_pos(&mut new_link, char_pos);
                self.link_number = link_no;
                self.link = Some(new_link);
            }
            // On ne retourne pas la valeur du signal — l'événement
            // est effectivement traité (verbatim).
            ret = true;
        } else if event.button == 1 {
            // Sélection normale.
            self.highlight(layout, false, 0);
            self.reset_selection();

            self.state = Sel::Selecting;
            let new_from = DeepIterator::new(layout, it);
            if new_from.is_empty() {
                self.reset_selection();
            } else {
                let mut from = new_from;
                self.from_char = Self::correct_char_pos(&mut from, char_pos);
                let mut to = from.clone();
                self.to_char = Self::correct_char_pos(&mut to, char_pos);
                self.to = Some(to);
                self.from = Some(from);
            }
            ret = true;
        }

        ret
    }

    /// `buttonRelease` (selection.cc:163-204) — verbatim.
    fn button_release(
        &mut self,
        layout: &mut Layout,
        it: Box<dyn DwIterator>,
        char_pos: i32,
        link_no: i32,
        event: &EventButton,
    ) -> bool {
        let it_widget = Some(it.widget());
        let mut ret = false;

        if self.link_state == LinkSel::Pressed && event.button == self.link_button {
            // Lien.
            ret = true;
            if link_no != -1 {
                let _ = layout.emit_link_release(it_widget, link_no, -1, -1, -1, event);
            }

            // Le lien où le bouton a été pressé ?
            if link_no == self.link_number {
                self.reset_link();
                let _ = layout.emit_link_click(it_widget, link_no, -1, -1, -1, event);
            } else if event.button == 1 {
                // Bascule vers la sélection — traitée plus bas
                // (verbatim). `it` encore nécessaire pour
                // `adjust_selection` — clone ≙ pointeur partagé.
                self.switch_link_to_selection(layout, it.clone_box(), char_pos);
            }
        }

        if self.state == Sel::Selecting && event.button == 1 {
            // Sélection normale.
            ret = true;
            self.adjust_selection(layout, it, char_pos);

            // `from`/`to` non-nuls par invariant `SELECTING`.
            let collapsed = match (self.from.as_ref(), self.to.as_ref()) {
                (Some(from), Some(to)) => {
                    from.compare_to(to) == 0 && self.from_char == self.to_char
                }
                _ => true,
            };
            if collapsed {
                // Rien de sélectionné.
                self.reset_selection();
            } else {
                self.copy(layout, 0);
                self.state = Sel::Selected;
            }
        }

        ret
    }

    /// `buttonMotion` (selection.cc:206-222) — verbatim.
    fn button_motion(
        &mut self,
        layout: &mut Layout,
        it: Box<dyn DwIterator>,
        char_pos: i32,
        link_no: i32,
        _event: &EventMotion,
    ) -> bool {
        if self.link_state == LinkSel::Pressed {
            // Lien.
            if link_no != self.link_number {
                // Plus sur le lien pressé : bascule en sélection.
                self.switch_link_to_selection(layout, it, char_pos);
            }
            // Toujours dans le lien : rien à faire (verbatim).
        } else if self.state == Sel::Selecting {
            // Sélection.
            self.adjust_selection(layout, it, char_pos);
        }

        true
    }

    /// `handleEvent` (selection.cc:228-250) — forme générale ;
    /// `it` ≙ `Iterator*` de `sendSelectionEvent`.
    pub(crate) fn handle_event(
        &mut self,
        layout: &mut Layout,
        event_type: SelectionEvent,
        it: Box<dyn DwIterator>,
        char_pos: i32,
        link_no: i32,
        event: SelectionMouseEvent<'_>,
    ) -> bool {
        match (event_type, event) {
            (SelectionEvent::ButtonPress, SelectionMouseEvent::Button(e)) => {
                self.button_press(layout, it, char_pos, link_no, e)
            }
            (SelectionEvent::ButtonRelease, SelectionMouseEvent::Button(e)) => {
                self.button_release(layout, it, char_pos, link_no, e)
            }
            (SelectionEvent::ButtonMotion, SelectionMouseEvent::Motion(e)) => {
                self.button_motion(layout, it, char_pos, link_no, e)
            }
            // `misc::assertNotReached` amont.
            _ => {
                debug_assert!(false);
                false
            }
        }
    }

    /// `switchLinkToSelection` (selection.cc:256-285) — verbatim.
    fn switch_link_to_selection(
        &mut self,
        layout: &mut Layout,
        it: Box<dyn DwIterator>,
        char_pos: i32,
    ) {
        // `link` peut être NULL (pressé sans contenu — verbatim).
        if self.link.is_some() {
            // Ancienne sélection effacée.
            self.highlight(layout, false, 0);
            self.reset_selection();

            // État lien → état sélection. `take` ≙
            // `link->cloneDeepIterator()` puis `resetLink` (delete).
            self.from = self.link.take();
            self.from_char = self.link_char;
            let mut to = DeepIterator::create_variant(layout, it);
            self.to_char = Self::correct_char_pos(&mut to, char_pos);
            self.to = Some(to);
            self.state = Sel::Selecting;

            self.reset_link();

            self.highlight(layout, true, 0);
        } else {
            // Pressé sur un lien mais rien à sélectionner.
            self.reset_selection();
            self.reset_link();
        }
    }

    /// `adjustSelection` (selection.cc:291-351) — verbatim.
    fn adjust_selection(&mut self, layout: &mut Layout, it: Box<dyn DwIterator>, char_pos: i32) {
        // `from`/`to` non-nuls par invariant `SELECTING` —
        // l'amont déréférencerait (UB) ; le port dégrade.
        let (Some(to), Some(from)) = (self.to.as_ref(), self.from.as_ref()) else {
            return;
        };
        let mut new_to = DeepIterator::create_variant(layout, it);
        let new_to_char = Self::correct_char_pos(&mut new_to, char_pos);

        let cmp_old = to.compare_to(from);
        let cmp_new = new_to.compare_to(from);

        let brute_highlighting = cmp_old == 0 || cmp_new == 0 || cmp_old * cmp_new < 0;

        if !brute_highlighting {
            // cmpOld et cmpNew équivalents et != 0 (verbatim).
            let cmp_diff = new_to.compare_to(to);

            if cmp_old * cmp_diff > 0 {
                // Sélection agrandie : surligne la différence.
                if cmp_diff < 0 {
                    let mut to_tmp = to.clone();
                    let len = Self::correct_char_pos(&mut to_tmp, END_OF_WORD);
                    Self::highlight0(
                        layout,
                        true,
                        Some(&mut new_to),
                        new_to_char,
                        self.to.as_mut(),
                        len + 1,
                        1,
                    );
                } else {
                    Self::highlight0(
                        layout,
                        true,
                        self.to.as_mut(),
                        0,
                        Some(&mut new_to),
                        new_to_char,
                        -1,
                    );
                }
            } else {
                if cmp_old * cmp_diff < 0 {
                    // Sélection réduite : dés-surligne la différence.
                    Self::highlight0(
                        layout,
                        false,
                        self.to.as_mut(),
                        0,
                        Some(&mut new_to),
                        0,
                        cmp_diff,
                    );
                }

                // Sinon petit déplacement — dans les deux cas,
                // re-surligne la nouvelle position (verbatim).
                if cmp_old < 0 {
                    let len = Self::correct_char_pos(&mut new_to, END_OF_WORD);
                    new_to.highlight(layout, new_to_char, len + 1, HighlightLayer::Selection);
                } else {
                    new_to.highlight(layout, 0, new_to_char, HighlightLayer::Selection);
                }
            }
        }

        if brute_highlighting {
            self.highlight(layout, false, 0);
        }

        self.to = Some(new_to);
        self.to_char = new_to_char;

        if brute_highlighting {
            self.highlight(layout, true, 0);
        }
    }

    /// `correctCharPos` (selection.cc:357-368) — gère le passage
    /// de `END_OF_WORD` (verbatim).
    fn correct_char_pos(it: &mut DeepIterator, char_pos: i32) -> i32 {
        let len = it.top_iterator().map_or(0, |top| {
            if top.content().r#type == content_type::TEXT {
                match &top.content().data {
                    ContentData::Text(t) => t.len() as i32,
                    _ => 1,
                }
            } else {
                1
            }
        });
        char_pos.min(len)
    }

    /// `highlight` (inline amont → `highlight0` sur les bornes
    /// courantes).
    fn highlight(&mut self, layout: &mut Layout, fl: bool, dir: i32) {
        let (from_char, to_char) = (self.from_char, self.to_char);
        Self::highlight0(
            layout,
            fl,
            self.from.as_mut(),
            from_char,
            self.to.as_mut(),
            to_char,
            dir,
        );
    }

    /// `highlight0` (selection.cc:373-434) — verbatim.
    fn highlight0(
        layout: &mut Layout,
        fl: bool,
        from: Option<&mut DeepIterator>,
        from_char: i32,
        to: Option<&mut DeepIterator>,
        to_char: i32,
        dir: i32,
    ) {
        if let (Some(from), Some(to)) = (from, to) {
            let cmp = from.compare_to(to);
            if cmp == 0 {
                if fl {
                    if from_char < to_char {
                        from.highlight(layout, from_char, to_char, HighlightLayer::Selection);
                    } else {
                        from.highlight(layout, to_char, from_char, HighlightLayer::Selection);
                    }
                } else {
                    from.unhighlight(layout, 0, HighlightLayer::Selection);
                }
                return;
            }

            let (a, a_char, b, b_char) = if cmp < 0 {
                (from, from_char, to, to_char)
            } else {
                (to, to_char, from, from_char)
            };

            let mut i = a.clone();
            let mut start = true;
            loop {
                let cmp = i.compare_to(b);
                if cmp > 0 {
                    break;
                }
                if i.content().r#type == content_type::TEXT {
                    let text_len = match &i.content().data {
                        ContentData::Text(t) => t.len() as i32,
                        _ => 0,
                    };
                    if fl {
                        if start {
                            i.highlight(layout, a_char, text_len + 1, HighlightLayer::Selection);
                        } else if cmp == 0 {
                            // La fin.
                            i.highlight(layout, 0, b_char, HighlightLayer::Selection);
                        } else {
                            i.highlight(layout, 0, text_len + 1, HighlightLayer::Selection);
                        }
                    } else {
                        i.unhighlight(layout, dir, HighlightLayer::Selection);
                    }
                }
                i.next(layout);
                start = false;
            }
        }
    }

    /// `copy` (selection.cc:436-499) — verbatim ; `strbuf`
    /// ≙ `Vec<u8>`, `strndup` ≙ tranche.
    pub(crate) fn copy(&mut self, layout: &mut Layout, selection: i32) {
        let (Some(from), Some(to)) = (self.from.as_ref(), self.to.as_ref()) else {
            return;
        };
        let mut strbuf: Vec<u8> = Vec::new();

        let cmp = from.compare_to(to);
        if cmp == 0 {
            if let ContentData::Text(text) = from.content().data.clone() {
                let (lo, hi) = if self.from_char < self.to_char {
                    (self.from_char, self.to_char)
                } else {
                    (self.to_char, self.from_char)
                };
                // `strndup(text + lo, hi - lo)` — bornes déjà
                // clampées par `correct_char_pos` (invariant).
                if let Some(slice) = text.get(lo as usize..hi as usize) {
                    strbuf.extend_from_slice(slice);
                }
            }
        } else {
            let (a, a_char, b, b_char) = if cmp < 0 {
                (from, self.from_char, to, self.to_char)
            } else {
                (to, self.to_char, from, self.from_char)
            };

            let mut i = a.clone();
            let mut start = true;
            loop {
                let cmp = i.compare_to(b);
                if cmp > 0 {
                    break;
                }
                let c = i.content().clone();
                match c.r#type {
                    content_type::TEXT => {
                        if let ContentData::Text(text) = &c.data {
                            if start {
                                if let Some(slice) = text.get(a_char as usize..) {
                                    strbuf.extend_from_slice(slice);
                                }
                            } else if cmp == 0 {
                                // La fin.
                                if let Some(slice) = text.get(..b_char as usize) {
                                    strbuf.extend_from_slice(slice);
                                }
                            } else {
                                strbuf.extend_from_slice(text);
                            }
                        }
                        if c.space && cmp != 0 {
                            strbuf.extend_from_slice(b" ");
                        }
                    }
                    content_type::BREAK => {
                        let break_space = match &c.data {
                            ContentData::BreakSpace(bs) => *bs,
                            _ => 0,
                        };
                        if break_space > 0 {
                            strbuf.extend_from_slice(b"\n\n");
                        } else {
                            strbuf.extend_from_slice(b"\n");
                        }
                    }
                    _ => {
                        // `DW_CONTENT_WIDGET` jamais retourné par
                        // un DeepIterator (verbatim).
                    }
                }
                i.next(layout);
                start = false;
            }
        }

        layout
            .platform
            .borrow_mut()
            .copy_selection(&strbuf, selection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imgbuf::{Imgbuf, ImgbufType};
    use crate::layout::{IdleCallback, LinkReceiver};
    use crate::platform::Platform;
    use crate::style::{Font, FontAttrs, Style, StyleAttrs, Tooltip};
    use crate::textblock::iter::textblock_word_index_iterator;
    use crate::types::Content;
    use crate::view::View;
    use crate::WidgetId;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// `Platform` minimale — enregistre `copySelection`.
    struct MockPlatform {
        /// Ids idle alloués.
        next_id: i32,
        /// Dernière sélection copiée.
        copied: Vec<u8>,
        /// Nombre d'appels.
        copies: usize,
    }

    impl Platform for MockPlatform {
        fn set_layout(&mut self, _l: usize) {}
        fn attach_view(&mut self, _v: &mut dyn View) {}
        fn detach_view(&mut self, _v: &mut dyn View) {}
        fn text_width(&mut self, _f: &Font, t: &[u8]) -> i32 {
            t.len() as i32
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
        fn add_idle(&mut self, _cb: IdleCallback) -> i32 {
            self.next_id += 1;
            self.next_id
        }
        fn remove_idle(&mut self, _id: i32) {}
        fn take_pending_idles(&mut self) -> Vec<IdleCallback> {
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
            false
        }
        fn create_color(&mut self, c: i32) -> crate::style::Color {
            crate::style::Color::new(c)
        }
        fn create_tooltip(&mut self, t: &[u8]) -> Rc<Tooltip> {
            Tooltip::create(t, None)
        }
        fn cancel_tooltip(&mut self) {}
        fn create_imgbuf(&mut self, _t: ImgbufType, _w: i32, _h: i32, _g: f64) -> Rc<dyn Imgbuf> {
            unreachable!("imgbuf non utilisé par les tests selection")
        }
        fn copy_selection(&mut self, t: &[u8], _d: i32) {
            self.copied = t.to_vec();
            self.copies += 1;
        }
        fn get_resource_factory(&mut self) -> Option<Rc<dyn std::any::Any>> {
            None
        }
    }

    /// `LinkReceiver` enregistreur.
    #[derive(Default)]
    struct Rec {
        /// press/release/click comptés.
        presses: usize,
        releases: usize,
        clicks: usize,
    }

    impl LinkReceiver for Rec {
        fn pressed(
            &mut self,
            _w: Option<WidgetId>,
            _l: i32,
            _i: i32,
            _x: i32,
            _y: i32,
            _e: &EventButton,
        ) -> bool {
            self.presses += 1;
            true
        }
        fn released(
            &mut self,
            _w: Option<WidgetId>,
            _l: i32,
            _i: i32,
            _x: i32,
            _y: i32,
            _e: &EventButton,
        ) -> bool {
            self.releases += 1;
            true
        }
        fn clicked(
            &mut self,
            _w: Option<WidgetId>,
            _l: i32,
            _i: i32,
            _x: i32,
            _y: i32,
            _e: &EventButton,
        ) -> bool {
            self.clicks += 1;
            true
        }
    }

    /// Layout + textblock "hello world" (2 mots TEXT).
    fn make() -> (
        Layout,
        Rc<RefCell<MockPlatform>>,
        Rc<RefCell<Rec>>,
        WidgetId,
    ) {
        let platform = Rc::new(RefCell::new(MockPlatform {
            next_id: 0,
            copied: Vec::new(),
            copies: 0,
        }));
        let mut l = Layout::new(platform.clone(), true);
        let w = l.tb_new(false, false);
        let style = Style::create(&StyleAttrs::default());
        l.tb_add_text(w, b"hello world", b"hello world".len(), &style);
        l.set_widget(w);
        let rec = Rc::new(RefCell::new(Rec::default()));
        l.connect_link(rec.clone());
        (l, platform, rec, w)
    }

    /// `EventButton` minimal.
    fn button(btn: i32) -> EventButton {
        EventButton {
            state: 0,
            x_canvas: 0,
            y_canvas: 0,
            x_widget: 0,
            y_widget: 0,
            num_pressed: 1,
            button: btn,
        }
    }

    /// `EventMotion` minimal.
    fn motion() -> EventMotion {
        EventMotion {
            state: 0,
            x_canvas: 0,
            y_canvas: 0,
            x_widget: 0,
            y_widget: 0,
        }
    }

    /// `createWordIndexIterator` + `handleEvent` (≙
    /// `sendSelectionEvent` avec wordIndex donné).
    fn event_at_word(
        l: &mut Layout,
        w: WidgetId,
        ty: SelectionEvent,
        word_index: i32,
        char_pos: i32,
        link: i32,
        ev: SelectionMouseEvent<'_>,
    ) -> bool {
        let it: Box<dyn DwIterator> = Box::new(textblock_word_index_iterator(
            l,
            w,
            Content::mask_for_selection(true),
            word_index,
        ));
        l.selection_state_handle_event(ty, it, char_pos, link, ev)
    }

    #[test]
    fn press_drag_release_selects_and_copies() {
        let (mut l, platform, _rec, w) = make();
        let press = button(1);
        let release = button(1);
        let mot = motion();

        assert!(event_at_word(
            &mut l,
            w,
            SelectionEvent::ButtonPress,
            0,
            0,
            -1,
            SelectionMouseEvent::Button(&press)
        ));
        assert!(event_at_word(
            &mut l,
            w,
            SelectionEvent::ButtonMotion,
            1,
            END_OF_WORD,
            -1,
            SelectionMouseEvent::Motion(&mot)
        ));
        assert!(event_at_word(
            &mut l,
            w,
            SelectionEvent::ButtonRelease,
            1,
            END_OF_WORD,
            -1,
            SelectionMouseEvent::Button(&release)
        ));

        let p = platform.borrow();
        assert_eq!(p.copies, 1);
        assert_eq!(p.copied, b"hello world");
    }

    #[test]
    fn link_press_release_clicks() {
        let (mut l, platform, rec, w) = make();
        let press = button(1);
        let release = button(1);

        // Pression sur le lien 3 (≙ `linkNo != -1` → LINK_PRESSED).
        assert!(event_at_word(
            &mut l,
            w,
            SelectionEvent::ButtonPress,
            0,
            0,
            3,
            SelectionMouseEvent::Button(&press)
        ));
        assert_eq!(rec.borrow().presses, 1);
        assert_eq!(platform.borrow().copies, 0);

        // Relâchement sur le même lien → release + click.
        assert!(event_at_word(
            &mut l,
            w,
            SelectionEvent::ButtonRelease,
            0,
            0,
            3,
            SelectionMouseEvent::Button(&release)
        ));
        let r = rec.borrow();
        assert_eq!(r.releases, 1);
        assert_eq!(r.clicks, 1);
        assert_eq!(platform.borrow().copies, 0);
    }

    #[test]
    fn link_drag_switches_to_selection() {
        let (mut l, platform, rec, w) = make();
        let press = button(1);
        let release = button(1);
        let mot = motion();

        assert!(event_at_word(
            &mut l,
            w,
            SelectionEvent::ButtonPress,
            0,
            0,
            3,
            SelectionMouseEvent::Button(&press)
        ));
        // Glisser hors du lien → `switchLinkToSelection` (verbatim :
        // link devient `from`, position courante `to`).
        assert!(event_at_word(
            &mut l,
            w,
            SelectionEvent::ButtonMotion,
            1,
            END_OF_WORD,
            -1,
            SelectionMouseEvent::Motion(&mot)
        ));
        assert!(event_at_word(
            &mut l,
            w,
            SelectionEvent::ButtonRelease,
            1,
            END_OF_WORD,
            -1,
            SelectionMouseEvent::Button(&release)
        ));

        let p = platform.borrow();
        assert_eq!(p.copies, 1);
        assert_eq!(p.copied, b"hello world");
        assert_eq!(rec.borrow().clicks, 0);
    }

    #[test]
    fn right_button_press_emits_link_press() {
        let (mut l, _platform, rec, w) = make();
        let press = button(3);
        // Bouton 3 → popup : `emitLinkPress` seul, pas de sélection.
        assert!(event_at_word(
            &mut l,
            w,
            SelectionEvent::ButtonPress,
            0,
            0,
            7,
            SelectionMouseEvent::Button(&press)
        ));
        assert_eq!(rec.borrow().presses, 1);
        assert_eq!(rec.borrow().clicks, 0);
    }
}
