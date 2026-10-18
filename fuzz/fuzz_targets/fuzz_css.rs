#![no_main]

//! `cssparser.cc` feuille + declaration-block — voir
//! `dillo_fuzz::css`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| dillo_fuzz::css(data));
