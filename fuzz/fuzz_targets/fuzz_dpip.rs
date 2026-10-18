#![no_main]

//! `dpip.c` parseur de tag wire + round-trip — voir
//! `dillo_fuzz::dpip`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| dillo_fuzz::dpip(data));
