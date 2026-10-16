//! dillo — façade + binaire du navigateur.
//!
//! Port de `src/dillo.cc`. **Phase 0.5** : tranche verticale pilote
//! — fenêtre `winit`, présentation `softbuffer`, raster `tiny-skia`,
//! texte `cosmic-text`, parseur `minidoc`, `file://` via
//! `dillo-net::fetch`. Layout minimal (blocs empilés + wrap mot) —
//! sera remplacé par `dw/` en phase 4 (ADR-0005 option D).
//!
//! Idle/boucle : `ControlFlow::Wait` (équivalent calme de
//! `Fl::add_idle` — ADR-0003 §6) + `request_redraw` ciblé ; pas de
//! spin CPU une fois la page posée.

// Pilote 0.5 : les casts de géométrie écran (px bornés à la taille
// fenêtre) et les noms courts de coordonnées sont tolérés ici ; le
// vrai pipeline dw/platform utilisera des types stricts.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)]

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use dillo_html::minidoc::{parse_minidoc, Block, Inline};
use dillo_net::fetch::{fetch, resolve_href};
use dillo_platform::surface::PlatformSurface;
use std::sync::Arc;
use tiny_skia::{Pixmap, PremultipliedColorU8};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const DEMO: &str = concat!(
    "<h1>DilloRust &mdash; tranche verticale</h1>",
    "<p>Phase 0.5 : winit + softbuffer + tiny-skia + cosmic-text. ",
    "Un <a href=\"page2.html\">lien test</a> pour la navigation.</p>",
    "<p>Second paragraphe, assez long pour forcer le wrap de mots ",
    "quand la fenêtre est étroite — le layout est recalculé au ",
    "redimensionnement.</p>"
);

/// Hitbox d'un lien à l'écran (coordonnées page, scroll soustrait
/// au hit-test).
#[derive(Debug)]
struct LinkBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    href: String,
}

struct App {
    window: Option<Arc<Window>>,
    surface: Option<PlatformSurface>,
    font_system: FontSystem,
    swash: SwashCache,
    blocks: Vec<Block>,
    base: String,
    links: Vec<LinkBox>,
    cursor: (f64, f64),
    scroll_y: f32,
    page_height: f32,
    status: String,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            surface: None,
            font_system: FontSystem::new(),
            swash: SwashCache::new(),
            blocks: parse_minidoc(DEMO),
            base: String::new(),
            links: Vec::new(),
            cursor: (0.0, 0.0),
            scroll_y: 0.0,
            page_height: 0.0,
            status: "about:demo".to_owned(),
        }
    }

    fn load(&mut self, url: &str) {
        match fetch(url) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                self.blocks = parse_minidoc(&text);
                self.base.clone_from(&url.to_owned());
                self.scroll_y = 0.0;
                self.status = url.to_string();
            }
            Err(e) => {
                self.status = format!("ERR {url}: {e}");
            }
        }
        if let Some(w) = &self.window {
            w.set_title(&format!("dillo — {}", self.status));
            w.request_redraw();
        }
    }

    /// Rendu bloc-par-bloc dans la `Pixmap` : wraps via cosmic-text
    /// (`Wrap::Word`), collecte les hitboxes de liens par `metadata`.
    fn render(&mut self, width: u32, height: u32) -> Pixmap {
        let mut pm = Pixmap::new(width, height)
            .unwrap_or_else(|| Pixmap::new(1, 1).unwrap_or_else(|| unreachable!()));
        pm.fill(tiny_skia::Color::WHITE);
        self.links.clear();
        let mut y = 16.0f32 - self.scroll_y;
        for block in &self.blocks {
            let (runs, size) = match block {
                Block::Heading { level, runs } => (runs, 30.0 - f32::from(*level) * 2.0),
                Block::Paragraph { runs } => (runs, 16.0),
            };
            let metrics = Metrics::new(size, size * 1.25);
            let mut buf = Buffer::new(&mut self.font_system, metrics);
            buf.set_size(
                &mut self.font_system,
                Some((width as f32 - 32.0).max(8.0)),
                None,
            );
            buf.set_wrap(&mut self.font_system, Wrap::Word);
            let plain = Attrs::new().family(Family::SansSerif);
            let spans: Vec<(String, Attrs)> = runs
                .iter()
                .enumerate()
                .map(|(i, r)| match r {
                    Inline::Text(t) => (t.clone(), plain),
                    Inline::Link { text, .. } => (
                        text.clone(),
                        plain.color(Color::rgb(0x10, 0x40, 0xd0)).metadata(i),
                    ),
                })
                .collect();
            buf.set_rich_text(
                &mut self.font_system,
                spans.iter().map(|(s, a)| (s.as_str(), *a)),
                plain,
                Shaping::Advanced,
            );
            buf.shape_until_scroll(&mut self.font_system, false);

            // Hitboxes par metadata (= index du run Inline)
            for run in buf.layout_runs() {
                for g in run.glyphs {
                    if let Inline::Link { href, .. } = &runs[g.metadata] {
                        self.links.push(LinkBox {
                            x: 16.0 + g.x,
                            y: y + run.line_y,
                            w: g.w,
                            h: run.line_height,
                            href: href.clone(),
                        });
                    }
                }
            }
            let block_h = buf
                .layout_runs()
                .last()
                .map_or(0.0, |r| r.line_y + r.line_height);
            buf.draw(
                &mut self.font_system,
                &mut self.swash,
                Color::rgb(0x20, 0x20, 0x20),
                |x, yy, w, h, c| {
                    blend(&mut pm, x + 16, yy + y as i32, w, h, c);
                },
            );
            // souligné des liens (1px sous chaque glyph box)
            for lb in self.links.iter().rev() {
                if lb.y + lb.h < y || lb.y > y + block_h {
                    continue;
                }
                draw_line(
                    &mut pm,
                    lb.x,
                    lb.y + lb.h - 2.0,
                    lb.w,
                    Color::rgb(0x10, 0x40, 0xd0),
                );
            }
            y += block_h + size * 0.5;
        }
        self.page_height = y + self.scroll_y;
        pm
    }

    fn hit(&self) -> Option<String> {
        let (cx, cy) = (self.cursor.0 as f32, self.cursor.1 as f32);
        self.links
            .iter()
            .find(|lb| cx >= lb.x && cx <= lb.x + lb.w && cy >= lb.y && cy <= lb.y + lb.h)
            .map(|lb| lb.href.clone())
    }
}

fn blend(pm: &mut Pixmap, x: i32, y: i32, w: u32, h: u32, c: Color) {
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    for dy in 0..h as i32 {
        for dx in 0..w as i32 {
            let (px, py) = (x + dx, y + dy);
            if px < 0 || py < 0 || px >= pw || py >= ph {
                continue;
            }
            let idx = (py * pw + px) as usize;
            let a = u16::from(c.a());
            let inv = 255 - a;
            let Some(d) = pm.pixels().get(idx).copied() else {
                continue;
            };
            let mix =
                |s: u8, d: u8| -> u8 { ((u16::from(s) * a + u16::from(d) * inv) / 255) as u8 };
            if let Some(p) = PremultipliedColorU8::from_rgba(
                mix(c.r(), d.red()),
                mix(c.g(), d.green()),
                mix(c.b(), d.blue()),
                255,
            ) {
                pm.pixels_mut()[idx] = p;
            }
        }
    }
}

fn draw_line(pm: &mut Pixmap, x: f32, y: f32, w: f32, c: Color) {
    blend(
        pm,
        x as i32,
        y as i32,
        w.max(1.0) as u32,
        1,
        Color::rgba(c.r(), c.g(), c.b(), 200),
    );
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("dillo — tranche verticale")
            .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0));
        let Ok(window) = event_loop.create_window(attrs) else {
            event_loop.exit();
            return;
        };
        let window = Arc::new(window);
        let Ok(surface) = PlatformSurface::new(Arc::clone(&window)) else {
            event_loop.exit();
            return;
        };
        self.surface = Some(surface);
        self.window = Some(window);
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(href) = self.hit() {
                    let url = resolve_href(&self.base, &href);
                    self.load(&url);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 30.0,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                self.scroll_y = (self.scroll_y - dy).clamp(0.0, self.page_height.max(0.0));
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let Some((w, h)) = self.surface.as_ref().map(PlatformSurface::size) else {
                    return;
                };
                if w == 0 || h == 0 {
                    return;
                }
                let pm = self.render(w, h);
                if let Some(surface) = &mut self.surface {
                    let _ = surface.present(&pm);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Fl::add_idle équivalent : rien à faire → Wait (pas de
        // spin). Le layout progressif en tranches viendra avec dw/.
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap_or_else(|e| {
        eprintln!("event loop: {e}");
        std::process::exit(1);
    });
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new();
    if let Some(arg) = std::env::args().nth(1) {
        app.base.clone_from(&arg);
        app.load(&arg);
    }
    let _ = event_loop.run_app(&mut app);
}
