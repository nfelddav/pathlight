//! Sparse-strip rasterizer (not implemented yet).
//!
//! Plan: flatten curves to lines in device space, then walk each line and
//! record per-tile coverage contributions for only the tiles the edge
//! touches ("strips"). Tiles fully inside a shape are filled as spans
//! without per-pixel coverage. Memory scales with the length of the
//! outline, not the area of the shape, and the tile buffers have a fixed
//! layout that maps onto SIMD lanes now and GPU compute later.

/// Tile width in pixels.
#[allow(dead_code)]
pub(crate) const TILE_WIDTH: u32 = 4;
/// Tile height in pixels.
#[allow(dead_code)]
pub(crate) const TILE_HEIGHT: u32 = 4;
