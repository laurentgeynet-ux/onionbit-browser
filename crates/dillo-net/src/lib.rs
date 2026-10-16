//! dillo-net — couche I/O.
//!
//! Cible du port : `src/IO/` (IO, iowatch, http, mime, proto, about,
//! dpi client, tls→rustls) et `src/{url,dns,decode,chain}.c`.
#![forbid(unsafe_code)]

pub mod about;
pub mod chain;
pub mod decode;
pub mod dns;
pub mod fetch;
pub mod io;
pub mod mime;
pub mod tls;
pub mod url;
