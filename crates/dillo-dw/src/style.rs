//! Port partiel de `dw/style.hh` et `dw/style.cc`.
//!
//! Ce module grandira avec le lot style complet (`Style`,
//! `Font`, `Tooltip`, `StyleImage`, `Length`, tables de
//! hachage par `Layout`). Pour l'instant : `Color` +
//! `Shading` + `shadeColor` — nécessaires aux primitives de
//! dessin (`types.rs`, `view.rs`).
//!
//! Note de fidélité : amont, `Color` est une ressource poolée
//! par `Layout` (`Color::create`/`remove`, refCount). Le port la
//! porte en valeur `Copy` — le pooling viendra avec
//! `layout.cc` s'il s'avère observable (perf), sinon la valeur
//! suffit (le pool n'est pas sémantique).

/// `dw::core::style::Color` — couleur `0xRRGGBB` (≙
/// `ColorAttrs::color` + `Shading`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Color {
    color: i32,
}

/// `dw::core::style::Color::Shading`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Shading {
    /// `SHADING_NORMAL`.
    Normal,
    /// `SHADING_INVERSE`.
    Inverse,
    /// `SHADING_DARK`.
    Dark,
    /// `SHADING_LIGHT`.
    Light,
}

/// `SHADING_NUM`.
pub const SHADING_NUM: usize = 4;

impl Color {
    /// `ColorAttrs (color)`.
    #[must_use]
    pub fn new(color: i32) -> Self {
        Color { color }
    }

    /// `getColor` — `0xRRGGBB`.
    #[must_use]
    pub fn get_color(&self) -> i32 {
        self.color
    }

    /// `shadeColor (color, d)` — verbatim : décale la luminosité
    /// (max/255) de ±0.4 près des extrêmes, ±0.2 sinon.
    /// Casts `(int)` amont → troncature volontaire.
    #[allow(clippy::cast_possible_truncation)]
    fn shade_color_int(color: i32, d: i32) -> i32 {
        let red = (color >> 16) & 255;
        let green = (color >> 8) & 255;
        let blue = color & 255;

        let old_lightness = f64::from(red.max(green).max(blue)) / 255.0;
        let new_lightness = if old_lightness > 0.8 {
            if d > 0 {
                old_lightness - 0.2
            } else {
                old_lightness - 0.4
            }
        } else if old_lightness < 0.2 {
            if d > 0 {
                old_lightness + 0.4
            } else {
                old_lightness + 0.2
            }
        } else {
            old_lightness + f64::from(d) * 0.2
        };

        let (red, green, blue) = if old_lightness == 0.0 {
            let v = (new_lightness * 255.0) as i32;
            (v, v, v)
        } else {
            let f = new_lightness / old_lightness;
            (
                (f64::from(red) * f) as i32,
                (f64::from(green) * f) as i32,
                (f64::from(blue) * f) as i32,
            )
        };

        (red << 16) | (green << 8) | blue
    }

    /// `shadeColor (color, Shading)` — `SHADING_NORMAL` →
    /// identité, `INVERSE` → `^ 0xff_ffff`, `LIGHT`/`DARK` → ±1.
    #[must_use]
    pub fn shade(&self, shading: Shading) -> Color {
        let c = match shading {
            Shading::Normal => self.color,
            Shading::Light => Self::shade_color_int(self.color, 1),
            Shading::Inverse => self.color ^ 0xff_ffff,
            Shading::Dark => Self::shade_color_int(self.color, -1),
        };
        Color { color: c }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shade_normal_and_inverse() {
        let c = Color::new(0x80_4020);
        assert_eq!(c.shade(Shading::Normal).get_color(), 0x80_4020);
        assert_eq!(c.shade(Shading::Inverse).get_color(), 0x7f_bfdf);
    }

    #[test]
    fn shade_light_dark_midrange() {
        // 0x80_8080 → lightness 128/255 ≈ 0.502 (midrange)
        let c = Color::new(0x80_8080);
        // f = 1 + 0.2*255/128 → 128*f = 179 exactement → 0xb3_b3b3
        assert_eq!(c.shade(Shading::Light).get_color(), 0xb3_b3b3);
        // f = 1 - 0.2*255/128 → 128*f = 77 exactement → 0x4d_4d4d
        assert_eq!(c.shade(Shading::Dark).get_color(), 0x4d_4d4d);
    }

    #[test]
    fn shade_extremes() {
        // blanc : oldLightness 1.0 > 0.8, d>0 → 0.8 → 0xcc_cccc
        assert_eq!(
            Color::new(0xff_ffff).shade(Shading::Light).get_color(),
            0xcc_cccc
        );
        // noir : oldLightness 0 < 0.2, d>0 → 0.4 → 102 = 0x66_6666
        assert_eq!(Color::new(0).shade(Shading::Light).get_color(), 0x66_6666);
    }
}
