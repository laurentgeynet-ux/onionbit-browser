//! dillo-dw — moteur de layout platform-agnostique.
//!
//! Cible du port : `dw/*` hors `fltk*` (types, style, widget,
//! layout, textblock, tables, OOF/floats, iterators, selection,
//! findtext, hyphenator, image/ui).
//!
//! Le modèle de propriété de l'arbre de widgets est fixé par
//! l'ADR-0005 (prototype `dillo-dw-proto`) : arène plate +
//! `WidgetId`, `enum`-dispatch des kinds, événements drainés
//! après mutation — jamais de `&mut` sur le `kind` d'un nœud
//! pendant un appel récursif.
#![forbid(unsafe_code)]

pub mod bullet;
pub mod events;
pub mod findtext;
pub mod hyphenator;
pub mod image;
pub mod imgbuf;
pub mod imgrenderer;
pub mod iterator;
pub mod layout;
pub mod oof;
pub mod platform;
pub mod ruler;
pub mod selection;
pub mod style;
pub mod textblock;
pub mod tools;
pub mod types;
pub mod ui;
pub mod view;
pub mod widget;

/// Index dans l'arène de widgets (≙ `dw::core::Widget*`
/// amont). Le proto ADR-0005 utilise `usize` ; le vrai `dw/`
/// conserve ce choix.
pub type WidgetId = usize;
