//! Pixel diff between a rendered pixmap and a reference.
//!
//! The raster backend is compared with what Chrome's canvas paints for the
//! same calls (the fixture display lists in `tests/fixtures/`), and later
//! with upstream's exported PNGs. Two rasterisers never agree on every
//! anti-aliased edge to the last bit, so a comparison states its
//! [`Tolerance`]: how far a pixel may be off, and how many pixels may be
//! off by more than that.
//!
//! Pixels are compared premultiplied, as both sides composite them: a
//! pixel's difference is the largest absolute difference over its red,
//! green, blue and alpha channels.

use std::fmt;

use tiny_skia::{Pixmap, PixmapRef, PremultipliedColorU8};

/// How far a render may be from its reference: at most `pixels` pixels may
/// differ by more than `channel` in some channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tolerance {
    pub channel: u8,
    pub pixels: usize,
}

impl Tolerance {
    /// Every pixel identical.
    pub const EXACT: Tolerance = Tolerance {
        channel: 0,
        pixels: 0,
    };
}

/// One pixel of a comparison: its position and both values, premultiplied
/// RGBA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pixel {
    pub x: u32,
    pub y: u32,
    pub actual: [u8; 4],
    pub expected: [u8; 4],
}

/// The two pixmaps have different sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeMismatch {
    pub actual: (u32, u32),
    pub expected: (u32, u32),
}

impl fmt::Display for SizeMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "size differs: rendered {}x{}, reference {}x{}",
            self.actual.0, self.actual.1, self.expected.0, self.expected.1
        )
    }
}

impl std::error::Error for SizeMismatch {}

/// The result of [`compare`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff {
    pub width: u32,
    pub height: u32,
    /// The largest channel difference anywhere.
    pub max_channel: u8,
    /// The first pixel (in row order) with the largest difference, or
    /// `None` when the pixmaps are identical.
    pub worst: Option<Pixel>,
    /// `histogram[d]`: how many pixels differ by exactly `d`.
    histogram: Box<[usize; 256]>,
}

impl Diff {
    /// How many pixels differ by more than `channel`.
    pub fn pixels_over(&self, channel: u8) -> usize {
        self.histogram[usize::from(channel) + 1..].iter().sum()
    }

    /// Whether the render is within `tolerance` of the reference.
    pub fn within(&self, tolerance: &Tolerance) -> bool {
        self.pixels_over(tolerance.channel) <= tolerance.pixels
    }

    /// A one-line account of the comparison against `tolerance`.
    pub fn report(&self, tolerance: &Tolerance) -> String {
        let mut text = format!(
            "{} of {} pixels differ by more than {} (allowed {}); max channel difference {}",
            self.pixels_over(tolerance.channel),
            u64::from(self.width) * u64::from(self.height),
            tolerance.channel,
            tolerance.pixels,
            self.max_channel,
        );
        if let Some(p) = self.worst {
            text.push_str(&format!(
                " at ({}, {}): rendered {:?}, reference {:?}",
                p.x, p.y, p.actual, p.expected
            ));
        }
        text
    }
}

fn rgba(c: PremultipliedColorU8) -> [u8; 4] {
    [c.red(), c.green(), c.blue(), c.alpha()]
}

fn difference(a: [u8; 4], b: [u8; 4]) -> u8 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(y))
        .max()
        .unwrap_or(0)
}

fn check_size(actual: PixmapRef<'_>, expected: PixmapRef<'_>) -> Result<(), SizeMismatch> {
    let a = (actual.width(), actual.height());
    let e = (expected.width(), expected.height());
    if a == e {
        Ok(())
    } else {
        Err(SizeMismatch {
            actual: a,
            expected: e,
        })
    }
}

/// Compare `actual` with `expected` pixel by pixel.
pub fn compare(actual: PixmapRef<'_>, expected: PixmapRef<'_>) -> Result<Diff, SizeMismatch> {
    check_size(actual, expected)?;
    let width = actual.width();
    let mut histogram = Box::new([0usize; 256]);
    let mut max_channel = 0;
    let mut worst = None;
    for (i, (a, e)) in actual.pixels().iter().zip(expected.pixels()).enumerate() {
        let (a, e) = (rgba(*a), rgba(*e));
        let d = difference(a, e);
        histogram[usize::from(d)] += 1;
        if d > max_channel {
            max_channel = d;
            let i = i as u32;
            worst = Some(Pixel {
                x: i % width,
                y: i / width,
                actual: a,
                expected: e,
            });
        }
    }
    Ok(Diff {
        width,
        height: actual.height(),
        max_channel,
        worst,
        histogram,
    })
}

/// A picture of the comparison for a person to look at: pixels over the
/// tolerance's channel bound in opaque red, pixels that differ within it in
/// opaque yellow, and equal pixels as the reference faded to light grey
/// (over white), so the drawing stays recognisable.
pub fn diff_image(
    actual: PixmapRef<'_>,
    expected: PixmapRef<'_>,
    tolerance: &Tolerance,
) -> Result<Pixmap, SizeMismatch> {
    check_size(actual, expected)?;
    let mut out = Pixmap::new(actual.width(), actual.height())
        .expect("a pixmap's size is a valid pixmap size");
    let red = PremultipliedColorU8::from_rgba(255, 0, 0, 255).expect("opaque");
    let yellow = PremultipliedColorU8::from_rgba(255, 255, 0, 255).expect("opaque");
    for ((o, a), e) in out
        .pixels_mut()
        .iter_mut()
        .zip(actual.pixels())
        .zip(expected.pixels())
    {
        let (a, e) = (rgba(*a), rgba(*e));
        let d = difference(a, e);
        *o = if d > tolerance.channel {
            red
        } else if d > 0 {
            yellow
        } else {
            // The reference over white, as luma, mapped into 160..=255.
            let alpha = u32::from(e[3]);
            let over_white = |c: u8| u32::from(c) + (255 - alpha);
            let luma =
                (299 * over_white(e[0]) + 587 * over_white(e[1]) + 114 * over_white(e[2])) / 1000;
            let g = (160 + luma * 95 / 255) as u8;
            PremultipliedColorU8::from_rgba(g, g, g, 255).expect("opaque")
        };
    }
    Ok(out)
}
