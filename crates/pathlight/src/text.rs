//! Single-line shaped text.
//!
//! A string is split into bidi runs (unicode-bidi) in visual order and each
//! of those into script runs (unicode-script). Each piece is shaped with
//! harfrust, and the glyph outlines (from skrifa, unhinted, in font units)
//! are gathered into one path that is filled through the same rasterizer as
//! any other path. Line breaking, paragraph layout and font fallback are out
//! of scope.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use harfrust::{Buffer, Direction, ShapeOptions, ShaperFont, Tag};
use skrifa::{DrawMemory, GlyphId, PathSink, ScalerFont};
use unicode_bidi::BidiInfo;
use unicode_script::{Script, UnicodeScript};

use crate::blend::BlendMode;
use crate::error::Error;
use crate::geom::{Point, Transform};
use crate::paint::Paint;
use crate::path::{FillRule, Path, PathBuilder};
use crate::render::fill;
use crate::surface::{Mask, Surface};

/// A font face loaded from bytes the caller supplies. pathlight never reads
/// fonts from the filesystem or asks the platform for system fonts.
///
/// Cloning is cheap: clones share the parsed font.
#[derive(Clone)]
pub struct Font {
    inner: Arc<FontInner>,
}

struct FontInner {
    font: skrifa::Font,
    scaler: ScalerFont,
    index: u32,
}

impl fmt::Debug for Font {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Font")
            .field("index", &self.inner.index)
            .field("units_per_em", &self.units_per_em())
            .finish()
    }
}

impl Font {
    /// Copies `data` and loads face `index` from it (0 unless `data` is a
    /// font collection).
    pub fn from_bytes(data: &[u8], index: u32) -> Result<Self, Error> {
        let font = skrifa::Font::new(data.to_vec(), index).ok_or(Error::InvalidFont)?;
        if font.units_per_em() == 0 {
            return Err(Error::InvalidFont);
        }
        // Unhinted, in font units: glyphs are scaled and transformed as paths.
        let scaler = ScalerFont::force_unhinted(font.clone(), None);
        Ok(Font {
            inner: Arc::new(FontInner {
                font,
                scaler,
                index,
            }),
        })
    }

    pub fn index(&self) -> u32 {
        self.inner.index
    }

    pub fn units_per_em(&self) -> u16 {
        self.inner.font.units_per_em()
    }
}

/// A positioned glyph, in font units with y up, relative to the origin.
struct PlacedGlyph {
    id: u32,
    x: f32,
    y: f32,
}

/// Shapes `text` into glyphs in visual order and returns them with the total
/// advance, all in font units.
fn layout(font: &Font, text: &str) -> Result<(Vec<PlacedGlyph>, f32), Error> {
    let shaper = ShaperFont::new(&font.inner.font);
    let bidi = BidiInfo::new(text, None);
    let mut glyphs = Vec::new();
    let mut pen = 0.0;
    for para in &bidi.paragraphs {
        let (_, runs) = bidi.visual_runs(para, para.range.clone());
        for run in runs {
            let rtl = bidi.levels[run.start].is_rtl();
            let mut pieces = script_runs(text, run);
            // Within a right-to-left run, later text sits further left.
            if rtl {
                pieces.reverse();
            }
            for (range, script) in pieces {
                let mut buffer = Buffer::new();
                buffer.push_str(&text[range.clone()]);
                buffer.set_pre_context(&text[..range.start]);
                buffer.set_post_context(&text[range.end..]);
                buffer.set_direction(if rtl {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                });
                let tag = script.short_name().as_bytes();
                if let Ok(tag) = <&[u8; 4]>::try_from(tag) {
                    buffer.set_script(harfrust::Script::from_iso15924_tag(Tag::new(tag)));
                }
                buffer.guess_segment_properties();
                harfrust::shape(&shaper, &mut buffer, ShapeOptions::new())
                    .map_err(|_| Error::InvalidArgument)?;
                // A right-to-left buffer comes back in visual order already.
                for (info, pos) in buffer.glyph_infos().iter().zip(buffer.glyph_positions()) {
                    glyphs.push(PlacedGlyph {
                        id: info.glyph_id,
                        x: pen + pos.x_offset as f32,
                        y: pos.y_offset as f32,
                    });
                    pen += pos.x_advance as f32;
                }
            }
        }
    }
    Ok((glyphs, pen))
}

/// Splits `range` of `text` into runs of one script each, in logical order.
/// Common and inherited characters (spaces, digits, punctuation, combining
/// marks) join the run around them.
fn script_runs(text: &str, range: Range<usize>) -> Vec<(Range<usize>, Script)> {
    let mut runs: Vec<(Range<usize>, Script)> = Vec::new();
    for (i, c) in text[range.clone()].char_indices() {
        let at = range.start + i;
        let script = c.script();
        let neutral = matches!(script, Script::Common | Script::Inherited | Script::Unknown);
        match runs.last_mut() {
            Some((r, s)) if neutral || *s == script => r.end = at + c.len_utf8(),
            // Leading neutral characters take the first real script.
            Some((r, s)) if matches!(*s, Script::Common | Script::Inherited | Script::Unknown) => {
                *s = script;
                r.end = at + c.len_utf8();
            }
            _ => runs.push((at..at + c.len_utf8(), script)),
        }
    }
    runs
}

/// Collects glyph outlines into a path, mapping font units (y up) at `origin`
/// to user space (y down).
struct GlyphSink<'a> {
    path: &'a mut PathBuilder,
    origin: Point,
    scale: f32,
}

impl GlyphSink<'_> {
    fn map(&self, x: f32, y: f32) -> Point {
        Point::new(
            self.origin.x + x * self.scale,
            self.origin.y - y * self.scale,
        )
    }
}

impl PathSink for GlyphSink<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.path.move_to(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.path.line_to(p);
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (c, p) = (self.map(cx, cy), self.map(x, y));
        self.path.quad_to(c, p);
    }

    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        let (c1, c2, p) = (self.map(c1x, c1y), self.map(c2x, c2y), self.map(x, y));
        self.path.cubic_to(c1, c2, p);
    }

    fn close(&mut self) {
        self.path.close();
    }
}

/// Shapes `text` and returns its outline as a path, baseline origin at
/// `(x, y)` in user space, `size` units per em. Glyphs the font cannot draw
/// are skipped.
pub fn text_path(font: &Font, text: &str, size: f32, x: f32, y: f32) -> Result<Path, Error> {
    if !(size.is_finite() && size > 0.0 && x.is_finite() && y.is_finite()) {
        return Err(Error::InvalidArgument);
    }
    let (glyphs, _) = layout(font, text)?;
    let scale = size / font.units_per_em() as f32;
    let mut path = PathBuilder::new();
    for g in &glyphs {
        let Some(outline) = font.inner.scaler.outline(GlyphId::new(g.id)) else {
            continue;
        };
        let origin = Point::new(x + g.x * scale, y - g.y * scale);
        let mut sink = GlyphSink {
            path: &mut path,
            origin,
            scale,
        };
        // A glyph whose program fails to run is left out, not fatal.
        let _ = outline.draw(DrawMemory::Scratch, &mut sink);
    }
    Ok(path.finish())
}

/// The horizontal advance of `text` set at `size` units per em, i.e. how far
/// the pen moves when it is drawn.
pub fn measure_text(font: &Font, text: &str, size: f32) -> Result<f32, Error> {
    if !(size.is_finite() && size > 0.0) {
        return Err(Error::InvalidArgument);
    }
    let (_, advance) = layout(font, text)?;
    Ok(advance * size / font.units_per_em() as f32)
}

/// Draws `text` on one line with its baseline origin at `(x, y)` in user
/// space, `size` pixels per em. See [`crate::fill`] for `clip`.
#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    surface: &mut Surface<'_>,
    font: &Font,
    text: &str,
    size: f32,
    x: f32,
    y: f32,
    paint: &Paint,
    transform: &Transform,
    blend: BlendMode,
    clip: Option<&Mask<'_>>,
) -> Result<(), Error> {
    if !transform.is_finite() || !paint.is_valid() {
        return Err(Error::InvalidArgument);
    }
    let path = text_path(font, text, size, x, y)?;
    fill(
        surface,
        &path,
        FillRule::NonZero,
        paint,
        transform,
        blend,
        clip,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::Color;
    use crate::surface::Format;

    const FONT: &[u8] = include_bytes!("../tests/fonts/DejaVuSans-subset.ttf");

    fn font() -> Font {
        Font::from_bytes(FONT, 0).unwrap()
    }

    fn ids(text: &str) -> Vec<u32> {
        layout(&font(), text)
            .unwrap()
            .0
            .iter()
            .map(|g| g.id)
            .collect()
    }

    fn render(text: &str, size: f32, t: &Transform, w: u32, h: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (w * h) as usize];
        let mut s = Surface::new(&mut buf, w, h, w as usize, Format::A8).unwrap();
        let black = Paint::Solid(Color::rgba(0.0, 0.0, 0.0, 1.0));
        draw_text(
            &mut s,
            &font(),
            text,
            size,
            2.0,
            size,
            &black,
            t,
            BlendMode::Normal,
            None,
        )
        .unwrap();
        buf
    }

    #[test]
    fn rejects_garbage_font() {
        assert_eq!(
            Font::from_bytes(b"not a font", 0).unwrap_err(),
            Error::InvalidFont
        );
        assert_eq!(Font::from_bytes(FONT, 1).unwrap_err(), Error::InvalidFont);
    }

    #[test]
    fn advance_matches_the_font_metrics() {
        let f = font();
        assert_eq!(f.units_per_em(), 2048);
        // DejaVu Sans 'H' advances 1540 font units.
        assert_eq!(measure_text(&f, "H", 2048.0).unwrap(), 1540.0);
        assert_eq!(
            measure_text(&f, "HH", 20.48).unwrap(),
            2.0 * measure_text(&f, "H", 20.48).unwrap()
        );
        assert_eq!(measure_text(&f, "", 12.0).unwrap(), 0.0);
    }

    #[test]
    fn kerning_is_applied() {
        let f = font();
        let pair = measure_text(&f, "AV", 100.0).unwrap();
        let apart = measure_text(&f, "A", 100.0).unwrap() + measure_text(&f, "V", 100.0).unwrap();
        assert!(pair < apart, "AV {pair} vs {apart}");
    }

    #[test]
    fn arabic_letters_take_contextual_forms() {
        // Seen alone, seen is its isolated form; starting a word it joins.
        let alone = ids("س");
        let word = ids("سلام");
        assert_eq!(alone.len(), 1);
        assert_ne!(word.last(), alone.first(), "word glyphs {word:?}");
        assert!(!word.contains(&alone[0]));
    }

    #[test]
    fn right_to_left_runs_are_reversed_visually() {
        let (alef, bet) = (ids("א")[0], ids("ב")[0]);
        // Logical alef-bet is displayed bet on the left, alef on the right.
        assert_eq!(ids("אב"), vec![bet, alef]);
        // Embedded in left-to-right text, the Hebrew run is still reversed
        // but stays between the Latin letters.
        let (a, b) = (ids("a")[0], ids("b")[0]);
        let space = ids(" ")[0];
        assert_eq!(ids("a אב b"), vec![a, space, bet, alef, space, b]);
    }

    #[test]
    fn script_runs_absorb_neutral_characters() {
        let text = "1 abc שלום!";
        let runs = script_runs(text, 0..text.len());
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].1, Script::Latin);
        assert_eq!(&text[runs[0].0.clone()], "1 abc ");
        assert_eq!(runs[1].1, Script::Hebrew);
    }

    #[test]
    fn drawn_text_lands_inside_its_advance_box() {
        let (w, h) = (64, 32);
        let img = render("Hi", 20.0, &Transform::IDENTITY, w, h);
        let advance = measure_text(&font(), "Hi", 20.0).unwrap();
        let ink: f32 = img.iter().map(|&a| a as f32).sum::<f32>() / 255.0;
        assert!(ink > 20.0, "ink {ink}");
        for y in 0..h as usize {
            for x in 0..w as usize {
                if img[y * w as usize + x] > 0 {
                    // Baseline at y = 20, ascender below the top of the surface.
                    assert!(y < 21, "ink below the baseline at ({x}, {y})");
                    assert!(
                        (x as f32) >= 2.0 && (x as f32) < 2.0 + advance + 1.0,
                        "ink at ({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn text_scales_with_the_transform() {
        let one = render("o", 16.0, &Transform::IDENTITY, 64, 64);
        let two = render("o", 16.0, &Transform::scale(2.0, 2.0), 64, 64);
        let area = |img: &[u8]| img.iter().map(|&a| a as f32 / 255.0).sum::<f32>();
        let ratio = area(&two) / area(&one);
        assert!((ratio - 4.0).abs() < 0.2, "ratio {ratio}");
    }

    #[test]
    fn bad_sizes_are_rejected() {
        assert_eq!(
            measure_text(&font(), "a", 0.0).unwrap_err(),
            Error::InvalidArgument
        );
        assert_eq!(
            text_path(&font(), "a", f32::NAN, 0.0, 0.0).unwrap_err(),
            Error::InvalidArgument
        );
    }
}
