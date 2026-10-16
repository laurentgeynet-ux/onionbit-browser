//! Option B — `Rc`/`Weak` mixte : le plus proche structurellement
//! du C++ (chaque widget est un objet, `children` = `Rc` forts,
//! remontées = `Weak`).
//!
//! **Ce que le scénario démontre** : `size_request(root)` tient le
//! `RefMut` de `root` pendant toute l'itération des enfants (comme
//! le C++ tient `this`). Quand `t1` déclenche `queue_resize(img)` →
//! `mark_size_change` → `root.try_borrow_mut()` **échoue** — le
//! `BorrowMutError` annoncé par l'ADR. Le port correct sous B est
//! donc `try_borrow_mut` + **déféré** : la remontée reprend après
//! coup via un registre `id → Weak`.
//!
//! Coûts visibles vs A/D :
//! - tout code amont qui fait `parent->foo()` devient `upgrade` +
//!   `try_borrow` + branche d'échec ;
//! - l'ordre des événements de remontée se ré-ordonne (les marques
//!   déferrées arrivent après les événements émis entre-temps) ;
//! - un registre global id→widget devient nécessaire, plus des ids
//!   synthétiques (`Rc::as_ptr` ne suffit pas pour l'égalité des
//!   événements/tests).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::{Rc, Weak};

/// Id synthétique (le registre `Shared` les alloue).
pub type WidgetId = usize;

/// Événements drainés après mutation (cf. `d.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// `extremesChanged` remonté par `markSizeChange`.
    SizeHintChanged(WidgetId),
    /// `Layout::queueResize(this)`.
    ResizeQueued(WidgetId),
}

/// Kind du widget (champs propres à chaque type).
pub enum Kind {
    /// `dw::core::Widget` conteneur.
    Container,
    /// `dw::Textblock` simplifié.
    Textblock {
        /// Largeurs de mots.
        words: Vec<i32>,
        /// Hauteur de ligne.
        line_height: i32,
        /// Trigger interne armé.
        armed: bool,
        /// Cible du trigger.
        trigger: Option<WidgetId>,
    },
    /// `dw::Image` — taille fixe.
    Image {
        /// Largeur fixe.
        w: i32,
        /// Hauteur fixe.
        h: i32,
    },
}

/// Un widget = objet `RefCell` partagé (analogue au C++).
pub struct Node {
    id: WidgetId,
    parent: Option<Weak<RefCell<Node>>>,
    quasi_parent: Option<Weak<RefCell<Node>>>,
    generator: Option<Weak<RefCell<Node>>>,
    children: Vec<Rc<RefCell<Node>>>,
    dirty: bool,
    extremes_dirty: bool,
    req: (i32, i32),
    kind: Kind,
    ctx: Rc<Shared>,
}

/// État partagé (l'équivalent de `dw::Layout` + les files). En
/// `RefCell` car atteint depuis n'importe quel `borrow_mut` de nœud.
#[derive(Default)]
pub struct Shared {
    next_id: Cell<WidgetId>,
    resize_queue: RefCell<VecDeque<WidgetId>>,
    /// Marques `markSizeChange` repoussées par un conflit d'emprunt
    /// — LA preuve de la friction B. Stocke des `Weak` (un `borrow`
    /// pour lire l'id échouerait aussi — `Rc::as_ptr`/`Weak` sont les
    /// seules identités accessibles sans emprunt).
    deferred_marks: RefCell<Vec<Weak<RefCell<Node>>>>,
    /// Nombre de conflits `try_borrow` (instrumentation du proto).
    n_conflicts: Cell<usize>,
    events: RefCell<Vec<Event>>,
    registry: RefCell<HashMap<WidgetId, Weak<RefCell<Node>>>>,
}

impl Shared {
    /// `Layout()` vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Compteur d'instrumentation : conflits `try_borrow` évités.
    #[must_use]
    pub fn borrow_conflicts(&self) -> usize {
        self.n_conflicts.get()
    }
}

fn id_of(n: &Rc<RefCell<Node>>) -> WidgetId {
    n.borrow().id
}

/// `add(kind)` — crée et enregistre un widget.
pub fn add(ctx: &Rc<Shared>, kind: Kind) -> Rc<RefCell<Node>> {
    let id = ctx.next_id.get();
    ctx.next_id.set(id + 1);
    let n = Rc::new(RefCell::new(Node {
        id,
        parent: None,
        quasi_parent: None,
        generator: None,
        children: Vec::new(),
        dirty: false,
        extremes_dirty: false,
        req: (0, 0),
        kind,
        ctx: Rc::clone(ctx),
    }));
    ctx.registry.borrow_mut().insert(id, Rc::downgrade(&n));
    n
}

/// `setParent` + rattachement aux `children` du conteneur.
pub fn set_parent(child: &Rc<RefCell<Node>>, parent: &Rc<RefCell<Node>>) {
    child.borrow_mut().parent = Some(Rc::downgrade(parent));
    parent.borrow_mut().children.push(Rc::clone(child));
}

/// `generator` (cross-link).
pub fn set_generator(w: &Rc<RefCell<Node>>, gen: &Rc<RefCell<Node>>) {
    w.borrow_mut().generator = Some(Rc::downgrade(gen));
}

/// `quasiParent`.
pub fn set_quasi_parent(w: &Rc<RefCell<Node>>, qp: &Rc<RefCell<Node>>) {
    w.borrow_mut().quasi_parent = Some(Rc::downgrade(qp));
}

/// `getGenerator()` — fallback `parent` (widget.hh:600).
#[must_use]
pub fn get_generator(w: &Rc<RefCell<Node>>) -> Option<Rc<RefCell<Node>>> {
    let b = w.borrow();
    b.generator
        .as_ref()
        .and_then(Weak::upgrade)
        .or_else(|| b.parent.as_ref().and_then(Weak::upgrade))
}

/// `getParent()`.
#[must_use]
pub fn parent(w: &Rc<RefCell<Node>>) -> Option<Rc<RefCell<Node>>> {
    w.borrow().parent.as_ref().and_then(Weak::upgrade)
}

/// `getQuasiParent()` — fallback `parent` comme amont.
#[must_use]
pub fn get_quasi_parent(w: &Rc<RefCell<Node>>) -> Option<Rc<RefCell<Node>>> {
    let b = w.borrow();
    b.quasi_parent
        .as_ref()
        .and_then(Weak::upgrade)
        .or_else(|| b.parent.as_ref().and_then(Weak::upgrade))
}

/// Flag `resizePending`.
#[must_use]
pub fn is_dirty(w: &Rc<RefCell<Node>>) -> bool {
    w.borrow().dirty
}

/// Flag `extremesChanged`.
#[must_use]
pub fn extremes_dirty(w: &Rc<RefCell<Node>>) -> bool {
    w.borrow().extremes_dirty
}

/// `Widget::queueResize()` — marque, enfile, propage. Toutes les
/// remontées passent par `try_borrow_mut` : sur conflit, la marque
/// est **déférée** (sinon `BorrowMutError` = panique interdite).
pub fn queue_resize(w: &Rc<RefCell<Node>>) {
    let ctx = {
        let mut b = w.borrow_mut();
        b.dirty = true;
        Rc::clone(&b.ctx)
    };
    let id = id_of(w);
    ctx.resize_queue.borrow_mut().push_back(id);
    ctx.events.borrow_mut().push(Event::ResizeQueued(id));
    mark_size_change(w);
}

/// `Widget::markSizeChange()` — remonte `parent` en `try_borrow_mut`.
/// Un ancêtre en cours d'itération (`RefMut` tenu) renvoie `Err` →
/// l'id est poussé dans `deferred_marks`, à reprendre à la sortie
/// de la récursion (cf. `drain_deferred`).
pub fn mark_size_change(w: &Rc<RefCell<Node>>) {
    let ctx = Rc::clone(&w.borrow().ctx);
    let mut cur = parent(w);
    while let Some(p) = cur {
        if let Ok(mut pb) = p.try_borrow_mut() {
            pb.extremes_dirty = true;
            ctx.events.borrow_mut().push(Event::SizeHintChanged(pb.id));
            cur = pb.parent.as_ref().and_then(Weak::upgrade);
        } else {
            ctx.n_conflicts.set(ctx.n_conflicts.get() + 1);
            ctx.deferred_marks.borrow_mut().push(Rc::downgrade(&p));
            break;
        }
    }
}

/// Reprend les marques déférées — à appeler quand plus aucun
/// `RefMut` d'ancêtre n'est tenu (après la passe de mesure).
pub fn drain_deferred(ctx: &Rc<Shared>) {
    loop {
        let next = ctx.deferred_marks.borrow_mut().pop();
        let Some(w) = next else { break };
        let Some(n) = w.upgrade() else {
            continue;
        };
        // Reprend la remontée depuis ce nœud (désormais libre).
        let mut cur = Some(n);
        while let Some(p) = cur {
            let mut pb = p.borrow_mut();
            pb.extremes_dirty = true;
            ctx.events.borrow_mut().push(Event::SizeHintChanged(pb.id));
            cur = pb.parent.as_ref().and_then(Weak::upgrade);
        }
    }
}

/// `arm_trigger` sur un `Textblock`.
pub fn arm_trigger(w: &Rc<RefCell<Node>>, target: &Rc<RefCell<Node>>) {
    let mut b = w.borrow_mut();
    if let Kind::Textblock { armed, trigger, .. } = &mut b.kind {
        *armed = true;
        *trigger = Some(id_of(target));
    }
}

/// `Widget::sizeRequest()` — le `RefMut` de `self` est tenu pendant
/// toute la récursion des enfants, comme `this` en C++ : c'est ce
/// qui *provoque* les conflits remontés par `mark_size_change`.
pub fn size_request(w: &Rc<RefCell<Node>>) -> (i32, i32) {
    let mut b = w.borrow_mut();
    let req = match &mut b.kind {
        Kind::Container => {
            let mut size = (0, 0);
            for c in &b.children {
                let (cw, ch) = size_request(c);
                size.0 = size.0.max(cw);
                size.1 += ch;
            }
            size
        }
        Kind::Textblock {
            words,
            line_height,
            armed,
            trigger,
        } => {
            let req = (
                words.iter().copied().max().unwrap_or(0),
                i32::try_from(words.len()).unwrap_or(i32::MAX) * *line_height,
            );
            if *armed {
                *armed = false;
                if let Some(t) = *trigger {
                    let ctx = Rc::clone(&b.ctx);
                    let target = ctx.registry.borrow().get(&t).and_then(Weak::upgrade);
                    if let Some(target) = target {
                        // Appel croisé pendant que `b` (self) et le
                        // parent (plus haut) sont empruntés — marchait
                        // en C++, passe ici via try_borrow+defer.
                        queue_resize(&target);
                    }
                }
            }
            req
        }
        Kind::Image { w: iw, h: ih } => (*iw, *ih),
    };
    b.req = req;
    req
}

/// `Layout::layout()` incrémental : requête racine → drain des
/// marques déférées → drain de la file de resize.
pub fn relayout(ctx: &Rc<Shared>, root: &Rc<RefCell<Node>>) -> (i32, i32) {
    let r = size_request(root);
    drain_deferred(ctx);
    while let Some(id) = ctx.resize_queue.borrow_mut().pop_front() {
        if let Some(w) = ctx.registry.borrow().get(&id).and_then(Weak::upgrade) {
            let mut b = w.borrow_mut();
            if b.dirty {
                b.dirty = false;
                drop(b);
                size_request(&w);
            }
        }
    }
    r
}

/// Draine les événements (cf. `d.rs`).
pub fn drain_events(ctx: &Rc<Shared>) -> Vec<Event> {
    std::mem::take(&mut *ctx.events.borrow_mut())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Tree = (
        Rc<Shared>,
        Rc<RefCell<Node>>,
        Rc<RefCell<Node>>,
        Rc<RefCell<Node>>,
        Rc<RefCell<Node>>,
    );

    fn build() -> Tree {
        let ctx = Rc::new(Shared::new());
        let root = add(&ctx, Kind::Container);
        let t1 = add(
            &ctx,
            Kind::Textblock {
                words: vec![30, 40, 50],
                line_height: 10,
                armed: false,
                trigger: None,
            },
        );
        let img = add(&ctx, Kind::Image { w: 20, h: 15 });
        let t2 = add(
            &ctx,
            Kind::Textblock {
                words: vec![10],
                line_height: 10,
                armed: false,
                trigger: None,
            },
        );
        set_parent(&t1, &root);
        set_parent(&img, &root);
        set_parent(&t2, &root);
        set_generator(&img, &t1);
        arm_trigger(&t1, &img);
        (ctx, root, t1, img, t2)
    }

    #[test]
    fn cross_links_and_fallback() {
        let (_ctx, root, t1, img, t2) = build();
        assert_eq!(id_of(&get_generator(&img).unwrap()), id_of(&t1));
        assert_eq!(id_of(&get_generator(&t2).unwrap()), id_of(&root));
        assert_eq!(id_of(&parent(&img).unwrap()), id_of(&root));
    }

    #[test]
    fn resize_during_iteration_no_panic() {
        let (ctx, root, _t1, img, _t2) = build();
        assert_eq!(relayout(&ctx, &root), (50, 55));
        assert!(!is_dirty(&img));
        assert!(extremes_dirty(&root));
        // La preuve B : root était emprunté pendant la remontée de
        // img → au moins 1 conflit évité par try_borrow+defer.
        assert!(ctx.borrow_conflicts() >= 1);
        let ev = drain_events(&ctx);
        assert!(ev.contains(&Event::ResizeQueued(id_of(&img))));
        assert!(ev.contains(&Event::SizeHintChanged(id_of(&root))));
    }
}
