//! Sparse-strip rasterizer.
//!
//! A path becomes coverage in three stages, each with plain data in between
//! so a stage can later move to SIMD or the GPU on its own:
//!
//! 1. [`flatten`]: transform to device space and flatten curves into lines.
//! 2. [`cells`]: walk every line and record signed area contributions for
//!    only the pixels it crosses. Memory scales with the outline's length.
//! 3. [`strips`]: sweep the cells in bands of [`TILE_HEIGHT`] rows. Runs of
//!    tiles that contain edges become strips with per-pixel alpha; the gaps
//!    between them have constant coverage and become solid fills.
//!
//! The compositor (`crate::composite`) then paints strips and fills.

mod cells;
pub(crate) mod flatten;
mod strips;

pub(crate) use strips::Coverage;

use crate::geom::Transform;
use crate::path::{FillRule, Path};

/// Tile width in pixels.
pub(crate) const TILE_WIDTH: u32 = 4;
/// Tile height in pixels; also the height of a band.
pub(crate) const TILE_HEIGHT: u32 = 4;

/// Maximum distance, in pixels, between a curve and its flattened lines.
pub(crate) const TOLERANCE: f32 = 0.1;

/// Rasterizes `path` into sparse coverage for a `width` × `height` target.
pub(crate) fn rasterize(
    path: &Path,
    transform: &Transform,
    fill_rule: FillRule,
    width: u32,
    height: u32,
) -> Coverage {
    let mut lines = Vec::new();
    flatten::flatten(
        path,
        transform,
        TOLERANCE,
        width as f32,
        height as f32,
        &mut lines,
    );
    let mut cells = Vec::new();
    for line in &lines {
        cells::line_cells(*line, width, height, &mut cells);
    }
    strips::build(&mut cells, fill_rule, width)
}

#[cfg(test)]
mod tests;
