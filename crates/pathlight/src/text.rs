//! Single-line shaped text.
//!
//! A string is split into bidi runs (unicode-bidi) and script runs
//! (unicode-script), each run is shaped with harfrust, and the resulting
//! glyph outlines (from skrifa) are filled through the same rasterizer as
//! paths. Line breaking, paragraph layout and font fallback are out of scope.

use crate::blend::BlendMode;
use crate::error::Error;
use crate::geom::Transform;
use crate::paint::Paint;
use crate::surface::{Mask, Surface};

/// A font face loaded from bytes the caller supplies. pathlight never reads
/// fonts from the filesystem or asks the platform for system fonts.
#[derive(Clone, Debug)]
pub struct Font {
    data: Vec<u8>,
    index: u32,
}

impl Font {
    /// Copies `data` and loads face `index` from it (0 unless `data` is a
    /// font collection).
    pub fn from_bytes(data: &[u8], index: u32) -> Result<Self, Error> {
        skrifa::FontRef::from_index(data, index).map_err(|_| Error::InvalidFont)?;
        Ok(Font {
            data: data.to_vec(),
            index,
        })
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn index(&self) -> u32 {
        self.index
    }
}

/// Draws `text` on one line with its baseline origin at `(x, y)` in user
/// space, `size` pixels per em. See [`crate::fill`] for `clip`.
#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    surface: &mut Surface<'_>,
    font: &Font,
    text: &str,
    size: f32,
    x: f32,
    y: f32,
    paint: &Paint,
    transform: &Transform,
    blend: BlendMode,
    clip: Option<&Mask<'_>>,
) -> Result<(), Error> {
    let _ = (surface, font, text, blend, clip);
    if !(size.is_finite()
        && size > 0.0
        && x.is_finite()
        && y.is_finite()
        && transform.is_finite()
        && paint.is_valid())
    {
        return Err(Error::InvalidArgument);
    }
    Err(Error::Unimplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage_font() {
        assert_eq!(
            Font::from_bytes(b"not a font", 0).unwrap_err(),
            Error::InvalidFont
        );
    }
}
