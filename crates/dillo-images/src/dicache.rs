//! Port de `src/dicache.c` + `dicache.h` — cache d'images décompressées.
//!
//! `CachedIMGs` amont (Dlist triée url+version) → `thread_local` de
//! `Rc<RefCell<DicacheEntry>>` ; `RefCount` compte les clients cache
//! (pas l'`Rc` technique — même sémantique amont), `a_Imgbuf_unref` ≙
//! drop du `Rc<Imgbuf>` partagé.
//!
//! Décodeurs : `DicEntry->Decoder`/`DecoderData` amont sont fusionnés
//! en [`ImageDecoder`] (le `*Data`/`CbData` void* devient `self`) —
//! `a_Xxx_new` → factory [`DecoderFactory`], `a_Xxx_callback` →
//! [`ImageDecoder::callback`]. À la suppression d'entrée amont appelle
//! `Decoder(CA_Abort, DecoderData)` — ici [`ImageDecoder::abort`].
//!
//! `Web->Image` ≙ `HttpWeb::image` (`Rc<dyn Any>` → `Rc<RefCell<
//! DilloImage>>`) ; `a_Bw_close_client`/`a_UIcmd_set_msg` passent par
//! les accesseurs `cache` cross-module (seams injectées).

use std::cell::RefCell;
use std::rc::Rc;

use dillo_common::bitvec::Bitvec;
use dillo_common::dlib;
use dillo_net::cache::{self, CaCb, CacheClient, CA_ABORT, CA_CLOSE, CA_SEND};
use dillo_net::http::HttpWeb;
use dillo_net::url::DilloUrl;

use crate::image::{
    image_abort, image_close, image_get_dw, image_new, image_ref, image_set_parms, image_write,
    DilloImage, ImgRenderer,
};
use crate::imgbuf::{rgb_line, ImgType, Imgbuf, ImgbufType};

/// `DIC_Gif`/`DIC_Png`/`DIC_Webp`/`DIC_Jpeg`/`DIC_Svg` (dicache.c).
pub const DIC_GIF: usize = 0;
/// `DIC_Png`.
pub const DIC_PNG: usize = 1;
/// `DIC_Webp`.
pub const DIC_WEBP: usize = 2;
/// `DIC_Jpeg`.
pub const DIC_JPEG: usize = 3;
/// `DIC_Svg`.
pub const DIC_SVG: usize = 4;
/// `DIC_MAX`.
pub const DIC_MAX: usize = 5;

/// `format_name[]` amont.
const FORMAT_NAME: [&str; DIC_MAX] = ["gif", "png", "webp", "jpeg", "svg"];

/// `DIC_Last` — « dernière version » symbolique.
pub const DIC_LAST: i32 = -1;
/// `DIF_Last`.
const DIF_LAST: i16 = 1;
/// `DIF_Valid`.
const DIF_VALID: i16 = 2;

/// `DicEntryState` — état de l'entrée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DicEntryState {
    /// `DIC_Empty`.
    Empty,
    /// `DIC_SetParms`.
    SetParms,
    /// `DIC_SetCmap`.
    SetCmap,
    /// `DIC_Write`.
    Write,
    /// `DIC_Close`.
    Close,
    /// `DIC_Abort`.
    Abort,
}

/// `CA_Callback_t` côté décodeur — `a_Xxx_callback(Op, Client)` ;
/// `client.buf` ≙ `Client->Buf`/`BufSize` (snapshot complet — le delta
/// est suivi par `decoded_size` comme amont).
pub trait ImageDecoder {
    /// `Decoder(Op, Client)` amont.
    fn callback(&mut self, op: i32, client: &mut CacheClient);
    /// `Decoder(CA_Abort, DecoderData)` amont — appelé à la suppression
    /// de l'entrée (pas de client disponible ; se libérer).
    fn abort(&mut self) {}
}

/// `a_Xxx_new(Image, url, version)` — factory d'un décodeur attaché à
/// une entrée.
pub type DecoderFactory =
    fn(image: Rc<RefCell<DilloImage>>, url: DilloUrl, version: i32) -> Box<dyn ImageDecoder>;

/// `DICacheEntry` — une version d'une URL dans le dicache.
pub struct DicacheEntry {
    /// `url` — clé primaire (`a_Url_dup` amont).
    url: DilloUrl,
    /// `type` — `DilloImgType`.
    img_type: ImgType,
    /// `width`/`height` — depuis les données image.
    width: u32,
    /// `height`.
    height: u32,
    /// `Flags` — `DIF_*`.
    flags: i16,
    /// `SurvCleanup` — passes de cleanup à survivre.
    surv_cleanup: i16,
    /// `cmap` — palette (`3*num_colors_max`).
    cmap: Option<Vec<u8>>,
    /// `v_imgbuf` — buffer RGB partagé (`a_Imgbuf_unref` ≙ drop `Rc`).
    imgbuf: Option<Rc<Imgbuf>>,
    /// `TotalSize` — `3*w*h` (invariant `dicache_size_total`).
    total_size: usize,
    /// `ScanNumber` — scan courant.
    scan_number: u32,
    /// `BitVec` — lignes décodées.
    bitvec: Option<Bitvec>,
    /// `State`.
    state: DicEntryState,
    /// `RefCount` — compte les **clients cache** (pas l'`Rc`).
    ref_count: i32,
    /// `version` — numéro de version de l'URL (>0).
    version: i32,
    /// `DecodedSize` — données déjà passées au décodeur.
    decoded_size: usize,
    /// `Decoder`/`DecoderData` — fusionnés en objet.
    decoder: Option<Box<dyn ImageDecoder>>,
}

/// `CachedIMGs` + `dicache_size_total` + `disabled_formats` amont.
pub struct Dicache {
    /// `CachedIMGs` — triée (`url_cmp`, `version`) ≙ `dList_insert_sorted`.
    entries: Vec<Rc<RefCell<DicacheEntry>>>,
    /// `dicache_size_total` — invariant : somme des `TotalSize`.
    size_total: usize,
    /// `disabled_formats[]` ≙ `prefs.ignore_image_formats`.
    disabled: [bool; DIC_MAX],
    /// Factories de décodeurs par format (`a_Jpeg_new`…).
    factories: [Option<DecoderFactory>; DIC_MAX],
    /// Sink `MSG`/`_MSG`.
    log: Box<dyn FnMut(&str)>,
}

thread_local! {
    static DICACHE: RefCell<Dicache> = RefCell::new(Dicache::new());
}

impl Dicache {
    fn new() -> Self {
        Self {
            entries: Vec::with_capacity(256),
            size_total: 0,
            disabled: [false; DIC_MAX],
            factories: [None, None, None, None, None],
            log: Box::new(|_| {}),
        }
    }

    /// `Dicache_entry_cmp` — `(url_cmp, version)` ; la sonde
    /// `version == DIC_LAST` ne matche que l'entrée `DIF_Last`.
    fn entry_cmp(a: &DicacheEntry, b_url: &DilloUrl, b_version: i32) -> i32 {
        let st = a.url.url_cmp(b_url);
        if st != 0 {
            return st;
        }
        if b_version == DIC_LAST {
            if a.flags & DIF_LAST != 0 {
                0
            } else {
                -1
            }
        } else {
            a.version - b_version
        }
    }

    /// `dList_find_sorted` — position de `(url, version)` ou point
    /// d'insertion pour la recherche.
    fn find_pos(&self, url: &DilloUrl, version: i32) -> Option<usize> {
        // Recherche linéaire fidèle : la sonde `DIC_Last` n'est pas un
        // ordre total (amont utilise `dList_find_sorted` — même résultat).
        self.entries
            .iter()
            .position(|e| Self::entry_cmp(&e.borrow(), url, version) == 0)
    }

    /// Point d'insertion trié `(url_cmp, version)` (versions concrètes).
    fn insert_pos(&self, url: &DilloUrl, version: i32) -> usize {
        self.entries
            .iter()
            .position(|e| {
                let c = e.borrow().url.url_cmp(url);
                c > 0 || (c == 0 && e.borrow().version > version)
            })
            .unwrap_or(self.entries.len())
    }
}

/// `Dicache_entry_new` — valeurs initiales amont.
fn entry_new(url: &DilloUrl) -> DicacheEntry {
    DicacheEntry {
        url: url.clone(),
        img_type: ImgType::NotSet,
        width: 0,
        height: 0,
        flags: DIF_VALID,
        surv_cleanup: 0,
        cmap: None,
        imgbuf: None,
        total_size: 0,
        scan_number: 0,
        bitvec: None,
        state: DicEntryState::Empty,
        ref_count: 1,
        version: 1,
        decoded_size: 0,
        decoder: None,
    }
}

/// `a_Dicache_init` — `CachedIMGs` + `disabled_formats` depuis
/// `prefs.ignore_image_formats` (`dStriAsciiStr` substring ci).
pub fn dicache_init(ignore_image_formats: Option<&[u8]>) {
    DICACHE.with(|d| {
        let mut d = d.borrow_mut();
        d.entries.clear();
        d.size_total = 0;
        if let Some(ig) = ignore_image_formats {
            for (i, name) in FORMAT_NAME.iter().enumerate() {
                if dlib::stri_ascii_str(ig, name.as_bytes()).is_some() {
                    d.disabled[i] = true;
                }
            }
        }
    });
}

/// Injecte les factories de décodeurs (`a_Png_new`…, phase décodeurs).
pub fn dicache_set_factories(factories: [(usize, DecoderFactory); 5]) {
    DICACHE.with(|d| {
        let mut d = d.borrow_mut();
        for (idx, f) in factories {
            if idx < DIC_MAX {
                d.factories[idx] = Some(f);
            }
        }
    });
}

/// Variante partielle : une seule factory (tests).
pub fn dicache_set_factory(idx: usize, f: DecoderFactory) {
    if idx < DIC_MAX {
        DICACHE.with(|d| d.borrow_mut().factories[idx] = Some(f));
    }
}

/// Injecte le sink de logs `MSG`/`_MSG`.
pub fn dicache_set_log(log: Box<dyn FnMut(&str)>) {
    DICACHE.with(|d| d.borrow_mut().log = log);
}

/// `a_Dicache_get_entry` — sonde `(url, version)` ; `version == 0` →
/// `None` ; `DIC_Last` → uniquement l'entrée `DIF_Last` **valide**.
#[must_use]
pub fn dicache_get_entry(url: &DilloUrl, version: i32) -> Option<Rc<RefCell<DicacheEntry>>> {
    if version == 0 {
        return None;
    }
    DICACHE.with(|d| {
        let d = d.borrow();
        d.find_pos(url, version).and_then(|p| {
            let e = d.entries[p].clone();
            // `!(Flags & DIF_Valid) && version == DIC_Last → NULL`.
            let eb = e.borrow();
            if eb.flags & DIF_VALID == 0 && version == DIC_LAST {
                None
            } else {
                drop(eb);
                Some(e)
            }
        })
    })
}

/// `Dicache_add_entry` — nouvelle version si l'URL existe déjà
/// (`DIF_Last` transféré, `version = last+1`), insertion triée.
fn dicache_add_entry(url: &DilloUrl) -> Rc<RefCell<DicacheEntry>> {
    DICACHE.with(|d| {
        let mut d = d.borrow_mut();
        let mut entry = entry_new(url);
        if let Some(p) = d.find_pos(url, DIC_LAST) {
            let last = d.entries[p].clone();
            let mut lb = last.borrow_mut();
            lb.flags &= !DIF_LAST;
            entry.version = lb.version + 1;
        }
        entry.flags |= DIF_LAST;
        let pos = d.insert_pos(url, entry.version);
        let rc = Rc::new(RefCell::new(entry));
        d.entries.insert(pos, rc.clone());
        rc
    })
}

/// `Dicache_remove` — retire l'entrée : `size_total` ajusté, imgbuf
/// unref (drop `Rc`), `Decoder(CA_Abort, DecoderData)` si décodeur.
fn dicache_remove(url: &DilloUrl, version: i32) {
    let (entry, decoder) = DICACHE.with(|d| {
        let mut d = d.borrow_mut();
        let Some(p) = d.find_pos(url, version) else {
            return (None, None);
        };
        let e = d.entries.remove(p);
        let mut eb = e.borrow_mut();
        d.size_total -= eb.total_size;
        // `a_Imgbuf_unref` ≙ drop du `Rc` — le widget peut survivre.
        eb.imgbuf = None;
        (Some(e.clone()), eb.decoder.take())
    });
    if let Some(e) = entry {
        let _ = e;
        // `entry->Decoder(CA_Abort, entry->DecoderData)` amont — hors
        // emprunts (le décodeur peut réentrer dans le dicache).
        if let Some(mut dec) = decoder {
            dec.abort();
        }
    }
}

/// `Dicache_is_last_ref` — `a_Imgbuf_last_reference` ≙ `strong_count`.
fn is_last_ref(entry: &DicacheEntry) -> i32 {
    entry
        .imgbuf
        .as_ref()
        .map_or(0, |b| i32::from(Imgbuf::last_reference(b)))
}

/// `a_Dicache_unref` — un client de moins ; plus de client ni imgbuf
/// → retrait immédiat (sinon `dicache_cleanup` plus tard).
pub fn dicache_unref(url: &DilloUrl, version: i32) {
    if let Some(e) = dicache_get_entry(url, version) {
        let (remove, ver) = {
            let mut eb = e.borrow_mut();
            if eb.ref_count > 0 {
                eb.ref_count -= 1;
            }
            (eb.ref_count == 0 && eb.imgbuf.is_none(), eb.version)
        };
        if remove {
            dicache_remove(url, ver);
        }
    }
}

/// `a_Dicache_ref` — un client de plus.
#[must_use]
pub fn dicache_ref(url: &DilloUrl, version: i32) -> Option<Rc<RefCell<DicacheEntry>>> {
    dicache_get_entry(url, version).inspect(|e| e.borrow_mut().ref_count += 1)
}

/// `a_Dicache_invalidate_entry` — reload : `DIC_Last` renvoie `None`
/// ensuite (les versions courantes restent vivantes).
pub fn dicache_invalidate_entry(url: &DilloUrl) {
    if let Some(e) = dicache_get_entry(url, DIC_LAST) {
        e.borrow_mut().flags &= !DIF_VALID;
    }
}

/* ------------------------------------------------------------------ */

/// `a_Dicache_set_parms` — dimensions + type depuis les données ;
/// `dReturn_if_fail` amont → retours muets (`Image` non-NULL, w/h > 0,
/// entrée trouvée, `State < SetParms`).
pub fn dicache_set_parms(
    url: &DilloUrl,
    version: i32,
    image: &Rc<RefCell<DilloImage>>,
    width: u32,
    height: u32,
    img_type: ImgType,
    gamma: f64,
) {
    if width == 0 || height == 0 {
        return;
    }
    let Some(entry) = dicache_get_entry(url, version) else {
        return;
    };
    let mut eb = entry.borrow_mut();
    if eb.state >= DicEntryState::SetParms {
        return;
    }
    // `Image->layout` amont n'est pas nécessaire — notre Imgbuf est
    // auto-suffisant ; `layout` reste un handle opaque.
    let _ = image.borrow().layout;
    eb.imgbuf = Some(Imgbuf::new(ImgbufType::Rgb, width, height, gamma));
    eb.total_size = (3 * width * height) as usize;
    eb.width = width;
    eb.height = height;
    eb.img_type = img_type;
    eb.bitvec = Some(Bitvec::new(height as usize));
    eb.state = DicEntryState::SetParms;
    let total = eb.total_size;
    drop(eb);
    DICACHE.with(|d| d.borrow_mut().size_total += total);
}

/// `a_Dicache_set_cmap` — palette (`3*num_colors_max` zéroé, copie
/// `3*num_colors`, `bg_index` dans `[0, num_colors)` → couleur de fond).
pub fn dicache_set_cmap(
    url: &DilloUrl,
    version: i32,
    bg_color: i32,
    cmap: &[u8],
    num_colors: u32,
    num_colors_max: i32,
    bg_index: i32,
) {
    let Some(entry) = dicache_get_entry(url, version) else {
        return;
    };
    let mut eb = entry.borrow_mut();
    // `dFree(cmap); cmap = dNew0(uchar_t, 3*num_colors_max)` — amont
    // lit `3*num_colors` même si `cmap` court (confiance décodeur).
    let max = usize::try_from(num_colors_max).unwrap_or(0);
    let mut new_cmap = vec![0u8; 3 * max];
    let n = (3 * num_colors as usize)
        .min(new_cmap.len())
        .min(cmap.len());
    new_cmap[..n].copy_from_slice(&cmap[..n]);
    if u32::try_from(bg_index).is_ok_and(|b| b < num_colors) {
        let i = 3 * usize::try_from(bg_index).unwrap_or(0);
        if i + 2 < new_cmap.len() {
            new_cmap[i] = u8::try_from((bg_color >> 16) & 0xff).unwrap_or(0);
            new_cmap[i + 1] = u8::try_from((bg_color >> 8) & 0xff).unwrap_or(0);
            new_cmap[i + 2] = u8::try_from(bg_color & 0xff).unwrap_or(0);
        }
    }
    eb.cmap = Some(new_cmap);
    eb.state = DicEntryState::SetCmap;
}

/// `a_Dicache_new_scan` — reset scan (multi-scan/interlacé) ;
/// `State < SetParms` → MSG+exit amont (ici log + muet).
pub fn dicache_new_scan(url: &DilloUrl, version: i32) {
    let Some(entry) = dicache_get_entry(url, version) else {
        return;
    };
    let imgbuf = {
        let mut eb = entry.borrow_mut();
        if eb.state < DicEntryState::SetParms {
            let msg = "a_Dicache_new_scan before DIC_SetParms".to_string();
            drop(eb);
            DICACHE.with(|d| (d.borrow_mut().log)(&msg));
            return;
        }
        if let Some(bv) = &mut eb.bitvec {
            bv.clear();
        }
        eb.scan_number += 1;
        eb.imgbuf.clone()
    };
    if let Some(b) = imgbuf {
        b.new_scan();
    }
}

/// `a_Dicache_write` — `a_Imgbuf_update` (conversion RGB + `copy_row`)
/// + bit de ligne + `State = Write`.
pub fn dicache_write(url: &DilloUrl, version: i32, buf: &[u8], y: u32) {
    let Some(entry) = dicache_get_entry(url, version) else {
        return;
    };
    let (imgbuf, line) = {
        let eb = entry.borrow();
        if eb.width == 0 || eb.height == 0 {
            return;
        }
        let line = rgb_line(buf, eb.img_type, eb.cmap.as_deref(), eb.width);
        (eb.imgbuf.clone(), line)
    };
    if let Some(b) = &imgbuf {
        b.copy_row(y, &line);
    }
    let mut eb = entry.borrow_mut();
    if let Some(bv) = &mut eb.bitvec {
        bv.set_bit(y as usize);
    }
    eb.state = DicEntryState::Write;
}

/// `a_Dicache_close` — `State→Close`, cmap libérée, décodeur détaché
/// (amont le `DecoderData` n'est **pas** libéré ici — le décodeur se
/// libère lui-même dans `Xxx_close` ; le drop `Box` est l'équivalent
/// net), `unref` + `a_Bw_close_client`.
pub fn dicache_close(url: &DilloUrl, version: i32, client: &CacheClient) {
    let Some(entry) = dicache_get_entry(url, version) else {
        return;
    };
    let (bw, key, ver) = {
        let mut eb = entry.borrow_mut();
        if eb.state < DicEntryState::Close {
            eb.state = DicEntryState::Close;
            eb.cmap = None;
            eb.decoder = None;
        }
        (client.web.bw, client.key, eb.version)
    };
    dicache_unref(url, ver);
    cache::cache_close_client(bw, key);
}

/* ------------------------------------------------------------------ */

/// Seam `a_Image_new_with_dw` — création du widget image
/// (`new dw::Image(alt_text, mark_unloaded)` amont + `layout->dpiX()`).
pub trait ImageFactory {
    /// Crée le renderer/widget pour `web` (`web.bw`, `web.bg_color` et
    /// `prefs` résolus côté navigateur). Retourne le renderer et la
    /// `dpi` du layout.
    fn new_image(&mut self, web: &HttpWeb) -> (Rc<RefCell<dyn ImgRenderer>>, f32);
}

thread_local! {
    static IMAGE_FACTORY: RefCell<Option<Box<dyn ImageFactory>>> = const { RefCell::new(None) };
}

/// Injecte la factory de widgets image (phase dw/UI).
pub fn dicache_set_image_factory(f: Box<dyn ImageFactory>) {
    IMAGE_FACTORY.with(|fac| *fac.borrow_mut() = Some(f));
}

/// `Dicache_image` — handler MIME générique : crée `web->Image` au
/// besoin (`a_Image_new_with_dw`), entrée dicache créée+decodeur
/// attaché ou `ref` si répétée, `SurvCleanup = 3`, retourne
/// (`a_Dicache_callback`, `a_Image_get_dw`) ≙ `*Call`/`*Data`+retour
/// amont. `None` = format désactivé / pas de factory.
#[must_use]
pub fn dicache_image(
    fmt: usize,
    _mime: &str,
    web: &Rc<HttpWeb>,
) -> Option<(CaCb, Rc<RefCell<dyn ImgRenderer>>)> {
    if fmt < DIC_MAX && DICACHE.with(|d| d.borrow().disabled[fmt]) {
        return None;
    }

    // `if (!web->Image) web->Image = a_Image_new_with_dw(...)` + ref.
    if web.image.borrow().is_none() {
        let (rndr, dpi) =
            IMAGE_FACTORY.with(|f| f.borrow_mut().as_mut().map(|f| f.new_image(web)))?;
        let img = image_new(0, rndr, web.bg_color, 0, dpi);
        let _ = image_ref(&img);
        *web.image.borrow_mut() = Some(img as Rc<dyn std::any::Any>);
    }
    let image: Rc<RefCell<DilloImage>> = web
        .image
        .borrow()
        .as_ref()
        .and_then(|a| a.clone().downcast::<RefCell<DilloImage>>().ok())?;

    let entry = match dicache_get_entry(&web.url, DIC_LAST) {
        None => {
            // Nouvelle entrée + attache décodeur.
            let e = dicache_add_entry(&web.url);
            let version = e.borrow().version;
            let dec = DICACHE
                .with(|d| d.borrow().factories[fmt])
                .map(|f| f(image.clone(), e.borrow().url.clone(), version));
            e.borrow_mut().decoder = dec;
            e
        }
        Some(e) => {
            // Image répétée — extraire avant `ref` (emprunts).
            let (url, version) = {
                let eb = e.borrow();
                (eb.url.clone(), eb.version)
            };
            let _ = dicache_ref(&url, version);
            e
        }
    };
    entry.borrow_mut().surv_cleanup = 3;

    Some((Box::new(dicache_callback), image_get_dw(&image)))
}

/// `a_Dicache_callback` — client cache alimentant le décodeur puis
/// l'image (`Client->Version`, `DecodedSize < BufSize`, propagation
/// `BitVec`/scan vers `DilloImage`, close/abort → `a_Bw_close_client`).
pub fn dicache_callback(op: i32, client: &mut CacheClient) {
    let web = client.web.clone();
    let Some(entry) = dicache_get_entry(&web.url, DIC_LAST) else {
        return;
    };

    // `Client->Version == 0 → Version = DicEntry->version`.
    if client.version == 0 {
        client.version = entry.borrow().version;
    }

    // `Decoder(Op, Client)` aux bons moments — take/appel/restore hors
    // emprunt : le décodeur rappelle `dicache_*` (réentrance `RefCell`).
    {
        let mut eb = entry.borrow_mut();
        if op == CA_SEND
            && eb.state < DicEntryState::Close
            && eb.decoded_size < client.buf.len()
        {
            let mut dec = eb.decoder.take();
            drop(eb);
            if let Some(d) = &mut dec {
                d.callback(op, client);
            }
            let mut eb2 = entry.borrow_mut();
            eb2.decoded_size = client.buf.len();
            if eb2.decoder.is_none() {
                eb2.decoder = dec;
            }
        } else if op == CA_CLOSE || op == CA_ABORT {
            if eb.state < DicEntryState::Close {
                let mut dec = eb.decoder.take();
                drop(eb);
                if let Some(d) = &mut dec {
                    d.callback(op, client);
                }
                let mut eb2 = entry.borrow_mut();
                if eb2.decoder.is_none() {
                    eb2.decoder = dec;
                }
            } else {
                let url = eb.url.clone();
                let ver = eb.version;
                drop(eb);
                dicache_close(&url, ver, client);
            }
        }
    }

    // Propagation vers `DilloImage` (`web->Image`) — seulement si le
    // flux est une vraie image (`v_imgbuf` reste NULL sinon).
    let imgbuf_present = op == CA_SEND && entry.borrow().imgbuf.is_some();
    let image: Option<Rc<RefCell<DilloImage>>> = web
        .image
        .borrow()
        .as_ref()
        .and_then(|a| a.clone().downcast::<RefCell<DilloImage>>().ok());

    if imgbuf_present {
        propagate_to_image(&entry, image.as_ref());
    } else if let Some(i) = &image {
        if op == CA_CLOSE {
            image_close(i);
            cache::cache_close_client(web.bw, client.key);
        } else if op == CA_ABORT {
            image_abort(i);
            cache::cache_close_client(web.bw, client.key);
        }
    }
}

/// Branche `Op == CA_Send && v_imgbuf` de `a_Dicache_callback` —
/// `set_parms` si `Image->height == 0`, puis copie des lignes selon
/// `BitVec`/`ScanNumber` (multi-scan : lignes obsolètes nettoyées).
fn propagate_to_image(
    entry: &Rc<RefCell<DicacheEntry>>,
    image: Option<&Rc<RefCell<DilloImage>>>,
) {
    let eb = entry.borrow();
    let imb = image.map(|i| i.borrow());
    let parms = if imb.as_ref().is_some_and(|i| i.height == 0)
        && eb.state >= DicEntryState::SetParms
    {
        eb.imgbuf.clone().map(|b| (b, eb.width, eb.height))
    } else {
        None
    };
    let mut writes = Vec::new();
    let mut clear_rows = false;
    let mut new_scan_num = 0u32;
    if eb.state == DicEntryState::Write {
        if let Some(i) = &imb {
            let ebit = |y: u32| {
                eb.bitvec
                    .as_ref()
                    .is_some_and(|b| b.get_bit(y as usize) != 0)
            };
            let ibit = |y: u32| {
                i.bitvec
                    .as_ref()
                    .is_some_and(|b| b.get_bit(y as usize) != 0)
            };
            if eb.scan_number == i.scan_number {
                for y in 0..eb.height {
                    if ebit(y) && !ibit(y) {
                        writes.push(y);
                    }
                }
            } else {
                for y in 0..eb.height {
                    if ebit(y) || !ibit(y) || eb.scan_number > i.scan_number + 1 {
                        writes.push(y);
                    }
                }
                clear_rows = true;
                new_scan_num = eb.scan_number;
            }
        }
    }
    drop(imb);
    drop(eb);

    if let (Some((buf, w, h)), Some(i)) = (parms, image) {
        image_set_parms(i, buf, w, h);
    }
    if let Some(i) = image {
        for y in writes {
            image_write(i, y);
        }
        if clear_rows {
            // `if (!ebit) a_Bitvec_clear_bit(Image->BitVec, i)` amont.
            let mut ib = i.borrow_mut();
            for y in 0..ib.height {
                let ebit = entry
                    .borrow()
                    .bitvec
                    .as_ref()
                    .is_some_and(|b| b.get_bit(y as usize) != 0);
                if !ebit {
                    if let Some(bv) = &mut ib.bitvec {
                        bv.clear_bit(y as usize);
                    }
                }
            }
            ib.scan_number = new_scan_num;
        }
    }
}

/* ------------------------------------------------------------------ */

/// `a_Dicache_cleanup` — libère les imgbufs des entrées inutilisées
/// (`RefCount==0` && (`!imgbuf` || `last_ref`)) après `SurvCleanup`
/// passes.
pub fn dicache_cleanup() {
    let mut i = 0usize;
    loop {
        let (entry, cont) = DICACHE.with(|d| {
            let d = d.borrow();
            d.entries.get(i).cloned().map_or((None, false), |e| {
                let eb = e.borrow();
                let stale =
                    eb.ref_count == 0 && eb.imgbuf.as_ref().is_none_or(Imgbuf::last_reference);
                (Some(e.clone()), stale)
            })
        });
        let Some(e) = entry else { break };
        if cont {
            let (surv, url, ver) = {
                let mut eb = e.borrow_mut();
                eb.surv_cleanup -= 1;
                (eb.surv_cleanup >= 0, eb.url.clone(), eb.version)
            };
            if surv {
                i += 1;
                continue;
            }
            dicache_remove(&url, ver);
            // `--i` amont : ne pas incrémenter (l'élément i est nouveau).
            continue;
        }
        i += 1;
    }
}

/// `a_Dicache_freeall` — à la sortie, sans clients en file.
pub fn dicache_freeall() {
    DICACHE.with(|d| {
        let mut d = d.borrow_mut();
        let mut total = 0usize;
        for e in d.entries.drain(..) {
            let mut eb = e.borrow_mut();
            eb.imgbuf = None;
            total += eb.total_size;
        }
        d.size_total = d.size_total.saturating_sub(total);
    });
}

/// `a_Dicache_stats` — page `about:dicache` (HTML verbatim).
#[must_use]
#[allow(clippy::cast_precision_loss)] // page de stats : f64 suffit.
pub fn dicache_stats() -> String {
    use std::fmt::Write as _;
    let mut html = String::from(
        "<!DOCTYPE HTML>\n<html>\n\
         <head><title>Decompressed Image Cache</title></head>\n<body>\n",
    );
    let mut bytes_cached = 0usize;
    DICACHE.with(|dc| {
        let dc = dc.borrow();
        let count = dc.entries.len();
        let _ = writeln!(html, "<h1>Decompressed Image Cache ({count})</h1>");
        html.push_str(
            "<table>\n<tr>\n\
            <th><span title='Survival Counter'>S</span></th>\n\
            <th><span title='Last Reference'>L</span></th>\n\
            <th>Area</th>\n<th>Size</th>\n<th>URL</th>\n</tr>\n",
        );
        for e in &dc.entries {
            let eb = e.borrow();
            html.push_str("<tr>\n");
            let _ = write!(
                html,
                "<td style='text-align:right'>{}</td>\
                 <td style='text-align:right'>{}</td>\
                 <td style='text-align:right'>{}x{}</td>",
                eb.surv_cleanup,
                is_last_ref(&eb),
                eb.width,
                eb.height
            );
            let kib = eb.total_size as f64 / 1024.0;
            let _ = writeln!(html, "<td style='text-align:right'>{kib:.2} KiB</td>");
            let mut entry_url = eb.url.clone();
            let url_str = String::from_utf8_lossy(entry_url.url_str()).into_owned();
            let _ = write!(html, "<td><a href='{url_str}'>");
            // `dStr_shorten(s, url, 60)` amont : milieu raccourci
            // (m=57, n1=28, n2=29) sur bytes — URL est ASCII de fait.
            if url_str.len() > 60 {
                let bytes = url_str.as_bytes();
                html.push_str(&String::from_utf8_lossy(&bytes[..28]));
                html.push_str("...");
                html.push_str(&String::from_utf8_lossy(&bytes[bytes.len() - 29..]));
            } else {
                html.push_str(&url_str);
            }
            html.push_str("</a></td>\n</tr>\n");
            bytes_cached += eb.total_size;
        }
    });
    html.push_str("</table>\n");
    let mib = bytes_cached as f64 / (1024.0 * 1024.0);
    let _ = writeln!(html, "<p>Total cached: {mib:.2} MiB</p>");
    html.push_str("</body>\n</html>\n");
    html
}

/// Impl réelle de la seam [`cache::DicacheOps`] (injectée côté
/// navigateur — voit le dicache complet).
#[derive(Default)]
pub struct DicacheOpsImpl;
impl cache::DicacheOps for DicacheOpsImpl {
    fn invalidate_entry(&mut self, url: &DilloUrl) {
        dicache_invalidate_entry(url);
    }
    fn get_entry(&mut self, url: &DilloUrl, version: i32) -> bool {
        dicache_get_entry(url, version).is_some()
    }
    fn unref(&mut self, url: &DilloUrl, version: i32) {
        dicache_unref(url, version);
    }
    fn cleanup(&mut self) {
        dicache_cleanup();
    }
    fn stats(&mut self) -> Option<String> {
        Some(dicache_stats())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dillo_net::http::HttpWeb;

    fn u(s: &str) -> DilloUrl {
        dillo_net::url::url_new(s.as_bytes(), None).unwrap()
    }

    /// Reset du registre global entre tests.
    fn reset() {
        dicache_freeall();
        cache::cache_init();
    }

    /// Décodeur factice : enregistre les ops reçus.
    struct SpyDec(Rc<RefCell<Vec<i32>>>);
    impl ImageDecoder for SpyDec {
        fn callback(&mut self, op: i32, _c: &mut CacheClient) {
            self.0.borrow_mut().push(op);
        }
        fn abort(&mut self) {
            self.0.borrow_mut().push(99);
        }
    }

    /// Fabrique un `CacheClient` minimal pointant `web`.
    fn client(key: i32, web: &Rc<HttpWeb>, buf: &[u8]) -> CacheClient {
        CacheClient {
            key,
            url: web.url.clone(),
            version: 0,
            buf: buf.to_vec(),
            callback: None,
            web: web.clone(),
        }
    }

    /// Factory qui installe un `SpyDec` enregistrant dans `ops`.
    fn spy_factory(ops: Rc<RefCell<Vec<i32>>>) -> DecoderFactory {
        fn mk(image: Rc<RefCell<DilloImage>>, url: DilloUrl, v: i32) -> Box<dyn ImageDecoder> {
            let _ = (image, url, v);
            Box::new(SpyDec(OPS.with(|o| o.borrow().clone())))
        }
        thread_local! {
            static OPS: RefCell<Rc<RefCell<Vec<i32>>>> =
                RefCell::new(Rc::new(RefCell::new(Vec::new())));
        }
        OPS.with(|o| *o.borrow_mut() = ops);
        mk
    }

    struct FakeFactory;
    impl ImageFactory for FakeFactory {
        fn new_image(&mut self, _web: &HttpWeb) -> (Rc<RefCell<dyn ImgRenderer>>, f32) {
            let r = Rc::new(RefCell::new(crate::image::NullRenderer::default()));
            (r, 96.0)
        }
    }

    #[test]
    fn add_entry_versions_and_last_flag() {
        reset();
        let url = u("http://a/b.png");
        let e1 = dicache_add_entry(&url);
        assert_eq!(e1.borrow().version, 1);
        assert!(e1.borrow().flags & DIF_LAST != 0);
        let e2 = dicache_add_entry(&url);
        assert_eq!(e2.borrow().version, 2);
        assert!(e2.borrow().flags & DIF_LAST != 0);
        assert_eq!(e1.borrow().flags & DIF_LAST, 0);
        // DIC_Last résout vers la version courante.
        let got = dicache_get_entry(&url, DIC_LAST).unwrap();
        assert_eq!(got.borrow().version, 2);
        // Version explicite atteint l'ancienne.
        let got1 = dicache_get_entry(&url, 1).unwrap();
        assert_eq!(got1.borrow().version, 1);
    }

    #[test]
    fn get_entry_zero_and_invalid() {
        reset();
        assert!(dicache_get_entry(&u("http://x/y.png"), 0).is_none());
        let url = u("http://x/y.png");
        dicache_add_entry(&url);
        dicache_invalidate_entry(&url);
        // Invalide → DIC_Last retourne None, la version explicite reste.
        assert!(dicache_get_entry(&url, DIC_LAST).is_none());
        assert!(dicache_get_entry(&url, 1).is_some());
    }

    #[test]
    fn refcount_and_cleanup_survival() {
        reset();
        let url = u("http://c/d.png");
        dicache_add_entry(&url);
        dicache_unref(&url, 1); // RefCount 0, pas d'imgbuf → retrait
        assert!(dicache_get_entry(&url, 1).is_none());
    }

    #[test]
    fn cleanup_survives_passes() {
        reset();
        let url = u("http://c/e.png");
        let e = dicache_add_entry(&url);
        dicache_set_parms(
            &url,
            1,
            &image_new(
                0,
                Rc::new(RefCell::new(crate::image::NullRenderer::default())),
                0,
                0,
                96.0,
            ),
            4,
            4,
            ImgType::Rgb,
            1.0,
        );
        // Plus de réf client, imgbuf présent → last_reference? Le
        // DilloImage partagé n'a pas pris le buf → Rc = 1 → last_ref.
        e.borrow_mut().ref_count = 0;
        e.borrow_mut().surv_cleanup = 1;
        dicache_cleanup(); // passe 1 → survit (surv_cleanup 0)
        assert!(dicache_get_entry(&url, 1).is_some());
        dicache_cleanup(); // passe 2 → retiré
        assert!(dicache_get_entry(&url, 1).is_none());
    }

    #[test]
    fn set_cmap_bg_index() {
        reset();
        let url = u("http://c/f.png");
        dicache_add_entry(&url);
        dicache_set_cmap(&url, 1, 0x0011_2233, &[9, 9, 9, 8, 8, 8], 2, 4, 1);
        let e = dicache_get_entry(&url, 1).unwrap();
        let eb = e.borrow();
        let cm = eb.cmap.as_ref().unwrap();
        // 4 couleurs allouées, 2 copiées, index 1 remplacé par bg.
        assert_eq!(cm.len(), 12);
        assert_eq!(&cm[0..3], &[9, 9, 9]);
        assert_eq!(&cm[3..6], &[0x11, 0x22, 0x33]);
        assert_eq!(&cm[6..12], &[0; 6]);
        assert_eq!(eb.state, DicEntryState::SetCmap);
    }

    #[test]
    fn write_marks_bits_and_rgb() {
        reset();
        let url = u("http://c/g.png");
        let img = image_new(
            0,
            Rc::new(RefCell::new(crate::image::NullRenderer::default())),
            0,
            0,
            96.0,
        );
        dicache_add_entry(&url);
        dicache_set_parms(&url, 1, &img, 2, 2, ImgType::Rgb, 1.0);
        dicache_write(&url, 1, &[1, 2, 3, 4, 5, 6], 1);
        let e = dicache_get_entry(&url, 1).unwrap();
        let eb = e.borrow();
        assert_eq!(eb.state, DicEntryState::Write);
        assert_eq!(eb.bitvec.as_ref().unwrap().get_bit(1), 1);
        assert_eq!(eb.bitvec.as_ref().unwrap().get_bit(0), 0);
        assert_eq!(
            eb.imgbuf.as_ref().unwrap().row_copy(1),
            vec![1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn new_scan_resets_bits() {
        reset();
        let url = u("http://c/h.png");
        let img = image_new(
            0,
            Rc::new(RefCell::new(crate::image::NullRenderer::default())),
            0,
            0,
            96.0,
        );
        dicache_add_entry(&url);
        dicache_set_parms(&url, 1, &img, 2, 2, ImgType::Rgb, 1.0);
        dicache_write(&url, 1, &[0; 6], 0);
        dicache_new_scan(&url, 1);
        let e = dicache_get_entry(&url, 1).unwrap();
        let eb = e.borrow();
        assert_eq!(eb.scan_number, 1);
        assert_eq!(eb.bitvec.as_ref().unwrap().get_bit(0), 0);
        // Avant set_parms → muet (MSG amont).
        let url2 = u("http://c/i.png");
        dicache_add_entry(&url2);
        dicache_new_scan(&url2, 1); // no-op sans panic
    }

    #[test]
    fn callback_feeds_decoder_then_close() {
        reset();
        let url = u("http://d/j.png");
        let web = Rc::new(HttpWeb::new(url.clone(), None, 0));
        // Image fabriquée par dicache_image (factory injectée).
        dicache_set_image_factory(Box::new(FakeFactory));
        let ops = Rc::new(RefCell::new(Vec::new()));
        dicache_set_factory(DIC_PNG, spy_factory(ops.clone()));
        let (cb, _rndr) = dicache_image(DIC_PNG, "image/png", &web).unwrap();
        let _ = cb;
        // Le décodeur a été attaché.
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        assert!(e.borrow().decoder.is_some());
        assert_eq!(e.borrow().surv_cleanup, 3);

        // CA_Send : décodeur alimenté + DecodedSize à jour.
        let mut cl = client(7, &web, &[1, 2, 3]);
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(cl.version, 1);
        assert_eq!(e.borrow().decoded_size, 3);
        assert_eq!(ops.borrow().as_slice(), &[CA_SEND]);
        // CA_Send sans nouvelles données → pas d'appel décodeur.
        dicache_callback(CA_SEND, &mut cl);
        assert_eq!(ops.borrow().len(), 1);
        // CA_Close → décodeur close + image_close + close_client.
        dicache_callback(CA_CLOSE, &mut cl);
        assert!(ops.borrow().contains(&CA_CLOSE));
    }

    #[test]
    fn callback_abort_path() {
        reset();
        let url = u("http://d/k.png");
        let web = Rc::new(HttpWeb::new(url.clone(), None, 0));
        dicache_set_image_factory(Box::new(FakeFactory));
        let ops = Rc::new(RefCell::new(Vec::new()));
        dicache_set_factory(DIC_PNG, spy_factory(ops.clone()));
        let _ = dicache_image(DIC_PNG, "image/png", &web);
        let mut cl = client(9, &web, &[1]);
        dicache_callback(CA_ABORT, &mut cl);
        assert_eq!(ops.borrow().as_slice(), &[CA_ABORT]);
        let img = web.image.borrow().as_ref().unwrap().clone();
        let img: Rc<RefCell<DilloImage>> = img.downcast().unwrap();
        assert_eq!(img.borrow().state, crate::image::ImageState::Abort);
    }

    #[test]
    fn repeated_image_refcounts_entry() {
        reset();
        let url = u("http://d/l.png");
        let web = Rc::new(HttpWeb::new(url.clone(), None, 0));
        dicache_set_image_factory(Box::new(FakeFactory));
        let _ = dicache_image(DIC_PNG, "image/png", &web);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        assert_eq!(e.borrow().ref_count, 1);
        let _ = dicache_image(DIC_PNG, "image/png", &web);
        assert_eq!(e.borrow().ref_count, 2);
    }

    #[test]
    fn stats_page_lists_entry() {
        reset();
        let url = u("http://stats/img.png");
        let e = dicache_add_entry(&url);
        e.borrow_mut().total_size = 4096;
        DICACHE.with(|d| d.borrow_mut().size_total += 4096);
        let html = dicache_stats();
        assert!(html.contains("Decompressed Image Cache (1)"));
        assert!(html.contains("http://stats/img.png"));
        assert!(html.contains("4.00 KiB"));
    }
}
