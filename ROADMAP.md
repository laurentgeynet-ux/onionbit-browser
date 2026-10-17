# ROADMAP.md — DilloRust port phases

Ground truth: `D:\Projet\dillo` (read-only). Status tracking:
`docs/PORTING.md`. Each phase ends with the listed audit skill(s)
in `rapport` mode, then `apply`.

## Phase 0 — Scaffolding (fondations repo)

- [x] Cargo workspace, `crates/` skeleton, CI (fmt/clippy/test/doc, MSRV)
- [x] `dillo-common` : `dlib/dlib.c` **[`dlib.rs`]** +
      `lout/{misc,unicode,object,signal,identity}` **[`lout/`]** —
      `container`/`debug`/`msg` → won't port justifiés (std/tracing)
- [x] Headless golden-render harness **[`dillo-platform::golden`]**
      — `Pixmap` offscreen tiny-skia, `diff_pixmaps` (compte + bbox +
      delta canal), PNG load/save, `assert_golden` +
      `DILLO_UPDATE_GOLDEN=1` pour régénérer
- [x] **`dillo-dw-proto`** (ADR-0005) : prototype options A/B/D sur
      3 widgets → **option D retenue** (arène+`enum`+`LayoutCtx`) —
      B a paniqué `BorrowMutError` au 1er essai puis exigé
      try_borrow+defer ; ADR amendé avec règles de port
- [ ] PORTING.md rows phase-0 → `ported`

**Audits** : none yet (infrastructure).

## Phase 0.5 — Vertical slice (« Hello HTML »)

Jalon de dé-risquage et de motivation : une tranche verticale
minimale qui valide l'intégration winit↔tokio↔dw **avant** les
gros chantiers. Ne vise pas la parité — vise l'affichage.

- [x] Fenêtre `winit` + `softbuffer` + `tiny-skia` (1er pixel) —
      `dillo-platform::surface` (Pixmap → blit 0RGB → present)
- [x] `dillo-platform` minimal : `PlatformSurface` (fenêtre+surface)
      — le trait `Platform` complet viendra avec dw
- [x] Parseur HTML minimaliste **`dillo-html::minidoc`** : `<h1-6>`,
      `<p>`, `<a href>`, entités de base, tolérance tag-soup
- [x] Textblock minimal : wrap `Wrap::Word` cosmic-text, hitboxes de
      liens via `Attrs::metadata` (pilote — sera remplacé par dw)
- [x] Clic sur lien → `dillo-net::fetch` (`file://` + `resolve_href`
      URL-semantics) → re-render ; `http(s)` = phase 1
- [x] Boucle events : clic, resize, scroll molette, curseur
- [x] **Idle/layout progressif** : `ControlFlow::Wait` +
      `request_redraw` ciblé — pas de spin (mapping `Fl::add_idle`
      documenté ADR-0003)

**Audits** : none — jetable/pilote, le code sera repris dans les
vraies phases. Sert aussi de banc d'essai au choix ADR-0005.

## Phase 1 — Parsing foundations

- [x] `utf8.cc` → `common::utf8` (lenient decode semantics)
- [x] `prefsparser.cc` + `prefs.c` → `dillo-browser::prefs` (dillorc
      byte-compatible)
- [x] `colors.c` (named colors → RGB), `misc.c`
- [x] `url.c` → `net::url` (lenient parse, relative resolution)
- [x] `decode.c` (gzip/deflate/chunked — `flate2` rust_backend +
      `brotli` + `encoding_rs`, gzip header/trailer à la main)
- [x] `dns.c` → `net::dns` (tokio `spawn_blocking`), `chain.c`/`IO.c`/
      `iowatch.cc` → `net::chain` + `net::io` (CCC verbatim, reactor tokio)
- [x] `mime.c`, `http.c` → HTTP/1.1 client (request fmt, redirects,
      proxy) + `tls` via rustls
- [x] `about.c`/`proto.c` internal schemes

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

- [~] `types`, `style`, `events`, `core.hh` traits → `dillo-dw` core
      — types/events/tools portés, `style` complet (pool/table tl,
      `Font`, `numtostr`, `StyleImage`+buffer tuilé, renderers
      externes, `drawBorder`/`drawBackground*` trapezes verbatim),
      traits `imgbuf`/`imgrenderer`, traits `platform`/`view`
      complets ; `[img]::imgbuf` impl le trait dw (arbre
      scaled-buffers ≙`fltkimgbuf`, `scaleMode=SIMPLE` vs
      `BEAUTIFUL_GAMMA` amont — déviation documentée) ; `image.cc`
      aligné `Rc<dyn Imgbuf>`
- [ ] `widget.cc` + `layout.cc` (size negotiate, allocation, draw,
      damage/queue-draw, anchors)
- [ ] `iterator.cc`, `textblock.cc` + `textblock_linebreaking.cc` +
      `textblock_iterator.cc` (word wrap, breaking spaces doc parity)
- [ ] `hyphenator.cc` (+ `.pat` files), `ruler.cc`, `bullet.cc`,
      `listitem.cc`, `alignedtextblock.cc`
- [ ] `findtext.cc`, `selection.cc`
- [ ] `image.cc` (widget side), `imgrenderer.cc`
- [~] `tools.cc`, `ui.cc` (dw::core::ui widgets) — `tools` porté

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
      `findbar.cc`, `tipwin.cc` (`timeout.cc` déjà porté dans
      `dillo-net` — la boucle de drain `poll_timeouts` y est partagée)
- [ ] `paths.cc`, `dillo.cc` main, `dilloc.c`

**Audit** : `audit-10-platform-ui`

## Phase 7 — Browser services

- [x] `cache.c` + `capi.c` + `web.cc` (dans `dillo-net` — pré-requis
      du transport/dicache ; seams `BrowserOps`/`UiOps`/`NavOps`/
      `DicacheOps`/`WebDispatch`/`DpiOps` injectées depuis le futur
      `dillo-browser`) — reste `bw.c` (browser windows)
- [x] `dicache.c`, `imgbuf.cc`, `image.cc` (src) — noyau dicache
      porté (states `DIC_*`, scans, `about:dicache`, seam
      `cache::DicacheOps` implémentée)
- [x] decoders `gif.c` (LZW maison verbatim), `png.c` (crate `png`),
      `jpeg.c` (`jpeg-decoder`), `webp.c` (`image-webp`),
      `svg.c` (`resvg`/`usvg`) + registre `mime.c` → viewers
      `image/*` câblés ; déviations « decode-at-completion »
      documentées par module (affichage progressif perdu vs
      libpng/libjpeg/libwebp amont)
- [x] `nav.c`, `history.c`, `bookmark.c`, `auth.c`, `digest.c` (md-5),
      `cookies.c`, `domain.c`, `hsts.c` (+preload data), `bitvec.c`,
      `klist.c`
- [x] `actions.c`, `dpiapi.c`

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
