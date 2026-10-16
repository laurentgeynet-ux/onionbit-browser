//! dillo-dpi — protocole dpi, démon et plugins.
//!
//! Cible du port : `dpip/dpip.c` (framing byte-exact), `dpid/*`
//! (démon, socket dir), `dpi/*` (plugins). Interop C↔Rust exigée
//! (ADR-0004).
#![forbid(unsafe_code)]
