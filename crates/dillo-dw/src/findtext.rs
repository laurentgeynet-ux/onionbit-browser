//! `dw/findtext.cc` — `dw::core::FindtextState` : recherche
//! incrémentale KMP sur un [`CharIterator`] (verbatim).
//!
//! Adaptations : `char*` → `Vec<u8>` (`rev`/nexttab par octets,
//! comparaisons ASCII comme amont), `Widget*` → `WidgetId`, l'état
//! vit dans `Layout` et reçoit `&mut Layout` par paramètre
//! (≙ champ `layout` amont).

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use dillo_common::dlib;

use crate::iterator::{CharIterator, CHAR_END};
use crate::layout::{FindtextResult, Layout};
use crate::types::{HPosition, HighlightLayer, VPosition};
use crate::WidgetId;

/// `dw::core::FindtextState` (findtext.hh) — verbatim.
#[derive(Debug)]
pub(crate) struct FindtextState {
    /// `key` — clé de la dernière recherche (la recherche se
    /// poursuit si inchangée — verbatim).
    key: Option<Vec<u8>>,
    /// `caseSens`.
    case_sens: bool,
    /// `nexttab` — table KMP.
    nexttab: Option<Vec<i32>>,
    /// `widget` — sommet de l'arbre où chercher.
    widget: Option<WidgetId>,
    /// `iterator` — position de la prochaine recherche.
    iterator: Option<CharIterator>,
    /// `hlIterator` — position à partir de laquelle les
    /// caractères sont surlignés (`None` ≙ NULL amont).
    hl_iterator: Option<CharIterator>,
}

impl FindtextState {
    /// `FindtextState ()`.
    pub(crate) const fn new() -> Self {
        FindtextState {
            key: None,
            case_sens: false,
            nexttab: None,
            widget: None,
            iterator: None,
            hl_iterator: None,
        }
    }

    /// `setWidget` — un changement de widget relance la recherche
    /// (verbatim).
    pub(crate) fn set_widget(&mut self, widget: Option<WidgetId>) {
        self.widget = widget;
        self.key = None;
        self.nexttab = None;
        self.iterator = None;
        self.hl_iterator = None;
    }

    /// `search` (findtext.cc:67-133) — verbatim.
    pub(crate) fn search(
        &mut self,
        layout: &mut Layout,
        key: &[u8],
        case_sens: bool,
        backwards: bool,
    ) -> FindtextResult {
        // `*key == 0` amont ≙ clé vide.
        let Some(widget) = self.widget else {
            return FindtextResult::NotFound;
        };
        if key.is_empty() {
            return FindtextResult::NotFound;
        }

        let was_highlighted = self.unhighlight(layout);

        // Changement de clé/widget/sensibilité → on repart du
        // début (verbatim).
        let new_key = self.key.as_deref() != Some(key) || self.case_sens != case_sens;
        if new_key {
            self.key = Some(key.to_vec());
            self.case_sens = case_sens;
            self.nexttab = Some(Self::create_nexttab(key, case_sens, backwards));

            let mut it = CharIterator::new(layout, widget, true);
            if backwards {
                // Aller à la fin ; `prev` pour ne pas rester à END
                // (verbatim).
                while it.next(layout) {}
                it.prev(layout);
            } else {
                it.next(layout);
            }
            self.iterator = Some(it);
        }

        let first_trial = !was_highlighted || new_key;

        if self.search0(layout, backwards, first_trial) {
            // Surlignage via un clone (verbatim).
            if let Some(iterator) = self.iterator.as_mut() {
                let mut hl = iterator.clone();
                for _ in 0..key.len() {
                    hl.next(layout);
                }
                CharIterator::highlight(layout, iterator, &mut hl, HighlightLayer::Findtext);
                CharIterator::scroll_to(
                    layout,
                    iterator,
                    &mut hl,
                    HPosition::IntoView,
                    VPosition::Center,
                );

                // La recherche repart après la position trouvée.
                iterator.next(layout);
                self.hl_iterator = Some(hl);
            }
            FindtextResult::Success
        } else if first_trial {
            FindtextResult::NotFound
        } else {
            // Plus rien : réinitialise pour le prochain essai
            // (verbatim).
            let mut it = CharIterator::new(layout, widget, true);
            if backwards {
                while it.next(layout) {}
                it.prev(layout);
            } else {
                it.next(layout);
            }
            self.iterator = Some(it);
            // Succès attendu — `assert` amont.
            let result2 = self.search(layout, key, case_sens, backwards);
            debug_assert_eq!(result2, FindtextResult::Success);
            FindtextResult::Restart
        }
    }

    /// `resetSearch` — appelé à la fermeture du dialogue
    /// « find text » (verbatim).
    pub(crate) fn reset_search(&mut self, layout: &mut Layout) {
        self.unhighlight(layout);
        self.key = None;
    }

    /// `rev` — chaîne inversée par octets (verbatim).
    fn rev(s: &[u8]) -> Vec<u8> {
        s.iter().rev().copied().collect()
    }

    /// `createNexttab` (findtext.cc:174-197) — table KMP ;
    /// sur la clé inversée en recherche arrière (verbatim).
    fn create_nexttab(needle: &[u8], case_sens: bool, backwards: bool) -> Vec<i32> {
        let key = if backwards {
            Self::rev(needle)
        } else {
            needle.to_vec()
        };
        let l = key.len() as i32;
        // `+1` nécessaire pour `l == 1` (verbatim).
        let mut nexttab = vec![0i32; key.len() + 1];
        let mut i = 0i32;
        let mut j = -1i32;
        nexttab[0] = -1;

        // `do … while (i < l - 1)` amont.
        loop {
            if j == -1
                || Self::chars_equal(
                    i32::from(key[i as usize]),
                    i32::from(key[j as usize]),
                    case_sens,
                )
            {
                i += 1;
                j += 1;
                nexttab[i as usize] = j;
            } else {
                j = nexttab[j as usize];
            }
            if i >= l - 1 {
                break;
            }
        }

        nexttab
    }

    /// `unhighlight` — dés-surligne et indique si une région
    /// était surlignée (verbatim ; `take` ≙ `delete hlIterator;
    /// hlIterator = NULL`).
    fn unhighlight(&mut self, layout: &mut Layout) -> bool {
        if let Some(mut hl) = self.hl_iterator.take() {
            let mut start = hl.clone();
            if let Some(key) = &self.key {
                for _ in 0..key.len() {
                    start.prev(layout);
                }
            }
            CharIterator::unhighlight(layout, &mut start, &mut hl, HighlightLayer::Findtext);
            true
        } else {
            false
        }
    }

    /// `search0` (findtext.cc:221-290) — KMP verbatim, incluant
    /// le repositionnement arrière (`l-3`) et le cas `l==1`.
    fn search0(&mut self, layout: &mut Layout, backwards: bool, first_trial: bool) -> bool {
        let Some(iterator) = self.iterator.as_mut() else {
            return false;
        };
        if iterator.get_char() == CHAR_END {
            return false;
        }

        let mut ret = false;
        let search_key = if backwards {
            Self::rev(self.key.as_deref().unwrap_or(&[]))
        } else {
            self.key.clone().unwrap_or_default()
        };
        let mut j = 0i32;
        let mut nextit = true;
        let l = search_key.len() as i32;

        if backwards && !first_trial {
            // Positionne sur l'avant-avant-dernier caractère du
            // mot précédemment trouvé : `n = l - 3` positions
            // (commentaire et formule verbatim amont).
            if l == 1 {
                iterator.prev(layout);
                iterator.prev(layout);
            } else if l == 2 {
                iterator.prev(layout);
            } else if l >= 4 {
                for _ in 0..l - 3 {
                    iterator.next(layout);
                }
            }
        } else if backwards && l == 1 {
            // Cas particulier : dernier caractère introuvable
            // sinon (verbatim).
            iterator.next(layout);
        }

        let nexttab = self.nexttab.clone().unwrap_or_default();
        loop {
            if j == -1
                || Self::chars_equal(
                    iterator.get_char(),
                    i32::from(search_key[j as usize]),
                    self.case_sens,
                )
            {
                j += 1;
                nextit = if backwards {
                    iterator.prev(layout)
                } else {
                    iterator.next(layout)
                };
            } else {
                j = nexttab[j as usize];
            }
            // `while (nextit && j < l)` amont.
            if !(nextit && j < l) {
                break;
            }
        }

        if j >= l {
            if backwards {
                // Position de la clé (verbatim).
                iterator.next(layout);
            } else {
                // Retour à l'endroit où la clé a été trouvée.
                for _ in 0..l {
                    iterator.prev(layout);
                }
            }
            ret = true;
        }

        ret
    }

    /// `charsEqual` — `dTolower`/`dIsspace` ASCII (verbatim,
    /// espaces équivalents en insensible).
    fn chars_equal(c1: i32, c2: i32, case_sens: bool) -> bool {
        if case_sens {
            c1 == c2
        } else {
            let (b1, b2) = (c1 as u8, c2 as u8);
            dlib::to_lower(b1) == dlib::to_lower(b2) || (dlib::is_space(b1) && dlib::is_space(b2))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imgbuf::{Imgbuf, ImgbufType};
    use crate::layout::IdleCallback;
    use crate::platform::Platform;
    use crate::style::{Font, FontAttrs, Style, StyleAttrs, Tooltip};
    use crate::view::View;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// `Platform` minimale — `text_width` = 1px/octet pour des
    /// largeurs non nulles (≙ mock de `iterator::tests`).
    #[derive(Default)]
    struct MockPlatform {
        /// Ids idle alloués.
        next_id: i32,
        /// Dernière sélection copiée (`copySelection`).
        copied: Vec<u8>,
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
            unreachable!("imgbuf non utilisé par les tests findtext")
        }
        fn copy_selection(&mut self, t: &[u8], _d: i32) {
            self.copied = t.to_vec();
        }
        fn get_resource_factory(&mut self) -> Option<Rc<dyn crate::ui::ResourceFactory>> {
            None
        }
    }

    /// Layout + textblock en top-level avec `text` en mots.
    fn make_tb(l: &mut Layout, text: &[u8]) -> WidgetId {
        let w = l.tb_new(false, false);
        let style = Style::create(&StyleAttrs::default());
        l.tb_add_text(w, text, text.len(), &style);
        l.set_widget(w);
        w
    }

    #[test]
    fn search_forward_success_restart() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        make_tb(&mut l, b"hello world hello");
        // 1re occurrence puis 2e, puis boucle → RESTART (verbatim
        // findtext.cc:118-131), puis à nouveau la 2e.
        assert_eq!(l.search(b"hello", true, false), FindtextResult::Success);
        assert_eq!(l.search(b"hello", true, false), FindtextResult::Success);
        assert_eq!(l.search(b"hello", true, false), FindtextResult::Restart);
        assert_eq!(l.search(b"hello", true, false), FindtextResult::Success);
    }

    #[test]
    fn search_not_found_and_empty_key() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        make_tb(&mut l, b"hello world");
        assert_eq!(l.search(b"zzz", true, false), FindtextResult::NotFound);
        // `*key == 0` amont ≙ clé vide → NOT_FOUND.
        assert_eq!(l.search(b"", true, false), FindtextResult::NotFound);
    }

    #[test]
    fn search_backwards_finds_last() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        make_tb(&mut l, b"one hello two hello three");
        assert_eq!(l.search(b"hello", true, true), FindtextResult::Success);
        assert_eq!(l.search(b"hello", true, true), FindtextResult::Success);
        assert_eq!(l.search(b"hello", true, true), FindtextResult::Restart);
    }

    #[test]
    fn search_case_sensitivity() {
        let mut l = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        make_tb(&mut l, b"Hello world");
        assert_eq!(l.search(b"hello", false, false), FindtextResult::Success);
        // Nouvelle clé → repart du début ; casse → NOT_FOUND.
        let mut l2 = Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true);
        make_tb(&mut l2, b"Hello world");
        assert_eq!(l2.search(b"hello", true, false), FindtextResult::NotFound);
    }
}
