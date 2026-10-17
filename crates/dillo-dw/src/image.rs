//! Port de `dw/image.hh` + `dw/image.cc` — widget `Image` +
//! `ImageMapsList` (cartes imagemap client/serveur).
//!
//! `Image : Widget, ImgRenderer` amont ≙ `WidgetKind::Image` avec
//! [`ImageData`] et [`ImageImgRenderer`]. La partie `ImgRenderer`
//! tient `Weak<Arena>`+`WidgetId` — modèle D, même pattern que
//! `WordImgRenderer`.
//!
//! `ImageMapsList` : `HashTable<Object,ImageMap>` amont ≙
//! `Vec<(Rc<dyn ImageMapKey>, ImageMap)>` — la recherche est
//! linéaire sur `key_equals` (peu de cartes par page ; la
//! table de hachage amont n'est qu'une optimisation, non
//! observable). `mapKey` possédé amont ≙ `Rc` partagé ; la
//! clé est un trait dédié parce que `lout::Object::equals`
//! n'est pas object-safe (`Self: Sized`).
//!
//! `~Image` : `altText`/`buffer`/`altStyle`/`savedStyle`/
//! `mapKey` libérés — ≙ drops RAII.
//!
//! `setBuffer`/`drawRow` appellent `queueResize`/`queueDrawArea`
//! amont — hors `&mut Layout` le renderer enfile dans
//! `Arena::pending_resizes`/`pending_draws` (même discipline
//! différée que `WidgetImgRenderer`, drainée par
//! [`Layout::drain_idles`]/`flush_pending_draws`).

// Casts int/size_t verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::events::{EventButton, EventCrossing, EventMotion, MousePositionEvent};
use crate::imgbuf::Imgbuf;
use crate::imgrenderer::ImgRenderer;
use crate::layout::{Arena, Layout};
use crate::style::{Color, Style, StyleAttrs};
use crate::textblock::words::{null_style, unreachable_font};
use crate::types::{Allocation, DrawingContext, Extremes, Rectangle, Requisition, Shape};
use crate::view::View;
use crate::widget::{split_height_preserve_descent, WidgetKind};
use crate::WidgetId;

// ------------------------------------------------------------------
// Clé de carte ≙ `object::Object` amont (typiquement une URL).
// ------------------------------------------------------------------

/// `object::Object` des clés `ImageMapsList` amont — `equals` +
/// `hashValue` virtuels ≙ ce trait object-safe.
pub trait ImageMapKey: std::any::Any {
    /// `equals` — comparaison de clés (même type des deux côtés
    /// ≙ contrat amont).
    fn key_equals(&self, other: &Rc<dyn ImageMapKey>) -> bool;
    /// `hashValue` (inutilisé — recherche linéaire — gardé pour
    /// la parité d'interface).
    fn key_hash(&self) -> i32;
}

// ------------------------------------------------------------------
// `ImageMapsList` (image.cc:36-143)
// ------------------------------------------------------------------

/// `ImageMapsList::ImageMap::ShapeAndLink` ≙ struct.
struct ShapeAndLink {
    /// `shape`.
    shape: Rc<dyn Shape>,
    /// `link`.
    link: i32,
}

/// `ImageMapsList::ImageMap`.
#[derive(Default)]
struct ImageMap {
    /// `shapesAndLinks`.
    shapes_and_links: Vec<ShapeAndLink>,
    /// `defaultLink` (−1 initial).
    default_link: i32,
}

impl ImageMap {
    /// `ImageMap::draw` (image.cc:52-61).
    #[allow(clippy::trivially_copy_pass_by_ref)] // verbatim `Shape::draw`
    fn draw(&self, view: &mut dyn View, color: &Color, x: i32, y: i32) {
        for sl in &self.shapes_and_links {
            sl.shape.draw(view, color, x, y);
        }
    }

    /// `ImageMap::link` (image.cc:73-87) — premier shape
    /// contenant le point, sinon `defaultLink`.
    fn link(&self, x: i32, y: i32) -> i32 {
        let mut link = self.default_link;
        for sl in &self.shapes_and_links {
            if sl.shape.is_point_within(x, y) {
                link = sl.link;
                break;
            }
        }
        link
    }
}

/// `dw::ImageMapsList` — cartes imagemap d'une page (partagée
/// par plusieurs images : `Rc<RefCell>` côté appelant).
#[derive(Default)]
pub struct ImageMapsList {
    /// `imageMaps` ≙ `HashTable` (recherche linéaire `key_equals`).
    image_maps: Vec<(Rc<dyn ImageMapKey>, ImageMap)>,
    /// `currentMap` ≙ index dans `image_maps`.
    current_map: Option<usize>,
}

impl ImageMapsList {
    /// `ImageMapsList()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `startNewMap` (image.cc:113-117).
    pub fn start_new_map(&mut self, key: Rc<dyn ImageMapKey>) {
        self.image_maps.push((
            key,
            ImageMap {
                default_link: -1,
                ..ImageMap::default()
            },
        ));
        self.current_map = Some(self.image_maps.len() - 1);
    }

    /// `addShapeToCurrentMap` (image.cc:124-127).
    pub fn add_shape_to_current_map(&mut self, shape: Rc<dyn Shape>, link: i32) {
        if let Some(i) = self.current_map {
            if let Some(m) = self.image_maps.get_mut(i) {
                m.1.shapes_and_links.push(ShapeAndLink { shape, link });
            }
        }
    }

    /// `setCurrentMapDefaultLink` (image.cc:132-135).
    pub fn set_current_map_default_link(&mut self, link: i32) {
        if let Some(i) = self.current_map {
            if let Some(m) = self.image_maps.get_mut(i) {
                m.1.default_link = link;
            }
        }
    }

    /// ≙ `imageMaps->get (key)` — `key_equals`.
    fn get(&self, key: &Rc<dyn ImageMapKey>) -> Option<&ImageMap> {
        self.image_maps
            .iter()
            .find(|(k, _)| k.key_equals(key))
            .map(|(_, m)| m)
    }

    /// `drawMap` (image.cc:137-143) — `style` amont réduit à
    /// `style->color` (comme `Shape::draw`).
    pub fn draw_map(
        &self,
        key: &Rc<dyn ImageMapKey>,
        view: &mut dyn View,
        color: &Color,
        x: i32,
        y: i32,
    ) {
        if let Some(m) = self.get(key) {
            m.draw(view, color, x, y);
        }
    }

    /// `link` (image.cc:145-152).
    pub fn link(&self, key: &Rc<dyn ImageMapKey>, x: i32, y: i32) -> i32 {
        self.get(key).map_or(-1, |m| m.link(x, y))
    }
}

// ------------------------------------------------------------------
// `Image` (image.cc:146-591)
// ------------------------------------------------------------------

/// Champs `dw::Image` (image.hh:139-158).
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ImageData {
    /// `altText`.
    pub alt_text: Option<Vec<u8>>,
    /// `buffer` (≙ référencé amont — `Rc` ici).
    pub buffer: Option<Rc<dyn Imgbuf>>,
    /// `bufWidth` (−1 initialement).
    pub buf_width: i32,
    /// `bufHeight`.
    pub buf_height: i32,
    /// `altTextWidth` (−1 = non calculé).
    pub alt_text_width: i32,
    /// `clicking`.
    pub clicking: bool,
    /// `currLink` (−1).
    pub curr_link: i32,
    /// `mapList` — non possédé amont (l'appelant garde la liste
    /// en vie) ≙ `Rc` partagée (même durée de vie garantie).
    pub map_list: Option<Rc<RefCell<ImageMapsList>>>,
    /// `mapKey` — possédé amont ≙ `Rc` partagé.
    pub map_key: Option<Rc<dyn ImageMapKey>>,
    /// `isMap` (carte serveur).
    pub is_map: bool,
    /// `markEmpty` (image sans données → bordure alt).
    pub mark_empty: bool,
    /// `useAltStyle`.
    pub use_alt_style: bool,
    /// `altStyle`.
    pub alt_style: Option<Rc<Style>>,
    /// `savedStyle`.
    pub saved_style: Option<Rc<Style>>,
}

impl std::fmt::Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageData")
            .field("alt_text", &self.alt_text)
            .field("buf", &(self.buf_width, self.buf_height))
            .field("curr_link", &self.curr_link)
            .field("is_map", &self.is_map)
            .finish_non_exhaustive()
    }
}

impl ImageData {
    /// `Image (altText, markEmpty)` (image.cc:154-172).
    fn new(alt_text: Option<Vec<u8>>, mark_empty: bool) -> Self {
        ImageData {
            alt_text,
            buf_width: -1,
            buf_height: -1,
            alt_text_width: -1,
            curr_link: -1,
            mark_empty,
            ..ImageData::default()
        }
    }
}

/// `Image` vu comme `ImgRenderer` (image.hh:161-176) — le
/// dicache/html pousse `setBuffer`/`drawRow`/`finish`/`fatal`
/// via ce renderer ≙ `Weak<Arena>`+`WidgetId` (modèle D : pas
/// d'accès `&mut Layout` — resize/draw différés via
/// `Arena::pending_resizes`/`pending_draws`, drainés par
/// `drain_idles`/`flush_pending_draws`).
pub struct ImageImgRenderer {
    /// Arène faible (≙ `this`).
    arena: Weak<RefCell<Arena>>,
    /// `Layout::id` émetteur (tag des files différées — arène
    /// partageable entre layouts, `ui::ComplexButtonResource`).
    layout_id: usize,
    /// Le widget image.
    widget: WidgetId,
}

impl ImageImgRenderer {
    /// Constructeur — ≙ `Image` casté `ImgRenderer`.
    pub(crate) fn new(arena: &Rc<RefCell<Arena>>, layout_id: usize, widget: WidgetId) -> Self {
        Self {
            arena: Rc::downgrade(arena),
            layout_id,
            widget,
        }
    }
}

impl ImgRenderer for ImageImgRenderer {
    /// `Image::setBuffer` (image.cc:507-538) — verbatim champs +
    /// `queueResize (0, true)` ≙ `pending_resizes`.
    fn set_buffer(&mut self, buffer: &Rc<dyn Imgbuf>, _resize: bool) {
        let Some(arena) = self.arena.upgrade() else {
            return;
        };
        let mut a = arena.borrow_mut();
        let node = &mut a.nodes[self.widget];
        let (cw, ch, was_alloc, needs) = (
            node.get_content_width(),
            node.get_content_height(),
            node.was_allocated(),
            node.needs_resize(),
        );
        let WidgetKind::Image { img } = &mut node.kind else {
            return;
        };
        if was_alloc && needs && cw > 0 && ch > 0 {
            // Pas de nouveau buffer pour la transition alt→img ;
            // scale seulement si les deux dimensions connues.
            img.buf_width = cw;
            img.buf_height = ch;
            img.buffer = Some(buffer.clone().get_scaled_buf(cw, ch));
        } else {
            img.buf_width = buffer.get_root_width();
            img.buf_height = buffer.get_root_height();
            img.buffer = Some(buffer.clone());
        }
        if img.buf_width != 0 {
            node.ratio = img.buf_width as f32 / img.buf_height as f32;
        }
        a.pending_resizes
            .push((self.layout_id, self.widget, 0, true));
    }

    /// `Image::drawRow` (image.cc:540-551) — ≙
    /// `queueDrawArea` (widget-relative) → `pending_draws`.
    fn draw_row(&mut self, row: i32) {
        let Some(arena) = self.arena.upgrade() else {
            return;
        };
        let mut a = arena.borrow_mut();
        let node = &mut a.nodes[self.widget];
        let (ox, oy) = (node.box_offset_x(), node.box_offset_y());
        let WidgetKind::Image { img } = &mut node.kind else {
            return;
        };
        let Some(buffer) = img.buffer.clone() else {
            return;
        };
        let mut row_area = Rectangle::default();
        buffer.get_row_area(row, &mut row_area);
        if row_area.width != 0 && row_area.height != 0 {
            a.pending_draws.push((
                self.layout_id,
                Some(self.widget),
                Rectangle::new(
                    row_area.x + ox,
                    row_area.y + oy,
                    row_area.width,
                    row_area.height,
                ),
            ));
        }
    }

    /// `finish` — no-op verbatim (images dessinées ligne à
    /// ligne — image.cc:553-556).
    fn finish(&mut self) {}

    /// `fatal` — no-op verbatim (image.cc:558-561).
    fn fatal(&mut self) {}

    /// Downcast pour les appelants (≙ cast amont).
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl Layout {
    /// `Image (altText, markEmpty)` (image.cc:154-172).
    pub fn image_new(&mut self, alt_text: Option<Vec<u8>>, mark_empty: bool) -> WidgetId {
        let img = ImageData::new(alt_text, mark_empty);
        self.add_widget_node(WidgetKind::Image { img })
    }

    /// Accès données image.
    #[must_use]
    pub fn node_img<T>(&self, w: WidgetId, f: impl FnOnce(&ImageData) -> T) -> Option<T> {
        self.node(w, |n| match &n.kind {
            WidgetKind::Image { img } => Some(f(img)),
            _ => None,
        })
    }

    /// Mutation données image.
    pub fn node_img_mut<T>(&self, w: WidgetId, f: impl FnOnce(&mut ImageData) -> T) -> Option<T> {
        self.node_mut(w, |n| match &mut n.kind {
            WidgetKind::Image { img } => Some(f(img)),
            _ => None,
        })
    }

    /// ≙ `(ImgRenderer*)image` — renderer à donner au dicache.
    #[must_use]
    pub fn image_img_renderer(&self, w: WidgetId) -> Rc<RefCell<dyn ImgRenderer>> {
        Rc::new(RefCell::new(ImageImgRenderer::new(&self.arena, self.id, w)))
    }

    /// `getBuffer` (image.hh:174).
    #[must_use]
    pub fn image_get_buffer(&self, w: WidgetId) -> Option<Rc<dyn Imgbuf>> {
        self.node_img(w, |i| i.buffer.clone()).flatten()
    }

    /// `setBuffer` (image.cc:507-538) — porte d'entrée publique
    /// (le chemin `Layout` complet — le renderer interne enfile
    /// le même corps quand `Layout` n'est pas accessible).
    pub fn image_set_buffer(&mut self, w: WidgetId, buffer: &Rc<dyn Imgbuf>, resize: bool) {
        let _ = resize;
        let (cw, ch, was_alloc, needs) = self.node(w, |n| {
            (
                n.get_content_width(),
                n.get_content_height(),
                n.was_allocated(),
                n.needs_resize(),
            )
        });
        if was_alloc && needs && cw > 0 && ch > 0 {
            let scaled = buffer.clone().get_scaled_buf(cw, ch);
            self.node_img_mut(w, |i| {
                i.buf_width = cw;
                i.buf_height = ch;
                i.buffer = Some(scaled);
            });
        } else {
            let (bw, bh) = (buffer.get_root_width(), buffer.get_root_height());
            self.node_img_mut(w, |i| {
                i.buffer = Some(buffer.clone());
                i.buf_width = bw;
                i.buf_height = bh;
            });
        }
        self.widget_queue_resize(w, 0, true);
        let (bw, bh) = self
            .node_img(w, |i| (i.buf_width, i.buf_height))
            .unwrap_or((0, 0));
        if bw != 0 {
            self.node_mut(w, |n| n.ratio = bw as f32 / bh as f32);
        }
    }

    /// `drawRow` (image.cc:540-551) — chemin `Layout`.
    pub fn image_draw_row(&mut self, w: WidgetId, row: i32) {
        let buffer = self.node_img(w, |i| i.buffer.clone()).flatten();
        if let Some(buffer) = buffer {
            let mut area = Rectangle::default();
            buffer.get_row_area(row, &mut area);
            if area.width != 0 && area.height != 0 {
                let (ox, oy) = self.node(w, |n| (n.box_offset_x(), n.box_offset_y()));
                self.widget_queue_draw_area(w, area.x + ox, area.y + oy, area.width, area.height);
            }
        }
    }

    /// `finish` — verbatim no-op (image.cc:553-556).
    pub fn image_finish(&mut self, _w: WidgetId) {}

    /// `fatal` — verbatim no-op (image.cc:558-561).
    pub fn image_fatal(&mut self, _w: WidgetId) {}

    /// `setIsMap` (image.cc:571-574).
    pub fn image_set_is_map(&mut self, w: WidgetId) {
        self.node_img_mut(w, |i| i.is_map = true);
    }

    /// `setUseMap` (image.cc:582-588) — `list` non possédée
    /// amont ≙ `Rc` partagée ; `key` possédée ≙ `Rc`.
    pub fn image_set_use_map(
        &mut self,
        w: WidgetId,
        list: Rc<RefCell<ImageMapsList>>,
        key: Rc<dyn ImageMapKey>,
    ) {
        self.node_img_mut(w, |i| {
            i.map_list = Some(list);
            i.map_key = Some(key);
        });
    }

    /// `forceMapRedraw` (image.hh:190).
    pub fn image_force_map_redraw(&mut self, w: WidgetId) {
        if self
            .node_img(w, |i| i.map_key.is_some() && i.buffer.is_none())
            .unwrap_or(false)
        {
            self.widget_queue_draw(w);
        }
    }

    /// Largeur du texte alt (≙ `altTextWidth` lazy + `layout->
    /// textWidth`) — factorisé des trois sites verbatim.
    fn img_alt_text_width(&mut self, w: WidgetId, alt_text: &[u8]) -> i32 {
        if self.node_img(w, |i| i.alt_text_width).unwrap_or(-1) == -1 {
            let font = self
                .widget_style(w)
                .and_then(|s| s.attrs.font.clone())
                .unwrap_or_else(unreachable_font);
            let tw = self.platform.borrow_mut().text_width(&font, alt_text);
            self.node_img_mut(w, |i| i.alt_text_width = tw);
        }
        self.node_img(w, |i| i.alt_text_width).unwrap_or(0)
    }

    /// `sizeRequestSimpl` (image.cc:187-238) — verbatim.
    pub(crate) fn img_size_request_simpl(&mut self, w: WidgetId, requisition: &mut Requisition) {
        let buffer = self.node_img(w, |i| i.buffer.clone()).flatten();
        if let Some(buffer) = buffer {
            // `useAltStyle` → restaure `savedStyle`.
            let (use_alt, saved) = self
                .node_img(w, |i| (i.use_alt_style, i.saved_style.clone()))
                .unwrap_or((false, None));
            if use_alt {
                if let Some(s) = saved {
                    self.widget_set_style(w, &s);
                }
            }
            requisition.width = buffer.get_root_width();
            requisition.ascent = buffer.get_root_height();
            requisition.descent = 0;
        } else {
            let (mark_empty, use_alt) = self
                .node_img(w, |i| (i.mark_empty, i.use_alt_style))
                .unwrap_or((false, false));
            if mark_empty && !use_alt {
                let cur = self.widget_style(w).unwrap_or_else(null_style);
                let mut alt_attrs: StyleAttrs = cur.attrs.clone();
                alt_attrs.border_width.set_val(1);
                alt_attrs.padding.set_val(2);
                alt_attrs.set_border_color(alt_attrs.color);
                alt_attrs.set_border_style(crate::style::BorderStyle::Solid);
                let alt_style = Style::create(&alt_attrs);
                self.node_img_mut(w, |i| {
                    i.saved_style = Some(cur);
                    i.alt_style = Some(alt_style.clone());
                    i.use_alt_style = true;
                });
                self.widget_set_style(w, &alt_style);
            }
            let alt_text = self.node_img(w, |i| i.alt_text.clone()).flatten();
            if let Some(t) = alt_text.as_ref().filter(|t| !t.is_empty()) {
                let tw = self.img_alt_text_width(w, t);
                let (fa, fd) = self
                    .widget_style(w)
                    .and_then(|s| s.attrs.font.as_ref().map(|f| (f.ascent, f.descent)))
                    .unwrap_or((0, 0));
                requisition.width = tw;
                requisition.ascent = fa;
                requisition.descent = fd;
            } else {
                requisition.width = 0;
                requisition.ascent = 0;
                requisition.descent = 0;
            }
        }
        let (bdw, boy, brh) = self.node(w, |n| {
            (n.box_diff_width(), n.box_offset_y(), n.box_rest_height())
        });
        requisition.width += bdw;
        requisition.ascent += boy;
        requisition.descent += brh;
        self.widget_correct_requisition(w, requisition, split_height_preserve_descent, true, true);
    }

    /// `getExtremesSimpl` (image.cc:240-264) — verbatim.
    pub(crate) fn img_get_extremes_simpl(&mut self, w: WidgetId, extremes: &mut Extremes) {
        let buffer = self.node_img(w, |i| i.buffer.clone()).flatten();
        let content_width = if let Some(buffer) = buffer {
            buffer.get_root_width()
        } else {
            let alt_text = self.node_img(w, |i| i.alt_text.clone()).flatten();
            match alt_text.as_ref().filter(|t| !t.is_empty()) {
                Some(t) => self.img_alt_text_width(w, t),
                None => 0,
            }
        };
        let bdw = self.node(w, crate::widget::WidgetNode::box_diff_width);
        let width = content_width + bdw;
        // Largeur % : l'image peut être plus étroite que le
        // buffer (verbatim — même effet ignoré pour maxWidth).
        let is_per = self
            .widget_style(w)
            .is_some_and(|s| crate::style::is_per_length(s.attrs.width));
        extremes.min_width = if is_per { bdw } else { width };
        extremes.max_width = width;
        extremes.min_width_intrinsic = extremes.min_width;
        extremes.max_width_intrinsic = extremes.max_width;
        self.widget_correct_extremes(w, extremes, false);
        extremes.adjustment_width = extremes.min_width_intrinsic.min(extremes.min_width);
    }

    /// `sizeAllocateImpl` (image.cc:266-307) — verbatim.
    pub(crate) fn img_size_allocate_impl(&mut self, w: WidgetId, allocation: &Allocation) {
        let (bdw, bdh) = self.node(w, |n| (n.box_diff_width(), n.box_diff_height()));
        let new_buf_width = allocation.width - bdw;
        let new_buf_height = allocation.ascent + allocation.descent - bdh;
        let (buffer, bw, bh) = self
            .node_img(w, |i| (i.buffer.clone(), i.buf_width, i.buf_height))
            .unwrap_or((None, 0, 0));
        if let Some(buffer) = buffer {
            if new_buf_width > 0
                && new_buf_height > 0
                && (new_buf_width != bw || new_buf_height != bh)
            {
                let scaled = buffer.get_scaled_buf(new_buf_width, new_buf_height);
                self.node_img_mut(w, |i| {
                    i.buffer = Some(scaled);
                    i.buf_width = new_buf_width;
                    i.buf_height = new_buf_height;
                });
            }
        }
    }

    /// `contentX`/`contentY` (image.cc:366-379) — coords
    /// relatives au contenu, clampées aux bords.
    fn img_content_xy(&self, w: WidgetId, event: &MousePositionEvent) -> (i32, i32) {
        let (ox, oy, cw, ch) = self.node(w, |n| {
            (
                n.box_offset_x(),
                n.box_offset_y(),
                n.get_content_width(),
                n.get_content_height(),
            )
        });
        (
            (event.x_widget - ox).clamp(0, cw),
            (event.y_widget - oy).clamp(0, ch),
        )
    }

    /// `enterNotifyImpl` (image.cc:323-330) — `x_link` du style
    /// (BUG amont : faux pour les cartes — verbatim) puis base.
    pub(crate) fn img_enter_notify_impl(&mut self, w: WidgetId, event: &EventCrossing) {
        let _ = event;
        let x_link = self.widget_style(w).map_or(-1, |s| s.attrs.x_link);
        self.node_img_mut(w, |i| i.curr_link = x_link);
        if x_link != -1 {
            self.emit_link_enter(Some(w), x_link, -1, -1, -1);
        }
        // `Widget::enterNotifyImpl` verbatim (tooltip).
        let tooltip = self.node(w, |n| {
            n.style.as_ref().and_then(|s| s.attrs.x_tooltip.clone())
        });
        if let Some(t) = tooltip {
            t.on_enter();
        }
    }

    /// `leaveNotifyImpl` (image.cc:332-342) — verbatim.
    pub(crate) fn img_leave_notify_impl(&mut self, w: WidgetId, event: &EventCrossing) {
        let _ = event;
        let had_link = self.node_img(w, |i| i.curr_link).unwrap_or(-1) != -1;
        self.node_img_mut(w, |i| {
            i.clicking = false;
            i.curr_link = -1;
        });
        if had_link {
            self.emit_link_enter(Some(w), -1, -1, -1, -1);
        }
        // `Widget::leaveNotifyImpl` verbatim (tooltip).
        let tooltip = self.node(w, |n| {
            n.style.as_ref().and_then(|s| s.attrs.x_tooltip.clone())
        });
        if let Some(t) = tooltip {
            t.on_leave();
        }
    }

    /// `motionNotifyImpl` (image.cc:381-405) — verbatim.
    #[allow(clippy::many_single_char_names)] // verbatim
    pub(crate) fn img_motion_notify_impl(&mut self, w: WidgetId, event: &EventMotion) -> bool {
        let (has_map_list, is_map) = self
            .node_img(w, |i| (i.map_list.is_some(), i.is_map))
            .unwrap_or((false, false));
        if has_map_list || is_map {
            let pos = MousePositionEvent::from(event);
            let (x, y) = self.img_content_xy(w, &pos);
            if has_map_list {
                // Carte client.
                let (list, key) = self
                    .node_img(w, |i| (i.map_list.clone(), i.map_key.clone()))
                    .unwrap_or((None, None));
                let new_link = match (&list, &key) {
                    (Some(l), Some(k)) => l.borrow().link(k, x, y),
                    _ => -1,
                };
                let curr = self.node_img(w, |i| i.curr_link).unwrap_or(-1);
                if new_link != curr {
                    self.node_img_mut(w, |i| {
                        i.curr_link = new_link;
                        i.clicking = false;
                    });
                    let style_cursor = self
                        .widget_style(w)
                        .map_or(crate::style::Cursor::Default, |s| s.attrs.cursor);
                    self.set_cursor(if new_link == -1 {
                        style_cursor
                    } else {
                        crate::style::Cursor::Pointer
                    });
                    self.emit_link_enter(Some(w), new_link, -1, -1, -1);
                }
            } else if is_map {
                // Carte serveur.
                let curr = self.node_img(w, |i| i.curr_link).unwrap_or(-1);
                if curr != -1 {
                    self.emit_link_enter(Some(w), curr, -1, x, y);
                }
            }
        }
        true
    }

    /// `buttonPressImpl` (image.cc:407-426) — verbatim.
    #[allow(clippy::many_single_char_names)] // verbatim
    pub(crate) fn img_button_press_impl(&mut self, w: WidgetId, event: &EventButton) -> bool {
        let pos = MousePositionEvent::from(event);
        let (x, y) = self.img_content_xy(w, &pos);
        let link = {
            let (list, key) = self
                .node_img(w, |i| (i.map_list.clone(), i.map_key.clone()))
                .unwrap_or((None, None));
            match (&list, &key) {
                (Some(l), Some(k)) => l.borrow().link(k, x, y),
                _ => self.widget_style(w).map_or(-1, |s| s.attrs.x_link),
            }
        };
        self.node_img_mut(w, |i| i.curr_link = link);
        if event.button == 3 {
            let x_img = self.widget_style(w).map_or(-1, |s| s.attrs.x_img);
            self.emit_link_press(Some(w), link, x_img, -1, -1, event);
            true
        } else if event.button == 1 || link != -1 {
            self.node_img_mut(w, |i| i.clicking = true);
            true
        } else {
            false
        }
    }

    /// `buttonReleaseImpl` (image.cc:428-440) — verbatim.
    #[allow(clippy::many_single_char_names)] // verbatim
    pub(crate) fn img_button_release_impl(&mut self, w: WidgetId, event: &EventButton) -> bool {
        let pos = MousePositionEvent::from(event);
        let (x, y) = self.img_content_xy(w, &pos);
        let link = {
            let (list, key) = self
                .node_img(w, |i| (i.map_list.clone(), i.map_key.clone()))
                .unwrap_or((None, None));
            match (&list, &key) {
                (Some(l), Some(k)) => l.borrow().link(k, x, y),
                _ => self.widget_style(w).map_or(-1, |s| s.attrs.x_link),
            }
        };
        self.node_img_mut(w, |i| i.curr_link = link);
        if self.node_img(w, |i| i.clicking).unwrap_or(false) {
            let is_map = self.node_img(w, |i| i.is_map).unwrap_or(false);
            let (cx, cy) = if is_map { (x, y) } else { (-1, -1) };
            self.node_img_mut(w, |i| i.clicking = false);
            let x_img = self.widget_style(w).map_or(-1, |s| s.attrs.x_img);
            self.emit_link_click(Some(w), link, x_img, cx, cy, event);
            return true;
        }
        false
    }

    /// `draw` (image.cc:442-505) — verbatim.
    pub(crate) fn img_draw(
        &mut self,
        w: WidgetId,
        view: &mut dyn View,
        area: &Rectangle,
        _context: &mut DrawingContext,
    ) {
        self.widget_draw_widget_box(w, view, area, false);
        let (buffer, dx, dy, cw, ch, ax, ay) = self.node(w, |n| {
            let b = match &n.kind {
                WidgetKind::Image { img } => img.buffer.clone(),
                _ => None,
            };
            (
                b,
                n.box_offset_x(),
                n.box_offset_y(),
                n.get_content_width(),
                n.get_content_height(),
                n.allocation.x,
                n.allocation.y,
            )
        });
        if let Some(buffer) = buffer {
            let content = Rectangle::new(dx, dy, cw, ch);
            if let Some(inter) = area.intersects_with(&content) {
                view.draw_image(
                    &buffer,
                    ax + dx,
                    ay + dy,
                    inter.x - dx,
                    inter.y - dy,
                    inter.width,
                    inter.height,
                );
            }
            return;
        }
        let alt_text = self.node_img(w, |i| i.alt_text.clone()).flatten();
        if let Some(t) = alt_text.as_ref().filter(|t| !t.is_empty()) {
            let tw = self.img_alt_text_width(w, t);
            let (font, color, fa, fd) = {
                let s = self.widget_style(w).unwrap_or_else(null_style);
                (
                    s.attrs.font.clone().unwrap_or_else(unreachable_font),
                    s.attrs.color.unwrap_or_else(|| Color::new(0)),
                    s.attrs.font.as_ref().map_or(0, |f| f.ascent),
                    s.attrs.font.as_ref().map_or(0, |f| f.descent),
                )
            };
            let mut clipping_view = if cw < tw || ch < fa + fd {
                Some(view.get_clipping_view(ax + dx, ay + dy, cw, ch))
            } else {
                None
            };
            let text_view: &mut dyn View = match &mut clipping_view {
                Some(v) => &mut **v,
                None => view,
            };
            text_view.draw_simple_wrapped_text(
                &font,
                &color,
                crate::style::Shading::Normal,
                ax + dx,
                ay + dy,
                cw,
                ch,
                t,
            );
            if let Some(cv) = clipping_view {
                view.merge_clipping_view(cv);
            }
        }
        let (list, key) = self
            .node_img(w, |i| (i.map_list.clone(), i.map_key.clone()))
            .unwrap_or((None, None));
        if let (Some(l), Some(k)) = (list, key) {
            let mut clipping_view = view.get_clipping_view(ax + dx, ay + dy, cw, ch);
            let color = self
                .widget_style(w)
                .and_then(|s| s.attrs.color)
                .unwrap_or_else(|| Color::new(0));
            l.borrow()
                .draw_map(&k, &mut *clipping_view, &color, ax + dx, ay + dy);
            view.merge_clipping_view(clipping_view);
        }
    }
}
