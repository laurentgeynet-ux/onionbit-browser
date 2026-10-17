//! `doctree.hh` — arbre document pour le matching des sélecteurs CSS.
//!
//! Amont : `DoctreeNode` chaîné par pointeurs (`parent`/`sibling`/`lastChild`),
//! tous les nœuds libérés récursivement à la destruction du `Doctree`
//! (`~DoctreeNode` parcourt la chaîne `lastChild`/`sibling`). Les nœuds ne
//! sont jamais libérés individuellement → arène `Vec<DoctreeNode>` : la
//! sémantique de durée de vie est identique, les indices remplacent les
//! pointeurs.
//!
//! Invariants verbatim :
//! - `push` insère en tête de la liste des enfants du parent (`sibling`
//!   = ancien `lastChild`), `num` croissant strictement.
//! - `top()` retourne `None` quand `topNode == rootNode`.
//! - `parent(node)` retourne `None` quand `node.parent == rootNode` — la
//!   racine n'est jamais un ancêtre matcheable.
//! - `pop` remonte `top` sans supprimer le nœud.

/// `DoctreeNode` — verbatim (champs publiques amont).
#[derive(Debug, Clone)]
pub struct DoctreeNode {
    /// `parent` — indice du parent dans l'arène (`ROOT` pour les fils
    /// directs de la racine).
    pub parent: usize,
    /// `sibling` — frère précédent (insertion en tête).
    pub sibling: Option<usize>,
    /// `lastChild` — dernier enfant inséré (tête de la chaîne `sibling`).
    pub last_child: Option<usize>,
    /// `num` — identifiant unique croissant (utilisé par le `MatchCache`).
    pub num: i32,
    /// `element` — index du tag (`a_Html_tag_index`, -1 si inconnu).
    pub element: i32,
    /// `klass` — liste des classes (`class="a b"` → `splitStr`).
    pub klass: Option<Vec<Vec<u8>>>,
    /// `pseudo` — pseudo-classe (`"link"`, `"visited"`, sinon `None`).
    pub pseudo: Option<Vec<u8>>,
    /// `id`.
    pub id: Option<Vec<u8>>,
}

impl DoctreeNode {
    fn new() -> Self {
        DoctreeNode {
            parent: 0,
            sibling: None,
            last_child: None,
            num: 0,
            element: 0,
            klass: None,
            pseudo: None,
            id: None,
        }
    }
}

/// `Doctree` — interface de l'arbre pour le matching des sélecteurs.
#[derive(Debug)]
pub struct Doctree {
    nodes: Vec<DoctreeNode>,
    top_node: usize,
    root_node: usize,
    num: i32,
}

/// Indice de nœud dans l'arène du [`Doctree`].
pub type NodeRef = usize;

impl Default for Doctree {
    fn default() -> Self {
        Self::new()
    }
}

impl Doctree {
    /// Ctor — crée le nœud racine (`element=0`, `num` non utilisé amont).
    #[must_use]
    pub fn new() -> Self {
        Doctree {
            nodes: vec![DoctreeNode::new()],
            top_node: 0,
            root_node: 0,
            num: 0,
        }
    }

    /// `push` — insère en tête des enfants du sommet et devient le sommet.
    pub fn push(&mut self) -> NodeRef {
        let idx = self.nodes.len();
        let mut dn = DoctreeNode::new();
        dn.parent = self.top_node;
        dn.sibling = self.nodes[self.top_node].last_child;
        dn.num = self.num;
        self.num += 1;
        self.nodes[self.top_node].last_child = Some(idx);
        self.nodes.push(dn);
        self.top_node = idx;
        idx
    }

    /// `pop` — remonte au parent (`assert top != root` amont).
    pub fn pop(&mut self) {
        debug_assert_ne!(self.top_node, self.root_node);
        self.top_node = self.nodes[self.top_node].parent;
    }

    /// `top` — `None` quand on est sur la racine (verbatim).
    #[must_use]
    pub fn top(&self) -> Option<NodeRef> {
        if self.top_node == self.root_node {
            None
        } else {
            Some(self.top_node)
        }
    }

    /// `parent` — `None` quand le parent est la racine (verbatim).
    #[must_use]
    pub fn parent(&self, node: NodeRef) -> Option<NodeRef> {
        if self.nodes[node].parent == self.root_node {
            None
        } else {
            Some(self.nodes[node].parent)
        }
    }

    /// `sibling` — verbatim.
    #[must_use]
    pub fn sibling(&self, node: NodeRef) -> Option<NodeRef> {
        self.nodes[node].sibling
    }

    /// Accès lecture au nœud (équivaut au déréférencement amont).
    #[must_use]
    pub fn node(&self, node: NodeRef) -> &DoctreeNode {
        &self.nodes[node]
    }

    /// Accès mutable (le parseur écrit `element`/`id`/`klass`/`pseudo`).
    pub fn node_mut(&mut self, node: NodeRef) -> &mut DoctreeNode {
        &mut self.nodes[node]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_top_pop() {
        let mut dt = Doctree::new();
        assert!(dt.top().is_none());
        let a = dt.push();
        assert_eq!(dt.top(), Some(a));
        let b = dt.push();
        assert_eq!(dt.top(), Some(b));
        assert_eq!(dt.parent(b), Some(a));
        dt.pop();
        assert_eq!(dt.top(), Some(a));
        dt.pop();
        assert!(dt.top().is_none());
    }

    #[test]
    fn sibling_chain_is_head_inserted() {
        let mut dt = Doctree::new();
        let p = dt.push();
        let c1 = dt.push();
        dt.pop();
        let c2 = dt.push();
        dt.pop();
        let c3 = dt.push();
        assert_eq!(dt.sibling(c3), Some(c2));
        assert_eq!(dt.sibling(c2), Some(c1));
        assert_eq!(dt.sibling(c1), None);
        assert_eq!(dt.node(p).last_child, Some(c3));
    }

    #[test]
    fn root_parent_is_none() {
        let mut dt = Doctree::new();
        let a = dt.push();
        assert_eq!(dt.parent(a), None);
        let b = dt.push();
        assert_eq!(dt.parent(b), Some(a));
    }

    #[test]
    fn num_is_strictly_increasing() {
        let mut dt = Doctree::new();
        let a = dt.push();
        let b = dt.push();
        dt.pop();
        let c = dt.push();
        assert!(dt.node(a).num < dt.node(b).num);
        assert!(dt.node(b).num < dt.node(c).num);
    }
}
