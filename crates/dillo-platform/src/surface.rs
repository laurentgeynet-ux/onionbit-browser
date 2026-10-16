//! `surface` — fenêtre `winit` + présentation `softbuffer`/`tiny-skia`.
//!
//! Port précurseur de `dw/fltkviewbase.cc` + `dw/fltkviewport.cc` :
//! la `Pixmap` tiny-skia joue le rôle du back-buffer FLTK, blitée
//! dans le buffer softbuffer (format `0RGB`) puis présentée.

use std::num::NonZeroU32;
use std::sync::Arc;
use winit::window::Window;

/// Erreurs de surface (contexte softbuffer ou présentation).
#[derive(Debug, thiserror::Error)]
pub enum SurfaceError {
    /// Initialisation softbuffer (contexte ou surface).
    #[error("softbuffer: {0}")]
    Softbuffer(#[from] softbuffer::SoftBufferError),
}

/// Fenêtre + surface de présentation (1 fenêtre = 1 surface).
pub struct PlatformSurface {
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
}

impl PlatformSurface {
    /// Crée la surface softbuffer attachée à `window`.
    ///
    /// # Errors
    /// `SurfaceError::Softbuffer` si le contexte/display refuse.
    pub fn new(window: Arc<Window>) -> Result<Self, SurfaceError> {
        let ctx = softbuffer::Context::new(window.clone())?;
        let surface = softbuffer::Surface::new(&ctx, window.clone())?;
        Ok(Self { window, surface })
    }

    /// Taille courante de la fenêtre `(w, h)` en pixels physiques.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        let s = self.window.inner_size();
        (s.width, s.height)
    }

    /// Fenêtre sous-jacente (pour `request_redraw`, titre…).
    #[must_use]
    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    /// Blit `pixmap` (RGBA prémultiplié tiny-skia → `0RGB` u32) et
    /// présente. Redimensionne la surface si besoin.
    ///
    /// # Errors
    /// `SurfaceError::Softbuffer` sur échec buffer/présentation.
    pub fn present(&mut self, pixmap: &tiny_skia::Pixmap) -> Result<(), SurfaceError> {
        let (w, h) = self.size();
        if w == 0 || h == 0 {
            return Ok(());
        }
        self.surface.resize(
            NonZeroU32::new(w).unwrap_or_else(|| unreachable!()),
            NonZeroU32::new(h).unwrap_or_else(|| unreachable!()),
        )?;
        let mut buf = self.surface.buffer_mut()?;
        let bw = buf.len() / h as usize;
        let (pw, ph) = (pixmap.width(), pixmap.height());
        for y in 0..h.min(ph) {
            for x in 0..pw.min(u32::try_from(bw).unwrap_or(u32::MAX)) {
                let Some(p) = pixmap.pixel(x, y) else {
                    continue;
                };
                buf[y as usize * bw + x as usize] =
                    (u32::from(p.red()) << 16) | (u32::from(p.green()) << 8) | u32::from(p.blue());
            }
        }
        buf.present()?;
        Ok(())
    }
}
