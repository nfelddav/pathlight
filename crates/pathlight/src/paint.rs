/// A straight-alpha color in the sRGB color space, each channel in `0.0..=1.0`.
///
/// Blending happens on sRGB-encoded values, matching the CSS color model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Color { r, g, b, a }
    }

    /// Returns `[r, g, b, a]` premultiplied by alpha.
    pub fn premultiplied(&self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }
}

/// What fills the covered pixels of a shape.
///
/// v1 has solid colors only. Gradients and image patterns are expected to be
/// added as new variants, which is why this is an enum and not a bare color.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Paint {
    Solid(Color),
}
