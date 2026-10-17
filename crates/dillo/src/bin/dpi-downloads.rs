//! Port of `dpi/downloads.cc` — le plugin `downloads.dpi`.
//!
//! La fenêtre FLTK amont est remplacée par une fenêtre native
//! `winit` + `softbuffer` + `tiny-skia` (ADR-0002), avec la même
//! géométrie : fenêtre 420×85 « Dillo Downloads », items empilés
//! (nouveaux en haut, verbatim `mPG->insert(.., 0)`), chaque item
//! 400×70 : titre ombré, barre bleue, colonnes Got/Size/Rate/~Rate/
//! ETA, bouton Stop/Done/Close.
//!
//! Protocole verbatim : stdin = socket d'écoute héritée de dpid ;
//! chaque connexion → auth + tag `{cmd=download,url,destination,
//! user-agent}` → `DlItem` + `wget` fils ; tick 1 s ≙ `update_cb`
//! /`cleanup_cb` ; sortie quand la liste se vide après usage.
//!
//! POSIX : le serveur stdin-socket est `cfg(unix)` comme amont
//! (`fork`/`SIGCHLD`/`accept` sur fd 0) ; hors Unix la fenêtre
//! s'ouvre mais aucune requête n'arrive.

#![forbid(unsafe_code)]
// Lint verbatim : coords/géométrie C amont en `i32`/`f64` mixtes.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::too_many_arguments)]

use std::sync::mpsc::Receiver;

use dillo_dpi::dlitem::DlItem;
use dillo_platform::cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache};
use dillo_platform::surface::PlatformSurface;
use dillo_platform::tiny_skia::{Paint, PathBuilder, Pixmap, Rect, Transform};
use dillo_platform::winit::application::ApplicationHandler;
use dillo_platform::winit::event::{ElementState, MouseButton, WindowEvent};
use dillo_platform::winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use dillo_platform::winit::window::{WindowAttributes, WindowId};

/// Requête parsée du tag dpip (`read_req_cb` verbatim).
type Request = (String, String, String);

// ── géométrie verbatim (Fl_Group 400×70) ──────────────────────────
const ROW_W: i32 = 400;
const ROW_H: i32 = 70;
const TITLE: (i32, i32, i32, i32) = (24, 7, 290, 23);
const BAR: (i32, i32, i32, i32) = (24, 40, 92, 20);
const COL_X0: i32 = 122;
const COL_W: i32 = 50;
const HEAD_Y: i32 = 37;
const VAL_Y: i32 = 51;
const BTN: (i32, i32, i32, i32) = (326, 9, 44, 19);
const WIN_W: u32 = 420;
const WIN_H: u32 = 85;

fn fill_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, rgba: (u8, u8, u8, u8)) {
    let Some(r) = Rect::from_xywh(x, y, w.max(0.0), h.max(0.0)) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(rgba.0, rgba.1, rgba.2, rgba.3);
    pm.fill_rect(r, &paint, Transform::default(), None);
}

/// Bordure rect (≙ `FL_THIN_UP_BOX`/`FL_RSHADOW_BOX` simplifiée).
fn border_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32) {
    let Some(r) = Rect::from_xywh(x, y, w, h) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(90, 90, 90, 255);
    paint.anti_alias = false;
    let mut pb = PathBuilder::new();
    pb.push_rect(r);
    let Some(path) = pb.finish() else { return };
    pm.stroke_path(
        &path,
        &paint,
        &dillo_platform::tiny_skia::Stroke {
            width: 1.0,
            ..Default::default()
        },
        Transform::default(),
        None,
    );
}

/// Texte cosmic-text (≙ `fl_draw` labelsize 12).
fn text(
    pm: &mut Pixmap,
    fs: &mut FontSystem,
    sw: &mut SwashCache,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    s: &str,
    rgba: (u8, u8, u8, u8),
    left: bool,
) {
    use dillo_platform::cosmic_text::Color as CColor;
    let mut buf = Buffer::new(fs, Metrics::new(12.0, 14.0));
    buf.set_size(fs, Some(w.max(1) as f32), Some(h.max(1) as f32));
    buf.set_text(fs, s, Attrs::new(), Shaping::Advanced);
    buf.shape_until_scroll(fs, true);
    let tw = buf.layout_runs().map(|r| r.line_w).fold(0.0f32, f32::max);
    let ox = if left {
        3.0
    } else {
        ((w as f32 - tw) / 2.0).max(0.0)
    };
    let cc = CColor::rgba(rgba.0, rgba.1, rgba.2, rgba.3);
    let clip = (x, y, x + w, y + h);
    buf.draw(fs, sw, cc, |px_, py_, gw, gh, col| {
        let pc = col.as_rgba();
        for dy in 0..gh {
            for dx in 0..gw {
                let (tx, ty) = (
                    x + (ox + px_ as f32) as i32 + dx as i32,
                    y + py_ + dy as i32,
                );
                if tx < clip.0
                    || ty < clip.1
                    || tx >= clip.2
                    || ty >= clip.3
                    || tx < 0
                    || ty < 0
                    || tx >= pm.width() as i32
                    || ty >= pm.height() as i32
                {
                    continue;
                }
                let i = ty as usize * pm.width() as usize + tx as usize;
                if let Some(p) = pm.pixels_mut().get_mut(i) {
                    let a = u16::from(pc[3]);
                    *p = tiny_premul(pc[2], pc[1], pc[0], a.min(255) as u8);
                }
            }
        }
    });
}

fn tiny_premul(r: u8, g: u8, b: u8, a: u8) -> dillo_platform::tiny_skia::PremultipliedColorU8 {
    dillo_platform::tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, a)
        .unwrap_or(dillo_platform::tiny_skia::PremultipliedColorU8::TRANSPARENT)
}

// ── listener dpip (read_req_cb verbatim) ──────────────────────────

#[cfg(unix)]
fn serve_conn(sock: std::net::TcpStream, tx: &std::sync::mpsc::Sender<Request>) {
    use dillo_dpi::dpip::{check_auth, Dsh};
    use dillo_net::dpip::get_attr;
    let _ = sock.set_nonblocking(false);
    let Some(mut sh) = Dsh::tcp(sock) else { return };
    // verbatim : auth puis tag de requête.
    let Some(auth) = sh.read_token(true) else {
        return;
    };
    if !check_auth(&String::from_utf8_lossy(&auth)) {
        return;
    }
    let Some(tag) = sh.read_token(true) else {
        return;
    };
    sh.close();
    let tag = String::from_utf8_lossy(&tag).into_owned();
    let get = |k: &str| get_attr(&tag, k);
    let Some(cmd) = get("cmd") else { return };
    // verbatim : DpiBye ignoré, cmd != « download » → fin.
    if cmd == "DpiBye" || cmd != "download" {
        return;
    }
    let (Some(url), Some(dst), Some(ua)) = (get("url"), get("destination"), get("user-agent"))
    else {
        return;
    };
    let _ = tx.send((dst, url, ua));
}

/// Boucle `accept` sur stdin (socket d'écoute verbatim).
#[cfg(unix)]
fn listener(tx: std::sync::mpsc::Sender<Request>) {
    let stdin_ref = socket2::SockRef::from(&std::io::stdin());
    loop {
        match stdin_ref.accept() {
            Ok((sock, _)) => {
                let std_sock: std::net::TcpStream = sock.into();
                let tx2 = tx.clone();
                std::thread::spawn(move || serve_conn(std_sock, &tx2));
            }
            Err(_) => break,
        }
    }
}

// ── fenêtre (DLWin) ────────────────────────────────────────────────

struct DlWin {
    surf: Option<PlatformSurface>,
    fs: FontSystem,
    sw: SwashCache,
    items: Vec<DlItem>,
    rx: Receiver<Request>,
    scroll: i32,
    mouse: (i32, i32),
    confirm_quit: bool,
    used: bool,
    home: String,
    next_tick: std::time::Instant,
    exit: bool,
}

impl DlWin {
    /// `update_cb` + `cleanup_cb` verbatim (tick 1 s).
    fn tick(&mut self) {
        // ≙ read_req_cb : draine les requêtes acceptées.
        while let Ok((dst, url, ua)) = self.rx.try_recv() {
            let mut it = DlItem::new(&dst, &url, &ua, &self.home);
            if it.spawn().is_err() {
                eprintln!("[downloads dpi]: wget spawn failed");
                it.abort();
                it.wget_status = -1;
                it.child_finished(-1);
                it.fork_done = true;
            }
            // verbatim `mPG->insert(.., 0)` — nouveau en haut.
            self.items.insert(0, it);
        }
        // ≙ read_log_cb + waitpid.
        for it in &mut self.items {
            it.drain_log();
            it.poll_child();
        }
        // verbatim update_cb.
        let mut i = 0;
        while i < self.items.len() {
            if !self.items[i].widget_done {
                self.items[i].update();
            } else if self.items[i].fork_done {
                self.items.remove(i);
                continue;
            }
            self.used = true;
            i += 1;
        }
        if self.used && self.items.is_empty() {
            self.exit = true;
        }
    }

    /// `dlwin_esc_cb` verbatim.
    fn close_request(&mut self) {
        if self.items.iter().any(|it| !it.fork_done) {
            self.confirm_quit = true;
        } else {
            self.items.iter_mut().for_each(DlItem::abort);
        }
    }

    /// Dessine un item (coords verbatim dans le group 400×70).
    fn draw_item(pm: &mut Pixmap, fs: &mut FontSystem, sw: &mut SwashCache, it: &DlItem, oy: i32) {
        let dark = (224, 222, 216, 255);
        let blue = (0, 0, 200, 255);
        let red = (255, 0, 0, 255);
        let black = (0, 0, 0, 255);
        // group FL_ROUNDED_BOX.
        border_rect(pm, 0.0, oy as f32, ROW_W as f32, ROW_H as f32 - 1.0);
        // prTitle FL_RSHADOW_BOX blanc.
        fill_rect(
            pm,
            TITLE.0 as f32,
            oy as f32 + TITLE.1 as f32,
            TITLE.2 as f32,
            TITLE.3 as f32,
            (255, 255, 255, 255),
        );
        border_rect(
            pm,
            TITLE.0 as f32,
            oy as f32 + TITLE.1 as f32,
            TITLE.2 as f32,
            TITLE.3 as f32,
        );
        text(
            pm,
            fs,
            sw,
            TITLE.0,
            oy + TITLE.1,
            TITLE.2,
            TITLE.3,
            &it.title,
            black,
            true,
        );
        // prBar : thin-up + remplissage bleu + texte rouge.
        let (bx, by, bw, bh) = BAR;
        fill_rect(
            pm,
            bx as f32,
            oy as f32 + by as f32,
            bw as f32,
            bh as f32,
            (255, 255, 255, 255),
        );
        let pct = it.bar.0.clamp(0.0, 100.0) / 100.0;
        fill_rect(
            pm,
            bx as f32,
            oy as f32 + by as f32,
            (bw as f64 * pct + 0.5) as f32,
            bh as f32,
            blue,
        );
        border_rect(pm, bx as f32, oy as f32 + by as f32, bw as f32, bh as f32);
        let label = it
            .bar
            .1
            .clone()
            .unwrap_or_else(|| format!("{}%", (pct * 100.0 + 0.5) as i32));
        // verbatim : % seulement si taille connue / msg.
        if it.total_bytesize != -1 || it.bar.1.is_some() {
            text(pm, fs, sw, bx, oy + by, bw, bh, &label, red, false);
        }
        // Colonnes Got/Size/Rate/~Rate/ETA (verbatim ix=122, iw=50).
        let headers = ["Got", "Size", "Rate", "~Rate", &it.eta_title];
        let values = [&it.got, &it.size, &it.rate, &it.avg_rate, &it.eta];
        for (c, (h, v)) in headers.iter().zip(values.iter()).enumerate() {
            let x = COL_X0 + c as i32 * COL_W;
            fill_rect(
                pm,
                x as f32,
                oy as f32 + HEAD_Y as f32,
                COL_W as f32,
                14.0,
                dark,
            );
            text(pm, fs, sw, x, oy + HEAD_Y, COL_W, 14, h, black, false);
            text(pm, fs, sw, x, oy + VAL_Y, COL_W, 14, v, blue, false);
        }
        // prButton (Stop/Done/Close).
        let (x, y, w, h) = BTN;
        let fg = if it.button.1 {
            black
        } else {
            (150, 150, 150, 255)
        };
        fill_rect(pm, x as f32, oy as f32 + y as f32, w as f32, h as f32, dark);
        border_rect(pm, x as f32, oy as f32 + y as f32, w as f32, h as f32);
        text(pm, fs, sw, x, oy + y, w, h, &it.button.0, fg, false);
    }

    fn draw(&mut self) {
        let Some(surf) = self.surf.as_mut() else {
            return;
        };
        let (w, h) = surf.size();
        if w == 0 || h == 0 {
            return;
        }
        let Some(mut pm) = Pixmap::new(w, h) else {
            return;
        };
        let fs = &mut self.fs;
        let sw = &mut self.sw;
        // Fond FLTK-ish.
        fill_rect(&mut pm, 0.0, 0.0, w as f32, h as f32, (197, 194, 197, 255));
        for (i, it) in self.items.iter().enumerate() {
            let oy = i as i32 * ROW_H - self.scroll;
            if oy + ROW_H < 0 || oy > h as i32 {
                continue;
            }
            Self::draw_item(&mut pm, fs, sw, it, oy);
        }
        // Tooltip ≙ `prTitle->tooltip(log_text)` : affiche le log au
        // survol du titre.
        let (mx, my) = self.mouse;
        for (i, it) in self.items.iter().enumerate() {
            let oy = i as i32 * ROW_H - self.scroll;
            if mx >= TITLE.0
                && mx < TITLE.0 + TITLE.2
                && my >= oy + TITLE.1
                && my < oy + TITLE.1 + TITLE.3
            {
                let mut ty = (my + 12).min(h as i32 - 14 * 6);
                for line in it
                    .log_text
                    .lines()
                    .rev()
                    .take(6)
                    .collect::<Vec<_>>()
                    .iter()
                    .rev()
                {
                    fill_rect(
                        &mut pm,
                        4.0,
                        ty as f32 - 2.0,
                        w as f32 - 8.0,
                        14.0,
                        (255, 255, 210, 230),
                    );
                    text(
                        &mut pm,
                        fs,
                        sw,
                        6,
                        ty,
                        w as i32 - 12,
                        14,
                        line,
                        (0, 0, 0, 255),
                        true,
                    );
                    ty += 14;
                }
                break;
            }
        }
        // Confirmation verbatim `dlwin_esc_cb`.
        if self.confirm_quit {
            fill_rect(&mut pm, 0.0, 0.0, w as f32, h as f32, (197, 194, 197, 255));
            let msg = "There are running downloads. ABORT them and EXIT anyway?";
            text(
                &mut pm,
                fs,
                sw,
                8,
                12,
                w as i32 - 16,
                30,
                msg,
                (0, 0, 0, 255),
                true,
            );
            for (i, label) in ["Cancel", "No", "Yes"].iter().enumerate() {
                let x = 40 + i as i32 * 120;
                fill_rect(&mut pm, x as f32, 48.0, 90.0, 22.0, (224, 222, 216, 255));
                border_rect(&mut pm, x as f32, 48.0, 90.0, 22.0);
                text(&mut pm, fs, sw, x, 48, 90, 22, label, (0, 0, 0, 255), false);
            }
        }
        let _ = surf.present(&pm);
    }

    /// Hit-test boutons (≙ `prButton_cb` / `fl_choice`).
    fn click(&mut self, x: i32, y: i32) {
        if self.confirm_quit {
            // Cancel / No / Yes à (40/160/280, 48, 90×22).
            for i in 0..3 {
                let bx = 40 + i * 120;
                if x >= bx && x < bx + 90 && y >= 48 && y < 70 {
                    match i {
                        2 => {
                            self.confirm_quit = false;
                            self.items.iter_mut().for_each(DlItem::abort);
                        }
                        _ => self.confirm_quit = false,
                    }
                    break;
                }
            }
            return;
        }
        for (i, it) in self.items.iter_mut().enumerate() {
            let oy = i as i32 * ROW_H - self.scroll;
            if x >= BTN.0
                && x < BTN.0 + BTN.2
                && y >= oy + BTN.1
                && y < oy + BTN.1 + BTN.3
                && it.button.1
            {
                // verbatim prButton_cb : deactivate + abort_dl.
                it.button.1 = false;
                it.abort();
                break;
            }
        }
    }
}

impl ApplicationHandler for DlWin {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.surf.is_none() {
            let attrs = WindowAttributes::default()
                .with_title("Dillo Downloads")
                .with_inner_size(dillo_platform::winit::dpi::LogicalSize::new(
                    WIN_W as f64,
                    WIN_H as f64,
                ));
            if let Ok(win) = el.create_window(attrs) {
                if let Ok(surf) = PlatformSurface::new(std::sync::Arc::new(win)) {
                    self.surf = Some(surf);
                }
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => self.close_request(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(_) => self.draw(),
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = (position.x as i32, position.y as i32);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.click(self.mouse.0, self.mouse.1);
                self.draw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    dillo_platform::winit::event::MouseScrollDelta::LineDelta(_, y) => {
                        (-y * 3.0 * 14.0) as i32
                    }
                    dillo_platform::winit::event::MouseScrollDelta::PixelDelta(p) => -p.y as i32,
                };
                let max = (self.items.len() as i32 * ROW_H
                    - self.surf.as_ref().map_or(0, |s| s.size().1 as i32))
                .max(0);
                self.scroll = (self.scroll + d).clamp(0, max);
                self.draw();
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.logical_key
                        == dillo_platform::winit::keyboard::Key::Named(
                            dillo_platform::winit::keyboard::NamedKey::Escape,
                        ) =>
            {
                self.close_request();
            }
            _ => {}
        }
        if self.exit {
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if std::time::Instant::now() >= self.next_tick {
            self.next_tick += std::time::Duration::from_secs(1);
            self.tick();
            self.draw();
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
        if self.exit {
            el.exit();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = std::sync::mpsc::channel();
    #[cfg(unix)]
    {
        std::thread::spawn(move || listener(tx));
    }
    #[cfg(not(unix))]
    {
        let _ = tx;
        eprintln!("[downloads dpi]: stdin socket unsupported on this platform");
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| ".".to_string(), |h| h.to_string_lossy().into_owned());
    let el = EventLoop::new()?;
    let mut app = DlWin {
        surf: None,
        fs: FontSystem::new(),
        sw: SwashCache::new(),
        items: Vec::new(),
        rx,
        scroll: 0,
        mouse: (-1, -1),
        confirm_quit: false,
        used: false,
        home,
        next_tick: std::time::Instant::now() + std::time::Duration::from_secs(1),
        exit: false,
    };
    el.run_app(&mut app)?;
    Ok(())
}
