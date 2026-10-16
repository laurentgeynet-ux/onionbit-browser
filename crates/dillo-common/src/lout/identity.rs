//! Port of `lout/identity.hh` + `lout/identity.cc`.
//!
//! `IdentifiableObject` fournissait un RTTI maison (`CLASS_ID`
//! enregistrés + chaîne de parents pour `instanceOf`). En Rust,
//! `TypeId`/`Any` couvre le type exact ; la hiérarchie de classes
//! se traduit par une liste d'ancêtres déclarée par l'implémenteur
//! (les widgets dw seront de toute façon un `enum` — `matches!`
//! suffira souvent, cf. ADR-0005).

/// `Identifiable` — équivalent de `identity::IdentifiableObject`.
pub trait Identifiable {
    /// `getClassName` — nom enregistré (convention amont : nom
    /// qualifié, ex. `"dw::Textblock"`).
    fn class_name(&self) -> &'static str;

    /// Noms des ancêtres du plus proche au plus éloigné — reflète
    /// la chaîne `parent` construite par `registerName` amont.
    /// À implémenter par les classes non-feuilles.
    fn ancestors(&self) -> &'static [&'static str] {
        &[]
    }

    /// `instanceOf(className)` — vrai si `self` est de cette classe
    /// ou d'une sous-classe (remonte la chaîne comme amont).
    fn instance_of(&self, class_name: &str) -> bool {
        self.class_name() == class_name || self.ancestors().contains(&class_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Textblock;
    impl Identifiable for Textblock {
        fn class_name(&self) -> &'static str {
            "dw::Textblock"
        }
        fn ancestors(&self) -> &'static [&'static str] {
            &["dw::Widget", "dw::core::Widget"]
        }
    }

    struct Bullet;
    impl Identifiable for Bullet {
        fn class_name(&self) -> &'static str {
            "dw::Bullet"
        }
    }

    #[test]
    fn instance_of_chain() {
        let t = Textblock;
        assert!(t.instance_of("dw::Textblock")); // type exact
        assert!(t.instance_of("dw::Widget")); // ancêtre
        assert!(!t.instance_of("dw::Table")); // non lié
        let b = Bullet;
        assert!(!b.instance_of("dw::Widget")); // ancêtres non déclarés
    }
}
