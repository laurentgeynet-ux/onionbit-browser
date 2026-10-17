//! dillo-html — parseur HTML tag-soup tolérant et arbre de document.
//!
//! Cible du port : `src/html.cc`, `html_common.hh`,
//! `html_charrefs.h`, `doctree.hh`, `plain.cc`, `table.cc`,
//! `form.cc`.
#![forbid(unsafe_code)]

pub mod common;
pub mod entities;
mod entities_table;
pub mod form;
pub mod html;
pub mod minidoc;
pub mod plain;
pub mod sink;
pub mod table;
pub mod tags;
#[cfg(test)]
mod tests;
