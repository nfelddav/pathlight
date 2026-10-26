//! Renders a few shapes into `demo.ppm`: `cargo run --example demo`.

use pathlight::{
    BlendMode, Color, FillRule, Format, Paint, PathBuilder, Point, Surface, Transform,
};

fn main() {
    let (w, h) = (320u32, 200u32);
    let mut pixels = vec![255u8; (w * h * 4) as usize];
    let mut surface = Surface::new(&mut pixels, w, h, w as usize * 4, Format::Rgba8).unwrap();
    let id = Transform::IDENTITY;

    // A closed uniform cubic B-spline.
    let mut b = PathBuilder::new();
    let ctrl = [
        (40.0, 40.0),
        (160.0, 20.0),
        (190.0, 120.0),
        (120.0, 180.0),
        (20.0, 150.0),
    ];
    b.bspline(&ctrl.map(|(x, y)| Point::new(x, y)), true);
    let teal = Paint::Solid(Color::rgba(0.0, 0.6, 0.6, 1.0));
    pathlight::fill(
        &mut surface,
        &b.finish(),
        FillRule::NonZero,
        &teal,
        &id,
        BlendMode::Normal,
        None,
    )
    .unwrap();

    // Overlapping circles composited with multiply and screen.
    let circles = [
        (
            170.0,
            100.0,
            Color::rgba(1.0, 0.8, 0.0, 0.9),
            BlendMode::Multiply,
        ),
        (
            230.0,
            100.0,
            Color::rgba(0.9, 0.1, 0.4, 0.8),
            BlendMode::Multiply,
        ),
        (
            200.0,
            145.0,
            Color::rgba(0.2, 0.3, 1.0, 0.7),
            BlendMode::Screen,
        ),
    ];
    for (cx, cy, color, blend) in circles {
        let path = circle(cx, cy, 50.0);
        pathlight::fill(
            &mut surface,
            &path,
            FillRule::NonZero,
            &Paint::Solid(color),
            &id,
            blend,
            None,
        )
        .unwrap();
    }

    // A star, filled even-odd so its center stays empty.
    let mut b = PathBuilder::new();
    for i in 0..5 {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * 4.0 * std::f32::consts::PI / 5.0;
        let p = Point::new(275.0 + 35.0 * a.cos(), 50.0 + 35.0 * a.sin());
        if i == 0 {
            b.move_to(p)
        } else {
            b.line_to(p)
        };
    }
    b.close();
    let dark = Paint::Solid(Color::rgba(0.15, 0.15, 0.2, 1.0));
    pathlight::fill(
        &mut surface,
        &b.finish(),
        FillRule::EvenOdd,
        &dark,
        &id,
        BlendMode::Normal,
        None,
    )
    .unwrap();

    let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in pixels.chunks_exact(4) {
        ppm.extend_from_slice(&px[..3]);
    }
    std::fs::write("demo.ppm", ppm).unwrap();
    println!("wrote demo.ppm");
}

fn circle(cx: f32, cy: f32, r: f32) -> pathlight::Path {
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
