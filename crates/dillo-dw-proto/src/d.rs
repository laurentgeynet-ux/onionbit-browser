//! Option D — arène + `enum WidgetKind` + `LayoutCtx` (pas de `dyn`).
//!
//! Candidat hybride : tous les nœuds vivent dans un `Vec` plat,
//! tous les liens (`parent`, `quasi_parent`, `generator`,
//! `children`) sont des `WidgetId`. Le C++ `&mut self` se traduit
//! par `&mut LayoutCtx` + id — aucune serrure d'emprunt à contourner :
//! la récursion `size_request` pendant `mark_size_change` marche
//! nativement (les données du nœud courant sont extraites via
//! `mem::take`, ou copiées par champs).
//!
//! Discipline imposée au port dw : ne jamais garder `&mut` sur le
//! `kind` d'un nœud pendant un appel récursif — extraire (take/copy)
//! ce qui est nécessaire, rappeler, réécrire. Les `Signal` lout
//! deviennent `events` drainés après mutation (contrainte ADR).

use std::collections::VecDeque;

/// Index dans l'arène. Le proto ne supprime pas de nœud : pas de
/// génération de slot nécessaire (à revoir pour le vrai `dw/`).
pub type WidgetId = usize;

/// Événements drainés après mutation (remplace `Signal::emit*`
/// synchrones — contrainte ADR-0005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// `Widget::extremesChanged` émis en remontant `markSizeChange`.
    SizeHintChanged(WidgetId),
    /// `Layout::queueResize(this)`.
    ResizeQueued(WidgetId),
}

/// `enum`-dispatch du kind widget — pas de `dyn`, pas de downcast.
#[derive(Debug)]
pub enum WidgetKind {
    /// `dw::core::Widget` conteneur (`children` = enfants de layout).
    Container {
        /// Enfants ordonnés.
        children: Vec<WidgetId>,
    },
    /// `dw::Textblock` (simplifié : mots entiers).
    Textblock {
        /// Largeurs de mots.
        words: Vec<i32>,
        /// Hauteur de ligne.
        line_height: i32,
        /// Simule un trigger interne (image à requérir) : `queue_resize`
        /// sur `trigger` pendant son propre `size_request`, une fois.
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

impl Default for WidgetKind {
    fn default() -> Self {
        Self::Container {
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
struct Node {
    // --- liens croisés de widget.hh:127-146 ---
    parent: Option<WidgetId>,
    quasi_parent: Option<WidgetId>,
    generator: Option<WidgetId>,
    // --- état incrémental ---
    dirty: bool,
    extremes_dirty: bool,
    req: (i32, i32),
    kind: WidgetKind,
}

/// Contexte central : possède l'arène, la file de resize et les
/// événements. Remplace `dw::Layout` + les `Widget*` libres du C++.
#[derive(Debug, Default)]
pub struct LayoutCtx {
    nodes: Vec<Node>,
    resize_queue: VecDeque<WidgetId>,
    events: Vec<Event>,
}

/// Données extraites d'un `Textblock` pour le calcul (pattern
/// take/copy — la règle d'or du modèle D).
enum Snap {
    Container(Vec<WidgetId>),
    Textblock {
        words: Vec<i32>,
        line_height: i32,
        armed: bool,
        trigger: Option<WidgetId>,
    },
    Image {
        w: i32,
        h: i32,
    },
}

impl LayoutCtx {
    /// `Layout()` vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Crée un nœud (sans parent).
    pub fn add(&mut self, kind: WidgetKind) -> WidgetId {
        self.nodes.push(Node {
            kind,
            ..Node::default()
        });
        self.nodes.len() - 1
    }

    /// `setParent` + rattachement aux `children` du conteneur.
    /// (Amont : `Container::addWidget`.)
    pub fn set_parent(&mut self, child: WidgetId, parent: WidgetId) {
        self.nodes[child].parent = Some(parent);
        if let WidgetKind::Container { children } = &mut self.nodes[parent].kind {
            children.push(child);
        }
    }

    /// `generator` (cross-link de contenu, ex. widget de référence).
    pub fn set_generator(&mut self, w: WidgetId, gen: WidgetId) {
        self.nodes[w].generator = Some(gen);
    }

    /// `quasiParent` (floats/OOF — conteneur logique ≠ parent layout).
    pub fn set_quasi_parent(&mut self, w: WidgetId, qp: WidgetId) {
        self.nodes[w].quasi_parent = Some(qp);
    }

    /// `Widget::getGenerator()` — fallback sur `parent` si `generator`
    /// est `None` (widget.hh:600).
    #[must_use]
    pub fn get_generator(&self, w: WidgetId) -> Option<WidgetId> {
        self.nodes[w].generator.or(self.nodes[w].parent)
    }

    /// `Widget::getParent()`.
    #[must_use]
    pub fn parent(&self, w: WidgetId) -> Option<WidgetId> {
        self.nodes[w].parent
    }

    /// `Widget::getQuasiParent()` — fallback sur `parent` comme amont.
    #[must_use]
    pub fn get_quasi_parent(&self, w: WidgetId) -> Option<WidgetId> {
        self.nodes[w].quasi_parent.or(self.nodes[w].parent)
    }

    /// Flag `resizePending` du nœud.
    #[must_use]
    pub fn is_dirty(&self, w: WidgetId) -> bool {
        self.nodes[w].dirty
    }

    /// Flag `extremesChanged` remonté par `markSizeChange`.
    #[must_use]
    pub fn extremes_dirty(&self, w: WidgetId) -> bool {
        self.nodes[w].extremes_dirty
    }

    /// `Widget::queueResize()` : marque sale, enfile, puis propage
    /// `markSizeChange` vers les ancêtres (appel croisé enfant→parent,
    /// légal ici *même en pleine itération* — aucun emprunt tenu).
    pub fn queue_resize(&mut self, w: WidgetId) {
        self.nodes[w].dirty = true;
        self.resize_queue.push_back(w);
        self.events.push(Event::ResizeQueued(w));
        self.mark_size_change(w);
    }

    /// `Widget::markSizeChange()` : `extremesChanged` sur chaque
    /// ancêtre de `parent` (remontée synchrone, comme amont).
    pub fn mark_size_change(&mut self, w: WidgetId) {
        let mut cur = self.nodes[w].parent;
        while let Some(p) = cur {
            self.nodes[p].extremes_dirty = true;
            self.events.push(Event::SizeHintChanged(p));
            cur = self.nodes[p].parent;
        }
    }

    /// `Widget::sizeRequest()` — calcule + cache la réquisition.
    /// Pattern take/copy : on extrait les données nécessaires, on
    /// rappelle récursivement, on réécrit — le `&mut self` C++ devient
    /// un passage de `ctx`, sans verrou d'emprunt.
    pub fn size_request(&mut self, w: WidgetId) -> (i32, i32) {
        let snap = match std::mem::take(&mut self.nodes[w].kind) {
            WidgetKind::Container { children } => Snap::Container(children),
            WidgetKind::Textblock {
                words,
                line_height,
                armed,
                trigger,
            } => Snap::Textblock {
                words,
                line_height,
                armed,
                trigger,
            },
            WidgetKind::Image { w: iw, h: ih } => Snap::Image { w: iw, h: ih },
        };
        let req = match snap {
            Snap::Container(children) => {
                let mut size = (0, 0);
                for c in &children {
                    let (cw, ch) = self.size_request(*c);
                    size.0 = size.0.max(cw);
                    size.1 += ch;
                }
                self.nodes[w].kind = WidgetKind::Container { children };
                size
            }
            Snap::Textblock {
                words,
                line_height,
                armed,
                trigger,
            } => {
                let req = (
                    words.iter().copied().max().unwrap_or(0),
                    i32::try_from(words.len()).unwrap_or(i32::MAX) * line_height,
                );
                self.nodes[w].kind = WidgetKind::Textblock {
                    words,
                    line_height,
                    armed: false,
                    trigger,
                };
                // Trigger mid-request : comme un `queueResize` interne
                // amont — récursion réentrante, libre en modèle D.
                if armed {
                    if let Some(t) = trigger {
                        self.queue_resize(t);
                    }
                }
                req
            }
            Snap::Image { w: iw, h: ih } => {
                self.nodes[w].kind = WidgetKind::Image { w: iw, h: ih };
                (iw, ih)
            }
        };
        self.nodes[w].req = req;
        req
    }

    /// `Layout::layout()` incrémental : requête racine, puis drain de
    /// la file de resize (chaque widget sale est re-mesuré).
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

    /// `setTrigger` sur un `Textblock` (helper de test).
    pub fn arm_trigger(&mut self, w: WidgetId, target: WidgetId) {
        if let WidgetKind::Textblock { armed, trigger, .. } = &mut self.nodes[w].kind {
            *armed = true;
            *trigger = Some(target);
        }
    }

    /// Draine les événements émis pendant la mutation
    /// (le drain *après* coup remplace `Signal::emit`).
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scénario commun (identique à `a`/`b`) :
    /// root = [t1(Textblock, trigger→img), img(gen=t1), t2] ;
    /// t1 déclenche `queue_resize(img)` pendant son `size_request`.
    fn build() -> (LayoutCtx, WidgetId, WidgetId, WidgetId, WidgetId) {
        let mut ctx = LayoutCtx::new();
        let root = ctx.add(WidgetKind::Container {
            children: Vec::new(),
        });
        let t1 = ctx.add(WidgetKind::Textblock {
            words: vec![30, 40, 50],
            line_height: 10,
            armed: false,
            trigger: None,
        });
        let img = ctx.add(WidgetKind::Image { w: 20, h: 15 });
        let t2 = ctx.add(WidgetKind::Textblock {
            words: vec![10],
            line_height: 10,
            armed: false,
            trigger: None,
        });
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
        assert_eq!(ctx.get_generator(t2), Some(root)); // fallback parent
        assert_eq!(ctx.parent(img), Some(root));
    }

    #[test]
    fn resize_during_iteration_no_panic() {
        let (mut ctx, root, _t1, img, _t2) = build();
        // t1:(50,30) img:(20,15) t2:(10,10) → root (50,55)
        assert_eq!(ctx.relayout(root), (50, 55));
        // img a été re-mesuré via la file (sale puis nettoyé)
        assert!(!ctx.is_dirty(img));
        // markSizeChange(img) a marqué l'ancêtre root
        assert!(ctx.extremes_dirty(root));
        // Événements : queueResize(img) puis SizeHintChanged(root)
        let ev = ctx.drain_events();
        assert!(ev.contains(&Event::ResizeQueued(img)));
        assert!(ev.contains(&Event::SizeHintChanged(root)));
    }
}
