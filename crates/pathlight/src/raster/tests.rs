use crate::blend::BlendMode;
use crate::geom::{Point, Transform};
use crate::paint::{Color, Paint};
use crate::path::{FillRule, Path, PathBuilder};
use crate::render::fill;
use crate::surface::{Format, Mask, Surface};

const RED: Paint = Paint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0));

fn polygon(points: &[(f32, f32)]) -> Path {
    let mut b = PathBuilder::new();
    b.move_to(Point::new(points[0].0, points[0].1));
    for &(x, y) in &points[1..] {
        b.line_to(Point::new(x, y));
    }
    b.close();
    b.finish()
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Path {
    polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

/// A circle of 4 cubics (max radial error about 0.03%).
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

/// Renders `path` as an A8 coverage image.
fn coverage(path: &Path, rule: FillRule, w: u32, h: u32) -> Vec<u8> {
    coverage_t(path, rule, &Transform::IDENTITY, w, h)
}

fn coverage_t(path: &Path, rule: FillRule, t: &Transform, w: u32, h: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (w * h) as usize];
    let mut s = Surface::new(&mut buf, w, h, w as usize, Format::A8).unwrap();
    fill(&mut s, path, rule, &RED, t, BlendMode::Normal, None).unwrap();
    buf
}

fn area(img: &[u8]) -> f32 {
    img.iter().map(|&a| a as f32 / 255.0).sum()
}

fn shoelace(points: &[(f32, f32)]) -> f32 {
    let n = points.len();
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = points[i];
        let (x1, y1) = points[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    (a * 0.5).abs()
}

#[test]
fn pixel_aligned_rect_is_exact() {
    let img = coverage(&rect(2.0, 3.0, 7.0, 5.0), FillRule::NonZero, 10, 8);
    for y in 0..8 {
        for x in 0..10 {
            let inside = (2..7).contains(&x) && (3..5).contains(&y);
            assert_eq!(
                img[y * 10 + x],
                if inside { 255 } else { 0 },
                "pixel ({x}, {y})"
            );
        }
    }
}

#[test]
fn half_pixel_edges_are_half_covered() {
    let img = coverage(&rect(1.5, 1.0, 4.5, 2.0), FillRule::NonZero, 6, 3);
    assert_eq!(&img[6..12], &[0, 128, 255, 255, 128, 0]);
}

#[test]
fn winding_direction_does_not_matter() {
    let cw = coverage(
        &polygon(&[(1.0, 1.0), (9.0, 2.0), (4.0, 8.5)]),
        FillRule::NonZero,
        12,
        12,
    );
    let ccw = coverage(
        &polygon(&[(4.0, 8.5), (9.0, 2.0), (1.0, 1.0)]),
        FillRule::NonZero,
        12,
        12,
    );
    assert_eq!(cw, ccw);
}

#[test]
fn random_convex_polygons_match_shoelace_area() {
    let mut seed = 0x2545_f491_u32;
    let mut rand = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    for _ in 0..200 {
        // Points on an ellipse in angle order form a convex polygon.
        let (cx, cy) = (20.0 + rand() * 24.0, 20.0 + rand() * 24.0);
        let (rx, ry) = (2.0 + rand() * 17.0, 2.0 + rand() * 17.0);
        let n = 3 + (rand() * 9.0) as usize;
        let mut angles: Vec<f32> = (0..n).map(|_| rand() * std::f32::consts::TAU).collect();
        angles.sort_by(f32::total_cmp);
        let pts: Vec<(f32, f32)> = angles
            .iter()
            .map(|a| (cx + rx * a.cos(), cy + ry * a.sin()))
            .collect();
        let img = coverage(&polygon(&pts), FillRule::NonZero, 64, 64);
        let (got, want) = (area(&img), shoelace(&pts));
        // Per-pixel rounding to 8 bits is at most 1/510 per edge pixel.
        assert!(
            (got - want).abs() < 0.5 + want * 0.004,
            "{pts:?}: area {got}, want {want}"
        );
    }
}

#[test]
fn circle_area_and_symmetry() {
    let img = coverage(&circle(32.0, 32.0, 20.0), FillRule::NonZero, 64, 64);
    let want = std::f32::consts::PI * 400.0;
    // Flattened chords sit inside the curve, so the area comes out slightly
    // low; with 0.1 px tolerance the shortfall stays well under 1%.
    let got = area(&img);
    assert!(
        got <= want && (want - got) / want < 0.006,
        "area {got}, want {want}"
    );
    for y in 0..64 {
        for x in 0..64 {
            let a = img[y * 64 + x] as i32;
            assert!(
                (a - img[y * 64 + (63 - x)] as i32).abs() <= 2,
                "mirror at ({x}, {y})"
            );
            assert!(
                (a - img[(63 - y) * 64 + x] as i32).abs() <= 2,
                "mirror at ({x}, {y})"
            );
        }
    }
}

#[test]
fn fill_rules_differ_on_overlap() {
    // Two overlapping squares wound the same way.
    let mut b = PathBuilder::new();
    for (x, y) in [(0.0, 0.0), (4.0, 4.0)] {
        b.move_to(Point::new(x, y))
            .line_to(Point::new(x + 8.0, y))
            .line_to(Point::new(x + 8.0, y + 8.0))
            .line_to(Point::new(x, y + 8.0))
            .close();
    }
    let path = b.finish();
    let nz = coverage(&path, FillRule::NonZero, 12, 12);
    let eo = coverage(&path, FillRule::EvenOdd, 12, 12);
    let overlap = 6 * 12 + 6;
    assert_eq!((nz[overlap], eo[overlap]), (255, 0));
    assert_eq!((nz[12 + 1], eo[12 + 1]), (255, 255));
}

#[test]
fn shapes_partly_or_fully_off_surface() {
    // Extends past every edge: the whole surface is covered.
    assert!(
        coverage(&rect(-5.0, -5.0, 20.0, 20.0), FillRule::NonZero, 8, 8)
            .iter()
            .all(|&a| a == 255)
    );
    // A slanted edge entering from the left still winds correctly.
    let img = coverage(
        &polygon(&[(-10.0, 0.0), (4.0, 0.0), (4.0, 8.0), (-30.0, 8.0)]),
        FillRule::NonZero,
        8,
        8,
    );
    for y in 0..8 {
        assert_eq!(&img[y * 8..y * 8 + 8], &[255, 255, 255, 255, 0, 0, 0, 0]);
    }
    // Entirely outside, including huge coordinates: nothing drawn, no panic.
    assert!(
        coverage(&rect(-50.0, 0.0, -10.0, 8.0), FillRule::NonZero, 8, 8)
            .iter()
            .all(|&a| a == 0)
    );
    assert!(
        coverage(&rect(9.0, 0.0, 1e30, 8.0), FillRule::NonZero, 8, 8)
            .iter()
            .all(|&a| a == 0)
    );
    let huge = circle(0.0, 0.0, 1e20);
    assert!(
        coverage(&huge, FillRule::NonZero, 8, 8)
            .iter()
            .all(|&a| a == 255)
    );
}

#[test]
fn transform_is_applied() {
    let t = Transform::scale(2.0, 2.0).then(&Transform::translate(1.0, 1.0));
    let img = coverage_t(&rect(0.0, 0.0, 2.0, 1.0), FillRule::NonZero, &t, 8, 4);
    for y in 0..4 {
        for x in 0..8 {
            let inside = (1..5).contains(&x) && (1..3).contains(&y);
            assert_eq!(img[y * 8 + x], if inside { 255 } else { 0 });
        }
    }
}

#[test]
fn wide_shapes_use_solid_fills() {
    let path = rect(0.5, 0.0, 999.5, 4.0);
    let cov = super::rasterize(&path, &Transform::IDENTITY, FillRule::NonZero, 1000, 4);
    // Two edge strips, one fill between them, and no per-pixel alpha for
    // the interior.
    assert_eq!(cov.strips.len(), 2);
    assert_eq!(cov.fills.len(), 1);
    assert!(cov.alphas.len() <= 2 * 8 * 4);
}

fn rgba_pixel(fmt: Format, bg: [u8; 4], paint: Paint, blend: BlendMode) -> [u8; 4] {
    let mut buf = bg.repeat(4);
    let mut s = Surface::new(&mut buf, 2, 2, 8, fmt).unwrap();
    fill(
        &mut s,
        &rect(0.0, 0.0, 2.0, 2.0),
        FillRule::NonZero,
        &paint,
        &Transform::IDENTITY,
        blend,
        None,
    )
    .unwrap();
    [buf[0], buf[1], buf[2], buf[3]]
}

#[test]
fn compositing_formats_and_blend_modes() {
    assert_eq!(
        rgba_pixel(Format::Rgba8, [0; 4], RED, BlendMode::Normal),
        [255, 0, 0, 255]
    );
    assert_eq!(
        rgba_pixel(Format::Bgra8, [0; 4], RED, BlendMode::Normal),
        [0, 0, 255, 255]
    );
    let half_blue = Paint::Solid(Color::rgba(0.0, 0.0, 1.0, 0.5));
    assert_eq!(
        rgba_pixel(
            Format::Rgba8,
            [255, 255, 255, 255],
            half_blue,
            BlendMode::Normal
        ),
        [128, 128, 255, 255]
    );
    let gray = [128, 128, 128, 255];
    assert_eq!(
        rgba_pixel(Format::Rgba8, gray, RED, BlendMode::Multiply),
        [128, 0, 0, 255]
    );
    assert_eq!(
        rgba_pixel(Format::Rgba8, gray, RED, BlendMode::Screen),
        [255, 128, 128, 255]
    );
}

#[test]
fn clip_mask_scales_coverage() {
    let mut buf = [0u8; 4 * 4];
    let mask_data = [255u8, 0, 128, 255];
    let mask = Mask::new(&mask_data, 2, 2, 2).unwrap();
    let mut s = Surface::new(&mut buf, 2, 2, 8, Format::Rgba8).unwrap();
    let path = rect(0.0, 0.0, 2.0, 2.0);
    fill(
        &mut s,
        &path,
        FillRule::NonZero,
        &RED,
        &Transform::IDENTITY,
        BlendMode::Normal,
        Some(&mask),
    )
    .unwrap();
    assert_eq!(
        buf,
        [255, 0, 0, 255, 0, 0, 0, 0, 128, 0, 0, 128, 255, 0, 0, 255]
    );
}

#[test]
fn tight_last_row_is_not_overrun() {
    // Stride 16 but the last row has only 2 pixels (8 bytes).
    let mut buf = vec![0u8; 16 + 8];
    let mut s = Surface::new(&mut buf, 2, 2, 16, Format::Rgba8).unwrap();
    fill(
        &mut s,
        &rect(-1.0, -1.0, 9.0, 9.0),
        FillRule::NonZero,
        &RED,
        &Transform::IDENTITY,
        BlendMode::Normal,
        None,
    )
    .unwrap();
    assert_eq!(&buf[16..], &[255, 0, 0, 255, 255, 0, 0, 255]);
    assert_eq!(&buf[8..16], &[0; 8]);
}

#[test]
fn extreme_inputs_do_not_hang_or_panic() {
    // Coordinates that overflow f32 once transformed.
    let t = Transform::scale(1e30, 1e30);
    coverage_t(&rect(-1e10, -1e10, 1e10, 1e10), FillRule::NonZero, &t, 8, 8);
    // Nearly horizontal lines with an enormous run.
    let path = polygon(&[(-1e9, 1.0), (1e9, 1.000_000_1), (1e9, 3.0), (-1e9, 3.0)]);
    let img = coverage(&path, FillRule::NonZero, 8, 8);
    assert_eq!(img[2 * 8 + 4], 255);
    // A degenerate path that is just a point.
    assert!(
        coverage(&polygon(&[(3.0, 3.0), (3.0, 3.0)]), FillRule::NonZero, 8, 8)
            .iter()
            .all(|&a| a == 0)
    );
}
