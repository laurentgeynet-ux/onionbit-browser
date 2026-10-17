//! Port of `dlib/dlib.c` + `dlib/dlib.h`.
//!
//! `Dstr` → [`Dstr`] (wrapper `Vec<u8>`, NULs embarqués tolérés),
//! `Dlist` → [`Dlist<T>`] générique, helpers `dStr*`/`dIs*` et le
//! parseur de lignes rc (`dParser_parse_rc_line`).
//!
//! Les fonctions sans équivalent Rust (`dMalloc`, `dClose`,
//! `dUsleep`, `dGetcwd`) ne sont pas portées : allocation std,
//! `Drop`, `std::thread::sleep`, `std::env::current_dir`.

use std::cmp::Ordering;
use std::io::{self, BufRead};

// ---------------------------------------------------------------------------
// CType helpers (dIs*) — ASCII-safe, locale-indépendants comme amont
// ---------------------------------------------------------------------------

/// `dIsalnum` — `isalnum` sur `unsigned char`.
#[must_use]
pub fn is_alnum(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}
/// `dIsalpha`.
#[must_use]
pub fn is_alpha(c: u8) -> bool {
    c.is_ascii_alphabetic()
}
/// `dIscntrl`.
#[must_use]
pub fn is_cntrl(c: u8) -> bool {
    c.is_ascii_control()
}
/// `dIsdigit`.
#[must_use]
pub fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}
/// `dIsprint` — amont = `isprint` C : 0x20..=0x7E.
#[must_use]
pub fn is_print(c: u8) -> bool {
    (0x20..=0x7E).contains(&c)
}
/// `dIspunct` — printable non-alnum non-espace.
#[must_use]
pub fn is_punct(c: u8) -> bool {
    c.is_ascii_punctuation()
}
/// `dIsspace` — espace C : space, \t, \n, \v, \f, \r.
/// NB : `u8::is_ascii_whitespace` n'inclut PAS `\v` (0x0B).
#[must_use]
pub fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}
/// `dIsxdigit`.
#[must_use]
pub fn is_xdigit(c: u8) -> bool {
    c.is_ascii_hexdigit()
}
/// `dTolower`.
#[must_use]
pub fn to_lower(c: u8) -> u8 {
    c.to_ascii_lowercase()
}
/// `dIsascii`.
#[must_use]
pub fn is_ascii(c: u8) -> bool {
    c.is_ascii()
}
/// `D_ASCII_TOUPPER` — ASCII seul, pas de locale.
#[must_use]
pub fn ascii_to_upper(c: u8) -> u8 {
    c.to_ascii_uppercase()
}
/// `D_ASCII_TOLOWER`.
#[must_use]
pub fn ascii_to_lower(c: u8) -> u8 {
    c.to_ascii_lowercase()
}

// ---------------------------------------------------------------------------
// C strings helpers (dStr* sur &[u8])
// ---------------------------------------------------------------------------

/// `dStrndup` : les `sz` premiers octets (copie).
#[must_use]
pub fn str_ndup(s: &[u8], sz: usize) -> Vec<u8> {
    s[..sz.min(s.len())].to_vec()
}

/// `dStrstrip` : retire les espaces C en tête/queue. Travaille en
/// place comme amont (renvoie la tranche utile).
pub fn str_strip(s: &mut Vec<u8>) {
    let start = s.iter().position(|&c| !is_space(c)).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&c| !is_space(c))
        .map_or(start, |i| i + 1);
    if start >= end {
        s.clear();
    } else {
        s.drain(..start);
        s.truncate(end - start);
    }
}

/// `dStrshred` : efface le contenu (zéros) — secrets en mémoire.
pub fn str_shred(s: &mut [u8]) {
    s.fill(0);
}

/// `dStrnfill` : chaîne de `len` fois le caractère `c`.
#[must_use]
pub fn str_nfill(len: usize, c: u8) -> Vec<u8> {
    vec![c; len]
}

/// `dStrsep` : découpe `orig` au premier caractère de `delim`.
/// Avance le curseur ; renvoie `None` quand `orig` est épuisé.
pub fn str_sep<'a>(orig: &mut Option<&'a [u8]>, delim: &[u8]) -> Option<&'a [u8]> {
    let str_ = (*orig)?;
    if let Some(p) = str_.iter().position(|c| delim.contains(c)) {
        *orig = Some(&str_[p + 1..]);
        Some(&str_[..p])
    } else {
        *orig = None;
        Some(str_)
    }
}

/// `dStriAsciiStr` : recherche insensible à la casse (ASCII),
/// retourne l'offset de la première occurrence.
#[must_use]
pub fn stri_ascii_str(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| {
        haystack[i..i + needle.len()]
            .iter()
            .zip(needle)
            .all(|(&a, &b)| ascii_to_lower(a) == ascii_to_lower(b))
    })
}

/// `dStrAsciiCasecmp` : compare ASCII insensible à la casse,
/// retourne la différence des premiers octets divergents (0 = égaux).
#[must_use]
pub fn str_ascii_casecmp(s1: &[u8], s2: &[u8]) -> i32 {
    let mut i = 0;
    loop {
        let a = s1.get(i).copied().unwrap_or(0);
        let b = s2.get(i).copied().unwrap_or(0);
        if a == 0 && b == 0 {
            return 0;
        }
        let diff = i32::from(ascii_to_lower(a)) - i32::from(ascii_to_lower(b));
        if diff != 0 {
            return diff;
        }
        i += 1;
    }
}

/// `dStrnAsciiCasecmp` : idem borné à `n` caractères.
#[must_use]
pub fn strn_ascii_casecmp(s1: &[u8], s2: &[u8], n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let a = s1.get(i).copied().unwrap_or(0);
        let b = s2.get(i).copied().unwrap_or(0);
        if a == 0 && b == 0 {
            return 0;
        }
        let diff = i32::from(ascii_to_lower(a)) - i32::from(ascii_to_lower(b));
        if diff != 0 {
            return diff;
        }
        i += 1;
    }
    0
}

// ---------------------------------------------------------------------------
// Dstr
// ---------------------------------------------------------------------------

/// `Dstr` — chaîne dynamique d'octets (NULs embarqués tolérés, `len`
/// distinct de la capacité comme amont).
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dstr {
    buf: Vec<u8>,
}

impl Dstr {
    /// `dStr_new` : contenu `s` ou vide.
    #[must_use]
    pub fn new(s: &[u8]) -> Self {
        let mut ds = Self::sized_new(0);
        if !s.is_empty() {
            ds.append(s);
        }
        ds
    }

    /// `dStr_sized_new` : capacité initiale `sz` (min 2 comme amont).
    #[must_use]
    pub fn sized_new(sz: usize) -> Self {
        Self {
            buf: Vec::with_capacity(sz.max(2) + 1),
        }
    }

    /// Longueur utile (`ds->len`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Octets utiles (`ds->str` sans le NUL amont).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// `dStr_fit` : libère le surplus.
    pub fn fit(&mut self) {
        self.buf.shrink_to_fit();
    }

    /// `dStr_append_c`.
    pub fn append_c(&mut self, c: u8) {
        self.buf.push(c);
    }

    /// `dStr_append`.
    pub fn append(&mut self, s: &[u8]) {
        self.buf.extend_from_slice(s);
    }

    /// `dStr_append_l`.
    pub fn append_l(&mut self, s: &[u8], l: usize) {
        self.buf.extend_from_slice(&s[..l.min(s.len())]);
    }

    /// `dStr_insert_l` : insère `l` octets de `s` à `pos_0`
    /// (no-op si `pos_0 > len`, comme amont).
    pub fn insert_l(&mut self, pos_0: usize, s: &[u8], l: usize) {
        if l == 0 || pos_0 > self.buf.len() {
            return;
        }
        let l = l.min(s.len());
        self.buf.splice(pos_0..pos_0, s[..l].iter().copied());
    }

    /// `dStr_insert`.
    pub fn insert(&mut self, pos_0: usize, s: &[u8]) {
        self.insert_l(pos_0, s, s.len());
    }

    /// `dStr_truncate` : tronque à `len` (no-op si `len >= len`,
    /// amont ne rallonge pas).
    pub fn truncate(&mut self, len: usize) {
        self.buf.truncate(len);
    }

    /// `dStr_shred` : met à zéro le buffer alloué.
    pub fn shred(&mut self) {
        self.buf.fill(0);
    }

    /// `dStr_erase` : supprime `len` octets à `pos_0`.
    pub fn erase(&mut self, pos_0: usize, len: usize) {
        if len == 0 || pos_0 + len > self.buf.len() {
            return;
        }
        self.buf.drain(pos_0..pos_0 + len);
    }

    /// `dStr_sprintf` : remplace le contenu par la chaîne formatée.
    pub fn sprintf(&mut self, args: std::fmt::Arguments<'_>) {
        use std::io::Write as _;
        self.buf.clear();
        // Infaillible : écrire dans un Vec ne retourne jamais Err.
        let _ = self.buf.write_fmt(args);
    }

    /// `dStr_sprintfa` : ajoute la chaîne formatée.
    pub fn sprintfa(&mut self, args: std::fmt::Arguments<'_>) {
        use std::io::Write as _;
        let _ = self.buf.write_fmt(args);
    }

    /// `dStr_cmp` : `memcmp` sur `min(len1+1, len2+1)` — implémenté
    /// via `Ord` (tranches puis longueurs).
    #[must_use]
    pub fn dstr_cmp(&self, other: &Self) -> Ordering {
        self.cmp(other)
    }

    /// `dStr_memmem` : première occurrence de `needle` dans `self`.
    #[must_use]
    pub fn memmem(&self, needle: &Self) -> Option<usize> {
        let n = &needle.buf;
        let h = &self.buf;
        if n.is_empty() {
            return Some(0);
        }
        if n.len() > h.len() {
            return None;
        }
        h.windows(n.len()).position(|w| w == n.as_slice())
    }

    /// `dStr_printable` : représentation printable (`\xHH` pour
    /// non-printable, `\n` conservé, tronqué à ~`maxlen` + `...`).
    #[must_use]
    pub fn printable(&self, maxlen: usize) -> Vec<u8> {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut out = Vec::new();
        for &c in &self.buf {
            if out.len() >= maxlen {
                break;
            }
            if is_print(c) || c == b'\n' {
                out.push(c);
            } else {
                out.extend_from_slice(b"\\x");
                out.push(HEX[usize::from(c >> 4) & 15]);
                out.push(HEX[usize::from(c) & 15]);
            }
        }
        if out.len() >= maxlen {
            out.extend_from_slice(b"...");
        }
        out
    }

    /// `dStr_shorten` : coupe `src` au milieu par `...` pour tenir
    /// en `n` caractères (n min = 9). Ajouté à `self`.
    pub fn shorten(&mut self, src: &[u8], n: usize) {
        let n = n.max(9);
        if src.len() > n {
            let m = n - 3;
            let n1 = m / 2;
            let n2 = m - n1;
            self.append_l(src, n1);
            self.append(b"...");
            self.append(&src[src.len() - n2..]);
        } else {
            self.append(src);
        }
    }

    /// Conversion consommatrice (`dstr->str` récupéré, `dStr_free(ds,0)`).
    #[must_use]
    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }
}

impl From<&[u8]> for Dstr {
    fn from(s: &[u8]) -> Self {
        Self::new(s)
    }
}

// ---------------------------------------------------------------------------
// Dlist
// ---------------------------------------------------------------------------

/// Comparateur amont : <0 / 0 / >0 (`dCompareFunc`).
pub type CompareFunc<T> = fn(&T, &T) -> Ordering;

/// `Dlist` — liste dynamique ; `Vec<T>` avec la sémantique des
/// opérations amont (`remove_fast` = swap-remove, sorted insert =
/// binary search comme `dList_insert_sorted`).
#[derive(Debug, Default)]
pub struct Dlist<T> {
    list: Vec<T>,
}

impl<T> Dlist<T> {
    /// `dList_new`.
    #[must_use]
    pub fn new(size: usize) -> Self {
        Self {
            list: Vec::with_capacity(size),
        }
    }

    /// `dList_length`.
    #[must_use]
    pub fn length(&self) -> usize {
        self.list.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// `dList_insert_pos` : no-op si `pos0 > len` (comme amont).
    pub fn insert_pos(&mut self, data: T, pos0: usize) {
        if pos0 <= self.list.len() {
            self.list.insert(pos0, data);
        }
    }

    /// `dList_append`.
    pub fn append(&mut self, data: T) {
        self.list.push(data);
    }

    /// `dList_prepend`.
    pub fn prepend(&mut self, data: T) {
        self.list.insert(0, data);
    }

    /// `dList_remove` : supprime en préservant l'ordre.
    pub fn remove(&mut self, data: &T)
    where
        T: PartialEq,
    {
        if let Some(i) = self.list.iter().position(|x| x == data) {
            self.list.remove(i);
        }
    }

    /// `dList_remove_fast` : supprime sans préserver l'ordre.
    pub fn remove_fast(&mut self, data: &T)
    where
        T: PartialEq,
    {
        if let Some(i) = self.list.iter().position(|x| x == data) {
            self.list.swap_remove(i);
        }
    }

    /// `dList_nth_data`.
    #[must_use]
    pub fn nth_data(&self, n0: usize) -> Option<&T> {
        self.list.get(n0)
    }

    /// `dList_find`.
    #[must_use]
    pub fn find(&self, data: &T) -> Option<&T>
    where
        T: PartialEq,
    {
        self.list.iter().find(|x| *x == data)
    }

    /// `dList_find_idx`.
    #[must_use]
    pub fn find_idx(&self, data: &T) -> Option<usize>
    where
        T: PartialEq,
    {
        self.list.iter().position(|x| x == data)
    }

    /// `dList_find_custom` : `func(item, data)` == Equal → item.
    #[must_use]
    pub fn find_custom<'a, D>(&'a self, data: &D, func: fn(&T, &D) -> Ordering) -> Option<&'a T> {
        self.list.iter().find(|x| func(x, data) == Ordering::Equal)
    }

    /// `dList_sort` : tri avec `func` (amont = quicksort maison,
    /// instable — `sort_unstable` est équivalent).
    pub fn sort(&mut self, func: CompareFunc<T>) {
        self.list.sort_unstable_by(func);
    }

    /// `dList_insert_sorted` : insertion binaire ; si un élément
    /// `func`-égal existe, insère **avant** lui (amont s'arrête au
    /// premier `st == 0` puis insère à `i` — comportement reproduit :
    /// première position `st >= 0` rencontrée).
    pub fn insert_sorted(&mut self, data: T, func: CompareFunc<T>) {
        // Reproduit la boucle amont : binary search, `st` du dernier
        // pivot examiné décide i ou i+1.
        let mut min = 0usize;
        let mut max = self.list.len().wrapping_sub(1);
        // `st` amont initialisé à 0 → insertion à i=0 sur liste vide.
        let (mut i, mut st) = (0usize, Ordering::Equal);
        if !self.list.is_empty() {
            while min <= max {
                i = usize::midpoint(min, max);
                st = func(&self.list[i], &data);
                match st {
                    Ordering::Less => min = i + 1,
                    Ordering::Greater => {
                        if i == 0 {
                            break;
                        }
                        max = i - 1;
                    }
                    Ordering::Equal => break,
                }
            }
        }
        let pos = if st == Ordering::Less { i + 1 } else { i };
        self.insert_pos(data, pos);
    }

    /// `dList_find_sorted` : recherche binaire dans une liste triée
    /// par `func`.
    #[must_use]
    pub fn find_sorted<D>(&self, data: &D, func: fn(&T, &D) -> Ordering) -> Option<&T> {
        self.list
            .binary_search_by(|x| func(x, data))
            .ok()
            .map(|i| &self.list[i])
    }

    /// Itérateur sur les éléments.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.list.iter()
    }
}

impl<T> From<Vec<T>> for Dlist<T> {
    fn from(list: Vec<T>) -> Self {
        Self { list }
    }
}

impl<'a, T> IntoIterator for &'a Dlist<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

// ---------------------------------------------------------------------------
// Parse function (dParser_parse_rc_line)
// ---------------------------------------------------------------------------

/// Résultat de `dParser_parse_rc_line` (retours amont 0/1/-1).
#[derive(Debug, PartialEq, Eq)]
pub enum RcLine {
    /// `name = value` — `value` désquotée si `"..."`.
    Pair { name: Vec<u8>, value: Vec<u8> },
    /// Ligne vide ou commentaire `#` (retour amont 1).
    Skip,
    /// Pas de `=` (retour amont -1).
    Error,
}

/// `dParser_parse_rc_line` : parse `name = value`, désquote
/// `"value"`, ignore lignes vides/`#`. Entrée = la ligne complète.
#[must_use]
pub fn parse_rc_line(line: &[u8]) -> RcLine {
    let mut line = line.to_vec();
    str_strip(&mut line);
    if line.is_empty() || line[0] == b'#' {
        return RcLine::Skip;
    }
    let Some(eq) = line.iter().position(|&c| c == b'=') else {
        return RcLine::Error;
    };
    // name : jusqu'au premier espace ou '=' (amont tronque à l'espace)
    let name_end = line[..eq]
        .iter()
        .position(|&c| is_space(c) || c == b'=')
        .unwrap_or(eq);
    // amont exige qu'il n'y ait que des espaces entre name et '='
    if line[name_end..eq].iter().any(|&c| !is_space(c)) {
        // cas impossible par construction mais gardé pour lisibilité
        return RcLine::Error;
    }
    let mut p = eq + 1;
    // skip whitespace après '='
    while p < line.len() && is_space(line[p]) {
        p += 1;
    }
    let mut value = line[p..].to_vec();
    if value.len() >= 2 && value[0] == b'"' && value[value.len() - 1] == b'"' {
        value.remove(value.len() - 1);
        value.remove(0);
    }
    RcLine::Pair {
        name: line[..name_end].to_vec(),
        value,
    }
}

// ---------------------------------------------------------------------------
// Misc utility functions
// ---------------------------------------------------------------------------

/// `dGetline` : lit une ligne (terminateur `\n` inclus) depuis un
/// flux bufferisé ; `None` à EOF si rien lu.
///
/// # Errors
/// Propage `io::Error` du `BufRead` sous-jacent.
pub fn getline(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line)?;
    Ok(if line.is_empty() { None } else { Some(line) })
}

/// `dGethomedir` — `$HOME` sinon `$HOMEDRIVE$HOMEPATH` sinon `/`
/// (verbatim dlib.c:933 — pas de cache : l'appel est rare et les
/// variables d'env sont relues à chaud).
#[must_use]
pub fn get_home_dir() -> Vec<u8> {
    if let Ok(home) = std::env::var("HOME") {
        home.into_bytes()
    } else if let (Ok(drive), Ok(path)) = (std::env::var("HOMEDRIVE"), std::env::var("HOMEPATH")) {
        let mut v = drive.into_bytes();
        v.extend_from_slice(&path.into_bytes());
        v
    } else {
        b"/".to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- CType helpers -----------------------------------------------------
    #[test]
    fn ctype_ascii() {
        assert!(is_space(b' ') && is_space(b'\t') && !is_space(b'a'));
        assert!(is_digit(b'7') && !is_digit(b'x'));
        assert!(is_punct(b'!') && !is_punct(b'a'));
        assert_eq!(ascii_to_upper(b'a'), b'A');
        assert_eq!(ascii_to_upper(b'1'), b'1');
        assert_eq!(ascii_to_lower(b'A'), b'a');
        // dIsspace amont inclut \v \f \r
        for c in b" \t\n\x0b\x0c\r" {
            assert!(is_space(*c));
        }
    }

    // --- C strings ---------------------------------------------------------
    #[test]
    fn strstrip_variants() {
        let mut v = b"  hello \t\n".to_vec();
        str_strip(&mut v);
        assert_eq!(v, b"hello");
        let mut v = b"   ".to_vec();
        str_strip(&mut v);
        assert_eq!(v, b"");
        let mut v = b"no-strip".to_vec();
        str_strip(&mut v);
        assert_eq!(v, b"no-strip");
    }

    #[test]
    fn strsep_splits_and_advances() {
        let mut cur = Some(&b"a:b,c"[..]);
        assert_eq!(str_sep(&mut cur, b":,"), Some(&b"a"[..]));
        assert_eq!(str_sep(&mut cur, b":,"), Some(&b"b"[..]));
        assert_eq!(str_sep(&mut cur, b":,"), Some(&b"c"[..]));
        assert_eq!(str_sep(&mut cur, b":,"), None);
        // délimiteur absent : tout retourné puis None
        let mut cur = Some(&b"abc"[..]);
        assert_eq!(str_sep(&mut cur, b","), Some(&b"abc"[..]));
        assert!(cur.is_none());
    }

    #[test]
    fn stri_ascii_str_finds() {
        assert_eq!(stri_ascii_str(b"Hello World", b"world"), Some(6));
        assert_eq!(stri_ascii_str(b"aBc", b"ABC"), Some(0));
        assert_eq!(stri_ascii_str(b"abc", b"d"), None);
        assert_eq!(stri_ascii_str(b"ab", b"abc"), None);
        // cas du rollback i -= j amont
        assert_eq!(stri_ascii_str(b"aaab", b"aab"), Some(1));
    }

    #[test]
    fn casecmp_semantics() {
        assert_eq!(str_ascii_casecmp(b"abc", b"ABC"), 0);
        assert!(str_ascii_casecmp(b"abd", b"abc") > 0);
        assert!(str_ascii_casecmp(b"ab", b"abc") < 0); // préfixe
        assert_eq!(strn_ascii_casecmp(b"abcd", b"abce", 3), 0);
        assert!(strn_ascii_casecmp(b"abcd", b"abce", 4) < 0);
        assert_eq!(strn_ascii_casecmp(b"", b"", 5), 0);
    }

    // --- Dstr --------------------------------------------------------------
    #[test]
    fn dstr_append_insert_erase() {
        let mut d = Dstr::new(b"world");
        d.insert(0, b"hello ");
        assert_eq!(d.as_bytes(), b"hello world");
        d.insert_l(0, b"XY", 1);
        assert_eq!(d.as_bytes(), b"Xhello world");
        d.erase(0, 1);
        assert_eq!(d.as_bytes(), b"hello world");
        d.truncate(5);
        assert_eq!(d.as_bytes(), b"hello");
        // insert hors borne = no-op amont
        d.insert(99, b"!");
        assert_eq!(d.as_bytes(), b"hello");
    }

    #[test]
    fn dstr_cmp_memmem_printable() {
        let a = Dstr::new(b"foo");
        let b = Dstr::new(b"foo");
        assert_eq!(a.dstr_cmp(&b), Ordering::Equal);
        let hay = Dstr::new(b"the quick brown");
        let needle = Dstr::new(b"quick");
        assert_eq!(hay.memmem(&needle), Some(4));
        let empty = Dstr::new(b"");
        assert_eq!(hay.memmem(&empty), Some(0));
        let bin = Dstr::new(b"a\x01b\n");
        assert_eq!(bin.printable(64), b"a\\x01b\n");
    }

    #[test]
    fn dstr_shorten_middle() {
        let mut d = Dstr::default();
        d.shorten(b"0123456789abcdef", 12);
        // m=9, n1=4, n2=5 → 4 + "..." + 5 derniers = 12 octets
        assert_eq!(d.as_bytes(), b"0123...bcdef");
        let mut d2 = Dstr::default();
        d2.shorten(b"short", 12);
        assert_eq!(d2.as_bytes(), b"short");
    }

    #[test]
    fn dstr_sprintf() {
        let mut d = Dstr::new(b"old");
        d.sprintf(format_args!("{}={}", "a", 42));
        assert_eq!(d.as_bytes(), b"a=42");
        d.sprintfa(format_args!("+{}", 1));
        assert_eq!(d.as_bytes(), b"a=42+1");
    }

    // --- Dlist -------------------------------------------------------------
    #[test]
    fn dlist_ops() {
        let mut l = Dlist::new(4);
        l.append(2);
        l.append(3);
        l.prepend(1);
        assert_eq!(l.length(), 3);
        l.insert_pos(0, 1);
        assert_eq!(l.nth_data(1), Some(&0));
        l.insert_pos(9, 99); // no-op amont
        assert_eq!(l.length(), 4);
        l.remove(&2);
        assert_eq!(l.find_idx(&3), Some(2));
        l.remove_fast(&0);
        assert_eq!(l.length(), 2);
    }

    #[test]
    fn dlist_sorted() {
        let mut l = Dlist::new(4);
        for v in [5, 1, 9, 3, 7] {
            l.insert_sorted(v, Ord::cmp);
        }
        let got: Vec<_> = l.iter().copied().collect();
        assert_eq!(got, vec![1, 3, 5, 7, 9]);
        assert_eq!(l.find_sorted(&7, Ord::cmp), Some(&7));
        assert_eq!(l.find_sorted(&4, Ord::cmp), None);
        l.sort(Ord::cmp);
    }

    // --- parse_rc_line -----------------------------------------------------
    #[test]
    fn rc_line_pairs() {
        assert_eq!(
            parse_rc_line(b"font = serif"),
            RcLine::Pair {
                name: b"font".to_vec(),
                value: b"serif".to_vec()
            }
        );
        assert_eq!(
            parse_rc_line(b"  name=\"quoted value\"  "),
            RcLine::Pair {
                name: b"name".to_vec(),
                value: b"quoted value".to_vec()
            }
        );
        assert_eq!(
            parse_rc_line(b"k = v=extra"),
            RcLine::Pair {
                name: b"k".to_vec(),
                value: b"v=extra".to_vec()
            }
        );
    }

    #[test]
    fn rc_line_skip_and_error() {
        assert_eq!(parse_rc_line(b"   "), RcLine::Skip);
        assert_eq!(parse_rc_line(b"# comment"), RcLine::Skip);
        assert_eq!(parse_rc_line(b"  # indented"), RcLine::Skip);
        assert_eq!(parse_rc_line(b"no equals"), RcLine::Error);
        // `name =` vide : amont retourne -1 ? -> vérif upstream:
        // p == eq ? non, p pointe après '=' -> value vide -> Pair
        // NB : `k =` donne p == line.end -> value "" -> Pair chez amont
    }

    #[test]
    fn getline_reads() {
        let mut r = std::io::BufReader::new(&b"l1\nl2"[..]);
        assert_eq!(getline(&mut r).unwrap(), Some(b"l1\n".to_vec()));
        assert_eq!(getline(&mut r).unwrap(), Some(b"l2".to_vec()));
        assert_eq!(getline(&mut r).unwrap(), None);
    }
}
