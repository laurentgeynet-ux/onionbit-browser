//! dillo-dpi — protocole dpi, démon et plugins.
//!
//! Cible du port : `dpip/dpip.c` (framing byte-exact, côté serveur
//! ici — `build_cmd`/`get_attr` sont dans `dillo_net::dpip`),
//! `dpi/dpiutil.c` (échappements), `dpid/*` (démon, socket dir),
//! `dpi/*` (plugins). Interop C↔Rust exigée (ADR-0004).
#![forbid(unsafe_code)]
// Lint verbatim : fidélité aux casts/tests C amont (voir AGENTS.md §5).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::similar_names)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::result_unit_err)]
#![allow(clippy::format_push_string)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::needless_continue)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::manual_strip)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::overly_complex_bool_expr)]
pub mod bookmarks;
pub mod cookies;
pub mod dlitem;
pub mod dpid;
pub mod dpid_common;
pub mod dpip;
pub mod dpiutil;
