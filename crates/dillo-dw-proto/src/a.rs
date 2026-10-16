//! Option A — arène + `WidgetId`, widgets en `Box<dyn WidgetObj>`.
//!
//! Même arène plate que D, mais le polymorphisme passe par un trait
//! objet plutôt qu'un `enum`. Deux frictions démontrées :
//!
//! 1. **Take/replace obligatoire** : `ctx.size_request(id)` doit
//!    sortir le `Box<dyn>` du nœud (placeholder `Null`) pour pouvoir
//!    repasser `&mut ctx` à la méthode — sinon double emprunt
//!    `ctx.nodes[id].widget.size_request(&mut ctx)`.
//! 2. **Downcast pour la config** : `arm_trigger` doit récupérer
//!    `Textblock` via `Any` — le C++ `static_cast` refait surface.
//!
//! Le reste (liens en `WidgetId`, files d'événements drainées) est
//! identique à D — c'est le trait objet qui coûte.

use std::any::Any;
use std::collections::VecDeque;

/// Index dans l'arène (cf. `d.rs`).
pub type WidgetId = usize;

/// Événements drainés après mutation (cf. `d.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// `extremesChanged` remonté par `markSizeChange`.
    SizeHintChanged(WidgetId),
    /// `Layout::queueResize(this)`.
    ResizeQueued(WidgetId),
}

/// `dw::core::Widget` en trait objet — chaque implémentation reçoit
/// `ctx` + son propre id (le `this` C++ est dans le ctx, pas dans
/// la méthode).
pub trait WidgetObj {
    /// `sizeRequest(Size*)` simplifié → `(w, h)`.
    fn size_request(&mut self, ctx: &mut LayoutCtx, id: WidgetId) -> (i32, i32);
    /// Downcast pour la configuration (le `static_cast` du port).
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Placeholder pendant le take/replace (friction 1).
struct Null;
impl WidgetObj for Null {
    fn size_request(&mut self, _ctx: &mut LayoutCtx, _id: WidgetId) -> (i32, i32) {
        (0, 0)
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// `dw::core::Widget` conteneur.
pub struct Container {
    /// Enfants ordonnés.
    pub children: Vec<WidgetId>,
}
impl WidgetObj for Container {
    fn size_request(&mut self, ctx: &mut LayoutCtx, _id: WidgetId) -> (i32, i32) {
        let mut size = (0, 0);
        for c in &self.children {
            let (cw, ch) = ctx.size_request(*c);
            size.0 = size.0.max(cw);
            size.1 += ch;
        }
        size
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// `dw::Textblock` simplifié.
pub struct Textblock {
    /// Largeurs de mots.
    pub words: Vec<i32>,
    /// Hauteur de ligne.
    pub line_height: i32,
    /// Trigger interne armé (`queue_resize` frère pendant le request).
    pub armed: bool,
    /// Cible du trigger.
    pub trigger: Option<WidgetId>,
}
impl WidgetObj for Textblock {
    fn size_request(&mut self, ctx: &mut LayoutCtx, _id: WidgetId) -> (i32, i32) {
        let req = (
            self.words.iter().copied().max().unwrap_or(0),
            i32::try_from(self.words.len()).unwrap_or(i32::MAX) * self.line_height,
        );
        if self.armed {
            self.armed = false;
            if let Some(t) = self.trigger {
                ctx.queue_resize(t);
            }
        }
        req
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// `dw::Image` — taille fixe.
pub struct Image {
    /// Largeur fixe.
    pub w: i32,
    /// Hauteur fixe.
    pub h: i32,
}
impl WidgetObj for Image {
    fn size_request(&mut self, _ctx: &mut LayoutCtx, _id: WidgetId) -> (i32, i32) {
        (self.w, self.h)
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

struct Node {
    parent: Option<WidgetId>,
    quasi_parent: Option<WidgetId>,
    generator: Option<WidgetId>,
    dirty: bool,
    extremes_dirty: bool,
    req: (i32, i32),
    widget: Box<dyn WidgetObj>,
}

/// Contexte central (même rôle qu'en D).
#[derive(Default)]
pub struct LayoutCtx {
    nodes: Vec<Node>,
    resize_queue: VecDeque<WidgetId>,
    events: Vec<Event>,
}

impl LayoutCtx {
    /// `Layout()` vide.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            resize_queue: VecDeque::new(),
            events: Vec::new(),
        }
    }

    /// Crée un nœud.
    pub fn add(&mut self, widget: Box<dyn WidgetObj>) -> WidgetId {
        self.nodes.push(Node {
            parent: None,
            quasi_parent: None,
            generator: None,
            dirty: false,
            extremes_dirty: false,
            req: (0, 0),
            widget,
        });
        self.nodes.len() - 1
    }

    /// `setParent` + rattachement aux `children` du conteneur —
    /// passe par le downcast (friction 2).
    pub fn set_parent(&mut self, child: WidgetId, parent: WidgetId) {
        self.nodes[child].parent = Some(parent);
        if let Some(c) = self.nodes[parent]
            .widget
            .as_any_mut()
            .downcast_mut::<Container>()
        {
            c.children.push(child);
        }
    }

    /// `generator` (cross-link).
    pub fn set_generator(&mut self, w: WidgetId, gen: WidgetId) {
        self.nodes[w].generator = Some(gen);
    }

    /// `quasiParent`.
    pub fn set_quasi_parent(&mut self, w: WidgetId, qp: WidgetId) {
        self.nodes[w].quasi_parent = Some(qp);
    }

    /// `getGenerator()` — fallback `parent` (widget.hh:600).
    #[must_use]
    pub fn get_generator(&self, w: WidgetId) -> Option<WidgetId> {
        self.nodes[w].generator.or(self.nodes[w].parent)
    }

    /// `getParent()`.
    #[must_use]
    pub fn parent(&self, w: WidgetId) -> Option<WidgetId> {
        self.nodes[w].parent
    }

    /// `getQuasiParent()` — fallback `parent` comme amont.
    #[must_use]
    pub fn get_quasi_parent(&self, w: WidgetId) -> Option<WidgetId> {
        self.nodes[w].quasi_parent.or(self.nodes[w].parent)
    }

    /// Flag `resizePending`.
    #[must_use]
    pub fn is_dirty(&self, w: WidgetId) -> bool {
        self.nodes[w].dirty
    }

    /// Flag `extremesChanged`.
    #[must_use]
    pub fn extremes_dirty(&self, w: WidgetId) -> bool {
        self.nodes[w].extremes_dirty
    }

    /// `Widget::queueResize()` — identique à D.
    pub fn queue_resize(&mut self, w: WidgetId) {
        self.nodes[w].dirty = true;
        self.resize_queue.push_back(w);
        self.events.push(Event::ResizeQueued(w));
        self.mark_size_change(w);
    }

    /// `Widget::markSizeChange()` — identique à D.
    pub fn mark_size_change(&mut self, w: WidgetId) {
        let mut cur = self.nodes[w].parent;
        while let Some(p) = cur {
            self.nodes[p].extremes_dirty = true;
            self.events.push(Event::SizeHintChanged(p));
            cur = self.nodes[p].parent;
        }
    }

    /// `sizeRequest` — take/replace du `Box<dyn>` (friction 1) puis
    /// dispatch. Le `Null` placeholder est le coût de l'option A.
    pub fn size_request(&mut self, w: WidgetId) -> (i32, i32) {
        let mut obj = std::mem::replace(&mut self.nodes[w].widget, Box::new(Null));
        let req = obj.size_request(self, w);
        self.nodes[w].widget = obj;
        self.nodes[w].req = req;
        req
    }

    /// Layout incrémental (identique à D).
    pub fn relayout(&mut self, root: WidgetId) -> (i32, i32) {
        let r = self.size_request(root);
        while let Some(w) = self.resize_queue.pop_front() {
            if self.nodes[w].dirty {
                self.nodes[w].dirty = false;
                self.size_request(w);
            }
        }
        r
    }

    /// `arm_trigger` via downcast (friction 2 bis).
    pub fn arm_trigger(&mut self, w: WidgetId, target: WidgetId) {
        if let Some(t) = self.nodes[w]
            .widget
            .as_any_mut()
            .downcast_mut::<Textblock>()
        {
            t.armed = true;
            t.trigger = Some(target);
        }
    }

    /// Draine les événements (cf. `d.rs`).
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build() -> (LayoutCtx, WidgetId, WidgetId, WidgetId, WidgetId) {
        let mut ctx = LayoutCtx::new();
        let root = ctx.add(Box::new(Container {
            children: Vec::new(),
        }));
        let t1 = ctx.add(Box::new(Textblock {
            words: vec![30, 40, 50],
            line_height: 10,
            armed: false,
            trigger: None,
        }));
        let img = ctx.add(Box::new(Image { w: 20, h: 15 }));
        let t2 = ctx.add(Box::new(Textblock {
            words: vec![10],
            line_height: 10,
            armed: false,
            trigger: None,
        }));
        ctx.set_parent(t1, root);
        ctx.set_parent(img, root);
        ctx.set_parent(t2, root);
        ctx.set_generator(img, t1);
        ctx.arm_trigger(t1, img);
        (ctx, root, t1, img, t2)
    }

    #[test]
    fn cross_links_and_fallback() {
        let (ctx, root, t1, img, t2) = build();
        assert_eq!(ctx.get_generator(img), Some(t1));
        assert_eq!(ctx.get_generator(t2), Some(root));
        assert_eq!(ctx.parent(img), Some(root));
    }

    #[test]
    fn resize_during_iteration_no_panic() {
        let (mut ctx, root, _t1, img, _t2) = build();
        assert_eq!(ctx.relayout(root), (50, 55));
        assert!(!ctx.is_dirty(img));
        assert!(ctx.extremes_dirty(root));
        let ev = ctx.drain_events();
        assert!(ev.contains(&Event::ResizeQueued(img)));
        assert!(ev.contains(&Event::SizeHintChanged(root)));
    }
}
