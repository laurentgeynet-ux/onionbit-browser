#![no_main]

//! `gif/png/jpeg/webp/svg` via `dicache` + `xpm_decode` + `Imgbuf` —
//! voir `dillo_fuzz::images`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| dillo_fuzz::images(data));
