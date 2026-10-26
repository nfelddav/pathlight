use crate::blend::BlendMode;
use crate::error::Error;
use crate::geom::Transform;
use crate::paint::Paint;
use crate::path::{FillRule, Path};
use crate::stroke::StrokeStyle;
use crate::surface::{Mask, Surface};

/// Fills `path` into `surface`.
///
/// `clip`, if given, multiplies coverage by the mask; pixels outside the mask
/// are left untouched.
pub fn fill(
    surface: &mut Surface<'_>,
    path: &Path,
    fill_rule: FillRule,
    paint: &Paint,
    transform: &Transform,
    blend: BlendMode,
    clip: Option<&Mask<'_>>,
) -> Result<(), Error> {
    let _ = (surface, path, fill_rule, paint, blend, clip);
    if !transform.is_finite() {
        return Err(Error::InvalidArgument);
    }
    Err(Error::Unimplemented)
}

/// Strokes `path` into `surface`. See [`fill`] for `clip`.
pub fn stroke(
    surface: &mut Surface<'_>,
    path: &Path,
    style: &StrokeStyle,
    paint: &Paint,
    transform: &Transform,
    blend: BlendMode,
    clip: Option<&Mask<'_>>,
) -> Result<(), Error> {
    let _ = (surface, path, paint, blend, clip);
    if !transform.is_finite() || !style.is_valid() {
        return Err(Error::InvalidArgument);
    }
    Err(Error::Unimplemented)
}
