#![no_main]

//! `html.cc` via `DilloHtml::write` incrémental — voir
//! `dillo_fuzz::html`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| dillo_fuzz::html(data));
