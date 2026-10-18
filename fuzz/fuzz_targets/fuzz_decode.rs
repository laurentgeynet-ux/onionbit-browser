#![no_main]

//! `decode.c` transfer/content/charset — voir `dillo_fuzz::decode`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| dillo_fuzz::decode(data));
