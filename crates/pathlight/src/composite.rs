//! Paints sparse coverage into a surface.

use crate::blend::{BlendMode, composite};
use crate::paint::Paint;
use crate::raster::{Coverage, TILE_HEIGHT};
use crate::surface::{Format, Mask, Surface};

/// Composites `paint` through `coverage` (and `clip`, if any) onto `surface`.
pub(crate) fn paint_coverage(
    surface: &mut Surface<'_>,
    coverage: &Coverage,
    paint: &Paint,
    blend: BlendMode,
    clip: Option<&Mask<'_>>,
) {
    let Paint::Solid(color) = paint;
    let c = color.premultiplied().map(|v| v.clamp(0.0, 1.0));
    let mut painter = Painter::new(surface, c, blend, clip);
    let th = TILE_HEIGHT as usize;
    for strip in &coverage.strips {
        let alphas =
            &coverage.alphas[strip.alpha_offset..strip.alpha_offset + strip.width as usize * th];
        for (col, column) in alphas.chunks_exact(th).enumerate() {
            for (row, &a) in column.iter().enumerate() {
                painter.pixel(
                    strip.x + col as u32,
                    strip.band * TILE_HEIGHT + row as u32,
                    a,
                );
            }
        }
    }
    for fill in &coverage.fills {
        for (row, &a) in fill.coverage.iter().enumerate() {
            painter.span(fill.x0, fill.x1, fill.band * TILE_HEIGHT + row as u32, a);
        }
    }
}

struct Painter<'s, 'a, 'm> {
    surface: &'s mut Surface<'a>,
    /// Premultiplied source color, `[r, g, b, a]`.
    color: [f32; 4],
    blend: BlendMode,
    clip: Option<&'m Mask<'m>>,
    /// The color's bytes in the surface's channel order, when a fully
    /// covered pixel can simply be overwritten with them.
    opaque_bytes: Option<[u8; 4]>,
}

impl<'s, 'a, 'm> Painter<'s, 'a, 'm> {
    fn new(
        surface: &'s mut Surface<'a>,
        color: [f32; 4],
        blend: BlendMode,
        clip: Option<&'m Mask<'m>>,
    ) -> Self {
        // Source-over with an opaque source replaces the backdrop outright.
        let opaque_bytes = (blend == BlendMode::Normal && color[3] >= 1.0).then(|| {
            let [r, g, b, a] = color.map(to_u8);
            match surface.format() {
                Format::Rgba8 => [r, g, b, a],
                Format::Bgra8 => [b, g, r, a],
                Format::A8 => [a, 0, 0, 0],
            }
        });
        Painter {
            surface,
            color,
            blend,
            clip,
            opaque_bytes,
        }
    }

    fn span(&mut self, x0: u32, x1: u32, y: u32, alpha: u8) {
        if alpha == 0 || y >= self.surface.height() {
            return;
        }
        let x1 = x1.min(self.surface.width());
        if x0 >= x1 {
            return;
        }
        if let (Some(bytes), 255, None) = (self.opaque_bytes, alpha, self.clip) {
            let bpp = self.surface.format().bytes_per_pixel();
            let start = y as usize * self.surface.stride() + x0 as usize * bpp;
            let end = start + (x1 - x0) as usize * bpp;
            for px in self.surface.data_mut()[start..end].chunks_exact_mut(bpp) {
                px.copy_from_slice(&bytes[..bpp]);
            }
            return;
        }
        for x in x0..x1 {
            self.pixel(x, y, alpha);
        }
    }

    fn pixel(&mut self, x: u32, y: u32, alpha: u8) {
        if alpha == 0 || x >= self.surface.width() || y >= self.surface.height() {
            return;
        }
        let mut cov = alpha as f32 / 255.0;
        if let Some(clip) = self.clip {
            cov *= clip.coverage(x, y) as f32 / 255.0;
            if cov == 0.0 {
                return;
            }
        }
        let format = self.surface.format();
        let bpp = format.bytes_per_pixel();
        let at = y as usize * self.surface.stride() + x as usize * bpp;
        let px = &mut self.surface.data_mut()[at..at + bpp];
        if cov >= 1.0
            && let Some(bytes) = self.opaque_bytes
        {
            px.copy_from_slice(&bytes[..bpp]);
            return;
        }
        let src = self.color.map(|v| v * cov);
        match format {
            Format::A8 => {
                let da = from_u8(px[0]);
                px[0] = to_u8(src[3] + da - src[3] * da);
            }
            Format::Rgba8 | Format::Bgra8 => {
                let (ri, bi) = if format == Format::Rgba8 {
                    (0, 2)
                } else {
                    (2, 0)
                };
                let dst = [
                    from_u8(px[ri]),
                    from_u8(px[1]),
                    from_u8(px[bi]),
                    from_u8(px[3]),
                ];
                let out = composite(self.blend, src, dst);
                let a = to_u8(out[3]);
                // Rounding must not leave a color channel above alpha.
                px[ri] = to_u8(out[0]).min(a);
                px[1] = to_u8(out[1]).min(a);
                px[bi] = to_u8(out[2]).min(a);
                px[3] = a;
            }
        }
    }
}

fn from_u8(v: u8) -> f32 {
    v as f32 / 255.0
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}
