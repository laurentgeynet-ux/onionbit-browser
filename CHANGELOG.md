# Changelog

All notable changes to DilloRust are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Repository scaffold: mission contract (`AGENTS.md`), feasibility
  study (`docs/feasibility.md`), phase plan (`ROADMAP.md`), full
  upstream→Rust file mapping (`docs/PORTING.md`).
- ADRs 0001–0005: port scope & fidelity, GUI stack
  (winit+softbuffer+tiny-skia), I/O model (tokio+rustls), DPI
  subsystem, widget-tree ownership (open, prototype-gated).
- 12 parity-audit skills in `.devin/skills/` (report/apply two-mode
  protocol, `docs/audits/` outputs).
- Cargo workspace : 11 crates squelettes (`dillo-common` → `dillo`,
  + `dillo-dw-proto` pour l'ADR-0005), `#![forbid(unsafe_code)]`,
  lints workspace (unsafe forbidden, clippy pedantic warn), CI
  GitHub Actions (fmt/clippy/test/doc + MSRV 1.85 sur ubuntu+windows).
- `dillo-common::dlib` : port de `dlib/dlib.c` — `Dstr`, `Dlist<T>`,
  helpers `dStr*`/`dIs*` ASCII, `parse_rc_line`, `getline` (14 tests).
- `dillo-common::lout` : port de `lout/` —
  `misc` (`SimpleVector`, `NotSoSimpleVector` fidèle avec extra-
  arrays, `StringBuffer`, `BitSet` avec sa sur-allocation amont,
  `round_int`), `unicode` (décodeur UTF-8 lenient + table alpha
  verbatim), `object` (traits + enum `Value`), `signal` (`Emitter<R>`
  sur `Weak<RefCell<R>>`, `emit_bool` sans court-circuit),
  `identity` (trait `Identifiable`). `container`/`debug`/`msg` →
  won't port (collections std / `tracing`). 15 tests ajoutés.
- `dillo-dw-proto` : prototype ADR-0005 (3 options × scénario
  `queue_resize` en pleine itération) → **option D retenue** :
  arène `Layout` + `WidgetId` + `enum WidgetKind` + `LayoutCtx`.
  B (`Rc`/`Weak`) a paniqué `BorrowMutError` au premier essai puis
  exigé try_borrow+defer partout ; A (`dyn`) paie take/replace +
  downcast sans gain. ADR-0005 amendé `accepté` + règles de port
  `dw/` (liens en ids, take/copy avant récursion, events drainés).
- `dillo-platform::golden` : harness golden-render headless —
  `Pixmap` offscreen tiny-skia, diff pixel-à-pixel (compte, bbox,
  delta canal), PNG load/save, `assert_golden` avec régénération
  via `DILLO_UPDATE_GOLDEN=1`. Deps : `tiny-skia` 0.11 (ADR-0002),
  `png` 0.17 (pur Rust — stockage des images de référence).
- **Phase 0.5 — tranche verticale** : `dillo` bin ouvre une fenêtre
  winit, rend blocs+liens via tiny-skia/cosmic-text, navigue par
  clic (`file://`). Modules : `dillo-html::minidoc` (parseur
  tolérant h1-6/p/a/entités), `dillo-net::fetch` (`file://` +
  `resolve_href` URL-semantics), `dillo-platform::surface`
  (winit+softbuffer+pixmap blit). `ControlFlow::Wait` sans spin.
  Deps : `winit` 0.30, `softbuffer` 0.4, `cosmic-text` 0.12
  (ADR-0002). Pilote — remplacé par dw phase 4.
