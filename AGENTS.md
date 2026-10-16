# AGENTS.md — Guide for coding agents and contributors

DilloRust is a **semantically faithful port of the Dillo web browser
(C/C++) to 100% native Rust**. This file is the working contract for
anyone — human or AI — contributing code. Read it fully before
touching anything.

## 1. Mission and ground truth

- **Reference implementation**: Dillo @ `D:\Projet\dillo`
  (~130 000 LOC : ~50k `src/`, ~36k `dw/`, ~6.5k `src/IO/`,
  ~4.5k `lout/`, ~1.2k `dlib/`, ~10k `dpi`/`dpid`/`dpip`).
  This checkout is the single source of truth — **read-only, never
  modify, never copy wholesale**.
- **Fidelity rule**: match *observable behavior* of the C code —
  parsing tolerance, layout results, wire formats (HTTP, dpi
  protocol), file formats (dillorc, bookmarks, cookies, hsts) — not
  its line-by-line structure. We write idiomatic Rust, we do not
  transliterate C.
- **Rendering is the spec**: the port is correct when `test/html/render`
  pages produce pixel-equivalent layout and when upstream HTML torture
  tests (tables, floats, malformed markup) degrade identically.
- **100% native Rust**: no C/C++ dependency in the default build
  (ADR-0002). FLTK is replaced by `winit` + `softbuffer` + `tiny-skia`;
  OpenSSL/mbedTLS by `rustls`; bundled image decoders by pure-Rust
  crates (`png`, `jpeg-decoder`, `gif`, `webp`/`image-webp`, `resvg`
  or vendored nanosvg port).
- **Complete-port goal**: every `dlib/lout/dw/src/dpi*` file gets a
  Rust module or a reasoned `won't port` row in `docs/PORTING.md`.
  "Deferred" is a phase position, never a permanent status.

## 2. Repository layout

```
crates/
  dillo-common     dlib + lout : misc utils, unicode, containers,
                   object/signal → typed events, identity
  dillo-net        src/IO : url, dns, IO chain, http, decode (gzip/
                   chunked/deflate), mime, tls (rustls), dpi client
  dillo-css        cssparser.cc, css.cc, styleengine.cc
  dillo-html       html.cc parser + doctree, plain.cc, html table/form
  dillo-dw         dw/* platform-agnostic : types, style, widget,
                   layout, textblock, tables, OOF/floats, iterators,
                   selection, findtext, hyphenator, image abstraction
  dillo-images     imgbuf, dicache, gif/png/jpeg/webp/svg decoders
                   (pure-Rust crates behind dillo's Imgbuf interface)
  dillo-platform   dw::core::Platform impl on winit + softbuffer +
                   tiny-skia ; replaces all dw/fltk*.cc
  dillo-browser    cache, web, nav, history, bookmark, cookies, auth,
                   hsts, capi, prefs, paths, actions, keys, menu,
                   dialog, findbar, tipwin, timeout, ui — the shell
  dillo-dpi        dpip protocol lib + dpi/* servers + dpid daemon
  dillo            facade + binary (dillo.cc, dilloc.c)
  dillo-dw-proto   TEMPORAIRE — banc d'essai ADR-0005 (modèle de
                   propriété de l'arbre de widgets) ; supprimé après
                   la décision
AGENTS.md          ce fichier — le contrat
ROADMAP.md         phases du portage (racine, pas docs/)
CHANGELOG.md       Keep a Changelog
docs/
  PORTING.md       C/C++→Rust file mapping + status — ALWAYS up to date
  feasibility.md   étude de faisabilité
  adr/             architecture decision records
  audits/          rapports + fragments changelog (skills .devin/skills)
.devin/skills/     audit-*/SKILL.md — audits de parité par périmètre
```

Dependency direction is strictly downward:
`common ← net ← css ← html ← dw ← images ← platform ← browser ← dillo`,
`common ← net ← dillo-dpi`. Never introduce an upward or sideways
dependency; shared code goes down a layer (or open an ADR).

## 3. Commands (run before every commit)

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps   # must be warning-free
```

## 4. Hard rules

1. **`#![forbid(unsafe_code)]` in all our crates.** Dependencies using
   unsafe internally are fine; *our* code does not. If an OS feature
   truly has no safe equivalent, open an ADR first (cf. ADR-0002 §4).
2. **No new dependency without an ADR note or commit-message
   justification.** Crates ≥ 7 days old, pinned to minor versions in
   `[workspace.dependencies]`. **Pure-Rust only** — a crate that links
   C code (`*-sys` without a pure-Rust feature) breaks the mission.
3. **No `unwrap()`/`expect()`/`panic!`/`todo!()` in shipped code.**
   Allowed in tests and in the CLI/binaries. Libraries return
   `Result`/`Option`. Dillo is *tolerant* software: a malformed page,
   image or cookie must degrade, never crash.
4. **Update `docs/PORTING.md` in the same commit** as the port it
   describes. A module is not `ported` until its ported tests pass.
5. **Update `CHANGELOG.md`** (Keep a Changelog) for user-visible
   changes.
6. **Do not copy C comments verbatim.** Re-express the *why* in rustdoc;
   keep upstream names recognizable (`DwTextblock` → `Textblock`,
   `a_Url_new` → `Url::new`) so `PORTING.md` stays greppable.
7. **Naming**: `snake_case` modules/functions, `CamelCase` types.
   One C file ≈ one Rust module; every ported module starts with a
   `//! Port of src/foo.c (+ foo.h)` header line.
8. **Error handling**: `thiserror` enums per crate, `Result<T>`
   aliases. Never `Box<dyn Error>` in public APIs.
9. **Async / event model** (ADR-0003): all I/O on `tokio`. Dillo's
   FLTK event loop + `IO`/iowatch become tokio tasks; the layout
   engine (dw) stays synchronous on the UI thread and is fed in
   bounded chunks — never block it on a socket. Incremental
   re-layout must match upstream's idle-callback semantics.
10. **Tracing**: `tracing` crate, no `println!` outside binaries/tests.
    Upstream `MSG`/`_MSG`/`DEBUG_MSG` map to `warn!`/`info!`/`debug!`.
11. **No subsystem binds sockets directly** except `dillo-net`: dpi,
    http, dns all go through injectable connectors so tests can run
    fully in-process.

## 5. Porting workflow (follow it for every module)

1. **Pick a module** in `docs/PORTING.md` marked `not started`, in
   `ROADMAP.md` dependency order.
2. **Read the C/C++**: header first, then implementation. Note
   invariants, parser recovery paths, `dReturn_if_fail` preconditions,
   defaults from `dillorc`/`prefs`.
3. **Design the Rust API** matching §4 conventions. C++ inheritance
   (`dw::core::Widget` → `Textblock`…) becomes a `Widget` trait +
   enum dispatch or trait objects; `Dlist`/`Dstr`/`dVector` become
   `Vec`/`String`/standard collections; `lout::object::Signal`
   becomes typed event channels or callbacks.
4. **Implement** in the target crate.
5. **Port the tests**: `test/unit/*`, `test/dw/*`, `test/html/render`
   are the spec. Screenshot tests become golden-layout tests via a
   headless `tiny-skia` backend; copy test *data* only (`test/html`,
   `.pat` hyphenation files…) into `crates/<crate>/tests/data/`.
6. **Mark status** in `docs/PORTING.md`: `in progress` → `ported`
   (compiles + unit tests) → `verified` (golden render / behavioral
   parity test against upstream).
7. **Commit** as `port(<crate>): <module>` + changelog entry.
8. **Docs à jour à chaque étape** : chaque incrément de travail
   se termine par la mise à jour des documents impactés
   (`docs/PORTING.md`, `ROADMAP.md` checkboxes, `CHANGELOG.md`,
   ADR si décision) **dans le même commit** — le repo local est le
   journal du portage ; un commit de code sans sa doc est incomplet.

### Parity audits

`.devin/skills/audit-*/SKILL.md` define scoped semantic audits
(report mode → `docs/audits/reports/`, apply mode → worktree + merge).
Run them per wave as the ROADMAP unlocks phases; they are the
deviation-control mechanism of the port.

## 6. C/C++ → Rust cheatsheet

| C/C++ (Dillo) | Rust (DilloRust) |
|---|---|
| `dStr`/`dStr_sprintf` | `String` / `format!` |
| `Dlist`, `dVector`, `Dlist` iter | `Vec`, `VecDeque`, iterators |
| `lout::object::Object` + `Signal`/`Receiver` | enum `Event` + `mpsc`/callback traits |
| `dw::core::Widget` vtables + cross-pointers | selon ADR-0005 (arène+`WidgetId` / `Rc`/`Weak` / enum dispatch) |
| `dw::core::Iterator` | standard `Iterator` / index cursors |
| `FLTK` views/platform | `dillo-platform` (winit+softbuffer+tiny-skia) |
| `a_IO_*` + `IO` struct + iowatch | tokio `AsyncRead/AsyncWrite` + channels |
| `a_Cache_*` process callback chain | `Sink`/`Stream`-style typed event chain |
| `dReturn_if_fail` / asserts | `debug_assert!` + graceful `Result` |
| `a_Misc_parse_geometry` etc. `a_*` helpers | free fns in `dillo-common::misc` |
| FLTK timers/`Fl::add_timeout` | `tokio::time` / `winit` control-flow timers |
| mbedtls/openssl TLS | `rustls` (+ `rustls-native-certs`) |
| system `gethostbyname` thread | `tokio::net::lookup_host` / `hickory-resolver` |

## 7. Definition of done for a ported module

- [ ] Compiles with `clippy -D warnings`, `#![forbid(unsafe_code)]`.
- [ ] `//! Port of …` header cites the C files.
- [ ] Public items have rustdoc; non-trivial invariants documented.
- [ ] Corresponding upstream tests ported and passing.
- [ ] Golden-format compatibility where applicable (dillorc,
      bookmarks/cookies files, dpi protocol, HTTP wire).
- [ ] `docs/PORTING.md` updated; `CHANGELOG.md` entry added.
- [ ] No `unwrap`/`expect`/`todo!` outside `#[cfg(test)]`.

## 8. What NOT to port (or port differently)

- `src/md5.c` (MD5 digest auth) → `md-5` crate. `src/digest.c` logic
  stays ported, only the hash primitive is replaced.
- `tls_mbedtls.c` / `tls_openssl.c` → `rustls` backend only; keep the
  `Tls` trait shape so upstream's two-backend structure maps to one
  default impl.
- `nanosvg.h`/`nanosvgrast.h` (vendored single-header) → prefer
  `resvg`/`usvg`; if SVG subset parity matters, port `svg.c` glue to it.
- `fltk*.cc` in `dw/` → not ported 1:1; replaced by `dillo-platform`
  implementing `dw::core::Platform` (ADR-0002). `xembed.cc` →
  `won't port` (X11-specific embedding).
- `Doxyfile`, autotools build system, `*.desktop`, icons → `won't port`.
- `test/` FLTK drivers → replaced by headless golden-render harness.
- Everything else deferred → a ROADMAP phase, not a skip. A
  `won't port` row requires an explicit reason.

## 9. Known traps (read before porting!)

- **Widget cross-pointers** (`dw/widget.hh:127-146`) : `parent`
  (propriétaire), `quasiParent` (conteneur logique OOF),
  `generator` (avec fallback `getGenerator()→parent` si null),
  `container` (containing block) sont 4 liens bruts sans refcount.
  Le parent de *contenu* ≠ parent de *layout* pour les widgets OOF.
  Le modèle de propriété Rust est fixé par **ADR-0005** — ne pas
  commencer `dw/` avant sa résolution par prototype.
- **Dillo parses tolerantly**: `html.cc` recovers from malformed input
  in specific ways (implicit closes, tag soup nesting rules). A
  spec-compliant-but-stricter parser *diverges*. Port the state
  machine, not HTML5.
- **Incremental everything**: parser, image decoders and layout all run
  on partial data. `Cache`/decoder callbacks fire per chunk; layout
  re-queues itself via idle. Batching "for simplicity" changes
  observable behavior (progressive rendering).
- **`dw` size negotiation** (`getExtremes`/`setSize`/`draw`) is a
  two-pass contract; `RoundingErrors` doc explains deliberate integer
  rounding — preserve it or tables drift by pixels.
- **OOF managers**: floats/abs/fixed/rel positioning interlock via
  `oof*mrg` generators (`GBFF`, etc.) — order of allocation matters.
- **Imgbuf row callbacks**: decoders signal per-scanline; the image
  widget paints incomplete rows. Keep per-row granularity.
- **Charset**: everything flows as UTF-8 internally via `utf8.cc`
  (`a_Utf8_decode` lenient mode). `decode.c` handles transfer
  encodings, not charsets — don't conflate.
- **Cookies/HSTS files** are line-based shared with C Dillo; parse
  leniently, write byte-identically.
- **dpi protocol** is an fd/socket text protocol (`dH_dpipe`); keep
  framing byte-exact (`dpip.c`) — C plugins must interoperate during
  the transition.
- **Windows port**: upstream is POSIX-first; `paths.cc`/`dlib`
  already abstract some. Our Rust port targets Windows + Linux from
  day one — isolate `std::os::unix` usage behind `dillo-platform`.

## 10. Concurrency model

Upstream Dillo is single-threaded + a DNS helper thread. We keep the
*single-writer* property: all layout/DOM state lives on the UI task;
network/decoders run on tokio worker tasks and deliver chunks via
channels. Never `Arc<Mutex>` the widget tree. When in doubt: **read
the C first, then re-read it** — upstream is the spec, not intuition.
