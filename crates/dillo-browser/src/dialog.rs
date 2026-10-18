//! Port of `src/dialog.cc` + `dialog.hh` — boîtes de dialogue modales.
//!
//! Amont construit des fenêtres FLTK modales (`window->set_modal()` +
//! `while (shown()) Fl::wait()`). Le port sépare le *modèle* verbatim
//! ([`DialogSpec`] : titre, message, géométrie, widgets, alternatives)
//! du *runner* ([`DialogRunner`], seam injectée par l'app — fenêtre
//! native ou boucle modale in-window). Les statiques amont
//! (`input_answer`, `input_str`, `choice_answer`) sont dans
//! [`DialogState`].
//!
//! `a_Dialog_input` amont contient un `Fl_Choice` de moteurs de
//! recherche (`prefs.search_urls`, write-back de
//! `prefs.search_url_idx` à OK — quirk `bool_t` préservé : l'index
//! choisi est tronqué à `!= 0`).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::field_reassign_with_default)]
#![forbid(unsafe_code)]

use dillo_common::misc;

use crate::prefs::Prefs;

/// Géométrie verbatim `a_Dialog_input` (`ww,wh,gap,ih,bw,bh`).
pub const INPUT_DIALOG: DialogGeo = DialogGeo {
    ww: 450,
    wh: 130,
    gap: 10,
    icon_h: 60,
    btn_w: 80,
    btn_h: 30,
};

/// `a_Dialog_user_password` (fenêtre 300×variable).
pub const USERPASS_DIALOG: DialogGeo = DialogGeo {
    ww: 300,
    wh: 280,
    gap: 20,
    icon_h: 0,
    btn_w: 50,
    btn_h: 30,
};

/// Géométrie de dialogue (constantes amont groupées).
#[derive(Clone, Copy, Debug)]
pub struct DialogGeo {
    /// `ww`.
    pub ww: i32,
    /// `wh` initial (user/pass est re-sizé).
    pub wh: i32,
    /// `gap`.
    pub gap: i32,
    /// `ih` (hauteur icône "?").
    pub icon_h: i32,
    /// `bw`.
    pub btn_w: i32,
    /// `bh`.
    pub btn_h: i32,
}

/// Kind de dialogue ≙ fonction `a_Dialog_*` amont.
#[derive(Clone, Debug)]
pub enum DialogKind {
    /// `a_Dialog_msg` — `fl_message`.
    Msg,
    /// `a_Dialog_choice` — boutons `EnterButton` 1..=n.
    Choice,
    /// `a_Dialog_input` — `CustInput3` + `CustChoice2` moteurs.
    Input,
    /// `a_Dialog_passwd` — `fl_password`.
    Passwd,
    /// `a_Dialog_user_password` — user + secret inputs.
    UserPassword,
    /// `a_Dialog_save_file` — `fl_file_chooser` (save).
    SaveFile,
    /// `a_Dialog_select_file` — idem save (verbatim : même path).
    SelectFile,
    /// `a_Dialog_open_file` — chooser + `a_Misc_escape_chars("% #")`.
    OpenFile,
    /// `a_Dialog_text_window` — text display + bouton Close.
    TextWindow,
}

/// Requête de dialogue ≙ signature `a_Dialog_*`.
#[derive(Clone, Debug)]
pub struct DialogSpec {
    /// Kind.
    pub kind: DialogKind,
    /// `title` (après default verbatim : `""`/`None` →
    /// `"OnionBit-Browser: <Kind>"` — cf. [`default_title`]).
    pub title: String,
    /// `msg`.
    pub msg: String,
    /// `a_Dialog_choice` alternatives (1-based answer).
    pub alternatives: Vec<String>,
    /// `fl_file_chooser` pattern.
    pub pattern: Option<String>,
    /// `fname` / `init` suggested.
    pub suggested: Option<String>,
    /// `a_Dialog_text_window` body.
    pub text: String,
    /// `a_Dialog_input` : labels des moteurs de recherche
    /// (`prefs.search_urls` parsés — `a_Misc_parse_search_url`).
    pub search_labels: Vec<String>,
    /// `prefs.search_url_idx` courant (présélection du choix).
    pub search_url_idx: i32,
    /// Taille fenêtre du `text_window` (`prefs.width`/`height` amont)
    /// et facteur fonte (`prefs.font_factor`).
    pub window_wh: (i32, i32),
    /// `prefs.font_factor` (textsize = `rint(14*factor)`).
    pub font_factor: f64,
}

/// Réponse ≙ valeur retournée par `a_Dialog_*`.
#[derive(Clone, Debug, PartialEq)]
pub enum DialogAnswer {
    /// Fenêtre fermée / Cancel (`NULL` / `choice_answer = 0` amont).
    Closed,
    /// `a_Dialog_input` OK → `(input_str, search_url_idx choisi)`.
    Input(String, i32),
    /// `a_Dialog_choice` — alternative 1-based.
    Choice(i32),
    /// `a_Dialog_user_password` — `ok`, `user`, `password`.
    UserPassword(bool, String, String),
    /// `fl_file_chooser`/`fl_password` — `None` ≙ `NULL`.
    Path(Option<String>),
    /// `a_Dialog_msg`/`a_Dialog_text_window` — pas de retour.
    Ack,
}

/// Statics amont (`input_answer`, `input_str`, `choice_answer`).
#[derive(Default)]
pub struct DialogState {
    /// `input_answer` (1=OK, 2=Cancel).
    pub input_answer: i32,
    /// `input_str`.
    pub input_str: Option<String>,
    /// `choice_answer` (1-based).
    pub choice_answer: i32,
    /// Menu items des moteurs (cache ≙ `static Fl_Menu_Item *pm`).
    pub search_menu: Option<Vec<String>>,
}

/// Seam du runner — présente le dialogue et rend la réponse
/// (bloquant ≙ `while (shown()) Fl::wait()` amont ; l'app peut faire
/// une boucle modale native ou in-window). Les items de menu du
/// choix moteur sont construits une fois (`static pm` amont) via
/// [`search_labels`].
pub trait DialogRunner {
    /// Présente `spec` modalement et retourne la réponse.
    fn run(&mut self, spec: &DialogSpec) -> DialogAnswer;
}

/// Runner par défaut — fenêtre fermée immédiatement (verbatim :
/// `input_answer == 0` → `NULL`, `choice_answer = 0` → `0`,
/// `fl_*` annulé → `NULL`). Utilisé tant que l'app n'a pas injecté
/// son runner (chrome-4).
#[derive(Default)]
pub struct AutoCancelRunner;

impl DialogRunner for AutoCancelRunner {
    fn run(&mut self, spec: &DialogSpec) -> DialogAnswer {
        match spec.kind {
            DialogKind::Msg | DialogKind::TextWindow => DialogAnswer::Ack,
            DialogKind::Choice => DialogAnswer::Choice(0),
            DialogKind::SaveFile
            | DialogKind::SelectFile
            | DialogKind::OpenFile
            | DialogKind::Passwd => DialogAnswer::Path(None),
            DialogKind::UserPassword => {
                DialogAnswer::UserPassword(false, String::new(), String::new())
            }
            DialogKind::Input => DialogAnswer::Closed,
        }
    }
}

/// `title && *title` default verbatim par kind.
#[must_use]
pub fn default_title(kind: &DialogKind, title: Option<&str>) -> String {
    if let Some(t) = title {
        if !t.is_empty() {
            return t.to_string();
        }
    }
    match kind {
        DialogKind::Msg => "OnionBit-Browser: Message",
        DialogKind::Input => "OnionBit-Browser: Input",
        DialogKind::Passwd => "OnionBit-Browser: Password",
        DialogKind::Choice => "OnionBit-Browser: Choice",
        DialogKind::UserPassword => "OnionBit-Browser: User/Password",
        DialogKind::TextWindow => "OnionBit-Browser: Text",
        DialogKind::SaveFile | DialogKind::SelectFile | DialogKind::OpenFile => {
            "OnionBit-Browser: File"
        }
    }
    .to_string()
}

/// `pm[j++] = label` — construction du menu moteurs
/// (`a_Dialog_input`, verbatim : labels parsés une fois, `static`).
pub fn search_labels(state: &mut DialogState, prefs: &Prefs) -> Vec<String> {
    if let Some(l) = &state.search_menu {
        return l.clone();
    }
    let mut v = Vec::new();
    for src in &prefs.search_urls {
        if let Some((label, _url)) = misc::parse_search_url(src) {
            v.push(String::from_utf8_lossy(&label).into_owned());
        }
    }
    state.search_menu = Some(v.clone());
    v
}

/// Spec de `a_Dialog_input` (verbatim : icon "?", msg box,
/// `CustInput3`, `CustChoice2`, OK/Cancel).
#[must_use]
pub fn input_spec(state: &mut DialogState, prefs: &Prefs, title: &str, msg: &str) -> DialogSpec {
    DialogSpec {
        kind: DialogKind::Input,
        title: default_title(&DialogKind::Input, Some(title)),
        msg: msg.to_string(),
        alternatives: Vec::new(),
        pattern: None,
        suggested: None,
        text: String::new(),
        search_labels: search_labels(state, prefs),
        search_url_idx: i32::from(prefs.search_url_idx.get()),
        window_wh: (prefs.width, prefs.height),
        font_factor: prefs.font_factor,
    }
}

/// Spec de `a_Dialog_choice` — `n == 0` → `None` ≙
/// `MSG_ERR + return 0` amont.
#[must_use]
pub fn choice_spec(title: &str, msg: &str, alts: &[&str]) -> Option<DialogSpec> {
    if alts.is_empty() {
        tracing::error!("Dialog_choice: no alternatives.");
        return None;
    }
    Some(DialogSpec {
        kind: DialogKind::Choice,
        title: default_title(&DialogKind::Choice, Some(title)),
        msg: msg.to_string(),
        alternatives: alts.iter().map(|s| (*s).to_string()).collect(),
        pattern: None,
        suggested: None,
        text: String::new(),
        search_labels: Vec::new(),
        search_url_idx: 0,
        window_wh: (0, 0),
        font_factor: 0.0,
    })
}

/// Spec de `a_Dialog_user_password`.
#[must_use]
pub fn user_password_spec(title: &str, msg: &str) -> DialogSpec {
    DialogSpec {
        kind: DialogKind::UserPassword,
        title: default_title(&DialogKind::UserPassword, Some(title)),
        msg: msg.to_string(),
        alternatives: Vec::new(),
        pattern: None,
        suggested: None,
        text: String::new(),
        search_labels: Vec::new(),
        search_url_idx: 0,
        window_wh: (USERPASS_DIALOG.ww, USERPASS_DIALOG.wh),
        font_factor: 0.0,
    }
}

/// Spec `fl_file_chooser` (save/select/open selon `kind`).
#[must_use]
pub fn file_spec(
    kind: DialogKind,
    title: &str,
    pattern: Option<&str>,
    fname: Option<&str>,
) -> DialogSpec {
    DialogSpec {
        title: default_title(&kind, Some(title)),
        kind,
        msg: String::new(),
        alternatives: Vec::new(),
        pattern: pattern.map(str::to_string),
        suggested: fname.map(str::to_string),
        text: String::new(),
        search_labels: Vec::new(),
        search_url_idx: 0,
        window_wh: (0, 0),
        font_factor: 0.0,
    }
}

/// Spec de `a_Dialog_text_window` — `wh/ww` = prefs, bouton Close
/// 30px en bas (verbatim).
#[must_use]
pub fn text_window_spec(title: &str, txt: &str, prefs: &Prefs) -> DialogSpec {
    DialogSpec {
        kind: DialogKind::TextWindow,
        title: default_title(&DialogKind::TextWindow, Some(title)),
        msg: String::new(),
        alternatives: Vec::new(),
        pattern: None,
        suggested: None,
        text: txt.to_string(),
        search_labels: Vec::new(),
        search_url_idx: 0,
        window_wh: (prefs.width, prefs.height),
        font_factor: prefs.font_factor,
    }
}

// -- key handling verbatim ------------------------------------------------------

/// État d'une ligne d'input (`CustInput3`/`MyInput` partagent les
/// raccourcis Ctrl+A/E/K/D) — `buf` UTF-8 + `pos` insertion.
#[derive(Debug)]
pub struct LineEditor {
    /// `value()`.
    pub buf: String,
    /// `insert_position()`/`d_position()` — en **bytes** (FLTK indexe
    /// en octets UTF-8).
    pub pos: usize,
}

impl LineEditor {
    /// `Fl_Input` vide.
    #[must_use]
    pub fn new() -> Self {
        LineEditor {
            buf: String::new(),
            pos: 0,
        }
    }

    /// `size()` — longueur en octets (verbatim).
    #[must_use]
    pub fn size(&self) -> usize {
        self.buf.len()
    }

    /// `cut(a, b)` verbatim — supprime `[a,b)` en bytes (clampé aux
    /// bornes UTF-8 — amont manipule des octets ; on garde la sémantique
    /// en respectant les frontières de char pour ne pas invalider).
    pub fn cut(&mut self, a: usize, b: usize) {
        let a = a.min(self.buf.len());
        let b = b.min(self.buf.len()).max(a);
        let (a, b) = (self.floor_char(a), self.floor_char(b));
        self.buf.replace_range(a..b, "");
        self.pos = self.pos.min(self.buf.len());
        if self.pos > a {
            self.pos = a;
        }
    }

    /// `insert_position(p)` — clamp aux frontières UTF-8.
    pub fn set_pos(&mut self, p: usize) {
        self.pos = self.floor_char(p.min(self.buf.len()));
    }

    /// Frontière UTF-8 ≤ `p` (≙ comportement FLTK qui clamp).
    fn floor_char(&self, p: usize) -> usize {
        let mut p = p.min(self.buf.len());
        while p > 0 && !self.buf.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    /// `CustInput3::handle`/`MyInput` part : `Ctrl+A` pos→0,
    /// `Ctrl+E` pos→size, `Ctrl+K` cut(pos,size), `Ctrl+D`
    /// cut(pos,pos+1). Retourne `true` ≙ `return 1`.
    ///
    /// `modifier` ≙ `Fl::event_state() & (FL_SHIFT|FL_CTRL|FL_ALT)` —
    /// l'appelant passe déjà filtré ; ici on teste `== FL_CTRL`
    /// verbatim (shift+ctrl passera à l'input normal).
    pub fn ctrl_edit(&mut self, key: i32, ctrl_only: bool) -> bool {
        if !ctrl_only {
            return false;
        }
        match key {
            // 'a' → home, 'e' → end (verbatim : `d_position(k=='a'?0:size())`)
            k if k == i32::from(b'a') || k == i32::from(b'e') => {
                self.pos = if k == i32::from(b'a') { 0 } else { self.size() };
                true
            }
            k if k == i32::from(b'k') => {
                self.cut(self.pos, self.size());
                true
            }
            k if k == i32::from(b'd') => {
                self.cut(self.pos, self.pos + 1);
                true
            }
            _ => false,
        }
    }
}

impl Default for LineEditor {
    fn default() -> Self {
        Self::new()
    }
}

/// `CustChoice2::handle` verbatim — `Enter`/`Down` sans modifiers →
/// activation du choix (`return handle(FL_PUSH)`).
#[must_use]
pub fn choice_key_activates(key: i32, mods_none: bool) -> bool {
    use crate::keys::keysym;
    mods_none && (key == keysym::ENTER || key == keysym::DOWN)
}

/// `EnterButton::handle` verbatim — `Enter` quand le bouton a le
/// focus → `do_callback` (`simulate_key_action` + `set_changed`).
#[must_use]
pub fn enter_button_activates(key: i32, focused: bool) -> bool {
    use crate::keys::keysym;
    focused && key == keysym::ENTER
}

/// `a_Dialog_open_file` verbatim — `fl_file_chooser` + escape
/// `% #` sur le résultat (`NULL` → `None`).
#[must_use]
pub fn open_file_escape(path: Option<&str>) -> Option<String> {
    path.map(|p| String::from_utf8_lossy(&misc::escape_chars(p.as_bytes(), b"% #")).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_edit_verbatim() {
        // `CustInput3::handle` : a→0, e→size, k→cut(pos,size), d→cut+1
        let mut e = LineEditor {
            buf: "abcdef".into(),
            pos: 2,
        };
        assert!(e.ctrl_edit(i32::from(b'a'), true));
        assert_eq!(e.pos, 0);
        assert!(e.ctrl_edit(i32::from(b'e'), true));
        assert_eq!(e.pos, 6);
        e.pos = 2;
        assert!(e.ctrl_edit(i32::from(b'k'), true));
        assert_eq!(e.buf, "ab");
        e.buf = "abcd".into();
        e.pos = 1;
        assert!(e.ctrl_edit(i32::from(b'd'), true));
        assert_eq!(e.buf, "acd");
        // pas ctrl seul → pas d'édition (verbatim : modifier==FL_CTRL)
        let mut e2 = LineEditor {
            buf: "x".into(),
            pos: 0,
        };
        assert!(!e2.ctrl_edit(i32::from(b'a'), false));
    }

    #[test]
    fn cut_clamps_utf8() {
        let mut e = LineEditor {
            buf: "éèa".into(),
            pos: 0,
        };
        // cut au milieu d'un char → clampé frontière (pas de panic)
        e.cut(1, 3);
        assert_eq!(e.buf, "èa"); // [0,2) supprimé = "é"
    }

    #[test]
    fn choice_spec_none_on_empty() {
        assert!(choice_spec("t", "m", &[]).is_none());
        let s = choice_spec("t", "m", &["a", "b", "c"]).unwrap();
        assert_eq!(s.alternatives.len(), 3);
        assert_eq!(s.title, "t");
    }

    #[test]
    fn default_titles_verbatim() {
        assert_eq!(
            default_title(&DialogKind::Choice, Some("")),
            "OnionBit-Browser: Choice"
        );
        assert_eq!(
            default_title(&DialogKind::Input, None),
            "OnionBit-Browser: Input"
        );
        assert_eq!(default_title(&DialogKind::Input, Some("T")), "T");
    }

    #[test]
    fn open_file_escapes() {
        // verbatim : a_Misc_escape_chars(fc_name, "% #")
        let p = open_file_escape(Some("/tmp/a b%c.png"));
        assert_eq!(p.as_deref(), Some("/tmp/a%20b%25c.png"));
        assert_eq!(open_file_escape(None), None);
    }

    #[test]
    fn auto_cancel_matches_closed_window() {
        let mut r = AutoCancelRunner;
        // verbatim : window hidden sans click → 0/None/Closed
        let s = choice_spec("t", "m", &["a", "b"]).unwrap();
        assert_eq!(r.run(&s), DialogAnswer::Choice(0));
    }

    #[test]
    fn search_labels_from_prefs() {
        let mut prefs = Prefs::default();
        prefs.search_urls = vec![
            b"Wikipedia https://en.wikipedia.org/wiki/Special:Search?search=%s".to_vec(),
            b"ddg https://duckduckgo.com/?q=%s".to_vec(),
        ];
        let mut st = DialogState::default();
        let labels = search_labels(&mut st, &prefs);
        assert_eq!(labels, vec!["Wikipedia", "ddg"]);
        // cache ≙ `static pm` — seconde passe identique
        assert_eq!(search_labels(&mut st, &prefs), labels);
    }
}
