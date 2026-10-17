//! dillo-net — couche I/O.
//!
//! Cible du port : `src/IO/` (IO, iowatch, http, mime, proto, about,
//! dpi client, tls→rustls) et `src/{url,dns,decode,chain}.c`.
#![forbid(unsafe_code)]

pub mod about;
pub mod auth;
pub mod cache;
pub mod capi;
pub mod chain;
pub mod control;
pub mod cookies;
pub mod decode;
pub mod digest;
pub mod dns;
pub mod domain;
pub mod dpip;
pub mod fetch;
pub mod history;
pub mod hsts;
pub mod http;
pub mod io;
pub mod mime;
pub mod timeout;
pub mod tls;
pub mod url;
pub mod web;
