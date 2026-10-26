//! Sweeping cells into strips and fills.

use super::cells::{Cell, unpack};
use super::{TILE_HEIGHT, TILE_WIDTH};
use crate::path::FillRule;

/// A run of tiles in one band that contain edges. Its alpha values live in
/// [`Coverage::alphas`] starting at `alpha_offset`, column-major: one column
/// of [`TILE_HEIGHT`] values per pixel of `width`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Strip {
    pub x: u32,
    pub band: u32,
    pub width: u32,
    pub alpha_offset: usize,
}

/// A span of pixels `x0..x1` in one band with constant coverage per row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fill {
    pub x0: u32,
    pub x1: u32,
    pub band: u32,
    pub coverage: [u8; TILE_HEIGHT as usize],
}

/// Sparse coverage of one path, ready to composite.
#[derive(Clone, Debug, Default)]
pub(crate) struct Coverage {
    pub strips: Vec<Strip>,
    pub fills: Vec<Fill>,
    pub alphas: Vec<u8>,
}

fn to_alpha(acc: f32, rule: FillRule) -> u8 {
    let a = acc.abs();
    let c = match rule {
        FillRule::NonZero => a.min(1.0),
        FillRule::EvenOdd => {
            let m = a % 2.0;
            if m > 1.0 { 2.0 - m } else { m }
        }
    };
    (c * 255.0 + 0.5) as u8
}

/// Sorts `cells` and sweeps them into strips and fills for a surface
/// `width` pixels wide.
pub(crate) fn build(cells: &mut [Cell], rule: FillRule, width: u32) -> Coverage {
    cells.sort_unstable_by_key(|c| c.0);
    let mut cov = Coverage::default();
    let th = TILE_HEIGHT as usize;
    let mut i = 0;
    while i < cells.len() {
        let (band, _, _) = unpack(cells[i].0);
        let mut acc = [0.0f32; TILE_HEIGHT as usize];
        let mut x_done = 0u32;
        while i < cells.len() && unpack(cells[i].0).0 == band {
            // Find the run of adjacent tiles starting at this cell.
            let first_tile = unpack(cells[i].0).1 / TILE_WIDTH;
            let mut last_tile = first_tile;
            let mut j = i;
            while j < cells.len() {
                let (b, x, _) = unpack(cells[j].0);
                if b != band || x / TILE_WIDTH > last_tile + 1 {
                    break;
                }
                last_tile = x / TILE_WIDTH;
                j += 1;
            }
            let x0 = first_tile * TILE_WIDTH;
            let x1 = ((last_tile + 1) * TILE_WIDTH).min(width);
            push_fill(&mut cov, band, x_done, x0, &acc, rule);

            let alpha_offset = cov.alphas.len();
            let mut k = i;
            for x in x0..x1 {
                while k < j {
                    let (_, cx, row) = unpack(cells[k].0);
                    if cx != x {
                        break;
                    }
                    acc[row as usize] += cells[k].1;
                    k += 1;
                }
                cov.alphas.extend(acc.iter().map(|&a| to_alpha(a, rule)));
            }
            debug_assert_eq!(cov.alphas.len() - alpha_offset, (x1 - x0) as usize * th);
            cov.strips.push(Strip {
                x: x0,
                band,
                width: x1 - x0,
                alpha_offset,
            });
            x_done = x1;
            i = j;
        }
        // Lines dropped past the right edge can leave coverage running to it.
        push_fill(&mut cov, band, x_done, width, &acc, rule);
    }
    cov
}

fn push_fill(
    cov: &mut Coverage,
    band: u32,
    x0: u32,
    x1: u32,
    acc: &[f32; TILE_HEIGHT as usize],
    rule: FillRule,
) {
    if x0 >= x1 {
        return;
    }
    let coverage = acc.map(|a| to_alpha(a, rule));
    if coverage.iter().any(|&c| c != 0) {
        cov.fills.push(Fill {
            x0,
            x1,
            band,
            coverage,
        });
    }
}
