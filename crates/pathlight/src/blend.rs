//! Blend modes from CSS Compositing and Blending Level 1.
//!
//! Every mode is separable: it acts on each color channel independently,
//! which keeps the compositor one table lookup per mode and maps cleanly
//! onto SIMD lanes. The non-separable modes (hue, saturation, color,
//! luminosity) are deliberately out of scope.

/// How a source color mixes with the backdrop. All modes then composite with
/// source-over, as in CSS `mix-blend-mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum BlendMode {
    /// Plain source-over.
    #[default]
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Overlay = 3,
    Darken = 4,
    Lighten = 5,
    ColorDodge = 6,
    ColorBurn = 7,
    HardLight = 8,
    SoftLight = 9,
    Difference = 10,
    Exclusion = 11,
}

impl BlendMode {
    pub const ALL: [BlendMode; 12] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
    ];

    pub fn from_u32(v: u32) -> Option<Self> {
        Self::ALL.get(v as usize).copied()
    }

    /// The CSS blend function B(Cb, Cs) on straight (non-premultiplied)
    /// channel values in `0.0..=1.0`.
    fn channel(self, cb: f32, cs: f32) -> f32 {
        match self {
            BlendMode::Normal => cs,
            BlendMode::Multiply => cb * cs,
            BlendMode::Screen => screen(cb, cs),
            BlendMode::Overlay => hard_light(cs, cb),
            BlendMode::Darken => cb.min(cs),
            BlendMode::Lighten => cb.max(cs),
            BlendMode::ColorDodge => {
                if cb == 0.0 {
                    0.0
                } else if cs >= 1.0 {
                    1.0
                } else {
                    (cb / (1.0 - cs)).min(1.0)
                }
            }
            BlendMode::ColorBurn => {
                if cb >= 1.0 {
                    1.0
                } else if cs == 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - cb) / cs).min(1.0)
                }
            }
            BlendMode::HardLight => hard_light(cb, cs),
            BlendMode::SoftLight => {
                if cs <= 0.5 {
                    cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
                } else {
                    let d = if cb <= 0.25 {
                        ((16.0 * cb - 12.0) * cb + 4.0) * cb
                    } else {
                        cb.sqrt()
                    };
                    cb + (2.0 * cs - 1.0) * (d - cb)
                }
            }
            BlendMode::Difference => (cb - cs).abs(),
            BlendMode::Exclusion => cb + cs - 2.0 * cb * cs,
        }
    }
}

fn screen(cb: f32, cs: f32) -> f32 {
    cb + cs - cb * cs
}

fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        cb * 2.0 * cs
    } else {
        screen(cb, 2.0 * cs - 1.0)
    }
}

/// Blends a premultiplied source pixel onto a premultiplied backdrop pixel
/// and composites with source-over. Channels are `[r, g, b, a]`.
///
/// This is the scalar reference: the optimized compositors must agree with it.
pub fn composite(mode: BlendMode, src: [f32; 4], dst: [f32; 4]) -> [f32; 4] {
    let (sa, da) = (src[3], dst[3]);
    let mut out = [0.0; 4];
    for i in 0..3 {
        let cs = if sa > 0.0 { src[i] / sa } else { 0.0 };
        let cb = if da > 0.0 { dst[i] / da } else { 0.0 };
        out[i] = src[i] * (1.0 - da) + dst[i] * (1.0 - sa) + sa * da * mode.channel(cb, cs);
    }
    out[3] = sa + da - sa * da;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn normal_is_source_over() {
        let src = [0.25, 0.0, 0.0, 0.5];
        let dst = [0.0, 0.0, 1.0, 1.0];
        assert!(close(
            composite(BlendMode::Normal, src, dst),
            [0.25, 0.0, 0.5, 1.0]
        ));
    }

    #[test]
    fn every_mode_is_source_over_onto_transparent() {
        let src = [0.2, 0.4, 0.1, 0.6];
        for mode in BlendMode::ALL {
            assert!(close(composite(mode, src, [0.0; 4]), src), "{mode:?}");
        }
    }

    #[test]
    fn opaque_multiply_and_screen() {
        let src = [0.5, 1.0, 0.0, 1.0];
        let dst = [0.5, 0.5, 0.5, 1.0];
        assert!(close(
            composite(BlendMode::Multiply, src, dst),
            [0.25, 0.5, 0.0, 1.0]
        ));
        assert!(close(
            composite(BlendMode::Screen, src, dst),
            [0.75, 1.0, 0.5, 1.0]
        ));
    }

    #[test]
    fn from_u32_round_trips() {
        for mode in BlendMode::ALL {
            assert_eq!(BlendMode::from_u32(mode as u32), Some(mode));
        }
        assert_eq!(BlendMode::from_u32(12), None);
    }
}
