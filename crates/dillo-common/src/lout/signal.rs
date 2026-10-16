//! Port of `lout/signal.hh` + `lout/signal.cc`.
//!
//! `Emitter`/`Receiver` C++ → [`Emitter<R>`] générique tenant une
//! liste de `Weak<RefCell<R>>` :
//!
//! - `connect` : `Rc` partagé — le récepteur disparaît → la connexion
//!   meurt avec lui (équivalent de `~Receiver` → unconnect amont).
//! - `emitVoid`/`emitBool` : itération sur **tous** les récepteurs
//!   vivants ; `emit_bool` retourne la disjonction **sans court-
//!   circuit** (note amont `signal.cc:97` préservée).
//! - `ObservedObject`/`DeletionEmitter` → non porté : en Rust, le
//!   `Drop` du récepteur supprime la connexion (le `Weak` expire) —
//!   pas besoin d'un signal explicite de destruction.
//!
//! Limite assumée (documentée ADR-0005) : l'émission emprunte chaque
//! récepteur en `borrow_mut` — un récepteur qui réémet vers lui-même
//! paniquerait ; le prototype widget tranchera le pattern final.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// `signal::Emitter` typé : émet vers des `Rc<RefCell<R>>`.
/// `R` est le trait concret du groupe de signaux (ex. un trait
/// `DeletionReceiver`), pas un `Object` générique.
pub struct Emitter<R: ?Sized> {
    receivers: Vec<Weak<RefCell<R>>>,
}

impl<R: ?Sized> Default for Emitter<R> {
    fn default() -> Self {
        Self {
            receivers: Vec::new(),
        }
    }
}

impl<R: ?Sized> Emitter<R> {
    /// `Emitter()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `connect(receiver)` — enregistre une référence faible.
    /// Plusieurs connexions du même récepteur = plusieurs appels
    /// (amont permet le multi-connect).
    pub fn connect(&mut self, receiver: &Rc<RefCell<R>>) {
        self.receivers.push(Rc::downgrade(receiver));
    }

    /// `unconnect` — retire les références faibles vers `receiver`.
    pub fn unconnect(&mut self, receiver: &Rc<RefCell<R>>) {
        self.receivers
            .retain(|w| !w.ptr_eq(&Rc::downgrade(receiver)));
    }

    /// Purge les `Weak` expirés (les `~Receiver` amont qui se
    /// déconnectaient eux-mêmes).
    fn prune(&mut self) {
        self.receivers.retain(|w| w.strong_count() > 0);
    }

    /// Nombre de récepteurs vivants (diagnostic/tests).
    #[must_use]
    pub fn receiver_count(&mut self) -> usize {
        self.prune();
        self.receivers.len()
    }

    /// `emitVoid` — appelle `f` sur chaque récepteur vivant, dans
    /// l'ordre de connexion.
    pub fn emit_void(&mut self, mut f: impl FnMut(&mut R)) {
        for w in &self.receivers {
            if let Some(rc) = w.upgrade() {
                f(&mut rc.borrow_mut());
            }
        }
    }

    /// `emitBool` — disjonction des retours ; **tous les récepteurs
    /// sont appelés** même après un `true` (reproduit la note
    /// `signal.cc:97` : pas de court-circuit `||`).
    pub fn emit_bool(&mut self, mut f: impl FnMut(&mut R) -> bool) -> bool {
        let mut b = false;
        for w in &self.receivers {
            if let Some(rc) = w.upgrade() {
                let bt = f(&mut rc.borrow_mut());
                b = b || bt;
            }
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct CountReceiver {
        hits: Cell<usize>,
        ret: bool,
    }

    #[test]
    fn emit_bool_calls_all_receivers() {
        // Reproduit la sémantique amont : pas de court-circuit.
        let mut e: Emitter<CountReceiver> = Emitter::new();
        let r1 = Rc::new(RefCell::new(CountReceiver {
            hits: Cell::new(0),
            ret: true,
        }));
        let r2 = Rc::new(RefCell::new(CountReceiver {
            hits: Cell::new(0),
            ret: false,
        }));
        e.connect(&r1);
        e.connect(&r2);
        let got = e.emit_bool(|r| {
            r.hits.set(r.hits.get() + 1);
            r.ret
        });
        assert!(got);
        assert_eq!(r1.borrow().hits.get(), 1);
        assert_eq!(r2.borrow().hits.get(), 1); // appelé malgré r1==true
    }

    #[test]
    fn weak_disconnect_on_drop() {
        let mut e: Emitter<CountReceiver> = Emitter::new();
        let r1 = Rc::new(RefCell::new(CountReceiver {
            hits: Cell::new(0),
            ret: true,
        }));
        e.connect(&r1);
        assert_eq!(e.receiver_count(), 1);
        drop(r1);
        assert_eq!(e.receiver_count(), 0);
    }

    #[test]
    fn emit_void_order() {
        let mut e: Emitter<Cell<usize>> = Emitter::new();
        let a = Rc::new(RefCell::new(Cell::new(0)));
        let b = Rc::new(RefCell::new(Cell::new(0)));
        e.connect(&a);
        e.connect(&b);
        let mut order = Vec::new();
        e.emit_void(|c| {
            c.set(order.len() + 1);
            order.push(c.get());
        });
        assert_eq!(a.borrow().get(), 1);
        assert_eq!(b.borrow().get(), 2);
    }
}
