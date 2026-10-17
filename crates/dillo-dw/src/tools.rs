//! Port de `dw/tools.hh` et `dw/tools.cc`.
//!
//! `SizeParams` — arguments passés à `Widget::sizeRequest` /
//! `getExtremes` pour les widgets positionnés (références
//! `Widget*` → `WidgetId`).

use crate::WidgetId;

/// `dw::core::SizeParams` — positions relatives d'un ensemble de
/// widgets de référence pendant `sizeRequest`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SizeParams {
    /// `numPos` implicite — longueur des trois vecteurs.
    num_pos: usize,
    /// `references`.
    references: Vec<WidgetId>,
    /// `x`.
    x: Vec<i32>,
    /// `y`.
    y: Vec<i32>,
}

impl SizeParams {
    /// `SizeParams ()`.
    #[must_use]
    pub fn new() -> Self {
        SizeParams::default()
    }

    /// `SizeParams (numPos, references, x, y)`.
    #[must_use]
    pub fn with(references: &[WidgetId], x: &[i32], y: &[i32]) -> Self {
        let mut p = SizeParams::new();
        p.fill(references, x, y);
        p
    }

    /// `fill` — remplace le contenu (`cleanup` + copie).
    pub fn fill(&mut self, references: &[WidgetId], x: &[i32], y: &[i32]) {
        self.num_pos = references.len();
        self.references = references.to_vec();
        self.x = x.to_vec();
        self.y = y.to_vec();
    }

    /// `forChild` — construit les params d'un enfant : pour
    /// chaque référence de `child`, `(xRel,yRel)` si c'est
    /// `parent`, sinon la position de cette référence dans les
    /// params courants + `(xRel,yRel)` (verbatim).
    ///
    /// `child_references` ≙ `child->sizeRequestReference(i)` pour
    /// `i < child->numSizeRequestReferences()` — fourni par
    /// l'appelant (arène).
    #[must_use]
    pub fn for_child(
        &self,
        parent: WidgetId,
        child_references: &[WidgetId],
        x_rel: i32,
        y_rel: i32,
    ) -> SizeParams {
        let mut child_params = SizeParams::new();
        for &child_ref in child_references {
            if child_ref == parent {
                child_params.references.push(child_ref);
                child_params.x.push(x_rel);
                child_params.y.push(y_rel);
            } else {
                let mut found = false;
                for j in 0..self.num_pos {
                    if !found && child_ref == self.references[j] {
                        found = true;
                        child_params.references.push(child_ref);
                        child_params.x.push(self.x[j] + x_rel);
                        child_params.y.push(self.y[j] + y_rel);
                    }
                }
            }
        }
        child_params.num_pos = child_params.references.len();
        child_params
    }

    /// `findReference` — `Some((x, y))` si la référence est
    /// présente.
    #[must_use]
    pub fn find_reference(&self, reference: WidgetId) -> Option<(i32, i32)> {
        for i in 0..self.num_pos {
            if reference == self.references[i] {
                return Some((self.x[i], self.y[i]));
            }
        }
        None
    }

    /// `isEquivalent` — ordre des références indifférent, les
    /// positions doivent correspondre (verbatim).
    #[must_use]
    pub fn is_equivalent(&self, other: &SizeParams) -> bool {
        if self.num_pos != other.num_pos {
            return false;
        }
        for i in 0..self.num_pos {
            let mut other_found = false;
            for j in 0..self.num_pos {
                if !other_found && self.references[i] == other.references[j] {
                    other_found = true;
                    if !(self.x[i] == other.x[j] && self.y[i] == other.y[j]) {
                        return false;
                    }
                }
            }
            if !other_found {
                return false;
            }
        }
        true
    }

    /// `getNumPos`.
    #[must_use]
    pub fn num_pos(&self) -> usize {
        self.num_pos
    }

    /// `getReference`.
    #[must_use]
    pub fn reference(&self, i: usize) -> Option<WidgetId> {
        self.references.get(i).copied()
    }

    /// `getX`.
    #[must_use]
    pub fn get_x(&self, i: usize) -> Option<i32> {
        self.x.get(i).copied()
    }

    /// `getY`.
    #[must_use]
    pub fn get_y(&self, i: usize) -> Option<i32> {
        self.y.get(i).copied()
    }

    /// Tranche `references` (≙ `getReferences()` amont).
    #[must_use]
    pub fn references(&self) -> &[WidgetId] {
        &self.references
    }

    /// Tranche `x` (≙ `getX()` tableau amont).
    #[must_use]
    pub fn xs(&self) -> &[i32] {
        &self.x
    }

    /// Tranche `y` (≙ `getY()` tableau amont).
    #[must_use]
    pub fn ys(&self) -> &[i32] {
        &self.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_child_translates_positions() {
        // refs: w1@(10,20) w2@(30,40)
        let p = SizeParams::with(&[1, 2], &[10, 30], &[20, 40]);
        // child référence w2 et le parent(9) + w3 inconnu
        let c = p.for_child(9, &[2, 9, 3], 100, 200);
        assert_eq!(c.num_pos(), 2);
        // w2: (30+100, 40+200)
        assert_eq!(c.find_reference(2), Some((130, 240)));
        // parent: (100, 200)
        assert_eq!(c.find_reference(9), Some((100, 200)));
        // w3 pas dans les params parent → ignoré
        assert_eq!(c.find_reference(3), None);
    }

    #[test]
    fn is_equivalent_ignores_order() {
        let a = SizeParams::with(&[1, 2], &[10, 30], &[20, 40]);
        let b = SizeParams::with(&[2, 1], &[30, 10], &[40, 20]);
        assert!(a.is_equivalent(&b));
        let c = SizeParams::with(&[2, 1], &[30, 10], &[99, 20]);
        assert!(!a.is_equivalent(&c));
        let d = SizeParams::with(&[1], &[10], &[20]);
        assert!(!a.is_equivalent(&d));
    }
}
