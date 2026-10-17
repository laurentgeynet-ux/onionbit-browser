//! Port de `dw/platform.hh` — trait `dw::core::Platform`
//! (dépendances plateforme du layout : métriques texte, factories
//! de ressources de style, file idle).
//!
//! Rôle amont : interface injectée dans `Layout`. Toutes les
//! méthodes sont pures seam — les implémentations arrivent avec
//! `fltkplatform` (≙ `dillo-platform`).

use crate::imgbuf::Imgbuf;
use crate::style::{Color, Font, FontAttrs, Tooltip};
use crate::view::View;
use std::rc::Rc;

/// `dw::core::Platform` — verbatim `platform.hh`.
///
/// Différence de modèle : `addIdle(void (Layout::*func)())` amont
/// prend un pointeur de méthode de `Layout` ; le port prend un
/// `IdleCallback` (`Rc<dyn Fn(&mut Layout)>` appelé par le
/// drain de la file idle du layout — ADR-0005 : pas de `&mut
/// self` flottant, le callback reçoit `&mut Layout` au moment du
/// drain, pas à l'enregistrement).
pub trait Platform {
    /// `setLayout` — notification d'attachement.
    fn set_layout(&mut self, layout: usize);

    /// `attachView` — une vue vient d'être attachée au layout.
    fn attach_view(&mut self, view: &mut dyn View);

    /// `detachView`.
    fn detach_view(&mut self, view: &mut dyn View);

    /// `textWidth` — largeur du texte dans la fonte donnée.
    fn text_width(&mut self, font: &Font, text: &[u8]) -> i32;

    /// `textToUpper` — retourne le texte en majuscules.
    fn text_to_upper(&mut self, text: &[u8]) -> Vec<u8>;

    /// `textToLower`.
    fn text_to_lower(&mut self, text: &[u8]) -> Vec<u8>;

    /// `nextGlyph` — index du glyphe suivant.
    fn next_glyph(&mut self, text: &[u8], idx: i32) -> i32;

    /// `prevGlyph` — index du glyphe précédent.
    fn prev_glyph(&mut self, text: &[u8], idx: i32) -> i32;

    /// `dpiX` — résolution écran horizontale.
    fn dpi_x(&mut self) -> f64;

    /// `dpiY`.
    fn dpi_y(&mut self) -> f64;

    /// `addIdle` — enregistre une fonction idle (un seul appel,
    /// retirée après exécution) ; retour = id pour `removeIdle`.
    /// L'id -1 = « aucun idle enregistré » (usage amont).
    fn add_idle(&mut self, func: IdleCallback) -> i32;

    /// `removeIdle` — retire un idle pas encore exécuté.
    fn remove_idle(&mut self, idle_id: i32);

    /// Collecte les callbacks idle dus — seam du port : amont
    /// la plateforme appelle elle-même les callbacks (elle tient
    /// `Layout*`) ; le modèle D n'a pas de `&mut Layout`
    /// flottant, la plateforme expose les callbacks « dus » et
    /// [`crate::layout::Layout::drain_idles`] les appelle sur
    /// `&mut self`. L'ordre/la priorité restent choix de
    /// l'implémentation plateforme.
    fn take_pending_idles(&mut self) -> Vec<crate::layout::IdleCallback> {
        Vec::new()
    }

    /// `createFont` — crée une fonte et remplit ses métriques.
    /// `try_everything` ≙ `tryEverything` (fallback agressif).
    fn create_font(&mut self, attrs: &FontAttrs, try_everything: bool) -> Rc<Font>;

    /// `fontExists`.
    fn font_exists(&mut self, name: &[u8]) -> bool;

    /// `createColor` — ressource couleur pour `0xRRGGBB`.
    fn create_color(&mut self, color: i32) -> Color;

    /// `createTooltip`.
    fn create_tooltip(&mut self, text: &[u8]) -> Rc<Tooltip>;

    /// `cancelTooltip`.
    fn cancel_tooltip(&mut self);

    /// `createImgbuf` — buffer image plateforme
    /// (`gamma` = encodage des données entrantes).
    fn create_imgbuf(
        &mut self,
        buf_type: crate::imgbuf::ImgbufType,
        width: i32,
        height: i32,
        gamma: f64,
    ) -> Rc<dyn Imgbuf>;

    /// `copySelection` — copie le texte sélectionné
    /// (`destination` = sélection primaire/presse-papier, indice
    /// plateforme verbatim).
    fn copy_selection(&mut self, text: &[u8], destination: i32);

    /// `getResourceFactory` — factory UI (≙ `ui::ResourceFactory`,
    /// seam `Any` tant que `dw/ui.hh` n'est pas porté).
    fn get_resource_factory(&mut self) -> Option<Rc<dyn std::any::Any>>;
}

/// `void (Layout::*func)()` amont — callback idle déclenché au
/// drain de la file par `Layout::process_idles`.
pub type IdleCallback = std::rc::Rc<dyn Fn(&mut crate::layout::Layout)>;
