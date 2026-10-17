//! Port de `dw/hyphenator.cc` + `.hh` — césure par patrons TeX.
//!
//! Traduction directe de l'implémentation Python de Ned Batchelder
//! (comme amont). `DILLO_LIBDIR` amont (chemin de compilation) →
//! `env::var("DILLO_LIBDIR")` au runtime, fallback `libdir` par
//! défaut — déviation documentée (build-time → runtime).
//!
//! `Hyphenator` est mis en cache par langue dans une table
//! `thread_local` (amont : `HashTable` globale — dillo est
//! mono-thread).

// Casts / verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::rc::Rc;

use dillo_common::lout::unicode;

use crate::platform::Platform;

/// `LEN` — taille des buffers de lecture amont.
const LEN: usize = 1000;

/// Chemin de base des fichiers d'hyphenation — ≙ `DILLO_LIBDIR`
/// (défini à la compilation amont ; ici variable d'environnement
/// avec valeur par défaut). Déviation documentée.
#[must_use]
fn dillo_libdir() -> String {
    std::env::var("DILLO_LIBDIR").unwrap_or_else(|_| "/usr/local/lib/dillo".into())
}

// ------------------------------------------------------------------
// Trie — verbatim hyphenator.hh/cc.
// ------------------------------------------------------------------

/// `Trie::TrieNode`.
#[derive(Debug, Clone)]
struct TrieNode {
    /// Caractère (0 = slot libre).
    c: u8,
    /// Prochain état (`-1` = feuille/aucun).
    next: i32,
    /// Points de césure (≙ `const char *` dans la zone).
    data: Option<Vec<u8>>,
}

/// `trieNodeNull` — nœud vide verbatim.
const fn trie_node_null() -> TrieNode {
    TrieNode {
        c: 0,
        next: 0,
        data: None,
    }
}

/// `class Trie`.
#[derive(Debug, Default)]
struct Trie {
    /// `array` — table des nœuds (allouée, pas de `freeArray` en
    /// Rust : toujours possédée).
    array: Vec<TrieNode>,
}

impl Trie {
    /// `validState`.
    fn valid_state(&self, state: i32) -> bool {
        state >= 0 && (state as usize) < self.array.len()
    }

    /// `getData (c, &state)` — verbatim : retourne `data` et avance
    /// l'état (`next > 0 ? next : -1`), `-1` et `None` sinon.
    fn get_data(&self, c: u8, state: &mut i32) -> Option<&[u8]> {
        if !self.valid_state(*state) {
            return None;
        }
        let tn = &self.array[*state as usize + c as usize];
        if tn.c == c {
            *state = if tn.next > 0 { tn.next } else { -1 };
            tn.data.as_deref()
        } else {
            *state = -1;
            None
        }
    }

    /// `save` — format texte `%d, %d[, %s]\n` verbatim.
    #[allow(dead_code)] // `saveTrie` est exposé amont mais inutilisé.
    fn save(&self, w: &mut dyn std::io::Write) {
        for tn in &self.array {
            match &tn.data {
                Some(data) => {
                    let _ = writeln!(
                        w,
                        "{}, {}, {}",
                        tn.c,
                        tn.next,
                        String::from_utf8_lossy(data)
                    );
                }
                None => {
                    let _ = writeln!(w, "{}, {}", tn.c, tn.next);
                }
            }
        }
    }

    /// `load` — 0 au succès, 1 à l'erreur (verbatim).
    fn load(&mut self, r: &mut dyn BufRead) -> i32 {
        let mut tree: Vec<TrieNode> = Vec::with_capacity(100);
        let mut max_next = 0i32;

        let mut line = String::with_capacity(LEN);
        loop {
            line.clear();
            match r.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let s = line.trim_end_matches(['\n', '\r']);
            if s.is_empty() {
                continue;
            }
            // `sscanf (s, "%d, %d, %s", &c, &next, data)`.
            let mut parts = s.splitn(3, ',');
            let c = parts.next().and_then(|p| p.trim().parse::<i32>().ok());
            let next = parts.next().and_then(|p| p.trim().parse::<i32>().ok());
            let data = parts.next().map(str::trim).filter(|d| !d.is_empty());

            if let (Some(c), Some(next)) = (c, next) {
                if !(0..256).contains(&c) || next < 0 {
                    return 1;
                }
                tree.push(TrieNode {
                    c: c as u8,
                    next,
                    data: data.map(|d| d.as_bytes().to_vec()),
                });
                max_next = max_next.max(next);
            } else {
                return 1;
            }
        }

        if max_next >= tree.len() as i32 {
            return 1;
        }
        self.array = tree;
        0
    }
}

// ------------------------------------------------------------------
// TrieBuilder — verbatim.
// ------------------------------------------------------------------

/// `TrieBuilder::StackEntry` — `next[256]`/`data[256]`.
struct StackEntry {
    /// Caractère menant à cet état.
    c: u8,
    /// Nombre d'entrées utilisées.
    count: i32,
    /// `next[256]`.
    next: Box<[i32; 256]>,
    /// `data[256]` (≙ pointeurs zone).
    data: Box<[Option<Vec<u8>>; 256]>,
    /// `data1` — valeur du chemin complet.
    data1: Option<Vec<u8>>,
}

impl StackEntry {
    /// `memset(e, 0, sizeof)` + `e->c = c` verbatim.
    fn new(c: u8) -> Self {
        StackEntry {
            c,
            count: 0,
            next: Box::new([0; 256]),
            data: Box::new(std::array::from_fn(|_| None)),
            data1: None,
        }
    }
}

/// `TrieBuilder::DataEntry`.
struct DataEntry {
    /// `key` (≙ `dStrdup`).
    key: Vec<u8>,
    /// `value` (≙ `dataZone->strdup`).
    value: Vec<u8>,
}

/// `class TrieBuilder`.
struct TrieBuilder {
    /// `pack`.
    pack: usize,
    /// `tree` — table en construction.
    tree: Vec<TrieNode>,
    /// `dataList` — paires clé/valeur à trier.
    data_list: Vec<DataEntry>,
    /// `stateStack` — pile d'états pendant l'insertion triée.
    state_stack: Vec<StackEntry>,
}

impl TrieBuilder {
    /// `TrieBuilder (pack)` verbatim.
    fn new(pack: usize) -> Self {
        let mut tb = TrieBuilder {
            pack,
            tree: Vec::with_capacity(20000),
            data_list: Vec::with_capacity(10000),
            state_stack: Vec::with_capacity(10),
        };
        tb.state_stack_push(0);
        tb
    }

    /// `insert (key, value)`.
    fn insert(&mut self, key: &[u8], value: &[u8]) {
        self.data_list.push(DataEntry {
            key: key.to_vec(),
            value: value.to_vec(),
        });
    }

    /// `insertState (state, root)` — verbatim (slot linéaire avec
    /// réservation 0..256 pour la racine).
    fn insert_state(&mut self, state: &StackEntry, root: bool) -> i32 {
        if state.count == 0 {
            return 0;
        }

        let mut i: usize;
        if root {
            i = 0; // slot 0 réservé à la racine
        } else {
            // Plus `pack` est grand, plus le trie est compact.
            i = (self.tree.len() as i64 - self.pack as i64 + 2 * i64::from(state.count)).max(0)
                as usize;
            if i < 256 {
                i = 256;
            }
        }

        loop {
            if i + 256 > self.tree.len() {
                self.tree.resize(i + 256, trie_node_null());
            }
            let mut j = 1usize;
            while j < 256 {
                let tn = &self.tree[i + j];
                if tn.c == j as u8 || ((state.next[j] != 0 || state.data[j].is_some()) && tn.c != 0)
                {
                    break;
                }
                j += 1;
            }
            if j == 256 {
                break;
            }
            i += 1;
        }

        for j in 1..256 {
            if state.next[j] != 0 || state.data[j].is_some() {
                let tn = &mut self.tree[i + j];
                tn.c = j as u8;
                tn.next = state.next[j];
                tn.data.clone_from(&state.data[j]);
            }
        }

        debug_assert!(root || i >= 256);
        debug_assert!(!root || i == 0);
        i as i32
    }

    /// `stateStackPush`.
    fn state_stack_push(&mut self, c: u8) {
        self.state_stack.push(StackEntry::new(c));
    }

    /// `stateStackPop` — insère l'état au sommet puis le relie au
    /// parent (verbatim).
    fn state_stack_pop(&mut self) -> i32 {
        let is_root = self.state_stack.len() == 1;
        let (next, c, data) = {
            let (next, c, data) = match self.state_stack.last() {
                Some(e) => {
                    // Extraire avant l'appel `&mut self` (≙ amont :
                    // `insertState(e)` lit `e` qui vit dans la pile).
                    let c = e.c;
                    let data = e.data1.clone();
                    let entry = StackEntry {
                        c: e.c,
                        count: e.count,
                        next: e.next.clone(),
                        data: e.data.clone(),
                        data1: e.data1.clone(),
                    };
                    (self.insert_state(&entry, is_root), c, data)
                }
                None => (0, 0, None),
            };
            (next, c, data)
        };
        self.state_stack.pop();

        if let Some(parent) = self.state_stack.last_mut() {
            debug_assert_eq!(parent.next[c as usize], 0);
            debug_assert!(parent.data[c as usize].is_none());
            parent.next[c as usize] = next;
            parent.data[c as usize] = data;
            parent.count += 1;
        }

        next
    }

    /// `createTrie` — tri lexicographique par octets + insertion
    /// triée + vidage de la pile (verbatim).
    fn create_trie(mut self) -> Trie {
        let mut data_list = std::mem::take(&mut self.data_list);
        data_list.sort_by(|a, b| a.key.cmp(&b.key));

        for entry in &data_list {
            self.insert_sorted(&entry.key, &entry.value);
        }

        while !self.state_stack.is_empty() {
            self.state_stack_pop();
        }

        Trie {
            array: std::mem::take(&mut self.tree),
        }
    }

    /// `insertSorted` — verbatim (pop des états divergents, push
    /// jusqu'à `len`, pop l'excédent, `data1 = data`).
    fn insert_sorted(&mut self, s: &[u8], data: &[u8]) {
        let len = s.len();

        for (i, &b) in s.iter().enumerate() {
            if self.state_stack.len() > i + 1 && self.state_stack[i + 1].c != b {
                for _ in ((i + 1)..self.state_stack.len()).rev() {
                    self.state_stack_pop();
                }
            }

            if i + 1 >= self.state_stack.len() {
                self.state_stack_push(b);
            }
        }

        while self.state_stack.len() > len + 1 {
            self.state_stack_pop();
        }

        debug_assert_eq!(self.state_stack.len(), len + 1);
        if let Some(last) = self.state_stack.last_mut() {
            last.data1 = Some(data.to_vec());
        }
    }
}

// ------------------------------------------------------------------
// Hyphenator — verbatim.
// ------------------------------------------------------------------

// `hyphenators` — cache par langue (`HashTable` globale amont ;
// `thread_local` ici, dillo étant mono-thread).
thread_local! {
    static HYPHENATORS: RefCell<HashMap<Vec<u8>, Rc<Hyphenator>>> =
        RefCell::new(HashMap::new());
}

/// `class Hyphenator`.
pub struct Hyphenator {
    /// `trie` — `None` ≙ `NULL` (pas de fichier de patrons).
    trie: Option<Trie>,
    /// `exceptions` — `HashTable<ConstString, Vector<Integer>>`.
    exceptions: Option<HashMap<Vec<u8>, Vec<i32>>>,
}

impl Hyphenator {
    /// `Hyphenator (patFile, excFile, pack)` — verbatim : `.trie`
    /// précompilé si présent, sinon parse `.pat` ; `.exc` toujours.
    fn new(pat_file: &str, exc_file: &str, pack: usize) -> Self {
        let mut trie = None;

        // `snprintf (buf, "%s.trie", patFile)` + load.
        let trie_path = format!("{pat_file}.trie");
        if let Ok(f) = File::open(&trie_path) {
            let mut t = Trie::default();
            if t.load(&mut BufReader::new(f)) == 0 {
                trie = Some(t);
            }
        }

        if trie.is_none() {
            if let Ok(f) = File::open(pat_file) {
                let mut tb = TrieBuilder::new(pack);
                for line in BufReader::new(f).lines().map_while(Result::ok) {
                    if !line.starts_with('%') {
                        Hyphenator::insert_pattern(&mut tb, line.as_bytes());
                    }
                }
                trie = Some(tb.create_trie());
            }
        }

        let mut exceptions = None;
        if let Ok(f) = File::open(exc_file) {
            let mut map = HashMap::new();
            for line in BufReader::new(f).lines().map_while(Result::ok) {
                if !line.starts_with('%') {
                    Self::insert_exception(&mut map, line.as_bytes());
                }
            }
            exceptions = Some(map);
        }

        Hyphenator { trie, exceptions }
    }

    /// `getHyphenator (language)` — cache + chemins
    /// `%s/hyphenation/%s.pat|.exc` verbatim.
    #[must_use]
    pub fn get_hyphenator(language: &[u8]) -> Rc<Hyphenator> {
        if let Some(h) = HYPHENATORS.with(|t| t.borrow().get(language).cloned()) {
            return h;
        }
        let libdir = dillo_libdir();
        let lang = String::from_utf8_lossy(language);
        let pat_file = format!("{libdir}/hyphenation/{lang}.pat");
        let exc_file = format!("{libdir}/hyphenation/{lang}.exc");
        let h = Rc::new(Hyphenator::new(&pat_file, &exc_file, 256));
        HYPHENATORS.with(|t| t.borrow_mut().insert(language.to_vec(), h.clone()));
        h
    }

    /// `insertPattern` — 'a1bc3d4' → chars 'abcd' + points
    /// [0,1,0,3,4] (verbatim, un digit par point).
    fn insert_pattern(trie_builder: &mut TrieBuilder, s: &[u8]) {
        let mut chars: Vec<u8> = Vec::with_capacity(s.len() + 1);
        let mut points: Vec<u8> = Vec::with_capacity(s.len() + 2);

        for &b in s {
            if b.is_ascii_digit() {
                if points.len() <= chars.len() {
                    points.resize(chars.len() + 1, b'0');
                }
                points[chars.len()] = b;
            } else {
                chars.push(b);
            }
        }
        if points.len() < chars.len() + 2 {
            points.resize(chars.len() + 2, b'0');
        }
        points[chars.len() + 1] = 0;

        trie_builder.insert(&chars, &points);
    }

    /// `insertException` — retire les soft hyphens (0xC2 0xAD),
    /// mémorise leurs positions dans la chaîne nettoyée (verbatim).
    fn insert_exception(map: &mut HashMap<Vec<u8>, Vec<i32>>, s: &[u8]) {
        let mut breaks: Vec<i32> = Vec::with_capacity(1);
        let len = s.len();
        let mut i = 0;
        while i < len - 1 {
            if s[i] == 0xc2 && s[i + 1] == 0xad {
                breaks.push(i as i32 - 2 * breaks.len() as i32);
            }
            i += 1;
        }

        let mut no_hyphens = Vec::with_capacity(len);
        let mut i = 0;
        while i < len {
            if i < len - 1 && s[i] == 0xc2 && s[i + 1] == 0xad {
                i += 2;
            } else {
                no_hyphens.push(s[i]);
                i += 1;
            }
        }

        map.insert(no_hyphens, breaks);
    }

    /// `isHyphenationCandidate` — verbatim : mots ≤ 4 octets non.
    #[must_use]
    pub fn is_hyphenation_candidate(word: &[u8]) -> bool {
        word.len() > 4
    }

    /// `isCharPartOfActualWord` — `isAlpha (decodeUtf8 (s))`
    /// verbatim.
    fn is_char_part_of_actual_word(s: &[u8]) -> bool {
        unicode::is_alpha(unicode::decode_utf8(s))
    }

    /// `hyphenateWord (platform, word, &numBreaks)` — verbatim :
    /// minuscule via plateforme, découpe en runs alpha, points de
    /// césure cumulés en offsets octets. Retourne les positions.
    pub fn hyphenate_word(&self, platform: &Rc<RefCell<dyn Platform>>, word: &[u8]) -> Vec<i32> {
        if (self.trie.is_none() && self.exceptions.is_none())
            || !Self::is_hyphenation_candidate(word)
        {
            return Vec::new();
        }

        let mut word_lc = platform.borrow_mut().text_to_lower(word);

        let mut start = 0usize;
        let mut break_pos: Vec<i32> = Vec::with_capacity(1);

        loop {
            while start < word_lc.len()
                && word_lc[start] != 0
                && !Self::is_char_part_of_actual_word(&word_lc[start..])
            {
                start = platform.borrow_mut().next_glyph(&word_lc, start as i32) as usize;
            }

            if start >= word_lc.len() || word_lc[start] == 0 {
                break;
            }

            let mut end = start;
            let mut i = end;
            while i < word_lc.len() && word_lc[i] != 0 {
                if !Self::is_char_part_of_actual_word(&word_lc[i..]) {
                    break;
                }
                end = i;
                i = platform.borrow_mut().next_glyph(&word_lc, i as i32) as usize;
            }
            end = platform.borrow_mut().next_glyph(&word_lc, end as i32) as usize;

            let next_start;
            if end < word_lc.len() && word_lc[end] != 0 {
                next_start = platform.borrow_mut().next_glyph(&word_lc, end as i32) as usize;
                word_lc[end] = 0;
            } else {
                next_start = end;
            }

            self.hyphenate_single_word(&word_lc[start..end], start as i32, &mut break_pos);
            start = next_start;
        }

        break_pos
    }

    /// `hyphenateSingleWord` — exceptions d'abord, sinon marche du
    /// trie sur ".mot." en accumulant `max` par point ; zéro sur les
    /// deux premiers/derniers *caractères* ; points impairs →
    /// positions `i + 1 + offset` (verbatim).
    fn hyphenate_single_word(&self, word_lc: &[u8], offset: i32, break_pos: &mut Vec<i32>) {
        // Exceptions.
        if let Some(exceptions) = &self.exceptions {
            if let Some(exceptional_breaks) = exceptions.get(word_lc) {
                for &b in exceptional_breaks {
                    break_pos.push(b + offset);
                }
                return;
            }
        }

        // `trie == NULL` → pas de patrons.
        let Some(trie) = &self.trie else { return };

        let mut work = Vec::with_capacity(word_lc.len() + 2);
        work.push(b'.');
        work.extend_from_slice(word_lc);
        work.push(b'.');

        let l = work.len();
        let mut points = vec![0u8; l + 1];

        for i in 0..l {
            let mut state = 0i32;
            let mut j = i;
            while j < l && trie.valid_state(state) {
                if let Some(p) = trie.get_data(work[j], &mut state) {
                    for (k, &pk) in p.iter().enumerate() {
                        if pk != 0 {
                            points[i + k] = points[i + k].max(pk - b'0');
                        }
                    }
                }
                j += 1;
            }
        }

        // Pas de césure sur les 2 premiers et 2 derniers *caractères*
        // (verbatim : deux `nextUtf8Char` pour le début ; itération
        // `s` jusqu'à NUL avec `i == lenUtf8 - 2` pour la fin).
        if let Some(rest1) = unicode::next_utf8_char(word_lc) {
            if let Some(rest2) = unicode::next_utf8_char(rest1) {
                let bytes_start = word_lc.len() - rest2.len();
                // `points.set (i + 1, 0)` pour `i < bytesStart`.
                for point in points.iter_mut().take(bytes_start + 1).skip(1) {
                    *point = 0;
                }

                let len_bytes = word_lc.len();
                let len_utf8 = unicode::num_utf8_chars(word_lc);
                let mut bytes_end = 0usize;

                // `for (i = 0; s; s = nextUtf8Char (s), i++)` — `cur`
                // pointe le caractère `i` ; l'offset octet est
                // `word_lc.len() - cur.len()`.
                let mut s: Option<&[u8]> = Some(word_lc);
                let mut i = 0usize;
                while let Some(cur) = s {
                    if cur.is_empty() {
                        break;
                    }
                    if len_utf8 >= 2 && i == len_utf8 - 2 {
                        bytes_end = len_bytes - (word_lc.len() - cur.len());
                    }
                    i += 1;
                    s = unicode::next_utf8_char(cur);
                }

                for i in 0..bytes_end.min(points.len().saturating_sub(2)) {
                    let idx = points.len() - 2 - i;
                    points[idx] = 0;
                }
            }
        }

        // Points impairs → positions de césure.
        let n = word_lc.len().min(points.len().saturating_sub(2));
        for i in 0..n {
            if points[i + 2] % 2 == 1 {
                break_pos.push(i as i32 + 1 + offset);
            }
        }
    }
}
