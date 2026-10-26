use crate::error::Error;

/// Pixel layout of a [`Surface`]. Color formats store premultiplied alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Format {
    /// 8-bit R, G, B, A in memory order, premultiplied.
    Rgba8 = 0,
    /// 8-bit B, G, R, A in memory order, premultiplied.
    Bgra8 = 1,
    /// 8-bit alpha (coverage) only.
    A8 = 2,
}

impl Format {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Format::Rgba8),
            1 => Some(Format::Bgra8),
            2 => Some(Format::A8),
            _ => None,
        }
    }

    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Format::Rgba8 | Format::Bgra8 => 4,
            Format::A8 => 1,
        }
    }
}

/// The tallest surface or mask pathlight accepts, in rows. The rasterizer
/// packs the row into a 64-bit sort key and needs the headroom.
pub const MAX_HEIGHT: u32 = 1 << 30;

/// A caller-owned pixel buffer to render into. pathlight never allocates or
/// frees the pixels.
#[derive(Debug)]
pub struct Surface<'a> {
    data: &'a mut [u8],
    width: u32,
    height: u32,
    stride: usize,
    format: Format,
}

impl<'a> Surface<'a> {
    /// Wraps `data` as `height` rows, `stride` bytes apart; the last row only
    /// needs `width` pixels. `height` may be at most [`MAX_HEIGHT`].
    pub fn new(
        data: &'a mut [u8],
        width: u32,
        height: u32,
        stride: usize,
        format: Format,
    ) -> Result<Self, Error> {
        check_layout(data.len(), width, height, stride, format)?;
        Ok(Surface {
            data,
            width,
            height,
            stride,
            format,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn data(&self) -> &[u8] {
        self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        self.data
    }
}

/// A read-only A8 coverage mask, used to clip any draw call to an arbitrary
/// shape: render the clip path into an A8 surface, then pass it as a mask.
#[derive(Clone, Copy, Debug)]
pub struct Mask<'a> {
    data: &'a [u8],
    width: u32,
    height: u32,
    stride: usize,
}

impl<'a> Mask<'a> {
    pub fn new(data: &'a [u8], width: u32, height: u32, stride: usize) -> Result<Self, Error> {
        check_layout(data.len(), width, height, stride, Format::A8)?;
        Ok(Mask {
            data,
            width,
            height,
            stride,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Coverage at `(x, y)`, or 0 outside the mask.
    pub fn coverage(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.data[y as usize * self.stride + x as usize]
    }
}

fn check_layout(
    len: usize,
    width: u32,
    height: u32,
    stride: usize,
    format: Format,
) -> Result<(), Error> {
    let row = (width as usize)
        .checked_mul(format.bytes_per_pixel())
        .ok_or(Error::InvalidArgument)?;
    if width == 0 || height == 0 || height > MAX_HEIGHT || stride < row {
        return Err(Error::InvalidArgument);
    }
    // The last row only needs `row` bytes, not a full stride.
    let needed = stride
        .checked_mul(height as usize - 1)
        .and_then(|n| n.checked_add(row))
        .ok_or(Error::InvalidArgument)?;
    if len < needed {
        return Err(Error::InvalidArgument);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_tight_last_row() {
        let mut buf = vec![0u8; 16 * 3 + 8];
        assert!(Surface::new(&mut buf, 2, 4, 16, Format::Rgba8).is_ok());
    }

    #[test]
    fn rejects_short_buffer_and_stride() {
        let mut buf = vec![0u8; 15];
        assert_eq!(
            Surface::new(&mut buf, 2, 2, 8, Format::Rgba8).unwrap_err(),
            Error::InvalidArgument
        );
        assert_eq!(
            Surface::new(&mut buf, 4, 1, 3, Format::A8).unwrap_err(),
            Error::InvalidArgument
        );
    }

    #[test]
    fn rejects_height_above_limit() {
        // One byte per row would do, but the height check comes first.
        let data = [0u8; 1];
        assert_eq!(
            Mask::new(&data, 1, MAX_HEIGHT + 1, 0).unwrap_err(),
            Error::InvalidArgument
        );
    }

    #[test]
    fn mask_outside_is_zero() {
        let data = [255u8; 4];
        let m = Mask::new(&data, 2, 2, 2).unwrap();
        assert_eq!(m.coverage(1, 1), 255);
        assert_eq!(m.coverage(2, 0), 0);
    }
}
