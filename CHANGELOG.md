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
