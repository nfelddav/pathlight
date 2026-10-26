//! Python bindings for pathlight, built with maturin.
//!
//! Unlike the C ABI, a Python `Surface` owns its pixels, so no buffer
//! lifetimes leak into Python code.

// Enum variants use Python constant style (BlendMode.COLOR_DODGE).
#![allow(non_camel_case_types, clippy::upper_case_acronyms)]

use pyo3::exceptions::{PyNotImplementedError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use pathlight as core;

fn to_py(e: core::Error) -> PyErr {
    match e {
        core::Error::Unimplemented => PyNotImplementedError::new_err(e.to_string()),
        core::Error::InvalidArgument | core::Error::InvalidFont => {
            PyValueError::new_err(e.to_string())
        }
    }
}

/// Pixel layout. Color formats store premultiplied alpha.
#[pyclass(eq, eq_int, frozen, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
enum Format {
    RGBA8,
    BGRA8,
    A8,
}

impl From<Format> for core::Format {
    fn from(f: Format) -> Self {
        match f {
            Format::RGBA8 => core::Format::Rgba8,
            Format::BGRA8 => core::Format::Bgra8,
            Format::A8 => core::Format::A8,
        }
    }
}

/// CSS blend mode; all modes composite with source-over.
#[pyclass(eq, eq_int, frozen, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
enum BlendMode {
    NORMAL,
    MULTIPLY,
    SCREEN,
    OVERLAY,
    DARKEN,
    LIGHTEN,
    COLOR_DODGE,
    COLOR_BURN,
    HARD_LIGHT,
    SOFT_LIGHT,
    DIFFERENCE,
    EXCLUSION,
}

impl From<BlendMode> for core::BlendMode {
    fn from(b: BlendMode) -> Self {
        core::BlendMode::from_u32(b as u32)
            .expect("Python and core blend modes are in the same order")
    }
}

#[pyclass(eq, eq_int, frozen, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
enum FillRule {
    NONZERO,
    EVENODD,
}

/// A path built from lines, Béziers and uniform cubic B-splines.
#[pyclass]
#[derive(Default)]
struct Path(core::PathBuilder);

#[pymethods]
impl Path {
    #[new]
    fn new() -> Self {
        Self::default()
    }

    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(core::Point::new(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(core::Point::new(x, y));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0
            .quad_to(core::Point::new(cx, cy), core::Point::new(x, y));
    }

    fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.0.cubic_to(
            core::Point::new(c1x, c1y),
            core::Point::new(c2x, c2y),
            core::Point::new(x, y),
        );
    }

    fn close(&mut self) {
        self.0.close();
    }

    /// Adds a uniform cubic B-spline through `points`, a list of (x, y).
    #[pyo3(signature = (points, closed=false))]
    fn bspline(&mut self, points: Vec<(f32, f32)>, closed: bool) -> PyResult<()> {
        let pts: Vec<_> = points
            .into_iter()
            .map(|(x, y)| core::Point::new(x, y))
            .collect();
        if self.0.bspline(&pts, closed) {
            Ok(())
        } else {
            Err(PyValueError::new_err(
                "too few control points for a B-spline",
            ))
        }
    }

    fn __len__(&self) -> usize {
        self.0.as_path().verbs().len()
    }
}

/// A tightly packed pixel buffer, initially transparent.
#[pyclass]
struct Surface {
    data: Vec<u8>,
    width: u32,
    height: u32,
    format: Format,
}

impl Surface {
    fn stride(&self) -> usize {
        self.width as usize * core::Format::from(self.format).bytes_per_pixel()
    }

    fn view(&mut self) -> PyResult<core::Surface<'_>> {
        let stride = self.stride();
        core::Surface::new(
            &mut self.data,
            self.width,
            self.height,
            stride,
            self.format.into(),
        )
        .map_err(to_py)
    }

    fn mask(&self) -> PyResult<core::Mask<'_>> {
        if self.format != Format::A8 {
            return Err(PyValueError::new_err("a clip surface must use Format.A8"));
        }
        core::Mask::new(&self.data, self.width, self.height, self.stride()).map_err(to_py)
    }
}

#[pymethods]
impl Surface {
    #[new]
    #[pyo3(signature = (width, height, format=Format::RGBA8))]
    fn new(width: u32, height: u32, format: Format) -> PyResult<Self> {
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(core::Format::from(format).bytes_per_pixel()))
            .filter(|&n| n > 0)
            .ok_or_else(|| PyValueError::new_err("invalid surface size"))?;
        Ok(Surface {
            data: vec![0; len],
            width,
            height,
            format,
        })
    }

    #[getter]
    fn width(&self) -> u32 {
        self.width
    }

    #[getter]
    fn height(&self) -> u32 {
        self.height
    }

    /// A copy of the pixels.
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.data)
    }
}

type Affine = (f32, f32, f32, f32, f32, f32);
type Rgba = (f32, f32, f32, f32);

fn solid((r, g, b, a): Rgba) -> core::Paint {
    core::Paint::Solid(core::Color::rgba(r, g, b, a))
}

fn affine(t: Option<Affine>) -> core::Transform {
    t.map(|(a, b, c, d, e, f)| core::Transform { a, b, c, d, e, f })
        .unwrap_or(core::Transform::IDENTITY)
}

/// Borrows the target for writing and the optional clip for reading, then
/// runs `draw`. A surface cannot be its own clip.
fn with_target(
    surface: &Bound<'_, Surface>,
    clip: Option<&Bound<'_, Surface>>,
    draw: impl FnOnce(&mut core::Surface<'_>, Option<&core::Mask<'_>>) -> Result<(), core::Error>,
) -> PyResult<()> {
    if clip.is_some_and(|c| c.is(surface)) {
        return Err(PyValueError::new_err("a surface cannot be its own clip"));
    }
    let clip = clip.map(|c| c.borrow());
    let mask = clip.as_ref().map(|c| c.mask()).transpose()?;
    let mut target = surface.borrow_mut();
    draw(&mut target.view()?, mask.as_ref()).map_err(to_py)
}

/// Fills `path` with a straight-alpha sRGB `color` (r, g, b, a in 0..1).
/// `transform` is (a, b, c, d, e, f) or None for identity. `clip`, if given,
/// is an A8 surface whose coverage masks the fill.
#[pyfunction]
#[pyo3(signature = (surface, path, color, blend=BlendMode::NORMAL, fill_rule=FillRule::NONZERO, transform=None, clip=None))]
fn fill(
    surface: &Bound<'_, Surface>,
    path: &Path,
    color: Rgba,
    blend: BlendMode,
    fill_rule: FillRule,
    transform: Option<Affine>,
    clip: Option<&Bound<'_, Surface>>,
) -> PyResult<()> {
    let rule = match fill_rule {
        FillRule::NONZERO => core::FillRule::NonZero,
        FillRule::EVENODD => core::FillRule::EvenOdd,
    };
    with_target(surface, clip, |target, mask| {
        core::fill(
            target,
            path.0.as_path(),
            rule,
            &solid(color),
            &affine(transform),
            blend.into(),
            mask,
        )
    })
}

#[pyclass(eq, eq_int, frozen, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
enum LineCap {
    BUTT,
    ROUND,
    SQUARE,
}

#[pyclass(eq, eq_int, frozen, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
enum LineJoin {
    MITER,
    ROUND,
    BEVEL,
}

/// Strokes `path`. `dashes` alternates dash and gap lengths; empty means a
/// solid stroke. See `fill` for `transform` and `clip`.
#[pyfunction]
#[pyo3(signature = (
    surface, path, color, width=1.0, cap=LineCap::BUTT, join=LineJoin::MITER, miter_limit=4.0,
    dashes=Vec::new(), dash_offset=0.0, blend=BlendMode::NORMAL, transform=None, clip=None
))]
#[allow(clippy::too_many_arguments)]
fn stroke(
    surface: &Bound<'_, Surface>,
    path: &Path,
    color: Rgba,
    width: f32,
    cap: LineCap,
    join: LineJoin,
    miter_limit: f32,
    dashes: Vec<f32>,
    dash_offset: f32,
    blend: BlendMode,
    transform: Option<Affine>,
    clip: Option<&Bound<'_, Surface>>,
) -> PyResult<()> {
    let style = core::StrokeStyle {
        width,
        cap: match cap {
            LineCap::BUTT => core::LineCap::Butt,
            LineCap::ROUND => core::LineCap::Round,
            LineCap::SQUARE => core::LineCap::Square,
        },
        join: match join {
            LineJoin::MITER => core::LineJoin::Miter,
            LineJoin::ROUND => core::LineJoin::Round,
            LineJoin::BEVEL => core::LineJoin::Bevel,
        },
        miter_limit,
        dashes,
        dash_offset,
    };
    with_target(surface, clip, |target, mask| {
        core::stroke(
            target,
            path.0.as_path(),
            &style,
            &solid(color),
            &affine(transform),
            blend.into(),
            mask,
        )
    })
}

/// A font face loaded from font file bytes (face `index` of a collection).
#[pyclass(frozen)]
struct Font(core::Font);

#[pymethods]
impl Font {
    #[new]
    #[pyo3(signature = (data, index=0))]
    fn new(data: &[u8], index: u32) -> PyResult<Self> {
        core::Font::from_bytes(data, index).map(Font).map_err(to_py)
    }
}

/// Draws one line of `text`, baseline origin at (x, y), `size` pixels per
/// em. See `fill` for `transform` and `clip`.
#[pyfunction]
#[pyo3(signature = (surface, font, text, size, x, y, color, blend=BlendMode::NORMAL, transform=None, clip=None))]
#[allow(clippy::too_many_arguments)]
fn draw_text(
    surface: &Bound<'_, Surface>,
    font: &Font,
    text: &str,
    size: f32,
    x: f32,
    y: f32,
    color: Rgba,
    blend: BlendMode,
    transform: Option<Affine>,
    clip: Option<&Bound<'_, Surface>>,
) -> PyResult<()> {
    with_target(surface, clip, |target, mask| {
        core::draw_text(
            target,
            &font.0,
            text,
            size,
            x,
            y,
            &solid(color),
            &affine(transform),
            blend.into(),
            mask,
        )
    })
}

#[pymodule(name = "pathlight")]
fn pathlight_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<Format>()?;
    m.add_class::<BlendMode>()?;
    m.add_class::<FillRule>()?;
    m.add_class::<LineCap>()?;
    m.add_class::<LineJoin>()?;
    m.add_class::<Path>()?;
    m.add_class::<Surface>()?;
    m.add_class::<Font>()?;
    m.add_function(wrap_pyfunction!(fill, m)?)?;
    m.add_function(wrap_pyfunction!(stroke, m)?)?;
    m.add_function(wrap_pyfunction!(draw_text, m)?)?;
    Ok(())
}
