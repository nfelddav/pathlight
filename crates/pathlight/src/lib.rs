//! A lightweight 2D path renderer.
//!
//! pathlight renders filled and stroked paths, built from quadratic and cubic
//! Béziers or uniform cubic B-splines, into caller-owned pixel buffers. Shapes
//! are composited with source-over or any separable CSS blend mode, and text
//! is shaped (with bidi and script itemization) and drawn through the same
//! rasterizer as paths.
//!
//! The pipeline is flatten → sparse-strip coverage → composite. Each stage is
//! kept free of pixel-format assumptions so it can later gain SIMD or GPU
//! backends without changing the public API. See `docs/DESIGN.md`.

mod blend;
mod composite;
mod error;
mod geom;
mod paint;
mod path;
mod raster;
mod render;
mod stroke;
mod surface;
#[cfg(feature = "text")]
mod text;

pub use blend::{BlendMode, composite};
pub use error::Error;
pub use geom::{Point, Transform};
pub use paint::{Color, Paint};
pub use path::{FillRule, Path, PathBuilder, Verb};
pub use render::{fill, stroke};
pub use stroke::{LineCap, LineJoin, StrokeStyle};
pub use surface::{Format, MAX_HEIGHT, Mask, Surface};
#[cfg(feature = "text")]
pub use text::{Font, draw_text, measure_text, text_path};
