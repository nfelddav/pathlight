use crate::geom::Point;

/// The rule that decides which regions of a path are inside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// One path command. The points each verb consumes are stored separately in
/// [`Path::points`], in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    /// Consumes one point.
    MoveTo,
    /// Consumes one point.
    LineTo,
    /// Consumes two points: control, end.
    QuadTo,
    /// Consumes three points: control 1, control 2, end.
    CubicTo,
    /// Consumes no points.
    Close,
}

/// An immutable path made of lines, quadratic and cubic Béziers.
///
/// B-splines are converted to cubic Béziers when they are added, so the
/// rasterizer only ever sees these verbs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    verbs: Vec<Verb>,
    points: Vec<Point>,
}

impl Path {
    pub fn verbs(&self) -> &[Verb] {
        &self.verbs
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty()
    }

    /// Whether every coordinate is finite (no NaN or infinity).
    pub fn is_finite(&self) -> bool {
        self.points
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite())
    }
}

/// Builds a [`Path`].
///
/// Drawing commands issued before any `move_to` start a subpath at the origin.
#[derive(Clone, Debug, Default)]
pub struct PathBuilder {
    path: Path,
    in_subpath: bool,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_to(&mut self, p: Point) -> &mut Self {
        self.path.verbs.push(Verb::MoveTo);
        self.path.points.push(p);
        self.in_subpath = true;
        self
    }

    pub fn line_to(&mut self, p: Point) -> &mut Self {
        self.ensure_subpath();
        self.path.verbs.push(Verb::LineTo);
        self.path.points.push(p);
        self
    }

    pub fn quad_to(&mut self, c: Point, p: Point) -> &mut Self {
        self.ensure_subpath();
        self.path.verbs.push(Verb::QuadTo);
        self.path.points.extend([c, p]);
        self
    }

    pub fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) -> &mut Self {
        self.ensure_subpath();
        self.path.verbs.push(Verb::CubicTo);
        self.path.points.extend([c1, c2, p]);
        self
    }

    pub fn close(&mut self) -> &mut Self {
        if self.in_subpath {
            self.path.verbs.push(Verb::Close);
            self.in_subpath = false;
        }
        self
    }

    /// Adds a uniform cubic B-spline through `control` as a new subpath.
    ///
    /// The curve is converted to one cubic Bézier per span. Like any uniform
    /// B-spline it does not pass through its first and last control points;
    /// repeat an endpoint three times to make the curve start or end on it.
    /// An open spline needs at least 4 control points, a closed one at least 3.
    /// Returns `false` and leaves the builder unchanged if there are too few.
    pub fn bspline(&mut self, control: &[Point], closed: bool) -> bool {
        let n = control.len();
        let spans = match (closed, n) {
            (false, n) if n >= 4 => n - 3,
            (true, n) if n >= 3 => n,
            _ => return false,
        };
        let at = |i: usize| control[i % n];
        for i in 0..spans {
            let [b0, b1, b2, b3] = bspline_span_to_bezier(at(i), at(i + 1), at(i + 2), at(i + 3));
            if i == 0 {
                self.move_to(b0);
            }
            self.cubic_to(b1, b2, b3);
        }
        if closed {
            self.close();
        }
        true
    }

    /// The path built so far.
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    pub fn finish(self) -> Path {
        self.path
    }

    fn ensure_subpath(&mut self) {
        if !self.in_subpath {
            let start = self.last_subpath_start();
            self.move_to(start);
        }
    }

    // After a close, the next segment starts where the closed subpath began.
    fn last_subpath_start(&self) -> Point {
        let mut pi = 0;
        let mut start = Point::default();
        for v in &self.path.verbs {
            match v {
                Verb::MoveTo => {
                    start = self.path.points[pi];
                    pi += 1;
                }
                Verb::LineTo => pi += 1,
                Verb::QuadTo => pi += 2,
                Verb::CubicTo => pi += 3,
                Verb::Close => {}
            }
        }
        start
    }
}

/// Converts one span of a uniform cubic B-spline into its Bézier control points.
fn bspline_span_to_bezier(p0: Point, p1: Point, p2: Point, p3: Point) -> [Point; 4] {
    let sixth = 1.0 / 6.0;
    let third = 1.0 / 3.0;
    [
        (p0 + p1 * 4.0 + p2) * sixth,
        (p1 * 2.0 + p2) * third,
        (p1 + p2 * 2.0) * third,
        (p1 + p2 * 4.0 + p3) * sixth,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(v: &[(f32, f32)]) -> Vec<Point> {
        v.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    #[test]
    fn open_bspline_has_one_cubic_per_span() {
        let mut b = PathBuilder::new();
        assert!(b.bspline(
            &pts(&[(0., 0.), (1., 2.), (3., 2.), (4., 0.), (6., 1.)]),
            false
        ));
        let path = b.finish();
        assert_eq!(path.verbs(), &[Verb::MoveTo, Verb::CubicTo, Verb::CubicTo]);
        assert_eq!(path.points().len(), 7);
    }

    #[test]
    fn collinear_bspline_stays_on_line() {
        let mut b = PathBuilder::new();
        b.bspline(&pts(&[(0., 0.), (6., 0.), (12., 0.), (18., 0.)]), false);
        let path = b.finish();
        assert_eq!(
            path.points(),
            &pts(&[(6., 0.), (8., 0.), (10., 0.), (12., 0.)])[..]
        );
    }

    #[test]
    fn closed_bspline_is_continuous_and_closed() {
        let mut b = PathBuilder::new();
        b.bspline(&pts(&[(0., 0.), (6., 0.), (6., 6.), (0., 6.)]), true);
        let path = b.finish();
        assert_eq!(path.verbs().len(), 1 + 4 + 1);
        assert_eq!(path.verbs().last(), Some(&Verb::Close));
        assert_eq!(path.points().first(), path.points().last());
    }

    #[test]
    fn too_few_control_points_is_rejected() {
        let mut b = PathBuilder::new();
        assert!(!b.bspline(&pts(&[(0., 0.), (1., 1.), (2., 0.)]), false));
        assert!(!b.bspline(&pts(&[(0., 0.), (1., 1.)]), true));
        assert!(b.finish().is_empty());
    }

    #[test]
    fn line_after_close_restarts_at_subpath_start() {
        let mut b = PathBuilder::new();
        b.move_to(Point::new(5., 5.))
            .line_to(Point::new(9., 5.))
            .close();
        b.line_to(Point::new(5., 9.));
        let path = b.finish();
        assert_eq!(path.verbs()[3], Verb::MoveTo);
        assert_eq!(path.points()[2], Point::new(5., 5.));
    }
}
