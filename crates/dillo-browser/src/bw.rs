//! Port of `src/bw.c` + `bw.h` — data structures for each browser
//! window.
//!
//! `BrowserWindow` verbatim (bw.h:31-70) : listes de clients
//! (`RootClients`/`ImageClients` ≙ clés cache actives), `Docs`
//! (handles opaques ≙ `void*` — `DilloHtml*` amont), `PageUrls`
//! (dédoublonnées par `a_Url_cmp`), `nav_*` regroupés dans
//! [`Nav`] (nav.rs), `nav_expect_url`, `meta_refresh_*`,
//! `page_bugs`, `zoom`.
//!
//! Mutabilité intérieure **par champ** (`RefCell`/`Cell`) : amont
//! mute les champs via `BrowserWindow*` nu ; ici les seams
//! ([`NavBw`]) doivent toucher `bw` *pendant* `nav.push` — des
//! cellules par groupe de champs évitent tout double-emprunt
//! (`b.nav.borrow_mut()` tenu pendant `b.expect.borrow_mut()`).
//!
//! Le registre `static bws[]` amont devient [`BwRegistry`]
//! (possédant ; `bw_id` = index ≙ pointeur bw utilisé comme
//! identifiant par `a_UIcmd_*`/`nav.c`).
//!
//! Seam [`BwUi`] : `a_UIcmd_set_img_prog`/`set_buttons_sens`/
//! `finish_loading` appelés par `bw.c`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dillo_dw::layout::Layout;
use dillo_net::capi::capi_stop_client;
use dillo_net::url::DilloUrl;

use crate::nav::{Nav, NavBw};

/// `BW_Root` (bw.h) — clients racine (documents).
pub const BW_ROOT: i32 = 1;
/// `BW_Img` — clients images.
pub const BW_IMG: i32 = 2;
/// `BW_Force` — abort aussi la CCC (verbatim bw.h).
pub const BW_FORCE: i32 = 4;

/// Seam `a_UIcmd_*` appelés par `bw.c` (progression images,
/// sensibilité boutons, fin de chargement).
pub trait BwUi {
    /// `a_UIcmd_set_img_prog(bw, got, total, force)`.
    fn set_img_prog(&mut self, bw: u32, got: i32, total: i32, force: bool);
    /// `a_UIcmd_set_buttons_sens(bw)`.
    fn set_buttons_sens(&mut self, bw: u32);
    /// `a_UIcmd_finish_loading(bw)` — ≙ reset progress + title.
    fn finish_loading(&mut self, bw: u32);
}

/// `BrowserWindow` (bw.h) — état par fenêtre/onglet.
///
/// `docs`/`layout` sont des handles opaques : amont ce sont des
/// `void*` (`DilloHtml*`/`dw::core::Layout*`) — ici `usize`/
/// `Rc<RefCell<Layout>>`.
pub struct BrowserWindow {
    /// `bw->ui` — handle de la UI propriétaire (opaque ≙ `UI*`).
    pub ui: Cell<usize>,
    /// `bw->render_layout` (≙ `dw::core::Layout*`).
    pub layout: RefCell<Option<Rc<RefCell<Layout>>>>,
    /// `bw->Docs` — documents racine (`DilloHtml*` amont ≙ id).
    pub docs: RefCell<Vec<usize>>,
    /// `bw->RootClients` — clés cache des clients racine.
    pub root_clients: RefCell<Vec<i32>>,
    /// `bw->ImageClients` — clés cache des clients images.
    pub image_clients: RefCell<Vec<i32>>,
    /// `bw->NumImages`.
    pub num_images: Cell<i32>,
    /// `bw->NumImagesGot`.
    pub num_images_got: Cell<i32>,
    /// `bw->NumPendingStyleSheets`.
    pub num_pending_style_sheets: Cell<i32>,
    /// `bw->PageUrls` — dédoublonnées (`dList_find_custom` +
    /// `a_Url_cmp` ≙ `url_cmp == 0`).
    pub page_urls: RefCell<Vec<DilloUrl>>,
    /// `bw->nav_stack`/`nav_stack_ptr`/`meta_refresh_*` ≙ [`Nav`].
    pub nav: RefCell<Nav>,
    /// `bw->nav_expect_url` — URL « cliquée » en attente de dw.
    pub nav_expect_url: RefCell<Option<DilloUrl>>,
    /// `bw->redirect_level` — compteur anti-boucle (root seul).
    pub redirect_level: Cell<i32>,
    /// `bw->num_page_bugs`.
    pub num_page_bugs: Cell<i32>,
    /// `bw->page_bugs` (`Dstr` ≙ `String`).
    pub page_bugs: RefCell<String>,
    /// `bw->zoom` (1.0 ≙ verbatim `a_Bw_new`).
    pub zoom: Cell<f32>,
    /// `bw->bgColor` — lu par `a_Web_new` amont (seam `NavBw`).
    pub bg_color: Cell<i32>,
}

impl BrowserWindow {
    /// `dNew0(BrowserWindow)` + init verbatim (`a_Bw_new` :
    /// `nav_stack_ptr=-1` — dans `Nav::default`, `redirect_level
    /// =0`, `zoom=1.0`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            ui: Cell::new(0),
            layout: RefCell::new(None),
            docs: RefCell::new(Vec::new()),
            root_clients: RefCell::new(Vec::new()),
            image_clients: RefCell::new(Vec::new()),
            num_images: Cell::new(0),
            num_images_got: Cell::new(0),
            num_pending_style_sheets: Cell::new(0),
            page_urls: RefCell::new(Vec::new()),
            nav: RefCell::new(Nav::default()),
            nav_expect_url: RefCell::new(None),
            redirect_level: Cell::new(0),
            num_page_bugs: Cell::new(0),
            page_bugs: RefCell::new(String::new()),
            zoom: Cell::new(1.0),
            bg_color: Cell::new(0x00ff_ffff),
        }
    }

    /// `a_Bw_add_client` — verbatim : `root`→`RootClients` sinon
    /// `ImageClients`+`NumImages++`+progression ; premier client
    /// → `set_buttons_sens`.
    pub fn add_client(&self, bw_id: u32, key: i32, root: bool, ui: &mut dyn BwUi) {
        if root {
            self.root_clients.borrow_mut().push(key);
        } else {
            self.image_clients.borrow_mut().push(key);
            self.num_images.set(self.num_images.get() + 1);
            // ≙ progression image (a_UIcmd_set_img_prog).
            ui.set_img_prog(
                bw_id,
                self.num_images_got.get(),
                self.num_images.get(),
                true,
            );
        }
        if self.root_clients.borrow().len() + self.image_clients.borrow().len() == 1 {
            ui.set_buttons_sens(bw_id);
        }
    }

    /// `a_Bw_remove_client` — `0` trouvé, `1` sinon (verbatim).
    /// `dList_remove_fast` ≙ `swap_remove` (ordre non préservé
    /// amont aussi).
    pub fn remove_client(&self, key: i32) -> i32 {
        let mut rc = self.root_clients.borrow_mut();
        if let Some(i) = rc.iter().position(|k| *k == key) {
            rc.swap_remove(i);
            return 0;
        }
        drop(rc);
        let mut ic = self.image_clients.borrow_mut();
        if let Some(i) = ic.iter().position(|k| *k == key) {
            ic.swap_remove(i);
            drop(ic);
            self.num_images_got.set(self.num_images_got.get() + 1);
            0
        } else {
            1
        }
    }

    /// `a_Bw_close_client` — remove + compteurs + `finish_loading`
    /// quand plus de racines (verbatim bw.c:167-178).
    pub fn close_client(&self, bw_id: u32, key: i32, ui: &mut dyn BwUi) {
        if self.remove_client(key) == 0 {
            let (got, total) = (self.num_images_got.get(), self.num_images.get());
            ui.set_img_prog(bw_id, got, total, true);
            if got == total {
                ui.set_img_prog(bw_id, 0, 0, false);
            }
            if self.root_clients.borrow().is_empty() {
                ui.set_buttons_sens(bw_id);
                ui.finish_loading(bw_id);
            }
        }
    }

    /// `a_Bw_stop_clients` — `a_Capi_stop_client` par client,
    /// listes vidées (verbatim bw.c:184-203). `flags` =
    /// `BW_ROOT|BW_IMG` `|BW_FORCE`.
    pub fn stop_clients(&self, flags: i32) {
        if flags & BW_ROOT != 0 {
            let keys: Vec<i32> = self.root_clients.borrow_mut().drain(..).collect();
            for k in keys {
                capi_stop_client(k, flags & BW_FORCE != 0);
            }
        }
        if flags & BW_IMG != 0 {
            let keys: Vec<i32> = self.image_clients.borrow_mut().drain(..).collect();
            for k in keys {
                capi_stop_client(k, flags & BW_FORCE != 0);
            }
        }
    }

    /// `a_Bw_add_url` — append si pas déjà (`a_Url_cmp` ≙
    /// `url_cmp == 0`).
    pub fn add_url(&self, url: &DilloUrl) {
        let mut urls = self.page_urls.borrow_mut();
        if !urls.iter().any(|u| u.url_cmp(url) == 0) {
            urls.push(url.clone());
        }
    }

    /// `a_Bw_add_doc`.
    pub fn add_doc(&self, doc: usize) {
        self.docs.borrow_mut().push(doc);
    }

    /// `a_Bw_get_current_doc` — verbatim : 1 doc → le premier ;
    /// plusieurs → `MSG` (« multiple docs not implemented ») +
    /// `None` ≙ `NULL`.
    #[must_use]
    pub fn get_current_doc(&self) -> Option<usize> {
        let docs = self.docs.borrow();
        match docs.len() {
            1 => docs.first().copied(),
            n if n > 1 => {
                tracing::warn!("a_Bw_get_current_doc() multiple docs not implemented");
                None
            }
            _ => None,
        }
    }

    /// `a_Bw_get_url_doc` — doc si `url` ∈ `PageUrls` (verbatim —
    /// sert aux popups : page inchangée ?).
    #[must_use]
    pub fn get_url_doc(&self, url: &DilloUrl) -> Option<usize> {
        if self.page_urls.borrow().iter().any(|u| u.url_cmp(url) == 0) {
            self.get_current_doc()
        } else {
            None
        }
    }

    /// `a_Bw_remove_doc`.
    pub fn remove_doc(&self, doc: usize) {
        let mut docs = self.docs.borrow_mut();
        if let Some(i) = docs.iter().position(|d| *d == doc) {
            docs.swap_remove(i);
        }
    }

    /// `a_Bw_cleanup` — vide clients/urls + compteurs (verbatim).
    pub fn cleanup(&self) {
        self.root_clients.borrow_mut().clear();
        self.image_clients.borrow_mut().clear();
        self.page_urls.borrow_mut().clear();
        self.num_images.set(0);
        self.num_images_got.set(0);
        self.num_pending_style_sheets.set(0);
    }

    /// `a_Bw_expect` — verbatim (dup + free du précédent).
    pub fn expect(&self, url: &DilloUrl) {
        *self.nav_expect_url.borrow_mut() = Some(url.clone());
    }

    /// `a_Bw_cancel_expect`.
    pub fn cancel_expect(&self) {
        *self.nav_expect_url.borrow_mut() = None;
    }

    /// `a_Bw_expecting`.
    #[must_use]
    pub fn expecting(&self) -> bool {
        self.nav_expect_url.borrow().is_some()
    }

    /// `a_Bw_expected_url`.
    #[must_use]
    pub fn expected_url(&self) -> Option<DilloUrl> {
        self.nav_expect_url.borrow().clone()
    }
}

impl Default for BrowserWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// `BwRegistry` — ≙ `static bws[]`/`num_bws` (bw.c:29-30).
/// Possède les fenêtres ; `bw_id` = index ≙ `BrowserWindow*`.
/// `Rc` sans `RefCell` externe : les cellules sont par champ
/// (seams utilisables pendant un `borrow` d'un autre champ).
#[derive(Default)]
pub struct BwRegistry {
    /// `bws` — fenêtres actives.
    windows: Vec<Rc<BrowserWindow>>,
}

impl BwRegistry {
    /// `a_Bw_init` (verbatim — tableau vide).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `a_Bw_new` — crée + enregistre ; retourne `bw_id`.
    pub fn add(&mut self, bw: BrowserWindow) -> u32 {
        let id = u32::try_from(self.windows.len()).unwrap_or(u32::MAX);
        self.windows.push(Rc::new(bw));
        id
    }

    /// `a_Bw_get` — `None` hors bornes ≙ `NULL`.
    #[must_use]
    pub fn get(&self, id: u32) -> Option<Rc<BrowserWindow>> {
        self.windows.get(id as usize).cloned()
    }

    /// `a_Bw_num`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    /// `bws` vide ?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// `a_Bw_free` — retire du registre (les listes internes
    /// sont libérées par `Drop` ≙ `dList_free`/`a_Url_free`).
    pub fn free(&mut self, id: u32) {
        if (id as usize) < self.windows.len() {
            self.windows.remove(id as usize);
        }
    }
}

/// `NavBw` sur le registre — `bw_id` ≙ `BrowserWindow*` amont.
/// Chaque méthode n'emprunte que *sa* cellule (appelable pendant
/// `nav.borrow_mut().push(…, reg, …)` — verbatim amont où `nav.c`
/// mute `bw` via pointeur).
impl NavBw for BwRegistry {
    /// `a_Bw_expecting`.
    fn expecting(&mut self, bw: u32) -> bool {
        self.get(bw).is_some_and(|b| b.expecting())
    }

    /// `a_Bw_cancel_expect`.
    fn cancel_expect(&mut self, bw: u32) {
        if let Some(b) = self.get(bw) {
            b.cancel_expect();
        }
    }

    /// `a_Bw_expected_url`.
    fn expected_url(&mut self, bw: u32) -> Option<DilloUrl> {
        self.get(bw).and_then(|b| b.expected_url())
    }

    /// `a_Bw_expect`.
    fn expect(&mut self, bw: u32, url: &DilloUrl) {
        if let Some(b) = self.get(bw) {
            b.expect(url);
        }
    }

    /// `a_Bw_stop_clients` — verbatim (capi appelé ici ≙ bw.c).
    fn stop_clients(&mut self, bw: u32, which: i32) {
        if let Some(b) = self.get(bw) {
            b.stop_clients(which);
        }
    }

    /// `a_Bw_cleanup`.
    fn cleanup(&mut self, bw: u32) {
        if let Some(b) = self.get(bw) {
            b.cleanup();
        }
    }

    /// `a_Bw_add_client` — sans progression (la seam `NavBw` ne
    /// porte pas [`BwUi`] — la version UI est appelée par le
    /// shell via [`BrowserWindow::add_client`] directement).
    fn add_client(&mut self, bw: u32, key: i32, root: bool) {
        if let Some(b) = self.get(bw) {
            if root {
                b.root_clients.borrow_mut().push(key);
            } else {
                b.image_clients.borrow_mut().push(key);
                b.num_images.set(b.num_images.get() + 1);
            }
        }
    }

    /// `a_Bw_add_url`.
    fn add_url(&mut self, bw: u32, url: &DilloUrl) {
        if let Some(b) = self.get(bw) {
            b.add_url(url);
        }
    }

    /// `bw->bgColor`.
    fn bg_color(&mut self, bw: u32) -> i32 {
        self.get(bw).map_or(0x00ff_ffff, |b| b.bg_color.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dillo_net::url::url_new;

    struct FakeUi {
        img_prog: Vec<(i32, i32, bool)>,
        sens: u32,
        finish: u32,
    }

    impl BwUi for FakeUi {
        fn set_img_prog(&mut self, _b: u32, g: i32, t: i32, f: bool) {
            self.img_prog.push((g, t, f));
        }
        fn set_buttons_sens(&mut self, _b: u32) {
            self.sens += 1;
        }
        fn finish_loading(&mut self, _b: u32) {
            self.finish += 1;
        }
    }

    fn fake_ui() -> FakeUi {
        FakeUi {
            img_prog: vec![],
            sens: 0,
            finish: 0,
        }
    }

    /// `a_Bw_new` verbatim — compteurs à 0, `nav_stack_ptr=-1`,
    /// `zoom=1.0`.
    #[test]
    fn new_defaults() {
        let bw = BrowserWindow::new();
        assert_eq!(bw.nav.borrow().stack_ptr(), -1);
        assert_eq!(bw.zoom.get(), 1.0);
        assert!(bw.nav_expect_url.borrow().is_none());
        assert_eq!(bw.redirect_level.get(), 0);
    }

    /// `a_Bw_add_client` — image → `NumImages++` + progression ;
    /// premier client → `set_buttons_sens`.
    #[test]
    fn add_client_counts_and_progress() {
        let bw = BrowserWindow::new();
        let mut ui = fake_ui();
        bw.add_client(0, 7, false, &mut ui);
        assert_eq!(bw.num_images.get(), 1);
        assert_eq!(ui.img_prog, vec![(0, 1, true)]);
        assert_eq!(ui.sens, 1);
        // 2e client : pas de nouvel appel sens (len != 1).
        bw.add_client(0, 8, false, &mut ui);
        assert_eq!(ui.sens, 1);
    }

    /// `a_Bw_remove_client` — image → `NumImagesGot++`, `1` absent.
    #[test]
    fn remove_client_counts_got() {
        let bw = BrowserWindow::new();
        let mut ui = fake_ui();
        bw.add_client(0, 7, false, &mut ui);
        assert_eq!(bw.remove_client(7), 0);
        assert_eq!(bw.num_images_got.get(), 1);
        assert_eq!(bw.remove_client(99), 1);
    }

    /// `a_Bw_close_client` — toutes les images → reset (0,0,false) ;
    /// plus de racines → `finish_loading`.
    #[test]
    fn close_client_finish_when_no_roots() {
        let bw = BrowserWindow::new();
        let mut ui = fake_ui();
        bw.add_client(0, 1, true, &mut ui);
        bw.add_client(0, 7, false, &mut ui);
        bw.close_client(0, 7, &mut ui);
        bw.close_client(0, 1, &mut ui);
        assert_eq!(ui.finish, 1);
        assert!(ui.img_prog.contains(&(0, 0, false)));
    }

    /// `a_Bw_stop_clients` vide les listes (capi seam = cache —
    /// inerte sans enregistrement).
    #[test]
    fn stop_clients_drains_lists() {
        let bw = BrowserWindow::new();
        let mut ui = fake_ui();
        bw.add_client(0, 1, true, &mut ui);
        bw.add_client(0, 7, false, &mut ui);
        bw.stop_clients(BW_ROOT | BW_IMG);
        assert!(bw.root_clients.borrow().is_empty());
        assert!(bw.image_clients.borrow().is_empty());
    }

    /// `a_Bw_add_url` dédoublonne par `a_Url_cmp`.
    #[test]
    fn add_url_dedups() {
        let bw = BrowserWindow::new();
        let Some(u) = url_new(b"http://example.com/", None) else {
            panic!()
        };
        bw.add_url(&u);
        bw.add_url(&u);
        assert_eq!(bw.page_urls.borrow().len(), 1);
        // doc par url : verbatim `get_url_doc` ≙ url∈PageUrls.
        assert_eq!(bw.get_url_doc(&u), None);
        bw.add_doc(42);
        assert_eq!(bw.get_url_doc(&u), Some(42));
    }

    /// `a_Bw_get_current_doc` — verbatim « multiple docs not
    /// implemented » → `None`.
    #[test]
    fn current_doc_multi_is_none() {
        let bw = BrowserWindow::new();
        bw.add_doc(1);
        assert_eq!(bw.get_current_doc(), Some(1));
        bw.add_doc(2);
        assert_eq!(bw.get_current_doc(), None);
    }

    /// `a_Bw_cleanup` vide tout.
    #[test]
    fn cleanup_resets() {
        let bw = BrowserWindow::new();
        let mut ui = fake_ui();
        bw.add_client(0, 1, true, &mut ui);
        bw.add_client(0, 7, false, &mut ui);
        bw.cleanup();
        assert!(bw.root_clients.borrow().is_empty());
        assert!(bw.image_clients.borrow().is_empty());
        assert_eq!(bw.num_images.get(), 0);
        assert_eq!(bw.num_pending_style_sheets.get(), 0);
    }

    /// `a_Bw_expect`/`cancel`/`expecting` verbatim.
    #[test]
    fn expect_lifecycle() {
        let bw = BrowserWindow::new();
        let Some(u) = url_new(b"http://x/", None) else {
            panic!()
        };
        bw.expect(&u);
        assert!(bw.expecting());
        assert_eq!(bw.expected_url().unwrap_or_default().url_cmp(&u), 0);
        bw.cancel_expect();
        assert!(!bw.expecting());
    }

    /// `BwRegistry` ≙ `bws[]` — add/get/free/num + `NavBw`.
    #[test]
    fn registry_ops() {
        let mut reg = BwRegistry::new();
        let id = reg.add(BrowserWindow::new());
        assert_eq!(reg.len(), 1);
        assert!(reg.get(id).is_some());
        assert!(reg.get(9).is_none());
        // `NavBw::expect` route sur la fenêtre.
        let Some(u) = url_new(b"http://x/", None) else {
            panic!()
        };
        reg.expect(id, &u);
        assert!(reg.expecting(id));
        assert_eq!(reg.expected_url(id).unwrap_or_default().url_cmp(&u), 0);
        reg.free(id);
        assert!(reg.is_empty());
    }
}
