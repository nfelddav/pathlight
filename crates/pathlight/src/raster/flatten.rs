//! Curve flattening in device space.

use crate::geom::{Point, Transform};
use crate::path::{Path, Verb};

/// A line segment in device space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Line {
    pub p0: Point,
    pub p1: Point,
}

/// Device coordinates are clamped to this magnitude. Finite user-space
/// points can still overflow under a large transform, and nothing this far
/// out changes which pixels on a real surface are covered by more than
/// rounding.
const MAX_COORD: f32 = 1e9;

/// Upper bound on the lines one curve is split into, so pathological
/// coordinates cannot blow up memory. Curves that need more are already far
/// larger than any surface and have been culled to chords.
const MAX_SUBDIVISIONS: u32 = 1 << 12;

/// Transforms `path` to device space and appends its outline as lines.
/// Every subpath is closed, as filling requires.
///
/// A curve whose control points all lie outside the `width` × `height`
/// surface is replaced by its chord: nothing of it is visible, and the only
/// thing it can contribute (winding from the left of the surface) depends
/// only on its endpoints.
pub(crate) fn flatten(
    path: &Path,
    t: &Transform,
    tolerance: f32,
    width: f32,
    height: f32,
    out: &mut Vec<Line>,
) {
    let pts = path.points();
    let device = |p: Point| {
        let q = t.apply(p);
        let fix = |v: f32| {
            if v.is_nan() {
                0.0
            } else {
                v.clamp(-MAX_COORD, MAX_COORD)
            }
        };
        Point::new(fix(q.x), fix(q.y))
    };
    let mut pi = 0;
    let mut start = Point::default();
    let mut cur = Point::default();
    let line = |out: &mut Vec<Line>, p0: Point, p1: Point| {
        if p0.y != p1.y {
            out.push(Line { p0, p1 });
        }
    };
    let visible = |ps: &[Point]| {
        let (mut x0, mut y0, mut x1, mut y1) = (
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        );
        for p in ps {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        x1 >= 0.0 && x0 <= width && y1 >= 0.0 && y0 <= height
    };

    for verb in path.verbs() {
        match verb {
            Verb::MoveTo => {
                line(out, cur, start);
                start = device(pts[pi]);
                cur = start;
                pi += 1;
            }
            Verb::LineTo => {
                let p = device(pts[pi]);
                line(out, cur, p);
                cur = p;
                pi += 1;
            }
            Verb::QuadTo => {
                let (c, p) = (device(pts[pi]), device(pts[pi + 1]));
                if visible(&[cur, c, p]) {
                    let dd = (cur - c * 2.0 + p).len();
                    // A chord over a parameter step h deviates from a quadratic
                    // by at most |B''| h² / 8 = |dd| h² / 4.
                    let n = subdivisions((dd / (4.0 * tolerance)).sqrt());
                    let mut prev = cur;
                    for i in 1..=n {
                        let s = i as f32 / n as f32;
                        let q = if i == n { p } else { quad_at(cur, c, p, s) };
                        line(out, prev, q);
                        prev = q;
                    }
                } else {
                    line(out, cur, p);
                }
                cur = p;
                pi += 2;
            }
            Verb::CubicTo => {
                let (c1, c2, p) = (device(pts[pi]), device(pts[pi + 1]), device(pts[pi + 2]));
                if visible(&[cur, c1, c2, p]) {
                    // Wang's formula for uniform subdivision of a cubic.
                    let dd = (cur - c1 * 2.0 + c2).len().max((c1 - c2 * 2.0 + p).len());
                    let n = subdivisions((0.75 * dd / tolerance).sqrt());
                    let mut prev = cur;
                    for i in 1..=n {
                        let s = i as f32 / n as f32;
                        let q = if i == n {
                            p
                        } else {
                            cubic_at(cur, c1, c2, p, s)
                        };
                        line(out, prev, q);
                        prev = q;
                    }
                } else {
                    line(out, cur, p);
                }
                cur = p;
                pi += 3;
            }
            Verb::Close => {
                line(out, cur, start);
                cur = start;
            }
        }
    }
    line(out, cur, start);
}

fn subdivisions(n: f32) -> u32 {
    if n.is_finite() {
        (n.ceil() as u32).clamp(1, MAX_SUBDIVISIONS)
    } else {
        MAX_SUBDIVISIONS
    }
}

fn quad_at(p0: Point, c: Point, p1: Point, t: f32) -> Point {
    let mt = 1.0 - t;
    p0 * (mt * mt) + c * (2.0 * mt * t) + p1 * (t * t)
}

fn cubic_at(p0: Point, c1: Point, c2: Point, p1: Point, t: f32) -> Point {
    let mt = 1.0 - t;
    p0 * (mt * mt * mt) + c1 * (3.0 * mt * mt * t) + c2 * (3.0 * mt * t * t) + p1 * (t * t * t)
}
