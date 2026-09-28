//! Skia's dasher, ported from Skia at `chrome/m153`
//! (f8b66b7597c4cc859d3ed190e9c6872241e6721c): `SkDashPath.cpp`
//! (`InternalFilter`, `CalcDashParameters`, the cull of lines and
//! rectangles, `SpecialLineRec`) over `SkContourMeasure.cpp` (arc length
//! by recursive subdivision, `getSegment`). It walks conics as conics, as
//! Chrome's canvas does for a dashed `arc()`. Arithmetic is `f32`, as
//! Skia's, with the dash distance in doubles where Skia keeps it so.

use crate::edges::Seg;

type F = (f32, f32);

fn f(p: (f64, f64)) -> F {
    (p.0 as f32, p.1 as f32)
}

fn d(p: F) -> (f64, f64) {
    (f64::from(p.0), f64::from(p.1))
}

fn interp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp(a: F, b: F, t: f32) -> F {
    (interp(a.0, b.0, t), interp(a.1, b.1, t))
}

/// `SkPoint::Length`.
fn length(dx: f32, dy: f32) -> f32 {
    let mag2 = dx * dx + dy * dy;
    if mag2.is_finite() {
        mag2.sqrt()
    } else {
        (f64::from(dx) * f64::from(dx) + f64::from(dy) * f64::from(dy)).sqrt() as f32
    }
}

fn distance(a: F, b: F) -> f32 {
    length(b.0 - a.0, b.1 - a.1)
}

// ---------------------------------------------------------------------------
// Curve evaluation and chopping (SkGeometry.cpp)

fn eval_quad(p: [F; 3], t: f32) -> F {
    let c = p[0];
    let b = ((p[1].0 - c.0) * 2.0, (p[1].1 - c.1) * 2.0);
    let a = (p[2].0 - p[1].0 * 2.0 + c.0, p[2].1 - p[1].1 * 2.0 + c.1);
    ((a.0 * t + b.0) * t + c.0, (a.1 * t + b.1) * t + c.1)
}

fn eval_cubic(p: [F; 4], t: f32) -> F {
    let a = (
        p[3].0 + 3.0 * (p[1].0 - p[2].0) - p[0].0,
        p[3].1 + 3.0 * (p[1].1 - p[2].1) - p[0].1,
    );
    let b = (
        3.0 * (p[2].0 - p[1].0 * 2.0 + p[0].0),
        3.0 * (p[2].1 - p[1].1 * 2.0 + p[0].1),
    );
    let c = (3.0 * (p[1].0 - p[0].0), 3.0 * (p[1].1 - p[0].1));
    (
        ((a.0 * t + b.0) * t + c.0) * t + p[0].0,
        ((a.1 * t + b.1) * t + c.1) * t + p[0].1,
    )
}

/// `SkConicCoeff`: numerator and denominator polynomials.
fn conic_coeff(p: [F; 3], w: f32) -> ([F; 3], [f32; 3]) {
    let p1w = (p[1].0 * w, p[1].1 * w);
    let na = (p[2].0 - p1w.0 * 2.0 + p[0].0, p[2].1 - p1w.1 * 2.0 + p[0].1);
    let nb = ((p1w.0 - p[0].0) * 2.0, (p1w.1 - p[0].1) * 2.0);
    let db = (w - 1.0) * 2.0;
    ([na, nb, p[0]], [-db, db, 1.0])
}

fn eval_conic(p: [F; 3], w: f32, t: f32) -> F {
    let (n, dn) = conic_coeff(p, w);
    let num = (
        (n[0].0 * t + n[1].0) * t + n[2].0,
        (n[0].1 * t + n[1].1) * t + n[2].1,
    );
    let den = (dn[0] * t + dn[1]) * t + dn[2];
    (num.0 / den, num.1 / den)
}

fn chop_quad(p: [F; 3], t: f32) -> [F; 5] {
    let p01 = lerp(p[0], p[1], t);
    let p12 = lerp(p[1], p[2], t);
    [p[0], p01, lerp(p01, p12, t), p12, p[2]]
}

fn chop_cubic(c: [F; 4], t: f32) -> [F; 7] {
    if t == 1.0 {
        return [c[0], c[1], c[2], c[3], c[3], c[3], c[3]];
    }
    let mix = |a: F, b: F| ((b.0 - a.0) * t + a.0, (b.1 - a.1) * t + a.1);
    let ab = mix(c[0], c[1]);
    let bc = mix(c[1], c[2]);
    let cd = mix(c[2], c[3]);
    let abc = mix(ab, bc);
    let bcd = mix(bc, cd);
    let abcd = mix(abc, bcd);
    [c[0], ab, abc, abcd, bcd, cd, c[3]]
}

/// `SkConic::chopAt(t, dst[2])`: the halves and their weights, or `None`
/// when a value is not finite.
fn chop_conic_at(p: [F; 3], w: f32, t: f32) -> Option<([F; 3], f32, [F; 3], f32)> {
    let tmp = [
        (p[0].0, p[0].1, 1.0),
        (p[1].0 * w, p[1].1 * w, w),
        (p[2].0, p[2].1, 1.0),
    ];
    let interp3 = |k: usize| {
        let get = |i: usize| match k {
            0 => tmp[i].0,
            1 => tmp[i].1,
            _ => tmp[i].2,
        };
        let ab = interp(get(0), get(1), t);
        let bc = interp(get(1), get(2), t);
        [ab, interp(ab, bc, t), bc]
    };
    let (xs, ys, zs) = (interp3(0), interp3(1), interp3(2));
    let down = |i: usize| (xs[i] / zs[i], ys[i] / zs[i]);
    let a = [p[0], down(0), down(1)];
    let b = [down(1), down(2), p[2]];
    let root = zs[1].sqrt();
    let (wa, wb) = (zs[0] / root, zs[2] / root);
    let finite = a
        .iter()
        .chain(b.iter())
        .all(|q| q.0.is_finite() && q.1.is_finite())
        && wa.is_finite()
        && wb.is_finite();
    finite.then_some((a, wa, b, wb))
}

/// `SkConic::chopAt(t1, t2, dst)`.
fn chop_conic_between(p: [F; 3], w: f32, t1: f32, t2: f32) -> Option<([F; 3], f32)> {
    if t1 == 0.0 || t2 == 1.0 {
        if t1 == 0.0 && t2 == 1.0 {
            return Some((p, w));
        }
        let (a, wa, b, wb) = chop_conic_at(p, w, if t1 != 0.0 { t1 } else { t2 })?;
        return Some(if t1 != 0.0 { (b, wb) } else { (a, wa) });
    }
    let (n, dn) = conic_coeff(p, w);
    let num = |t: f32| {
        (
            (n[0].0 * t + n[1].0) * t + n[2].0,
            (n[0].1 * t + n[1].1) * t + n[2].1,
        )
    };
    let den = |t: f32| (dn[0] * t + dn[1]) * t + dn[2];
    let (a_xy, a_zz) = (num(t1), den(t1));
    let mid = (t1 + t2) / 2.0;
    let (d_xy, d_zz) = (num(mid), den(mid));
    let (c_xy, c_zz) = (num(t2), den(t2));
    let b_xy = (
        d_xy.0 * 2.0 - (a_xy.0 + c_xy.0) * 0.5,
        d_xy.1 * 2.0 - (a_xy.1 + c_xy.1) * 0.5,
    );
    let b_zz = d_zz * 2.0 - (a_zz + c_zz) * 0.5;
    let pts = [
        (a_xy.0 / a_zz, a_xy.1 / a_zz),
        (b_xy.0 / b_zz, b_xy.1 / b_zz),
        (c_xy.0 / c_zz, c_xy.1 / c_zz),
    ];
    Some((pts, b_zz / (a_zz * c_zz).sqrt()))
}

// ---------------------------------------------------------------------------
// The output path (SkPathBuilder as the dasher writes it)

#[derive(Default)]
struct Out {
    segs: Vec<Seg>,
}

impl Out {
    fn last_pt(&self) -> Option<F> {
        self.segs.iter().rev().find_map(|s| match *s {
            Seg::Move(p)
            | Seg::Line(p)
            | Seg::Quad(_, p)
            | Seg::Conic(_, p, _)
            | Seg::Cubic(_, _, p) => Some(f(p)),
            Seg::Close => None,
        })
    }

    fn move_to(&mut self, p: F) {
        if let Some(Seg::Move(last)) = self.segs.last_mut() {
            *last = d(p);
        } else {
            self.segs.push(Seg::Move(d(p)));
        }
    }

    fn line_to(&mut self, p: F) {
        self.segs.push(Seg::Line(d(p)));
    }

    fn quad_to(&mut self, c: F, p: F) {
        self.segs.push(Seg::Quad(d(c), d(p)));
    }

    fn conic_to(&mut self, c: F, p: F, w: f32) {
        if w <= 0.0 {
            self.line_to(p);
        } else if w == 1.0 {
            self.quad_to(c, p);
        } else if w.is_finite() {
            self.segs.push(Seg::Conic(d(c), d(p), f64::from(w)));
        } else {
            self.line_to(c);
            self.line_to(p);
        }
    }

    fn cubic_to(&mut self, a: F, b: F, p: F) {
        self.segs.push(Seg::Cubic(d(a), d(b), d(p)));
    }
}

// ---------------------------------------------------------------------------
// SkContourMeasure

const MAX_T_VALUE: u32 = 0x3FFF_FFFF;
const MAX_RECURSION_DEPTH: u32 = 8;

fn t_value_to_scalar(t: u32) -> f32 {
    t as f32 * (1.0 / MAX_T_VALUE as f32)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SegType {
    Line,
    Quad,
    Conic,
    Cubic,
}

#[derive(Clone, Copy, Debug)]
struct MSeg {
    distance: f32,
    pt_index: usize,
    t_value: u32,
    kind: SegType,
}

struct Measure {
    segs: Vec<MSeg>,
    /// The points, a conic's weight stored as a point `(w, 0)` before its
    /// control and end points, as Skia stores it.
    pts: Vec<F>,
    length: f32,
    closed: bool,
}

fn tspan_big_enough(tspan: u32) -> bool {
    (tspan >> 10) != 0
}

fn quad_too_curvy(p: [F; 3], tol: f32) -> bool {
    let dx = (p[1].0 / 2.0) - (((p[0].0 + p[2].0) / 2.0) / 2.0);
    let dy = (p[1].1 / 2.0) - (((p[0].1 + p[2].1) / 2.0) / 2.0);
    dx.abs().max(dy.abs()) > tol
}

fn conic_too_curvy(first: F, mid: F, last: F, tol: f32) -> bool {
    let mid_ends = ((first.0 + last.0) * 0.5, (first.1 + last.1) * 0.5);
    let (dx, dy) = (mid.0 - mid_ends.0, mid.1 - mid_ends.1);
    dx.abs().max(dy.abs()) > tol
}

fn cheap_dist_exceeds(p: F, x: f32, y: f32, tol: f32) -> bool {
    (x - p.0).abs().max((y - p.1).abs()) > tol
}

fn cubic_too_curvy(p: [F; 4], tol: f32) -> bool {
    cheap_dist_exceeds(
        p[1],
        interp(p[0].0, p[3].0, 1.0 / 3.0),
        interp(p[0].1, p[3].1, 1.0 / 3.0),
        tol,
    ) || cheap_dist_exceeds(
        p[2],
        interp(p[0].0, p[3].0, 2.0 / 3.0),
        interp(p[0].1, p[3].1, 2.0 / 3.0),
        tol,
    )
}

struct Builder<'a> {
    tol: f32,
    segs: &'a mut Vec<MSeg>,
}

impl Builder<'_> {
    fn push(&mut self, distance: f32, prev: f32, pt_index: usize, kind: SegType, t_value: u32) {
        if distance > prev {
            self.segs.push(MSeg {
                distance,
                pt_index,
                t_value,
                kind,
            });
        }
    }

    fn line(&mut self, p0: F, p1: F, dist: f32, pt_index: usize) -> f32 {
        let dd = distance(p0, p1);
        let next = dist + dd;
        self.push(next, dist, pt_index, SegType::Line, MAX_T_VALUE);
        next
    }

    fn quad(
        &mut self,
        p: [F; 3],
        dist: f32,
        mint: u32,
        maxt: u32,
        pt_index: usize,
        depth: u32,
    ) -> f32 {
        if depth < MAX_RECURSION_DEPTH
            && tspan_big_enough(maxt - mint)
            && quad_too_curvy(p, self.tol)
        {
            let tmp = chop_quad(p, 0.5);
            let half = (mint + maxt) >> 1;
            let dist = self.quad(
                [tmp[0], tmp[1], tmp[2]],
                dist,
                mint,
                half,
                pt_index,
                depth + 1,
            );
            self.quad(
                [tmp[2], tmp[3], tmp[4]],
                dist,
                half,
                maxt,
                pt_index,
                depth + 1,
            )
        } else {
            let next = dist + distance(p[0], p[2]);
            self.push(next, dist, pt_index, SegType::Quad, maxt);
            next
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn conic(
        &mut self,
        p: [F; 3],
        w: f32,
        dist: f32,
        mint: u32,
        min_pt: F,
        maxt: u32,
        max_pt: F,
        pt_index: usize,
        depth: u32,
    ) -> f32 {
        let half = (mint + maxt) >> 1;
        let half_pt = eval_conic(p, w, t_value_to_scalar(half));
        if !(half_pt.0.is_finite() && half_pt.1.is_finite()) {
            return dist;
        }
        if depth < MAX_RECURSION_DEPTH
            && tspan_big_enough(maxt - mint)
            && conic_too_curvy(min_pt, half_pt, max_pt, self.tol)
        {
            let dist = self.conic(p, w, dist, mint, min_pt, half, half_pt, pt_index, depth + 1);
            self.conic(p, w, dist, half, half_pt, maxt, max_pt, pt_index, depth + 1)
        } else {
            let next = dist + distance(min_pt, max_pt);
            self.push(next, dist, pt_index, SegType::Conic, maxt);
            next
        }
    }

    fn cubic(
        &mut self,
        p: [F; 4],
        dist: f32,
        mint: u32,
        maxt: u32,
        pt_index: usize,
        depth: u32,
    ) -> f32 {
        if depth < MAX_RECURSION_DEPTH
            && tspan_big_enough(maxt - mint)
            && cubic_too_curvy(p, self.tol)
        {
            let tmp = chop_cubic(p, 0.5);
            let half = (mint + maxt) >> 1;
            let dist = self.cubic(
                [tmp[0], tmp[1], tmp[2], tmp[3]],
                dist,
                mint,
                half,
                pt_index,
                depth + 1,
            );
            self.cubic(
                [tmp[3], tmp[4], tmp[5], tmp[6]],
                dist,
                half,
                maxt,
                pt_index,
                depth + 1,
            )
        } else {
            let next = dist + distance(p[0], p[3]);
            self.push(next, dist, pt_index, SegType::Cubic, maxt);
            next
        }
    }
}

/// `SkContourMeasureIter` over the path (not force-closed): one measure
/// per contour with length.
fn measure(segs: &[Seg], res_scale: f32) -> Vec<Measure> {
    let tol = 0.5 * (1.0 / res_scale);
    let mut out = Vec::new();
    let mut i = 0;
    while i < segs.len() {
        let mut msegs = Vec::new();
        let mut pts: Vec<F> = Vec::new();
        let mut pt_index: isize = -1;
        let mut dist = 0.0f32;
        let mut seen_close = false;
        let mut seen_move = false;
        let mut last = (0.0f32, 0.0f32);
        while i < segs.len() {
            let s = segs[i];
            if seen_move && matches!(s, Seg::Move(_)) {
                break;
            }
            let idx = pt_index.max(0) as usize;
            let mut b = Builder {
                tol,
                segs: &mut msegs,
            };
            match s {
                Seg::Move(p) => {
                    pt_index += 1;
                    pts.push(f(p));
                    seen_move = true;
                    last = f(p);
                }
                Seg::Line(p) => {
                    let prev = dist;
                    dist = b.line(last, f(p), dist, idx);
                    if dist > prev {
                        pts.push(f(p));
                        pt_index += 1;
                    }
                    last = f(p);
                }
                Seg::Quad(c, p) => {
                    let prev = dist;
                    dist = b.quad([last, f(c), f(p)], dist, 0, MAX_T_VALUE, idx, 0);
                    if dist > prev {
                        pts.extend([f(c), f(p)]);
                        pt_index += 2;
                    }
                    last = f(p);
                }
                Seg::Conic(c, p, w) => {
                    let prev = dist;
                    let w = w as f32;
                    dist = b.conic(
                        [last, f(c), f(p)],
                        w,
                        dist,
                        0,
                        last,
                        MAX_T_VALUE,
                        f(p),
                        idx,
                        0,
                    );
                    if dist > prev {
                        pts.extend([(w, 0.0), f(c), f(p)]);
                        pt_index += 3;
                    }
                    last = f(p);
                }
                Seg::Cubic(a, c, p) => {
                    let prev = dist;
                    dist = b.cubic([last, f(a), f(c), f(p)], dist, 0, MAX_T_VALUE, idx, 0);
                    if dist > prev {
                        pts.extend([f(a), f(c), f(p)]);
                        pt_index += 3;
                    }
                    last = f(p);
                }
                Seg::Close => seen_close = true,
            }
            i += 1;
        }
        if !dist.is_finite() || msegs.is_empty() {
            continue;
        }
        if seen_close {
            let prev = dist;
            let first = pts[0];
            let idx = pt_index.max(0) as usize;
            let mut b = Builder {
                tol,
                segs: &mut msegs,
            };
            dist = b.line(pts[idx], first, dist, idx);
            if dist > prev {
                pts.push(first);
            }
        }
        out.push(Measure {
            segs: msegs,
            pts,
            length: dist,
            closed: seen_close,
        });
    }
    out
}

impl Measure {
    fn scalar_t(&self, s: &MSeg) -> f32 {
        t_value_to_scalar(s.t_value)
    }

    /// `distanceToSegment`: the segment and its `t` at `distance`.
    fn distance_to_segment(&self, distance: f32) -> (usize, f32) {
        // SkTKSearch on fDistance.
        let count = self.segs.len();
        let (mut lo, mut hi) = (0usize, count - 1);
        while lo < hi {
            let mid = (hi + lo) >> 1;
            if self.segs[mid].distance < distance {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let mut index = hi as isize;
        if self.segs[hi].distance < distance {
            index = !(hi as isize + 1);
        } else if distance < self.segs[hi].distance {
            index = !(hi as isize);
        }
        let index = (index ^ (index >> 63)) as usize;
        let seg = self.segs[index];
        let (mut start_t, mut start_d) = (0.0f32, 0.0f32);
        if index > 0 {
            start_d = self.segs[index - 1].distance;
            if self.segs[index - 1].pt_index == seg.pt_index {
                start_t = self.scalar_t(&self.segs[index - 1]);
            }
        }
        let t = start_t
            + (self.scalar_t(&seg) - start_t) * (distance - start_d) / (seg.distance - start_d);
        (index, t)
    }

    fn pos(&self, seg: &MSeg, t: f32) -> F {
        let p = &self.pts[seg.pt_index..];
        match seg.kind {
            SegType::Line => lerp(p[0], p[1], t),
            SegType::Quad => eval_quad([p[0], p[1], p[2]], t),
            SegType::Conic => eval_conic([p[0], p[2], p[3]], p[1].0, t),
            SegType::Cubic => eval_cubic([p[0], p[1], p[2], p[3]], t),
        }
    }

    /// `SkContourMeasure_segTo`.
    fn seg_to(&self, seg: &MSeg, start_t: f32, stop_t: f32, dst: &mut Out) {
        if start_t == stop_t {
            if let Some(last) = dst.last_pt() {
                dst.line_to(last);
            }
            return;
        }
        let p = &self.pts[seg.pt_index..];
        match seg.kind {
            SegType::Line => {
                if stop_t == 1.0 {
                    dst.line_to(p[1]);
                } else {
                    dst.line_to(lerp(p[0], p[1], stop_t));
                }
            }
            SegType::Quad => {
                let q = [p[0], p[1], p[2]];
                if start_t == 0.0 {
                    if stop_t == 1.0 {
                        dst.quad_to(q[1], q[2]);
                    } else {
                        let t0 = chop_quad(q, stop_t);
                        dst.quad_to(t0[1], t0[2]);
                    }
                } else {
                    let t0 = chop_quad(q, start_t);
                    if stop_t == 1.0 {
                        dst.quad_to(t0[3], t0[4]);
                    } else {
                        let t1 =
                            chop_quad([t0[2], t0[3], t0[4]], (stop_t - start_t) / (1.0 - start_t));
                        dst.quad_to(t1[1], t1[2]);
                    }
                }
            }
            SegType::Conic => {
                let (c, w) = ([p[0], p[2], p[3]], p[1].0);
                if start_t == 0.0 {
                    if stop_t == 1.0 {
                        dst.conic_to(c[1], c[2], w);
                    } else if let Some((a, wa, _, _)) = chop_conic_at(c, w, stop_t) {
                        dst.conic_to(a[1], a[2], wa);
                    }
                } else if stop_t == 1.0 {
                    if let Some((_, _, b, wb)) = chop_conic_at(c, w, start_t) {
                        dst.conic_to(b[1], b[2], wb);
                    }
                } else if let Some((m, wm)) = chop_conic_between(c, w, start_t, stop_t) {
                    dst.conic_to(m[1], m[2], wm);
                }
            }
            SegType::Cubic => {
                let q = [p[0], p[1], p[2], p[3]];
                if start_t == 0.0 {
                    if stop_t == 1.0 {
                        dst.cubic_to(q[1], q[2], q[3]);
                    } else {
                        let t0 = chop_cubic(q, stop_t);
                        dst.cubic_to(t0[1], t0[2], t0[3]);
                    }
                } else {
                    let t0 = chop_cubic(q, start_t);
                    if stop_t == 1.0 {
                        dst.cubic_to(t0[4], t0[5], t0[6]);
                    } else {
                        let t1 = chop_cubic(
                            [t0[3], t0[4], t0[5], t0[6]],
                            (stop_t - start_t) / (1.0 - start_t),
                        );
                        dst.cubic_to(t1[1], t1[2], t1[3]);
                    }
                }
            }
        }
    }

    /// `getSegment(startD, stopD, dst, startWithMoveTo)`.
    fn get_segment(
        &self,
        mut start_d: f32,
        mut stop_d: f32,
        dst: &mut Out,
        start_with_move_to: bool,
    ) -> bool {
        if start_d < 0.0 {
            start_d = 0.0;
        }
        if stop_d > self.length {
            stop_d = self.length;
        }
        // Written to catch NaN distances as well.
        if start_d.is_nan() || stop_d.is_nan() || start_d > stop_d || self.segs.is_empty() {
            return false;
        }
        let (mut si, mut start_t) = self.distance_to_segment(start_d);
        if !start_t.is_finite() {
            return false;
        }
        let (stop_i, stop_t) = self.distance_to_segment(stop_d);
        if !stop_t.is_finite() {
            return false;
        }
        if start_with_move_to {
            let p = self.pos(&self.segs[si], start_t);
            dst.move_to(p);
        }
        if self.segs[si].pt_index == self.segs[stop_i].pt_index {
            self.seg_to(&self.segs[si], start_t, stop_t, dst);
        } else {
            loop {
                self.seg_to(&self.segs[si], start_t, 1.0, dst);
                // Segment::Next: the next segment with another point index.
                let pt = self.segs[si].pt_index;
                while si < self.segs.len() && self.segs[si].pt_index == pt {
                    si += 1;
                }
                start_t = 0.0;
                if self.segs[si].pt_index >= self.segs[stop_i].pt_index {
                    break;
                }
            }
            self.seg_to(&self.segs[si], 0.0, stop_t, dst);
        }
        true
    }
}

// ---------------------------------------------------------------------------
// SkDashPath

/// `SkDashPath::ValidDashPath` and `CalcDashParameters`: the adjusted
/// phase, the first interval's remaining length and index, and the total.
fn dash_parameters(phase: f32, intervals: &[f32]) -> Option<(f32, usize, f32, f32)> {
    if intervals.len() < 2 || !intervals.len().is_multiple_of(2) {
        return None;
    }
    let mut len = 0.0f32;
    for &i in intervals {
        if i < 0.0 {
            return None;
        }
        len += i;
    }
    if !(len > 0.0 && phase.is_finite() && len.is_finite()) {
        return None;
    }
    let mut phase = phase;
    if phase < 0.0 {
        phase = -phase;
        if phase > len {
            phase %= len;
        }
        phase = len - phase;
        if phase == len {
            phase = 0.0;
        }
    } else if phase >= len {
        phase %= len;
    }
    // find_first_interval.
    let mut p = phase;
    let mut first = (intervals[0], 0usize);
    let mut found = false;
    for (i, &gap) in intervals.iter().enumerate() {
        if p > gap || (p == gap && gap != 0.0) {
            p -= gap;
        } else {
            first = (gap - p, i);
            found = true;
            break;
        }
    }
    if !found {
        first = (intervals[0], 0);
    }
    Some((first.0, first.1, len, phase))
}

/// How the dashed path is drawn: the stroke's width, cap and join, and the
/// bounds the dash may cull to (the device clip in local coordinates,
/// outset by a pixel).
pub(crate) struct DashStroke {
    pub width: f32,
    pub butt_cap: bool,
    pub miter_join: bool,
    pub miter_limit: f32,
    pub res_scale: f32,
    pub cull: Option<[f32; 4]>,
}

fn adjust_zero_length_line(pts: &mut [F; 2]) {
    pts[1].0 += pts[1].0.max(1.001) * (1.0 / 4096.0);
}

/// `clip_line`: trim an axis-aligned line to the bounds, in phase.
fn clip_line(pts: &mut [F; 2], bounds: [f32; 4], interval_length: f32, prior_phase: f32) -> bool {
    let dxy = (pts[1].0 - pts[0].0, pts[1].1 - pts[0].1);
    if dxy.0 != 0.0 && dxy.1 != 0.0 {
        return false;
    }
    let vertical = dxy.1 != 0.0;
    let get = |p: F| if vertical { p.1 } else { p.0 };
    let (mut min, mut max) = (get(pts[0]), get(pts[1]));
    let swapped = max < min;
    if swapped {
        std::mem::swap(&mut min, &mut max);
    }
    let (lt, rb) = if vertical {
        (bounds[1], bounds[3])
    } else {
        (bounds[0], bounds[2])
    };
    if max < lt || min > rb {
        return false;
    }
    if min < lt {
        min = lt - (lt - min) % interval_length;
        if !swapped {
            min -= prior_phase;
        }
    }
    if max > rb {
        max = rb + (max - rb) % interval_length;
        if swapped {
            max += prior_phase;
        }
    }
    if swapped {
        std::mem::swap(&mut min, &mut max);
    }
    if vertical {
        pts[0].1 = min;
        pts[1].1 = max;
    } else {
        pts[0].0 = min;
        pts[1].0 = max;
    }
    if min == max {
        adjust_zero_length_line(pts);
    }
    true
}

fn is_line(segs: &[Seg]) -> Option<[F; 2]> {
    match segs {
        [Seg::Move(a), Seg::Line(b)] => Some([f(*a), f(*b)]),
        _ => None,
    }
}

fn points_of(segs: &[Seg]) -> Vec<F> {
    let mut out = Vec::new();
    for s in segs {
        match *s {
            Seg::Move(p) | Seg::Line(p) => out.push(f(p)),
            Seg::Quad(a, p) | Seg::Conic(a, p, _) => out.extend([f(a), f(p)]),
            Seg::Cubic(a, b, p) => out.extend([f(a), f(b), f(p)]),
            Seg::Close => {}
        }
    }
    out
}

/// `SkDashPath::InternalFilter`: the dashed path, and whether it is to be
/// filled as it is (`SpecialLineRec` turns a butt-capped line's dashes
/// into rectangles) rather than stroked.
pub(crate) fn dash(
    src: &[Seg],
    intervals: &[f32],
    phase: f32,
    stroke: &DashStroke,
) -> Option<(Vec<Seg>, bool)> {
    let (initial_len, initial_index, interval_length, phase) = dash_parameters(phase, intervals)?;
    let count = intervals.len();
    if points_of(src).is_empty() {
        return Some((Vec::new(), false));
    }
    let mut culled: Option<Vec<Seg>> = None;
    // cull_path.
    match stroke.cull {
        None => {
            if let Some(mut pts) = is_line(src) {
                if pts[0] == pts[1] {
                    adjust_zero_length_line(&mut pts);
                    culled = Some(vec![Seg::Move(d(pts[0])), Seg::Line(d(pts[1]))]);
                }
            }
        }
        Some(cull) => {
            let mut radius = stroke.width / 2.0;
            if radius == 0.0 {
                radius = 1.0;
            }
            if stroke.miter_join {
                radius *= stroke.miter_limit;
            }
            let bounds = [
                cull[0] - radius,
                cull[1] - radius,
                cull[2] + radius,
                cull[3] + radius,
            ];
            if let Some(mut pts) = is_line(src) {
                if clip_line(&mut pts, bounds, interval_length, 0.0) {
                    culled = Some(vec![Seg::Move(d(pts[0])), Seg::Line(d(pts[1]))]);
                }
            } else if crate::aaa::rect_of(src).is_some() {
                let mut out = Out::default();
                let mut accum = 0.0f64;
                let mut prev: Option<F> = None;
                for s in src {
                    match *s {
                        Seg::Move(p) => prev = Some(f(p)),
                        Seg::Line(p) => {
                            let a = prev.expect("a line follows a point");
                            let b = f(p);
                            let v = (b.0 - a.0, b.1 - a.1);
                            let mut pts = [a, b];
                            let prior = (accum % f64::from(interval_length)) as f32;
                            if clip_line(&mut pts, bounds, interval_length, prior) {
                                if out.last_pt() != Some(pts[0]) {
                                    out.move_to(pts[0]);
                                }
                                out.line_to(pts[1]);
                            }
                            accum += f64::from((v.0 + v.1).abs());
                            prev = Some(b);
                        }
                        Seg::Close => {
                            // SkPath::Iter's closing line, then stop at the
                            // close.
                            if let (Some(a), Some(Seg::Move(m))) = (prev, src.first()) {
                                let b = f(*m);
                                if a != b {
                                    let mut pts = [a, b];
                                    let prior = (accum % f64::from(interval_length)) as f32;
                                    if clip_line(&mut pts, bounds, interval_length, prior) {
                                        if out.last_pt() != Some(pts[0]) {
                                            out.move_to(pts[0]);
                                        }
                                        out.line_to(pts[1]);
                                    }
                                }
                            }
                            break;
                        }
                        _ => break,
                    }
                }
                if !out.segs.is_empty() {
                    let mut segs = out.segs;
                    // A closed rectangle that starts and ends in a dash gets
                    // a tiny right angle at its start, for the join.
                    let closed = src.last() == Some(&Seg::Close);
                    if closed && initial_index % 2 == 0 {
                        let path_length: f32 = measure(src, stroke.res_scale)
                            .first()
                            .map_or(0.0, |m| m.length);
                        let mut end_phase = (path_length + phase) % interval_length;
                        let mut index = 0;
                        while end_phase > intervals[index] {
                            end_phase -= intervals[index];
                            index += 1;
                            if index == count {
                                end_phase = 0.0;
                                break;
                            }
                        }
                        if (index % 2 == 0) == (end_phase > 0.0) {
                            let src_pts = points_of(src);
                            let mid = src_pts[0];
                            let mut last = src_pts.len() - 1;
                            while mid == src_pts[last] {
                                last -= 1;
                            }
                            let mut next = 1;
                            while mid == src_pts[next] {
                                next += 1;
                            }
                            let tiny = 1.0 / 4096.0;
                            let v = (
                                (mid.0 - src_pts[last].0) * tiny,
                                (mid.1 - src_pts[last].1) * tiny,
                            );
                            let mut extra = Out::default();
                            extra.move_to((mid.0 - v.0, mid.1 - v.1));
                            extra.line_to(mid);
                            let v = (
                                (mid.0 - src_pts[next].0) * tiny,
                                (mid.1 - src_pts[next].1) * tiny,
                            );
                            extra.line_to((mid.0 - v.0, mid.1 - v.1));
                            segs.extend(extra.segs);
                        }
                    }
                    culled = Some(segs);
                }
            }
        }
    }
    let src: &[Seg] = culled.as_deref().unwrap_or(src);

    // SpecialLineRec.
    let mut special: Option<([F; 2], F, F, f32)> = None;
    if stroke.butt_cap && stroke.width > 0.0 {
        if let Some(pts) = is_line(src) {
            let path_length = distance(pts[0], pts[1]);
            let tangent = (pts[1].0 - pts[0].0, pts[1].1 - pts[0].1);
            if tangent != (0.0, 0.0) {
                let inv = 1.0 / path_length;
                let tangent = (tangent.0 * inv, tangent.1 * inv);
                if tangent.0.is_finite() && tangent.1.is_finite() {
                    let normal = (tangent.1, -tangent.0);
                    let half = stroke.width / 2.0;
                    special = Some((
                        pts,
                        tangent,
                        (normal.0 * half, normal.1 * half),
                        path_length,
                    ));
                }
            }
        }
    }

    let mut dst = Out::default();
    let mut dash_count = 0.0f32;
    for meas in measure(src, stroke.res_scale) {
        let mut skip_first = meas.closed;
        let mut added = false;
        let length = meas.length;
        let mut index = initial_index;
        dash_count += length * (count >> 1) as f32 / interval_length;
        if dash_count > 1_000_000.0 {
            return None;
        }
        let mut dist = 0.0f64;
        let mut dlen = f64::from(initial_len);
        while dist < f64::from(length) {
            added = false;
            if index % 2 == 0 && !skip_first {
                added = true;
                match special {
                    Some((pts, tangent, normal, path_length)) => {
                        let d0 = dist as f32;
                        let d1 = ((dist + dlen) as f32).min(path_length);
                        let x0 = pts[0].0 + tangent.0 * d0;
                        let x1 = pts[0].0 + tangent.0 * d1;
                        let y0 = pts[0].1 + tangent.1 * d0;
                        let y1 = pts[0].1 + tangent.1 * d1;
                        dst.move_to((x0 + normal.0, y0 + normal.1));
                        dst.line_to((x1 + normal.0, y1 + normal.1));
                        dst.line_to((x1 - normal.0, y1 - normal.1));
                        dst.line_to((x0 - normal.0, y0 - normal.1));
                    }
                    None => {
                        meas.get_segment(dist as f32, (dist + dlen) as f32, &mut dst, true);
                    }
                }
            }
            dist += dlen;
            skip_first = false;
            index += 1;
            if index == count {
                index = 0;
            }
            dlen = f64::from(intervals[index]);
        }
        if meas.closed && initial_index % 2 == 0 && initial_len >= 0.0 {
            meas.get_segment(0.0, initial_len, &mut dst, !added);
        }
    }
    Some((dst.segs, special.is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dash_parameters_wrap_the_phase() {
        assert_eq!(
            dash_parameters(0.0, &[10.0, 4.0]),
            Some((10.0, 0, 14.0, 0.0))
        );
        assert_eq!(
            dash_parameters(5.0, &[10.0, 4.0]),
            Some((5.0, 0, 14.0, 5.0))
        );
        assert_eq!(
            dash_parameters(-3.0, &[10.0, 4.0]),
            Some((3.0, 1, 14.0, 11.0))
        );
        assert_eq!(
            dash_parameters(31.0, &[10.0, 4.0]),
            Some((7.0, 0, 14.0, 3.0))
        );
        assert_eq!(dash_parameters(0.0, &[5.0, -1.0]), None);
        assert_eq!(dash_parameters(0.0, &[0.0, 0.0]), None);
    }

    #[test]
    fn a_line_is_measured_and_cut() {
        let segs = [Seg::Move((0.0, 0.0)), Seg::Line((10.0, 0.0))];
        let m = measure(&segs, 1.0);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].length, 10.0);
        let mut out = Out::default();
        assert!(m[0].get_segment(2.0, 5.0, &mut out, true));
        assert_eq!(out.segs, vec![Seg::Move((2.0, 0.0)), Seg::Line((5.0, 0.0))]);
    }

    #[test]
    fn butt_capped_lines_dash_into_rectangles() {
        let segs = [Seg::Move((0.0, 5.0)), Seg::Line((20.0, 5.0))];
        let stroke = DashStroke {
            width: 2.0,
            butt_cap: true,
            miter_join: false,
            miter_limit: 10.0,
            res_scale: 1.0,
            cull: None,
        };
        let (out, fill) = dash(&segs, &[5.0, 5.0], 0.0, &stroke).unwrap();
        assert!(fill);
        assert_eq!(
            &out[..4],
            &[
                Seg::Move((0.0, 4.0)),
                Seg::Line((5.0, 4.0)),
                Seg::Line((5.0, 6.0)),
                Seg::Line((0.0, 6.0))
            ]
        );
        assert_eq!(out.iter().filter(|s| matches!(s, Seg::Move(_))).count(), 2);
    }

    #[test]
    fn conics_are_dashed_as_conics() {
        // A quarter circle as one conic: the dash keeps conic pieces.
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let segs = [
            Seg::Move((10.0, 0.0)),
            Seg::Conic((10.0, 10.0), (0.0, 10.0), w),
        ];
        let stroke = DashStroke {
            width: 2.0,
            butt_cap: false,
            miter_join: false,
            miter_limit: 10.0,
            res_scale: 1.0,
            cull: None,
        };
        let (out, fill) = dash(&segs, &[4.0, 4.0], 0.0, &stroke).unwrap();
        assert!(!fill);
        assert!(out.iter().any(|s| matches!(s, Seg::Conic(..))));
        let m = measure(&segs, 1.0);
        // The quarter of a radius-10 circle (15.708), measured by chords
        // within half a unit of the curve: a little short.
        assert!(
            m[0].length > 15.5 && m[0].length < 15.708,
            "{}",
            m[0].length
        );
    }
}
