//! Port de `src/klist.c` + `klist.h` — ADT clé→donnée.
//!
//! Sémantique amont portée verbatim : clés `i32` émises par un compteur
//! qui démarre à 0 → première clé = 1 ; à wrap `i32` le compteur repart
//! à 1 et `clean` passe à `false` — tant que `!clean` on saute les clés
//! encore en usage (recherche linéaire, quirk amont : une clé conservée
//! après un wrap complet peut être réémissionnée — responsabilité
//! laissée à l'appelant, note du fichier amont).
//!
//! La liste reste triée par clé (recherche dichotomique ≙
//! `dList_find_sorted`).

/// `KlistNode_t` — paire clé/donnée.
struct KlistNode<T> {
    /// `Key` — clé primaire (> 0).
    key: i32,
    /// `Data`.
    data: T,
}

/// `Klist_t` — `Clean` + `Counter` + liste triée.
pub struct Klist<T> {
    /// `Klist->List` (triée par clé — `dList_insert_sorted`).
    list: Vec<KlistNode<T>>,
    /// `Klist->Clean` — aucune clé réémise en vol si `true`.
    clean: bool,
    /// `Klist->Counter` — dernière clé émise.
    counter: i32,
}

impl<T> Default for Klist<T> {
    fn default() -> Self {
        Self {
            list: Vec::with_capacity(32),
            clean: true,
            counter: 0,
        }
    }
}

impl<T> Klist<T> {
    /// `a_Klist_new` — `dList_new(32)`, `Clean = 1`, `Counter = 0`.
    /// (Amont crée paresseusement au premier `insert` ; ici la création
    /// est explicite — comportement observable identique.)
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `a_Klist_get_data` — `NULL` si absent.
    #[must_use]
    pub fn get(&self, key: i32) -> Option<&T> {
        self.list
            .binary_search_by_key(&key, |n| n.key)
            .ok()
            .map(|i| &self.list[i].data)
    }

    /// `a_Klist_get_data` (mut).
    pub fn get_mut(&mut self, key: i32) -> Option<&mut T> {
        self.list
            .binary_search_by_key(&key, |n| n.key)
            .ok()
            .map(|i| &mut self.list[i].data)
    }

    /// `a_Klist_insert` — émet une nouvelle clé (première = 1), wrap à
    /// `INT_MAX` → repart à 1 + `clean = false`, puis saute les clés en
    /// usage tant que `!clean`.
    pub fn insert(&mut self, data: T) -> i32 {
        loop {
            // `++Counter == 0` → wrap : `Counter = 1; Clean = 0`.
            self.counter = self.counter.wrapping_add(1);
            if self.counter == 0 {
                self.counter = 1;
                self.clean = false;
            }
            if self.clean || self.get(self.counter).is_none() {
                break;
            }
        }
        let key = self.counter;
        let pos = self
            .list
            .binary_search_by_key(&key, |n| n.key)
            .unwrap_or_else(|i| i);
        self.list.insert(pos, KlistNode { key, data });
        key
    }

    /// `a_Klist_remove` — retire la donnée ; liste vide → `Clean = 1`.
    pub fn remove(&mut self, key: i32) {
        if let Ok(i) = self.list.binary_search_by_key(&key, |n| n.key) {
            self.list.remove(i);
        }
        if self.list.is_empty() {
            self.clean = true;
        }
    }

    /// `a_Klist_length`.
    #[must_use]
    pub fn length(&self) -> usize {
        self.list.len()
    }

    /// Liste vide ?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove() {
        let mut k = Klist::new();
        let a = k.insert("a");
        let b = k.insert("b");
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(k.get(1), Some(&"a"));
        assert_eq!(k.get(2), Some(&"b"));
        assert_eq!(k.get(3), None);
        k.remove(1);
        assert_eq!(k.get(1), None);
        assert_eq!(k.length(), 1);
        // Les clés restent croissantes après retrait.
        let c = k.insert("c");
        assert_eq!(c, 3);
        assert_eq!(k.get(3), Some(&"c"));
    }

    #[test]
    fn counter_wrap_resets_at_zero() {
        let mut k: Klist<i32> = Klist::new();
        let a = k.insert(10);
        assert_eq!(a, 1);
        // Amont : `++Counter == 0` — le wrap INT_MAX→INT_MIN émet une
        // clé négative (pas de reset) ; le reset n'arrive qu'à 0.
        k.counter = i32::MAX;
        let n1 = k.insert(20);
        assert_eq!(n1, i32::MIN);
        // À 0 : Counter=1, Clean=0 → saute les clés vivantes (1) → 2.
        k.counter = -1;
        let n2 = k.insert(30);
        assert_eq!(n2, 2);
        assert_eq!(k.get(2), Some(&30));
    }

    #[test]
    fn empty_restores_clean() {
        let mut k: Klist<i32> = Klist::new();
        k.insert(1);
        k.insert(2);
        k.counter = -1;
        k.clean = false;
        // Wrap → Counter=1, Clean=0 ; 1 et 2 vivants → saute → 3.
        let n = k.insert(3);
        assert_eq!(n, 3);
        // Tout retirer → Clean revient à 1 → la clé 1 peut repasser.
        k.remove(1);
        k.remove(2);
        k.remove(3);
        k.counter = -1;
        let n2 = k.insert(4);
        assert_eq!(n2, 1);
    }
}
