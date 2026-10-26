use crate::blend::BlendMode;
use crate::composite;
use crate::error::Error;
use crate::geom::Transform;
use crate::paint::Paint;
use crate::path::{FillRule, Path};
use crate::raster;
use crate::stroke::StrokeStyle;
use crate::stroker;
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
    if !transform.is_finite() || !path.is_finite() || !paint.is_valid() {
        return Err(Error::InvalidArgument);
    }
    let coverage = raster::rasterize(
        path,
        transform,
        fill_rule,
        surface.width(),
        surface.height(),
    );
    composite::paint_coverage(surface, &coverage, paint, blend, clip);
    Ok(())
}

/// Strokes `path` into `surface`. See [`fill`] for `clip`.
///
/// Returns [`Error::InvalidArgument`] if the dash pattern would split the
/// path into more than a million dashes, or if the outline would need more
/// than 2²³ points (or 64 per input point, if that is more).
pub fn stroke(
    surface: &mut Surface<'_>,
    path: &Path,
    style: &StrokeStyle,
    paint: &Paint,
    transform: &Transform,
    blend: BlendMode,
    clip: Option<&Mask<'_>>,
) -> Result<(), Error> {
    if !transform.is_finite() || !path.is_finite() || !paint.is_valid() || !style.is_valid() {
        return Err(Error::InvalidArgument);
    }
    let outline = stroker::stroke_outline(path, style, transform, raster::TOLERANCE)?;
    fill(
        surface,
        &outline,
        FillRule::NonZero,
        paint,
        transform,
        blend,
        clip,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;
    use crate::paint::Color;
    use crate::path::PathBuilder;
    use crate::surface::Format;

    fn triangle(p: Point) -> Path {
        let mut b = PathBuilder::new();
        b.move_to(Point::new(0.0, 0.0))
            .line_to(p)
            .line_to(Point::new(0.0, 4.0))
            .close();
        b.finish()
    }

    #[test]
    fn non_finite_inputs_are_rejected() {
        let mut buf = [0u8; 16];
        let mut s = Surface::new(&mut buf, 2, 2, 8, Format::Rgba8).unwrap();
        let red = Paint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0));
        let nan_paint = Paint::Solid(Color::rgba(f32::NAN, 0.0, 0.0, 1.0));
        let id = Transform::IDENTITY;
        let ok = triangle(Point::new(4.0, 0.0));
        let bad = triangle(Point::new(f32::INFINITY, 0.0));
        let n = BlendMode::Normal;
        assert_eq!(
            fill(&mut s, &bad, FillRule::NonZero, &red, &id, n, None),
            Err(Error::InvalidArgument)
        );
        assert_eq!(
            fill(&mut s, &ok, FillRule::NonZero, &nan_paint, &id, n, None),
            Err(Error::InvalidArgument)
        );
        let style = StrokeStyle {
            width: f32::NAN,
            ..StrokeStyle::default()
        };
        assert_eq!(
            stroke(&mut s, &ok, &style, &red, &id, n, None),
            Err(Error::InvalidArgument)
        );
        assert_eq!(
            stroke(&mut s, &bad, &StrokeStyle::default(), &red, &id, n, None),
            Err(Error::InvalidArgument)
        );
        assert_eq!(
            fill(&mut s, &ok, FillRule::NonZero, &red, &id, n, None),
            Ok(())
        );
    }
}
