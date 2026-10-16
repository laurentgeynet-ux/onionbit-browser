//! Port of `lout/misc.hh` + `lout/misc.cc`.
//!
//! Helpers divers : `SimpleVector<T>`, `NotSoSimpleVector<T>`
//! (optimisation insertions en début — algorithme amont porté
//! fidèlement), `StringBuffer`, `BitSet`, helpers ASCII.
//!
//! Non portés : `ZoneAllocator` (arène C strings — les usages
//! deviendront des collections propres), `init`/`prgName`,
//! `assertNotReached`/`notImplemented` (`unreachable!`/`tracing`).

use crate::dlib;

/// `misc::roundInt` : arrondi « away from zero » à la C — la
/// troncature `as i32` reproduit le cast `(int)` amont (sémantique
/// voulue, d'où l'`allow`).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn round_int(d: f64) -> i32 {
    if d > 0.0 {
        (d + 0.5) as i32
    } else {
        (d - 0.5) as i32
    }
}

/// `misc::AsciiTolower`.
#[must_use]
pub fn ascii_tolower(c: u8) -> i32 {
    i32::from(dlib::ascii_to_lower(c))
}

/// `misc::AsciiToupper`.
#[must_use]
pub fn ascii_toupper(c: u8) -> i32 {
    i32::from(dlib::ascii_to_upper(c))
}

/// `misc::AsciiStrcasecmp`.
#[must_use]
pub fn ascii_strcasecmp(s1: &[u8], s2: &[u8]) -> i32 {
    dlib::str_ascii_casecmp(s1, s2)
}

/// `misc::boolToStr`.
#[must_use]
pub fn bool_to_str(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

// ---------------------------------------------------------------------------
// SimpleVector
// ---------------------------------------------------------------------------

/// `misc::SimpleVector<T>` — vecteur avec `set()` explicite après
/// `increase`/`set_size` (amont n'initialise pas les nouveaux slots ;
/// nous initialisons à `T::default()` — strictement plus sûr,
/// comportement observable identique).
#[derive(Debug, Default, Clone)]
pub struct SimpleVector<T> {
    array: Vec<T>,
}

impl<T: Clone + Default> SimpleVector<T> {
    /// `SimpleVector(initAlloc)`.
    #[must_use]
    pub fn new(init_alloc: usize) -> Self {
        Self {
            array: Vec::with_capacity(init_alloc),
        }
    }

    /// `size()`.
    #[must_use]
    pub fn size(&self) -> usize {
        self.array.len()
    }

    /// `empty()`.
    #[must_use]
    pub fn empty(&self) -> bool {
        self.array.is_empty()
    }

    /// `increase()` : +1 élément en fin (valeur par défaut).
    pub fn increase(&mut self) {
        self.set_size(self.array.len() + 1);
    }

    /// `setSize(newSize)` : tronque ou étend (valeurs par défaut).
    pub fn set_size(&mut self, new_size: usize) {
        self.array.resize(new_size, T::default());
    }

    /// `setSize(newSize, t)` : étend en initialisant à `t`.
    pub fn set_size_with(&mut self, new_size: usize, t: &T) {
        let old = self.array.len();
        self.array.resize(new_size, T::default());
        for x in &mut self.array[old..] {
            *x = t.clone();
        }
    }

    /// `get(i)` — panique bornes comme `debug_assert` amont.
    #[must_use]
    pub fn get(&self, i: usize) -> &T {
        debug_assert!(i < self.array.len());
        &self.array[i]
    }

    /// `set(i, t)`.
    pub fn set(&mut self, i: usize, t: T) {
        debug_assert!(i < self.array.len());
        self.array[i] = t;
    }

    /// `getFirstRef`/`getLastRef`.
    #[must_use]
    pub fn first(&self) -> &T {
        debug_assert!(!self.array.is_empty());
        &self.array[0]
    }

    #[must_use]
    pub fn last(&self) -> &T {
        debug_assert!(!self.array.is_empty());
        &self.array[self.array.len() - 1]
    }

    /// `setLast(t)`.
    pub fn set_last(&mut self, t: T) {
        debug_assert!(!self.array.is_empty());
        let n = self.array.len() - 1;
        self.array[n] = t;
    }

    /// `copyTo(dest, thisStart, thisLast, destStart)` — copie vers un
    /// autre vecteur (pas intra-vecteur, comme amont).
    pub fn copy_to(&self, dest: &mut Self, this_start: usize, this_last: usize, dest_start: usize)
    where
        T: Clone,
    {
        for i in this_start..=this_last {
            dest.set(i - this_start + dest_start, self.array[i].clone());
        }
    }

    /// `getArray`/`detachArray` → tranche / Vec consommé.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.array
    }

    #[must_use]
    pub fn detach_array(&mut self) -> Vec<T> {
        std::mem::take(&mut self.array)
    }
}

// ---------------------------------------------------------------------------
// NotSoSimpleVector — port fidèle de l'optimisation upstream
// ---------------------------------------------------------------------------

/// `misc::NotSoSimpleVector<T>` — vecteur optimisé pour les
/// insertions répétées proches du début (hyphenation dans
/// `dw::Textblock`). Un « extra array » logique absorbe les petites
/// insertions consécutives : O(n) déplacements au total au lieu de
/// O(n²).
///
/// Différence avec amont : les slots insérés sont `T::default()` au
/// lieu d'être non-initialisés (les appelants amont les écrivent
/// toujours juste après `insert`).
#[derive(Debug, Default, Clone)]
pub struct NotSoSimpleVector<T> {
    array_main: Vec<T>,         // len = numMain
    array_extra1: Vec<T>,       // len = numExtra
    array_extra2: Vec<T>,       // scratch (resizeExtra alloue les deux)
    start_extra: Option<usize>, // None = -1 amont
}

impl<T: Clone + Default> NotSoSimpleVector<T> {
    /// `NotSoSimpleVector(initAlloc)`.
    #[must_use]
    pub fn new(init_alloc: usize) -> Self {
        Self {
            array_main: Vec::with_capacity(init_alloc),
            array_extra1: Vec::new(),
            array_extra2: Vec::new(),
            start_extra: None,
        }
    }

    fn num_extra(&self) -> usize {
        self.array_extra1.len()
    }

    /// `size()` = numMain + numExtra.
    #[must_use]
    pub fn size(&self) -> usize {
        self.array_main.len() + self.num_extra()
    }

    /// `empty()`.
    #[must_use]
    pub fn empty(&self) -> bool {
        self.size() == 0
    }

    /// `increase()`.
    pub fn increase(&mut self) {
        self.set_size(self.size() + 1);
    }

    /// `setSize(newSize)` : numMain = newSize - numExtra.
    pub fn set_size(&mut self, new_size: usize) {
        debug_assert!(new_size >= self.num_extra());
        self.array_main
            .resize(new_size - self.num_extra(), T::default());
    }

    /// `consolidate()` : fusionne l'extra array dans main.
    fn consolidate(&mut self) {
        if let Some(start) = self.start_extra.take() {
            let extra = std::mem::take(&mut self.array_extra1);
            self.array_main.splice(start..start, extra);
        }
    }

    /// `insert(index, numInsert)` — port fidèle des 3 cas amont :
    /// pas d'extra / avant l'extra (consolidate+récursion) /
    /// dans l'extra / après l'extra (redistribution diff).
    pub fn insert(&mut self, index: usize, num_insert: usize) {
        if num_insert == 0 {
            return;
        }
        match self.start_extra {
            None => {
                // simple case
                self.array_extra1 = vec![T::default(); num_insert];
                self.array_extra2 = Vec::new();
                self.start_extra = Some(index);
            }
            Some(start) if index < start => {
                self.consolidate();
                self.insert(index, num_insert);
            }
            Some(start) if index < start + self.num_extra() => {
                // dans l'extra : décale la queue à droite
                let off = index - start;
                let tail = self.array_extra1.split_off(off);
                self.array_extra1
                    .extend(std::iter::repeat_with(T::default).take(num_insert));
                self.array_extra1.extend(tail);
            }
            Some(start) => {
                // après l'extra : redistribution amont.
                // seq concaténée = extra1 ++ main[start..start+diff]
                // dest : diff premiers → main[start..], queue → extra2
                let old_num_extra = self.num_extra();
                let diff = index - start - old_num_extra;
                let mut seq: Vec<T> = Vec::with_capacity(old_num_extra + diff);
                seq.extend(self.array_extra1.iter().cloned());
                seq.extend(self.array_main[start..start + diff].iter().cloned());
                // diff premiers → main à partir de start_extra
                for (i, v) in seq[..diff].iter().enumerate() {
                    self.array_main[start + i] = v.clone();
                }
                // reste (old_num_extra) → extra2, + numInsert slots
                let mut extra2: Vec<T> = Vec::with_capacity(old_num_extra + num_insert);
                extra2.extend(seq[diff..].iter().cloned());
                extra2.extend(std::iter::repeat_with(T::default).take(num_insert));
                std::mem::swap(&mut self.array_extra1, &mut extra2);
                self.array_extra2 = extra2;
                self.start_extra = Some(index - old_num_extra);
            }
        }
    }

    /// `get(i)` — adressage logique (avant / dans / après l'extra).
    #[must_use]
    pub fn get(&self, i: usize) -> &T {
        match self.start_extra {
            None => &self.array_main[i],
            Some(start) if i < start => &self.array_main[i],
            Some(start) if i >= start + self.num_extra() => &self.array_main[i - self.num_extra()],
            Some(start) => &self.array_extra1[i - start],
        }
    }

    /// `set(i, t)`.
    pub fn set(&mut self, i: usize, t: T) {
        let ne = self.num_extra();
        match self.start_extra {
            None => self.array_main[i] = t,
            Some(start) if i < start => self.array_main[i] = t,
            Some(start) if i >= start + ne => self.array_main[i - ne] = t,
            Some(start) => self.array_extra1[i - start] = t,
        }
    }

    /// `getFirst`/`getLast`.
    #[must_use]
    pub fn first(&self) -> &T {
        self.get(0)
    }

    #[must_use]
    pub fn last(&self) -> &T {
        self.get(self.size() - 1)
    }

    /// `setLast(t)`.
    pub fn set_last(&mut self, t: T) {
        let n = self.size() - 1;
        self.set(n, t);
    }
}

// ---------------------------------------------------------------------------
// StringBuffer
// ---------------------------------------------------------------------------

/// `misc::StringBuffer` — concaténation différée ; `get_chars`
/// matérialise et cache le résultat (invalidé par `append`/`clear`).
#[derive(Debug, Default)]
pub struct StringBuffer {
    parts: Vec<String>,
    num_chars: usize,
    cached: Option<String>,
}

impl StringBuffer {
    /// `StringBuffer()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `append(str)` — copie comme amont (`append` → `dStrdup`).
    pub fn append(&mut self, s: &str) {
        self.num_chars += s.len();
        self.parts.push(s.to_owned());
        self.cached = None;
    }

    /// `appendInt(n)` — `%d`.
    pub fn append_int(&mut self, n: i32) {
        self.append(&n.to_string());
    }

    /// `appendPointer(p)` — `%p` (`0x...`).
    pub fn append_pointer<T>(&mut self, p: *const T) {
        self.append(&format!("{p:p}"));
    }

    /// `appendBool(b)`.
    pub fn append_bool(&mut self, b: bool) {
        self.append(bool_to_str(b));
    }

    /// `getChars()` — la concaténation (cachée entre mutations).
    #[must_use]
    pub fn get_chars(&mut self) -> &str {
        if self.cached.is_none() {
            let mut s = String::with_capacity(self.num_chars);
            for p in &self.parts {
                s.push_str(p);
            }
            self.cached = Some(s);
        }
        self.cached.as_deref().unwrap_or_default()
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        self.parts.clear();
        self.num_chars = 0;
        self.cached = None;
    }
}

// ---------------------------------------------------------------------------
// BitSet
// ---------------------------------------------------------------------------

/// `misc::BitSet` — bitset auto-extensible (doublement de capacité).
///
/// **Bizarrerie amont préservée** (`misc.cc`) : `set`/`get` bornent
/// par `8 * i >= numBytes` (au lieu de `i / 8 >= numBytes`), donc
/// la capacité grossit ~64× trop vite. La condition de `get` est
/// inobservable — tout `set(i)` garantit `numBytes > 8*i`, aucun
/// bit setté ne peut retomber dans la zone morte — mais la
/// sur-allocation est reproduite à l'identique pour la parité.
#[derive(Debug)]
pub struct BitSet {
    bits: Vec<u8>,
    num_bits: usize,
}

impl BitSet {
    fn bytes_for_bits(bits: usize) -> usize {
        if bits == 0 {
            1
        } else {
            bits.div_ceil(8)
        }
    }

    /// `BitSet(initBits)`.
    #[must_use]
    pub fn new(init_bits: usize) -> Self {
        Self {
            bits: vec![0; Self::bytes_for_bits(init_bits)],
            num_bits: init_bits,
        }
    }

    /// `get(i)` — voir note bug amont ci-dessus.
    #[must_use]
    pub fn get(&self, i: usize) -> bool {
        if 8 * i >= self.bits.len() {
            false
        } else {
            self.bits[i / 8] & (1 << (i % 8)) != 0
        }
    }

    /// `set(i, val)` — étend `num_bits` et double `numBytes` au
    /// besoin (zéro-rempli).
    pub fn set(&mut self, i: usize, val: bool) {
        if i > self.num_bits {
            self.num_bits = i;
        }
        if 8 * i >= self.bits.len() {
            let mut new_len = self.bits.len();
            while 8 * i >= new_len {
                new_len *= 2;
            }
            self.bits.resize(new_len, 0);
        }
        if val {
            self.bits[i / 8] |= 1 << (i % 8);
        } else {
            self.bits[i / 8] &= !(1 << (i % 8));
        }
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        self.bits.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_int_half_away_from_zero() {
        assert_eq!(round_int(1.5), 2);
        assert_eq!(round_int(-1.5), -2);
        assert_eq!(round_int(1.4), 1);
        assert_eq!(round_int(-1.4), -1);
    }

    #[test]
    fn simple_vector_upstream_api() {
        let mut v: SimpleVector<i32> = SimpleVector::new(1);
        for i in 1..=10 {
            v.increase();
            v.set(v.size() - 1, i);
        }
        assert_eq!(v.size(), 10);
        assert_eq!(*v.first(), 1);
        assert_eq!(*v.last(), 10);
        let mut d: SimpleVector<i32> = SimpleVector::new(0);
        d.set_size(5);
        v.copy_to(&mut d, 2, 4, 0);
        assert_eq!(*d.get(0), 3);
        assert_eq!(*d.get(2), 5);
    }

    /// Scénario exact de `test/unit/notsosimplevector.cc`.
    /// (`allow` : indices du scénario amont, valeurs minuscules.)
    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    fn not_so_simple_vector_upstream_scenario() {
        let mut v: NotSoSimpleVector<i32> = NotSoSimpleVector::new(1);
        for i in 1..=10 {
            v.increase();
            v.set(v.size() - 1, i);
        }
        let snap =
            |v: &NotSoSimpleVector<i32>| (0..v.size()).map(|i| *v.get(i)).collect::<Vec<_>>();

        assert_eq!(snap(&v), vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

        // NB amont : insert(2,4) ajoute 4 slots mais la boucle en
        // écrit 5 → l'élément à la position 6 (le `3`) est écrasé.
        v.insert(2, 4);
        for i in 0..5usize {
            v.set(2 + i, 31 + i as i32);
        }
        assert_eq!(
            snap(&v),
            vec![1, 2, 31, 32, 33, 34, 35, 4, 5, 6, 7, 8, 9, 10]
        );

        v.insert(8, 4);
        for i in 0..5usize {
            v.set(8 + i, 51 + i as i32);
        }
        assert_eq!(
            snap(&v),
            vec![1, 2, 31, 32, 33, 34, 35, 4, 51, 52, 53, 54, 55, 6, 7, 8, 9, 10]
        );

        v.insert(10, 4);
        for i in 0..5usize {
            v.set(10 + i, 531 + i as i32);
        }
        assert_eq!(
            snap(&v),
            vec![
                1, 2, 31, 32, 33, 34, 35, 4, 51, 52, 531, 532, 533, 534, 535, 54, 55, 6, 7, 8, 9,
                10
            ]
        );

        v.insert(1, 4);
        for i in 0..5usize {
            v.set(1 + i, 21 + i as i32);
        }
        assert_eq!(
            snap(&v),
            vec![
                1, 21, 22, 23, 24, 25, 31, 32, 33, 34, 35, 4, 51, 52, 531, 532, 533, 534, 535, 54,
                55, 6, 7, 8, 9, 10
            ]
        );

        let n = v.size();
        v.insert(n, 5);
        for i in 0..5usize {
            v.set(n + i, 101 + i as i32);
        }
        assert_eq!(snap(&v)[n..], vec![101, 102, 103, 104, 105]);
        assert_eq!(v.size(), 31);
    }

    #[test]
    fn string_buffer_cache() {
        let mut sb = StringBuffer::new();
        sb.append("<class ");
        sb.append("Foo");
        sb.append(" (");
        sb.append_int(3);
        sb.append(")>");
        assert_eq!(sb.get_chars(), "<class Foo (3)>");
        sb.append("x");
        assert_eq!(sb.get_chars(), "<class Foo (3)>x");
        sb.clear();
        assert_eq!(sb.get_chars(), "");
    }

    #[test]
    fn bitset_semantics_and_upstream_growth() {
        let mut bs = BitSet::new(8); // numBytes = 1
        bs.set(0, true);
        assert!(bs.get(0));
        // Amont : set(1) → 8*1 >= 1 → numBytes double jusqu'à 16
        // (sur-allocation ~64×), puis le bit est écrit et lisible.
        bs.set(1, true);
        assert!(bs.get(1));
        assert!(!bs.get(2));
        // clear conserve la capacité, remet les bits à 0
        bs.clear();
        assert!(!bs.get(0));
        bs.set(5, true);
        assert!(bs.get(5));
    }
}
