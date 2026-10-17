//! dillo-common — primitives partagées.
//!
//! Cible du port : `dlib/dlib.c` (Dstr, Dlist, helpers `d_*`),
//! `lout/` (container, misc, unicode, identity, object, signal) et
//! les utilitaires transverses de `src/` (`utf8.cc`, `misc.c`,
//! `colors.c`).
#![forbid(unsafe_code)]

pub mod bitvec;
pub mod colors;
pub mod dlib;
pub mod klist;
pub mod lout;
pub mod misc;
pub mod utf8;
