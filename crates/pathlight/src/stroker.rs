//! Turns a path and a [`StrokeStyle`] into an outline that can be filled.
//!
//! The path is flattened in user space, dashed if asked, and every piece of
//! the stroke (one quad per segment, a wedge per join, a cap per open end) is
//! emitted as its own convex polygon with the same orientation. Filling the
//! result with the non-zero rule gives their union, which stays correct
//! through cusps, self-intersections and joins tighter than the stroke width
//! without any offset-curve bookkeeping. Where pieces overlap along an
//! antialiased edge the edge can come out slightly darker than an exact
//! offset would make it.

use crate::error::Error;
use crate::geom::{Point, Transform};
use crate::path::{Path, PathBuilder, Verb};
use crate::raster::flatten::{cubic_at, cubic_subdivisions, quad_at, quad_subdivisions};
use crate::stroke::{LineCap, LineJoin, StrokeStyle};

/// Upper bound on the dashes one stroke may produce, so a tiny dash pattern
/// on a long path cannot exhaust memory.
const MAX_DASHES: f32 = 1e6;

/// A flattened subpath. `smooth[i]` marks vertices that lie inside a curve,
/// where the stroke always uses a round join so it stays smooth.
#[derive(Clone, Debug, Default)]
struct Polyline {
    pts: Vec<Point>,
    smooth: Vec<bool>,
    closed: bool,
}

impl Polyline {
    fn push(&mut self, p: Point, smooth: bool) {
        if self.pts.last() != Some(&p) {
            self.pts.push(p);
            self.smooth.push(smooth);
        }
    }
}

/// Builds the fillable outline of `path` stroked with `style`. `tolerance`
/// is the allowed flattening error in device pixels and `transform` maps
/// user space to device space; the outline itself stays in user space.
pub(crate) fn stroke_outline(
    path: &Path,
    style: &StrokeStyle,
    transform: &Transform,
    tolerance: f32,
) -> Result<Path, Error> {
    let scale = max_scale(transform);
    let mut out = PathBuilder::new();
    if scale == 0.0 {
        return Ok(out.finish());
    }
    let tol = tolerance / scale;
    let mut lines = polylines(path, tol);
    if !style.dashes.is_empty() {
        lines = dash(&lines, &style.dashes, style.dash_offset)?;
    }
    let mut s = Stroker {
        hw: style.width * 0.5,
        style,
        tol,
        out: &mut out,
    };
    for line in &lines {
        s.polyline(line);
    }
    Ok(out.finish())
}

/// The largest factor by which `t` stretches any length.
fn max_scale(t: &Transform) -> f32 {
    let p = t.a * t.a + t.b * t.b;
    let q = t.c * t.c + t.d * t.d;
    let r = t.a * t.c + t.b * t.d;
    // Largest eigenvalue of MᵀM = [[p, r], [r, q]].
    let mean = 0.5 * (p + q);
    let spread = (0.25 * (p - q) * (p - q) + r * r).sqrt();
    (mean + spread).sqrt()
}

fn polylines(path: &Path, tol: f32) -> Vec<Polyline> {
    let pts = path.points();
    let mut out = Vec::new();
    let mut cur = Polyline::default();
    // A lone move_to draws nothing; a subpath with any drawing verb does,
    // even if it has zero length (round and square caps then draw a dot).
    let mut drawn = false;
    let mut pi = 0;
    let mut last = Point::default();
    let finish = |cur: &mut Polyline, drawn: bool, out: &mut Vec<Polyline>| {
        let line = std::mem::take(cur);
        if drawn && !line.pts.is_empty() {
            out.push(line);
        }
    };
    for verb in path.verbs() {
        match verb {
            Verb::MoveTo => {
                finish(&mut cur, drawn, &mut out);
                drawn = false;
                last = pts[pi];
                cur.push(last, false);
                pi += 1;
            }
            Verb::LineTo => {
                last = pts[pi];
                cur.push(last, false);
                drawn = true;
                pi += 1;
            }
            Verb::QuadTo => {
                let (c, p) = (pts[pi], pts[pi + 1]);
                let n = quad_subdivisions(last, c, p, tol);
                for i in 1..n {
                    cur.push(quad_at(last, c, p, i as f32 / n as f32), true);
                }
                cur.push(p, false);
                last = p;
                drawn = true;
                pi += 2;
            }
            Verb::CubicTo => {
                let (c1, c2, p) = (pts[pi], pts[pi + 1], pts[pi + 2]);
                let n = cubic_subdivisions(last, c1, c2, p, tol);
                for i in 1..n {
                    cur.push(cubic_at(last, c1, c2, p, i as f32 / n as f32), true);
                }
                cur.push(p, false);
                last = p;
                drawn = true;
                pi += 3;
            }
            Verb::Close => {
                if cur.pts.len() > 1 && cur.pts.first() == cur.pts.last() {
                    cur.pts.pop();
                    cur.smooth.pop();
                }
                cur.closed = cur.pts.len() > 1;
                let start = cur.pts.first().copied();
                finish(&mut cur, true, &mut out);
                drawn = false;
                if let Some(start) = start {
                    // Drawing after a close continues from the subpath start.
                    last = start;
                    cur.push(start, false);
                }
            }
        }
    }
    finish(&mut cur, drawn, &mut out);
    out
}

/// Splits polylines into open dashes. Each subpath restarts the pattern at
/// `offset`. On a closed subpath, a dash running over the start point is
/// joined into one.
fn dash(lines: &[Polyline], dashes: &[f32], offset: f32) -> Result<Vec<Polyline>, Error> {
    // An odd-length pattern repeats to make an even one, as in SVG.
    let pattern: Vec<f32> = if dashes.len() % 2 == 1 {
        [dashes, dashes].concat()
    } else {
        dashes.to_vec()
    };
    let total: f32 = pattern.iter().sum();
    let length: f32 = lines.iter().map(polyline_length).sum();
    if length / total * pattern.len() as f32 > MAX_DASHES {
        return Err(Error::InvalidArgument);
    }
    // Find where in the pattern `offset` lands.
    let mut phase_idx = 0;
    let mut phase_left = pattern[0];
    let mut skip = offset.rem_euclid(total);
    while skip > 0.0 {
        if skip < phase_left {
            phase_left -= skip;
            break;
        }
        skip -= phase_left;
        phase_idx = (phase_idx + 1) % pattern.len();
        phase_left = pattern[phase_idx];
    }

    let mut out = Vec::new();
    for line in lines {
        let n = line.pts.len();
        let first_out = out.len();
        let mut idx = phase_idx;
        let mut left = phase_left;
        let starts_on = idx % 2 == 0;
        let mut current: Option<Polyline> = starts_on.then(|| {
            let mut d = Polyline::default();
            d.push(line.pts[0], false);
            d
        });
        let segments = if line.closed { n } else { n - 1 };
        for i in 0..segments {
            let a = line.pts[i];
            let b = line.pts[(i + 1) % n];
            let end_smooth = line.smooth[(i + 1) % n];
            let seg_len = (b - a).len();
            let mut pos = 0.0;
            while pos < seg_len {
                let step = left.min(seg_len - pos);
                pos += step;
                left -= step;
                let at_end = pos >= seg_len;
                let p = if at_end {
                    b
                } else {
                    a + (b - a) * (pos / seg_len)
                };
                if let Some(d) = current.as_mut() {
                    d.push(p, at_end && end_smooth);
                }
                if left <= 0.0 {
                    // This dash or gap is used up: switch to the next.
                    if let Some(d) = current.take() {
                        out.push(d);
                    }
                    idx = (idx + 1) % pattern.len();
                    left = pattern[idx];
                    if idx % 2 == 0 {
                        let mut d = Polyline::default();
                        d.push(p, false);
                        current = Some(d);
                    }
                }
            }
        }
        if let Some(mut d) = current.take() {
            // A dash still open at the end of a closed subpath that also
            // began on a dash continues into that first dash.
            if line.closed && starts_on && out.len() > first_out {
                let first = &out[first_out];
                for (k, &p) in first.pts.iter().enumerate() {
                    d.push(p, first.smooth[k]);
                }
                out[first_out] = d;
            } else if line.closed && starts_on {
                // The whole loop is one dash.
                let mut whole = line.clone();
                whole.closed = true;
                out.push(whole);
            } else {
                out.push(d);
            }
        }
    }
    Ok(out)
}

fn polyline_length(line: &Polyline) -> f32 {
    let mut len: f32 = line.pts.windows(2).map(|w| (w[1] - w[0]).len()).sum();
    if line.closed && line.pts.len() > 1 {
        len += (line.pts[0] - line.pts[line.pts.len() - 1]).len();
    }
    len
}

struct Stroker<'a> {
    hw: f32,
    style: &'a StrokeStyle,
    tol: f32,
    out: &'a mut PathBuilder,
}

fn unit(v: Point) -> Point {
    v * v.len().recip()
}

/// The left-hand normal of a unit direction.
fn normal(d: Point) -> Point {
    Point::new(-d.y, d.x)
}

fn cross(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}

fn dot(a: Point, b: Point) -> f32 {
    a.x * b.x + a.y * b.y
}

impl Stroker<'_> {
    fn polyline(&mut self, line: &Polyline) {
        let pts = &line.pts;
        let n = pts.len();
        let hw = self.hw;
        if n == 1 {
            let p = pts[0];
            match self.style.cap {
                LineCap::Butt => {}
                LineCap::Round => self.fan(p, Point::new(hw, 0.0), std::f32::consts::TAU),
                LineCap::Square => self.polygon(&[
                    p + Point::new(-hw, -hw),
                    p + Point::new(hw, -hw),
                    p + Point::new(hw, hw),
                    p + Point::new(-hw, hw),
                ]),
            }
            return;
        }
        let segments = if line.closed { n } else { n - 1 };
        let dir = |i: usize| unit(pts[(i + 1) % n] - pts[i]);
        for i in 0..segments {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let nrm = normal(dir(i)) * hw;
            self.polygon(&[a + nrm, b + nrm, b - nrm, a - nrm]);
        }
        let joins = if line.closed { 0..n } else { 1..n - 1 };
        for k in joins {
            let prev = (k + n - 1) % n;
            let join = if line.smooth[k] {
                LineJoin::Round
            } else {
                self.style.join
            };
            self.join(pts[k], dir(prev), dir(k), join);
        }
        if !line.closed {
            self.cap(pts[0], dir(0) * -1.0);
            self.cap(pts[n - 1], dir(n - 2));
        }
    }

    /// Adds a cap at `p`, where `d` is the unit direction pointing out of the
    /// line.
    fn cap(&mut self, p: Point, d: Point) {
        let nrm = normal(d) * self.hw;
        match self.style.cap {
            LineCap::Butt => {}
            LineCap::Square => {
                let ext = d * self.hw;
                self.polygon(&[p + nrm, p + nrm + ext, p - nrm + ext, p - nrm]);
            }
            LineCap::Round => self.fan(p, nrm * -1.0, std::f32::consts::PI),
        }
    }

    /// Fills the gap on the outer side of the corner at `p` between a
    /// segment arriving along `d0` and one leaving along `d1`.
    fn join(&mut self, p: Point, d0: Point, d1: Point, join: LineJoin) {
        let turn = cross(d0, d1);
        let cos = dot(d0, d1);
        if turn.abs() <= 1e-6 && cos > 0.0 {
            return;
        }
        // The outer side is opposite the direction of the turn.
        let side = if turn > 0.0 { -1.0 } else { 1.0 };
        let n0 = normal(d0) * side;
        let n1 = normal(d1) * side;
        let (a, b) = (p + n0 * self.hw, p + n1 * self.hw);
        match join {
            LineJoin::Bevel => self.polygon(&[p, a, b]),
            LineJoin::Miter => {
                // Ratio of miter length to stroke width is 1 / sin(θ/2) for
                // the angle θ between the segments, i.e. 1 / cos(turn/2).
                let half_cos = (0.5 * (1.0 + cos)).max(0.0).sqrt();
                if half_cos > 0.0 && half_cos.recip() <= self.style.miter_limit {
                    let tip = p + unit(n0 + n1) * (self.hw / half_cos);
                    self.polygon(&[p, a, tip, b]);
                } else {
                    self.polygon(&[p, a, b]);
                }
            }
            LineJoin::Round => {
                let sweep = cross(n0, n1).atan2(dot(n0, n1));
                self.fan(p, n0 * self.hw, sweep);
            }
        }
    }

    /// Adds a pie slice centered on `c`, starting at offset `from` and
    /// sweeping `sweep` radians.
    fn fan(&mut self, c: Point, from: Point, sweep: f32) {
        let r = from.len();
        // Inner vertices sit slightly outside the circle, at the radius that
        // gives the fan the same area as the sector. The polygon then
        // straddles the arc instead of falling a full tolerance short, as an
        // inscribed one would. The two end vertices stay on the circle so
        // they meet the segment edges exactly.
        let step = 2.0 * (1.0 - (self.tol / r).min(1.0)).acos();
        let n = ((sweep.abs() / step.max(0.05)).ceil() as usize).clamp(2, 1024);
        let angle = sweep / n as f32;
        let (sin, cos) = angle.sin_cos();
        let s = sin.abs();
        let t = angle.abs();
        // Fan area with inner radius r·x: two end triangles ½·r²·x·s and
        // n - 2 inner ones ½·r²·x²·s, set equal to the sector ½·r²·n·t.
        let outer = if n == 2 {
            t / s
        } else {
            let m = (n - 2) as f32;
            (-s + (s * s + m * s * n as f32 * t).sqrt()) / (m * s)
        };
        let mut pts = Vec::with_capacity(n + 2);
        pts.push(c);
        pts.push(c + from);
        let mut v = from;
        for _ in 1..n {
            v = Point::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
            pts.push(c + v * outer);
        }
        v = Point::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
        pts.push(c + v);
        self.polygon(&pts);
    }

    /// Adds a closed polygon, reversed if needed so that every piece winds
    /// the same way and the non-zero fill takes their union.
    fn polygon(&mut self, pts: &[Point]) {
        let mut area = 0.0;
        for i in 0..pts.len() {
            area += cross(pts[i], pts[(i + 1) % pts.len()]);
        }
        if !area.is_finite() || area == 0.0 {
            return;
        }
        let mut emit = |p: Point, first: bool| {
            if first {
                self.out.move_to(p);
            } else {
                self.out.line_to(p);
            }
        };
        if area > 0.0 {
            for (i, &p) in pts.iter().enumerate() {
                emit(p, i == 0);
            }
        } else {
            for (i, &p) in pts.iter().rev().enumerate() {
                emit(p, i == 0);
            }
        }
        self.out.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blend::BlendMode;
    use crate::paint::{Color, Paint};
    use crate::render::stroke;
    use crate::surface::{Format, Surface};

    const BLACK: Paint = Paint::Solid(Color::rgba(0.0, 0.0, 0.0, 1.0));

    fn style(width: f32, cap: LineCap, join: LineJoin) -> StrokeStyle {
        StrokeStyle {
            width,
            cap,
            join,
            ..StrokeStyle::default()
        }
    }

    fn render(path: &Path, style: &StrokeStyle, t: &Transform, w: u32, h: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (w * h) as usize];
        let mut s = Surface::new(&mut buf, w, h, w as usize, Format::A8).unwrap();
        stroke(&mut s, path, style, &BLACK, t, BlendMode::Normal, None).unwrap();
        buf
    }

    fn area(img: &[u8]) -> f32 {
        img.iter().map(|&a| a as f32 / 255.0).sum()
    }

    fn line(x0: f32, y0: f32, x1: f32, y1: f32) -> Path {
        let mut b = PathBuilder::new();
        b.move_to(Point::new(x0, y0)).line_to(Point::new(x1, y1));
        b.finish()
    }

    fn square(x0: f32, y0: f32, x1: f32, y1: f32) -> Path {
        let mut b = PathBuilder::new();
        b.move_to(Point::new(x0, y0))
            .line_to(Point::new(x1, y0))
            .line_to(Point::new(x1, y1))
            .line_to(Point::new(x0, y1))
            .close();
        b.finish()
    }

    fn circle(cx: f32, cy: f32, r: f32) -> Path {
        let k = 0.552_284_8 * r;
        let p = |x: f32, y: f32| Point::new(cx + x, cy + y);
        let mut b = PathBuilder::new();
        b.move_to(p(r, 0.0));
        b.cubic_to(p(r, k), p(k, r), p(0.0, r));
        b.cubic_to(p(-k, r), p(-r, k), p(-r, 0.0));
        b.cubic_to(p(-r, -k), p(-k, -r), p(0.0, -r));
        b.cubic_to(p(k, -r), p(r, -k), p(r, 0.0));
        b.close();
        b.finish()
    }

    const ID: Transform = Transform::IDENTITY;

    #[test]
    fn butt_line_is_an_exact_rectangle() {
        let img = render(
            &line(2.0, 5.0, 18.0, 5.0),
            &style(4.0, LineCap::Butt, LineJoin::Miter),
            &ID,
            20,
            10,
        );
        for y in 0..10 {
            for x in 0..20 {
                let inside = (2..18).contains(&x) && (3..7).contains(&y);
                assert_eq!(img[y * 20 + x], if inside { 255 } else { 0 }, "({x}, {y})");
            }
        }
    }

    #[test]
    fn caps_extend_the_line() {
        let l = line(4.0, 5.0, 16.0, 5.0);
        let butt = area(&render(
            &l,
            &style(4.0, LineCap::Butt, LineJoin::Miter),
            &ID,
            20,
            10,
        ));
        let square = area(&render(
            &l,
            &style(4.0, LineCap::Square, LineJoin::Miter),
            &ID,
            20,
            10,
        ));
        let round = area(&render(
            &l,
            &style(4.0, LineCap::Round, LineJoin::Miter),
            &ID,
            20,
            10,
        ));
        assert!((butt - 48.0).abs() < 0.01, "{butt}");
        assert!((square - 64.0).abs() < 0.01, "{square}");
        let want = 48.0 + std::f32::consts::PI * 4.0;
        assert!((round - want).abs() < 0.3, "{round} vs {want}");
    }

    #[test]
    fn joins_on_a_closed_square() {
        let sq = square(5.0, 5.0, 15.0, 15.0);
        let get = |join| area(&render(&sq, &style(2.0, LineCap::Butt, join), &ID, 20, 20));
        // Outer 12×12 minus inner 8×8, minus the corner cut by each join.
        assert!((get(LineJoin::Miter) - 80.0).abs() < 0.01);
        assert!((get(LineJoin::Bevel) - 78.0).abs() < 0.01);
        let round = 80.0 - 4.0 * (1.0 - std::f32::consts::FRAC_PI_4);
        assert!((get(LineJoin::Round) - round).abs() < 0.1);
    }

    #[test]
    fn miter_limit_falls_back_to_bevel() {
        let mut b = PathBuilder::new();
        b.move_to(Point::new(5.0, 35.0))
            .line_to(Point::new(20.0, 5.0))
            .line_to(Point::new(35.0, 35.0));
        let v = b.finish();
        let mut s = style(4.0, LineCap::Butt, LineJoin::Miter);
        s.miter_limit = 10.0;
        let miter = area(&render(&v, &s, &ID, 40, 40));
        s.miter_limit = 1.5;
        let limited = area(&render(&v, &s, &ID, 40, 40));
        let bevel = area(&render(
            &v,
            &style(4.0, LineCap::Butt, LineJoin::Bevel),
            &ID,
            40,
            40,
        ));
        assert!(miter > bevel + 1.0, "{miter} vs {bevel}");
        assert!((limited - bevel).abs() < 0.01);
    }

    #[test]
    fn dashes_and_offset() {
        let l = line(0.0, 1.0, 16.0, 1.0);
        let mut s = style(2.0, LineCap::Butt, LineJoin::Miter);
        s.dashes = vec![4.0, 4.0];
        let row = |img: &[u8]| img[16..32].to_vec();
        let on = |xs: &[std::ops::Range<usize>]| {
            (0..16)
                .map(|x| {
                    if xs.iter().any(|r| r.contains(&x)) {
                        255
                    } else {
                        0
                    }
                })
                .collect::<Vec<u8>>()
        };
        assert_eq!(row(&render(&l, &s, &ID, 16, 3)), on(&[0..4, 8..12]));
        s.dash_offset = 2.0;
        assert_eq!(row(&render(&l, &s, &ID, 16, 3)), on(&[0..2, 6..10, 14..16]));
        // An odd pattern repeats: [4] means 4 on, 4 off.
        s.dashes = vec![4.0];
        s.dash_offset = 0.0;
        assert_eq!(row(&render(&l, &s, &ID, 16, 3)), on(&[0..4, 8..12]));
        // Negative offsets wrap.
        s.dash_offset = -2.0;
        assert_eq!(row(&render(&l, &s, &ID, 16, 3)), on(&[2..6, 10..14]));
    }

    #[test]
    fn dash_over_the_start_of_a_closed_path_is_one_dash() {
        let lines = polylines(&square(0.0, 0.0, 10.0, 10.0), 0.1);
        // Perimeter 40, offset 5: dashes at 0..5, 15..25 and 35..40. The
        // last is still running at the end and joins the first across the
        // start corner.
        let dashes = dash(&lines, &[10.0, 10.0], 5.0).unwrap();
        assert_eq!(dashes.len(), 2);
        let merged = &dashes[0];
        assert_eq!(merged.pts.first(), Some(&Point::new(0.0, 5.0)));
        assert_eq!(merged.pts.last(), Some(&Point::new(5.0, 0.0)));
        assert!(merged.pts.contains(&Point::new(0.0, 0.0)));
    }

    #[test]
    fn zero_length_subpath_draws_a_dot_with_round_caps() {
        let mut b = PathBuilder::new();
        b.move_to(Point::new(10.0, 10.0)).close();
        let dot = b.finish();
        let round = area(&render(
            &dot,
            &style(6.0, LineCap::Round, LineJoin::Miter),
            &ID,
            20,
            20,
        ));
        assert!((round - std::f32::consts::PI * 9.0).abs() < 0.3, "{round}");
        let square = area(&render(
            &dot,
            &style(6.0, LineCap::Square, LineJoin::Miter),
            &ID,
            20,
            20,
        ));
        assert!((square - 36.0).abs() < 0.01);
        assert_eq!(
            area(&render(
                &dot,
                &style(6.0, LineCap::Butt, LineJoin::Miter),
                &ID,
                20,
                20
            )),
            0.0
        );
        // A lone move_to draws nothing.
        let mut b = PathBuilder::new();
        b.move_to(Point::new(10.0, 10.0));
        assert_eq!(
            area(&render(
                &b.finish(),
                &style(6.0, LineCap::Round, LineJoin::Miter),
                &ID,
                20,
                20
            )),
            0.0
        );
    }

    #[test]
    fn stroke_is_offset_in_user_space() {
        let l = line(2.0, 0.0, 2.0, 8.0);
        let img = render(
            &l,
            &style(1.0, LineCap::Butt, LineJoin::Miter),
            &Transform::scale(4.0, 1.0),
            16,
            8,
        );
        for y in 0..8 {
            assert_eq!(
                &img[y * 16..y * 16 + 16],
                &[0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0]
            );
        }
    }

    #[test]
    fn stroked_circle_is_an_annulus() {
        let img = render(
            &circle(32.0, 32.0, 20.0),
            &style(4.0, LineCap::Butt, LineJoin::Miter),
            &ID,
            64,
            64,
        );
        let want = std::f32::consts::PI * (22.0 * 22.0 - 18.0 * 18.0);
        assert!(
            ((area(&img) - want) / want).abs() < 0.01,
            "{} vs {want}",
            area(&img)
        );
        // Centre stays empty, the ring is solid.
        assert_eq!(img[32 * 64 + 32], 0);
        assert_eq!(img[32 * 64 + 52], 255);
    }

    #[test]
    fn self_crossing_stroke_has_no_holes() {
        let mut b = PathBuilder::new();
        b.move_to(Point::new(2.0, 2.0))
            .line_to(Point::new(18.0, 18.0))
            .line_to(Point::new(18.0, 2.0))
            .line_to(Point::new(2.0, 18.0))
            .close();
        let img = render(
            &b.finish(),
            &style(3.0, LineCap::Butt, LineJoin::Round),
            &ID,
            20,
            20,
        );
        assert_eq!(img[10 * 20 + 10], 255);
    }

    #[test]
    fn too_many_dashes_is_an_error() {
        let mut s = style(1.0, LineCap::Butt, LineJoin::Miter);
        s.dashes = vec![1e-4, 1e-4];
        let mut buf = vec![0u8; 4];
        let mut surf = Surface::new(&mut buf, 2, 2, 2, Format::A8).unwrap();
        let r = stroke(
            &mut surf,
            &line(0.0, 0.0, 1000.0, 0.0),
            &s,
            &BLACK,
            &ID,
            BlendMode::Normal,
            None,
        );
        assert_eq!(r, Err(Error::InvalidArgument));
    }
}
