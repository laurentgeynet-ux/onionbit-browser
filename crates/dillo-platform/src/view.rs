//! `view` — `SkiaView`, implémentation de `dw::core::View`.
//!
//! Remplace `dw/fltkviewbase.cc` + `dw/fltkviewport.cc` +
//! `dw/fltkflatview.cc` (ADR-0002) : le `Pixmap` tiny-skia joue le
//! rôle du back-buffer FLTK (`FltkViewBase::BackBuffer`), les
//! primitives `draw*` sont rasterisées par tiny-skia, et le
//! viewport reproduit `scrollTo`/`scroll(cmd)`/`fl_scroll`
//! verbatim.
//!
//! Coordonnées : `canvas` = repère du contenu (≙ canvas amont) ;
//! `view` = repère pixmap = `canvas - scroll`. Les `draw_*`
//! reçoivent des coords *canvas* (verbatim
//! `translateCanvasXToViewX`). `FlatView` (≙ `FltkFlatView`, vue
//! des layouts internes des `ComplexButtonResource`) écrit dans le
//! même back-buffer avec un `origin` en coords *canvas* externes.
//!
//! État partagé : `Canvas` (`Rc<RefCell>`) détient pixmap, clips,
//! scroll, damage — `SkiaView`, `ClippingView` (≙
//! `fl_push_clip`/`fl_pop_clip`) et `FlatView` y délèguent, comme
//! amont où la view est unique et la pile de clips globale au
//! thread.
//!
//! Damage : `queue_draw` accumule une région (≙ `drawRegion` +
//! `FL_DAMAGE_USER1`) ; le shell appelle [`SkiaView::take_damage`]
//! puis `Layout::draw` + `present` (≙ `Fl_Widget::draw`).
//!
//! `Pixmap` est `Option` : tiny-skia retourne `Option` à la
//! création — la seule cause réelle d'échec après clamp est l'OOM
//! (non rattrapable en pratique, `vec!` aborte) ; le `None`
//! dégrade les draws en no-op plutôt que paniquer (tolérance).

// Casts verbatim (ints amont → f32 pixels) — convention du port.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
// Signatures verbatim `View` (couleurs par ref, noms courts,
// 8+ args ≙ API C).
#![allow(clippy::many_single_char_names)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::struct_excessive_bools)]

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::{Rc, Weak};

use dillo_common::utf8;
use dillo_dw::imgbuf::Imgbuf;
use dillo_dw::style::{Color, Cursor, Font, FontVariant, LineType, Shading};
use dillo_dw::types::{Point, Rectangle, ScrollCommand};
use dillo_dw::view::View;
use tiny_skia::{FillRule, Mask, Paint, PathBuilder, Pixmap, Rect, Stroke, StrokeDash, Transform};

use crate::platform::SkiaPlatform;

/// `SCROLLBAR_THICKNESS` (fltkviewport.hh:40).
pub const SCROLLBAR_THICKNESS: i32 = 15;
/// `pageScrollDelay` (fltkviewport.cc:79) — consommé par `app.rs`.
pub const PAGE_SCROLL_DELAY: f64 = 0.300;
/// `pageScrollInterval` (fltkviewport.cc:80).
pub const PAGE_SCROLL_INTERVAL: f64 = 0.100;
/// `selectionScroll` période (fltkviewport.cc:544).
pub const SELECTION_SCROLL_PERIOD: f64 = 0.025;

/// Conversion `Color`+`Shading` → `tiny_skia::Color` opaque
/// (≙ `colors[shading]` amont — `shade` verbatim `shadeColor`).
fn skia_color(color: &Color, shading: Shading) -> tiny_skia::Color {
    let c = color.shade(shading).get_color() as u32;
    tiny_skia::Color::from_rgba8(
        ((c >> 16) & 0xff) as u8,
        ((c >> 8) & 0xff) as u8,
        (c & 0xff) as u8,
        255,
    )
}

/// `Canvas` — état de dessin partagé `SkiaView`/`ClippingView`/
/// `FlatView` (≙ `FltkViewBase` : pixmap + clip stack + scroll +
/// damage). Toutes les méthodes de dessin prennent `origin`
/// (coords canvas externes où le (0,0) de la vue appelante se
/// projette — `(0,0)` pour la vue racine, position du widget pour
/// les `FlatView`).
pub(crate) struct Canvas {
    /// Back-buffer ≙ `FltkViewBase::BackBuffer` (offscreen).
    /// `None` = allocation impossible (dégradé no-op).
    pixmap: Option<Pixmap>,
    /// Plateforme (fontes, swash). `Weak<RefCell>` — le layout
    /// tient `Rc<RefCell<dyn Platform>>` ; la vue n'a qu'un lien
    /// faible (≙ `platform` pointeur non-possédant amont).
    platform: Weak<RefCell<SkiaPlatform>>,
    /// `scrollX`/`scrollY`/`scrollDX`/`scrollDY` (fltkviewport).
    scroll_x: i32,
    scroll_y: i32,
    scroll_dx: i32,
    scroll_dy: i32,
    /// `canvasWidth`/`canvasHeight` (`setCanvasSize`).
    canvas_w: i32,
    canvas_h: i32,
    /// `bgColor` (0xRRGGBB ; `FL_WHITE` ≙ 0xffffff défaut).
    bg: u32,
    /// `drawRegion` — régions de damage accumulées.
    damage: Vec<Rectangle>,
    /// Full redraw (`FL_DAMAGE_EXPOSE`).
    damage_all: bool,
    /// Pile de clips ≙ `fl_push_clip` (coords canvas de la vue
    /// racine — les `FlatView` y poussent traduits).
    clips: Vec<(i32, i32, i32, i32)>,
    /// `hscrollbar`/`vscrollbar` `->visible()`.
    hsb_visible: bool,
    vsb_visible: bool,
    /// `scrollbarOnLeft`.
    scrollbar_on_left: bool,
    /// `pageOverlap`.
    page_overlap: i32,
    /// `linesize` (h=v verbatim).
    scroll_step: i32,
    /// `scrollbarPageMode`.
    scrollbar_page_mode: bool,
    /// `mouse_x`/`mouse_y` (vue ; `-1` hors).
    mouse_x: i32,
    mouse_y: i32,
    /// `cursor` demandé par le contenu (≙ `Fl::cursor` — sur le
    /// Canvas pour que `SharedView` y accède comme amont).
    cursor: Cursor,
    /// Taille viewport widget (`w`/`h`).
    view_w: i32,
    view_h: i32,
}

impl Canvas {
    /// Pixmap borné : `Pixmap::new` refuse `0` et les débordements
    /// de `w*h*4` — dimensions clampées à `[1, i32::MAX/4]` × aire
    /// ≤ `i32::MAX/4` px ; `None` ne reste que pour l'échec
    /// d'allocation (dégradé).
    fn new_pixmap(w: u32, h: u32) -> Option<Pixmap> {
        const MAX_DIM: u32 = (i32::MAX / 4) as u32;
        let w = w.clamp(1, MAX_DIM);
        let h = h.clamp(1, MAX_DIM / w.max(1));
        Pixmap::new(w, h)
    }

    fn new(platform: &Rc<RefCell<SkiaPlatform>>, width: u32, height: u32) -> Self {
        Self {
            pixmap: Self::new_pixmap(width, height),
            platform: Rc::downgrade(platform),
            scroll_x: 0,
            scroll_y: 0,
            scroll_dx: 0,
            scroll_dy: 0,
            canvas_w: 1,
            canvas_h: 1,
            bg: 0xff_ffff,
            damage: Vec::new(),
            damage_all: true,
            clips: Vec::new(),
            hsb_visible: false,
            vsb_visible: false,
            scrollbar_on_left: false,
            page_overlap: 0,
            scroll_step: 0,
            scrollbar_page_mode: false,
            mouse_x: -1,
            mouse_y: -1,
            cursor: Cursor::Default,
            view_w: width.max(1) as i32,
            view_h: height.max(1) as i32,
        }
    }

    /// Dimensions du back-buffer `(0,0)` si absent.
    fn px_size(&self) -> (u32, u32) {
        self.pixmap
            .as_ref()
            .map_or((0, 0), |p| (p.width(), p.height()))
    }

    /// `translateCanvasXToViewX` (view commence à 0) + origine.
    fn vx(&self, origin: (i32, i32), x: i32) -> f32 {
        (x + origin.0 - self.scroll_x) as f32
    }

    /// `translateCanvasYToViewY` + origine.
    fn vy(&self, origin: (i32, i32), y: i32) -> f32 {
        (y + origin.1 - self.scroll_y) as f32
    }

    /// Intersection de la pile de clips en coords *view*
    /// (origine nulle — les clips sont déjà stockés traduits).
    /// `None` = pile vide (pas de clip).
    fn clip_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut b: Option<(f32, f32, f32, f32)> = None;
        for &(x, y, w, h) in &self.clips {
            let r = (
                (x - self.scroll_x) as f32,
                (y - self.scroll_y) as f32,
                w as f32,
                h as f32,
            );
            b = Some(match b {
                None => r,
                Some((bx, by, bw, bh)) => {
                    let x1 = bx.max(r.0);
                    let y1 = by.max(r.1);
                    let x2 = (bx + bw).min(r.0 + r.2);
                    let y2 = (by + bh).min(r.1 + r.3);
                    if x2 <= x1 || y2 <= y1 {
                        (0.0, 0.0, 0.0, 0.0)
                    } else {
                        (x1, y1, x2 - x1, y2 - y1)
                    }
                }
            });
        }
        b
    }

    /// `Mask` tiny-skia pour le clip courant — `None` = pas de
    /// clip (pile vide). Un clip *vide* produit un mask nul ≙
    /// rien n'est dessiné (verbatim `fl_clip` d'une région vide).
    fn clip_mask(&self) -> Option<Mask> {
        let (x, y, w, h) = self.clip_bounds()?;
        let (pw, ph) = self.px_size();
        let mut mask = Mask::new(pw.max(1), ph.max(1))?;
        if w > 0.0 && h > 0.0 {
            if let Some(r) = Rect::from_xywh(x, y, w, h) {
                mask.fill_path(
                    &PathBuilder::from_rect(r),
                    FillRule::Winding,
                    true,
                    Transform::default(),
                );
            }
        }
        Some(mask)
    }

    /// Paint solide `color`/`shading`.
    #[allow(clippy::unused_self)]
    fn paint(&self, color: &Color, shading: Shading) -> Paint<'static> {
        Paint {
            shader: tiny_skia::Shader::SolidColor(skia_color(color, shading)),
            anti_alias: true,
            ..Paint::default()
        }
    }

    /// `fl_scroll` — blit du contenu (≙ `FL_DAMAGE_SCROLL`).
    fn blit_scroll(&mut self, dx: i32, dy: i32) {
        if dx == 0 && dy == 0 {
            return;
        }
        let Some(src) = self.pixmap.take() else {
            return;
        };
        let (w, h) = (src.width() as i32, src.height() as i32);
        if w <= 0 || h <= 0 {
            self.pixmap = Some(src);
            return;
        }
        let Some(mut tmp) = Self::new_pixmap(src.width(), src.height()) else {
            self.pixmap = Some(src);
            return;
        };
        let w32 = w as u32;
        for sy in 0..h {
            let ty = sy + dy;
            if ty < 0 || ty >= h {
                continue;
            }
            for sx in 0..w {
                let tx = sx + dx;
                if tx < 0 || tx >= w {
                    continue;
                }
                if let Some(p) = src.pixel(sx as u32, sy as u32) {
                    tmp.pixels_mut()[(ty as u32 * w32 + tx as u32) as usize] = p;
                }
            }
        }
        self.pixmap = Some(tmp);
    }

    /// `fl_rectf(bg)` — fond d'une région (≙ début de `draw_area`).
    fn fill_bg(&mut self, origin: (i32, i32), rect: &Rectangle) {
        let Some(r) = Rect::from_xywh(
            self.vx(origin, rect.x),
            self.vy(origin, rect.y),
            rect.width.max(0) as f32,
            rect.height.max(0) as f32,
        ) else {
            return;
        };
        let c = self.bg;
        let paint = Paint {
            shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::from_rgba8(
                ((c >> 16) & 0xff) as u8,
                ((c >> 8) & 0xff) as u8,
                (c & 0xff) as u8,
                255,
            )),
            ..Paint::default()
        };
        let mut pb = PathBuilder::new();
        pb.push_rect(r);
        let Some(path) = pb.finish() else {
            return;
        };
        if let Some(px) = self.pixmap.as_mut() {
            px.fill_path(&path, &paint, FillRule::Winding, Transform::default(), None);
        }
    }

    /// `scrollTo` verbatim (fltkviewport.cc:481).
    fn scroll_to(&mut self, x: i32, y: i32) {
        let hdiff = i32::from(self.vsb_visible) * SCROLLBAR_THICKNESS;
        let vdiff = i32::from(self.hsb_visible) * SCROLLBAR_THICKNESS;
        let x = x.clamp(0, (self.canvas_w - self.view_w + hdiff).max(0));
        let y = y.clamp(0, (self.canvas_h - self.view_h + vdiff).max(0));
        if x == self.scroll_x && y == self.scroll_y {
            return;
        }
        let dx = x - self.scroll_x;
        let dy = y - self.scroll_y;
        self.scroll_dx += dx;
        self.scroll_dy += dy;
        self.scroll_x = x;
        self.scroll_y = y;
        // ≙ damage(FL_DAMAGE_SCROLL) → fl_scroll + bande exposée.
        self.blit_scroll(-dx, -dy);
        let (vx, vy) = (self.scroll_x, self.scroll_y);
        let band = if dx.abs() >= self.view_w || dy.abs() >= self.view_h {
            Rectangle {
                x: vx,
                y: vy,
                width: self.view_w,
                height: self.view_h,
            }
        } else if dx != 0 {
            Rectangle {
                x: if dx > 0 { vx + self.view_w - dx } else { vx },
                y: vy,
                width: dx.abs(),
                height: self.view_h,
            }
        } else {
            Rectangle {
                x: vx,
                y: if dy > 0 { vy + self.view_h - dy } else { vy },
                width: self.view_w,
                height: dy.abs(),
            }
        };
        self.damage.push(band);
    }

    /// `scroll(ScrollCommand)` verbatim (fltkviewport.cc:511).
    fn scroll_cmd(&mut self, cmd: ScrollCommand) {
        let hdiff = i32::from(self.vsb_visible) * SCROLLBAR_THICKNESS;
        let vdiff = i32::from(self.hsb_visible) * SCROLLBAR_THICKNESS;
        match cmd {
            ScrollCommand::ScreenUp => self.scroll_to(
                self.scroll_x,
                self.scroll_y - self.view_h + self.page_overlap + vdiff,
            ),
            ScrollCommand::ScreenDown => self.scroll_to(
                self.scroll_x,
                self.scroll_y + self.view_h - self.page_overlap - vdiff,
            ),
            ScrollCommand::ScreenLeft => self.scroll_to(
                self.scroll_x - self.view_w + self.page_overlap + hdiff,
                self.scroll_y,
            ),
            ScrollCommand::ScreenRight => self.scroll_to(
                self.scroll_x + self.view_w - self.page_overlap - hdiff,
                self.scroll_y,
            ),
            ScrollCommand::LineUp => {
                self.scroll_to(self.scroll_x, self.scroll_y - self.scroll_step);
            }
            ScrollCommand::LineDown => {
                self.scroll_to(self.scroll_x, self.scroll_y + self.scroll_step);
            }
            ScrollCommand::Left => {
                self.scroll_to(self.scroll_x - self.scroll_step, self.scroll_y);
            }
            ScrollCommand::Right => {
                self.scroll_to(self.scroll_x + self.scroll_step, self.scroll_y);
            }
            ScrollCommand::Top => self.scroll_to(self.scroll_x, 0),
            ScrollCommand::Bottom => self.scroll_to(self.scroll_x, self.canvas_h),
            ScrollCommand::None => {}
        }
    }

    /// Blend src-over d'un pixel RGBA non-premultiplié (couverture
    /// swash dans `a`) dans le pixmap — ≙ `fl_draw` glyph blending ;
    /// clip manuel ≙ `fl_push_clip` (les pixels passent par
    /// callback, pas par path). `x`/`y`/`clip` en coords *view*.
    fn blend_pixel(&mut self, x: i32, y: i32, pc: [u8; 4], clip: Option<(f32, f32, f32, f32)>) {
        if let Some((cx0, cy0, cw, ch)) = clip {
            if (x as f32) < cx0 || (y as f32) < cy0 || x as f32 >= cx0 + cw || y as f32 >= cy0 + ch
            {
                return;
            }
        }
        let Some(px) = self.pixmap.as_mut() else {
            return;
        };
        if x < 0 || y < 0 || x >= px.width() as i32 || y >= px.height() as i32 {
            return;
        }
        let idx = (y as u32 * px.width() + x as u32) as usize;
        let Some(dst) = px.pixel(x as u32, y as u32) else {
            return;
        };
        // src-over : `src*a/255 + dst*(1-a)/255` (`pc` non-premul).
        let a = u16::from(pc[3]);
        let inv = 255 - a;
        let r = (u16::from(pc[0]) * a / 255 + u16::from(dst.red()) * inv / 255) as u8;
        let g = (u16::from(pc[1]) * a / 255 + u16::from(dst.green()) * inv / 255) as u8;
        let b = (u16::from(pc[2]) * a / 255 + u16::from(dst.blue()) * inv / 255) as u8;
        if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, 255) {
            px.pixels_mut()[idx] = p;
        }
    }

    /// Dessine un `Buffer` cosmic à `(base_x, base_y)` (baseline)
    /// en pixels *view* — ≙ `fl_draw` glyph-by-glyph. Retourne la
    /// largeur shapeée (pour `letterSpacing`/small-caps
    /// par-caractère).
    fn draw_buffer(
        &mut self,
        text: &str,
        attrs: cosmic_text::Attrs<'_>,
        size: f32,
        base_x: f32,
        base_y: f32,
        rgba: tiny_skia::Color,
    ) -> f32 {
        let clip = self.clip_bounds();
        let Some(platform) = self.platform.upgrade() else {
            return 0.0;
        };
        let p = platform.borrow();
        let mut fs = p.font_system.borrow_mut();
        let mut sw = p.swash.borrow_mut();
        let mut buf = cosmic_text::Buffer::new(&mut fs, cosmic_text::Metrics::new(size, size));
        buf.set_text(&mut fs, text, attrs, cosmic_text::Shaping::Advanced);
        buf.shape_until_scroll(&mut fs, true);
        let w: f32 = buf.layout_runs().map(|r| r.line_w).sum();
        let cc = cosmic_text::Color::rgba(
            (rgba.red() * 255.0) as u8,
            (rgba.green() * 255.0) as u8,
            (rgba.blue() * 255.0) as u8,
            255,
        );
        let (bx, by) = (base_x as i32, base_y as i32);
        buf.draw(&mut fs, &mut sw, cc, |px, py, gw, gh, col| {
            let pc = col.as_rgba();
            for dy in 0..gh as i32 {
                for dx in 0..gw as i32 {
                    self.blend_pixel(px + dx + bx, py + dy + by, pc, clip);
                }
            }
        });
        w
    }

    // ------------------------------------------------------------------
    // Primitives `draw*` — coords *canvas* de la vue appelante +
    // `origin` (≙ `translateCanvas*ToView*` + offset FlatView).
    // ------------------------------------------------------------------

    /// `drawLine` — ≙ `fl_line` ; le clip ±5000 amont (artefacts
    /// de dash) devient le `Mask` (les artefacts n'existent pas en
    /// tiny-skia — stroke AA par segment).
    fn draw_line_at(
        &mut self,
        o: (i32, i32),
        color: &Color,
        shading: Shading,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    ) {
        let mut pb = PathBuilder::new();
        pb.move_to(self.vx(o, x1), self.vy(o, y1));
        pb.line_to(self.vx(o, x2), self.vy(o, y2));
        let Some(path) = pb.finish() else {
            return;
        };
        let paint = self.paint(color, shading);
        let mask = self.clip_mask();
        if let Some(px) = self.pixmap.as_mut() {
            px.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: 1.0,
                    ..Stroke::default()
                },
                Transform::default(),
                mask.as_ref(),
            );
        }
    }

    /// `drawTypedLine` — verbatim : dotted = dash `[1,
    /// 2*width-gap]` custom ; dashed = `[3,3]` round-cap.
    #[allow(clippy::too_many_arguments)]
    fn draw_typed_line_at(
        &mut self,
        o: (i32, i32),
        color: &Color,
        shading: Shading,
        line_type: LineType,
        width: i32,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    ) {
        let mut stroke = Stroke {
            width: width.max(1) as f32,
            line_cap: tiny_skia::LineCap::Round,
            ..Stroke::default()
        };
        if line_type == LineType::Dotted {
            // verbatim fltkviewbase.cc:478 — f=2.
            let len = if x2 == x1 {
                y2 - y1 + 1
            } else if y2 == y1 {
                x2 - x1 + 1
            } else {
                0
            };
            let w = if width == 1 { 0 } else { width };
            let ng = len / 2 * width;
            let d = len % (2 * width);
            let gap = if ng != 0 {
                d / ng + i32::from(w > 3) * 2
            } else {
                0
            };
            let dash_len = (2 * width - gap).max(1) as f32;
            stroke.dash = StrokeDash::new(vec![1.0, dash_len], 0.0);
        } else if line_type == LineType::Dashed {
            stroke.dash = StrokeDash::new(vec![3.0, 3.0], 0.0);
        }
        let mut pb = PathBuilder::new();
        pb.move_to(self.vx(o, x1), self.vy(o, y1));
        pb.line_to(self.vx(o, x2), self.vy(o, y2));
        let Some(path) = pb.finish() else {
            return;
        };
        let paint = self.paint(color, shading);
        let mask = self.clip_mask();
        if let Some(px) = self.pixmap.as_mut() {
            px.stroke_path(&path, &paint, &stroke, Transform::default(), mask.as_ref());
        }
    }

    /// `drawRectangle` — verbatim : w/h négatifs normalisés ;
    /// filled → fill ; outline → stroke 1 px (≙ `fl_rect`
    /// incluant les deux bords → rect `(+.5, w-1, h-1)`).
    #[allow(clippy::too_many_arguments)]
    fn draw_rectangle_at(
        &mut self,
        o: (i32, i32),
        color: &Color,
        shading: Shading,
        filled: bool,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        let (mut x, mut y, mut w, mut h) = (x, y, width, height);
        if w < 0 {
            x += w;
            w = -w;
        }
        if h < 0 {
            y += h;
            h = -h;
        }
        let paint = self.paint(color, shading);
        let mask = self.clip_mask();
        let mut pb = PathBuilder::new();
        if filled {
            if let Some(r) = Rect::from_xywh(self.vx(o, x), self.vy(o, y), w as f32, h as f32) {
                pb.push_rect(r);
            }
        } else if let Some(r) = Rect::from_xywh(
            self.vx(o, x) + 0.5,
            self.vy(o, y) + 0.5,
            (w - 1).max(1) as f32,
            (h - 1).max(1) as f32,
        ) {
            pb.push_rect(r);
        }
        let Some(path) = pb.finish() else {
            return;
        };
        let Some(px) = self.pixmap.as_mut() else {
            return;
        };
        if filled {
            px.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::default(),
                mask.as_ref(),
            );
        } else {
            px.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: 1.0,
                    ..Stroke::default()
                },
                Transform::default(),
                mask.as_ref(),
            );
        }
    }

    /// `drawArc` — `fl_arc`/`fl_pie` : ellipse centrée, angles en
    /// degrés FLTK (0 = 3h) ; verbatim arc *et* pie quand rempli
    /// (workaround STR #2703 conservé).
    #[allow(clippy::too_many_arguments)]
    fn draw_arc_at(
        &mut self,
        o: (i32, i32),
        color: &Color,
        shading: Shading,
        filled: bool,
        center_x: i32,
        center_y: i32,
        width: i32,
        height: i32,
        angle1: i32,
        angle2: i32,
    ) {
        let (cx, cy) = (self.vx(o, center_x), self.vy(o, center_y));
        let (rx, ry) = (width as f32 / 2.0, height as f32 / 2.0);
        // `fl_arc` : angles en 1/64°, anti-horaires depuis 3h ;
        // y écran vers le bas → `-a/64` reproduit la convention.
        let a0 = -(angle1 as f32 / 64.0).to_radians();
        let a1 = -((angle1 + angle2) as f32 / 64.0).to_radians();
        let steps = ((a1 - a0).abs() / std::f32::consts::FRAC_PI_2)
            .ceil()
            .max(1.0) as i32;
        let mut pb = PathBuilder::new();
        for i in 0..=steps {
            let t = a0 + (a1 - a0) * (i as f32 / steps as f32);
            let (px, py) = (cx + rx * t.cos(), cy + ry * t.sin());
            if i == 0 {
                if filled {
                    pb.move_to(cx, cy);
                }
                pb.move_to(px, py);
            } else {
                pb.line_to(px, py);
            }
        }
        if filled {
            pb.close();
        }
        let Some(path) = pb.finish() else {
            return;
        };
        let paint = self.paint(color, shading);
        let mask = self.clip_mask();
        let Some(px) = self.pixmap.as_mut() else {
            return;
        };
        if filled {
            px.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::default(),
                mask.as_ref(),
            );
        }
        // verbatim : `fl_arc` toujours tracé aussi.
        px.stroke_path(
            &path,
            &paint,
            &Stroke {
                width: 1.0,
                ..Stroke::default()
            },
            Transform::default(),
            mask.as_ref(),
        );
    }

    /// `drawPolygon` — convex → Winding, concave → `EvenOdd`
    /// (`fl_begin_polygon`/`complex`), outline → `fl_begin_loop`.
    fn draw_polygon_at(
        &mut self,
        o: (i32, i32),
        color: &Color,
        shading: Shading,
        filled: bool,
        convex: bool,
        points: &[Point],
    ) {
        if points.is_empty() {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(self.vx(o, points[0].x), self.vy(o, points[0].y));
        for p in &points[1..] {
            pb.line_to(self.vx(o, p.x), self.vy(o, p.y));
        }
        pb.close();
        let Some(path) = pb.finish() else {
            return;
        };
        let paint = self.paint(color, shading);
        let mask = self.clip_mask();
        let Some(px) = self.pixmap.as_mut() else {
            return;
        };
        if filled {
            px.fill_path(
                &path,
                &paint,
                if convex {
                    FillRule::Winding
                } else {
                    FillRule::EvenOdd
                },
                Transform::default(),
                mask.as_ref(),
            );
        } else {
            px.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: 1.0,
                    ..Stroke::default()
                },
                Transform::default(),
                mask.as_ref(),
            );
        }
    }

    /// `drawText` — baseline `(X, Y)` ≙ `fl_draw` ; découpage par
    /// caractère si `letterSpacing`/`small-caps` (verbatim
    /// fltkviewbase.cc:626-675 : `letterSpacing` ajouté avant le
    /// glyph *sauf* le premier).
    fn draw_text_at(
        &mut self,
        o: (i32, i32),
        font: &Font,
        color: &Color,
        shading: Shading,
        x: i32,
        y: i32,
        text: &[u8],
    ) {
        let rgba = skia_color(color, shading);
        let size = font.attrs.size.max(1) as f32;
        let Some(platform) = self.platform.upgrade() else {
            return;
        };
        let attrs = platform.borrow().cosmic_attrs(&font.attrs);
        let base_x = self.vx(o, x);
        let base_y = self.vy(o, y);

        let split =
            font.attrs.letter_spacing != 0 || font.attrs.font_variant == FontVariant::SmallCaps;
        if !split {
            let s = String::from_utf8_lossy(text).into_owned();
            self.draw_buffer(&s, attrs, size, base_x, base_y, rgba);
            return;
        }
        let sc_size = (size * 0.78).round().max(1.0);
        let mut run_x = base_x;
        let mut cur = 0usize;
        let mut first = true;
        while cur < text.len() {
            let (ucs, len) = utf8::decode(text, cur, text.len());
            let next = cur + len.max(1);
            let ch = char::from_u32(ucs as u32);
            let up = ch.map(|c| c.to_uppercase().next().unwrap_or(c));
            let small = font.attrs.font_variant == FontVariant::SmallCaps && up != ch;
            let mut upbuf = [0u8; 4];
            let seg: &[u8] = if small {
                match up {
                    Some(cu) => cu.encode_utf8(&mut upbuf).as_bytes(),
                    None => &text[cur..next],
                }
            } else {
                &text[cur..next]
            };
            let seg_size = if small { sc_size } else { size };
            if !first {
                run_x += font.attrs.letter_spacing as f32;
            }
            first = false;
            let s = String::from_utf8_lossy(seg).into_owned();
            let w = self.draw_buffer(&s, attrs, seg_size, run_x, base_y, rgba);
            run_x += w;
            cur = next;
        }
    }

    /// `drawSimpleWrappedText` — `fl_draw(FL_ALIGN_WRAP)` ≙
    /// `Buffer` borné `W×H`, top-left.
    #[allow(clippy::too_many_arguments)]
    fn draw_wrapped_text_at(
        &mut self,
        o: (i32, i32),
        font: &Font,
        color: &Color,
        shading: Shading,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        text: &[u8],
    ) {
        let rgba = skia_color(color, shading);
        let size = font.attrs.size.max(1) as f32;
        let s = String::from_utf8_lossy(text).into_owned();
        let Some(platform) = self.platform.upgrade() else {
            return;
        };
        let attrs = platform.borrow().cosmic_attrs(&font.attrs);
        let clip = self.clip_bounds();
        let bx = self.vx(o, x) as i32;
        let by = self.vy(o, y) as i32;
        let p = platform.borrow();
        let mut fs = p.font_system.borrow_mut();
        let mut sw = p.swash.borrow_mut();
        let mut buf = cosmic_text::Buffer::new(&mut fs, cosmic_text::Metrics::new(size, size));
        buf.set_size(&mut fs, Some(w.max(1) as f32), Some(h.max(1) as f32));
        buf.set_text(&mut fs, &s, attrs, cosmic_text::Shaping::Advanced);
        buf.shape_until_scroll(&mut fs, true);
        let cc = cosmic_text::Color::rgba(
            (rgba.red() * 255.0) as u8,
            (rgba.green() * 255.0) as u8,
            (rgba.blue() * 255.0) as u8,
            255,
        );
        buf.draw(&mut fs, &mut sw, cc, |px, py, gw, gh, col| {
            let pc = col.as_rgba();
            for dy in 0..gh as i32 {
                for dx in 0..gw as i32 {
                    self.blend_pixel(px + dx + bx, py + dy + by, pc, clip);
                }
            }
        });
    }

    /// `drawImage` — ≙ `FltkImgbuf::draw` : clip `x,y,w,h` aux
    /// bornes du buffer puis blit RGB par ligne (`rgb_row`).
    #[allow(clippy::too_many_arguments)]
    fn draw_image_at(
        &mut self,
        o: (i32, i32),
        imgbuf: &Rc<dyn Imgbuf>,
        x_root: i32,
        y_root: i32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        let (bw, bh) = (imgbuf.width(), imgbuf.height());
        if x > bw || y > bh {
            return;
        }
        let w = width.min(bw - x);
        let h = height.min(bh - y);
        let clip = self.clip_bounds();
        let (pw, ph) = self.px_size();
        for row in y..y + h {
            let data = imgbuf.rgb_row(row);
            let (vx, vy) = (self.vx(o, x_root + x), self.vy(o, y_root + row));
            for col in 0..w {
                let off = (3 * (x + col)) as usize;
                if off + 2 >= data.len() {
                    break;
                }
                let (px, py) = (vx + col as f32, vy);
                if let Some((cx0, cy0, cw, ch)) = clip {
                    if px < cx0 || py < cy0 || px >= cx0 + cw || py >= cy0 + ch {
                        continue;
                    }
                }
                if px < 0.0 || py < 0.0 || px >= pw as f32 || py >= ph as f32 {
                    continue;
                }
                let idx = (py as u32 * pw + px as u32) as usize;
                if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(
                    data[off],
                    data[off + 1],
                    data[off + 2],
                    255,
                ) {
                    if let Some(pix) = self.pixmap.as_mut() {
                        if idx < pix.pixels().len() {
                            pix.pixels_mut()[idx] = p;
                        }
                    }
                }
            }
        }
    }
}

/// Corps `View` partagé — délègue à `Canvas` via
/// `self.shared_canvas() -> Option<Rc<RefCell<Canvas>>>` +
/// `self.view_origin()` (≙ `translateCanvas*ToView*`).
/// Implémenté par `SkiaView`, `ClippingView`, `SharedView`
/// (`app.rs` — proxy `Weak` du layout ≙ pointeur widget amont).
macro_rules! canvas_view_impl {
    () => {
        /// `setLayout` — l'attachement réel est côté `app.rs`.
        fn set_layout(&mut self, _layout: Option<usize>) {}

        /// `setCanvasSize` + clamp (≙ `adjustScrollbarValues`).
        fn set_canvas_size(&mut self, width: i32, ascent: i32, descent: i32) {
            let Some(c) = self.shared_canvas() else {
                return;
            };
            let mut c = c.borrow_mut();
            c.canvas_w = width;
            c.canvas_h = ascent + descent;
            let (sx, sy) = (c.scroll_x, c.scroll_y);
            c.scroll_to(sx, sy);
        }

        /// `setBgColor` ≙ `bgColor = colors[SHADING_NORMAL]`.
        fn set_bg_color(&mut self, color: &Color) {
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut().bg = color.get_color() as u32;
            }
        }

        /// `usesViewport` — `FltkViewport` → `true`.
        fn uses_viewport(&mut self) -> bool {
            true
        }

        /// `getHScrollbarThickness` — `SCROLLBAR_THICKNESS` = 15.
        fn get_h_scrollbar_thickness(&mut self) -> i32 {
            SCROLLBAR_THICKNESS
        }

        /// `getVScrollbarThickness`.
        fn get_v_scrollbar_thickness(&mut self) -> i32 {
            SCROLLBAR_THICKNESS
        }

        /// `scrollbarOnLeft` flag.
        fn get_scrollbar_on_left(&mut self) -> i32 {
            self.shared_canvas()
                .map(|c| i32::from(c.borrow().scrollbar_on_left))
                .unwrap_or(0)
        }

        /// `scrollTo` verbatim.
        fn scroll_to(&mut self, x: i32, y: i32) {
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut().scroll_to(x, y);
            }
        }

        /// `scroll(ScrollCommand)` verbatim.
        fn scroll(&mut self, cmd: ScrollCommand) {
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut().scroll_cmd(cmd);
            }
        }

        /// `setViewportSize` — montre/cache barres, origine quand
        /// elles disparaissent (verbatim fltkviewport.cc:496-509).
        fn set_viewport_size(
            &mut self,
            width: i32,
            height: i32,
            h_scrollbar_thickness: i32,
            v_scrollbar_thickness: i32,
        ) {
            let Some(c) = self.shared_canvas() else {
                return;
            };
            let mut c = c.borrow_mut();
            c.view_w = width;
            c.view_h = height;
            c.hsb_visible = h_scrollbar_thickness > 0;
            c.vsb_visible = v_scrollbar_thickness > 0;
            let (sx, sy) = (
                if h_scrollbar_thickness > 0 {
                    c.scroll_x
                } else {
                    0
                },
                if v_scrollbar_thickness > 0 {
                    c.scroll_y
                } else {
                    0
                },
            );
            c.scroll_to(sx, sy);
        }

        /// `startDrawing` — no-op verbatim (fltkviewbase.cc:426).
        fn start_drawing(&mut self, _area: &Rectangle) {}

        /// `finishDrawing` — no-op verbatim.
        fn finish_drawing(&mut self, _area: &Rectangle) {}

        /// `queueDraw` — accumule `drawRegion` (≙ `FL_DAMAGE_USER1`).
        fn queue_draw(&mut self, area: &Rectangle) {
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut().damage.push(*area);
            }
        }

        /// `queueDrawTotal` — `FL_DAMAGE_EXPOSE`.
        fn queue_draw_total(&mut self) {
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut().damage_all = true;
            }
        }

        /// `cancelQueueDraw` — verbatim no-op amont ; on vide
        /// (notre boucle est explicite — doc A10).
        fn cancel_queue_draw(&mut self) {
            if let Some(c) = self.shared_canvas() {
                let mut c = c.borrow_mut();
                c.damage.clear();
                c.damage_all = false;
            }
        }

        /// `drawPoint` — verbatim no-op amont (fltkviewbase.cc:449).
        fn draw_point(&mut self, _color: &Color, _shading: Shading, _x: i32, _y: i32) {}

        fn draw_line(
            &mut self,
            color: &Color,
            shading: Shading,
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
        ) {
            let o = self.view_origin();
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut()
                    .draw_line_at(o, color, shading, x1, y1, x2, y2);
            }
        }

        fn draw_typed_line(
            &mut self,
            color: &Color,
            shading: Shading,
            line_type: LineType,
            width: i32,
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
        ) {
            let o = self.view_origin();
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut()
                    .draw_typed_line_at(o, color, shading, line_type, width, x1, y1, x2, y2);
            }
        }

        fn draw_rectangle(
            &mut self,
            color: &Color,
            shading: Shading,
            filled: bool,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
        ) {
            let o = self.view_origin();
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut()
                    .draw_rectangle_at(o, color, shading, filled, x, y, width, height);
            }
        }

        fn draw_arc(
            &mut self,
            color: &Color,
            shading: Shading,
            filled: bool,
            center_x: i32,
            center_y: i32,
            width: i32,
            height: i32,
            angle1: i32,
            angle2: i32,
        ) {
            let o = self.view_origin();
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut().draw_arc_at(
                    o, color, shading, filled, center_x, center_y, width, height, angle1, angle2,
                );
            }
        }

        fn draw_polygon(
            &mut self,
            color: &Color,
            shading: Shading,
            filled: bool,
            convex: bool,
            points: &[Point],
        ) {
            let o = self.view_origin();
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut()
                    .draw_polygon_at(o, color, shading, filled, convex, points);
            }
        }

        fn draw_text(
            &mut self,
            font: &Font,
            color: &Color,
            shading: Shading,
            x: i32,
            y: i32,
            text: &[u8],
        ) {
            let o = self.view_origin();
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut()
                    .draw_text_at(o, font, color, shading, x, y, text);
            }
        }

        fn draw_simple_wrapped_text(
            &mut self,
            font: &Font,
            color: &Color,
            shading: Shading,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            text: &[u8],
        ) {
            let o = self.view_origin();
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut()
                    .draw_wrapped_text_at(o, font, color, shading, x, y, w, h, text);
            }
        }

        fn draw_image(
            &mut self,
            imgbuf: &Rc<dyn Imgbuf>,
            x_root: i32,
            y_root: i32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
        ) {
            let o = self.view_origin();
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut()
                    .draw_image_at(o, imgbuf, x_root, y_root, x, y, width, height);
            }
        }

        /// `getClippingView` — `fl_push_clip` ≙ push sur la pile
        /// partagée (traduite par l'origine — les clips sont en
        /// coords canvas racine) ; retourne un proxy `View` sur le
        /// même canvas (amont retournait `this`).
        fn get_clipping_view(&mut self, x: i32, y: i32, width: i32, height: i32) -> Box<dyn View> {
            let o = self.view_origin();
            let Some(c) = self.shared_canvas() else {
                return Box::new(NullView);
            };
            c.borrow_mut().clips.push((x + o.0, y + o.1, width, height));
            Box::new(ClippingView {
                canvas: c,
                origin: o,
            })
        }

        /// `mergeClippingView` — `fl_pop_clip`.
        fn merge_clipping_view(&mut self, _clipping_view: Box<dyn View>) {
            if let Some(c) = self.shared_canvas() {
                c.borrow_mut().clips.pop();
            }
        }
    };
}

/// `SkiaView` — view + viewport (≙ `FltkViewport` =
/// `FltkViewBase` + scrollbars + scroll). La logique d'events
/// de scrollbar est dans `app.rs` (winit n'a pas de widgets
/// natifs).
pub struct SkiaView {
    canvas: Rc<RefCell<Canvas>>,
}

impl SkiaView {
    /// `FltkViewBase::FltkViewBase` — back-buffer `w×h` ; la vue
    /// garde un `Weak` sur la plateforme (≙ pointeur amont).
    #[must_use]
    pub fn new(platform: &Rc<RefCell<SkiaPlatform>>, width: u32, height: u32) -> Self {
        Self {
            canvas: Rc::new(RefCell::new(Canvas::new(platform, width, height))),
        }
    }

    /// `Rc` du canvas — impl `shared_canvas` de la macro.
    /// `Option` requis par le contrat de macro (`SharedView` a
    /// un `Weak`) — toujours `Some` ici.
    #[allow(clippy::unnecessary_wraps)]
    fn shared_canvas(&self) -> Option<Rc<RefCell<Canvas>>> {
        Some(Rc::clone(&self.canvas))
    }

    /// `Rc` du canvas — pour `FlatView`/`ClippingView` (≙ partage
    /// du back-buffer amont).
    pub(crate) fn canvas_rc(&self) -> Rc<RefCell<Canvas>> {
        Rc::clone(&self.canvas)
    }

    /// Origine `(0,0)` — la vue racine parle en coords canvas.
    #[allow(clippy::unused_self)]
    fn view_origin(&self) -> (i32, i32) {
        (0, 0)
    }

    /// Redimensionne le back-buffer (`BackBuffer::setSize` :
    /// grossit seulement, contenu conservé).
    pub fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (width.max(1), height.max(1));
        let mut c = self.canvas.borrow_mut();
        c.view_w = w as i32;
        c.view_h = h as i32;
        let (pw, ph) = c.px_size();
        if w > pw || h > ph {
            if let Some(mut new) = Canvas::new_pixmap(w.max(pw), h.max(ph)) {
                if let Some(old) = c.pixmap.as_ref() {
                    for y in 0..old.height() {
                        for x in 0..old.width() {
                            if let Some(p) = old.pixel(x, y) {
                                let idx = (y * new.width() + x) as usize;
                                if idx < new.pixels().len() {
                                    new.pixels_mut()[idx] = p;
                                }
                            }
                        }
                    }
                }
                c.pixmap = Some(new);
            }
        }
    }

    /// Back-buffer (present/golden) — `None` si allocation
    /// impossible (dégradé).
    #[must_use]
    pub fn pixmap(&self) -> Option<Ref<'_, Pixmap>> {
        Ref::filter_map(self.canvas.borrow(), |c| c.pixmap.as_ref()).ok()
    }

    /// Back-buffer mutable (dessin des scrollbars par `app.rs` —
    /// ≙ widgets `Fl_Scrollbar` dessinés après le contenu).
    pub(crate) fn pixmap_mut(&mut self) -> Option<RefMut<'_, Pixmap>> {
        RefMut::filter_map(self.canvas.borrow_mut(), |c| c.pixmap.as_mut()).ok()
    }

    /// `linesize` — pas de scroll par ligne (≙ `linesize` amont
    /// utilisé par `FL_MOUSEWHEEL`).
    #[must_use]
    pub fn scroll_step(&self) -> f32 {
        self.canvas.borrow().scroll_step.max(1) as f32
    }

    /// `scroll_step` setter (≙ `linesize` recalculé par le layout).
    pub fn set_scroll_step(&mut self, step: i32) {
        self.canvas.borrow_mut().scroll_step = step;
    }

    /// Damage en attente (`drawRegion` non vide ou `damage_all`).
    #[must_use]
    pub fn has_damage(&self) -> bool {
        let c = self.canvas.borrow();
        c.damage_all || !c.damage.is_empty()
    }

    /// `take_damage` — consomme `drawRegion` (≙ `damage()` +
    /// `drawRegion.clear()`). `(full, rects)`.
    pub fn take_damage(&mut self) -> (bool, Vec<Rectangle>) {
        let mut c = self.canvas.borrow_mut();
        let v = std::mem::take(&mut c.damage);
        let all = c.damage_all;
        c.damage_all = false;
        (all, v)
    }

    /// `fill_bg` — fond d'une région canvas (≙ `fl_rectf(bg)`).
    pub fn fill_bg(&mut self, rect: &Rectangle) {
        self.canvas.borrow_mut().fill_bg((0, 0), rect);
    }

    /// `translateViewXToCanvasX` (≙ conversion event → canvas).
    #[must_use]
    pub fn to_canvas_x(&self, x: i32) -> i32 {
        x + self.canvas.borrow().scroll_x
    }

    /// `translateViewYToCanvasY`.
    #[must_use]
    pub fn to_canvas_y(&self, y: i32) -> i32 {
        y + self.canvas.borrow().scroll_y
    }

    /// Positions de scroll `(scrollX, scrollY)`.
    #[must_use]
    pub fn scroll_pos(&self) -> (i32, i32) {
        let c = self.canvas.borrow();
        (c.scroll_x, c.scroll_y)
    }

    /// `mouse_x`/`mouse_y` vus (≙ champs — `-1` hors fenêtre).
    pub fn set_mouse_pos(&mut self, x: i32, y: i32) {
        let mut c = self.canvas.borrow_mut();
        c.mouse_x = x;
        c.mouse_y = y;
    }

    /// `mouse_pos` courante.
    #[must_use]
    pub fn mouse_pos(&self) -> (i32, i32) {
        let c = self.canvas.borrow();
        (c.mouse_x, c.mouse_y)
    }

    /// `setScrollbarPageMode`.
    pub fn set_scrollbar_page_mode(&mut self, enable: bool) {
        self.canvas.borrow_mut().scrollbar_page_mode = enable;
    }

    /// `scrollbarPageMode` courant.
    #[must_use]
    pub fn scrollbar_page_mode(&self) -> bool {
        self.canvas.borrow().scrollbar_page_mode
    }

    /// `setScrollbarOnLeft` (≙ flag + réallocation amont).
    pub fn set_scrollbar_on_left(&mut self, enable: bool) {
        let mut c = self.canvas.borrow_mut();
        c.scrollbar_on_left = enable;
        c.damage_all = true;
    }

    /// `(hscrollbar, vscrollbar)` visibles (≙ `->visible()`).
    #[must_use]
    pub fn scrollbars_visible(&self) -> (bool, bool) {
        let c = self.canvas.borrow();
        (c.hsb_visible, c.vsb_visible)
    }

    /// Taille canvas `(canvasWidth, canvasHeight)`.
    #[must_use]
    pub fn canvas_size(&self) -> (i32, i32) {
        let c = self.canvas.borrow();
        (c.canvas_w, c.canvas_h)
    }

    /// Taille viewport `(w, h)` widget.
    #[must_use]
    pub fn view_size(&self) -> (i32, i32) {
        let c = self.canvas.borrow();
        (c.view_w, c.view_h)
    }

    /// Curseur demandé par le contenu (≙ `Fl::cursor`).
    #[must_use]
    pub fn cursor(&self) -> Cursor {
        self.canvas.borrow().cursor
    }

    /// `selectionScroll` verbatim (fltkviewport.cc:520) — delta de
    /// scroll vers une souris sortie de vue pendant la sélection.
    #[must_use]
    pub fn selection_scroll_delta(&self) -> (i32, i32) {
        let c = self.canvas.borrow();
        let mut dx = 0;
        let mut dy = 0;
        let mut dist = -c.mouse_x;
        if dist > 0 {
            dx = -dist * c.scroll_step / 48 - 1;
        } else {
            dist = c.mouse_x - c.view_w;
            if dist > 0 {
                dx = dist * c.scroll_step / 48 + 1;
            }
        }
        let mut dist = -c.mouse_y;
        if dist > 0 {
            dy = -dist * c.scroll_step / 48 - 1;
        } else {
            dist = c.mouse_y - c.view_h;
            if dist > 0 {
                dy = dist * c.scroll_step / 48 + 1;
            }
        }
        (dx, dy)
    }
}

/// `View` pour `SkiaView` — impl commune + `set_cursor` (curseur
/// stocké sur le Canvas — `SharedView` y accède ≙ `Fl::cursor`).
impl View for SkiaView {
    /// `setCursor` — relayé à winit par `app.rs` (≙ `Fl::cursor`).
    fn set_cursor(&mut self, cursor: Cursor) {
        self.canvas.borrow_mut().cursor = cursor;
    }

    /// `as_any` — ≙ cast `FltkView*` amont.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    canvas_view_impl!();
}

/// `ClippingView` — proxy `View` déléguant au `Canvas` partagé
/// (clip déjà poussé par `get_clipping_view` ; ≙ `this` amont).
/// `origin` = origine de la vue qui l'a créée (≙ `FlatView` —
/// les sous-clips restent dans son repère).
pub(crate) struct ClippingView {
    canvas: Rc<RefCell<Canvas>>,
    /// Origine (coords canvas racine) de la vue parente.
    origin: (i32, i32),
}

impl ClippingView {
    fn view_origin(&self) -> (i32, i32) {
        self.origin
    }

    /// `Rc` du canvas (résolution `FlatView` par downcast).
    pub(crate) fn canvas_rc(&self) -> Rc<RefCell<Canvas>> {
        Rc::clone(&self.canvas)
    }

    /// Impl `shared_canvas` de la macro — `Option` requis par le
    /// contrat (`SharedView` a un `Weak`) — toujours `Some` ici.
    #[allow(clippy::unnecessary_wraps)]
    fn shared_canvas(&self) -> Option<Rc<RefCell<Canvas>>> {
        Some(Rc::clone(&self.canvas))
    }
}

impl View for ClippingView {
    /// `setCursor` — le curseur est global au canvas (verbatim
    /// `Fl::cursor` — la vue clippée est `this` amont).
    fn set_cursor(&mut self, cursor: Cursor) {
        self.canvas.borrow_mut().cursor = cursor;
    }

    /// `as_any` — ≙ cast `FltkView*` amont.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    canvas_view_impl!();
}

/// `FlatView` (≙ `FltkFlatView`) — vue du layout interne d'un
/// `ComplexButtonResource` : écrit dans le canvas racine à
/// `origin` (coords canvas externes = position du widget +
/// relief). `usesViewport() = false` verbatim.
pub(crate) struct FlatView {
    /// Canvas racine — cellule **partagée** avec la `FlatView`
    /// attachée au layout interne et les `SkiaComplexButtonOps`
    /// (≙ `attachView` différé : la flat view peut être créée
    /// avant que la vue externe ne soit attachée ; la résolution
    /// paresseuse se fait dans `ops.draw` via `View::as_any`).
    canvas: Rc<RefCell<Option<Rc<RefCell<Canvas>>>>>,
    /// Origine en coords canvas externes (`(x+rx, y+ry)` amont) —
    /// partagée (`size_allocate` pose, draws lisent).
    origin: Rc<Cell<(i32, i32)>>,
    /// Taille viewport propre — partagée (`size_allocate` ≙
    /// `flatView->resize` amont).
    view_size: Rc<Cell<(i32, i32)>>,
    /// `canvasWidth`/`Height` propres (layout interne).
    canvas_w: Cell<i32>,
    canvas_h: Cell<i32>,
}

/// Cellules partagées d'une flat view — créées par les
/// `SkiaComplexButtonOps`, passées à chaque `FlatView::new`.
pub(crate) struct FlatViewShared {
    /// Canvas racine (`None` tant que non attachée).
    pub(crate) canvas: Rc<RefCell<Option<Rc<RefCell<Canvas>>>>>,
    /// Origine (coords canvas externes).
    pub(crate) origin: Rc<Cell<(i32, i32)>>,
    /// Taille viewport interne.
    pub(crate) view_size: Rc<Cell<(i32, i32)>>,
}

impl FlatViewShared {
    /// Nouveau trio de cellules (≙ `new FltkFlatView (0,0,1,1)`).
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            canvas: Rc::new(RefCell::new(None)),
            origin: Rc::new(Cell::new((0, 0))),
            view_size: Rc::new(Cell::new((1, 1))),
        }
    }
}

impl Default for FlatViewShared {
    fn default() -> Self {
        Self::new()
    }
}

impl FlatView {
    /// `new FltkFlatView (0,0,1,1)` — partage `shared` avec les
    /// ops et la vue attachée au layout interne.
    #[must_use]
    pub(crate) fn new(shared: &FlatViewShared) -> Self {
        Self {
            canvas: Rc::clone(&shared.canvas),
            origin: Rc::clone(&shared.origin),
            view_size: Rc::clone(&shared.view_size),
            canvas_w: Cell::new(0),
            canvas_h: Cell::new(0),
        }
    }

    /// Résolution du canvas racine (≙ `FltkFlatView` dessinant
    /// dans la fenêtre parente).
    fn canvas(&self) -> Option<Rc<RefCell<Canvas>>> {
        self.canvas.borrow().clone()
    }

    /// `Rc` du canvas partagé (résolution `FlatView` imbriquée
    /// par downcast — boutons complexes dans boutons complexes).
    pub(crate) fn shared_canvas(&self) -> Option<Rc<RefCell<Canvas>>> {
        self.canvas()
    }

    fn view_origin(&self) -> (i32, i32) {
        self.origin.get()
    }
}

/// `View` pour `FlatView` — délègue au canvas racine avec
/// l'origine ; scrollbars/scroll inopérants (verbatim :
/// `usesFltkWidgets()==false`, pas de scrollbars).
impl View for FlatView {
    /// `setLayout` — verbatim no-op (l'attachement est géré par
    /// `ComplexButtonResource::init`).
    fn set_layout(&mut self, _layout: Option<usize>) {}

    /// `setCanvasSize` — mémoire propre (pas de scroll interne).
    fn set_canvas_size(&mut self, width: i32, ascent: i32, descent: i32) {
        self.canvas_w.set(width);
        self.canvas_h.set(ascent + descent);
    }

    /// `setCursor` — relayé à la vue racine via le canvas
    /// (curseur global amont `Fl::cursor`).
    fn set_cursor(&mut self, _cursor: Cursor) {}

    /// `setBgColor` — sur le canvas racine (verbatim : la flat
    /// view partage le fond de la fenêtre).
    fn set_bg_color(&mut self, color: &Color) {
        if let Some(c) = self.canvas() {
            c.borrow_mut().bg = color.get_color() as u32;
        }
    }

    /// `usesViewport` — `false` verbatim (flat view).
    fn uses_viewport(&mut self) -> bool {
        false
    }

    /// `getHScrollbarThickness` — 0 (pas de barres internes).
    fn get_h_scrollbar_thickness(&mut self) -> i32 {
        0
    }

    /// `getVScrollbarThickness`.
    fn get_v_scrollbar_thickness(&mut self) -> i32 {
        0
    }

    /// `getScrollbarOnLeft`.
    fn get_scrollbar_on_left(&mut self) -> i32 {
        0
    }

    /// `scrollTo` — ignoré (verbatim : pas de scroll interne).
    fn scroll_to(&mut self, _x: i32, _y: i32) {}

    /// `scroll` — idem.
    fn scroll(&mut self, _cmd: ScrollCommand) {}

    /// `setViewportSize` — mémorisé dans la cellule partagée
    /// (allocation de la flat view ≙ `resize` amont).
    fn set_viewport_size(
        &mut self,
        width: i32,
        height: i32,
        _h_scrollbar_thickness: i32,
        _v_scrollbar_thickness: i32,
    ) {
        self.view_size.set((width, height));
    }

    /// `startDrawing` — no-op verbatim.
    fn start_drawing(&mut self, _area: &Rectangle) {}

    /// `finishDrawing` — no-op verbatim.
    fn finish_drawing(&mut self, _area: &Rectangle) {}

    /// `queueDraw` — région traduite en coords racine.
    fn queue_draw(&mut self, area: &Rectangle) {
        if let Some(c) = self.canvas() {
            let o = self.origin.get();
            let mut a = *area;
            a.x += o.0;
            a.y += o.1;
            c.borrow_mut().damage.push(a);
        }
    }

    /// `queueDrawTotal`.
    fn queue_draw_total(&mut self) {
        if let Some(c) = self.canvas() {
            c.borrow_mut().damage_all = true;
        }
    }

    /// `cancelQueueDraw` — verbatim no-op amont.
    fn cancel_queue_draw(&mut self) {}

    /// `drawPoint` — verbatim no-op amont.
    fn draw_point(&mut self, _color: &Color, _shading: Shading, _x: i32, _y: i32) {}

    fn draw_line(&mut self, color: &Color, shading: Shading, x1: i32, y1: i32, x2: i32, y2: i32) {
        if let Some(c) = self.canvas() {
            let o = self.view_origin();
            c.borrow_mut()
                .draw_line_at(o, color, shading, x1, y1, x2, y2);
        }
    }

    fn draw_typed_line(
        &mut self,
        color: &Color,
        shading: Shading,
        line_type: LineType,
        width: i32,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    ) {
        if let Some(c) = self.canvas() {
            let o = self.view_origin();
            c.borrow_mut()
                .draw_typed_line_at(o, color, shading, line_type, width, x1, y1, x2, y2);
        }
    }

    fn draw_rectangle(
        &mut self,
        color: &Color,
        shading: Shading,
        filled: bool,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        if let Some(c) = self.canvas() {
            let o = self.view_origin();
            c.borrow_mut()
                .draw_rectangle_at(o, color, shading, filled, x, y, width, height);
        }
    }

    fn draw_arc(
        &mut self,
        color: &Color,
        shading: Shading,
        filled: bool,
        center_x: i32,
        center_y: i32,
        width: i32,
        height: i32,
        angle1: i32,
        angle2: i32,
    ) {
        if let Some(c) = self.canvas() {
            let o = self.view_origin();
            c.borrow_mut().draw_arc_at(
                o, color, shading, filled, center_x, center_y, width, height, angle1, angle2,
            );
        }
    }

    fn draw_polygon(
        &mut self,
        color: &Color,
        shading: Shading,
        filled: bool,
        convex: bool,
        points: &[Point],
    ) {
        if let Some(c) = self.canvas() {
            let o = self.view_origin();
            c.borrow_mut()
                .draw_polygon_at(o, color, shading, filled, convex, points);
        }
    }

    fn draw_text(
        &mut self,
        font: &Font,
        color: &Color,
        shading: Shading,
        x: i32,
        y: i32,
        text: &[u8],
    ) {
        if let Some(c) = self.canvas() {
            let o = self.view_origin();
            c.borrow_mut()
                .draw_text_at(o, font, color, shading, x, y, text);
        }
    }

    fn draw_simple_wrapped_text(
        &mut self,
        font: &Font,
        color: &Color,
        shading: Shading,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        text: &[u8],
    ) {
        if let Some(c) = self.canvas() {
            let o = self.view_origin();
            c.borrow_mut()
                .draw_wrapped_text_at(o, font, color, shading, x, y, w, h, text);
        }
    }

    fn draw_image(
        &mut self,
        imgbuf: &Rc<dyn Imgbuf>,
        x_root: i32,
        y_root: i32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        if let Some(c) = self.canvas() {
            let o = self.view_origin();
            c.borrow_mut()
                .draw_image_at(o, imgbuf, x_root, y_root, x, y, width, height);
        }
    }

    /// `getClippingView` — clip traduit en coords racine.
    fn get_clipping_view(&mut self, x: i32, y: i32, width: i32, height: i32) -> Box<dyn View> {
        let o = self.view_origin();
        if let Some(c) = self.canvas() {
            c.borrow_mut().clips.push((x + o.0, y + o.1, width, height));
            return Box::new(ClippingView {
                canvas: c,
                origin: o,
            });
        }
        // Pas de canvas (pas encore attaché) — proxy muet.
        Box::new(NullView)
    }

    /// `mergeClippingView` — `fl_pop_clip`.
    fn merge_clipping_view(&mut self, _clipping_view: Box<dyn View>) {
        if let Some(c) = self.canvas() {
            c.borrow_mut().clips.pop();
        }
    }
}

/// `SharedView` — proxy `View` non-possédant sur le `Canvas`
/// (≙ pointeur widget amont : le layout tient la vue sans la
/// posséder). `Weak` mort ≙ widget détruit → no-op.
///
/// Proxie le `Canvas` et non la `SkiaView` : le layout rappelle
/// `view.*` *pendant* `Layout::draw` qui tient `RefMut<SkiaView>`
/// → proxier la vue paniquerait (`RefCell` emprunté) ; le canvas
/// n'est jamais gardé emprunté entre deux appels `View`.
pub struct SharedView {
    /// Canvas cible (`Weak` ≙ widget non possédé).
    canvas: Weak<RefCell<Canvas>>,
}

impl SharedView {
    /// `SharedView` vers le canvas de `view` (≙ le widget amont).
    #[must_use]
    pub fn new(view: &Rc<RefCell<SkiaView>>) -> Self {
        Self {
            canvas: Rc::downgrade(&view.borrow().canvas_rc()),
        }
    }

    /// Impl `shared_canvas` de la macro — `upgrade` du `Weak`.
    fn shared_canvas(&self) -> Option<Rc<RefCell<Canvas>>> {
        self.canvas.upgrade()
    }

    /// Origine `(0,0)` — même repère canvas que la vue racine.
    #[allow(clippy::unused_self)]
    fn view_origin(&self) -> (i32, i32) {
        (0, 0)
    }
}

impl View for SharedView {
    /// `setCursor` — sur le canvas (≙ `Fl::cursor` global).
    fn set_cursor(&mut self, cursor: Cursor) {
        if let Some(c) = self.shared_canvas() {
            c.borrow_mut().cursor = cursor;
        }
    }

    canvas_view_impl!();
}

/// `NullView` — `View` muet (flat view sans canvas — verbatim :
/// amont les draws iraient dans un widget non affiché).
struct NullView;

impl View for NullView {
    fn set_layout(&mut self, _layout: Option<usize>) {}
    fn set_canvas_size(&mut self, _width: i32, _ascent: i32, _descent: i32) {}
    fn set_cursor(&mut self, _cursor: Cursor) {}
    fn set_bg_color(&mut self, _color: &Color) {}
    fn uses_viewport(&mut self) -> bool {
        false
    }
    fn get_h_scrollbar_thickness(&mut self) -> i32 {
        0
    }
    fn get_v_scrollbar_thickness(&mut self) -> i32 {
        0
    }
    fn get_scrollbar_on_left(&mut self) -> i32 {
        0
    }
    fn scroll_to(&mut self, _x: i32, _y: i32) {}
    fn set_viewport_size(&mut self, _w: i32, _h: i32, _ht: i32, _vt: i32) {}
    fn start_drawing(&mut self, _area: &Rectangle) {}
    fn finish_drawing(&mut self, _area: &Rectangle) {}
    fn queue_draw(&mut self, _area: &Rectangle) {}
    fn queue_draw_total(&mut self) {}
    fn cancel_queue_draw(&mut self) {}
    fn draw_point(&mut self, _color: &Color, _shading: Shading, _x: i32, _y: i32) {}
    fn draw_line(
        &mut self,
        _color: &Color,
        _shading: Shading,
        _x1: i32,
        _y1: i32,
        _x2: i32,
        _y2: i32,
    ) {
    }
    fn draw_typed_line(
        &mut self,
        _color: &Color,
        _shading: Shading,
        _line_type: LineType,
        _width: i32,
        _x1: i32,
        _y1: i32,
        _x2: i32,
        _y2: i32,
    ) {
    }
    fn draw_rectangle(
        &mut self,
        _color: &Color,
        _shading: Shading,
        _filled: bool,
        _x: i32,
        _y: i32,
        _width: i32,
        _height: i32,
    ) {
    }
    fn draw_arc(
        &mut self,
        _color: &Color,
        _shading: Shading,
        _filled: bool,
        _center_x: i32,
        _center_y: i32,
        _width: i32,
        _height: i32,
        _angle1: i32,
        _angle2: i32,
    ) {
    }
    fn draw_polygon(
        &mut self,
        _color: &Color,
        _shading: Shading,
        _filled: bool,
        _convex: bool,
        _points: &[Point],
    ) {
    }
    fn draw_text(
        &mut self,
        _font: &Font,
        _color: &Color,
        _shading: Shading,
        _x: i32,
        _y: i32,
        _text: &[u8],
    ) {
    }
    fn draw_simple_wrapped_text(
        &mut self,
        _font: &Font,
        _color: &Color,
        _shading: Shading,
        _x: i32,
        _y: i32,
        _w: i32,
        _h: i32,
        _text: &[u8],
    ) {
    }
    fn draw_image(
        &mut self,
        _imgbuf: &Rc<dyn Imgbuf>,
        _x_root: i32,
        _y_root: i32,
        _x: i32,
        _y: i32,
        _width: i32,
        _height: i32,
    ) {
    }
    fn get_clipping_view(&mut self, _x: i32, _y: i32, _width: i32, _height: i32) -> Box<dyn View> {
        Box::new(NullView)
    }
    fn merge_clipping_view(&mut self, _clipping_view: Box<dyn View>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::SkiaPlatform;

    fn mk_view(w: u32, h: u32) -> (Rc<RefCell<SkiaPlatform>>, SkiaView) {
        let p = Rc::new(RefCell::new(SkiaPlatform::new()));
        let v = SkiaView::new(&p, w, h);
        (p, v)
    }

    fn px_at(v: &SkiaView, x: usize, y: usize) -> (u8, u8, u8, u8) {
        let px = v.pixmap();
        let Some(px) = px else {
            return (0, 0, 0, 0);
        };
        let w = px.width() as usize;
        let i = (y * w + x) * 4;
        let d = px.data();
        (d[i], d[i + 1], d[i + 2], d[i + 3])
    }

    /// `scrollTo` clamp ≙ `adjustScrollbarValues`.
    #[test]
    fn scroll_to_clamps_to_canvas() {
        let (_p, mut v) = mk_view(100, 100);
        v.set_viewport_size(100, 100, 0, 0);
        v.set_canvas_size(100, 500, 0);
        v.scroll_to(0, 450);
        assert_eq!(v.scroll_pos(), (0, 400));
        v.scroll_to(-10, -5);
        assert_eq!(v.scroll_pos(), (0, 0));
    }

    /// `translateViewToCanvas*` ≙ scroll offset ajouté.
    #[test]
    fn to_canvas_translates_scroll() {
        let (_p, mut v) = mk_view(100, 100);
        v.set_viewport_size(100, 100, 0, 0);
        v.set_canvas_size(100, 500, 0);
        v.scroll_to(0, 100);
        assert_eq!(v.to_canvas_y(5), 105);
        assert_eq!(v.to_canvas_x(5), 5);
    }

    /// `queueDraw`/`queueDrawTotal` ≙ accumulateur `drawRegion`.
    #[test]
    fn damage_accumulates_and_drains() {
        let (_p, mut v) = mk_view(100, 100);
        // Le canvas naît en `damage_all` (premier expose ≙ FLTK).
        let (full, _) = v.take_damage();
        assert!(full);
        v.queue_draw(&Rectangle {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        });
        v.queue_draw(&Rectangle {
            x: 5,
            y: 5,
            width: 5,
            height: 5,
        });
        let (full, rects) = v.take_damage();
        assert!(!full);
        assert_eq!(rects.len(), 2);
        assert!(!v.has_damage());
        v.queue_draw_total();
        let (full, _) = v.take_damage();
        assert!(full);
    }

    /// `drawRectangle(filled)` écrit réellement dans le back-buffer.
    #[test]
    fn draw_rectangle_fills_pixels() {
        let (_p, mut v) = mk_view(40, 40);
        v.draw_rectangle(&Color::new(0xff_0000), Shading::Normal, true, 5, 5, 10, 10);
        let inside = px_at(&v, 7, 7);
        assert!(
            inside.0 > 200 && inside.1 < 60,
            "pixel rouge attendu: {inside:?}"
        );
        assert_eq!(px_at(&v, 2, 2), (0, 0, 0, 0), "hors rect non touché");
    }

    /// `getClippingView` ≙ `fl_push_clip` — le proxy ne dessine
    /// pas hors clip.
    #[test]
    fn clipping_view_limits_drawing() {
        let (_p, mut v) = mk_view(40, 40);
        let mut clip = v.get_clipping_view(0, 0, 10, 40);
        clip.draw_rectangle(&Color::new(0xff_0000), Shading::Normal, true, 20, 5, 10, 10);
        assert_eq!(px_at(&v, 25, 10), (0, 0, 0, 0), "hors clip non dessiné");
        clip.draw_rectangle(&Color::new(0x00_ff00), Shading::Normal, true, 2, 2, 5, 5);
        v.merge_clipping_view(clip);
        assert!(px_at(&v, 4, 4).1 > 200, "dans le clip dessiné");
    }

    /// `SharedView` (vue du layout ≙ pointeur widget amont)
    /// proxie le canvas sans posséder la vue.
    #[test]
    fn shared_view_proxies_canvas() {
        let p = Rc::new(RefCell::new(SkiaPlatform::new()));
        let view = Rc::new(RefCell::new(SkiaView::new(&p, 100, 100)));
        let mut sv = SharedView::new(&view);
        sv.set_viewport_size(100, 100, 0, 0);
        sv.set_canvas_size(100, 500, 0);
        sv.scroll_to(0, 40);
        assert_eq!(view.borrow().scroll_pos(), (0, 40));
        sv.set_cursor(Cursor::Pointer);
        assert_eq!(view.borrow().cursor(), Cursor::Pointer);
        // `Weak` mort ≙ widget détruit → no-op (pas de panic).
        drop(view);
        sv.scroll_to(0, 0);
        sv.queue_draw_total();
    }
}
