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
- `dillo-common::utf8` : port de `src/utf8.cc`/`utf8.hh` —
  `end_of_char`, `decode` lenient (fallback octet brut type
  ISO-8859-1), `encode` aux formes historiques 1–6 octets de
  `fl_utf8encode`, `test` (catégories 0–4), `ideographic`,
  `combining_char`, `char_count`, constantes U+FFFD/U+200B (8 tests).
- `dillo-common::misc` : port de `src/misc.c`/`misc.h` —
  `escape_chars`, `expand_tabs` (taquets 8 + queue de diacritiques
  combinants), sniffing `get_content_type_from_data`,
  `parse_content_type`/`cmp`/`check`, `parse_geometry`,
  `parse_search_url`, `encode_base64`, `parse_content_disposition`
  (filename sanitizé), `file_to_vec`. Quirks amont reproduits et
  documentés : `content_type_check` `-1` inatteignable ; le sniffing
  classe `text/plain` tout buffer UTF-8-valide (même octets de
  contrôle) (12 tests).
- `dillo-common::colors` : port de `src/colors.c` — table
  `EXTENDED_COLOR` verbatim (147 entrées triées), `color_parse`
  (`#`/`0x`/nom/hex nu + statut err 0/1/2), `color_vc` avec scores
  `distance2`/`distance3` verbatim (4 tests).
- `dillo-net::url` : port de `src/url.c`/`url.h`/`src/IO/Url.h` —
  `DilloUrl` (champs owned au lieu de vues NUL), parse lenient,
  `resolve_relative` RFC-3986 avec les mêmes hacks amont (dot-
  segments, hack `/` pré-authority, truncate `query_sep`), encodage
  `%XX` des chars illégaux, heuristique `http:`/`//` par défaut,
  `hostname`/`port` lazy (ipv6 `[]`, défauts 80/443), `url_cmp`
  (ordre amont), `decode`/`encode_hex_str`, `strip_delimiters`,
  `host_type`, `host_public_suffix` + liste TLD, `set_ismap_coords`.
  Switch HSTS isolé : `switch_to_https` reproduit la transform
  (`:80`→`:443`, port, scheme) — le déclencheur `prefs`/`hsts` se
  branchera avec leurs ports. 18 tests dont la batterie RFC-3986
  §5.4 complète (normal + abnormal).
- `dillo-browser::prefs` : port de `src/prefs.c` + `prefsparser.cc` —
  `Prefs` owned (pas de global), `Default` = `a_Prefs_init` verbatim
  (~110 champs, UA `Dillo/3.3.0-rc1`), `parse_dillorc` ligne à ligne
  via `dlib::parse_rc_line`, `parse_option` = symbol table → match
  avec les 12 types (BOOL/COLOR/STRING/STRINGS/URL/INT32/DOUBLE/
  FRACTION_100/GEOMETRY/PANEL_SIZE), sémantique « 1re occurrence
  vide le défaut » pour les listes, `strtod`/`strtol` lenients.
