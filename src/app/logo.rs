//! The toolbar's app icon, scaled on the CPU to the exact pixel size it is shown at.
//!
//! Handing the GPU the 256 px icon and letting it shrink it to ~36 px gives a soft,
//! shimmering result (the wgpu backend has no mipmaps). Instead the icon is area-averaged
//! down to the screen's pixel size, and re-made when the display scale changes. At 40 px
//! and under it uses the small design, as the .ico does.

use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};

const LARGE_PNG: &[u8] = super::ICON_PNG;
const SMALL_PNG: &[u8] = include_bytes!("../../assets/icon-small-256.png");
/// Up to this many pixels the small design reads better than the large one.
const SMALL_MAX_PX: usize = 40;

#[derive(Default)]
pub struct Logo {
    /// The texture and the pixel size it was made for.
    tex: Option<(usize, TextureHandle)>,
}

impl Logo {
    /// The texture for showing the logo `size` points square.
    pub fn texture(&mut self, ctx: &egui::Context, size: f32) -> Option<&TextureHandle> {
        let px = (size * ctx.pixels_per_point()).round().max(1.0) as usize;
        if self.tex.as_ref().is_none_or(|(made, _)| *made != px) {
            let png = if px <= SMALL_MAX_PX { SMALL_PNG } else { LARGE_PNG };
            self.tex = eframe::icon_data::from_png_bytes(png).ok().map(|d| {
                let img = downscale(&d.rgba, d.width as usize, d.height as usize, px);
                (px, ctx.load_texture("app-icon", img, TextureOptions::LINEAR))
            });
        }
        self.tex.as_ref().map(|(_, t)| t)
    }
}

/// Area-averages a square-ish RGBA image down to `out` × `out` pixels, in premultiplied
/// alpha so transparent edges do not darken.
fn downscale(rgba: &[u8], w: usize, h: usize, out: usize) -> ColorImage {
    let premul: Vec<[f32; 4]> = rgba
        .chunks_exact(4)
        .map(|p| {
            let a = p[3] as f32 / 255.0;
            [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, p[3] as f32]
        })
        .collect();
    let xs = weights(w, out);
    let ys = weights(h, out);
    let mut pixels = Vec::with_capacity(out * out);
    for row in &ys {
        for col in &xs {
            let mut acc = [0.0f32; 4];
            for &(sy, wy) in row {
                for &(sx, wx) in col {
                    let p = premul[sy * w + sx];
                    let k = wx * wy;
                    for c in 0..4 {
                        acc[c] += p[c] * k;
                    }
                }
            }
            let a = acc[3];
            let un = |v: f32| if a > 0.0 { (v * 255.0 / a).round().clamp(0.0, 255.0) as u8 } else { 0 };
            pixels.push(Color32::from_rgba_unmultiplied(un(acc[0]), un(acc[1]), un(acc[2]), a.round().clamp(0.0, 255.0) as u8));
        }
    }
    ColorImage::new([out, out], pixels)
}

/// For each of `n` output pixels, the source pixels it covers and their weights (summing to 1).
fn weights(src: usize, n: usize) -> Vec<Vec<(usize, f32)>> {
    let scale = src as f32 / n as f32;
    (0..n)
        .map(|i| {
            let (lo, hi) = (i as f32 * scale, (i + 1) as f32 * scale);
            let first = lo.floor() as usize;
            let last = (hi.ceil() as usize).min(src);
            (first..last)
                .map(|s| {
                    let cover = (hi.min(s as f32 + 1.0) - lo.max(s as f32)).max(0.0);
                    (s, cover / scale)
                })
                .filter(|&(_, w)| w > 0.0)
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_cover_each_output_pixel_once() {
        for (src, n) in [(256, 36), (256, 54), (256, 72), (256, 256), (10, 3)] {
            for row in weights(src, n) {
                let sum: f32 = row.iter().map(|&(_, w)| w).sum();
                assert!((sum - 1.0).abs() < 1e-4, "{src}->{n}: {sum}");
            }
        }
    }

    #[test]
    fn opaque_white_stays_white() {
        let img = downscale(&[255; 8 * 8 * 4], 8, 8, 3);
        assert!(img.pixels.iter().all(|&p| p == Color32::WHITE));
    }

    #[test]
    fn both_designs_decode() {
        for png in [LARGE_PNG, SMALL_PNG] {
            let d = eframe::icon_data::from_png_bytes(png).unwrap();
            assert_eq!((d.width, d.height), (256, 256));
        }
    }
}
