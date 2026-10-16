# ROADMAP.md — DilloRust port phases

Ground truth: `D:\Projet\dillo` (read-only). Status tracking:
`docs/PORTING.md`. Each phase ends with the listed audit skill(s)
in `rapport` mode, then `apply`.

## Phase 0 — Scaffolding (fondations repo)

- [x] Cargo workspace, `crates/` skeleton, CI (fmt/clippy/test/doc, MSRV)
- [~] `dillo-common` : port `dlib/dlib.c` (Dstr→String, Dlist→Vec,
      d_* helpers) **[fait : `dlib.rs`]**, `lout/{misc,unicode,container,identity,object,signal}`
- [ ] Headless golden-render harness (tiny-skia offscreen target +
      image diff) — prerequisite for all later verification
- [ ] **`dillo-dw-proto`** (ADR-0005) : prototype du modèle de
      propriété de l'arbre de widgets (3 widgets, options A/B/D),
      décision ADR-0005 amendée — **bloquant pour la phase 4**
- [ ] PORTING.md rows phase-0 → `ported`

**Audits** : none yet (infrastructure).

## Phase 0.5 — Vertical slice (« Hello HTML »)

Jalon de dé-risquage et de motivation : une tranche verticale
minimale qui valide l'intégration winit↔tokio↔dw **avant** les
gros chantiers. Ne vise pas la parité — vise l'affichage.

- [ ] Fenêtre `winit` + `softbuffer` + `tiny-skia` (1er pixel à l'écran)
- [ ] `dillo-platform` minimal : trait `Platform` (drawText/Rect seulement)
- [ ] Parseur HTML minimaliste : `<h1>`, `<p>`, `<a>` → doctree
- [ ] Textblock minimal : wrap simple, sans hyphenation
- [ ] Clic sur lien → `dillo-net` fetch → re-render
- [ ] Boucle events : clic souris, resize, scroll
- [ ] **Idle/layout progressif** : `Fl::add_idle` amont →
      `ControlFlow`/`AboutToWait` winit (tranches de layout sans
      bloquer l'event loop, sans spin CPU — ADR-0003)

**Audits** : none — jetable/pilote, le code sera repris dans les
vraies phases. Sert aussi de banc d'essai au choix ADR-0005.

## Phase 1 — Parsing foundations

- [ ] `utf8.cc` → `common::utf8` (lenient decode semantics)
- [ ] `prefsparser.cc` + `prefs.c` → `dillo-browser::prefs` (dillorc
      byte-compatible)
- [ ] `colors.c` (named colors → RGB), `misc.c`
- [ ] `url.c` → `net::url` (lenient parse, relative resolution)
- [ ] `decode.c` (gzip/deflate/chunked via `flate2`-pure or miniz-oxide)
- [ ] `dns.c` → tokio lookup, `chain.c`/`IO.c`/`iowatch` → `net` core
- [ ] `mime.c`, `http.c` → HTTP/1.1 client (request fmt, redirects,
      proxy) + `tls` via rustls
- [ ] `about.c`/`proto.c` internal schemes

**Audit** : `audit-01-io-http`, `audit-02-url-cookies` (partiel)

## Phase 2 — HTML engine

- [ ] `html.cc` + `html_common.hh` + `html_charrefs.h` →
      `dillo-html` tokenizer/parser (tag-soup state machine,
      entity table)
- [ ] `doctree.hh` → document tree
- [ ] `plain.cc` → plain-text doc type
- [ ] `form.cc` → forms model (submit logic différée à phase 6)
- [ ] `table.cc` (src/html glue) → table widget bridge

**Audit** : `audit-03-html-parser`

## Phase 3 — CSS engine

- [ ] `cssparser.cc` → tokenizer/parser (tolerant recovery)
- [ ] `css.cc` → declaration sets, selector matching, specificity
- [ ] `styleengine.cc` → cascade, inheritance, `dw::core::style` glue
- [ ] UA/user/author sheets + `style.css`/`dillorc` hooks

**Audit** : `audit-04-css-style`

## Phase 4 — dw layout engine (cœur)

**Prérequis : ADR-0005 résolu** (modèle de propriété de l'arbre —
prototype phase 0).

- [ ] `types`, `style`, `events`, `core.hh` traits → `dillo-dw` core
- [ ] `widget.cc` + `layout.cc` (size negotiate, allocation, draw,
      damage/queue-draw, anchors)
- [ ] `iterator.cc`, `textblock.cc` + `textblock_linebreaking.cc` +
      `textblock_iterator.cc` (word wrap, breaking spaces doc parity)
- [ ] `hyphenator.cc` (+ `.pat` files), `ruler.cc`, `bullet.cc`,
      `listitem.cc`, `alignedtextblock.cc`
- [ ] `findtext.cc`, `selection.cc`
- [ ] `image.cc` (widget side), `imgrenderer.cc`
- [ ] `tools.cc`, `ui.cc` (dw::core::ui widgets)

**Audit** : `audit-05-dw-core`, `audit-06-dw-textblock`

## Phase 5 — Tables & positioned layout

- [ ] `table.cc`/`tablecell`/`simpletablecell`/`alignedtablecell` +
      `table_iterator.cc`
- [ ] `outofflowmgr.cc`, `oofawarewidget.cc` (+iterator)
- [ ] `ooffloatsmgr.cc`, `oofposabs{,like}mgr.cc`, `oofposrelmgr.cc`,
      `oofposfixedmgr.cc`, `oofpositionedmgr.cc`
- [ ] `stackingcontextmgr.cc`, `regardingborder.cc`

**Audit** : `audit-07-dw-table`, `audit-08-dw-oof`

## Phase 6 — Platform & UI shell

- [ ] `dillo-platform` : `dw::core::Platform` impl (winit window,
      softbuffer present, tiny-skia `View`/`Layout` hooks, events,
      clipboard, cursors) — replaces `dw/fltk*.cc`
- [ ] `src/ui.cc` chrome (toolbars, tabs via `UI`/`CustTabs`),
      `uicmd.cc`, `keys.cc`+`keysrc`, `menu.cc`, `dialog.cc`,
      `findbar.cc`, `tipwin.cc`, `timeout.cc`
- [ ] `paths.cc`, `dillo.cc` main, `dilloc.c`

**Audit** : `audit-10-platform-ui`

## Phase 7 — Browser services

- [ ] `cache.c` + `capi.c` (client API, chain glue), `bw.c`
      (browser windows), `web.cc`
- [ ] `dicache.c`, `imgbuf.cc`, `image.cc` (src) + decoders
      `gif.c`,`png.c`,`jpeg.c`,`webp.c`,`svg.c` → pure-Rust crates
- [ ] `nav.c`, `history.c`, `bookmark.c`, `auth.c`, `digest.c` (md-5),
      `cookies.c`, `domain.c`, `hsts.c` (+preload data), `bitvec.c`,
      `klist.c`
- [ ] `actions.c`, `dpiapi.c`

**Audit** : `audit-09-images`, `audit-11-browser-glue`,
`audit-02-url-cookies` (complet)

## Phase 8 — DPI subsystem

- [ ] `dpip/dpip.c` → `dillo-dpi` protocol lib (byte-exact framing)
- [ ] `dpid/*` → daemon (unix socket dir + Windows named-pipe
      equivalent, service registry)
- [ ] `dpi/*` plugins : `file.c`, `datauri.c`, `downloads.cc`,
      `bookmarks.c`, `cookies.c`, `vsource.c`, `hello.c`, `ftp.c`,
      `dpiutil.c`
- [ ] `src/IO/dpi.c` client side

**Audit** : `audit-12-dpi`

## Phase 9 — Hardening & release

- [ ] `test/html/render` golden suite ≥ upstream coverage
- [ ] Fuzzing (html, css, decode, dpi, cookies files)
- [ ] Parity sign-off : all audits `apply` merged, PORTING.md 100%
      resolved
- [ ] Packaging (Windows/Linux), perf pass vs upstream baseline
