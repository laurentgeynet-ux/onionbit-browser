//! `lout/` — utilitaires C++ du moteur dw.
//!
//! | Amont | Rust | Statut |
//! |---|---|---|
//! | `misc.{hh,cc}` | [`misc`] | porté (sauf `ZoneAllocator`) |
//! | `unicode.{hh,cc}` | [`unicode`] | porté |
//! | `object.{hh,cc}` | [`object`] | porté (trait + enum `Value`) |
//! | `signal.{hh,cc}` | [`signal`] | porté (`Emitter<R>` + `Weak`) |
//! | `identity.{hh,cc}` | [`identity`] | porté (trait `Identifiable`) |
//! | `container.{hh,cc}` | collections std | won't port — `Vec`/`HashMap`/`VecDeque` remplacent les containers non-typés |
//! | `debug.hh`, `debug_rtfl.hh`, `msg.h` | `tracing` | won't port — macros de debug |

pub mod identity;
pub mod misc;
pub mod object;
pub mod signal;
pub mod unicode;
