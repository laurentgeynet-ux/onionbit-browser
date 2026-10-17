//! Port de `src/bitvec.c` + `bitvec.h` — ADT de tableaux de bits.
//!
//! Quirk amont préservé : `BVEC_SIZE = sizeof(uchar_t) = 1` — chaque
//! « bit » occupe un **octet** complet et seul le bit 0 est utilisé
//! (`get` retourne `vec[pos] & 1`). L'allocation est `num_bits + 1`
//! octets (`num_bits/BVEC_SIZE + 1`). La fidélité byte-exacte n'est pas
//! critique ici (structure interne), mais on conserve la sémantique
//! exacte des bornes (`dReturn_if_fail` → retour muet / `0`).

/// `bitvec_t` — `len` = nombre de bits demandé (commentaire amont dit
/// « [1 based] » mais la valeur stockée est `num_bits` tel quel).
#[derive(Debug, Clone)]
pub struct Bitvec {
    /// `bvec->vec` — `num_bits + 1` octets (quirk `BVEC_SIZE == 1`).
    vec: Vec<u8>,
    /// `bvec->len`.
    len: usize,
}

impl Bitvec {
    /// `a_Bitvec_new` — alloue `num_bits/BVEC_SIZE + 1` octets à zéro.
    #[must_use]
    pub fn new(num_bits: usize) -> Self {
        Self {
            vec: vec![0; num_bits + 1],
            len: num_bits,
        }
    }

    /// `a_Bitvec_get_bit` — `dReturn_val_if_fail(pos < len, 0)`.
    /// Retourne la valeur masquée (0 ou 1 — le bit 0 seul est utilisé
    /// amont, donc la valeur est toujours 0/1 en pratique).
    #[must_use]
    pub fn get_bit(&self, pos: usize) -> u8 {
        if pos >= self.len {
            return 0;
        }
        // `vec[pos/BVEC_SIZE] & 1 << pos%BVEC_SIZE` avec BVEC_SIZE=1.
        self.vec[pos] & 1
    }

    /// `a_Bitvec_set_bit` — `dReturn_if_fail(pos < len)`.
    pub fn set_bit(&mut self, pos: usize) {
        if pos >= self.len {
            return;
        }
        // `vec[pos/BVEC_SIZE] |= 1 << (pos % BVEC_SIZE)`, BVEC_SIZE=1.
        self.vec[pos] |= 1;
    }

    /// `a_Bitvec_clear_bit` (macro amont — pas de borne-check amont,
    /// mais `set`/`get` en ont ; on borne pareil pour la cohérence).
    pub fn clear_bit(&mut self, pos: usize) {
        if pos >= self.len {
            return;
        }
        self.vec[pos] &= !1;
    }

    /// `a_Bitvec_clear` — `memset(vec, 0, len/BVEC_SIZE + 1)`.
    pub fn clear(&mut self) {
        self.vec.fill(0);
    }

    /// Nombre de bits demandé à la création (`bvec->len`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `len == 0`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_lifecycle() {
        let mut b = Bitvec::new(4);
        for i in 0..4 {
            assert_eq!(b.get_bit(i), 0);
        }
        b.set_bit(1);
        b.set_bit(3);
        assert_eq!(b.get_bit(0), 0);
        assert_eq!(b.get_bit(1), 1);
        assert_eq!(b.get_bit(2), 0);
        assert_eq!(b.get_bit(3), 1);
        b.clear_bit(1);
        assert_eq!(b.get_bit(1), 0);
        b.clear();
        assert_eq!(b.get_bit(3), 0);
    }

    #[test]
    fn out_of_bounds_is_silent() {
        // `dReturn_if_fail` amont : get→0, set→muet, pas de panic.
        let mut b = Bitvec::new(2);
        assert_eq!(b.get_bit(2), 0);
        assert_eq!(b.get_bit(100), 0);
        b.set_bit(2);
        b.clear_bit(100);
        assert_eq!(b.get_bit(0), 0);
    }
}
