//! Port de `dw/imgrenderer.hh` et `dw/imgrenderer.cc`.
//!
//! `ImgRenderer` — interface recevant les notifications du
//! décodage (`setBuffer`/`drawRow`/`finish`/`fatal`).
//! `ImgRendererDist` distribue les appels à un ensemble de
//! renderers enfants (ordre indéfini amont — `HashSet` ; ici
//! `Vec`, documenté).
//!
//! `Imgbuf*` amont → `Rc<dyn Imgbuf>` ; `ImgRenderer*` enfants
//! → `Rc<RefCell<dyn ImgRenderer>>` (itération sur snapshot de
//! `Rc` : un enfant peut muter le dist pendant un callback sans
//! violer les emprunts — amont tolère la mutation du `HashSet`
//! en cours d'itération, de toute façon non déterministe).

use std::cell::RefCell;
use std::rc::Rc;

use crate::imgbuf::Imgbuf;

/// `dw::core::ImgRenderer`.
pub trait ImgRenderer: 'static {
    /// `setBuffer (buffer, resize)` — buffer attaché (métadonnées
    /// lues).
    fn set_buffer(&mut self, buffer: &Rc<dyn Imgbuf>, resize: bool);
    /// `drawRow (row)` — une ligne est disponible dans le buffer.
    fn draw_row(&mut self, row: i32);
    /// `finish` — toutes les données reçues.
    fn finish(&mut self);
    /// `fatal` — erreur de récupération.
    fn fatal(&mut self);
    /// Downcast pour les données spécifiques (`setData` des
    /// renderers textblock — adaptation modèle D : amont tient un
    /// pointeur `WordImgRenderer*` typé). Méthode requise
    /// (object-safety : pas de défaut possible).
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// `dw::core::ImgRendererDist` — distribue les appels à un
/// ensemble d'enfants.
#[derive(Default)]
pub struct ImgRendererDist {
    children: Vec<Rc<RefCell<dyn ImgRenderer>>>,
}

impl ImgRendererDist {
    /// `ImgRendererDist ()`.
    #[must_use]
    pub fn new() -> Self {
        ImgRendererDist::default()
    }

    /// `put (child)` — déduplication par identité pointeur
    /// (≙ `HashSet<TypedPointer>`).
    pub fn put(&mut self, child: &Rc<RefCell<dyn ImgRenderer>>) {
        if !self.children.iter().any(|c| Rc::ptr_eq(c, child)) {
            self.children.push(Rc::clone(child));
        }
    }

    /// `remove (child)`.
    pub fn remove(&mut self, child: &Rc<RefCell<dyn ImgRenderer>>) {
        self.children.retain(|c| !Rc::ptr_eq(c, child));
    }

    /// Snapshot de `Rc` pour itérer sans tenir d'emprunt sur le
    /// dist pendant les callbacks.
    fn snapshot(&self) -> Vec<Rc<RefCell<dyn ImgRenderer>>> {
        self.children.clone()
    }
}

impl ImgRenderer for ImgRendererDist {
    /// `ImgRendererDist::setBuffer`.
    fn set_buffer(&mut self, buffer: &Rc<dyn Imgbuf>, resize: bool) {
        for child in self.snapshot() {
            child.borrow_mut().set_buffer(buffer, resize);
        }
    }

    /// `ImgRendererDist::drawRow`.
    fn draw_row(&mut self, row: i32) {
        for child in self.snapshot() {
            child.borrow_mut().draw_row(row);
        }
    }

    /// `ImgRendererDist::finish`.
    fn finish(&mut self) {
        for child in self.snapshot() {
            child.borrow_mut().finish();
        }
    }

    /// `ImgRendererDist::fatal`.
    fn fatal(&mut self) {
        for child in self.snapshot() {
            child.borrow_mut().fatal();
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Counting {
        rows: Cell<i32>,
        finished: Cell<bool>,
    }

    impl ImgRenderer for Counting {
        fn set_buffer(&mut self, _buffer: &Rc<dyn Imgbuf>, _resize: bool) {}
        fn draw_row(&mut self, _row: i32) {
            self.rows.set(self.rows.get() + 1);
        }
        fn finish(&mut self) {
            self.finished.set(true);
        }
        fn fatal(&mut self) {}
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    #[test]
    fn dist_forwards_and_dedups() {
        let mut d = ImgRendererDist::new();
        let concrete = Rc::new(RefCell::new(Counting {
            rows: Cell::new(0),
            finished: Cell::new(false),
        }));
        let c: Rc<RefCell<dyn ImgRenderer>> = concrete.clone();
        d.put(&c);
        d.put(&c); // même pointeur → pas de doublon
        d.draw_row(3);
        assert_eq!(concrete.borrow().rows.get(), 1);
        d.finish();
        assert!(concrete.borrow().finished.get());
        d.remove(&c);
        d.draw_row(4);
        assert_eq!(concrete.borrow().rows.get(), 1);
    }
}
