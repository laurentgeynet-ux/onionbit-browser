//! Port de `src/nav.c` + `nav.h` — pile de navigation par fenêtre.
//!
//! Amont, l'état vit **dans** `BrowserWindow` (`nav_stack`,
//! `nav_stack_ptr`, `meta_refresh_url`, `meta_refresh_status`). Ici il
//! est regroupé dans [`Nav`] — future champ `nav` de la `BrowserWindow`
//! portée ; `bw: u32` identifie la fenêtre pour les seams.
//!
//! Les `a_Bw_*`/`a_UIcmd_*`/`a_Dialog_choice` amont passent par les
//! traits [`NavBw`]/[`NavUi`] (implémentés par la future `bw`/`uicmd`).
//! Les opérations différées `a_Timeout_add(0.0, cb, bw)` passent par
//! [`NavSched`] : la closure amont capture `bw` — ici on diffère un
//! [`NavOp`] et l'appelant ré-entre via `repush_now`/`reload_now`/
//! `redirection0_now` (le registre de fenêtres vit dans le shell).

use std::rc::Rc;

use dillo_net::cache::{self, CaCb};
use dillo_net::history;
use dillo_net::http::HttpWeb;
use dillo_net::url::{self, flags as url_flags, DilloUrl};
use dillo_net::{capi, web};

/// `nav_stack_item` — index d'historique + position de scroll.
struct NavStackItem {
    /// `url_idx` — index dans `history`.
    url_idx: i32,
    /// `posx`/`posy`.
    posx: i32,
    posy: i32,
}

/// `BW_Root` + `BW_Img` (bw.h) — types de clients stoppés à la nav.
const BW_ROOT: i32 = 1;
const BW_IMG: i32 = 2;

/// État de navigation d'une fenêtre (champs `bw->nav_*`/`meta_refresh_*`
/// amont).
pub struct Nav {
    /// `bw->nav_stack` (`Dlist` de `nav_stack_item` amont).
    stack: Vec<NavStackItem>,
    /// `bw->nav_stack_ptr` — `[0 based; -1 = empty]`.
    stack_ptr: i32,
    /// `bw->meta_refresh_url`.
    pub meta_refresh_url: Option<DilloUrl>,
    /// `bw->meta_refresh_status`.
    pub meta_refresh_status: i32,
}

impl Default for Nav {
    /// `a_Bw_new` verbatim — `nav_stack_ptr = -1` (vide),
    /// `meta_refresh_status = 0`.
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            stack_ptr: -1,
            meta_refresh_url: None,
            meta_refresh_status: 0,
        }
    }
}

/// Seam `a_Bw_*` appelés par `nav.c` (implémentée par la `BrowserWindow`
/// portée — `bw` = l'id fenêtre passé aux `HttpWeb`).
pub trait NavBw {
    /// `a_Bw_expecting`.
    fn expecting(&mut self, bw: u32) -> bool;
    /// `a_Bw_cancel_expect`.
    fn cancel_expect(&mut self, bw: u32);
    /// `a_Bw_expected_url`.
    fn expected_url(&mut self, bw: u32) -> Option<DilloUrl>;
    /// `a_Bw_expect`.
    fn expect(&mut self, bw: u32, url: &DilloUrl);
    /// `a_Bw_stop_clients` (`BW_Root | BW_Img` [| `BW_Force`]).
    fn stop_clients(&mut self, bw: u32, which: i32);
    /// `a_Bw_cleanup`.
    fn cleanup(&mut self, bw: u32);
    /// `a_Bw_add_client` — `root` ≙ le `1` amont.
    fn add_client(&mut self, bw: u32, key: i32, root: bool);
    /// `a_Bw_add_url`.
    fn add_url(&mut self, bw: u32, url: &DilloUrl);
    /// `bw->bgColor` — lu par `a_Web_new` amont (`web->bgColor`).
    fn bg_color(&mut self, bw: u32) -> i32;
}

/// Seam `a_UIcmd_*` + `a_Dialog_choice` appelés par `nav.c`.
pub trait NavUi {
    /// `a_UIcmd_set_msg`.
    fn set_msg(&mut self, bw: u32, msg: &str);
    /// `a_UIcmd_set_location_text`.
    fn set_location_text(&mut self, bw: u32, text: &[u8]);
    /// `a_UIcmd_set_buttons_sens`.
    fn set_buttons_sens(&mut self, bw: u32);
    /// `a_UIcmd_get_scroll_xy`.
    fn get_scroll_xy(&mut self, bw: u32) -> (i32, i32);
    /// `a_UIcmd_set_scroll_xy`.
    fn set_scroll_xy(&mut self, bw: u32, x: i32, y: i32);
    /// `a_UIcmd_set_scroll_by_fragment`.
    fn set_scroll_by_fragment(&mut self, bw: u32, fragment: &[u8]);
    /// `a_UIcmd_finish_loading`.
    fn finish_loading(&mut self, bw: u32);
    /// `a_UIcmd_open_url_nt` (nouvel onglet).
    fn open_url_nt(&mut self, bw: u32, url: &DilloUrl, offset: i32);
    /// `a_UIcmd_open_url_nw` (nouvelle fenêtre).
    fn open_url_nw(&mut self, bw: u32, url: &DilloUrl);
    /// `a_Dialog_choice` — retourne l'index du bouton choisi.
    fn dialog_choice(&mut self, title: &str, msg: &str, alt1: &str, alt2: &str, alt3: &str) -> i32;
}

/// Union des seams `nav.c` — amont `a_Bw_*`/`a_UIcmd_*` sont des
/// globales librement mixées ; ici un seul objet joue les deux rôles
/// (deux `&mut` vers le même registre de fenêtres seraient
/// impossibles — `NavUi` rappelle `bw` pendant `push`).
pub trait NavIo: NavBw + NavUi {}
impl<T: NavBw + NavUi + ?Sized> NavIo for T {}

/// Opération de navigation différée par `a_Timeout_add(0.0, …)` amont.
#[derive(Clone, Copy)]
pub enum NavOp {
    /// `Nav_repush_callback`.
    Repush,
    /// `Nav_reload_callback`.
    Reload,
    /// `Nav_redirection0_callback`.
    Redirection0,
}

/// Seam de planification : le shell mappe `NavOp` →
/// `timeout_add(0.0, move || with_bw(bw, |bw| nav::…_now(…)))` — la
/// closure amont capture `bw` ; ici on re-joint la fenêtre par id.
pub trait NavSched {
    /// `a_Timeout_add(0.0, cb, bw)`.
    fn defer(&mut self, bw: u32, op: NavOp);
}

/// `URL_FRAGMENT` amont = `NPTR2STR` → `""` si absent.
fn fragment_of(u: &DilloUrl) -> &[u8] {
    u.fragment.as_deref().unwrap_or(b"")
}

impl Nav {
    /// `a_Nav_stack_ptr` — `[0 based; -1 = empty]`.
    #[must_use]
    pub fn stack_ptr(&self) -> i32 {
        self.stack_ptr
    }

    /// `a_Nav_stack_size` — `[1 based]`.
    #[must_use]
    pub fn stack_size(&self) -> i32 {
        i32::try_from(self.stack.len()).unwrap_or(i32::MAX)
    }

    /// `a_Nav_get_uidx` — `url_idx` de l'élément i (`-1` = erreur).
    #[must_use]
    pub fn get_uidx(&self, i: i32) -> i32 {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.stack.get(i))
            .map_or(-1, |nsi| nsi.url_idx)
    }

    /// `a_Nav_get_top_uidx` — `url_idx` du sommet (`-1` si vide).
    #[must_use]
    pub fn get_top_uidx(&self) -> i32 {
        self.get_uidx(self.stack_ptr)
    }

    /// `Nav_stack_move_ptr` — `dReturn_if_fail` hors bornes.
    fn stack_move_ptr(&mut self, offset: i32) {
        if offset != 0 {
            let nptr = self.stack_ptr + offset;
            if nptr >= 0 && nptr < self.stack_size() {
                self.stack_ptr = nptr;
            }
        }
    }

    /// `Nav_stack_truncate` — retire `pos` et au-delà.
    fn stack_truncate(&mut self, pos: i32) {
        if pos < 0 {
            return;
        }
        self.stack.truncate(usize::try_from(pos).unwrap_or(0));
    }

    /// `Nav_stack_append` — empile `{url_idx, 0, 0}`.
    fn stack_append(&mut self, url_idx: i32) {
        self.stack.push(NavStackItem {
            url_idx,
            posx: 0,
            posy: 0,
        });
    }

    /// `Nav_get_scroll_pos` — position sauvée au sommet (`0,0` si vide).
    fn get_scroll_pos(&self) -> (i32, i32) {
        usize::try_from(self.stack_ptr)
            .ok()
            .and_then(|i| self.stack.get(i))
            .map_or((0, 0), |nsi| (nsi.posx, nsi.posy))
    }

    /// `Nav_save_scroll_pos` — enregistre la position à `idx`.
    #[allow(clippy::similar_names)] // `posx`/`posy` = champs amont
    fn save_scroll_pos(&mut self, idx: i32, posx: i32, posy: i32) {
        if let Ok(i) = usize::try_from(idx) {
            if let Some(nsi) = self.stack.get_mut(i) {
                nsi.posx = posx;
                nsi.posy = posy;
            }
        }
    }

    /// `Nav_stack_clean` — retire les doublons adjacents au sommet
    /// (cas des redirections).
    fn stack_clean(&mut self) {
        let n = self.stack_size();
        if n >= 2 && self.get_uidx(n - 2) == self.get_uidx(n - 1) {
            self.stack.pop();
            if self.stack_ptr >= self.stack_size() {
                self.stack_ptr = self.stack_size() - 1;
            }
        }
    }

    /// `Nav_open_url` — crée le `DilloWeb` racine et demande le cache.
    ///
    /// `a_Web_new` + `WEB_RootUrl` + `a_Capi_open_url` + `a_Bw_add_*`.
    /// Retourne la clé client (`0` = refusé — amont : `!= 0` pour agir).
    fn open_url(
        &mut self,
        bw_id: u32,
        url: &DilloUrl,
        requester: Option<DilloUrl>,
        offset: i32,
        seams: &mut dyn NavIo,
    ) {
        let force_reload = url.flags & (url_flags::E2E_QUERY | url_flags::RELOAD_FROM_CACHE) != 0;
        let ignore_scroll = url.flags & url_flags::IGNORE_SCROLL != 0;

        // URL de la page courante + sauvegarde du scroll.
        let idx = self.stack_ptr;
        let old_url = history::history_get_url(self.get_uidx(idx));
        if let Some(old) = &old_url {
            if !ignore_scroll {
                let (x, y) = seams.get_scroll_xy(bw_id);
                self.save_scroll_pos(idx, x, y);
                let _ = old;
            }
        }

        self.stack_move_ptr(offset);

        // Recharger si l'URL (fragment compris) diffère.
        let mut must_load = force_reload || old_url.is_none();
        if let Some(old) = &old_url {
            must_load |= old.url_cmp(url) != 0 || fragment_of(old) != fragment_of(url);
        }

        if must_load {
            seams.stop_clients(bw_id, BW_ROOT | BW_IMG);
            seams.cleanup(bw_id);

            let mut w = web::web_new(url.clone(), requester);
            w.flags |= web::WEB_ROOT_URL;
            w.bw = bw_id;
            w.bg_color = seams.bg_color(bw_id);
            let client_key = capi::capi_open_url(Rc::new(w), None);
            if client_key != 0 {
                seams.add_client(bw_id, client_key, true);
                seams.add_url(bw_id, url);
            }
        }
    }

    /// `a_Nav_cancel_expect` — annule l'attente (l'abort du flux reste
    /// à l'appelant) ; décrémente `meta_refresh_status` > 0.
    pub fn cancel_expect(&mut self, bw_id: u32, seams: &mut dyn NavIo) {
        if seams.expecting(bw_id) {
            seams.cancel_expect(bw_id);
            seams.set_buttons_sens(bw_id);
        }
        if self.meta_refresh_status > 0 {
            self.meta_refresh_status -= 1;
        }
    }

    /// `a_Nav_cancel_expect_if_eq` — annule si `url` == expected.
    pub fn cancel_expect_if_eq(&mut self, bw_id: u32, url: &DilloUrl, seams: &mut dyn NavIo) {
        if let Some(e) = seams.expected_url(bw_id) {
            if e.url_cmp(url) == 0 {
                self.cancel_expect(bw_id, seams);
            }
        }
    }

    /// `a_Nav_expect_done` — l'URL attendue a répondu (root seulement ;
    /// les repush passent deux fois ici, comme amont).
    pub fn expect_done(&mut self, bw_id: u32, seams: &mut dyn NavIo) {
        let mut goto_old_scroll = true;
        let mut fragment: Option<Vec<u8>> = None;

        if seams.expecting(bw_id) {
            if let Some(mut url) = seams.expected_url(bw_id) {
                let reload = url.flags & url_flags::RELOAD_PAGE != 0;
                let repush = url.flags & url_flags::RELOAD_FROM_CACHE != 0;
                let e2equery = url.flags & url_flags::E2E_QUERY != 0;
                if let Some(f) = &url.fragment {
                    let d = url::decode_hex_str(f);
                    if !d.is_empty() {
                        fragment = Some(d);
                    }
                }

                // Retire E2EQuery/Reload*/IgnoreScroll avant historique.
                url.flags &= !(url_flags::E2E_QUERY
                    | url_flags::RELOAD_PAGE
                    | url_flags::RELOAD_FROM_CACHE
                    | url_flags::IGNORE_SCROLL);
                let url_idx = history::history_add_url(&mut url);

                if repush || reload {
                    // repush/reload : pile inchangée (MSG amont).
                } else {
                    self.stack_truncate(self.stack_ptr + 1);
                    self.stack_append(url_idx);
                    self.stack_move_ptr(1);
                }

                if fragment.is_some() {
                    goto_old_scroll = false;
                    if repush {
                        let (px, py) = self.get_scroll_pos();
                        if px != 0 || py != 0 {
                            goto_old_scroll = true;
                        }
                    } else if e2equery {
                        // Reset scroll : le repush ira au fragment.
                        self.save_scroll_pos(self.stack_ptr, 0, 0);
                    }
                }
            }
            self.cancel_expect(bw_id, seams);
        }

        if goto_old_scroll {
            let (px, py) = self.get_scroll_pos();
            seams.set_scroll_xy(bw_id, px, py);
        } else if let Some(f) = &fragment {
            seams.set_scroll_by_fragment(bw_id, f);
        } else {
            seams.set_scroll_xy(bw_id, 0, 0);
        }

        self.stack_clean();
        seams.set_buttons_sens(bw_id);
        seams.finish_loading(bw_id);
    }

    /// `a_Nav_push` — `url` devient la page attendue + demande au cache.
    pub fn push(
        &mut self,
        bw_id: u32,
        url: &DilloUrl,
        requester: Option<DilloUrl>,
        seams: &mut dyn NavIo,
    ) {
        if let Some(e) = seams.expected_url(bw_id) {
            // Déjà attendue (double-clic probable) — fragment compris.
            if e.url_cmp(url) == 0 && fragment_of(&e) == fragment_of(url) {
                return;
            }
        }
        self.cancel_expect(bw_id, seams);
        seams.expect(bw_id, url);
        self.open_url(bw_id, url, requester, 0, seams);
        let mut u = url.clone();
        let text = u.url_str().to_vec();
        seams.set_location_text(bw_id, &text);
    }

    /// `Nav_repush` — corps du callback différé (reload depuis le
    /// cache ; utilisé par le changement de charset META).
    pub fn repush_now(&mut self, bw_id: u32, seams: &mut dyn NavIo) {
        self.cancel_expect(bw_id, seams);
        if self.stack_size() > 0 {
            if let Some(mut url) = history::history_get_url(self.get_top_uidx()) {
                url.flags |= url_flags::RELOAD_FROM_CACHE;
                seams.expect(bw_id, &url);
                self.open_url(bw_id, &url, None, 0, seams);
            }
        }
    }

    /// `a_Nav_repush` — diffère via timeout 0 (≙ `a_Timeout_add`).
    pub fn repush(&mut self, bw_id: u32, sched: &mut dyn NavSched) {
        sched.defer(bw_id, NavOp::Repush);
    }

    /// `Nav_redirection0_callback` — corps du callback différé.
    pub fn redirection0_now(&mut self, bw_id: u32, seams: &mut dyn NavIo) {
        let referer = history::history_get_url(self.get_top_uidx());
        if self.meta_refresh_status == 2 {
            self.stack_move_ptr(-1);
            if let Some(url) = self.meta_refresh_url.clone() {
                self.push(bw_id, &url, referer, seams);
            }
        }
        self.meta_refresh_url = None;
        self.meta_refresh_status = 0;
    }

    /// `a_Nav_redirection0` — redirection META à délai nul : stocke
    /// l'URL (`E2EQuery|IgnoreScroll`), `status = 2`, puis diffère.
    pub fn redirection0(&mut self, bw_id: u32, new_url: &DilloUrl, sched: &mut dyn NavSched) {
        self.meta_refresh_url = Some(new_url.clone());
        if let Some(u) = &mut self.meta_refresh_url {
            u.flags = new_url.flags | url_flags::E2E_QUERY | url_flags::IGNORE_SCROLL;
        }
        self.meta_refresh_status = 2;
        sched.defer(bw_id, NavOp::Redirection0);
    }

    /// `a_Nav_back` — page précédente.
    pub fn back(&mut self, bw_id: u32, seams: &mut dyn NavIo) {
        let idx = self.stack_ptr - 1;
        self.cancel_expect(bw_id, seams);
        if idx >= 0 {
            seams.set_msg(bw_id, "");
            if let Some(url) = history::history_get_url(self.get_uidx(idx)) {
                self.open_url(bw_id, &url, None, -1, seams);
            }
        }
    }

    /// `a_Nav_back_nt` — page précédente dans un nouvel onglet.
    pub fn back_nt(&self, bw_id: u32, seams: &mut dyn NavUi) {
        let idx = self.stack_ptr - 1;
        if idx >= 0 {
            seams.set_msg(bw_id, "");
            if let Some(url) = history::history_get_url(self.get_uidx(idx)) {
                seams.open_url_nt(bw_id, &url, -1);
            }
        }
    }

    /// `a_Nav_forw` — page suivante.
    pub fn forw(&mut self, bw_id: u32, seams: &mut dyn NavIo) {
        let idx = self.stack_ptr + 1;
        self.cancel_expect(bw_id, seams);
        if idx < self.stack_size() {
            seams.set_msg(bw_id, "");
            if let Some(url) = history::history_get_url(self.get_uidx(idx)) {
                self.open_url(bw_id, &url, None, 1, seams);
            }
        }
    }

    /// `a_Nav_forw_nt` — page suivante dans un nouvel onglet.
    pub fn forw_nt(&self, bw_id: u32, seams: &mut dyn NavUi) {
        let idx = self.stack_ptr + 1;
        if idx < self.stack_size() {
            seams.set_msg(bw_id, "");
            if let Some(url) = history::history_get_url(self.get_uidx(idx)) {
                seams.open_url_nt(bw_id, &url, 1);
            }
        }
    }

    /// `a_Nav_home` — `prefs.home` passée par l'appelant (le `Prefs`
    /// est owned, pas global).
    pub fn home(&mut self, bw_id: u32, home: &DilloUrl, seams: &mut dyn NavIo) {
        self.push(bw_id, home, None, seams);
    }

    /// `Nav_reload_callback` — corps du callback différé.
    ///
    /// Re-post confirmé via `a_Dialog_choice` sauf `dpi:/vsource/`
    /// (reload autorisé) ; marque `ReloadPage|E2EQuery` et retire
    /// `SpamSafe` (reload explicite).
    pub fn reload_now(&mut self, bw_id: u32, seams: &mut dyn NavIo) {
        self.cancel_expect(bw_id, seams);
        if self.stack_size() == 0 {
            return;
        }
        if let Some(h_url) = history::history_get_url(self.get_top_uidx()) {
            let mut confirmed = true;
            let is_vsource = h_url
                .scheme
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(b"dpi"))
                && h_url
                    .path
                    .as_deref()
                    .is_some_and(|p| p.starts_with(b"/vsource/"));
            if !is_vsource && h_url.flags & url::URL_POST != 0 {
                // Tentative de re-POST : confirmation utilisateur.
                let choice = seams.dialog_choice(
                    "Dillo: Repost form?",
                    "Repost form data?",
                    "No",
                    "Yes",
                    "Cancel",
                );
                confirmed = choice == 2; // "Yes"
            }

            if confirmed {
                let mut r_url = h_url;
                r_url.flags |= url_flags::RELOAD_PAGE | url_flags::E2E_QUERY;
                r_url.flags &= !url_flags::SPAM_SAFE;
                seams.expect(bw_id, &r_url);
                self.open_url(bw_id, &r_url, None, 0, seams);
            }
        }
    }

    /// `a_Nav_reload` — diffère (le timeout laisse les CCC se finir).
    pub fn reload(&mut self, bw_id: u32, sched: &mut dyn NavSched) {
        sched.defer(bw_id, NavOp::Reload);
    }

    /// `a_Nav_jump` — saute à `stack_ptr + offset` (`new_bw` ≙
    /// `a_UIcmd_open_url_nw`).
    pub fn jump(&mut self, bw_id: u32, offset: i32, new_bw: bool, seams: &mut dyn NavIo) {
        let idx = self.stack_ptr + offset;
        let url = history::history_get_url(self.get_uidx(idx));
        if new_bw {
            if let Some(u) = &url {
                seams.open_url_nw(bw_id, u);
            }
        } else {
            self.cancel_expect(bw_id, seams);
            if let Some(u) = &url {
                self.open_url(bw_id, u, None, offset, seams);
            }
            seams.set_buttons_sens(bw_id);
        }
    }
}

/// `Nav_save_cb` — écrit le flux dans `web->stream` depuis
/// `SavedBytes`, puis message final « File saved (N Bytes) ».
///
/// Amont : `Op != 0` (`CA_Close`/`CA_Abort`) → flush+fstat+fclose+msg ;
/// sinon (`CA_Send`) écrit le delta. `saved_bytes` est partagé sur le
/// `Rc` web comme `Web->SavedBytes` amont.
pub fn nav_save_cb(web: &Rc<HttpWeb>) -> CaCb {
    let web = web.clone();
    Box::new(move |op, client| {
        if op == cache::CA_SEND {
            let saved = web.saved_bytes.get();
            if client.buf.len() > saved {
                let bytes = &client.buf[saved..];
                if let Some(f) = web.stream.borrow_mut().as_mut() {
                    use std::io::Write;
                    if let Ok(n) = f.write(bytes) {
                        web.saved_bytes.set(saved + n);
                    }
                }
            }
        } else {
            // `fclose(web->stream)` + `a_UIcmd_set_msg(bw, "File saved..")`.
            if let Some(f) = web.stream.borrow_mut().take() {
                let size = f
                    .metadata()
                    .map_or(0, |m| usize::try_from(m.len()).unwrap_or(0));
                drop(f);
                cache::cache_ui_msg(web.bw, &format!("File saved ({size} Bytes)"));
            }
        }
    })
}

/// `a_Nav_save_url` — `a_Web_new` + `WEB_Download` + `a_Capi_open_url`
/// avec [`nav_save_cb`].
pub fn nav_save_url(bw_id: u32, url: &DilloUrl, filename: &str) {
    let mut w = web::web_new(url.clone(), None);
    w.filename = Some(filename.to_string());
    w.flags |= web::WEB_DOWNLOAD;
    w.bw = bw_id;
    let w = Rc::new(w);
    let cb = nav_save_cb(&w);
    let _ = capi::capi_open_url(w, Some(cb));
}

/// `a_Nav_get_buf` — wrapper `a_Capi_get_buf`.
#[must_use]
pub fn nav_get_buf(url: &DilloUrl) -> Option<Vec<u8>> {
    capi::capi_get_buf(url)
}

/// `a_Nav_unref_buf` — wrapper `a_Capi_unref_buf`.
pub fn nav_unref_buf(url: &DilloUrl) {
    capi::capi_unref_buf(url);
}

/// `a_Nav_get_content_type` — wrapper `a_Capi_get_content_type`.
#[must_use]
pub fn nav_get_content_type(url: &DilloUrl) -> Option<Vec<u8>> {
    capi::capi_get_content_type(url)
}

/// `a_Nav_set_vsource_url` — wrapper `a_Capi_set_vsource_url`.
pub fn nav_set_vsource_url(url: &DilloUrl) {
    capi::capi_set_vsource_url(url);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use dillo_net::url::url_new;

    /// `NavIo` de test : expect/expected + appels mémorisés
    /// (≙ `FakeBw`/`FakeUi` fusionnés — la seam est unique).
    struct FakeSeams {
        expected: Option<DilloUrl>,
        calls: Rc<RefCell<Vec<String>>>,
        scroll: (i32, i32),
        evs: Rc<RefCell<Vec<String>>>,
        dialog_ret: i32,
    }
    impl NavBw for FakeSeams {
        fn expecting(&mut self, _bw: u32) -> bool {
            self.expected.is_some()
        }
        fn cancel_expect(&mut self, _bw: u32) {
            self.expected = None;
        }
        fn expected_url(&mut self, _bw: u32) -> Option<DilloUrl> {
            self.expected.clone()
        }
        fn expect(&mut self, _bw: u32, url: &DilloUrl) {
            self.expected = Some(url.clone());
        }
        fn stop_clients(&mut self, _bw: u32, which: i32) {
            self.calls.borrow_mut().push(format!("stop:{which}"));
        }
        fn cleanup(&mut self, _bw: u32) {
            self.calls.borrow_mut().push("cleanup".into());
        }
        fn add_client(&mut self, _bw: u32, key: i32, root: bool) {
            self.calls.borrow_mut().push(format!("client:{key}:{root}"));
        }
        fn add_url(&mut self, _bw: u32, _url: &DilloUrl) {
            self.calls.borrow_mut().push("add_url".into());
        }
        fn bg_color(&mut self, _bw: u32) -> i32 {
            0
        }
    }
    impl NavUi for FakeSeams {
        fn set_msg(&mut self, _bw: u32, msg: &str) {
            self.evs.borrow_mut().push(format!("msg:{msg}"));
        }
        fn set_location_text(&mut self, _bw: u32, text: &[u8]) {
            self.evs
                .borrow_mut()
                .push(format!("loc:{}", String::from_utf8_lossy(text)));
        }
        fn set_buttons_sens(&mut self, _bw: u32) {
            self.evs.borrow_mut().push("sens".into());
        }
        fn get_scroll_xy(&mut self, _bw: u32) -> (i32, i32) {
            self.scroll
        }
        fn set_scroll_xy(&mut self, _bw: u32, x: i32, y: i32) {
            self.scroll = (x, y);
            self.evs.borrow_mut().push(format!("xy:{x}:{y}"));
        }
        fn set_scroll_by_fragment(&mut self, _bw: u32, f: &[u8]) {
            self.evs
                .borrow_mut()
                .push(format!("frag:{}", String::from_utf8_lossy(f)));
        }
        fn finish_loading(&mut self, _bw: u32) {
            self.evs.borrow_mut().push("fin".into());
        }
        fn open_url_nt(&mut self, _bw: u32, _url: &DilloUrl, off: i32) {
            self.evs.borrow_mut().push(format!("nt:{off}"));
        }
        fn open_url_nw(&mut self, _bw: u32, _url: &DilloUrl) {
            self.evs.borrow_mut().push("nw".into());
        }
        fn dialog_choice(&mut self, _t: &str, _m: &str, _a1: &str, _a2: &str, _a3: &str) -> i32 {
            self.dialog_ret
        }
    }

    struct FakeSched {
        ops: Rc<RefCell<VecDeque<NavOp>>>,
    }
    impl NavSched for FakeSched {
        fn defer(&mut self, _bw: u32, op: NavOp) {
            self.ops.borrow_mut().push_back(op);
        }
    }

    fn mk() -> (Nav, FakeSeams, FakeSched) {
        (
            Nav::default(),
            FakeSeams {
                expected: None,
                calls: Rc::new(RefCell::new(Vec::new())),
                scroll: (0, 0),
                evs: Rc::new(RefCell::new(Vec::new())),
                dialog_ret: 2,
            },
            FakeSched {
                ops: Rc::new(RefCell::new(VecDeque::new())),
            },
        )
    }

    fn u(s: &str) -> DilloUrl {
        url_new(s.as_bytes(), None).expect("url")
    }

    #[test]
    fn stack_push_truncates_and_dedups() {
        let (mut nav, mut s, _sched) = mk();
        history::history_freeall();
        // Push A : pile vide → expect + open_url (pas de client sans capi
        // http — key 0 → pas d'add_client ; la pile bouge à expect_done).
        nav.push(1, &u("http://a/"), None, &mut s);
        assert!(s.expected.is_some());
        // expect_done empile l'url_idx dans history.
        nav.expect_done(1, &mut s);
        assert_eq!(nav.stack_size(), 1);
        assert_eq!(nav.stack_ptr(), 0);
        // Push B → empile.
        nav.push(1, &u("http://b/"), None, &mut s);
        nav.expect_done(1, &mut s);
        assert_eq!(nav.stack_size(), 2);
        assert_eq!(nav.stack_ptr(), 1);
        // Back → pointeur -1 (open_url offset -1, sans client réel).
        nav.back(1, &mut s);
        // Re-push A : truncate à ptr+1 puis append → taille 2, doublon
        // adjacent (A déjà en position 0) → stack_clean le retire.
        nav.push(1, &u("http://a/"), None, &mut s);
        nav.expect_done(1, &mut s);
        assert_eq!(nav.stack_size(), 1);
        assert_eq!(nav.get_uidx(0), nav.get_top_uidx());
    }

    #[test]
    fn push_same_expected_url_is_ignored() {
        let (mut nav, mut s, _sched) = mk();
        let url = u("http://a/");
        s.expect(1, &url);
        let before = s.evs.borrow().len();
        nav.push(1, &url, None, &mut s);
        // Double-clic : retour immédiat, aucun événement UI.
        assert_eq!(s.evs.borrow().len(), before);
    }

    #[test]
    fn scroll_saved_and_restored() {
        let (mut nav, mut s, _sched) = mk();
        history::history_freeall();
        nav.push(1, &u("http://a/"), None, &mut s);
        nav.expect_done(1, &mut s);
        // L'utilisateur scrolle à (10,20) sur A (après le restore à 0,0
        // de expect_done).
        s.scroll = (10, 20);
        // La navigation vers B sauve le scroll de A (10,20) et
        // restaure celui de B (0,0).
        nav.push(1, &u("http://b/"), None, &mut s);
        nav.expect_done(1, &mut s);
        assert_eq!(nav.get_scroll_pos(), (0, 0));
        // Back → le scroll sauvé d'A est restauré.
        nav.back(1, &mut s);
        nav.expect_done(1, &mut s);
        // (10,20) sauvé en pile → goto_old_scroll.
        assert!(s.evs.borrow().iter().any(|e| e == "xy:10:20"));
    }

    #[test]
    fn repush_marks_reload_from_cache() {
        let (mut nav, mut s, mut sched) = mk();
        history::history_freeall();
        nav.push(1, &u("http://a/"), None, &mut s);
        nav.expect_done(1, &mut s);
        nav.repush(1, &mut sched);
        assert!(matches!(
            sched.ops.borrow_mut().pop_front(),
            Some(NavOp::Repush)
        ));
        nav.repush_now(1, &mut s);
        let e = s.expected.expect("expect");
        assert!(e.flags & url_flags::RELOAD_FROM_CACHE != 0);
    }

    #[test]
    fn reload_post_asks_confirmation() {
        let (mut nav, mut s, mut sched) = mk();
        history::history_freeall();
        let mut url = u("http://form/");
        url.flags |= url::URL_POST;
        // Simule : l'URL du sommet est une page POST.
        let idx = history::history_add_url(&mut url.clone());
        nav.stack_append(idx);
        nav.stack_move_ptr(1);
        // "No" (dialog_ret=1) → pas d'expect.
        s.dialog_ret = 1;
        nav.reload(1, &mut sched);
        nav.reload_now(1, &mut s);
        assert!(s.expected.is_none());
        // "Yes" → expect avec RELOAD_PAGE|E2E_QUERY, SPAM_SAFE retiré.
        url.flags |= url_flags::SPAM_SAFE;
        let idx2 = history::history_add_url(&mut url.clone());
        nav.stack_append(idx2);
        nav.stack_move_ptr(1);
        s.dialog_ret = 2;
        nav.reload_now(1, &mut s);
        let e = s.expected.expect("expect");
        assert!(e.flags & url_flags::RELOAD_PAGE != 0);
        assert!(e.flags & url_flags::E2E_QUERY != 0);
        assert_eq!(e.flags & url_flags::SPAM_SAFE, 0);
    }

    #[test]
    fn redirection0_replaces_top() {
        let (mut nav, mut s, mut sched) = mk();
        history::history_freeall();
        nav.push(1, &u("http://a/"), None, &mut s);
        nav.expect_done(1, &mut s);
        nav.redirection0(1, &u("http://b/"), &mut sched);
        assert_eq!(nav.meta_refresh_status, 2);
        assert!(nav
            .meta_refresh_url
            .as_ref()
            .is_some_and(|u| u.flags & url_flags::E2E_QUERY != 0));
        nav.redirection0_now(1, &mut s);
        assert_eq!(nav.meta_refresh_status, 0);
        assert!(nav.meta_refresh_url.is_none());
        // Push b a été déclenché (expect en cours).
        assert!(s.expected.is_some());
    }

    #[test]
    fn cancel_expect_decrements_meta_refresh() {
        let (mut nav, mut s, _sched) = mk();
        nav.meta_refresh_status = 3;
        s.expect(1, &u("http://a/"));
        nav.cancel_expect(1, &mut s);
        assert_eq!(nav.meta_refresh_status, 2);
        assert!(s.expected.is_none());
    }
}
