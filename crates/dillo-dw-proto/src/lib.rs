//! dillo-dw-proto — banc d'essai TEMPORAIRE pour l'ADR-0005.
//!
//! Prototype du modèle de propriété de l'arbre de widgets :
//! options A (arène + `WidgetId`), B (`Rc`/`Weak`), D (hybride
//! arène + enum dispatch + `LayoutCtx`) testées sur 3 widgets
//! (`Container`, `Textblock`, `Image`) avec un `queue_resize`
//! traversant `parent` et `generator` pendant une itération.
//!
//! Ce crate sera supprimé une fois l'ADR-0005 tranchée.
#![forbid(unsafe_code)]
