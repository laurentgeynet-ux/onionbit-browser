//! dillo-images — pipeline images incrémental.
//!
//! Cible du port : `src/{dicache.c,image.cc,imgbuf.cc}` et le glue
//! `gif.c`/`png.c`/`jpeg.c`/`webp.c`/`svg.c` vers des décodeurs
//! Rust purs (`gif`, `png`, `jpeg-decoder`, `image-webp`, `resvg`).
#![forbid(unsafe_code)]

pub mod dicache;
pub mod image;
pub mod imgbuf;
