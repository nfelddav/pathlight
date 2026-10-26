/// Shape at the open ends of a stroked subpath.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum LineCap {
    #[default]
    Butt = 0,
    Round = 1,
    Square = 2,
}

/// Shape where two stroked segments meet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum LineJoin {
    #[default]
    Miter = 0,
    Round = 1,
    Bevel = 2,
}

/// How to stroke a path. Strokes are offset in user space and then
/// transformed, so non-uniform transforms skew the stroke; there is no
/// special case for hairlines.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    /// Ratio of miter length to stroke width above which a miter join falls
    /// back to bevel. Same meaning as SVG `stroke-miterlimit`.
    pub miter_limit: f32,
    /// Alternating dash and gap lengths. Empty means a solid stroke.
    pub dashes: Vec<f32>,
    pub dash_offset: f32,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        StrokeStyle {
            width: 1.0,
            cap: LineCap::default(),
            join: LineJoin::default(),
            miter_limit: 4.0,
            dashes: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

impl StrokeStyle {
    pub(crate) fn is_valid(&self) -> bool {
        self.width.is_finite()
            && self.width > 0.0
            && self.miter_limit.is_finite()
            && self.miter_limit >= 1.0
            && self.dash_offset.is_finite()
            && self.dashes.iter().all(|d| d.is_finite() && *d >= 0.0)
            && (self.dashes.is_empty() || self.dashes.iter().any(|d| *d > 0.0))
    }
}
