//! Port de `dw/iterator.hh` + `dw/iterator.cc` — itérateurs
//! de contenu widget.
//!
//! `Iterator` (classe abstraite amont) → trait object
//! [`DwIterator`] : `Widget*` → [`WidgetId`], accès aux nœuds
//! via `&Layout`/`&mut Layout` (modèle ADR-0005 — les
//! itérateurs ne détiennent aucune référence, seulement des
//! ids + leur position).
//!
//! `clone()`/`cloneIterator()`/`cloneDeepIterator()` amont →
//! `clone_box`/`Clone` (`DeepIterator`/`CharIterator` sont des
//! types concrets `Clone` ; les impls `Iterator` restent des
//! trait objects clonables).
//!
//! `unref`/`delete` amont → `Drop`. `compareTo` entre
//! itérateurs de widgets différents est UB amont (cast forcé)
//! — le port compare la position (content.type puis index) :
//! comportement documenté, jamais appelé cross-kind en
//! pratique amont.
//!
//! `intoStringBuffer` (debug) non porté — `std::fmt::Debug`
//! suffit.
//!
//! Positions/index : `int` amont → `i32`/`usize` — les
//! contenus dw ne dépassent jamais `INT_MAX` éléments
//! (verbatim des limites amont).

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

use std::rc::Rc;

use crate::layout::Layout;
use crate::types::{
    content_type, Allocation, Content, ContentData, HPosition, HighlightLayer, VPosition,
};
use crate::WidgetId;

// ------------------------------------------------------------------
// DwIterator ≙ Iterator (trait object).
// ------------------------------------------------------------------

/// `dw::core::Iterator` — trait object.
///
/// Méthodes `next`/`prev` pures de lecture d'arène → `&Layout` ;
/// `highlight`/`unhighlight`/`scroll_to` mutent → `&mut Layout`.
pub trait DwIterator {
    /// `getWidget`.
    fn widget(&self) -> WidgetId;
    /// `getMask`.
    fn mask(&self) -> i16;
    /// `getContent`.
    fn content(&self) -> &Content;
    /// Mutation du contenu (≙ affectation `content = ...`).
    fn set_content(&mut self, content: Content);
    /// `cloneIterator` ≙ `clone` polymorphe amont.
    fn clone_box(&self) -> Box<dyn DwIterator>;
    /// `compareTo` — ordre au sein du widget (≙ même
    /// `widget` pour les itérateurs comparés).
    fn compare(&self, other: &dyn DwIterator) -> i32;

    /// Downcast (≙ cast C++ amont dans `compareTo` — le port
    /// vérifie le type au lieu de supposer).
    fn as_any(&self) -> &dyn std::any::Any;

    /// `equals` — `this == other || same widget && compareTo
    /// == 0` (verbatim ; les boîtes distinctes ne sont jamais
    /// identiques — seul le second terme reste).
    fn equals(&self, other: &dyn DwIterator) -> bool {
        self.widget() == other.widget() && self.compare(other) == 0
    }

    /// `next` — avance et stocke le contenu ; `false` à la fin
    /// (content = END ≙ amont).
    fn next(&mut self, layout: &Layout) -> bool;
    /// `prev`.
    fn prev(&mut self, layout: &Layout) -> bool;
    /// `highlight` — défaut no-op (≙ `EmptyIterator` amont).
    fn highlight(&mut self, _layout: &mut Layout, _start: i32, _end: i32, _layer: HighlightLayer) {}
    /// `unhighlight` — défaut no-op.
    fn unhighlight(&mut self, _layout: &mut Layout, _direction: i32, _layer: HighlightLayer) {}
    /// `getAllocation` — `None` ≙ rien écrit (≙
    /// `EmptyIterator` amont ; `allocation` sort inchangé).
    fn get_allocation(
        &self,
        _layout: &Layout,
        _start: i32,
        _end: i32,
        _allocation: &mut Allocation,
    ) {
    }
}

/// `Iterator::scrollTo` — verbatim iterator.cc:76 : région
/// entre `it1` et `it2` scrollée en vue.
pub fn iterator_scroll_to(
    layout: &mut Layout,
    it1: &mut dyn DwIterator,
    it2: &mut dyn DwIterator,
    start: i32,
    end: i32,
    hpos: HPosition,
    vpos: VPosition,
) {
    let mut alloc = Allocation::default();
    if it1.equals(it2) {
        it1.get_allocation(layout, start, end, &mut alloc);
        layout.scroll_to(
            hpos,
            vpos,
            alloc.x,
            alloc.y,
            alloc.width,
            alloc.ascent + alloc.descent,
        );
        return;
    }

    // Rectangle englobant tous les contenus entre it1 et it2.
    let mut x1 = i32::MAX;
    let mut x2 = i32::MIN;
    let mut y1 = i32::MAX;
    let mut y2 = i32::MIN;

    let mut eit3 = DeepIterator::new(layout, it1.clone_box());
    let eit2 = DeepIterator::new(layout, it2.clone_box());
    let mut at_start = true;
    loop {
        let cmp = eit3.compare_to(&eit2);
        if cmp > 0 {
            break;
        }
        let cur_start = if at_start { start } else { 0 };
        let cur_end = if cmp == 0 { end } else { i32::MAX };
        eit3.get_allocation(layout, cur_start, cur_end, &mut alloc);
        x1 = x1.min(alloc.x);
        x2 = x2.max(alloc.x + alloc.width);
        y1 = y1.min(alloc.y);
        y2 = y2.max(alloc.y + alloc.ascent + alloc.descent);
        eit3.next(layout);
        at_start = false;
    }

    let mut alloc1 = Allocation::default();
    let mut alloc2 = Allocation::default();
    it1.get_allocation(layout, start, i32::MAX, &mut alloc1);
    it2.get_allocation(layout, 0, end, &mut alloc2);

    if alloc1.x > alloc2.x {
        // Saut de ligne dans la région — verbatim : rétrécit
        // la région à la largeur du viewport (si plus large).
        if layout.get_uses_viewport() && x2 - x1 > layout.get_width_viewport() {
            x1 = x2 - layout.get_width_viewport();
            x2 = x1 + layout.get_width_viewport();
        }
    }
    if alloc1.y > alloc2.y && layout.get_uses_viewport() && y2 - y1 > layout.get_height_viewport() {
        y1 = y2 - layout.get_height_viewport();
        y2 = y1 + layout.get_height_viewport();
    }

    layout.scroll_to(hpos, vpos, x1, y1, x2 - x1, y2 - y1);
}

// ------------------------------------------------------------------
// EmptyIterator — widget sans contenu (iterator.cc:172).
// ------------------------------------------------------------------

/// `EmptyIterator`.
#[derive(Debug, Clone)]
pub struct EmptyIterator {
    /// `widget`.
    widget: WidgetId,
    /// `mask`.
    mask: i16,
    /// `content` — START ou END.
    content: Content,
}

impl EmptyIterator {
    /// `EmptyIterator (widget, mask, atEnd)` — verbatim.
    #[must_use]
    pub fn new(widget: WidgetId, mask: i16, at_end: bool) -> Self {
        EmptyIterator {
            widget,
            mask,
            content: Content {
                r#type: if at_end {
                    content_type::END
                } else {
                    content_type::START
                },
                space: false,
                data: ContentData::None,
            },
        }
    }
}

impl DwIterator for EmptyIterator {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn widget(&self) -> WidgetId {
        self.widget
    }
    fn mask(&self) -> i16 {
        self.mask
    }
    fn content(&self) -> &Content {
        &self.content
    }
    fn set_content(&mut self, content: Content) {
        self.content = content;
    }
    fn clone_box(&self) -> Box<dyn DwIterator> {
        Box::new(self.clone())
    }
    /// `compareTo` — verbatim (START < autre < END).
    fn compare(&self, other: &dyn DwIterator) -> i32 {
        let t_self = self.content.r#type;
        let t_other = other.content().r#type;
        if t_self == t_other {
            0
        } else if t_self == content_type::START {
            -1
        } else {
            1
        }
    }
    fn next(&mut self, _layout: &Layout) -> bool {
        self.content.r#type = content_type::END;
        false
    }
    fn prev(&mut self, _layout: &Layout) -> bool {
        self.content.r#type = content_type::START;
        false
    }
}

// ------------------------------------------------------------------
// TextIterator — un seul mot texte (iterator.cc:250).
// ------------------------------------------------------------------

/// `TextIterator`.
#[derive(Debug, Clone)]
pub struct TextIterator {
    /// `widget`.
    widget: WidgetId,
    /// `mask`.
    mask: i16,
    /// `content`.
    content: Content,
    /// `text` — `NULL` si `mask & TEXT == 0` (verbatim).
    text: Option<Rc<[u8]>>,
}

impl TextIterator {
    /// `TextIterator (widget, mask, atEnd, text)` — verbatim.
    #[must_use]
    pub fn new(widget: WidgetId, mask: i16, at_end: bool, text: &[u8]) -> Self {
        TextIterator {
            widget,
            mask,
            content: Content {
                r#type: if at_end {
                    content_type::END
                } else {
                    content_type::START
                },
                space: false,
                data: ContentData::None,
            },
            text: if mask & content_type::TEXT != 0 {
                Some(Rc::from(text))
            } else {
                None
            },
        }
    }
}

impl DwIterator for TextIterator {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn widget(&self) -> WidgetId {
        self.widget
    }
    fn mask(&self) -> i16 {
        self.mask
    }
    fn content(&self) -> &Content {
        &self.content
    }
    fn set_content(&mut self, content: Content) {
        self.content = content;
    }
    fn clone_box(&self) -> Box<dyn DwIterator> {
        Box::new(self.clone())
    }
    /// `compareTo` — verbatim iterator.cc:281.
    fn compare(&self, other: &dyn DwIterator) -> i32 {
        let t_self = self.content.r#type;
        let t_other = other.content().r#type;
        if t_self == t_other {
            return 0;
        }
        match t_self {
            t if t == content_type::START => -1,
            t if t == content_type::TEXT => {
                if t_other == content_type::START {
                    1
                } else {
                    -1
                }
            }
            _ => 1, // END.
        }
    }
    /// `next` — verbatim.
    fn next(&mut self, _layout: &Layout) -> bool {
        if self.content.r#type == content_type::START && self.text.is_some() {
            self.content.r#type = content_type::TEXT;
            self.content.data =
                ContentData::Text(self.text.as_ref().map_or_else(Vec::new, |t| t.to_vec()));
            true
        } else {
            self.content.r#type = content_type::END;
            false
        }
    }
    /// `prev` — verbatim.
    fn prev(&mut self, _layout: &Layout) -> bool {
        if self.content.r#type == content_type::END && self.text.is_some() {
            self.content.r#type = content_type::TEXT;
            self.content.data =
                ContentData::Text(self.text.as_ref().map_or_else(Vec::new, |t| t.to_vec()));
            true
        } else {
            self.content.r#type = content_type::START;
            false
        }
    }
    /// `getAllocation` — allocation du widget (verbatim).
    fn get_allocation(&self, layout: &Layout, _start: i32, _end: i32, allocation: &mut Allocation) {
        *allocation = layout.node(self.widget, |n| n.allocation);
    }
}

// ------------------------------------------------------------------
// ChildIterator — seam conteneur (≙ itérateur d'enfants directs
// `WIDGET_IN_FLOW` — les kinds concrets (Textblock…) auront
// leur itérateur dédié ; `WidgetKind::Container` itère ses
// `children`).
// ------------------------------------------------------------------

/// Itérateur sur `WidgetKind::Container::children` — seam
/// modèle D (≙ itérateur in-flow d'un conteneur amont).
#[derive(Debug, Clone)]
pub struct ChildIterator {
    /// `widget` (le conteneur).
    widget: WidgetId,
    /// `mask`.
    mask: i16,
    /// `content`.
    content: Content,
    /// Position : `-1` = START, `len` = END (snapshot à la
    /// création — ≙ itérateur amont sur structure live :
    /// les mutations invalident, documenté).
    pos: i64,
    /// `children` snapshot.
    children: Rc<[WidgetId]>,
}

impl ChildIterator {
    /// `ChildIterator (widget, mask, atEnd, children)`.
    #[must_use]
    pub fn new(widget: WidgetId, mask: i16, at_end: bool, children: Rc<[WidgetId]>) -> Self {
        ChildIterator {
            widget,
            mask,
            content: Content {
                r#type: if at_end {
                    content_type::END
                } else {
                    content_type::START
                },
                space: false,
                data: ContentData::None,
            },
            pos: if at_end { children.len() as i64 } else { -1 },
            children,
        }
    }
}

impl DwIterator for ChildIterator {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn widget(&self) -> WidgetId {
        self.widget
    }
    fn mask(&self) -> i16 {
        self.mask
    }
    fn content(&self) -> &Content {
        &self.content
    }
    fn set_content(&mut self, content: Content) {
        self.content = content;
    }
    fn clone_box(&self) -> Box<dyn DwIterator> {
        Box::new(self.clone())
    }
    /// `compareTo` — type puis index (≙ position amont dans la
    /// liste enfants : même ordre ; autre type → `0` ≙ cast
    /// amont non vérifié).
    fn compare(&self, other: &dyn DwIterator) -> i32 {
        let t_self = self.content.r#type;
        let t_other = other.content().r#type;
        if t_self == t_other {
            if t_self == content_type::WIDGET_IN_FLOW {
                let o_pos = other
                    .as_any()
                    .downcast_ref::<ChildIterator>()
                    .map_or(self.pos, |o| o.pos);
                return self.pos.cmp(&o_pos) as i32;
            }
            0
        } else if t_self == content_type::START {
            -1
        } else {
            1
        }
    }
    fn next(&mut self, _layout: &Layout) -> bool {
        if self.pos < self.children.len() as i64 - 1 {
            self.pos += 1;
            self.content = Content {
                r#type: content_type::WIDGET_IN_FLOW,
                space: false,
                data: ContentData::Widget(self.children[self.pos as usize]),
            };
            true
        } else {
            self.content.r#type = content_type::END;
            self.pos = self.children.len() as i64;
            false
        }
    }
    fn prev(&mut self, _layout: &Layout) -> bool {
        if self.pos > 0 {
            self.pos -= 1;
            self.content = Content {
                r#type: content_type::WIDGET_IN_FLOW,
                space: false,
                data: ContentData::Widget(self.children[self.pos as usize]),
            };
            true
        } else {
            self.content.r#type = content_type::START;
            self.pos = -1;
            false
        }
    }
    /// `getAllocation` — allocation du widget pointé (≙
    /// contenu widget amont : l'allocation du child).
    fn get_allocation(&self, layout: &Layout, _start: i32, _end: i32, allocation: &mut Allocation) {
        if let Some(w) = self.content.get_widget() {
            *allocation = layout.node(w, |n| n.allocation);
        }
    }
}

// ------------------------------------------------------------------
// DeepIterator — pile récursive (iterator.cc:334).
// ------------------------------------------------------------------

/// `DeepIterator` — itère récursivement les sous-arbres
/// widgets ; ne retourne jamais de contenu widget.
pub struct DeepIterator {
    /// `stack` — itérateurs par niveau (index = `level - 1`
    /// amont, top = dernier).
    stack: Vec<Box<dyn DwIterator>>,
    /// `mask`.
    mask: i16,
    /// `content`.
    content: Content,
    /// `hasContents`.
    has_contents: bool,
}

impl std::fmt::Debug for DeepIterator {
    /// `Box<dyn DwIterator>` n'est pas `Debug` — affiche les
    /// scalaires (≙ `print()` amont, réduit).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeepIterator")
            .field("stack_len", &self.stack.len())
            .field("mask", &self.mask)
            .field("content", &self.content.r#type)
            .field("has_contents", &self.has_contents)
            .finish()
    }
}

/// `clone` ≙ `DeepIterator::clone` amont (iterator.cc:598 :
/// chaque itérateur de pile est `cloneIterator()`-é).
impl Clone for DeepIterator {
    fn clone(&self) -> Self {
        DeepIterator {
            stack: self.stack.iter().map(|it| it.clone_box()).collect(),
            mask: self.mask,
            content: self.content.clone(),
            has_contents: self.has_contents,
        }
    }
}

/// `getRespectiveParent` — verbatim iterator.cc:421 :
/// générateur si `WIDGET_OOF_REF`, sinon parent.
fn respective_parent(layout: &Layout, widget: WidgetId, mask: i16) -> Option<WidgetId> {
    if mask & content_type::WIDGET_OOF_REF != 0 {
        layout.widget_generator(widget)
    } else {
        layout.widget_parent(widget)
    }
}

/// `getRespectiveLevel` — verbatim.
fn respective_level(layout: &Layout, widget: WidgetId, mask: i16) -> i32 {
    if mask & content_type::WIDGET_OOF_REF != 0 {
        layout.widget_generator_level(widget)
    } else {
        layout.widget_level(widget)
    }
}

/// `searchDownward` — verbatim iterator.cc:343 : descend dans
/// le widget pointé par `it` jusqu'à du contenu non-widget.
fn search_downward(
    layout: &Layout,
    it: &mut dyn DwIterator,
    mask: i16,
    from_end: bool,
) -> Option<Box<dyn DwIterator>> {
    let w = it.content().get_widget()?;
    let mut it2 = layout.w_iterator(w, mask, from_end);

    loop {
        let advanced = if from_end {
            it2.prev(layout)
        } else {
            it2.next(layout)
        };
        if !advanced {
            return None;
        }
        if it2.content().r#type & content_type::ANY_WIDGET != 0 {
            if let Some(it3) = search_downward(layout, it2.as_mut(), mask, from_end) {
                return Some(it3);
            }
        } else {
            return Some(it2);
        }
    }
}

/// `searchSideward` — verbatim iterator.cc:402 : avance dans
/// `it`, descend dans les widgets rencontrés, remonte au parent
/// « respectif » si épuisé.
fn search_sideward(
    layout: &Layout,
    it: &mut dyn DwIterator,
    mask: i16,
    from_end: bool,
) -> Option<Box<dyn DwIterator>> {
    let mut it2 = it.clone_box();
    loop {
        let advanced = if from_end {
            it2.prev(layout)
        } else {
            it2.next(layout)
        };
        if !advanced {
            break;
        }
        if it2.content().r#type & content_type::ANY_WIDGET != 0 {
            if let Some(it3) = search_downward(layout, it2.as_mut(), mask, from_end) {
                return Some(it3);
            }
        } else {
            return Some(it2);
        }
    }

    // Remontée dans l'arbre (verbatim).
    let resp_parent = respective_parent(layout, it.widget(), it.mask())?;
    let mut it2 = layout.w_iterator(resp_parent, mask, false);
    loop {
        if !it2.next(layout) {
            // `assertNotReached` amont — le parent contient
            // forcément l'enfant ; structure corrompue sinon.
            return None;
        }
        if it2.content().r#type & content_type::ANY_WIDGET != 0
            && it2.content().get_widget() == Some(it.widget())
        {
            return search_sideward(layout, it2.as_mut(), mask, from_end);
        }
    }
}

impl DeepIterator {
    /// `DeepIterator (Iterator *it)` — verbatim iterator.cc:454 :
    /// l'itérateur est corrigé vers du contenu non-widget, puis
    /// la pile ancêtres est construite niveau par niveau.
    #[must_use]
    pub fn new(layout: &Layout, it: Box<dyn DwIterator>) -> Self {
        let mut it = it;
        let mask = it.mask();
        let mut has_contents = true;

        if it.content().r#type & content_type::ANY_WIDGET != 0 {
            let fixed = search_downward(layout, it.as_mut(), mask, false)
                .or_else(|| search_sideward(layout, it.as_mut(), mask, false))
                .or_else(|| search_sideward(layout, it.as_mut(), mask, true));
            match fixed {
                Some(f) => it = f,
                None => has_contents = false,
            }
        }

        let mut stack: Vec<Box<dyn DwIterator>> = Vec::new();
        let mut content = Content {
            r#type: content_type::INVALID,
            space: false,
            data: ContentData::None,
        };

        if has_contents {
            // Pile : itérateurs des ancêtres « respectifs »
            // positionnés sur le widget du niveau inférieur
            // (verbatim : slot `level - 1`).
            let this_level = respective_level(layout, it.widget(), mask);
            let mut w = it.widget();
            let mut level = this_level;
            while let Some(p) = respective_parent(layout, w, mask) {
                let mut pit = layout.w_iterator(p, mask, false);
                loop {
                    // `assert(hasNext)` amont — le parent
                    // contient w par construction.
                    if !pit.next(layout) {
                        break;
                    }
                    if pit.content().r#type & content_type::ANY_WIDGET != 0
                        && pit.content().get_widget() == Some(w)
                    {
                        break;
                    }
                }
                let idx = (level - 1).max(0) as usize;
                if stack.len() <= idx {
                    stack.resize_with(idx + 1, || {
                        Box::new(EmptyIterator::new(w, mask, false)) as Box<dyn DwIterator>
                    });
                }
                stack[idx] = pit;
                w = p;
                level -= 1;
            }
            let idx = this_level.max(0) as usize;
            if stack.len() <= idx {
                stack.resize_with(idx + 1, || {
                    Box::new(EmptyIterator::new(w, mask, false)) as Box<dyn DwIterator>
                });
            }
            stack[idx] = it;
            content = stack[idx].content().clone();
        }

        DeepIterator {
            stack,
            mask,
            content,
            has_contents,
        }
    }

    /// `getTopIterator` — `None` si pile vide (≙ UB amont ;
    /// les appelants testent `is_empty` avant).
    #[must_use]
    pub fn top_iterator(&mut self) -> Option<&mut dyn DwIterator> {
        self.stack
            .last_mut()
            .map(|b| -> &mut dyn DwIterator { &mut **b })
    }

    /// `getContent`.
    #[must_use]
    pub fn content(&self) -> &Content {
        &self.content
    }

    /// `isEmpty`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.has_contents
    }

    /// `getMask`.
    #[must_use]
    pub fn mask(&self) -> i16 {
        self.mask
    }

    /// `compareTo` — verbatim iterator.cc:608 : plus haut niveau
    /// de widget commun, puis `compareTo` des itérateurs à ce
    /// niveau. (L'assert amont « même arbre » devient un ordre
    /// stable — les arbres disjoints comparent sur le niveau 0.)
    #[must_use]
    pub fn compare_to(&self, other: &DeepIterator) -> i32 {
        let mut level = 0usize;
        let max_level = self.stack.len().min(other.stack.len());
        while level + 1 < max_level
            && self
                .stack
                .get(level + 1)
                .zip(other.stack.get(level + 1))
                .is_some_and(|(a, b)| a.widget() == b.widget())
        {
            level += 1;
        }
        // recule tant que les widgets diffèrent (verbatim).
        while level > 0
            && self
                .stack
                .get(level)
                .zip(other.stack.get(level))
                .is_some_and(|(a, b)| a.widget() != b.widget())
        {
            level -= 1;
        }
        match (self.stack.get(level), other.stack.get(level)) {
            (Some(a), Some(b)) => a.compare(b.as_ref()),
            _ => 0,
        }
    }

    /// `createVariant` — verbatim (todo amont → `new DeepIterator`).
    #[must_use]
    pub fn create_variant(layout: &Layout, it: Box<dyn DwIterator>) -> Self {
        DeepIterator::new(layout, it)
    }

    /// `next` — verbatim iterator.cc:658 : descend dans les
    /// widgets rencontrés, remonte au parent si épuisé.
    pub fn next(&mut self, layout: &Layout) -> bool {
        if self.stack.is_empty() {
            self.content.r#type = content_type::END;
            return false;
        }
        let top = self.stack.len() - 1;
        if self.stack[top].next(layout) {
            if self.stack[top].content().r#type & content_type::ANY_WIDGET != 0 {
                let w = self.stack[top].content().get_widget();
                if let Some(w) = w {
                    self.stack.push(layout.w_iterator(w, self.mask, false));
                }
                self.next(layout)
            } else {
                self.content = self.stack[top].content().clone();
                true
            }
        } else if self.stack.len() > 1 {
            self.stack.pop();
            self.next(layout)
        } else {
            self.content.r#type = content_type::END;
            false
        }
    }

    /// `prev` — verbatim iterator.cc:687.
    pub fn prev(&mut self, layout: &Layout) -> bool {
        if self.stack.is_empty() {
            self.content.r#type = content_type::START;
            return false;
        }
        let top = self.stack.len() - 1;
        if self.stack[top].prev(layout) {
            if self.stack[top].content().r#type & content_type::ANY_WIDGET != 0 {
                let w = self.stack[top].content().get_widget();
                if let Some(w) = w {
                    self.stack.push(layout.w_iterator(w, self.mask, true));
                }
                self.prev(layout)
            } else {
                self.content = self.stack[top].content().clone();
                true
            }
        } else if self.stack.len() > 1 {
            self.stack.pop();
            self.prev(layout)
        } else {
            self.content.r#type = content_type::START;
            false
        }
    }

    /// `highlight` — délégué au top (verbatim inline).
    pub fn highlight(&mut self, layout: &mut Layout, start: i32, end: i32, layer: HighlightLayer) {
        if let Some(top) = self.stack.last_mut() {
            top.highlight(layout, start, end, layer);
        }
    }
    /// `unhighlight`.
    pub fn unhighlight(&mut self, layout: &mut Layout, direction: i32, layer: HighlightLayer) {
        if let Some(top) = self.stack.last_mut() {
            top.unhighlight(layout, direction, layer);
        }
    }
    /// `getAllocation`.
    pub fn get_allocation(
        &mut self,
        layout: &Layout,
        start: i32,
        end: i32,
        allocation: &mut Allocation,
    ) {
        if let Some(top) = self.stack.last_mut() {
            top.get_allocation(layout, start, end, allocation);
        }
    }
}

// ------------------------------------------------------------------
// CharIterator — caractères (iterator.cc:730).
// ------------------------------------------------------------------

/// `CharIterator::START`/`END` — hors plage char (verbatim).
pub const CHAR_START: i32 = 257;
/// `END`.
pub const CHAR_END: i32 = 258;

/// `strlen` ≙ longueur jusqu'au premier NUL (texte amont
/// NUL-terminé).
fn text_len(text: &[u8]) -> usize {
    text.iter().position(|&b| b == 0).unwrap_or(text.len())
}

/// `text[pos]` ≙ octet ou `0` hors bornes (NUL amont).
fn text_byte(text: &[u8], pos: usize) -> i32 {
    i32::from(text.get(pos).copied().unwrap_or(0))
}

/// `ContentData::Text` → slice.
fn content_text(c: &Content) -> &[u8] {
    match &c.data {
        ContentData::Text(t) => t,
        _ => &[],
    }
}

/// `CharIterator` — verbatim : `DeepIterator` + `pos`/`ch`.
#[derive(Clone)]
pub struct CharIterator {
    /// `it`.
    it: DeepIterator,
    /// `pos` — index dans le contenu TEXT courant.
    pos: usize,
    /// `ch` — caractère courant (`START`/`END` sentinels).
    ch: i32,
}

impl std::fmt::Debug for CharIterator {
    /// Scalaires uniquement (`DeepIterator` résumé — ≙ `print`
    /// amont réduit).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CharIterator")
            .field("pos", &self.pos)
            .field("ch", &self.ch)
            .field("it", &self.it)
            .finish()
    }
}

impl CharIterator {
    /// `CharIterator (widget, followReferences)` — verbatim :
    /// `DeepIterator` sur `maskForSelection`, `ch = START`.
    #[must_use]
    pub fn new(layout: &Layout, widget: WidgetId, follow_references: bool) -> Self {
        let mask = Content::mask_for_selection(follow_references);
        let it = layout.w_iterator(widget, mask, false);
        CharIterator {
            it: DeepIterator::new(layout, it),
            pos: 0,
            ch: CHAR_START,
        }
    }

    /// `getChar`.
    #[must_use]
    pub fn get_char(&self) -> i32 {
        self.ch
    }

    /// `pos` courant (≙ `it->pos` — utilisé par scrollTo).
    #[must_use]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// `compareTo` — verbatim : deep puis `pos`.
    #[must_use]
    pub fn compare_to(&self, other: &CharIterator) -> i32 {
        let c = self.it.compare_to(&other.it);
        if c != 0 {
            c
        } else {
            (self.pos as i32) - (other.pos as i32)
        }
    }

    /// `next` — verbatim iterator.cc:774 : BREAK→'\n', TEXT→
    /// octets (espaces `content.space` entre mots → ' '),
    /// fin → END.
    pub fn next(&mut self, layout: &Layout) -> bool {
        if self.ch == CHAR_START
            || self.it.content().r#type == content_type::BREAK
            || (self.it.content().r#type == content_type::TEXT
                && text_byte(content_text(self.it.content()), self.pos) == 0)
        {
            if self.it.next(layout) {
                if self.it.content().r#type == content_type::BREAK {
                    self.ch = i32::from(b'\n');
                } else {
                    // TEXT.
                    self.pos = 0;
                    self.ch = text_byte(content_text(self.it.content()), self.pos);
                    if self.ch == 0 {
                        // « should not happen » amont.
                        return self.next(layout);
                    }
                }
                true
            } else {
                self.ch = CHAR_END;
                false
            }
        } else if self.ch == CHAR_END {
            false
        } else {
            // TEXT courant.
            self.pos += 1;
            self.ch = text_byte(content_text(self.it.content()), self.pos);
            if self.ch == 0 {
                if self.it.content().space {
                    self.ch = i32::from(b' ');
                } else {
                    return self.next(layout);
                }
            }
            true
        }
    }

    /// `prev` — verbatim iterator.cc:805.
    pub fn prev(&mut self, layout: &Layout) -> bool {
        if self.ch == CHAR_END
            || self.it.content().r#type == content_type::BREAK
            || (self.it.content().r#type == content_type::TEXT && self.pos == 0)
        {
            if self.it.prev(layout) {
                if self.it.content().r#type == content_type::BREAK {
                    self.ch = i32::from(b'\n');
                } else {
                    // TEXT.
                    if text_byte(content_text(self.it.content()), 0) == 0 {
                        return self.prev(layout);
                    }
                    self.pos = text_len(content_text(self.it.content()));
                    if self.it.content().space {
                        self.ch = i32::from(b' ');
                    } else {
                        self.pos -= 1;
                        self.ch = text_byte(content_text(self.it.content()), self.pos);
                    }
                }
                true
            } else {
                self.ch = CHAR_START;
                false
            }
        } else if self.ch == CHAR_START {
            false
        } else {
            self.pos -= 1;
            self.ch = text_byte(content_text(self.it.content()), self.pos);
            true
        }
    }

    /// `highlight` (statique amont) — verbatim iterator.cc:831 :
    /// même contenu → plage partielle, sinon balayage deep.
    pub fn highlight(
        layout: &mut Layout,
        it1: &mut CharIterator,
        it2: &mut CharIterator,
        layer: HighlightLayer,
    ) {
        if it2.get_char() == CHAR_END {
            it2.prev(layout);
        }
        if it1.it.compare_to(&it2.it) == 0 {
            let (s, e) = (it1.pos as i32, it2.pos as i32);
            it1.it.highlight(layout, s, e, layer);
        } else {
            let mut it = it1.it.clone();
            let mut start = true;
            loop {
                let c = it.compare_to(&it2.it);
                if c > 0 {
                    break;
                }
                let end_of_word = if it.content().r#type == content_type::TEXT {
                    text_len(content_text(it.content())) as i32
                } else {
                    1
                };
                if start {
                    it.highlight(layout, it1.pos as i32, end_of_word, layer);
                } else if c == 0 {
                    it.highlight(layout, 0, it2.pos as i32, layer);
                } else {
                    it.highlight(layout, 0, end_of_word, layer);
                }
                it.next(layout);
                start = false;
            }
        }
    }

    /// `unhighlight` — verbatim iterator.cc:857.
    pub fn unhighlight(
        layout: &mut Layout,
        it1: &mut CharIterator,
        it2: &mut CharIterator,
        layer: HighlightLayer,
    ) {
        if it1.it.compare_to(&it2.it) == 0 {
            it1.it.unhighlight(layout, 0, layer);
        } else {
            let mut it = it1.it.clone();
            while it.compare_to(&it2.it) <= 0 {
                it.unhighlight(layout, -1, layer);
                it.next(layout);
            }
        }
    }

    /// `scrollTo` — verbatim : `DeepIterator::scrollTo` sur les
    /// positions courantes.
    pub fn scroll_to(
        layout: &mut Layout,
        it1: &mut CharIterator,
        it2: &mut CharIterator,
        hpos: HPosition,
        vpos: VPosition,
    ) {
        let (t1, t2) = (it1.it.stack.last_mut(), it2.it.stack.last_mut());
        if let (Some(a), Some(b)) = (t1, t2) {
            iterator_scroll_to(
                layout,
                a.as_mut(),
                b.as_mut(),
                it1.pos as i32,
                it2.pos as i32,
                hpos,
                vpos,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imgbuf::{Imgbuf, ImgbufType};
    use crate::layout::IdleCallback;
    use crate::platform::Platform;
    use crate::style::{Font, FontAttrs, Tooltip};
    use crate::view::View;
    use crate::widget::WidgetKind;
    use std::cell::RefCell;

    /// `Platform` minimale (≙ mock de `layout::tests`).
    #[derive(Default)]
    struct MockPlatform {
        /// Ids alloués.
        next_id: i32,
    }

    impl Platform for MockPlatform {
        fn set_layout(&mut self, _l: usize) {}
        fn attach_view(&mut self, _v: &mut dyn View) {}
        fn detach_view(&mut self, _v: &mut dyn View) {}
        fn text_width(&mut self, _f: &Font, _t: &[u8]) -> i32 {
            0
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
            self.next_id += 1;
            self.next_id
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
            unreachable!("imgbuf non utilisé par les tests iterator")
        }
        fn copy_selection(&mut self, _t: &[u8], _d: i32) {}
        fn get_resource_factory(&mut self) -> Option<Rc<dyn crate::ui::ResourceFactory>> {
            None
        }
    }

    fn make_layout() -> Layout {
        Layout::new(Rc::new(RefCell::new(MockPlatform::default())), true)
    }

    /// Arbre : root { mid { Text "ab" }, Text "cd" }.
    fn make_tree(l: &mut Layout) -> (WidgetId, WidgetId) {
        let root = l.add_widget_node(WidgetKind::Container {
            children: Vec::new(),
        });
        let mid = l.add_widget_node(WidgetKind::Container {
            children: Vec::new(),
        });
        let t1 = l.add_widget_node(WidgetKind::Text {
            text: Rc::from(&b"ab"[..]),
        });
        let t2 = l.add_widget_node(WidgetKind::Text {
            text: Rc::from(&b"cd"[..]),
        });
        l.container_add_child(mid, t1);
        l.container_add_child(root, mid);
        l.container_add_child(root, t2);
        (root, t2)
    }

    #[test]
    fn empty_iterator_start_end() {
        let mut it = EmptyIterator::new(1, content_type::ALL, false);
        assert_eq!(it.content().r#type, content_type::START);
        let l = make_layout();
        assert!(!it.next(&l));
        assert_eq!(it.content().r#type, content_type::END);
        assert!(!it.prev(&l));
        assert_eq!(it.content().r#type, content_type::START);
        // compareTo verbatim : START < END.
        let it2 = EmptyIterator::new(1, content_type::ALL, true);
        assert_eq!(it.compare(&it2), -1);
        assert_eq!(it2.compare(&it), 1);
        assert!(it.equals(&it));
    }

    #[test]
    fn text_iterator_word() {
        let l = make_layout();
        let mut it = TextIterator::new(1, content_type::ALL, false, b"hi");
        assert!(it.next(&l));
        assert_eq!(it.content().r#type, content_type::TEXT);
        assert_eq!(content_text(it.content()), b"hi");
        assert!(!it.next(&l));
        assert_eq!(it.content().r#type, content_type::END);
        assert!(it.prev(&l));
        assert_eq!(it.content().r#type, content_type::TEXT);
        // mask sans TEXT → texte ignoré (verbatim).
        let mut it2 = TextIterator::new(1, content_type::BREAK, false, b"hi");
        assert!(!it2.next(&l));
        assert_eq!(it2.content().r#type, content_type::END);
    }

    #[test]
    fn child_iterator_walk() {
        let mut l = make_layout();
        let (root, t2) = make_tree(&mut l);
        // root a 2 enfants (mid, t2) — l'itérateur les énumère
        // en WIDGET_IN_FLOW.
        let mut it = l.w_iterator(root, content_type::ALL, false);
        assert!(it.next(&l));
        assert_eq!(it.content().r#type, content_type::WIDGET_IN_FLOW);
        let mid_seen = it.content().get_widget();
        assert!(it.next(&l));
        let t2_seen = it.content().get_widget();
        assert!(!it.next(&l));
        assert_eq!(it.content().r#type, content_type::END);
        assert_eq!(t2_seen, Some(t2));
        assert_ne!(mid_seen, Some(t2));
        // prev depuis END revient sur t2.
        assert!(it.prev(&l));
        assert_eq!(it.content().get_widget(), Some(t2));
    }

    #[test]
    fn deep_iterator_skips_widgets() {
        let mut l = make_layout();
        let (root, t2) = make_tree(&mut l);
        let mask = Content::mask_for_selection(false);
        let it = l.w_iterator(root, mask, false);
        let mut d = DeepIterator::new(&l, it);
        // Verbatim : l'itérateur frais démarre à START
        // (`has_contents` vrai — la correction ne s'applique
        // que si le contenu initial est un widget).
        assert!(!d.is_empty());
        assert_eq!(d.content().r#type, content_type::START);
        // 1er next : descente root→mid→t1 → TEXT "ab".
        assert!(d.next(&l));
        assert_eq!(d.content().r#type, content_type::TEXT);
        assert_eq!(content_text(d.content()), b"ab");
        assert!(d.next(&l));
        assert_eq!(content_text(d.content()), b"cd");
        assert!(!d.next(&l));
        assert_eq!(d.content().r#type, content_type::END);
        // prev depuis END revient sur "cd".
        assert!(d.prev(&l));
        assert_eq!(content_text(d.content()), b"cd");
        // top pointe sur la feuille.
        assert_eq!(
            d.top_iterator().map(|it| DwIterator::widget(&*it)),
            Some(t2)
        );
    }

    #[test]
    fn deep_iterator_empty_tree() {
        let l = make_layout();
        let leaf = l.add_widget_node(WidgetKind::Base);
        let it = l.w_iterator(leaf, content_type::ALL, false);
        let mut d = DeepIterator::new(&l, it);
        // `is_empty` n'est vrai que si le contenu initial est
        // un widget sans voisin — une feuille START garde
        // `has_contents` (verbatim) ; `next` échoue direct.
        assert!(!d.is_empty());
        assert!(!d.next(&l));
        assert_eq!(d.content().r#type, content_type::END);
    }

    #[test]
    fn char_iterator_reads_words_across_widgets() {
        let mut l = make_layout();
        let (root, _t2) = make_tree(&mut l);
        let mut ci = CharIterator::new(&l, root, false);
        let mut text = Vec::new();
        while ci.next(&l) {
            text.push(u8::try_from(ci.get_char()).unwrap_or(b'?'));
        }
        // "ab" + "cd" — pas d'espace (content.space = false).
        assert_eq!(text, b"abcd");
        assert_eq!(ci.get_char(), CHAR_END);
        assert!(ci.prev(&l));
        assert_eq!(ci.get_char(), i32::from(b'd'));
        assert!(ci.prev(&l));
        assert_eq!(ci.get_char(), i32::from(b'c'));
        assert!(ci.prev(&l));
        assert_eq!(ci.get_char(), i32::from(b'b'));
        assert!(ci.prev(&l));
        assert_eq!(ci.get_char(), i32::from(b'a'));
        assert!(!ci.prev(&l));
        assert_eq!(ci.get_char(), CHAR_START);
    }

    #[test]
    fn char_iterator_space_flag() {
        // Chemin `content.space` via itérateur scripté
        // (TextIterator ne pose jamais space).
        struct Scripted {
            w: WidgetId,
            idx: usize,
            c: Content,
        }
        impl DwIterator for Scripted {
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn widget(&self) -> WidgetId {
                self.w
            }
            fn mask(&self) -> i16 {
                content_type::ALL
            }
            fn content(&self) -> &Content {
                &self.c
            }
            fn set_content(&mut self, c: Content) {
                self.c = c;
            }
            fn clone_box(&self) -> Box<dyn DwIterator> {
                Box::new(Scripted {
                    w: self.w,
                    idx: self.idx,
                    c: self.c.clone(),
                })
            }
            fn compare(&self, _o: &dyn DwIterator) -> i32 {
                0
            }
            fn next(&mut self, _l: &Layout) -> bool {
                match self.idx {
                    0 => {
                        self.c = Content {
                            r#type: content_type::TEXT,
                            space: true, // ≙ espace inter-mots.
                            data: ContentData::Text(b"xy".to_vec()),
                        };
                        self.idx = 1;
                        true
                    }
                    1 => {
                        self.c = Content {
                            r#type: content_type::TEXT,
                            space: false,
                            data: ContentData::Text(b"z".to_vec()),
                        };
                        self.idx = 2;
                        true
                    }
                    _ => {
                        self.c.r#type = content_type::END;
                        false
                    }
                }
            }
            fn prev(&mut self, _l: &Layout) -> bool {
                false
            }
        }
        let l = make_layout();
        let it: Box<dyn DwIterator> = Box::new(Scripted {
            w: 1,
            idx: 0,
            c: Content {
                r#type: content_type::START,
                space: false,
                data: ContentData::None,
            },
        });
        let mut ci = CharIterator {
            it: DeepIterator {
                stack: vec![it],
                mask: content_type::ALL,
                content: Content {
                    r#type: content_type::START,
                    space: false,
                    data: ContentData::None,
                },
                has_contents: true,
            },
            pos: 0,
            ch: CHAR_START,
        };
        let mut text = Vec::new();
        while ci.next(&l) {
            text.push(u8::try_from(ci.get_char()).unwrap_or(b'?'));
        }
        // "xy" + ' ' (space flag) + "z".
        assert_eq!(text, b"xy z");
    }

    #[test]
    fn deep_iterator_compare_orders_positions() {
        let mut l = make_layout();
        let (root, _t2) = make_tree(&mut l);
        let mask = Content::mask_for_selection(false);
        let a = DeepIterator::new(&l, l.w_iterator(root, mask, false));
        let mut b = DeepIterator::new(&l, l.w_iterator(root, mask, false));
        assert_eq!(a.compare_to(&b), 0);
        assert!(b.next(&l));
        assert!(a.compare_to(&b) < 0);
        assert!(b.compare_to(&a) > 0);
    }
}
