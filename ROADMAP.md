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
- [x] PORTING.md rows phase-0 → `ported`

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

- [x] `html.cc` + `html_common.hh` + `html_charrefs.h` →
      `dillo-html` tokenizer/parser (tag-soup state machine,
      entity table) — `html.rs`/`common.rs`/`entities.rs`/`tags.rs`
- [x] `doctree.hh` → document tree — `dillo-css::doctree`
- [x] `plain.cc` → plain-text doc type — `dillo-html::plain`
- [x] `form.cc` → forms model (submit logic différée à phase 6) —
      `dillo-html::form`
- [x] `table.cc` (src/html glue) → table widget bridge —
      `dillo-html::table`

**Audit** : `audit-03-html-parser`

## Phase 3 — CSS engine

- [x] `cssparser.cc` → tokenizer/parser (tolerant recovery) —
      **`dillo-css::parser`** (48 tests ; seams `CssHtmlHooks`/`TagIndex`)
- [x] `css.cc` → declaration sets, selector matching, specificity —
      **`dillo-css::css`** (quirks verbatim : `border-rigth-width`,
      table props désordonnée, `elementTable` guard)
- [x] `styleengine.cc` → cascade, inheritance, `dw::core::style` glue
      — **`dillo-css::styleengine`** (sentinel `(Color*)-1` ≙
      `Option`, quirk `z-index:auto`, UA sheet `thread_local`)
- [x] UA/user/author sheets + `style.css`/`dillorc` hooks
      (`StyleEnginePrefs` injecté ; `dillorc` parsing = phase 6 prefs)

**Audit** : `audit-04-css-style` — rapport `docs/audits/reports/A04*`

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
- [x] `widget.cc` + `layout.cc` (size negotiate, allocation, draw,
      damage/queue-draw, anchors)
- [x] `iterator.cc` **[`dw::iterator`]** porté (trait
      `DwIterator`, Deep/Char verbatim, ChildIterator seam)
      ; `textblock.cc` + `textblock_linebreaking.cc` +
      `textblock_iterator.cc` (word wrap, penalties/badness,
      collapsing spaces, justification cumulative, sizing,
      draw, itérateur `TbInFlow`) — **[`dw::textblock`]**
      porté (rapport A06 partiel)
- [x] `hyphenator.cc` (Trie + `.pat`/`.exc`/`.trie` loaders —
      fichiers `hyph-*.pat` data à copier en test-data),
      `ruler.cc`, `bullet.cc`, `listitem.cc`,
      `alignedtextblock.cc` **[`dw::textblock::listitem`,
      `dw::ruler`, `dw::bullet`, `dw::hyphenator`]**
- [x] `findtext.cc`, `selection.cc` **[`dw::findtext`,
      `dw::selection`]** (KMP verbatim, DeepIterator ancrage,
      clipboard)
- [x] `image.cc` (widget side), `imgrenderer.cc`
      **[`dw::image`]** — `WidgetKind::Image` + `ImageData`
      verbatim, `ImageImgRenderer` (`Weak<Arena>`+`WidgetId`,
      resize/draw différés tagués `Layout::id`), sizing/extremes/
      allocation/draw-lignes/alt-text verbatim, `ImageMapsList`
      (shapes + hit-test), events link/click `x_img`/`x_link`
- [x] `tools.cc`, `ui.cc` (dw::core::ui widgets) —
      **[`dw::ui`]** : `WidgetKind::Embed` + trait `Resource`
      (`&self`+Cell/RefCell — ré-entrant), receivers/emitters
      verbatim, sous-traits LabelButton/Selection/List/OptionMenu/
      Text/Entry/MultiLine/Toggle/Check/Radio + `ResourceFactory`,
      `ComplexButtonResource` complet (arène partagée
      `Layout::with_arena`, relief `2*x`/`y`, relay resize
      `pending_resizes` tagué)

**Audit** : `audit-05-dw-core`, `audit-06-dw-textblock`

## Phase 5 — Tables & positioned layout

- [x] `table.cc`/`tablecell`/`simpletablecell`/`alignedtablecell` +
      `table_iterator.cc` — algo verbatim (apportion2 cumulatif,
      spans, `\bug` amont conservés)
- [x] `outofflowmgr.cc`, `oofawarewidget.cc` (+iterator)
- [x] `ooffloatsmgr.cc` — `oofposabs{,like}mgr.cc`, `oofposrelmgr.cc`,
      `oofposfixedmgr.cc`, `oofpositionedmgr.cc` = `None` (`IMPL_POS=false`
      amont : morts et inaccessibles — vrai port si un jour activés)
- [x] `stackingcontextmgr.cc` (seams — `IMPL_POS=false` : SCM jamais
      instancié verbatim), `regardingborder.cc` (tag → `is_regarding_border_kind`)

**Audit** : `audit-07-dw-table`, `audit-08-dw-oof`

## Phase 6 — Platform & UI shell

- [x] `dillo-platform` : `dw::core::Platform` impl (winit window,
      softbuffer present, tiny-skia `View`/`Layout` hooks, events,
      clipboard, cursors) — replaces `dw/fltk*.cc`
      (`platform.rs`/`view.rs`/`ui.rs`/`app.rs`/`surface.rs` +
      `golden.rs`) — clipboard in-process (déviation documentée)
- [x] `src/ui.cc` chrome — `dillo-browser::ui` (`Ui`/`Group`/
      `Tabs`/`LineInput`/`Chrome` : widgets logiques auto-dessinés
      verbatim, dessin tiny-skia, icônes XPM `pixmaps.rs`,
      `UiShell` complet)
      (`timeout.cc` déjà porté dans `dillo-net` — la boucle de
      drain `poll_timeouts` y est partagée)
- [x] `src/menu.cc`/`dialog.cc`/`findbar.cc`/`tipwin.cc` —
      `dillo-browser::{menu,dialog,findbar,tipwin}` : 8 popups
      verbatim + `MenuCtx`/`dispatch`, seams `DialogRunner`/
      `DocOps`, `LineEditor` (Ctrl-AEKD+cut UTF-8), filtre clavier
      findbar Pass/Consume, `TipWin` partagé (délais 0.8/0.1s,
      position pointeur, highlight CustButton)
- [x] `bw.c`/`nav.c`/`keys.cc`+`keysrc`/`uicmd.cc` — glue
      `BrowserWindow`, pile de navigation, raccourcis clavier,
      dispatch `a_UIcmd_*` via seams `UiShell`/`NavIo`/`NavSched`
- [x] `paths.cc`, `dillo.cc` main, `dilloc.c` + `IO/control.c`
      — `dillo-browser::{paths,glue,doc}`, `dillo-net::control`
      (machine d'état + `CtlListener` Unix ≙ `ENABLE_CONTROL_SOCKET`),
      `dillo` bin : main verbatim (CLI, prefs, inits modules, factory
      moteurs tokio, fenêtre initiale, boucle `DwApp`, freeall)

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

- [x] `dpip/dpip.c` → `dillo-net::dpip` (client) +
      `dillo-dpi::dpip` (serveur : `DpIo`/`DpIoTcp`/`DpIoSock`,
      `Dsh` tokens/modes/`check_auth`) — framing byte-exact
- [x] `dpid/*` → `dillo-dpi::{dpid,dpid_common}` + bins
      `dillo-dpid`/`dillo-dpidc` (registry `*:` wildcards,
      spawn-on-demand, socket dir, boucle select→tokio)
- [x] `dpi/*` plugins : `file.c`→`dpi-file`, `datauri.c`→
      `dpi-datauri`, `downloads.cc`→`dlitem`+`dpi-downloads`
      (fenêtre native ≙ FLTK — ADR-0002), `bookmarks.c`→
      `bookmarks`+`dpi-bookmarks`, `cookies.c`→`cookies`+
      `dpi-cookies`, `vsource.c`→`dpi-vsource`, `hello.c`→
      `dpi-hello`, `ftp.c`→`dpi-ftp`, `dpiutil.c`→`dpiutil`
- [x] `src/IO/dpi.c` client side → `dillo-net::dpi`

**Audit** : `audit-12-dpi` — rapport `A12_dpi.md`

## Phase 9 — Hardening & release

- [x] `test/html/render` golden suite ≥ upstream coverage —
      `dillo/tests/render_ref.rs` : 39 paires `.html`/`.ref.html`
      pixel-identiques via la vraie pipeline (parser→style→layout→
      tiny-skia, images réelles dicache, canvas 780×580 verbatim
      `PREFS_GEOMETRY_DEFAULT_*`) ; `ALLOWED` = les 4 XFAIL amont
      (`margin-auto`, `max-width-html`, `span-padding`,
      `table-td-width-percent`) + `svg-current-color` (AA du
      rasterizer resvg ≠ nanosvg, sémantique `currentColor` ok)
- [x] Fuzzing (html, css, decode, dpi, cookies files) —
      `fuzz/` : 6 cibles `fuzz_target!` (libFuzzer, Linux/CI) +
      runner `smoke` stable (mutateur seedé, catch_unwind,
      artifacts) — cf. `docs/fuzzing.md`. 3 bugs trouvés et fixés
      (`html.rs` write clamp, `cosmic-text` classe bidi B,
      `decode.rs` charset drain). Campagne 30k×6 clean
- [x] Parity sign-off : 12/12 rapports d'audit complets (A08/A11
      complétés — plus de `_partial`), toutes les déviations
      appliquables mergées, PORTING.md 100% resolved.
      **Portée exacte** : parité démontrée sur le corpus golden
      + audits sémantiques + tests portés — pas sur le web réel ;
      une navigation vivante peut encore révéler des écarts non
      couverts par l'échantillon (cf. `pm_text`).
- [x] Packaging (Windows/Linux — `packaging/package.{sh,ps1}`, layout
      dpid verbatim) + perf pass (`docs/perf.md` : 60 ms startup,
      ~40 Mo RSS idle, corpus golden ~34 ms/page médiane)
