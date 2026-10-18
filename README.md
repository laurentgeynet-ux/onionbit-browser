# OnionBit-Browser (DilloRust)

A **semantically faithful port of the [Dillo](https://dillo-browser.github.io/)
web browser to 100% native Rust** — no C/C++ dependency in the default
build, `unsafe` forbidden in all first-party crates.

## Origin — born for OnionBit

This project exists because of **[OnionBit](../Tribler-Rust-Torrent)**,
a portable, anonymous, encrypted "swiss army knife" for nomadic users:
file sharing over a multi-hop IPv8 overlay, end-to-end encrypted
messaging, portable identity, encrypted vault and database, `.onion`
services — a suite designed to leave no trace.

OnionBit needed a browser to complete it: a **minimal `.onion`
document/service reader** that could ship inside a portable bundle,
connect through Tor without ever leaking, and hold its own on modest
hardware. Mainstream browsers were out of the question — hundreds of
megabytes, gigantic attack surface, impossible to audit line-by-line.
So instead of embedding someone else's monster, OnionBit spawned its
own browser: a faithful Rust port of Dillo, the smallest real browser
there is — audited module by module until the whole engine was
reproduced.

The result stands on its own: a complete, working, standalone browser
anyone can use. But its DNA is OnionBit's — small footprint, no
JavaScript, a confinable network path, and security properties that
were *measured*, not asserted.

The cargo package keeps the upstream-faithful name `dillo`.

## What it is (and is not)

Dillo is a deliberately small, fast, document-oriented browser: HTML
rendering, CSS, images, cookies, bookmarks, forms — **no JavaScript**,
no modern web platform. DilloRust preserves that contract: it renders
real pages quickly and cheaply, it does not try to be Chrome.

| Property | Value |
|---|---|
| Binary size | ~9.2 MiB (release) |
| Idle memory | ~40 MiB |
| Rendering | ~34 ms/page median on the golden corpus |
| Safety | `#![forbid(unsafe_code)]` in all first-party crates |
| TLS | `rustls` + `ring` (pure-Rust crypto path, no BoringSSL build) |
| UI | `winit` + `softbuffer` + `tiny-skia` (replaces FLTK) |
| Platforms | Windows and Linux |

## Port status — complete

The whole browser is ported: parser, CSS engine, `dw` layout core
(textblock, tables, floats/OOF), image decoders (gif/png/jpeg/webp/svg),
network chain (DNS, HTTP, TLS, dpi protocol), platform UI, browser glue
(history, bookmarks, prefs, dialogs), and the `dpid` plugin daemon with
its 8 plugins.

- **12/12 semantic parity audits** against upstream Dillo C/C++ —
  reports in `docs/audits/reports/`, deviations documented in
  `docs/PORTING.md`.
- **Golden render corpus**: pixel-compared pages covering tables,
  floats, fonts, malformed markup — *rendering is the spec*.
- **Fuzzing**: all tolerant input surfaces covered (HTML, CSS, decode,
  dpi protocol, config files, URLs, images) — 5 real bugs found and
  fixed this way.
- Honest caveat: parity is demonstrated *on the reference corpus* —
  live sites remain a discovery surface, by nature.

See `docs/PORTING.md` for the file-by-file mapping and `ROADMAP.md` for
the full history.

## Build & run

```sh
cargo build --release -p dillo
./target/release/onionbit-browser        # dillo.exe name on Windows: onionbit-browser.exe
```

Standalone, it behaves like stock Dillo: plugins (`dpid`), cookies,
bookmarks — nothing is disabled. Configuration lives in
`~/.dillo/dillorc` (`%USERPROFILE%\.dillo` on Windows).

Packaging scripts produce a self-contained archive with `dpid` and all
plugins in the upstream layout:

```sh
packaging/package.sh   # Linux: dist/ + tar.gz
packaging/package.ps1  # Windows: dist/ + zip
```

## OnionBit integration (optional, dormant)

The binary ships two documented extensions consumed by the OnionBit
daemon — they are **inert without an OnionBit-generated profile**:

- `http_proxypass` dillorc pref — proxy auth without a password dialog
  (upstream has `http_proxyuser` + interactive dialog only);
- `no_dpi` dillorc pref — fully confines the dpi subsystem for the
  `.onion` browsing profile (no plugin can open its own socket).

## Layout

```
crates/
  dillo-common     dlib + lout utilities (unicode, containers, events)
  dillo-net        URL, DNS, IO chain, HTTP, TLS, dpi client
  dillo-css        CSS parser + style engine
  dillo-html       tag-soup HTML parser + doctree, forms, plain
  dillo-dw         layout core: widgets, textblock, tables, floats
  dillo-images     imgbuf, dicache, decoders
  dillo-platform   winit/softbuffer/tiny-skia platform layer
  dillo-browser    cache, web, nav, history, prefs, ui glue
  dillo-dpi        dpip protocol + dpi plugins + dpid daemon
  dillo            facade + binaries (onionbit-browser, dilloc)
```

## Contributing

`AGENTS.md` is the working contract (fidelity rules, porting workflow,
audit methodology). Before committing:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

## License

GPL-3.0-or-later, as upstream Dillo. See `LICENSE`.
