//! dillo-platform — implémentation de `dw::core::Platform`.
//!
//! Remplace `dw/fltk*.cc` : surface `winit` + `softbuffer`,
//! raster `tiny-skia`, fontes `cosmic-text` (ADR-0002).
#![forbid(unsafe_code)]

pub mod app;
pub mod golden;
pub mod pixmaps;
pub mod platform;
pub mod surface;
pub mod ui;
pub mod view;

// Ré-exports pour les consommateurs de chrome (dillo-browser `ui.rs`)
// — une seule source de versions (ADR-0002).
pub use cosmic_text;
pub use tiny_skia;
pub use winit;
