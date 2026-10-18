//! Runner smoke — campagne de fuzz déterministe sur toolchain stable.
//!
//! libFuzzer n'est pas linkable sous Windows/MSVC (runtime sancov
//! absent des toolchains rustc) ; ce runner rejoue les mêmes
//! harnesses avec un mutateur PRNG seedé. Sous Linux/CI, utiliser
//! `cargo +nightly fuzz run <target>` pour la couverture guidée.
//!
//! Usage : `cargo run --release --bin smoke [iterations] [seed]`

use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

/// xorshift64* — PRNG déterministe, reproductible entre plateformes.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

/// Corpus de départ minimal — chaque seed vise un chemin distinct.
fn seeds(name: &str) -> Vec<Vec<u8>> {
    match name {
        "html" => vec![
            b"<html><body><p>hello".to_vec(),
            b"<table><tr><td>x<td>y</table>".to_vec(),
            b"<b><i><u>unclosed".to_vec(),
            b"<a href='x'><img src=y></a>".to_vec(),
            b"<!doctype html><frameset rows=*><frame src=x>".to_vec(),
            b"&amp;&#38;&#xzz;&bogus;<p \x00>".to_vec(),
            b"<style>a{color:red}</style><div style='x:y'>".to_vec(),
            b"<form><input type=text><select><option>o</form>".to_vec(),
        ],
        "css" => vec![
            b"a{color:red}".to_vec(),
            b"@media screen{p{margin:0}}".to_vec(),
            b"@import url(x); @charset 'utf-8';".to_vec(),
            b"p{content:'unclosed; margin:(1)}".to_vec(),
            b"*{font-size:1e10px !important}".to_vec(),
            b"div>x+y~z[a=b]:hover{background:url(\"f\")}".to_vec(),
        ],
        "decode" => vec![
            b"5\r\nhello\r\n0\r\n\r\n".to_vec(),
            b"1f8b0800".to_vec(),
            b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\xff".to_vec(),
            b"zzzz\r\nX\r\n0\r\n\r\n".to_vec(),
            b"\xff\xfe\xff\xfe bad utf8 \xc3\x28".to_vec(),
        ],
        "dpip" => vec![
            b"<cmd='auth' msg='deadbeef'>".to_vec(),
            b"<a cmd='DpiCmd' msg='x'>".to_vec(),
            b"<cmd='open_url' url='http://a/'>".to_vec(),
            b"<cmd='a' x='\\'>".to_vec(),
            b"<cmd='a' x='unterminated>".to_vec(),
        ],
        "files" => vec![
            b"a.b\tTRUE\t/\tFALSE\t0\tname\tvalue\n".to_vec(),
            b"DEFAULT DENY\n.a.b ACCEPT\n".to_vec(),
            b"a.b 0 0\n".to_vec(),
            b"max-age=31536000; includeSubDomains".to_vec(),
            b"a.b 1735689600 includeSubDomains\n".to_vec(),
        ],
        "url" => vec![
            b"http://a.b/c?d=e#f".to_vec(),
            b"../x/./y".to_vec(),
            b"//host/path".to_vec(),
            b"?q=%ff%2".to_vec(),
            b"http://a.b/%zz/%+f".to_vec(),
            b"javascript:alert(1)".to_vec(),
            b"mailto:a@b".to_vec(),
            b"\xff\xfe\x00".to_vec(),
        ],
        "images" => vec![
            // GIF89a 1×1 minimal.
            b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;".to_vec(),
            // PNG signature + IHDR partiel.
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06".to_vec(),
            // JPEG SOI + APP0.
            b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01".to_vec(),
            // RIFF WEBP VP8.
            b"RIFF\x1a\x00\x00\x00WEBPVP8 \x0e\x00\x00\x00".to_vec(),
            // SVG minimal.
            b"<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'><rect width='4' height='4' fill='red'/></svg>".to_vec(),
            // XPM.
            b"/* XPM */\nstatic char *x[]={\"2 2 1 1\",\"a c #ff0000\",\"aa\",\"aa\"};".to_vec(),
        ],
        _ => Vec::new(),
    }
}

/// Une mutation : flip, insert, delete, clone-section, ou seed brut.
fn mutate(rng: &mut Rng, corpus: &[Vec<u8>]) -> Vec<u8> {
    let mut buf = corpus[rng.below(corpus.len())].clone();
    match rng.below(6) {
        0 if !buf.is_empty() => {
            let i = rng.below(buf.len());
            buf[i] ^= 1 << rng.below(8);
        }
        1 => buf.insert(rng.below(buf.len() + 1), rng.next() as u8),
        2 if !buf.is_empty() => {
            buf.remove(rng.below(buf.len()));
        }
        3 if buf.len() > 1 => {
            let start = rng.below(buf.len());
            let end = (start + rng.below(64) + 1).min(buf.len());
            let piece: Vec<u8> = buf[start..end].to_vec();
            let at = rng.below(buf.len() + 1);
            buf.splice(at..at, piece);
        }
        4 => {
            // Byte-run aléatoire ajouté.
            let n = rng.below(32);
            for _ in 0..n {
                buf.push(rng.next() as u8);
            }
        }
        _ if buf.len() > 2 => buf.truncate(rng.below(buf.len())),
        _ => {}
    }
    buf.truncate(4096); // borne anti-explosion mémoire
    buf
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let iters: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let seed: u64 = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x64_6c_6c_6f); // "dllo"

    let targets: Vec<(&str, fn(&[u8]))> = vec![
        ("html", dillo_fuzz::html),
        ("css", dillo_fuzz::css),
        ("decode", dillo_fuzz::decode),
        ("dpip", dillo_fuzz::dpip),
        ("files", dillo_fuzz::files),
        ("url", dillo_fuzz::url),
        ("images", dillo_fuzz::images),
    ];

    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("artifacts");
    let mut failures = 0usize;

    for (name, harness) in &targets {
        let mut rng = Rng(seed ^ (name.len() as u64 * 0x9e37_79b9));
        let corpus = seeds(name);
        let mut crashes = 0usize;
        for i in 0..iters {
            let input = mutate(&mut rng, &corpus);
            if catch_unwind(AssertUnwindSafe(|| harness(&input))).is_err() {
                crashes += 1;
                let dir = artifacts.join(name);
                if fs::create_dir_all(&dir).is_ok() {
                    let _ = fs::write(dir.join(format!("crash-{seed}-{i}.bin")), &input);
                }
                if crashes >= 8 {
                    eprintln!("{name}: too many crashes, stopping early");
                    break;
                }
            }
        }
        println!("{name}: {iters} iters, {crashes} crashes");
        failures += crashes;
    }

    if failures > 0 {
        eprintln!("{failures} crash artifacts under {}", artifacts.display());
        std::process::exit(1);
    }
    println!("all targets clean");
}
