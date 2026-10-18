# Fuzzing — `fuzz/`

Harnesses de fuzz sur les entrées tolérantes du port. Propriété
invariante : **jamais de panic** — Dillo dégrade sur toute entrée
malformée (§4 règle 3).

## Cibles

| Target | Surface |
|---|---|
| `fuzz_html` | `html.cc` via `DilloHtml::write` — livraison cumulative par chunks ≤ 64 o (pipeline réelle parser→style→layout, `SkiaPlatform`, sink mock) |
| `fuzz_css` | `cssparser.cc` — `CssParser::parse` (feuille, `@`-directives) + `parse_declaration_block` (`style="…"`) |
| `fuzz_decode` | `decode.c` — `DecodeTransfer` chunked, `content_init` gzip/x-gzip/deflate/br, `charset_init` tous labels connus + label inconnu |
| `fuzz_dpip` | `dpip.c` — `get_attr_l` (tag wire byte-exact, quoting `'`/`"`/échappements) + round-trip `build_cmd`/`get_attr` |
| `fuzz_files` | `cookiesrc` + `cookies_get_query`, `hsts` preload + header `Strict-Transport-Security`, `domainrc` + `domain_permit` |
| `fuzz_url` | `url.c` — parse lenient, résolution relative, `decode/encode_hex_str`, `url_cmp` |
| `fuzz_images` | `gif/png/jpeg/webp/svg` via la vraie boucle `dicache_callback` (feed cumulatif `Client->Buf`, `CA_Send`/`CA_Close`), `xpm_decode` (icônes `ui.cc`), blits `Imgbuf` |

Les corps des cibles vivent dans `fuzz/src/lib.rs` (fonctions
`dillo_fuzz::*`); `fuzz_targets/*.rs` ne sont que des coquilles
`fuzz_target!`.

## Exécution

### Runner `smoke` (stable — Windows inclus)

libFuzzer n'est pas linkable sous MSVC (les toolchains rustc ne
livrent pas `librustc-nightly_rt.asan`/le runtime sancov) ni sous
gnullvm sans clang externe. Le runner `smoke` rejoue les mêmes
harnesses avec un mutateur xorshift64* seedé — déterministe,
reproductible, `catch_unwind` + artifacts :

```sh
cd fuzz
cargo run --release --bin smoke [iterations] [seed]   # défaut 20000
```

Crashes → `fuzz/artifacts/<target>/crash-<seed>-<iter>.bin`.
Rejouer un artifact : `cargo run --release --bin replay <target> <file>`
(target ∈ html|css|decode|dpip|files|url).

### libFuzzer (Linux / CI)

```sh
cargo +nightly fuzz run fuzz_html   # etc.
```

Sous Windows, utiliser `smoke` ; les cibles `fuzz_target!` restent
prêtes pour les environnements où le runtime sanitizer existe.

## Bugs trouvés (première campagne)

- `dillo-html/src/html.rs` — `DilloHtml::write` : `start_ofs` >
  `buf.len()` → slice panic. Fix : clamp `base` (le contrat cumulatif
  `Client->Buf` reste requis, mais la tolérance interdit le panic).
- `dillo-platform` — `cosmic-text` `shape.rs:813` assertait sur les
  lignes contenant FS/GS/RS/NEL/LS/PS (`BidiInfo` reconnaît toute la
  classe bidi B, le splitter de lignes cosmic non). Fix :
  `sanitize_shape_text` remplace les séparateurs classe B par des
  espaces avant `Buffer::set_text` (panique absente amont —
  `fl_width`/`fl_draw` n'ont pas de notion de paragraphe).
- `dillo-net/src/decode.rs` — `charset_process` : `read` d'encoding_rs
  compte déjà la séquence malformée ; le `pos += 1` supplémentaire
  (≙ `inPtr++` iconv) dépassait `leftover.len()` au `drain` → panic.
  Régression : `charset_malformed_tail_no_panic`.
