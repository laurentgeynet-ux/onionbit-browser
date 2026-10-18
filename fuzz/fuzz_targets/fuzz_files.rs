#![no_main]

//! `cookiesrc`/`hsts`/`domainrc` — voir `dillo_fuzz::files`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| dillo_fuzz::files(data));
