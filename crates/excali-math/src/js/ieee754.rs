//! V8's fdlibm (`src/base/ieee754.cc`, V8 14.6, the one in Node 26), the
//! code behind `Math.sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`,
//! `exp`, `log`, `log2`, `log10` and `cbrt`, ported line for line.
//!
//! V8 is built with clang, which contracts `a * b + c` inside one expression
//! into a fused multiply-add wherever the target has one
//! (`-ffp-contract=on`, clang's default): arm64 does, so arm64 V8 rounds
//! those sums once, and x64 V8 (no FMA in its baseline) rounds them twice.
//! The goldens are arm64 V8 (tools/goldens/README.md), so every expression
//! clang fuses is written here as [`fma`] (`f64::mul_add`, correctly
//! rounded on every target, in software where there is no instruction), in
//! the operand order clang's `tryEmitFMulAdd` picks: the left operand of
//! `+`/`-` if it is a product, else the right one, with `c - a * b` as
//! `fma(-a, b, c)` and `a * b - c` as `fma(a, b, -c)`. Checked against V8's
//! own file built with `-ffp-contract=on` (the same answer as Node 26.10.0
//! on arm64 for 400,000 random arguments per function) and against the
//! `fmuladd` count clang emits per function.
//!
//! The original code is covered by these notices:
//!
//! ```text
//! Copyright (C) 1993-2004 by Sun Microsystems, Inc. All rights reserved.
//!
//! Developed at SunSoft, a Sun Microsystems, Inc. business.
//! Permission to use, copy, modify, and distribute this
//! software is freely granted, provided that this notice
//! is preserved.
//! ```
//!
//! ```text
//! Copyright 2016 the V8 project authors. All rights reserved.
//!
//! Redistribution and use in source and binary forms, with or without
//! modification, are permitted provided that the following conditions are
//! met:
//!
//!     * Redistributions of source code must retain the above copyright
//!       notice, this list of conditions and the following disclaimer.
//!     * Redistributions in binary form must reproduce the above
//!       copyright notice, this list of conditions and the following
//!       disclaimer in the documentation and/or other materials provided
//!       with the distribution.
//!     * Neither the name of Google Inc. nor the names of its
//!       contributors may be used to endorse or promote products derived
//!       from this software without specific prior written permission.
//!
//! THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
//! "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
//! LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
//! A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
//! OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//! SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
//! LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
//! DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
//! THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
//! (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
//! OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//! ```

// The constants are fdlibm's, digit for digit (some are pi/2, 1/ln 2 and so
// on to more digits than a double holds); `x - x` is how fdlibm makes a NaN
// from an infinity; the loops keep fdlibm's counters.
#![allow(
    clippy::excessive_precision,
    clippy::approx_constant,
    clippy::eq_op,
    clippy::explicit_counter_loop
)]

/// `a * b + c` rounded once, as arm64 V8 computes a contracted expression.
#[inline(always)]
fn fma(a: f64, b: f64, c: f64) -> f64 {
    a.mul_add(b, c)
}

/// `GET_HIGH_WORD`.
#[inline(always)]
fn high(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

/// `GET_LOW_WORD`.
#[inline(always)]
fn low(x: f64) -> u32 {
    x.to_bits() as u32
}

/// `INSERT_WORDS`.
#[inline(always)]
fn from_words(high: u32, low: u32) -> f64 {
    f64::from_bits((u64::from(high) << 32) | u64::from(low))
}

/// `SET_HIGH_WORD`.
#[inline(always)]
fn with_high(x: f64, high: u32) -> f64 {
    from_words(high, low(x))
}

/// `SET_LOW_WORD`.
#[inline(always)]
fn with_low(x: f64, low: u32) -> f64 {
    from_words(self::high(x) as u32, low)
}

/// C's `scalbn(x, n)`: `x * 2^n`, exact but for overflow and underflow
/// (musl's, which does the multiplications C99 describes).
fn scalbn(x: f64, mut n: i32) -> f64 {
    let x1p1023 = f64::from_bits(0x7fe0_0000_0000_0000);
    let x1p53 = f64::from_bits(0x4340_0000_0000_0000);
    let x1p_1022 = f64::from_bits(0x0010_0000_0000_0000);
    let mut y = x;
    if n > 1023 {
        y *= x1p1023;
        n -= 1023;
        if n > 1023 {
            y *= x1p1023;
            n -= 1023;
            if n > 1023 {
                n = 1023;
            }
        }
    } else if n < -1022 {
        // make sure final n < -53 to avoid double rounding in the subnormal
        // range
        y *= x1p_1022 * x1p53;
        n += 1022 - 53;
        if n < -1022 {
            y *= x1p_1022 * x1p53;
            n += 1022 - 53;
            if n < -1022 {
                n = -1022;
            }
        }
    }
    y * f64::from_bits(((0x3ff + i64::from(n)) as u64) << 52)
}

// -- argument reduction ----------------------------------------------------------

/// Table of constants for 2/pi, 396 hex digits (476 decimal) of 2/pi.
const TWO_OVER_PI: [i32; 66] = [
    0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C, 0x439041, 0xFE5163,
    0xABDEBB, 0xC561B7, 0x246E3A, 0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129,
    0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C, 0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C,
    0x845F8B, 0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D, 0x367ECF,
    0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5, 0xF17B3D, 0x0739F7, 0x8A5292,
    0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3,
    0x91615E, 0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA,
    0x73A8C9, 0x60E27B, 0xC08C6B,
];

const NPIO2_HW: [i32; 32] = [
    0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB, 0x402921FB,
    0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB,
    0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C,
    0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
];

/// `__ieee754_rem_pio2(x, y)`: `(n, y[0], y[1])` with `x - n*pi/2 = y[0] +
/// y[1]`, `|y[0] + y[1]| <= pi/4`.
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    const HALF: f64 = 5.00000000000000000000e-01;
    const TWO24: f64 = 1.67772160000000000000e+07;
    const INVPIO2: f64 = 6.36619772367581382433e-01; // 53 bits of 2/pi
    const PIO2_1: f64 = 1.57079632673412561417e+00; // first 33 bits of pi/2
    const PIO2_1T: f64 = 6.07710050650619224932e-11; // pi/2 - PIO2_1
    const PIO2_2: f64 = 6.07710050630396597660e-11; // second 33 bits of pi/2
    const PIO2_2T: f64 = 2.02226624879595063154e-21; // pi/2 - (PIO2_1+PIO2_2)
    const PIO2_3: f64 = 2.02226624871116645580e-21; // third 33 bits of pi/2
    const PIO2_3T: f64 = 8.47842766036889956997e-32; // pi/2 - (PIO2_1+PIO2_2+PIO2_3)

    let hx = high(x);
    let ix = hx & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        // |x| ~<= pi/4, no need for reduction
        return (0, x, 0.0);
    }
    if ix < 0x4002D97C {
        // |x| < 3pi/4, special case with n = +-1
        return if hx > 0 {
            let mut z = x - PIO2_1;
            if ix != 0x3FF921FB {
                // 33+53 bit pi is good enough
                let y0 = z - PIO2_1T;
                (1, y0, (z - y0) - PIO2_1T)
            } else {
                // near pi/2, use 33+33+53 bit pi
                z -= PIO2_2;
                let y0 = z - PIO2_2T;
                (1, y0, (z - y0) - PIO2_2T)
            }
        } else {
            let mut z = x + PIO2_1;
            if ix != 0x3FF921FB {
                let y0 = z + PIO2_1T;
                (-1, y0, (z - y0) + PIO2_1T)
            } else {
                z += PIO2_2;
                let y0 = z + PIO2_2T;
                (-1, y0, (z - y0) + PIO2_2T)
            }
        };
    }
    if ix <= 0x413921FB {
        // |x| ~<= 2^19*(pi/2), medium size
        let t = x.abs();
        let n = fma(t, INVPIO2, HALF) as i32;
        let fn_ = f64::from(n);
        let mut r = fma(-fn_, PIO2_1, t);
        let mut w = fn_ * PIO2_1T; // 1st round good to 85 bits
        let mut y0;
        if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
            y0 = r - w; // quick check no cancellation
        } else {
            let j = ix >> 20;
            y0 = r - w;
            let i = j - ((high(y0) >> 20) & 0x7FF);
            if i > 16 {
                // 2nd iteration needed, good to 118
                let t = r;
                w = fn_ * PIO2_2;
                r = t - w;
                w = fma(fn_, PIO2_2T, -((t - r) - w));
                y0 = r - w;
                let i = j - ((high(y0) >> 20) & 0x7FF);
                if i > 49 {
                    // 3rd iteration needed, 151 bits acc
                    let t = r;
                    w = fn_ * PIO2_3;
                    r = t - w;
                    w = fma(fn_, PIO2_3T, -((t - r) - w));
                    y0 = r - w;
                }
            }
        }
        let y1 = (r - y0) - w;
        return if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) };
    }
    // all other (large) arguments
    if ix >= 0x7FF00000 {
        // x is inf or NaN
        let y = x - x;
        return (0, y, y);
    }
    // set z = scalbn(|x|, ilogb(x) - 23)
    let mut z = with_low(0.0, low(x));
    let e0 = (ix >> 20) - 1046; // e0 = ilogb(z) - 23
    z = with_high(z, ix.wrapping_sub(((e0 as u32) << 20) as i32) as u32);
    let mut tx = [0.0; 3];
    for t in tx.iter_mut().take(2) {
        *t = f64::from(z as i32);
        z = (z - *t) * TWO24;
    }
    tx[2] = z;
    let mut nx = 3;
    while tx[nx - 1] == 0.0 {
        nx -= 1; // skip zero term
    }
    let (n, y0, y1) = kernel_rem_pio2(&tx[..nx], e0);
    if hx < 0 {
        (-n, -y0, -y1)
    } else {
        (n, y0, y1)
    }
}

/// `__kernel_rem_pio2(x, y, e0, nx, prec = 2, two_over_pi)`: the reduction
/// of a large argument given as 24-bit pieces `x`, with `e0` its exponent
/// less 23.
fn kernel_rem_pio2(x: &[f64], e0: i32) -> (i32, f64, f64) {
    const PIO2: [f64; 8] = [
        1.57079625129699707031e+00,
        7.54978941586159635335e-08,
        5.39030252995776476554e-15,
        3.28200341580791294123e-22,
        1.27065575308067607349e-29,
        1.22933308981111328932e-36,
        2.73370053816464559624e-44,
        2.16741683877804819444e-51,
    ];
    const TWO24: f64 = 1.67772160000000000000e+07;
    const TWON24: f64 = 5.96046447753906250000e-08;

    // prec 2 (53 bits): jk = init_jk[2]
    let jk: usize = 4;
    let jp = jk;
    let mut iq = [0i32; 20];
    let mut f = [0.0f64; 20];
    let mut fq = [0.0f64; 20];
    let mut q = [0.0f64; 20];

    // determine jx, jv, q0; note that 3 > q0
    let jx = x.len() - 1;
    let jv = ((e0 - 3) / 24).max(0);
    let mut q0 = e0 - 24 * (jv + 1);
    let jv = jv as usize;

    // set up f[0] to f[jx+jk] where f[jx+jk] = two_over_pi[jv+jk]
    let mut j = jv as isize - jx as isize;
    for fi in f.iter_mut().take(jx + jk + 1) {
        *fi = if j < 0 {
            0.0
        } else {
            f64::from(TWO_OVER_PI[j as usize])
        };
        j += 1;
    }

    // compute q[0], q[1], ... q[jk]
    for i in 0..=jk {
        let mut fw = 0.0;
        for j in 0..=jx {
            fw = fma(x[j], f[jx + i - j], fw);
        }
        q[i] = fw;
    }

    let mut jz = jk;
    let (n, ih, z) = loop {
        // distill q[] into iq[] reversingly
        let mut z = q[jz];
        let mut i = 0;
        let mut j = jz;
        while j > 0 {
            let fw = f64::from((TWON24 * z) as i32);
            iq[i] = fma(-TWO24, fw, z) as i32;
            z = q[j - 1] + fw;
            i += 1;
            j -= 1;
        }

        // compute n
        z = scalbn(z, q0); // actual value of z
        z = fma(-8.0, (z * 0.125).floor(), z); // trim off integer >= 8
        let mut n = z as i32;
        z -= f64::from(n);
        let mut ih = 0;
        if q0 > 0 {
            // need iq[jz-1] to determine n
            let i = iq[jz - 1] >> (24 - q0);
            n += i;
            iq[jz - 1] -= i << (24 - q0);
            ih = iq[jz - 1] >> (23 - q0);
        } else if q0 == 0 {
            ih = iq[jz - 1] >> 23;
        } else if z >= 0.5 {
            ih = 2;
        }

        if ih > 0 {
            // q > 0.5
            n += 1;
            let mut carry = 0;
            for v in iq.iter_mut().take(jz) {
                // compute 1 - q
                let j = *v;
                if carry == 0 {
                    if j != 0 {
                        carry = 1;
                        *v = 0x1000000 - j;
                    }
                } else {
                    *v = 0xFFFFFF - j;
                }
            }
            if q0 > 0 {
                // rare case: chance is 1 in 12
                match q0 {
                    1 => iq[jz - 1] &= 0x7FFFFF,
                    2 => iq[jz - 1] &= 0x3FFFFF,
                    _ => {}
                }
            }
            if ih == 2 {
                z = 1.0 - z;
                if carry != 0 {
                    z -= scalbn(1.0, q0);
                }
            }
        }

        // check if recomputation is needed
        if z == 0.0 {
            let mut j = 0;
            for v in &iq[jk..jz] {
                j |= *v;
            }
            if j == 0 {
                // need recomputation
                let mut k = 1;
                while k <= jk && iq[jk - k] == 0 {
                    k += 1; // k = no. of terms needed
                }
                for i in jz + 1..=jz + k {
                    // add q[jz+1] to q[jz+k]
                    f[jx + i] = f64::from(TWO_OVER_PI[jv + i]);
                    let mut fw = 0.0;
                    for j in 0..=jx {
                        fw = fma(x[j], f[jx + i - j], fw);
                    }
                    q[i] = fw;
                }
                jz += k;
                continue;
            }
        }
        break (n, ih, z);
    };

    // chop off zero terms
    if z == 0.0 {
        jz -= 1;
        q0 -= 24;
        while iq[jz] == 0 {
            jz -= 1;
            q0 -= 24;
        }
    } else {
        // break z into 24-bit if necessary
        let z = scalbn(z, -q0);
        if z >= TWO24 {
            let fw = f64::from((TWON24 * z) as i32);
            iq[jz] = fma(-TWO24, fw, z) as i32;
            jz += 1;
            q0 += 24;
            iq[jz] = fw as i32;
        } else {
            iq[jz] = z as i32;
        }
    }

    // convert integer "bit" chunk to floating-point value
    let mut fw = scalbn(1.0, q0);
    for i in (0..=jz).rev() {
        q[i] = fw * f64::from(iq[i]);
        fw *= TWON24;
    }

    // compute PIo2[0,...,jp]*q[jz,...,0]
    for i in (0..=jz).rev() {
        let mut fw = 0.0;
        let mut k = 0;
        while k <= jp && k <= jz - i {
            fw = fma(PIO2[k], q[i + k], fw);
            k += 1;
        }
        fq[jz - i] = fw;
    }

    // compress fq[] into y[]
    let mut fw = 0.0;
    for v in fq[..=jz].iter().rev() {
        fw += *v;
    }
    let y0 = if ih == 0 { fw } else { -fw };
    fw = fq[0] - fw;
    for v in &fq[1..=jz] {
        fw += *v;
    }
    let y1 = if ih == 0 { fw } else { -fw };
    (n & 7, y0, y1)
}

// -- kernels -----------------------------------------------------------------------

/// `__kernel_cos(x, y)`: cos on [-pi/4, pi/4], `y` the tail of `x`.
fn kernel_cos(x: f64, y: f64) -> f64 {
    const C1: f64 = 4.16666666666666019037e-02;
    const C2: f64 = -1.38888888888741095749e-03;
    const C3: f64 = 2.48015872894767294178e-05;
    const C4: f64 = -2.75573143513906633035e-07;
    const C5: f64 = 2.08757232129817482790e-09;
    const C6: f64 = -1.13596475577881948265e-11;

    let ix = high(x) & 0x7FFFFFFF;
    if ix < 0x3E400000 && x as i32 == 0 {
        // |x| < 2^-27, generate inexact
        return 1.0;
    }
    let z = x * x;
    let r = z * fma(z, fma(z, fma(z, fma(z, fma(z, C6, C5), C4), C3), C2), C1);
    if ix < 0x3FD33333 {
        // |x| < 0.3
        1.0 - fma(0.5, z, -fma(z, r, -(x * y)))
    } else {
        let qx = if ix > 0x3FE90000 {
            // x > 0.78125
            0.28125
        } else {
            from_words((ix - 0x00200000) as u32, 0) // x/4
        };
        let iz = fma(0.5, z, -qx);
        let a = 1.0 - qx;
        a - (iz - fma(z, r, -(x * y)))
    }
}

/// `__kernel_sin(x, y, iy)`: sin on [-pi/4, pi/4]; `iy` = 0 when `y` is 0.
fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    const HALF: f64 = 5.00000000000000000000e-01;
    const S1: f64 = -1.66666666666666324348e-01;
    const S2: f64 = 8.33333333332248946124e-03;
    const S3: f64 = -1.98412698298579493134e-04;
    const S4: f64 = 2.75573137070700676789e-06;
    const S5: f64 = -2.50507602534068634195e-08;
    const S6: f64 = 1.58969099521155010221e-10;

    let ix = high(x) & 0x7FFFFFFF;
    if ix < 0x3E400000 && x as i32 == 0 {
        // |x| < 2^-27, generate inexact
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = fma(z, fma(z, fma(z, fma(z, S6, S5), S4), S3), S2);
    if iy == 0 {
        fma(v, fma(z, r, S1), x)
    } else {
        x - fma(-v, S1, fma(z, fma(HALF, y, -(v * r)), -y))
    }
}

/// `__kernel_tan(x, y, iy)`: tan on [-pi/4, pi/4]; `iy` = 1 for tan, -1 for
/// -1/tan.
fn kernel_tan(x: f64, y: f64, iy: i32) -> f64 {
    const T: [f64; 13] = [
        3.33333333333334091986e-01,
        1.33333333333201242699e-01,
        5.39682539762260521377e-02,
        2.18694882948595424599e-02,
        8.86323982359930005737e-03,
        3.59207910759131235356e-03,
        1.45620945432529025516e-03,
        5.88041240820264096874e-04,
        2.46463134818469906812e-04,
        7.81794442939557092300e-05,
        7.14072491382608190305e-05,
        -1.85586374855275456654e-05,
        2.59073051863633712884e-05,
    ];
    const ONE: f64 = 1.00000000000000000000e+00;
    const PIO4: f64 = 7.85398163397448278999e-01;
    const PIO4LO: f64 = 3.06161699786838301793e-17;

    let hx = high(x);
    let ix = hx & 0x7FFFFFFF;
    if ix < 0x3E300000 && x as i32 == 0 {
        // x < 2^-28, generate inexact
        if ((ix as u32 | low(x)) | (iy + 1) as u32) == 0 {
            return ONE / x.abs();
        }
        if iy == 1 {
            return x;
        }
        // compute -1 / (x+y) carefully
        let w = x + y;
        let z = with_low(w, 0);
        let v = y - (z - x);
        let a = -ONE / w;
        let t = with_low(a, 0);
        let s = fma(t, z, ONE);
        return fma(a, fma(t, v, s), t);
    }
    let (mut x, mut y) = (x, y);
    let big = ix >= 0x3FE59428; // |x| >= 0.6744
    if big {
        if hx < 0 {
            x = -x;
            y = -y;
        }
        let z = PIO4 - x;
        let w = PIO4LO - y;
        x = z + w;
        y = 0.0;
    }
    let z = x * x;
    let w = z * z;
    // Break x^5*(T[1]+x^2*T[2]+...) into
    // x^5(T[1]+x^4*T[3]+...+x^20*T[11]) +
    // x^5(x^2*(T[2]+x^4*T[4]+...+x^22*[T12]))
    let mut r = fma(
        w,
        fma(w, fma(w, fma(w, fma(w, T[11], T[9]), T[7]), T[5]), T[3]),
        T[1],
    );
    let v = z * fma(
        w,
        fma(w, fma(w, fma(w, fma(w, T[12], T[10]), T[8]), T[6]), T[4]),
        T[2],
    );
    let s = z * x;
    r = fma(z, fma(s, r + v, y), y);
    r = fma(T[0], s, r);
    let w = x + r;
    if big {
        let v = f64::from(iy);
        return f64::from(1 - ((hx >> 30) & 2)) * fma(-2.0, x - (w * w / (w + v) - r), v);
    }
    if iy == 1 {
        return w;
    }
    // compute -1.0 / (x+r) accurately
    let z = with_low(w, 0);
    let v = r - (z - x); // z+v = r+x
    let a = -1.0 / w;
    let t = with_low(a, 0);
    let s = fma(t, z, 1.0);
    fma(a, fma(t, v, s), t)
}

// -- the functions -------------------------------------------------------------------

/// `sin`.
pub fn sin(x: f64) -> f64 {
    let ix = high(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        return kernel_sin(x, 0.0, 0);
    }
    if ix >= 0x7FF00000 {
        return x - x; // sin(Inf or NaN) is NaN
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y0, y1, 1),
        1 => kernel_cos(y0, y1),
        2 => -kernel_sin(y0, y1, 1),
        _ => -kernel_cos(y0, y1),
    }
}

/// `cos`.
pub fn cos(x: f64) -> f64 {
    let ix = high(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        return kernel_cos(x, 0.0);
    }
    if ix >= 0x7FF00000 {
        return x - x; // cos(Inf or NaN) is NaN
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y0, y1),
        1 => -kernel_sin(y0, y1, 1),
        2 => -kernel_cos(y0, y1),
        _ => kernel_sin(y0, y1, 1),
    }
}

/// `tan`.
pub fn tan(x: f64) -> f64 {
    let ix = high(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        return kernel_tan(x, 0.0, 1);
    }
    if ix >= 0x7FF00000 {
        return x - x; // NaN
    }
    let (n, y0, y1) = rem_pio2(x);
    // 1 -> n even, -1 -> n odd
    kernel_tan(y0, y1, 1 - ((n & 1) << 1))
}

const PS0: f64 = 1.66666666666666657415e-01;
const PS1: f64 = -3.25565818622400915405e-01;
const PS2: f64 = 2.01212532134862925881e-01;
const PS3: f64 = -4.00555345006794114027e-02;
const PS4: f64 = 7.91534994289814532176e-04;
const PS5: f64 = 3.47933107596021167570e-05;
const QS1: f64 = -2.40339491173441421878e+00;
const QS2: f64 = 2.02094576023350569471e+00;
const QS3: f64 = -6.88283971605453293030e-01;
const QS4: f64 = 7.70381505559019352791e-02;
const PIO2_HI: f64 = 1.57079632679489655800e+00;
const PIO2_LO: f64 = 6.12323399573676603587e-17;

/// `t * (pS0 + t * (pS1 + ... + t * pS5))`, as asin and acos write it.
#[inline(always)]
fn asin_p(t: f64) -> f64 {
    t * fma(
        t,
        fma(t, fma(t, fma(t, fma(t, PS5, PS4), PS3), PS2), PS1),
        PS0,
    )
}

/// `1 + t * (qS1 + t * (qS2 + t * (qS3 + t * qS4)))`.
#[inline(always)]
fn asin_q(t: f64) -> f64 {
    fma(t, fma(t, fma(t, fma(t, QS4, QS3), QS2), QS1), 1.0)
}

/// `asin`.
pub fn asin(x: f64) -> f64 {
    const HUGE: f64 = 1.000e+300;
    const PIO4_HI: f64 = 7.85398163397448278999e-01;

    let hx = high(x);
    let ix = hx & 0x7FFFFFFF;
    if ix >= 0x3FF00000 {
        // |x| >= 1
        if ((ix - 0x3FF00000) as u32 | low(x)) == 0 {
            // asin(1) = +-pi/2 with inexact
            return fma(x, PIO2_HI, x * PIO2_LO);
        }
        return f64::NAN; // asin(|x|>1) is NaN
    }
    if ix < 0x3FE00000 {
        // |x| < 0.5
        let mut t = 0.0;
        if ix < 0x3E400000 {
            // |x| < 2^-27
            if HUGE + x > 1.0 {
                return x; // return x with inexact if x != 0
            }
        } else {
            t = x * x;
        }
        let w = asin_p(t) / asin_q(t);
        return fma(x, w, x);
    }
    // 1 > |x| >= 0.5
    let w = 1.0 - x.abs();
    let t = w * 0.5;
    let p = asin_p(t);
    let q = asin_q(t);
    let s = t.sqrt();
    let t = if ix >= 0x3FEF3333 {
        // |x| > 0.975
        let w = p / q;
        PIO2_HI - fma(2.0, fma(s, w, s), -PIO2_LO)
    } else {
        let w = with_low(s, 0);
        let c = fma(-w, w, t) / (s + w);
        let r = p / q;
        let p = fma(2.0 * s, r, -fma(-2.0, c, PIO2_LO));
        let q = fma(-2.0, w, PIO4_HI);
        PIO4_HI - (p - q)
    };
    if hx > 0 {
        t
    } else {
        -t
    }
}

/// `acos`.
pub fn acos(x: f64) -> f64 {
    const PI: f64 = 3.14159265358979311600e+00;

    let hx = high(x);
    let ix = hx & 0x7FFFFFFF;
    if ix >= 0x3FF00000 {
        // |x| >= 1
        if ((ix - 0x3FF00000) as u32 | low(x)) == 0 {
            // |x| == 1
            return if hx > 0 { 0.0 } else { PI + 2.0 * PIO2_LO };
        }
        return f64::NAN; // acos(|x|>1) is NaN
    }
    if ix < 0x3FE00000 {
        // |x| < 0.5
        if ix <= 0x3C600000 {
            return PIO2_HI + PIO2_LO; // |x| < 2^-57
        }
        let z = x * x;
        let r = asin_p(z) / asin_q(z);
        PIO2_HI - (x - fma(-x, r, PIO2_LO))
    } else if hx < 0 {
        // x < -0.5
        let z = (1.0 + x) * 0.5;
        let p = asin_p(z);
        let q = asin_q(z);
        let s = z.sqrt();
        let r = p / q;
        let w = fma(r, s, -PIO2_LO);
        fma(-2.0, s + w, PI)
    } else {
        // x > 0.5
        let z = (1.0 - x) * 0.5;
        let s = z.sqrt();
        let df = with_low(s, 0);
        let c = fma(-df, df, z) / (s + df);
        let p = asin_p(z);
        let q = asin_q(z);
        let r = p / q;
        let w = fma(r, s, c);
        2.0 * (df + w)
    }
}

/// `atan`.
pub fn atan(x: f64) -> f64 {
    const ATANHI: [f64; 4] = [
        4.63647609000806093515e-01, // atan(0.5)hi
        7.85398163397448278999e-01, // atan(1.0)hi
        9.82793723247329054082e-01, // atan(1.5)hi
        1.57079632679489655800e+00, // atan(inf)hi
    ];
    const ATANLO: [f64; 4] = [
        2.26987774529616870924e-17, // atan(0.5)lo
        3.06161699786838301793e-17, // atan(1.0)lo
        1.39033110312309984516e-17, // atan(1.5)lo
        6.12323399573676603587e-17, // atan(inf)lo
    ];
    const AT: [f64; 11] = [
        3.33333333333329318027e-01,
        -1.99999999998764832476e-01,
        1.42857142725034663711e-01,
        -1.11111104054623557880e-01,
        9.09088713343650656196e-02,
        -7.69187620504482999495e-02,
        6.66107313738753120669e-02,
        -5.83357013379057348645e-02,
        4.97687799461593236017e-02,
        -3.65315727442169155270e-02,
        1.62858201153657823623e-02,
    ];
    const HUGE: f64 = 1.0e300;

    let hx = high(x);
    let ix = hx & 0x7FFFFFFF;
    if ix >= 0x44100000 {
        // |x| >= 2^66
        if ix > 0x7FF00000 || (ix == 0x7FF00000 && low(x) != 0) {
            return x + x; // NaN
        }
        return if hx > 0 {
            ATANHI[3] + ATANLO[3]
        } else {
            -ATANHI[3] - ATANLO[3]
        };
    }
    let mut x = x;
    let id: Option<usize>;
    if ix < 0x3FDC0000 {
        // |x| < 0.4375
        if ix < 0x3E400000 && HUGE + x > 1.0 {
            return x; // |x| < 2^-27, raise inexact
        }
        id = None;
    } else {
        x = x.abs();
        if ix < 0x3FF30000 {
            // |x| < 1.1875
            if ix < 0x3FE60000 {
                // 7/16 <= |x| < 11/16
                id = Some(0);
                x = fma(2.0, x, -1.0) / (2.0 + x);
            } else {
                // 11/16 <= |x| < 19/16
                id = Some(1);
                x = (x - 1.0) / (x + 1.0);
            }
        } else if ix < 0x40038000 {
            // |x| < 2.4375
            id = Some(2);
            x = (x - 1.5) / fma(1.5, x, 1.0);
        } else {
            // 2.4375 <= |x| < 2^66
            id = Some(3);
            x = -1.0 / x;
        }
    }
    // end of argument reduction
    let z = x * x;
    let w = z * z;
    // break sum from i=0 to 10 aT[i]z**(i+1) into odd and even poly
    let s1 = z * fma(
        w,
        fma(
            w,
            fma(w, fma(w, fma(w, AT[10], AT[8]), AT[6]), AT[4]),
            AT[2],
        ),
        AT[0],
    );
    let s2 = w * fma(w, fma(w, fma(w, fma(w, AT[9], AT[7]), AT[5]), AT[3]), AT[1]);
    match id {
        None => fma(-x, s1 + s2, x),
        Some(id) => {
            let z = ATANHI[id] - (fma(x, s1 + s2, -ATANLO[id]) - x);
            if hx < 0 {
                -z
            } else {
                z
            }
        }
    }
}

/// `atan2`.
pub fn atan2(y: f64, x: f64) -> f64 {
    const TINY: f64 = 1.0e-300;
    const PI_O_4: f64 = 7.8539816339744827900E-01;
    const PI_O_2: f64 = 1.5707963267948965580E+00;
    const PI: f64 = 3.1415926535897931160E+00;
    const PI_LO: f64 = 1.2246467991473531772E-16;

    if x.is_nan() || y.is_nan() {
        return x + y; // x or y is NaN
    }
    let (hx, lx) = (high(x), low(x));
    let ix = hx & 0x7FFFFFFF;
    let (hy, ly) = (high(y), low(y));
    let iy = hy & 0x7FFFFFFF;
    if (hx.wrapping_sub(0x3FF00000) as u32 | lx) == 0 {
        return atan(y); // x = 1.0
    }
    let m = ((hy >> 31) & 1) | ((hx >> 30) & 2); // 2*sign(x)+sign(y)

    // when y = 0
    if (iy as u32 | ly) == 0 {
        match m {
            0 | 1 => return y,      // atan(+-0,+anything) = +-0
            2 => return PI + TINY,  // atan(+0,-anything) = pi
            _ => return -PI - TINY, // atan(-0,-anything) = -pi
        }
    }
    // when x = 0
    if (ix as u32 | lx) == 0 {
        return if hy < 0 {
            -PI_O_2 - TINY
        } else {
            PI_O_2 + TINY
        };
    }
    // when x is INF
    if ix == 0x7FF00000 {
        if iy == 0x7FF00000 {
            return match m {
                0 => PI_O_4 + TINY,        // atan(+INF,+INF)
                1 => -PI_O_4 - TINY,       // atan(-INF,+INF)
                2 => 3.0 * PI_O_4 + TINY,  // atan(+INF,-INF)
                _ => -3.0 * PI_O_4 - TINY, // atan(-INF,-INF)
            };
        }
        return match m {
            0 => 0.0,        // atan(+...,+INF)
            1 => -0.0,       // atan(-...,+INF)
            2 => PI + TINY,  // atan(+...,-INF)
            _ => -PI - TINY, // atan(-...,-INF)
        };
    }
    // when y is INF
    if iy == 0x7FF00000 {
        return if hy < 0 {
            -PI_O_2 - TINY
        } else {
            PI_O_2 + TINY
        };
    }

    // compute y/x
    let k = (iy - ix) >> 20;
    let mut m = m;
    let z = if k > 60 {
        // |y/x| > 2^60
        m &= 1;
        fma(0.5, PI_LO, PI_O_2)
    } else if hx < 0 && k < -60 {
        0.0 // 0 > |y|/x > -2^-60
    } else {
        atan((y / x).abs()) // safe to do y/x
    };
    match m {
        0 => z,                // atan(+,+)
        1 => -z,               // atan(-,+)
        2 => PI - (z - PI_LO), // atan(+,-)
        _ => (z - PI_LO) - PI, // atan(-,-)
    }
}

/// `exp`.
pub fn exp(x: f64) -> f64 {
    const HALF: [f64; 2] = [0.5, -0.5];
    const O_THRESHOLD: f64 = 7.09782712893383973096e+02;
    const U_THRESHOLD: f64 = -7.45133219101941108420e+02;
    const LN2HI: [f64; 2] = [6.93147180369123816490e-01, -6.93147180369123816490e-01];
    const LN2LO: [f64; 2] = [1.90821492927058770002e-10, -1.90821492927058770002e-10];
    const INVLN2: f64 = 1.44269504088896338700e+00;
    const P1: f64 = 1.66666666666666019037e-01;
    const P2: f64 = -2.77777777770155933842e-03;
    const P3: f64 = 6.61375632143793436117e-05;
    const P4: f64 = -1.65339022054652515390e-06;
    const P5: f64 = 4.13813679705723846039e-08;
    const E: f64 = 2.718281828459045;
    const HUGE: f64 = 1.0e+300;
    const TWOM1000: f64 = 9.33263618503218878990e-302; // 2^-1000
    const TWO1023: f64 = 8.988465674311579539e307; // 2^1023

    let hx = high(x) as u32;
    let xsb = ((hx >> 31) & 1) as usize; // sign bit of x
    let hx = hx & 0x7FFFFFFF; // high word of |x|

    // filter out non-finite argument
    if hx >= 0x40862E42 {
        // |x| >= 709.78...
        if hx >= 0x7FF00000 {
            if ((hx & 0xFFFFF) | low(x)) != 0 {
                return x + x; // NaN
            }
            return if xsb == 0 { x } else { 0.0 }; // exp(+-inf) = {inf, 0}
        }
        if x > O_THRESHOLD {
            return HUGE * HUGE; // overflow
        }
        if x < U_THRESHOLD {
            return TWOM1000 * TWOM1000; // underflow
        }
    }

    // argument reduction
    let mut x = x;
    let (mut hi, mut lo, mut k) = (0.0, 0.0, 0i32);
    if hx > 0x3FD62E42 {
        // |x| > 0.5 ln2
        if hx < 0x3FF0A2B2 {
            // and |x| < 1.5 ln2
            if x == 1.0 {
                return E;
            }
            hi = x - LN2HI[xsb];
            lo = LN2LO[xsb];
            k = 1 - xsb as i32 - xsb as i32;
        } else {
            k = fma(INVLN2, x, HALF[xsb]) as i32;
            let t = f64::from(k);
            hi = fma(-t, LN2HI[0], x); // t*ln2HI is exact here
            lo = t * LN2LO[0];
        }
        x = hi - lo;
    } else if hx < 0x3E300000 {
        // |x| < 2^-28
        if HUGE + x > 1.0 {
            return 1.0 + x; // trigger inexact
        }
    }

    // x is now in primary range
    let t = x * x;
    let twopk = if k >= -1021 {
        from_words(0x3FF00000u32.wrapping_add((k as u32) << 20), 0)
    } else {
        from_words(0x3FF00000u32.wrapping_add(((k + 1000) as u32) << 20), 0)
    };
    let c = fma(-t, fma(t, fma(t, fma(t, fma(t, P5, P4), P3), P2), P1), x);
    if k == 0 {
        return 1.0 - ((x * c) / (c - 2.0) - x);
    }
    let y = 1.0 - ((lo - (x * c) / (2.0 - c)) - hi);
    if k >= -1021 {
        if k == 1024 {
            return y * 2.0 * TWO1023;
        }
        return y * twopk;
    }
    y * twopk * TWOM1000
}

const TWO54: f64 = 1.80143985094819840000e+16;
const LG1: f64 = 6.666666666666735130e-01;
const LG2: f64 = 3.999999999940941908e-01;
const LG3: f64 = 2.857142874366239149e-01;
const LG4: f64 = 2.222219843214978396e-01;
const LG5: f64 = 1.818357216161805012e-01;
const LG6: f64 = 1.531383769920937332e-01;
const LG7: f64 = 1.479819860511658591e-01;

/// `log` (natural logarithm).
pub fn log(x: f64) -> f64 {
    const LN2_HI: f64 = 6.93147180369123816490e-01;
    const LN2_LO: f64 = 1.90821492927058770002e-10;

    let mut hx = high(x);
    let lx = low(x);
    let mut x = x;
    let mut k = 0;
    if hx < 0x00100000 {
        // x < 2^-1022
        if ((hx & 0x7FFFFFFF) as u32 | lx) == 0 {
            return f64::NEG_INFINITY; // log(+-0) = -inf
        }
        if hx < 0 {
            return f64::NAN; // log(-#) = NaN
        }
        k -= 54;
        x *= TWO54; // subnormal number, scale up x
        hx = high(x);
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000FFFFF;
    let i = (hx + 0x95F64) & 0x100000;
    x = with_high(x, (hx | (i ^ 0x3FF00000)) as u32); // normalize x or x/2
    k += i >> 20;
    let f = x - 1.0;
    if (0x000FFFFF & (2 + hx)) < 3 {
        // -2^-20 <= f < 2^-20
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = f64::from(k);
            return fma(dk, LN2_HI, dk * LN2_LO);
        }
        let r = f * f * fma(-0.33333333333333333, f, 0.5);
        if k == 0 {
            return f - r;
        }
        let dk = f64::from(k);
        return fma(dk, LN2_HI, -(fma(-dk, LN2_LO, r) - f));
    }
    let s = f / (2.0 + f);
    let dk = f64::from(k);
    let z = s * s;
    let mut i = hx - 0x6147A;
    let w = z * z;
    let j = 0x6B851 - hx;
    let t1 = w * fma(w, fma(w, LG6, LG4), LG2);
    let t2 = z * fma(w, fma(w, fma(w, LG7, LG5), LG3), LG1);
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - fma(-s, hfsq + r, hfsq)
        } else {
            fma(dk, LN2_HI, -((hfsq - fma(s, hfsq + r, dk * LN2_LO)) - f))
        }
    } else if k == 0 {
        fma(-s, f - r, f)
    } else {
        fma(dk, LN2_HI, -(fma(s, f - r, -(dk * LN2_LO)) - f))
    }
}

/// `k_log1p(f)`: `log(1 + f) - f + f * f / 2` for `1 + f` in [sqrt(2)/2,
/// sqrt(2)].
fn k_log1p(f: f64) -> f64 {
    let s = f / (2.0 + f);
    let z = s * s;
    let w = z * z;
    let t1 = w * fma(w, fma(w, LG6, LG4), LG2);
    let t2 = z * fma(w, fma(w, fma(w, LG7, LG5), LG3), LG1);
    let r = t2 + t1;
    let hfsq = 0.5 * f * f;
    s * (hfsq + r)
}

/// `log2`.
pub fn log2(x: f64) -> f64 {
    const IVLN2HI: f64 = 1.44269504072144627571e+00;
    const IVLN2LO: f64 = 1.67517131648865118353e-10;

    let mut hx = high(x);
    let lx = low(x);
    let mut x = x;
    let mut k = 0;
    if hx < 0x00100000 {
        // x < 2^-1022
        if ((hx & 0x7FFFFFFF) as u32 | lx) == 0 {
            return f64::NEG_INFINITY; // log(+-0) = -inf
        }
        if hx < 0 {
            return f64::NAN; // log(-#) = NaN
        }
        k -= 54;
        x *= TWO54; // subnormal number, scale up x
        hx = high(x);
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    if hx == 0x3FF00000 && lx == 0 {
        return 0.0; // log(1) = +0
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000FFFFF;
    let i = (hx + 0x95F64) & 0x100000;
    x = with_high(x, (hx | (i ^ 0x3FF00000)) as u32); // normalize x or x/2
    k += i >> 20;
    let y = f64::from(k);
    let f = x - 1.0;
    let hfsq = 0.5 * f * f;
    let r = k_log1p(f);

    let hi = with_low(f - hfsq, 0);
    let lo = (f - hi) - hfsq + r;
    let val_hi = hi * IVLN2HI;
    let mut val_lo = fma(lo + hi, IVLN2LO, lo * IVLN2HI);

    // spadd(val_hi, val_lo, y), except for not using double_t
    let w = y + val_hi;
    val_lo += (y - w) + val_hi;
    val_lo + w
}

/// `log10`.
pub fn log10(x: f64) -> f64 {
    const IVLN10: f64 = 4.34294481903251816668e-01;
    const LOG10_2HI: f64 = 3.01029995663611771306e-01;
    const LOG10_2LO: f64 = 3.69423907715893078616e-13;

    let mut hx = high(x);
    let mut lx = low(x);
    let mut x = x;
    let mut k = 0;
    if hx < 0x00100000 {
        // x < 2^-1022
        if ((hx & 0x7FFFFFFF) as u32 | lx) == 0 {
            return f64::NEG_INFINITY; // log(+-0) = -inf
        }
        if hx < 0 {
            return f64::NAN; // log(-#) = NaN
        }
        k -= 54;
        x *= TWO54; // subnormal number, scale up x
        hx = high(x);
        lx = low(x);
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    if hx == 0x3FF00000 && lx == 0 {
        return 0.0; // log(1) = +0
    }
    k += (hx >> 20) - 1023;

    let i = ((k as u32 & 0x80000000) >> 31) as i32;
    hx = (hx & 0x000FFFFF) | ((0x3FF - i) << 20);
    let y = f64::from(k + i);
    x = from_words(hx as u32, lx);

    let z = fma(y, LOG10_2LO, IVLN10 * log(x));
    fma(y, LOG10_2HI, z)
}

/// `cbrt`.
pub fn cbrt(x: f64) -> f64 {
    const B1: u32 = 715094163; // B1 = (1023-1023/3-0.03306235651)*2**20
    const B2: u32 = 696219795; // B2 = (1023-1023/3-54/3-0.03306235651)*2**20
                               // |1/cbrt(x) - p(x)| < 2**-23.5 (~[-7.93e-8, 7.929e-8])
    const P0: f64 = 1.87595182427177009643;
    const P1: f64 = -1.88497979543377169875;
    const P2: f64 = 1.621429720105354466140;
    const P3: f64 = -0.758397934778766047437;
    const P4: f64 = 0.145996192886612446982;

    let hx = high(x) as u32;
    let low = low(x);
    let sign = hx & 0x80000000;
    let hx = hx ^ sign;
    if hx >= 0x7FF00000 {
        return x + x; // cbrt(NaN, INF) is itself
    }

    // rough cbrt to 5 bits
    let mut t = if hx < 0x00100000 {
        // zero or subnormal
        if (hx | low) == 0 {
            return x; // cbrt(0) is itself
        }
        let t = from_words(0x43500000, 0) * x; // t = 2^54 * x
        let high = high(t) as u32;
        from_words(sign | ((high & 0x7FFFFFFF) / 3 + B2), 0)
    } else {
        from_words(sign | (hx / 3 + B1), 0)
    };

    // new cbrt to 23 bits
    let r = (t * t) * (t / x);
    t *= fma((r * r) * r, fma(r, P4, P3), fma(r, fma(r, P2, P1), P0));

    // round t away from zero to 23 bits
    t = f64::from_bits(t.to_bits().wrapping_add(0x80000000) & 0xFFFFFFFFC0000000);

    // one step Newton iteration to 53 bits with error < 0.667 ulps
    let s = t * t; // t*t is exact
    let r = x / s; // error <= 0.5 ulps; |r| < |t|
    let w = t + t; // t+t is exact
    let r = (r - t) / (w + r); // r-t is exact; w+r ~= 3*t
    fma(t, r, t) // error <= 0.5 + 0.5/3 + epsilon
}
