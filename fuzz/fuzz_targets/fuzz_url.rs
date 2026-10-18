#![no_main]

//! `url.c` parse/résolution/percent-codecs — voir `dillo_fuzz::url`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| dillo_fuzz::url(data));
