//! Port of `lout/object.hh` + `lout/object.cc`.
//!
//! Adaptation Rust idiomatique : la hiérarchie `Object`/`Comparable`/
//! `Pointer`/`Integer`/`Boolean`/`ConstString`/`String`/`Pair`
//! existait pour typer des arguments « pliés » des signaux et des
//! containers non-typés. Ici :
//!
//! - [`Object`] : trait de base (`equals`, `hash_value`,
//!   `into_string_buffer`) pour les types portés qui en ont besoin.
//! - [`Value`] : enum remplaçant les wrappers `Pointer`/`Integer`/
//!   `Boolean`/`ConstString`/`String`/`Pair` des argv de signaux.
//!
//! `Comparator`/`standardComparator` → `Ord` / fonctions de
//! comparaison (`Comparator` n'existait que pour le C non-typé).

use super::misc::StringBuffer;
use std::cmp::Ordering;

/// `object::Object` — méthodes communes des objets lout.
pub trait Object {
    /// `equals` — amont suppose même classe des deux côtés.
    fn equals(&self, other: &Self) -> bool
    where
        Self: Sized,
    {
        let _ = other;
        false
    }

    /// `hashValue`.
    fn hash_value(&self) -> i32 {
        0
    }

    /// `intoStringBuffer` — représentation texte (renommé `to_*` :
    /// `&self`, pas de consommation).
    fn to_string_buffer(&self, sb: &mut StringBuffer);
}

/// `object::Comparable` — objets ordonnables (`compareTo`).
pub trait Comparable: Object {
    /// `compareTo` — <0 / 0 / >0 comme amont (`Ordering` en Rust).
    fn compare_to(&self, other: &Self) -> Ordering;
}

/// `Value` — remplace les wrappers `object::Pointer`, `Integer`,
/// `Boolean`, `ConstString`, `String`, `Pair`/`TypedPair` utilisés
/// pour « plier » les arguments d'un signal (argv `Object**` amont).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    /// `object::Pointer`/`TypedPointer` — adresse brute (opaque).
    Pointer(usize),
    /// `object::Integer`.
    Integer(i32),
    /// `object::Boolean`.
    Boolean(bool),
    /// `object::ConstString`/`object::String` — le const/non-const
    /// disparaît (la chaîne est possédée).
    Str(Vec<u8>),
    /// `object::Pair`/`TypedPair`.
    Pair(Box<Value>, Box<Value>),
}

impl Object for Value {
    fn equals(&self, other: &Self) -> bool {
        self == other
    }

    // `allow` : l'adresse tronquée est le hash amont (Pointer::
    // hashValue retourne `(int)p`) — comportement voulu.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    fn hash_value(&self) -> i32 {
        // Amont : Pointer→adresse, Integer→valeur, ConstString→hash
        // de la chaîne. On conserve les mêmes idées.
        match self {
            Self::Pointer(p) => *p as i32,
            Self::Integer(v) => *v,
            Self::Boolean(b) => i32::from(*b),
            Self::Str(s) => hash_str(s),
            Self::Pair(a, b) => a.hash_value() ^ b.hash_value().rotate_left(16),
        }
    }

    fn to_string_buffer(&self, sb: &mut StringBuffer) {
        match self {
            Self::Pointer(p) => {
                sb.append("<pointer ");
                sb.append(&format!("{p:#x}"));
                sb.append(">");
            }
            Self::Integer(v) => {
                sb.append("<integer ");
                sb.append_int(*v);
                sb.append(">");
            }
            Self::Boolean(b) => {
                sb.append("<boolean ");
                sb.append_bool(*b);
                sb.append(">");
            }
            Self::Str(s) => {
                sb.append("<string \"");
                sb.append(&String::from_utf8_lossy(s));
                sb.append("\">");
            }
            Self::Pair(a, b) => {
                sb.append("<pair ");
                a.to_string_buffer(sb);
                sb.append(", ");
                b.to_string_buffer(sb);
                sb.append(">");
            }
        }
    }
}

impl Comparable for Value {
    fn compare_to(&self, other: &Self) -> Ordering {
        // amont : compareTo supposait même classe ; on ordonne par
        // variante puis par valeur (Ord dérivé).
        self.cmp(other)
    }
}

/// `ConstString::hashValue` — hash de chaîne amont
/// (`lout/object.cc`) : `h = h*33 + c` (djb2-like). Reproduit pour
/// la parité des tables de hachage portées.
#[must_use]
pub fn hash_str(s: &[u8]) -> i32 {
    let mut h: i32 = 5381;
    for &c in s {
        h = h.wrapping_mul(33).wrapping_add(i32::from(c));
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_wrappers() {
        assert!(Value::Integer(3).equals(&Value::Integer(3)));
        assert!(!Value::Integer(3).equals(&Value::Boolean(true)));
        assert_eq!(
            Value::Integer(3).compare_to(&Value::Integer(5)),
            Ordering::Less
        );
        // cross-variant : défini (ordre de déclaration des variantes)
        assert_ne!(
            Value::Boolean(true).compare_to(&Value::Integer(0)),
            Ordering::Equal
        );
    }

    #[test]
    fn value_to_string() {
        let mut sb = StringBuffer::new();
        Value::Pair(
            Box::new(Value::Integer(1)),
            Box::new(Value::Str(b"ab".to_vec())),
        )
        .to_string_buffer(&mut sb);
        assert_eq!(sb.get_chars(), "<pair <integer 1>, <string \"ab\">>");
    }

    #[test]
    fn hash_str_stable() {
        assert_eq!(hash_str(b""), 5381);
        assert_eq!(
            hash_str(b"abc"),
            5381 * 33 * 33 * 33 + 97 * 33 * 33 + 98 * 33 + 99
        );
    }
}
