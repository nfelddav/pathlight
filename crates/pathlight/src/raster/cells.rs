//! Line walking into sparse signed-area cells.
//!
//! Each cell holds a contribution to the running sum along its pixel row:
//! summing a row's cells from left to right gives the signed coverage at each
//! pixel (the accumulation scheme of font-rs, kept sparse). Only pixels a
//! line actually crosses get cells.

use super::TILE_HEIGHT;
use super::flatten::Line;
use crate::geom::Point;

/// A cell: a packed sort key (band, x, row-in-band) and its contribution.
pub(crate) type Cell = (u64, f32);

// The band takes the top 28 bits of the key, which every height up to
// MAX_HEIGHT fits.
const _: () = assert!((crate::surface::MAX_HEIGHT / TILE_HEIGHT) as u64 <= 1 << 28);

pub(crate) fn key(x: u32, y: u32) -> u64 {
    let band = (y / TILE_HEIGHT) as u64;
    let row = (y % TILE_HEIGHT) as u64;
    (band << 36) | ((x as u64) << 2) | row
}

pub(crate) fn unpack(key: u64) -> (u32, u32, u32) {
    (
        (key >> 36) as u32,
        ((key >> 2) & 0x3_ffff_ffff) as u32,
        (key & 3) as u32,
    )
}

/// Appends the cells for `line` on a `width` × `height` surface.
///
/// The line is first split where it crosses x = 0 and x = width. Parts left
/// of the surface still change the winding of everything to their right, so
/// they are moved onto x = 0 as vertical lines. Parts right of the surface
/// cannot affect any pixel and are dropped.
pub(crate) fn line_cells(line: Line, width: u32, height: u32, out: &mut Vec<Cell>) {
    let Line { p0, p1 } = line;
    let w = width as f32;
    let mut ts = [0.0, 1.0, 1.0, 1.0];
    let mut n = 1;
    if p0.x != p1.x {
        for edge in [0.0, w] {
            let t = (edge - p0.x) / (p1.x - p0.x);
            if t > 0.0 && t < 1.0 {
                ts[n] = t;
                n += 1;
            }
        }
    }
    ts[n] = 1.0;
    ts[1..n].sort_by(f32::total_cmp);
    for i in 0..n {
        let (t0, t1) = (ts[i], ts[i + 1]);
        let a = lerp(p0, p1, t0);
        let b = lerp(p0, p1, t1);
        let mid = 0.5 * (a.x + b.x);
        if mid > w {
            continue;
        }
        let (a, b) = if mid < 0.0 {
            (Point::new(0.0, a.y), Point::new(0.0, b.y))
        } else {
            (
                Point::new(a.x.clamp(0.0, w), a.y),
                Point::new(b.x.clamp(0.0, w), b.y),
            )
        };
        accumulate(a, b, width, height, out);
    }
}

fn lerp(a: Point, b: Point, t: f32) -> Point {
    if t == 0.0 {
        a
    } else if t == 1.0 {
        b
    } else {
        a + (b - a) * t
    }
}

/// Adds the signed area of a line with `0 <= x <= width` to the cells.
fn accumulate(p0: Point, p1: Point, width: u32, height: u32, out: &mut Vec<Cell>) {
    if p0.y == p1.y {
        return;
    }
    let (dir, a, b) = if p0.y < p1.y {
        (1.0, p0, p1)
    } else {
        (-1.0, p1, p0)
    };
    let y_start = a.y.max(0.0);
    let y_end = b.y.min(height as f32);
    if y_start >= y_end {
        return;
    }
    let dxdy = (b.x - a.x) / (b.y - a.y);
    if !dxdy.is_finite() {
        // So close to horizontal that its area contribution rounds to zero.
        return;
    }
    // x moves monotonically from a.x to b.x; clamping keeps float error from
    // stepping outside that range (and so outside the surface).
    let (x_lo, x_hi) = (a.x.min(b.x), a.x.max(b.x));
    let mut push = |x: i64, y: u32, v: f32| {
        if x >= 0 && (x as u64) < width as u64 && v != 0.0 {
            out.push((key(x as u32, y), v));
        }
    };
    let mut x = (a.x + (y_start - a.y) * dxdy).clamp(x_lo, x_hi);
    let y_first = y_start as u32;
    let y_last = (y_end.ceil() as u32).min(height);
    for y in y_first..y_last {
        let yf = y as f32;
        let dy = (yf + 1.0).min(y_end) - yf.max(y_start);
        if dy <= 0.0 {
            continue;
        }
        let x_next = (x + dxdy * dy).clamp(x_lo, x_hi);
        let d = dy * dir;
        let (x0, x1) = if x < x_next { (x, x_next) } else { (x_next, x) };
        let x0_floor = x0.floor();
        let x0i = x0_floor as i64;
        let x1_ceil = x1.ceil();
        let x1i = x1_ceil as i64;
        if x1i <= x0i + 1 {
            // The line stays within one pixel column on this row.
            let xmf = 0.5 * (x + x_next) - x0_floor;
            push(x0i, y, d * (1.0 - xmf));
            push(x0i + 1, y, d * xmf);
        } else {
            let s = (x1 - x0).recip();
            let x0f = x0 - x0_floor;
            let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
            let x1f = x1 - x1_ceil + 1.0;
            let am = 0.5 * s * x1f * x1f;
            push(x0i, y, d * a0);
            if x1i == x0i + 2 {
                push(x0i + 1, y, d * (1.0 - a0 - am));
            } else {
                let a1 = s * (1.5 - x0f);
                push(x0i + 1, y, d * (a1 - a0));
                for xi in x0i + 2..x1i - 1 {
                    push(xi, y, d * s);
                }
                let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                push(x1i - 1, y, d * (1.0 - a2 - am));
            }
            push(x1i, y, d * am);
        }
        x = x_next;
    }
}
