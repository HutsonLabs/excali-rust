//! Bilinear image sampling as Chrome's software canvas does it: Skia's
//! bitmap sampler (`SkBitmapProcState`) with its 4-bit subpixel filter.
//!
//! Sample positions come from the device pixel centres through the
//! inverse of the draw's matrix in `f32` (`SkMatrix::invert`, the
//! map-point procs), moved by half a texel and carried as 32.32 fractional
//! integers stepped per pixel along each span; each position becomes two
//! clamped texel indices and a 4-bit weight (`pack`), and the four texels
//! are blended with integer weights summing to 256, truncated
//! (`Filter_32_opaque`). Scaled images therefore show the 1/16 steps
//! Chrome shows, where a float bilinear filter would not.

use tiny_skia::{IntSize, Pixmap, PixmapRef};

/// A 2D affine matrix in `f32`, `SkMatrix`'s layout: `x' = sx·x + kx·y +
/// tx`, `y' = ky·x + sy·y + ty`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Matrix32 {
    pub sx: f32,
    pub kx: f32,
    pub tx: f32,
    pub ky: f32,
    pub sy: f32,
    pub ty: f32,
}

impl Matrix32 {
    fn is_scale_translate(&self) -> bool {
        self.kx == 0.0 && self.ky == 0.0
    }

    /// `SkMatrix::setConcat(self, m)`: scale/translate matrices multiply in
    /// `f32`, affine ones through `muladdmul` in `f64`.
    pub fn concat(&self, m: &Matrix32) -> Matrix32 {
        if self.is_scale_translate() && m.is_scale_translate() {
            return Matrix32 {
                sx: self.sx * m.sx,
                kx: 0.0,
                tx: self.sx * m.tx + self.tx,
                ky: 0.0,
                sy: self.sy * m.sy,
                ty: self.sy * m.ty + self.ty,
            };
        }
        let mam = |a: f32, b: f32, c: f32, d: f32| {
            (f64::from(a) * f64::from(b) + f64::from(c) * f64::from(d)) as f32
        };
        Matrix32 {
            sx: mam(self.sx, m.sx, self.kx, m.ky),
            kx: mam(self.sx, m.kx, self.kx, m.sy),
            tx: mam(self.sx, m.tx, self.kx, m.ty) + self.tx,
            ky: mam(self.ky, m.sx, self.sy, m.ky),
            sy: mam(self.ky, m.kx, self.sy, m.sy),
            ty: mam(self.ky, m.tx, self.sy, m.ty) + self.ty,
        }
    }

    /// `SkMatrix::invert`; `None` when it is singular or the inverse is not
    /// finite.
    pub fn invert(&self) -> Option<Matrix32> {
        let inv = if self.is_scale_translate() {
            let ix = 1.0 / self.sx;
            let iy = 1.0 / self.sy;
            Matrix32 {
                sx: ix,
                kx: 0.0,
                tx: -self.tx * ix,
                ky: 0.0,
                sy: iy,
                ty: -self.ty * iy,
            }
        } else {
            let det =
                f64::from(self.sx) * f64::from(self.sy) - f64::from(self.kx) * f64::from(self.ky);
            if det == 0.0 || !det.is_finite() {
                return None;
            }
            let inv_det = 1.0 / det;
            let dcross = |a: f32, b: f32, c: f32, d: f32| {
                f64::from(a) * f64::from(b) - f64::from(c) * f64::from(d)
            };
            Matrix32 {
                sx: (f64::from(self.sy) * inv_det) as f32,
                kx: (-f64::from(self.kx) * inv_det) as f32,
                tx: (dcross(self.kx, self.ty, self.sy, self.tx) * inv_det) as f32,
                ky: (-f64::from(self.ky) * inv_det) as f32,
                sy: (f64::from(self.sx) * inv_det) as f32,
                ty: (dcross(self.ky, self.tx, self.sx, self.ty) * inv_det) as f32,
            }
        };
        [inv.sx, inv.kx, inv.tx, inv.ky, inv.sy, inv.ty]
            .iter()
            .all(|v| v.is_finite())
            .then_some(inv)
    }

    /// The matrix's map-point proc on one point.
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        if self.is_scale_translate() {
            (x * self.sx + self.tx, y * self.sy + self.ty)
        } else {
            (
                x * self.sx + (y * self.kx + self.tx),
                x * self.ky + (y * self.sy + self.ty),
            )
        }
    }
}

/// `SkScalarToFractionalInt`: 32.32 fixed point, truncated.
fn fractional(v: f32) -> i64 {
    (f64::from(v) * 4294967296.0) as i64
}

/// Half a texel in 32.32 (`s.fFilterOneX >> 1` as a fractional int).
const HALF: i64 = 1 << 31;

/// A 32.32 position as its two clamped texel indices and 4-bit weight
/// (`pack` in `SkBitmapProcState_matrixProcs.cpp`).
fn taps(position: i64, max: i32) -> (usize, usize, u32) {
    let fixed = (position >> 16) as i32;
    let i0 = (fixed >> 16).clamp(0, max);
    let i1 = ((fixed.wrapping_add(1 << 16)) >> 16).clamp(0, max);
    (i0 as usize, i1 as usize, ((fixed >> 12) & 0xF) as u32)
}

/// `Filter_32_opaque`: the four premultiplied texels weighted by the
/// 4-bit subpixel position, each channel's sum over 256 truncated.
fn filter(x: u32, y: u32, a00: [u8; 4], a01: [u8; 4], a10: [u8; 4], a11: [u8; 4]) -> [u8; 4] {
    let xy = x * y;
    let s00 = (16 - x) * (16 - y);
    let s01 = 16 * x - xy;
    let s10 = 16 * y - xy;
    let s11 = xy;
    let mut out = [0; 4];
    for c in 0..4 {
        let sum = u32::from(a00[c]) * s00
            + u32::from(a01[c]) * s01
            + u32::from(a10[c]) * s10
            + u32::from(a11[c]) * s11;
        out[c] = (sum >> 8) as u8;
    }
    out
}

/// The bitmap sampled for the device pixels `left..right` x `top..bottom`
/// under `inverse` (device to image), as a pixmap of that size.
///
/// The blitter shades a row in spans, the runs of equal `coverage` it
/// blits (`blitAntiH` runs, `blitV` columns, `blitRect` interiors): each
/// span maps its first pixel and steps from there, which decides the last
/// bit of positions that land on a texel boundary.
pub(crate) fn sample(
    bitmap: PixmapRef<'_>,
    inverse: &Matrix32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    coverage: impl Fn(i32, i32) -> u8,
) -> Option<Pixmap> {
    let size = IntSize::from_wh((right - left) as u32, (bottom - top) as u32)?;
    let (bw, bh) = (bitmap.width() as i32, bitmap.height() as i32);
    let texel = |x: usize, y: usize| {
        let c = bitmap.pixels()[y * bw as usize + x];
        [c.red(), c.green(), c.blue(), c.alpha()]
    };
    let dx = fractional(inverse.sx);
    let dy = fractional(inverse.ky);
    let mut data = Vec::with_capacity(size.width() as usize * size.height() as usize * 4);
    for y in top..bottom {
        let (mut fx, mut fy) = (0, 0);
        let mut run = None;
        for x in left..right {
            let c = coverage(x, y);
            if run != Some(c) {
                // SkBitmapProcStateAutoMapper at the span's first pixel.
                let (px, py) = inverse.map(x as f32 + 0.5, y as f32 + 0.5);
                fx = fractional(px) - HALF;
                fy = fractional(py) - HALF;
                run = Some(c);
            }
            let (x0, x1, sub_x) = taps(fx, bw - 1);
            let (y0, y1, sub_y) = taps(fy, bh - 1);
            let c = filter(
                sub_x,
                sub_y,
                texel(x0, y0),
                texel(x1, y0),
                texel(x0, y1),
                texel(x1, y1),
            );
            data.extend_from_slice(&c);
            fx += dx;
            if !inverse.is_scale_translate() {
                fy += dy;
            }
        }
    }
    Pixmap::from_vec(data, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_are_sixteenths_truncated() {
        // Chrome's ramp between black and white: 255·k·16/256, truncated.
        let black = [0, 0, 0, 255];
        let white = [255, 255, 255, 255];
        let got: Vec<u8> = (0..16)
            .map(|k| filter(k, 0, black, white, black, white)[0])
            .collect();
        assert_eq!(got[4], 63);
        assert_eq!(got[5], 79);
        assert_eq!(got[8], 127);
        assert_eq!(got[15], 239);
        assert_eq!(filter(0, 0, white, black, black, black), white);
    }

    #[test]
    fn taps_clamp_and_split_the_position() {
        // 3.25 texels: 3 and 4, weight 4/16.
        assert_eq!(taps(fractional(3.25), 10), (3, 4, 4));
        // Past either end both taps clamp.
        assert_eq!(taps(fractional(-0.25), 10), (0, 0, 12));
        assert_eq!(taps(fractional(10.5), 10), (10, 10, 8));
    }

    #[test]
    fn inverse_and_concat() {
        let m = Matrix32 {
            sx: 3.0,
            kx: 0.0,
            tx: 52.0,
            ky: 0.0,
            sy: 3.0,
            ty: 60.0,
        };
        let inv = m.invert().unwrap();
        assert_eq!(inv.sx, 1.0 / 3.0);
        assert_eq!(inv.tx, -52.0 * (1.0 / 3.0_f32));
        let rot = Matrix32 {
            sx: 0.0,
            kx: -1.0,
            tx: 10.0,
            ky: 1.0,
            sy: 0.0,
            ty: 0.0,
        };
        let back = rot.concat(&rot.invert().unwrap());
        assert_eq!((back.sx, back.kx, back.ky, back.sy), (1.0, 0.0, 0.0, 1.0));
        assert_eq!((back.tx, back.ty), (0.0, 0.0));
    }
}
