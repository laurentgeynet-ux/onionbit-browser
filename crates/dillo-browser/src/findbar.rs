//! Port of `src/findbar.cc` + `findbar.hh` — barre de recherche.
//!
//! `Findbar` ≙ `Fl_Group` : `[hide 16px][input resizable][Next 70]
//! [Previous 70][Case-sensitive 140]` (border/gap 2 verbatim).
//! `MyInput::handle` devient [`input_key`] — filtrage verbatim des
//! touches (PgUp/PgDn/Up/Down passent au viewport, Shift+←/→ à l'UI,
//! Ctrl+A/E/K/D éditent, Ctrl+H/I/J/L/M et Escape passent).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::fn_params_excessive_bools)]
#![allow(clippy::doc_markdown)]
#![forbid(unsafe_code)]

use crate::dialog::LineEditor;
use crate::keys::keysym;
use crate::ui::Rect;

/// `button_width` verbatim.
pub const BUTTON_W: i32 = 70;
/// `gap` verbatim.
pub const GAP: i32 = 2;
/// `border` verbatim.
pub const BORDER: i32 = 2;
/// `hide_btn` width verbatim.
pub const HIDE_W: i32 = 16;
/// `check_btn` width = `2*button_width` verbatim.
pub const CHECK_W: i32 = 2 * BUTTON_W;

/// `Findbar` ≙ `Fl_Group` (`hide_btn/CustButton`, i/MyInput,
/// `next_btn`, `prev_btn`, `check_btn/Fl_Check_Button`).
///
/// `input` ≙ `MyInput` (`value()`+`insert_position`).
#[derive(Debug)]
pub struct Findbar {
    /// `i` — texte cherché + curseur.
    pub input: LineEditor,
    /// `check_btn->value()`.
    pub case_sens: bool,
    /// Focus sur l'input (`take_focus` à `show()`).
    pub input_focused: bool,
}

/// Rect des sous-widgets du findbar (pos relatifs ≙ constructeur
/// `Findbar(width,height)` amont — `x` accumulé gap à gap).
#[derive(Clone, Copy, Debug)]
pub struct FindbarLayout {
    /// `hide_btn` (16px, icon `new_s_xpm`).
    pub hide_btn: Rect,
    /// `i` (`MyInput`, resizable).
    pub input: Rect,
    /// `next_btn` (70px, "Next", shortcut Enter).
    pub next_btn: Rect,
    /// `prev_btn` (70px, "Previous", shortcut Shift+Enter).
    pub prev_btn: Rect,
    /// `check_btn` (140px, "Case-sensitive").
    pub check_btn: Rect,
}

impl Findbar {
    /// `Findbar::Findbar` (verbatim) — `input_width = width -
    /// (2*border + 4*(button_width+gap))`.
    #[must_use]
    pub fn new() -> Self {
        Findbar {
            input: LineEditor::new(),
            case_sens: false,
            input_focused: false,
        }
    }

    /// Layout verbatim — `height` = hauteur du groupe ; chaque widget
    /// est à `y=border`, `h=height-2*border`.
    #[must_use]
    pub fn layout(width: i32, height: i32) -> FindbarLayout {
        let h = height - 2 * BORDER;
        let input_width = width - (2 * BORDER + 4 * (BUTTON_W + GAP));
        let mut x = 0;
        let hide_btn = Rect {
            x,
            y: BORDER,
            w: HIDE_W,
            h,
        };
        x += HIDE_W + GAP;
        let input = Rect {
            x,
            y: BORDER,
            w: input_width,
            h,
        };
        x += input_width + GAP;
        let next_btn = Rect {
            x,
            y: BORDER,
            w: BUTTON_W,
            h,
        };
        x += BUTTON_W + GAP;
        let prev_btn = Rect {
            x,
            y: BORDER,
            w: BUTTON_W,
            h,
        };
        x += BUTTON_W + GAP;
        let check_btn = Rect {
            x,
            y: BORDER,
            w: CHECK_W,
            h,
        };
        FindbarLayout {
            hide_btn,
            input,
            next_btn,
            prev_btn,
            check_btn,
        }
    }

    /// `Findbar::show` (verbatim) — `take_focus` + `position(size,0)`
    /// ≙ select-all + curseur en fin.
    pub fn show(&mut self) {
        self.input_focused = true;
        // verbatim : `insert_position(size, 0)` — marque à 0 = tout
        // sélectionné ; on modélise pos=end (la sélection est rendue
        // par le dessin du LineInput amont — ici le caret va en fin).
        self.input.set_pos(self.input.size());
    }

    /// `hide_cb` ≙ `a_UIcmd_findbar_toggle(bw, 0)` — remontée via
    /// [`FindCmd::Hide`].
    pub fn clear_focus(&mut self) {
        self.input_focused = false;
    }

    /// `i->value()` non vide ?
    #[must_use]
    pub fn has_text(&self) -> bool {
        !self.input.buf.is_empty()
    }
}

impl Default for Findbar {
    fn default() -> Self {
        Self::new()
    }
}

/// Résultat du filtre clavier `MyInput::handle` (verbatim).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKey {
    /// `return 0` — l'événement monte à l'UI/viewport (scroll,
    /// shortcuts, Escape → hide findbar).
    Pass,
    /// `return 1` — consommé par l'input (Ctrl+A/E/K/D ou input normal).
    Consumed,
    /// `Fl_Input::handle` — le texte/curseur édite le buffer.
    Edit,
}

/// Commandes de la barre ≙ callbacks amont.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindCmd {
    /// `search_cb` — `a_UIcmd_findtext_search(.., backwards=false)`.
    Next,
    /// `searchBackwards_cb` — `backwards=true`.
    Prev,
    /// `hide_cb` — `a_UIcmd_findbar_toggle(bw, 0)`.
    Hide,
    /// `check_btn` toggle.
    ToggleCase,
}

/// `MyInput::handle` (verbatim) — filtre appliqué **avant** l'édition
/// normale. `shift/ctrl/alt/meta` ≙ `Fl::event_state() & (…|FL_META)`.
///
/// Retourne [`InputKey::Pass`] (≙ `return 0`), [`InputKey::Edit`]
/// (≙ `Fl_Input::handle` — l'appelant délègue à `LineInput::handle_key`)
/// ou [`InputKey::Consumed`] (Ctrl+`a|e|k|d` consommés ici).
#[must_use]
pub fn input_key(
    editor: &mut LineEditor,
    key: i32,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
) -> InputKey {
    // verbatim : modifier = state & (SHIFT|CTRL|ALT|META)
    let shift_only = shift && !ctrl && !alt && !meta;
    let ctrl_only = ctrl && !shift && !alt && !meta;
    let none = !(shift || ctrl || alt || meta);

    if key == keysym::PAGE_DOWN
        || key == keysym::PAGE_UP
        || key == keysym::UP
        || key == keysym::DOWN
    {
        return InputKey::Pass; // key commands & viewport motion
    }
    if shift_only && (key == keysym::LEFT || key == keysym::RIGHT) {
        return InputKey::Pass; // let these keys get to the UI
    }
    if ctrl_only {
        if editor.ctrl_edit(key, true) {
            return InputKey::Consumed;
        }
        // ^H ^I ^J ^L ^M — verbatim : passent (key commands)
        for kc in *b"hijlm" {
            if key == i32::from(kc) {
                return InputKey::Pass;
            }
        }
    }
    if none && key == keysym::ESCAPE {
        return InputKey::Pass; // hide, don't clear (UI handles)
    }
    InputKey::Edit
}

/// `search_cb`/`searchBackwards_cb` (verbatim) — `key[0] != '\0'`
/// requis ; retourne la commande à dispatcher vers
/// `UiCmd::findtext_search`.
#[must_use]
pub fn search_cmd(fb: &Findbar, backwards: bool) -> Option<(String, bool, bool)> {
    if fb.input.buf.is_empty() {
        return None;
    }
    Some((fb.input.buf.clone(), fb.case_sens, backwards))
}

/// Sous-widget touché (≙ dispatch FLTK interne du `Fl_Group`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindHit {
    /// `hide_btn`.
    Hide,
    /// `i` (`MyInput`).
    Input,
    /// `next_btn`.
    Next,
    /// `prev_btn`.
    Prev,
    /// `check_btn`.
    Case,
}

/// Hit-test verbatim — quel sous-widget (≙ dispatch FLTK interne du
/// groupe). `pos` en coords relatives au rect findbar.
#[must_use]
pub fn hit(layout: &FindbarLayout, pos: (i32, i32)) -> Option<FindHit> {
    let (x, y) = pos;
    if layout.hide_btn.contains(x, y) {
        return Some(FindHit::Hide);
    }
    if layout.input.contains(x, y) {
        return Some(FindHit::Input);
    }
    if layout.next_btn.contains(x, y) {
        return Some(FindHit::Next);
    }
    if layout.prev_btn.contains(x, y) {
        return Some(FindHit::Prev);
    }
    if layout.check_btn.contains(x, y) {
        return Some(FindHit::Case);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_verbatim() {
        // Findbar(600, 28) : hide[0,2,16], input, next, prev, check
        let l = Findbar::layout(600, 28);
        assert_eq!(
            l.hide_btn,
            Rect {
                x: 0,
                y: 2,
                w: 16,
                h: 24
            }
        );
        let iw = 600 - (2 * 2 + 4 * (70 + 2)); // = 600-292 = 308
        assert_eq!(
            l.input,
            Rect {
                x: 18,
                y: 2,
                w: iw,
                h: 24
            }
        );
        assert_eq!(l.next_btn.x, 18 + iw + 2);
        assert_eq!(l.prev_btn.x, l.next_btn.x + 72);
        assert_eq!(l.check_btn.w, 140);
        // hit-test
        assert_eq!(hit(&l, (1, 3)), Some(FindHit::Hide));
        assert_eq!(hit(&l, (20, 3)), Some(FindHit::Input));
        assert_eq!(hit(&l, (l.next_btn.x + 1, 3)), Some(FindHit::Next));
        assert_eq!(hit(&l, (l.check_btn.x + 1, 3)), Some(FindHit::Case));
        assert_eq!(hit(&l, (-1, -1)), None);
    }

    #[test]
    fn input_filter_verbatim() {
        let mut e = LineEditor::new();
        e.buf = "abc".into();
        // PgUp/PgDn/Up/Down → pass (viewport/key commands)
        for k in [keysym::PAGE_UP, keysym::PAGE_DOWN, keysym::UP, keysym::DOWN] {
            assert_eq!(
                input_key(&mut e, k, false, false, false, false),
                InputKey::Pass
            );
        }
        // Shift seul + Left/Right → pass
        assert_eq!(
            input_key(&mut e, keysym::LEFT, true, false, false, false),
            InputKey::Pass
        );
        // Ctrl+A/E/K/D consommé ici
        assert_eq!(
            input_key(&mut e, i32::from(b'a'), false, true, false, false),
            InputKey::Consumed
        );
        assert_eq!(e.pos, 0);
        // Ctrl+H/I/J/L/M → pass (évite insertion de contrôles)
        for c in *b"hijlm" {
            assert_eq!(
                input_key(&mut e, i32::from(c), false, true, false, false),
                InputKey::Pass
            );
        }
        // Escape sans modifier → pass (UI cache la barre)
        assert_eq!(
            input_key(&mut e, keysym::ESCAPE, false, false, false, false),
            InputKey::Pass
        );
        // touche normale → édition
        assert_eq!(
            input_key(&mut e, i32::from(b'x'), false, false, false, false),
            InputKey::Edit
        );
    }

    #[test]
    fn search_cmd_requires_text() {
        let mut fb = Findbar::new();
        assert!(search_cmd(&fb, false).is_none()); // key[0]=='\0' verbatim
        fb.input.buf = "needle".into();
        fb.case_sens = true;
        assert_eq!(search_cmd(&fb, false), Some(("needle".into(), true, false)));
        assert_eq!(search_cmd(&fb, true), Some(("needle".into(), true, true)));
    }

    #[test]
    fn show_selects_all() {
        let mut fb = Findbar::new();
        fb.input.buf = "abc".into();
        fb.show();
        assert!(fb.input_focused);
        assert_eq!(fb.input.pos, 3); // position(size,0) → fin
    }
}
