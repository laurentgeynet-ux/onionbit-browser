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
pub mod dpi;
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

/// `fgets(line, LINE_MAXLEN, stream)` (`LINE_MAXLEN` = 4096 amont) :
/// chaque « ligne » fait ≤ 4095 octets — le reste d'une ligne longue
/// est relu comme ligne suivante (`cookiesrc`, `hsts` preload).
/// La borne tombe sur une frontière UTF-8 (amont coupe à l'octet ;
/// `dStrstrip` rend le fragment invalide inerte de toute façon).
pub fn fgets_lines(content: &str) -> impl Iterator<Item = &str> {
    content.split('\n').flat_map(|line| {
        let mut chunks = Vec::new();
        let mut rest = line;
        while rest.len() > 4095 {
            let mut n = 4095;
            while !rest.is_char_boundary(n) {
                n -= 1;
            }
            chunks.push(&rest[..n]);
            rest = &rest[n..];
        }
        chunks.push(rest);
        chunks
    })
}
