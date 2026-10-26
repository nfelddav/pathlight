//! C ABI for pathlight.
//!
//! Conventions:
//! - Every function that can fail returns a `PathlightStatus`.
//! - Enum-like parameters are plain `uint32_t` values from the
//!   `PATHLIGHT_*` constants and are validated, so a bad value from C is an
//!   error, not undefined behavior.
//! - Optional pointers (`transform`, `clip`) may be NULL.
//! - Panics never cross the boundary; they become `PATHLIGHT_STATUS_INTERNAL`.

#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};

use pathlight::{
    BlendMode, Color, Error, FillRule, Font, Format, LineCap, LineJoin, Mask, Paint, PathBuilder,
    Point, StrokeStyle, Surface, Transform,
};

pub type PathlightStatus = i32;
pub const PATHLIGHT_STATUS_OK: PathlightStatus = 0;
pub const PATHLIGHT_STATUS_INVALID_ARGUMENT: PathlightStatus = 1;
pub const PATHLIGHT_STATUS_INVALID_FONT: PathlightStatus = 2;
pub const PATHLIGHT_STATUS_UNIMPLEMENTED: PathlightStatus = 3;
pub const PATHLIGHT_STATUS_INTERNAL: PathlightStatus = 4;

pub const PATHLIGHT_FORMAT_RGBA8: u32 = 0;
pub const PATHLIGHT_FORMAT_BGRA8: u32 = 1;
pub const PATHLIGHT_FORMAT_A8: u32 = 2;

/// Largest `height` a surface or clip may have.
pub const PATHLIGHT_MAX_HEIGHT: u32 = 1 << 30;
const _: () = assert!(PATHLIGHT_MAX_HEIGHT == pathlight::MAX_HEIGHT);

pub const PATHLIGHT_FILL_NONZERO: u32 = 0;
pub const PATHLIGHT_FILL_EVENODD: u32 = 1;

pub const PATHLIGHT_PAINT_SOLID: u32 = 0;

pub const PATHLIGHT_CAP_BUTT: u32 = 0;
pub const PATHLIGHT_CAP_ROUND: u32 = 1;
pub const PATHLIGHT_CAP_SQUARE: u32 = 2;

pub const PATHLIGHT_JOIN_MITER: u32 = 0;
pub const PATHLIGHT_JOIN_ROUND: u32 = 1;
pub const PATHLIGHT_JOIN_BEVEL: u32 = 2;

pub const PATHLIGHT_BLEND_NORMAL: u32 = 0;
pub const PATHLIGHT_BLEND_MULTIPLY: u32 = 1;
pub const PATHLIGHT_BLEND_SCREEN: u32 = 2;
pub const PATHLIGHT_BLEND_OVERLAY: u32 = 3;
pub const PATHLIGHT_BLEND_DARKEN: u32 = 4;
pub const PATHLIGHT_BLEND_LIGHTEN: u32 = 5;
pub const PATHLIGHT_BLEND_COLOR_DODGE: u32 = 6;
pub const PATHLIGHT_BLEND_COLOR_BURN: u32 = 7;
pub const PATHLIGHT_BLEND_HARD_LIGHT: u32 = 8;
pub const PATHLIGHT_BLEND_SOFT_LIGHT: u32 = 9;
pub const PATHLIGHT_BLEND_DIFFERENCE: u32 = 10;
pub const PATHLIGHT_BLEND_EXCLUSION: u32 = 11;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PathlightPoint {
    pub x: f32,
    pub y: f32,
}

/// Straight-alpha sRGB color, channels in 0..1.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PathlightColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// Affine transform mapping (x, y) to (a*x + c*y + e, b*x + d*y + f).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PathlightTransform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

/// A caller-owned pixel buffer of `height` rows, `stride` bytes apart. The
/// last row only needs `width` pixels. A clip passed to a draw call must not
/// share any bytes with the target.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PathlightSurface {
    pub data: *mut u8,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    /// One of PATHLIGHT_FORMAT_*.
    pub format: u32,
}

/// What fills a shape. `kind` is one of PATHLIGHT_PAINT_*; only SOLID exists
/// so far, which uses `color`. Later kinds will use `ext`, which must be NULL
/// for SOLID.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PathlightPaint {
    pub kind: u32,
    pub color: PathlightColor,
    pub ext: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PathlightStrokeStyle {
    pub width: f32,
    /// One of PATHLIGHT_CAP_*.
    pub cap: u32,
    /// One of PATHLIGHT_JOIN_*.
    pub join: u32,
    pub miter_limit: f32,
    /// Alternating dash and gap lengths, or NULL for a solid stroke.
    pub dashes: *const f32,
    pub dash_count: usize,
    pub dash_offset: f32,
}

/// An opaque, growable path.
pub struct PathlightPath(PathBuilder);

/// An opaque font face.
pub struct PathlightFont(Font);

fn status(r: Result<(), Error>) -> PathlightStatus {
    match r {
        Ok(()) => PATHLIGHT_STATUS_OK,
        Err(Error::InvalidArgument) => PATHLIGHT_STATUS_INVALID_ARGUMENT,
        Err(Error::InvalidFont) => PATHLIGHT_STATUS_INVALID_FONT,
        Err(Error::Unimplemented) => PATHLIGHT_STATUS_UNIMPLEMENTED,
    }
}

fn guard(f: impl FnOnce() -> Result<(), Error>) -> PathlightStatus {
    catch_unwind(AssertUnwindSafe(f))
        .map(status)
        .unwrap_or(PATHLIGHT_STATUS_INTERNAL)
}

fn point(x: f32, y: f32) -> Point {
    Point::new(x, y)
}

/// Number of bytes a surface spans: full strides for every row but the last,
/// which only needs `width` pixels. This matches what the core accepts, so a
/// caller with a tightly packed last row is never read past.
fn byte_len(s: &PathlightSurface, format: Format) -> Result<usize, Error> {
    if s.data.is_null() || s.width == 0 || s.height == 0 {
        return Err(Error::InvalidArgument);
    }
    let row = (s.width as usize)
        .checked_mul(format.bytes_per_pixel())
        .ok_or(Error::InvalidArgument)?;
    if s.stride < row {
        return Err(Error::InvalidArgument);
    }
    let len = s
        .stride
        .checked_mul(s.height as usize - 1)
        .and_then(|n| n.checked_add(row))
        .ok_or(Error::InvalidArgument)?;
    if len > isize::MAX as usize {
        return Err(Error::InvalidArgument);
    }
    Ok(len)
}

/// Builds the target surface and optional clip mask for a draw call,
/// rejecting a clip whose bytes overlap the target's, since the target is
/// written while the clip is read.
unsafe fn target_and_clip<'a>(
    target: *const PathlightSurface,
    clip: *const PathlightSurface,
) -> Result<(Surface<'a>, Option<Mask<'a>>), Error> {
    let t = unsafe { target.as_ref() }.ok_or(Error::InvalidArgument)?;
    let format = Format::from_u32(t.format).ok_or(Error::InvalidArgument)?;
    let t_len = byte_len(t, format)?;
    let t_start = t.data as usize;

    let mask = match unsafe { clip.as_ref() } {
        None => None,
        Some(c) => {
            if c.format != PATHLIGHT_FORMAT_A8 {
                return Err(Error::InvalidArgument);
            }
            let c_len = byte_len(c, Format::A8)?;
            let c_start = c.data as usize;
            if c_start < t_start.saturating_add(t_len) && t_start < c_start.saturating_add(c_len) {
                return Err(Error::InvalidArgument);
            }
            let data = unsafe { std::slice::from_raw_parts(c.data, c_len) };
            Some(Mask::new(data, c.width, c.height, c.stride)?)
        }
    };
    let data = unsafe { std::slice::from_raw_parts_mut(t.data, t_len) };
    Ok((
        Surface::new(data, t.width, t.height, t.stride, format)?,
        mask,
    ))
}

unsafe fn paint(p: *const PathlightPaint) -> Result<Paint, Error> {
    let p = unsafe { p.as_ref() }.ok_or(Error::InvalidArgument)?;
    match p.kind {
        PATHLIGHT_PAINT_SOLID if p.ext.is_null() => {
            let c = p.color;
            Ok(Paint::Solid(Color::rgba(c.r, c.g, c.b, c.a)))
        }
        _ => Err(Error::InvalidArgument),
    }
}

unsafe fn transform(t: *const PathlightTransform) -> Transform {
    match unsafe { t.as_ref() } {
        Some(t) => Transform {
            a: t.a,
            b: t.b,
            c: t.c,
            d: t.d,
            e: t.e,
            f: t.f,
        },
        None => Transform::IDENTITY,
    }
}

fn blend(v: u32) -> Result<BlendMode, Error> {
    BlendMode::from_u32(v).ok_or(Error::InvalidArgument)
}

unsafe fn stroke_style(s: *const PathlightStrokeStyle) -> Result<StrokeStyle, Error> {
    let s = unsafe { s.as_ref() }.ok_or(Error::InvalidArgument)?;
    let cap = match s.cap {
        PATHLIGHT_CAP_BUTT => LineCap::Butt,
        PATHLIGHT_CAP_ROUND => LineCap::Round,
        PATHLIGHT_CAP_SQUARE => LineCap::Square,
        _ => return Err(Error::InvalidArgument),
    };
    let join = match s.join {
        PATHLIGHT_JOIN_MITER => LineJoin::Miter,
        PATHLIGHT_JOIN_ROUND => LineJoin::Round,
        PATHLIGHT_JOIN_BEVEL => LineJoin::Bevel,
        _ => return Err(Error::InvalidArgument),
    };
    let dashes = match (s.dashes.is_null(), s.dash_count) {
        (_, 0) => Vec::new(),
        (true, _) => return Err(Error::InvalidArgument),
        (false, n) => unsafe { std::slice::from_raw_parts(s.dashes, n) }.to_vec(),
    };
    Ok(StrokeStyle {
        width: s.width,
        cap,
        join,
        miter_limit: s.miter_limit,
        dashes,
        dash_offset: s.dash_offset,
    })
}

/// Library version as a NUL-terminated string. Never free it.
#[unsafe(no_mangle)]
pub extern "C" fn pathlight_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

/// Creates an empty path. Free it with pathlight_path_free.
#[unsafe(no_mangle)]
pub extern "C" fn pathlight_path_new() -> *mut PathlightPath {
    Box::into_raw(Box::new(PathlightPath(PathBuilder::new())))
}

/// Frees a path. NULL is ignored.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_path_free(path: *mut PathlightPath) {
    if !path.is_null() {
        drop(unsafe { Box::from_raw(path) });
    }
}

unsafe fn with_path(
    path: *mut PathlightPath,
    f: impl FnOnce(&mut PathBuilder) -> Result<(), Error>,
) -> PathlightStatus {
    match unsafe { path.as_mut() } {
        Some(p) => guard(|| f(&mut p.0)),
        None => PATHLIGHT_STATUS_INVALID_ARGUMENT,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_path_move_to(
    path: *mut PathlightPath,
    x: f32,
    y: f32,
) -> PathlightStatus {
    unsafe {
        with_path(path, |b| {
            b.move_to(point(x, y));
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_path_line_to(
    path: *mut PathlightPath,
    x: f32,
    y: f32,
) -> PathlightStatus {
    unsafe {
        with_path(path, |b| {
            b.line_to(point(x, y));
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_path_quad_to(
    path: *mut PathlightPath,
    cx: f32,
    cy: f32,
    x: f32,
    y: f32,
) -> PathlightStatus {
    unsafe {
        with_path(path, |b| {
            b.quad_to(point(cx, cy), point(x, y));
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_path_cubic_to(
    path: *mut PathlightPath,
    c1x: f32,
    c1y: f32,
    c2x: f32,
    c2y: f32,
    x: f32,
    y: f32,
) -> PathlightStatus {
    unsafe {
        with_path(path, |b| {
            b.cubic_to(point(c1x, c1y), point(c2x, c2y), point(x, y));
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_path_close(path: *mut PathlightPath) -> PathlightStatus {
    unsafe {
        with_path(path, |b| {
            b.close();
            Ok(())
        })
    }
}

/// Adds a uniform cubic B-spline as a new subpath. An open spline needs at
/// least 4 control points, a closed one at least 3. The curve does not pass
/// through its end control points; repeat one three times to pin the curve
/// to it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_path_bspline(
    path: *mut PathlightPath,
    points: *const PathlightPoint,
    count: usize,
    closed: bool,
) -> PathlightStatus {
    if points.is_null() {
        return PATHLIGHT_STATUS_INVALID_ARGUMENT;
    }
    let pts: Vec<Point> = unsafe { std::slice::from_raw_parts(points, count) }
        .iter()
        .map(|p| point(p.x, p.y))
        .collect();
    unsafe {
        with_path(path, |b| {
            if b.bspline(&pts, closed) {
                Ok(())
            } else {
                Err(Error::InvalidArgument)
            }
        })
    }
}

/// Fills `path`. `transform` NULL means identity; `clip` NULL means no clip,
/// otherwise it must be an A8 surface.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_fill(
    target: *const PathlightSurface,
    path: *const PathlightPath,
    fill_rule: u32,
    paint_: *const PathlightPaint,
    transform_: *const PathlightTransform,
    blend_mode: u32,
    clip_: *const PathlightSurface,
) -> PathlightStatus {
    guard(|| unsafe {
        let path = path.as_ref().ok_or(Error::InvalidArgument)?;
        let rule = match fill_rule {
            PATHLIGHT_FILL_NONZERO => FillRule::NonZero,
            PATHLIGHT_FILL_EVENODD => FillRule::EvenOdd,
            _ => return Err(Error::InvalidArgument),
        };
        let (mut target, clip) = target_and_clip(target, clip_)?;
        pathlight::fill(
            &mut target,
            path.0.as_path(),
            rule,
            &paint(paint_)?,
            &transform(transform_),
            blend(blend_mode)?,
            clip.as_ref(),
        )
    })
}

/// Strokes `path`. See pathlight_fill for `transform` and `clip`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_stroke(
    target: *const PathlightSurface,
    path: *const PathlightPath,
    style: *const PathlightStrokeStyle,
    paint_: *const PathlightPaint,
    transform_: *const PathlightTransform,
    blend_mode: u32,
    clip_: *const PathlightSurface,
) -> PathlightStatus {
    guard(|| unsafe {
        let path = path.as_ref().ok_or(Error::InvalidArgument)?;
        let (mut target, clip) = target_and_clip(target, clip_)?;
        pathlight::stroke(
            &mut target,
            path.0.as_path(),
            &stroke_style(style)?,
            &paint(paint_)?,
            &transform(transform_),
            blend(blend_mode)?,
            clip.as_ref(),
        )
    })
}

/// Loads face `index` from `len` bytes of font data, which are copied. On
/// success `*out` receives a font to free with pathlight_font_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_font_new(
    data: *const u8,
    len: usize,
    index: u32,
    out: *mut *mut PathlightFont,
) -> PathlightStatus {
    if data.is_null() || out.is_null() {
        return PATHLIGHT_STATUS_INVALID_ARGUMENT;
    }
    guard(|| unsafe {
        let font = Font::from_bytes(std::slice::from_raw_parts(data, len), index)?;
        *out = Box::into_raw(Box::new(PathlightFont(font)));
        Ok(())
    })
}

/// Frees a font. NULL is ignored.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_font_free(font: *mut PathlightFont) {
    if !font.is_null() {
        drop(unsafe { Box::from_raw(font) });
    }
}

/// Draws `len` bytes of UTF-8 `text` on one line, baseline origin at (x, y),
/// `size` pixels per em. See pathlight_fill for `transform` and `clip`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pathlight_draw_text(
    target: *const PathlightSurface,
    font: *const PathlightFont,
    text: *const c_char,
    len: usize,
    size: f32,
    x: f32,
    y: f32,
    paint_: *const PathlightPaint,
    transform_: *const PathlightTransform,
    blend_mode: u32,
    clip_: *const PathlightSurface,
) -> PathlightStatus {
    guard(|| unsafe {
        let font = font.as_ref().ok_or(Error::InvalidArgument)?;
        if text.is_null() {
            return Err(Error::InvalidArgument);
        }
        let text = std::str::from_utf8(std::slice::from_raw_parts(text.cast(), len))
            .map_err(|_| Error::InvalidArgument)?;
        let (mut target, clip) = target_and_clip(target, clip_)?;
        pathlight::draw_text(
            &mut target,
            &font.0,
            text,
            size,
            x,
            y,
            &paint(paint_)?,
            &transform(transform_),
            blend(blend_mode)?,
            clip.as_ref(),
        )
    })
}
