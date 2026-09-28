//! Skia's stroker, ported from Skia at `chrome/m153`
//! (f8b66b7597c4cc859d3ed190e9c6872241e6721c): `SkStroke.cpp`
//! (`SkPathStroker`, `SkStroke::strokePath`, `strokeRect`) and
//! `SkStrokerPriv.cpp` (the cappers and joiners), with the `SkGeometry.cpp`
//! evaluators they use.
//!
//! Chrome strokes every canvas path with it. It keeps round caps, round
//! joins and arcs as conics (`conicTo`), which the edge builder turns into
//! quadratics within 1/4 device pixel; tiny-skia's port of the same
//! stroker emits at least two quadratics per conic instead, which moves
//! cap and join edges and changes the outline's convexity. Arithmetic is
//! `f32`, as Skia's.

use std::f32::consts::FRAC_1_SQRT_2;

use crate::edges::Seg;

type V = (f32, f32);

const NEARLY_ZERO: f32 = 1.0 / 4096.0;

fn add(a: V, b: V) -> V {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: V, b: V) -> V {
    (a.0 - b.0, a.1 - b.1)
}

fn mul(a: V, s: f32) -> V {
    (a.0 * s, a.1 * s)
}

fn neg(a: V) -> V {
    (-a.0, -a.1)
}

fn dot(a: V, b: V) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

fn cross(a: V, b: V) -> f32 {
    a.0 * b.1 - a.1 * b.0
}

fn length_sqd(a: V) -> f32 {
    dot(a, a)
}

fn distance_to_sqd(a: V, b: V) -> f32 {
    length_sqd(sub(a, b))
}

fn is_finite(a: V) -> bool {
    a.0.is_finite() && a.1.is_finite()
}

fn is_zero(a: V) -> bool {
    a.0 == 0.0 && a.1 == 0.0
}

/// `SkPointPriv::CanNormalize`.
fn can_normalize(a: V) -> bool {
    is_finite(a) && (a.0 != 0.0 || a.1 != 0.0)
}

/// `set_point_length` (`SkPoint::setLength`): the length computed in
/// doubles; `None` for a zero or non-finite result.
fn set_length(v: V, length: f32) -> Option<V> {
    let (xx, yy) = (f64::from(v.0), f64::from(v.1));
    let dmag = (xx * xx + yy * yy).sqrt();
    let dscale = f64::from(length) / dmag;
    let x = (f64::from(v.0) * dscale) as f32;
    let y = (f64::from(v.1) * dscale) as f32;
    if !x.is_finite() || !y.is_finite() || (x == 0.0 && y == 0.0) {
        return None;
    }
    Some((x, y))
}

fn rotate_ccw(v: V) -> V {
    (v.1, -v.0)
}

fn rotate_cw(v: V) -> V {
    (-v.1, v.0)
}

fn nearly_zero(x: f32) -> bool {
    x.abs() <= NEARLY_ZERO
}

fn equals_within_tolerance(a: V, b: V, tol: f32) -> bool {
    (a.0 - b.0).abs() <= tol && (a.1 - b.1).abs() <= tol
}

// ---------------------------------------------------------------------------
// SkGeometry.cpp evaluators

fn valid_unit_divide(numer: f32, denom: f32) -> Option<f32> {
    let (mut n, mut d) = (numer, denom);
    if n < 0.0 {
        n = -n;
        d = -d;
    }
    if d == 0.0 || n == 0.0 || n >= d {
        return None;
    }
    let r = n / d;
    if r.is_nan() || r == 0.0 {
        return None;
    }
    Some(r)
}

/// `SkFindUnitQuadRoots`.
fn find_unit_quad_roots(a: f32, b: f32, c: f32) -> Vec<f32> {
    if a == 0.0 {
        return valid_unit_divide(-c, b).into_iter().collect();
    }
    let dr = f64::from(b) * f64::from(b) - 4.0 * f64::from(a) * f64::from(c);
    if dr < 0.0 {
        return Vec::new();
    }
    let r = dr.sqrt() as f32;
    if !r.is_finite() {
        return Vec::new();
    }
    let q = if b < 0.0 {
        -(b - r) / 2.0
    } else {
        -(b + r) / 2.0
    };
    let mut roots: Vec<f32> = valid_unit_divide(q, a)
        .into_iter()
        .chain(valid_unit_divide(c, q))
        .collect();
    if roots.len() == 2 {
        if roots[0] > roots[1] {
            roots.swap(0, 1);
        } else if roots[0] == roots[1] {
            roots.pop();
        }
    }
    roots
}

/// `SkEvalQuadAt` (`SkQuadCoeff`).
fn eval_quad_at(q: [V; 3], t: f32) -> V {
    let c = q[0];
    let b = mul(sub(q[1], c), 2.0);
    let a = add(sub(q[2], mul(q[1], 2.0)), c);
    add(mul(add(mul(a, t), b), t), c)
}

/// `SkEvalQuadTangentAt`.
fn eval_quad_tangent_at(q: [V; 3], t: f32) -> V {
    if (t == 0.0 && q[0] == q[1]) || (t == 1.0 && q[1] == q[2]) {
        return sub(q[2], q[0]);
    }
    let b = sub(q[1], q[0]);
    let a = sub(sub(q[2], q[1]), b);
    let tt = add(mul(a, t), b);
    add(tt, tt)
}

/// `SkFindQuadMaxCurvature`.
fn find_quad_max_curvature(q: [V; 3]) -> f32 {
    let (ax, ay) = (q[1].0 - q[0].0, q[1].1 - q[0].1);
    let bx = q[0].0 - q[1].0 - q[1].0 + q[2].0;
    let by = q[0].1 - q[1].1 - q[1].1 + q[2].1;
    let mut numer = -(ax * bx + ay * by);
    let mut denom = bx * bx + by * by;
    if denom < 0.0 {
        numer = -numer;
        denom = -denom;
    }
    if numer <= 0.0 {
        return 0.0;
    }
    if numer >= denom {
        return 1.0;
    }
    numer / denom
}

/// `SkEvalCubicAt` (`SkCubicCoeff`) and its tangent.
fn eval_cubic_at(c: [V; 4], t: f32) -> V {
    let a = sub(add(c[3], mul(sub(c[1], c[2]), 3.0)), c[0]);
    let b = mul(add(sub(c[2], mul(c[1], 2.0)), c[0]), 3.0);
    let cc = mul(sub(c[1], c[0]), 3.0);
    add(mul(add(mul(add(mul(a, t), b), t), cc), t), c[0])
}

fn eval_cubic_tangent(c: [V; 4], t: f32) -> V {
    if (t == 0.0 && c[0] == c[1]) || (t == 1.0 && c[2] == c[3]) {
        let mut tangent = if t == 0.0 {
            sub(c[2], c[0])
        } else {
            sub(c[3], c[1])
        };
        if tangent.0 == 0.0 && tangent.1 == 0.0 {
            tangent = sub(c[3], c[0]);
        }
        return tangent;
    }
    // eval_cubic_derivative.
    let a = sub(add(c[3], mul(sub(c[1], c[2]), 3.0)), c[0]);
    let b = mul(add(sub(c[2], mul(c[1], 2.0)), c[0]), 2.0);
    let cc = sub(c[1], c[0]);
    add(mul(add(mul(a, t), b), t), cc)
}

/// `SkChopCubicAt(src, dst[7], t)`.
fn chop_cubic_at(c: [V; 4], t: f32) -> [V; 7] {
    if t == 1.0 {
        return [c[0], c[1], c[2], c[3], c[3], c[3], c[3]];
    }
    let mix = |a: V, b: V| add(mul(sub(b, a), t), a);
    let ab = mix(c[0], c[1]);
    let bc = mix(c[1], c[2]);
    let cd = mix(c[2], c[3]);
    let abc = mix(ab, bc);
    let bcd = mix(bc, cd);
    let abcd = mix(abc, bcd);
    [c[0], ab, abc, abcd, bcd, cd, c[3]]
}

/// `SkFindCubicInflections`.
fn find_cubic_inflections(c: [V; 4]) -> Vec<f32> {
    let (ax, ay) = (c[1].0 - c[0].0, c[1].1 - c[0].1);
    let bx = c[2].0 - 2.0 * c[1].0 + c[0].0;
    let by = c[2].1 - 2.0 * c[1].1 + c[0].1;
    let cx = c[3].0 + 3.0 * (c[1].0 - c[2].0) - c[0].0;
    let cy = c[3].1 + 3.0 * (c[1].1 - c[2].1) - c[0].1;
    find_unit_quad_roots(bx * cy - by * cx, ax * cy - ay * cx, ax * by - ay * bx)
}

fn formulate_f1_dot_f2(src: [f32; 4]) -> [f32; 4] {
    let a = src[1] - src[0];
    let b = src[2] - 2.0 * src[1] + src[0];
    let c = src[3] + 3.0 * (src[1] - src[2]) - src[0];
    [c * c, 3.0 * b * c, 2.0 * b * b + c * a, a * b]
}

fn solve_cubic_poly(coeff: [f32; 4]) -> Vec<f32> {
    if nearly_zero(coeff[0]) {
        return find_unit_quad_roots(coeff[1], coeff[2], coeff[3]);
    }
    let inva = 1.0 / coeff[0];
    let (a, b, c) = (coeff[1] * inva, coeff[2] * inva, coeff[3] * inva);
    let q = (a * a - b * 3.0) / 9.0;
    let r = (2.0 * a * a * a - 9.0 * a * b + 27.0 * c) / 54.0;
    let q3 = q * q * q;
    let r2_minus_q3 = r * r - q3;
    let adiv3 = a / 3.0;
    if r2_minus_q3 < 0.0 {
        let theta = (r / q3.sqrt()).clamp(-1.0, 1.0).acos();
        let neg2_root_q = -2.0 * q.sqrt();
        let pi = std::f32::consts::PI;
        let mut t = [
            (neg2_root_q * (theta / 3.0).cos() - adiv3).clamp(0.0, 1.0),
            (neg2_root_q * ((theta + 2.0 * pi) / 3.0).cos() - adiv3).clamp(0.0, 1.0),
            (neg2_root_q * ((theta - 2.0 * pi) / 3.0).cos() - adiv3).clamp(0.0, 1.0),
        ];
        // bubble_sort then collaps_duplicates.
        t.sort_by(f32::total_cmp);
        let mut out = vec![t[0]];
        for &v in &t[1..] {
            if v != *out.last().expect("one value") {
                out.push(v);
            }
        }
        out
    } else {
        let mut a2 = r.abs() + r2_minus_q3.sqrt();
        a2 = a2.powf(0.333_333_3);
        if r > 0.0 {
            a2 = -a2;
        }
        if a2 != 0.0 {
            a2 += q / a2;
        }
        vec![(a2 - adiv3).clamp(0.0, 1.0)]
    }
}

/// `SkFindCubicMaxCurvature`.
fn find_cubic_max_curvature(c: [V; 4]) -> Vec<f32> {
    let x = formulate_f1_dot_f2([c[0].0, c[1].0, c[2].0, c[3].0]);
    let y = formulate_f1_dot_f2([c[0].1, c[1].1, c[2].1, c[3].1]);
    solve_cubic_poly([x[0] + y[0], x[1] + y[1], x[2] + y[2], x[3] + y[3]])
}

fn on_same_side(src: [V; 4], test: usize, line: usize) -> bool {
    let origin = src[line];
    let l = sub(src[line + 1], origin);
    let c0 = cross(l, sub(src[test], origin));
    let c1 = cross(l, sub(src[test + 1], origin));
    c0 * c1 >= 0.0
}

/// `SkFindCubicCusp`: the `t` of a cusp, or -1.
fn find_cubic_cusp(c: [V; 4]) -> f32 {
    if c[0] == c[1] || c[2] == c[3] {
        return -1.0;
    }
    if on_same_side(c, 0, 2) || on_same_side(c, 2, 0) {
        return -1.0;
    }
    let precision =
        (distance_to_sqd(c[1], c[0]) + distance_to_sqd(c[2], c[1]) + distance_to_sqd(c[3], c[2]))
            * 1e-8;
    for t in find_cubic_max_curvature(c) {
        if t <= 0.0 || t >= 1.0 {
            continue;
        }
        let d = eval_cubic_tangent_raw(c, t);
        if length_sqd(d) < precision {
            return t;
        }
    }
    -1.0
}

/// `eval_cubic_derivative` without the end-point substitutions.
fn eval_cubic_tangent_raw(c: [V; 4], t: f32) -> V {
    let a = sub(add(c[3], mul(sub(c[1], c[2]), 3.0)), c[0]);
    let b = mul(add(sub(c[2], mul(c[1], 2.0)), c[0]), 2.0);
    let cc = sub(c[1], c[0]);
    add(mul(add(mul(a, t), b), t), cc)
}

/// A conic: `SkConic`.
#[derive(Clone, Copy)]
struct Conic {
    p: [V; 3],
    w: f32,
}

impl Conic {
    /// `SkConic::evalAt` (`SkConicCoeff`).
    fn eval_at(&self, t: f32) -> V {
        let (p0, p1, p2) = (self.p[0], self.p[1], self.p[2]);
        let p1w = mul(p1, self.w);
        let na = add(sub(p2, mul(p1w, 2.0)), p0);
        let nb = mul(sub(p1w, p0), 2.0);
        let numer = add(mul(add(mul(na, t), nb), t), p0);
        let db = 2.0 * (self.w - 1.0);
        let da = -db;
        let denom = (da * t + db) * t + 1.0;
        (numer.0 / denom, numer.1 / denom)
    }

    /// `SkConic::evalTangentAt`.
    fn eval_tangent_at(&self, t: f32) -> V {
        let (p0, p1, p2) = (self.p[0], self.p[1], self.p[2]);
        if (t == 0.0 && p0 == p1) || (t == 1.0 && p1 == p2) {
            return sub(p2, p0);
        }
        let p20 = sub(p2, p0);
        let p10 = sub(p1, p0);
        let c = mul(p10, self.w);
        let a = sub(mul(p20, self.w), p20);
        let b = sub(sub(p20, c), c);
        add(mul(add(mul(a, t), b), t), c)
    }
}

/// `SkConic::BuildUnitArc` for the arc from unit vector `start` to `stop`
/// (clockwise when `cw`), scaled by `radius` about `pivot`: each conic's
/// control point, end point and weight.
fn build_unit_arc(start: V, stop: V, cw: bool, radius: f32, pivot: V) -> Vec<(V, V, f32)> {
    let x = dot(start, stop);
    let mut y = cross(start, stop);
    let abs_y = y.abs();
    if abs_y <= NEARLY_ZERO && x > 0.0 && ((y >= 0.0 && cw) || (y <= 0.0 && !cw)) {
        return Vec::new();
    }
    if !cw {
        y = -y;
    }
    let quadrant = if y == 0.0 {
        2
    } else if x == 0.0 {
        if y > 0.0 {
            1
        } else {
            3
        }
    } else {
        let mut q = 0;
        if y < 0.0 {
            q += 2;
        }
        if (x < 0.0) != (y < 0.0) {
            q += 1;
        }
        q
    };
    const QUADRANT: [V; 8] = [
        (1.0, 0.0),
        (1.0, 1.0),
        (0.0, 1.0),
        (-1.0, 1.0),
        (-1.0, 0.0),
        (-1.0, -1.0),
        (0.0, -1.0),
        (1.0, -1.0),
    ];
    let mut conics: Vec<([V; 3], f32)> = (0..quadrant)
        .map(|i| {
            (
                [
                    QUADRANT[i * 2],
                    QUADRANT[i * 2 + 1],
                    QUADRANT[(i * 2 + 2) % 8],
                ],
                FRAC_1_SQRT_2,
            )
        })
        .collect();
    let final_p = (x, y);
    let last_q = QUADRANT[(quadrant * 2) % 8];
    let d = dot(last_q, final_p);
    if d.is_nan() {
        return Vec::new();
    }
    if d < 1.0 {
        let cos_half = ((1.0 + d) / 2.0).sqrt();
        if let Some(off) = set_length(add(last_q, final_p), 1.0 / cos_half) {
            if !equals_within_tolerance(last_q, off, NEARLY_ZERO) {
                conics.push(([last_q, off, final_p], cos_half));
            }
        }
    }
    // setSinCos(start.y, start.x), preScale(1, -1) when anticlockwise, then
    // scale by the radius and translate to the pivot.
    let (sin, cos) = (start.1, start.0);
    let map = |p: V| {
        let (px, py) = if cw { p } else { (p.0, -p.1) };
        let (rx, ry) = (cos * px - sin * py, sin * px + cos * py);
        (rx * radius + pivot.0, ry * radius + pivot.1)
    };
    conics
        .into_iter()
        .map(|(p, w)| (map(p[1]), map(p[2]), w))
        .collect()
}

// ---------------------------------------------------------------------------
// A path builder that keeps conics (SkPathBuilder, as the stroker uses it)

#[derive(Clone, Copy, Debug, PartialEq)]
enum Verb {
    Move(V),
    Line(V),
    Quad(V, V),
    Conic(V, V, f32),
    Cubic(V, V, V),
    Close,
}

#[derive(Default)]
struct Builder {
    verbs: Vec<Verb>,
    last_move: Option<V>,
}

impl Builder {
    fn last_pt(&self) -> Option<V> {
        match self.verbs.last()? {
            Verb::Move(p) | Verb::Line(p) | Verb::Quad(_, p) | Verb::Conic(_, p, _) => Some(*p),
            Verb::Cubic(_, _, p) => Some(*p),
            Verb::Close => self.last_move,
        }
    }

    fn point_count(&self) -> usize {
        self.verbs
            .iter()
            .map(|v| match v {
                Verb::Move(_) | Verb::Line(_) => 1,
                Verb::Quad(..) | Verb::Conic(..) => 2,
                Verb::Cubic(..) => 3,
                Verb::Close => 0,
            })
            .sum()
    }

    fn points(&self) -> Vec<V> {
        let mut out = Vec::new();
        for v in &self.verbs {
            match *v {
                Verb::Move(p) | Verb::Line(p) => out.push(p),
                Verb::Quad(a, p) | Verb::Conic(a, p, _) => out.extend([a, p]),
                Verb::Cubic(a, b, p) => out.extend([a, b, p]),
                Verb::Close => {}
            }
        }
        out
    }

    /// `ensureMove`: after a close (or on an empty path) drawing starts at
    /// the last move point.
    fn ensure_move(&mut self) {
        match self.verbs.last() {
            None => self.move_to((0.0, 0.0)),
            Some(Verb::Close) => {
                let p = self.last_move.unwrap_or((0.0, 0.0));
                self.move_to(p);
            }
            _ => {}
        }
    }

    fn move_to(&mut self, p: V) {
        if let Some(Verb::Move(last)) = self.verbs.last_mut() {
            *last = p;
        } else {
            self.verbs.push(Verb::Move(p));
        }
        self.last_move = Some(p);
    }

    fn line_to(&mut self, p: V) {
        self.ensure_move();
        self.verbs.push(Verb::Line(p));
    }

    fn quad_to(&mut self, c: V, p: V) {
        self.ensure_move();
        self.verbs.push(Verb::Quad(c, p));
    }

    fn conic_to(&mut self, c: V, p: V, w: f32) {
        self.ensure_move();
        if w <= 0.0 {
            self.verbs.push(Verb::Line(p));
        } else if w == 1.0 {
            self.verbs.push(Verb::Quad(c, p));
        } else if w.is_finite() {
            self.verbs.push(Verb::Conic(c, p, w));
        } else {
            self.verbs.push(Verb::Line(c));
            self.verbs.push(Verb::Line(p));
        }
    }

    fn close(&mut self) {
        if matches!(self.verbs.last(), Some(v) if *v != Verb::Close) {
            self.ensure_move();
            self.verbs.push(Verb::Close);
        }
    }

    /// `setLastPoint`.
    fn set_last_point(&mut self, p: V) {
        match self.verbs.last_mut() {
            None => self.move_to(p),
            Some(Verb::Move(q) | Verb::Line(q) | Verb::Quad(_, q) | Verb::Conic(_, q, _)) => *q = p,
            Some(Verb::Cubic(_, _, q)) => *q = p,
            Some(Verb::Close) => {
                // Skia moves the last stored point: the contour's last
                // point before the close.
                let n = self.verbs.len();
                if n >= 2 {
                    match &mut self.verbs[n - 2] {
                        Verb::Move(q) | Verb::Line(q) | Verb::Quad(_, q) | Verb::Conic(_, q, _) => {
                            *q = p
                        }
                        Verb::Cubic(_, _, q) => *q = p,
                        Verb::Close => {}
                    }
                }
            }
        }
    }

    /// `privateReversePathTo`: the last contour of `path`, backwards, from
    /// its last point (which the caller has already reached).
    fn reverse_path_to(&mut self, path: &Builder) {
        let verbs = &path.verbs;
        let mut i = verbs.len();
        while i > 0 {
            i -= 1;
            // The point before this verb: where the reversed segment ends.
            let before = verbs[..i].iter().rev().find_map(|v| match *v {
                Verb::Move(p) | Verb::Line(p) | Verb::Quad(_, p) | Verb::Conic(_, p, _) => Some(p),
                Verb::Cubic(_, _, p) => Some(p),
                Verb::Close => None,
            });
            match verbs[i] {
                Verb::Move(_) => return,
                Verb::Line(_) => self.line_to(before.expect("a line follows a point")),
                Verb::Quad(c, _) => self.quad_to(c, before.expect("a quad follows a point")),
                Verb::Conic(c, _, w) => {
                    self.conic_to(c, before.expect("a conic follows a point"), w)
                }
                Verb::Cubic(a, b, _) => {
                    let p = before.expect("a cubic follows a point");
                    self.ensure_move();
                    self.verbs.push(Verb::Cubic(b, a, p));
                }
                Verb::Close => {}
            }
        }
    }

    fn add_path(&mut self, path: &Builder) {
        for v in &path.verbs {
            match *v {
                Verb::Move(p) => self.move_to(p),
                Verb::Close => self.close(),
                other => {
                    self.ensure_move();
                    self.verbs.push(other);
                }
            }
        }
    }

    /// `SkPath_RectPointIterator` order from `start`, clockwise or not.
    fn add_rect(&mut self, r: [f32; 4], cw: bool, start: usize) {
        let pts = [(r[0], r[1]), (r[2], r[1]), (r[2], r[3]), (r[0], r[3])];
        let step = if cw { 1 } else { 3 };
        let mut i = start % 4;
        self.move_to(pts[i]);
        for _ in 0..3 {
            i = (i + step) % 4;
            self.line_to(pts[i]);
        }
        self.close();
    }

    /// `addOval(oval, dir, start)`: four quarter conics.
    fn add_oval(&mut self, r: [f32; 4], cw: bool, start: usize) {
        let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
        let oval = [(cx, r[1]), (r[2], cy), (cx, r[3]), (r[0], cy)];
        let rect = [(r[0], r[1]), (r[2], r[1]), (r[2], r[3]), (r[0], r[3])];
        let step = if cw { 1 } else { 3 };
        let mut oi = start % 4;
        let mut ri = (start + usize::from(!cw)) % 4;
        self.move_to(oval[oi]);
        for _ in 0..4 {
            oi = (oi + step) % 4;
            ri = (ri + step) % 4;
            self.conic_to(rect[ri], oval[oi], FRAC_1_SQRT_2);
        }
        self.close();
    }

    /// `addRRect(MakeRectXY(r, rad, rad), dir)` with the legacy start index
    /// (6 clockwise, 7 anticlockwise), through `SkPathRawShapes::RRect`.
    fn add_rrect(&mut self, r: [f32; 4], rad: f32, cw: bool) {
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        let index: usize = if cw { 6 } else { 7 };
        // SkRRect::setRectXY: radii scaled down to fit, collapsing to a
        // rect or an oval.
        let (mut rx, mut ry) = (rad, rad);
        if rx <= 0.0 || ry <= 0.0 || w <= 0.0 || h <= 0.0 {
            return self.add_rect(r, cw, index.div_ceil(2));
        }
        if w < rx + rx || h < ry + ry {
            let scale = (w / (rx + rx)).min(h / (ry + ry));
            rx *= scale;
            ry *= scale;
        }
        if rx >= w / 2.0 && ry >= h / 2.0 {
            return self.add_oval(r, cw, index / 2);
        }
        let pts = [
            (r[0] + rx, r[1]),
            (r[2] - rx, r[1]),
            (r[2], r[1] + ry),
            (r[2], r[3] - ry),
            (r[2] - rx, r[3]),
            (r[0] + rx, r[3]),
            (r[0], r[3] - ry),
            (r[0], r[1] + ry),
        ];
        let rect = [(r[0], r[1]), (r[2], r[1]), (r[2], r[3]), (r[0], r[3])];
        let step8 = if cw { 1 } else { 7 };
        let step4 = if cw { 1 } else { 3 };
        let starts_with_conic = (index & 1 == 1) == cw;
        let mut pi = index % 8;
        let mut ri = (index / 2 + usize::from(!cw)) % 4;
        let mut next_p = || {
            pi = (pi + step8) % 8;
            pts[pi]
        };
        let mut next_r = || {
            ri = (ri + step4) % 4;
            rect[ri]
        };
        self.move_to(pts[index % 8]);
        if starts_with_conic {
            for _ in 0..3 {
                let c = next_r();
                let p = next_p();
                self.conic_to(c, p, FRAC_1_SQRT_2);
                let l = next_p();
                self.line_to(l);
            }
            let c = next_r();
            let p = next_p();
            self.conic_to(c, p, FRAC_1_SQRT_2);
        } else {
            for _ in 0..4 {
                let l = next_p();
                self.line_to(l);
                let c = next_r();
                let p = next_p();
                self.conic_to(c, p, FRAC_1_SQRT_2);
            }
        }
        self.close();
    }

    fn add_polygon(&mut self, pts: &[V]) {
        self.move_to(pts[0]);
        for &p in &pts[1..] {
            self.line_to(p);
        }
        self.close();
    }

    fn into_segs(self) -> Vec<Seg> {
        let p = |v: V| (f64::from(v.0), f64::from(v.1));
        self.verbs
            .into_iter()
            .map(|v| match v {
                Verb::Move(a) => Seg::Move(p(a)),
                Verb::Line(a) => Seg::Line(p(a)),
                Verb::Quad(a, b) => Seg::Quad(p(a), p(b)),
                Verb::Conic(a, b, w) => Seg::Conic(p(a), p(b), f64::from(w)),
                Verb::Cubic(a, b, c) => Seg::Cubic(p(a), p(b), p(c)),
                Verb::Close => Seg::Close,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Caps and joins (SkStrokerPriv.cpp)

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Cap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Join {
    Miter,
    Round,
    Bevel,
}

fn cap(kind: Cap, sink: &mut Builder, pivot: V, normal: V, stop: V, extend_last_pt: bool) {
    match kind {
        Cap::Butt => sink.line_to(stop),
        Cap::Round => {
            let parallel = rotate_cw(normal);
            let projected = add(pivot, parallel);
            sink.conic_to(add(projected, normal), projected, FRAC_1_SQRT_2);
            sink.conic_to(sub(projected, normal), stop, FRAC_1_SQRT_2);
        }
        Cap::Square => {
            let parallel = rotate_cw(normal);
            if extend_last_pt {
                sink.set_last_point(add(add(pivot, normal), parallel));
                sink.line_to(add(sub(pivot, normal), parallel));
            } else {
                sink.line_to(add(add(pivot, normal), parallel));
                sink.line_to(add(sub(pivot, normal), parallel));
                sink.line_to(stop);
            }
        }
    }
}

fn is_clockwise(before: V, after: V) -> bool {
    before.0 * after.1 > before.1 * after.0
}

#[derive(PartialEq, Eq)]
enum AngleType {
    Nearly180,
    Sharp,
    Shallow,
    NearlyLine,
}

fn dot_to_angle_type(d: f32) -> AngleType {
    if d >= 0.0 {
        if nearly_zero(1.0 - d) {
            AngleType::NearlyLine
        } else {
            AngleType::Shallow
        }
    } else if nearly_zero(1.0 + d) {
        AngleType::Nearly180
    } else {
        AngleType::Sharp
    }
}

fn handle_inner_join(inner: &mut Builder, pivot: V, after: V) {
    inner.line_to(pivot);
    inner.line_to(sub(pivot, after));
}

const ONE_OVER_SQRT2: f32 = 0.707_106_77;

#[allow(clippy::too_many_arguments)]
fn join(
    kind: Join,
    outer_b: &mut Builder,
    inner_b: &mut Builder,
    before_unit: V,
    pivot: V,
    after_unit: V,
    radius: f32,
    inv_miter_limit: f32,
    prev_is_line: bool,
    mut curr_is_line: bool,
) {
    match kind {
        Join::Bevel => {
            let mut after = mul(after_unit, radius);
            let (outer, inner) = if !is_clockwise(before_unit, after_unit) {
                after = neg(after);
                (inner_b, outer_b)
            } else {
                (outer_b, inner_b)
            };
            outer.line_to(add(pivot, after));
            handle_inner_join(inner, pivot, after);
        }
        Join::Round => {
            let d = dot(before_unit, after_unit);
            if dot_to_angle_type(d) == AngleType::NearlyLine {
                return;
            }
            let (mut before, mut after) = (before_unit, after_unit);
            let cw = is_clockwise(before, after);
            let (outer, inner) = if !cw {
                before = neg(before);
                after = neg(after);
                (inner_b, outer_b)
            } else {
                (outer_b, inner_b)
            };
            let conics = build_unit_arc(before, after, cw, radius, pivot);
            if !conics.is_empty() {
                for (c, p, w) in conics {
                    outer.conic_to(c, p, w);
                }
                handle_inner_join(inner, pivot, mul(after, radius));
            }
        }
        Join::Miter => {
            let d = dot(before_unit, after_unit);
            let angle = dot_to_angle_type(d);
            let (mut before, mut after) = (before_unit, after_unit);
            if angle == AngleType::NearlyLine {
                return;
            }
            let (outer, inner): (&mut Builder, &mut Builder);
            if angle == AngleType::Nearly180 {
                curr_is_line = false;
                // DO_BLUNT with the original builders.
                let after = mul(after, radius);
                if !curr_is_line {
                    outer_b.line_to(add(pivot, after));
                }
                handle_inner_join(inner_b, pivot, after);
                return;
            }
            let ccw = !is_clockwise(before, after);
            if ccw {
                before = neg(before);
                after = neg(after);
                outer = inner_b;
                inner = outer_b;
            } else {
                outer = outer_b;
                inner = inner_b;
            }
            let mid = if d == 0.0 && inv_miter_limit <= ONE_OVER_SQRT2 {
                mul(add(before, after), radius)
            } else {
                let sin_half = ((1.0 + d) / 2.0).sqrt();
                if sin_half < inv_miter_limit {
                    let after = mul(after, radius);
                    outer.line_to(add(pivot, after));
                    handle_inner_join(inner, pivot, after);
                    return;
                }
                let m = if angle == AngleType::Sharp {
                    let m = (after.1 - before.1, before.0 - after.0);
                    if ccw {
                        neg(m)
                    } else {
                        m
                    }
                } else {
                    add(before, after)
                };
                // setLength leaves (0, 0) when it fails.
                set_length(m, radius / sin_half).unwrap_or((0.0, 0.0))
            };
            if prev_is_line {
                outer.set_last_point(add(pivot, mid));
            } else {
                outer.line_to(add(pivot, mid));
            }
            let after = mul(after, radius);
            if !curr_is_line {
                outer.line_to(add(pivot, after));
            }
            handle_inner_join(inner, pivot, after);
        }
    }
}

// ---------------------------------------------------------------------------
// SkPathStroker (SkStroke.cpp)

const RECURSIVE_LIMITS: [i32; 4] = [5 * 3, 24, 11 * 3, 11 * 3];
const TANGENT_LIMIT: usize = 0;
const CONIC_LIMIT: usize = 2;
const QUAD_LIMIT: usize = 3;

#[derive(Clone, Copy, Default)]
struct QuadConstruct {
    quad: [V; 3],
    tangent_start: V,
    tangent_end: V,
    start_t: f32,
    mid_t: f32,
    end_t: f32,
    start_set: bool,
    end_set: bool,
    opposite_tangents: bool,
}

impl QuadConstruct {
    fn init(&mut self, start: f32, end: f32) -> bool {
        self.start_t = start;
        self.mid_t = (start + end) * 0.5;
        self.end_t = end;
        self.start_set = false;
        self.end_set = false;
        self.start_t < self.mid_t && self.mid_t < self.end_t
    }

    fn init_with_start(&mut self, parent: &QuadConstruct) -> bool {
        if !self.init(parent.start_t, parent.mid_t) {
            return false;
        }
        self.quad[0] = parent.quad[0];
        self.tangent_start = parent.tangent_start;
        self.start_set = true;
        true
    }

    fn init_with_end(&mut self, parent: &QuadConstruct) -> bool {
        if !self.init(parent.mid_t, parent.end_t) {
            return false;
        }
        self.quad[2] = parent.quad[2];
        self.tangent_end = parent.tangent_end;
        self.end_set = true;
        true
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum ResultType {
    Split,
    Degenerate,
    Quad,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Reduction {
    Point,
    Line,
    Quad,
    Degenerate,
    Degenerate2,
    Degenerate3,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum RayType {
    CtrlPt,
    ResultType,
}

fn degenerate_vector(v: V) -> bool {
    !can_normalize(v)
}

/// `set_normal_unitnormal(before, after, scale, radius)`.
fn normal_unit_normal(before: V, after: V, scale: f32, radius: f32) -> Option<(V, V)> {
    let unit = set_length(
        ((after.0 - before.0) * scale, (after.1 - before.1) * scale),
        1.0,
    )?;
    let unit = rotate_ccw(unit);
    Some((mul(unit, radius), unit))
}

fn normal_unit_normal_vec(v: V, radius: f32) -> Option<(V, V)> {
    let unit = rotate_ccw(set_length(v, 1.0)?);
    Some((mul(unit, radius), unit))
}

fn pt_to_line(pt: V, start: V, end: V) -> f32 {
    let dxy = sub(end, start);
    let ab0 = sub(pt, start);
    let t = dot(dxy, ab0) / dot(dxy, dxy);
    if (0.0..=1.0).contains(&t) {
        let hit = add(mul(start, 1.0 - t), mul(end, t));
        distance_to_sqd(hit, pt)
    } else {
        distance_to_sqd(pt, start)
    }
}

fn pt_to_tangent_line(pt: V, start: V, tangent: V) -> f32 {
    let ab0 = sub(pt, start);
    let t = dot(tangent, ab0) / dot(tangent, tangent);
    if (0.0..=1.0).contains(&t) {
        let hit = add(start, mul(tangent, t));
        distance_to_sqd(hit, pt)
    } else {
        distance_to_sqd(pt, start)
    }
}

fn cubic_in_line(c: [V; 4]) -> bool {
    let mut pt_max = -1.0f32;
    let (mut outer1, mut outer2) = (0usize, 0usize);
    for index in 0..3 {
        for inner in index + 1..4 {
            let d = sub(c[inner], c[index]);
            let m = d.0.abs().max(d.1.abs());
            if pt_max < m {
                outer1 = index;
                outer2 = inner;
                pt_max = m;
            }
        }
    }
    let mid1 = (1 + (2 >> outer2)) >> outer1;
    let mid2 = outer1 ^ outer2 ^ mid1;
    let slop = pt_max * pt_max * 0.00001;
    pt_to_line(c[mid1], c[outer1], c[outer2]) <= slop
        && pt_to_line(c[mid2], c[outer1], c[outer2]) <= slop
}

fn quad_in_line(q: [V; 3]) -> bool {
    let mut pt_max = -1.0f32;
    let (mut outer1, mut outer2) = (0usize, 0usize);
    for index in 0..2 {
        for inner in index + 1..3 {
            let d = sub(q[inner], q[index]);
            let m = d.0.abs().max(d.1.abs());
            if pt_max < m {
                outer1 = index;
                outer2 = inner;
                pt_max = m;
            }
        }
    }
    let mid = outer1 ^ outer2 ^ 3;
    let slop = pt_max * pt_max * 0.000005;
    pt_to_line(q[mid], q[outer1], q[outer2]) <= slop
}

fn check_quad_linear(q: [V; 3]) -> (Reduction, V) {
    let ab = degenerate_vector(sub(q[1], q[0]));
    let bc = degenerate_vector(sub(q[2], q[1]));
    if ab && bc {
        return (Reduction::Point, (0.0, 0.0));
    }
    if ab || bc {
        return (Reduction::Line, (0.0, 0.0));
    }
    if !quad_in_line(q) {
        return (Reduction::Quad, (0.0, 0.0));
    }
    let t = find_quad_max_curvature(q);
    if t == 0.0 || t == 1.0 {
        return (Reduction::Line, (0.0, 0.0));
    }
    (Reduction::Degenerate, eval_quad_at(q, t))
}

fn check_conic_linear(c: &Conic) -> (Reduction, V) {
    let ab = degenerate_vector(sub(c.p[1], c.p[0]));
    let bc = degenerate_vector(sub(c.p[2], c.p[1]));
    if ab && bc {
        return (Reduction::Point, (0.0, 0.0));
    }
    if ab || bc {
        return (Reduction::Line, (0.0, 0.0));
    }
    if !quad_in_line(c.p) {
        return (Reduction::Quad, (0.0, 0.0));
    }
    let t = find_quad_max_curvature(c.p);
    if t == 0.0 || t.is_nan() {
        return (Reduction::Line, (0.0, 0.0));
    }
    (Reduction::Degenerate, c.eval_at(t))
}

/// `CheckCubicLinear`: the reduction, its points, and the point the start
/// tangent aims at.
fn check_cubic_linear(c: [V; 4]) -> (Reduction, Vec<V>, V) {
    let ab = degenerate_vector(sub(c[1], c[0]));
    let bc = degenerate_vector(sub(c[2], c[1]));
    let cd = degenerate_vector(sub(c[3], c[2]));
    if ab && bc && cd {
        return (Reduction::Point, Vec::new(), c[1]);
    }
    if usize::from(ab) + usize::from(bc) + usize::from(cd) == 2 {
        return (Reduction::Line, Vec::new(), c[1]);
    }
    if !cubic_in_line(c) {
        let tangent_pt = if ab { c[2] } else { c[1] };
        return (Reduction::Quad, Vec::new(), tangent_pt);
    }
    let mut reduction = Vec::new();
    for t in find_cubic_max_curvature(c) {
        if t <= 0.0 || t >= 1.0 {
            continue;
        }
        let p = eval_cubic_at(c, t);
        if p != c[0] && p != c[3] {
            reduction.push(p);
        }
    }
    match reduction.len() {
        0 => (Reduction::Line, reduction, c[1]),
        1 => (Reduction::Degenerate, reduction, c[1]),
        2 => (Reduction::Degenerate2, reduction, c[1]),
        _ => (Reduction::Degenerate3, reduction, c[1]),
    }
}

fn intersect_quad_ray(line: [V; 2], quad: [V; 3]) -> Vec<f32> {
    let vec = sub(line[1], line[0]);
    let r: Vec<f32> = quad.iter().map(|q| cross(vec, sub(*q, line[0]))).collect();
    let (mut a, mut b, c) = (r[2], r[1], r[0]);
    a += c - 2.0 * b;
    b -= c;
    find_unit_quad_roots(a, 2.0 * b, c)
}

fn points_within_dist(near: V, far: V, limit: f32) -> bool {
    distance_to_sqd(near, far) <= limit * limit
}

fn sharp_angle(q: [V; 3]) -> bool {
    let mut smaller = sub(q[1], q[0]);
    let mut larger = sub(q[1], q[2]);
    let smaller_len = length_sqd(smaller);
    let mut larger_len = length_sqd(larger);
    if smaller_len > larger_len {
        std::mem::swap(&mut smaller, &mut larger);
        larger_len = smaller_len;
    }
    match set_length(smaller, larger_len) {
        Some(s) => dot(s, larger) > 0.0,
        None => false,
    }
}

struct Stroker {
    radius: f32,
    inv_miter_limit: f32,
    res_scale: f32,
    inv_res_scale: f32,
    inv_res_scale_squared: f32,
    first_normal: V,
    prev_normal: V,
    first_unit_normal: V,
    prev_unit_normal: V,
    first_pt: V,
    prev_pt: V,
    first_outer_pt: V,
    first_outer_pt_index_in_contour: usize,
    segment_count: i32,
    prev_is_line: bool,
    cap: Cap,
    join: Join,
    inner: Builder,
    outer: Builder,
    cusper: Builder,
    stroke_type: f32,
    recursion_depth: i32,
    found_tangents: bool,
    join_completed: bool,
}

impl Stroker {
    fn new(radius: f32, miter_limit: f32, cap: Cap, mut join: Join, res_scale: f32) -> Self {
        let mut inv_miter_limit = 0.0;
        if join == Join::Miter {
            if miter_limit <= 1.0 {
                join = Join::Bevel;
            } else {
                inv_miter_limit = 1.0 / miter_limit;
            }
        }
        let inv_res_scale = 1.0 / (res_scale * 4.0);
        Stroker {
            radius,
            inv_miter_limit,
            res_scale,
            inv_res_scale,
            inv_res_scale_squared: inv_res_scale * inv_res_scale,
            first_normal: (0.0, 0.0),
            prev_normal: (0.0, 0.0),
            first_unit_normal: (0.0, 0.0),
            prev_unit_normal: (0.0, 0.0),
            first_pt: (0.0, 0.0),
            prev_pt: (0.0, 0.0),
            first_outer_pt: (0.0, 0.0),
            first_outer_pt_index_in_contour: 0,
            segment_count: -1,
            prev_is_line: false,
            cap,
            join,
            inner: Builder::default(),
            outer: Builder::default(),
            cusper: Builder::default(),
            stroke_type: 1.0,
            recursion_depth: 0,
            found_tangents: false,
            join_completed: false,
        }
    }

    fn do_join(
        &mut self,
        before_unit: V,
        pivot: V,
        after_unit: V,
        prev_is_line: bool,
        curr_is_line: bool,
    ) {
        join(
            self.join,
            &mut self.outer,
            &mut self.inner,
            before_unit,
            pivot,
            after_unit,
            self.radius,
            self.inv_miter_limit,
            prev_is_line,
            curr_is_line,
        );
    }

    fn pre_join_to(&mut self, curr: V, curr_is_line: bool) -> Option<(V, V)> {
        let (normal, unit) =
            match normal_unit_normal(self.prev_pt, curr, self.res_scale, self.radius) {
                Some(n) => n,
                None => {
                    if self.cap == Cap::Butt {
                        return None;
                    }
                    ((self.radius, 0.0), (1.0, 0.0))
                }
            };
        if self.segment_count == 0 {
            self.first_normal = normal;
            self.first_unit_normal = unit;
            self.first_outer_pt = add(self.prev_pt, normal);
            self.outer.move_to(self.first_outer_pt);
            self.inner.move_to(sub(self.prev_pt, normal));
        } else {
            let (before, pivot, prev_line) =
                (self.prev_unit_normal, self.prev_pt, self.prev_is_line);
            self.do_join(before, pivot, unit, prev_line, curr_is_line);
        }
        self.prev_is_line = curr_is_line;
        Some((normal, unit))
    }

    fn post_join_to(&mut self, curr: V, normal: V, unit: V) {
        self.join_completed = true;
        self.prev_pt = curr;
        self.prev_unit_normal = unit;
        self.prev_normal = normal;
        self.segment_count += 1;
    }

    fn finish_contour(&mut self, close: bool, curr_is_line: bool) {
        if self.segment_count > 0 {
            if close {
                let (before, pivot, after, prev_line) = (
                    self.prev_unit_normal,
                    self.prev_pt,
                    self.first_unit_normal,
                    self.prev_is_line,
                );
                self.do_join(before, pivot, after, prev_line, curr_is_line);
                self.outer.close();
                if let Some(pt) = self.inner.last_pt() {
                    self.outer.move_to(pt);
                    let inner = std::mem::take(&mut self.inner);
                    self.outer.reverse_path_to(&inner);
                    self.outer.close();
                }
            } else if let Some(pt) = self.inner.last_pt() {
                let (prev_pt, prev_normal) = (self.prev_pt, self.prev_normal);
                cap(
                    self.cap,
                    &mut self.outer,
                    prev_pt,
                    prev_normal,
                    pt,
                    curr_is_line,
                );
                let inner = std::mem::take(&mut self.inner);
                self.outer.reverse_path_to(&inner);
                let (first_pt, first_normal, first_outer, prev_line) = (
                    self.first_pt,
                    self.first_normal,
                    self.first_outer_pt,
                    self.prev_is_line,
                );
                cap(
                    self.cap,
                    &mut self.outer,
                    first_pt,
                    neg(first_normal),
                    first_outer,
                    prev_line,
                );
                self.outer.close();
            }
            if !self.cusper.verbs.is_empty() {
                let cusper = std::mem::take(&mut self.cusper);
                self.outer.add_path(&cusper);
            }
        }
        self.inner = Builder::default();
        self.segment_count = -1;
        self.first_outer_pt_index_in_contour = self.outer.point_count();
    }

    fn move_to(&mut self, p: V) {
        if self.segment_count > 0 {
            self.finish_contour(false, false);
        }
        self.segment_count = 0;
        self.first_pt = p;
        self.prev_pt = p;
        self.join_completed = false;
    }

    fn line_to_normal(&mut self, curr: V, normal: V) {
        self.outer.line_to(add(curr, normal));
        self.inner.line_to(sub(curr, normal));
    }

    /// `lineTo(currPt, iter)`; `has_valid_tangent` says whether a later
    /// segment of the contour has a direction.
    fn line_to(&mut self, curr: V, has_valid_tangent: Option<bool>) {
        let teeny = equals_within_tolerance(self.prev_pt, curr, NEARLY_ZERO * self.inv_res_scale);
        if self.cap == Cap::Butt && teeny {
            return;
        }
        if teeny && (self.join_completed || has_valid_tangent == Some(true)) {
            return;
        }
        let Some((normal, unit)) = self.pre_join_to(curr, true) else {
            return;
        };
        self.line_to_normal(curr, normal);
        self.post_join_to(curr, normal, unit);
    }

    fn set_quad_end_normal(&self, q: [V; 3], normal_ab: V, unit_ab: V) -> (V, V) {
        normal_unit_normal(q[1], q[2], self.res_scale, self.radius).unwrap_or((normal_ab, unit_ab))
    }

    fn set_cubic_end_normal(&self, c: [V; 4], normal_ab: V, unit_ab: V) -> (V, V) {
        let mut ab = sub(c[1], c[0]);
        let mut cd = sub(c[3], c[2]);
        let mut dab = degenerate_vector(ab);
        let mut dcd = degenerate_vector(cd);
        if dab && dcd {
            return (normal_ab, unit_ab);
        }
        if dab {
            ab = sub(c[2], c[0]);
            dab = degenerate_vector(ab);
        }
        if dcd {
            cd = sub(c[3], c[1]);
            dcd = degenerate_vector(cd);
        }
        if dab || dcd {
            return (normal_ab, unit_ab);
        }
        normal_unit_normal_vec(cd, self.radius).unwrap_or((normal_ab, unit_ab))
    }

    fn init(&mut self, stroke_type: f32, q: &mut QuadConstruct, t_start: f32, t_end: f32) {
        self.stroke_type = stroke_type;
        self.found_tangents = false;
        q.init(t_start, t_end);
    }

    fn sink(&mut self) -> &mut Builder {
        if self.stroke_type > 0.0 {
            &mut self.outer
        } else {
            &mut self.inner
        }
    }

    fn set_ray_pts(&self, t_pt: V, dxy: V) -> (V, V) {
        let dxy = set_length(dxy, self.radius).unwrap_or((self.radius, 0.0));
        let flip = self.stroke_type;
        ((t_pt.0 + flip * dxy.1, t_pt.1 - flip * dxy.0), dxy)
    }

    fn conic_perp_ray(&self, c: &Conic, t: f32) -> (V, V, V) {
        let t_pt = c.eval_at(t);
        let mut dxy = c.eval_tangent_at(t);
        if is_zero(dxy) {
            dxy = sub(c.p[2], c.p[0]);
        }
        let (on, tangent) = self.set_ray_pts(t_pt, dxy);
        (t_pt, on, tangent)
    }

    fn conic_quad_ends(&self, c: &Conic, q: &mut QuadConstruct) {
        if !q.start_set {
            let (_, on, tangent) = self.conic_perp_ray(c, q.start_t);
            q.quad[0] = on;
            q.tangent_start = tangent;
            q.start_set = true;
        }
        if !q.end_set {
            let (_, on, tangent) = self.conic_perp_ray(c, q.end_t);
            q.quad[2] = on;
            q.tangent_end = tangent;
            q.end_set = true;
        }
    }

    fn cubic_perp_ray(&self, c: [V; 4], t: f32) -> (V, V, V) {
        let t_pt = eval_cubic_at(c, t);
        let mut dxy = eval_cubic_tangent(c, t);
        if is_zero(dxy) {
            let mut c_pts = c;
            if nearly_zero(t) {
                dxy = sub(c[2], c[0]);
            } else if nearly_zero(1.0 - t) {
                dxy = sub(c[3], c[1]);
            } else {
                let chopped = chop_cubic_at(c, t);
                dxy = sub(chopped[3], chopped[2]);
                if is_zero(dxy) {
                    dxy = sub(chopped[3], chopped[1]);
                    c_pts = [chopped[0], chopped[1], chopped[2], chopped[3]];
                }
            }
            if is_zero(dxy) {
                dxy = sub(c_pts[3], c_pts[0]);
            }
        }
        let (on, tangent) = self.set_ray_pts(t_pt, dxy);
        (t_pt, on, tangent)
    }

    fn cubic_quad_ends(&self, c: [V; 4], q: &mut QuadConstruct) {
        if !q.start_set {
            let (_, on, tangent) = self.cubic_perp_ray(c, q.start_t);
            q.quad[0] = on;
            q.tangent_start = tangent;
            q.start_set = true;
        }
        if !q.end_set {
            let (_, on, tangent) = self.cubic_perp_ray(c, q.end_t);
            q.quad[2] = on;
            q.tangent_end = tangent;
            q.end_set = true;
        }
    }

    fn quad_perp_ray(&self, qd: [V; 3], t: f32) -> (V, V, V) {
        let t_pt = eval_quad_at(qd, t);
        let mut dxy = eval_quad_tangent_at(qd, t);
        if is_zero(dxy) {
            dxy = sub(qd[2], qd[0]);
        }
        let (on, tangent) = self.set_ray_pts(t_pt, dxy);
        (t_pt, on, tangent)
    }

    fn intersect_ray(&self, q: &mut QuadConstruct, ray_type: RayType) -> ResultType {
        let (start, end) = (q.quad[0], q.quad[2]);
        let (a_len, b_len) = (q.tangent_start, q.tangent_end);
        let denom = cross(a_len, b_len);
        if denom == 0.0 || !denom.is_finite() {
            q.opposite_tangents = dot(a_len, b_len) < 0.0;
            return ResultType::Degenerate;
        }
        q.opposite_tangents = false;
        let ab0 = sub(start, end);
        let mut numer_a = cross(b_len, ab0);
        let numer_b = cross(a_len, ab0);
        if (numer_a >= 0.0) == (numer_b >= 0.0) {
            let dist1 = pt_to_tangent_line(start, end, q.tangent_end);
            let dist2 = pt_to_tangent_line(end, start, q.tangent_start);
            if dist1.max(dist2) <= self.inv_res_scale_squared {
                return ResultType::Degenerate;
            }
            return ResultType::Split;
        }
        numer_a /= denom;
        let valid_divide = numer_a > numer_a - 1.0;
        if valid_divide {
            if ray_type == RayType::CtrlPt {
                q.quad[1] = add(start, mul(q.tangent_start, numer_a));
            }
            return ResultType::Quad;
        }
        q.opposite_tangents = dot(a_len, b_len) < 0.0;
        ResultType::Degenerate
    }

    fn tangents_meet(&self, c: [V; 4], q: &mut QuadConstruct) -> ResultType {
        self.cubic_quad_ends(c, q);
        self.intersect_ray(q, RayType::ResultType)
    }

    fn pt_in_quad_bounds(&self, q: [V; 3], pt: V) -> bool {
        let x_min = q[0].0.min(q[1].0).min(q[2].0);
        if pt.0 + self.inv_res_scale < x_min {
            return false;
        }
        let x_max = q[0].0.max(q[1].0).max(q[2].0);
        if pt.0 - self.inv_res_scale > x_max {
            return false;
        }
        let y_min = q[0].1.min(q[1].1).min(q[2].1);
        if pt.1 + self.inv_res_scale < y_min {
            return false;
        }
        let y_max = q[0].1.max(q[1].1).max(q[2].1);
        pt.1 - self.inv_res_scale <= y_max
    }

    fn stroke_close_enough(&self, stroke: [V; 3], ray: [V; 2], q: &QuadConstruct) -> ResultType {
        let stroke_mid = eval_quad_at(stroke, 0.5);
        if points_within_dist(ray[0], stroke_mid, self.inv_res_scale) {
            if sharp_angle(q.quad) {
                return ResultType::Split;
            }
            return ResultType::Quad;
        }
        if !self.pt_in_quad_bounds(stroke, ray[0]) {
            return ResultType::Split;
        }
        let roots = intersect_quad_ray(ray, stroke);
        if roots.len() != 1 {
            return ResultType::Split;
        }
        let quad_pt = eval_quad_at(stroke, roots[0]);
        let error = self.inv_res_scale * (1.0 - (roots[0] - 0.5).abs() * 2.0);
        if points_within_dist(ray[0], quad_pt, error) {
            if sharp_angle(q.quad) {
                return ResultType::Split;
            }
            return ResultType::Quad;
        }
        ResultType::Split
    }

    fn compare_quad_cubic(&self, c: [V; 4], q: &mut QuadConstruct) -> ResultType {
        self.cubic_quad_ends(c, q);
        let r = self.intersect_ray(q, RayType::CtrlPt);
        if r != ResultType::Quad {
            return r;
        }
        let (t_pt, on, _) = self.cubic_perp_ray(c, q.mid_t);
        self.stroke_close_enough(q.quad, [on, t_pt], q)
    }

    fn compare_quad_conic(&self, c: &Conic, q: &mut QuadConstruct) -> ResultType {
        self.conic_quad_ends(c, q);
        let r = self.intersect_ray(q, RayType::CtrlPt);
        if r != ResultType::Quad {
            return r;
        }
        let (t_pt, on, _) = self.conic_perp_ray(c, q.mid_t);
        self.stroke_close_enough(q.quad, [on, t_pt], q)
    }

    fn compare_quad_quad(&self, qd: [V; 3], q: &mut QuadConstruct) -> ResultType {
        if !q.start_set {
            let (_, on, tangent) = self.quad_perp_ray(qd, q.start_t);
            q.quad[0] = on;
            q.tangent_start = tangent;
            q.start_set = true;
        }
        if !q.end_set {
            let (_, on, tangent) = self.quad_perp_ray(qd, q.end_t);
            q.quad[2] = on;
            q.tangent_end = tangent;
            q.end_set = true;
        }
        let r = self.intersect_ray(q, RayType::CtrlPt);
        if r != ResultType::Quad {
            return r;
        }
        let (t_pt, on, _) = self.quad_perp_ray(qd, q.mid_t);
        self.stroke_close_enough(q.quad, [on, t_pt], q)
    }

    fn add_degenerate_line(&mut self, q: &QuadConstruct) {
        let p = q.quad[2];
        self.sink().line_to(p);
    }

    fn cubic_mid_on_line(&self, c: [V; 4], q: &QuadConstruct) -> bool {
        let (_, stroke_mid, _) = self.cubic_perp_ray(c, q.mid_t);
        pt_to_line(stroke_mid, q.quad[0], q.quad[2]) < self.inv_res_scale_squared
    }

    fn cubic_stroke(&mut self, c: [V; 4], q: &mut QuadConstruct) -> bool {
        if !self.found_tangents {
            let r = self.tangents_meet(c, q);
            if r != ResultType::Quad {
                if (r == ResultType::Degenerate
                    || points_within_dist(q.quad[0], q.quad[2], self.inv_res_scale))
                    && self.cubic_mid_on_line(c, q)
                {
                    self.add_degenerate_line(q);
                    return true;
                }
            } else {
                self.found_tangents = true;
            }
        }
        if self.found_tangents {
            let r = self.compare_quad_cubic(c, q);
            if r == ResultType::Quad {
                let (c1, p) = (q.quad[1], q.quad[2]);
                self.sink().quad_to(c1, p);
                return true;
            }
            if r == ResultType::Degenerate && !q.opposite_tangents {
                self.add_degenerate_line(q);
                return true;
            }
        }
        if !is_finite(q.quad[2]) {
            return false;
        }
        self.recursion_depth += 1;
        if self.recursion_depth > RECURSIVE_LIMITS[usize::from(self.found_tangents)] {
            self.add_degenerate_line(q);
            return true;
        }
        let mut half = QuadConstruct::default();
        if !half.init_with_start(q) {
            self.add_degenerate_line(q);
            self.recursion_depth -= 1;
            return true;
        }
        if !self.cubic_stroke(c, &mut half) {
            return false;
        }
        if !half.init_with_end(q) {
            self.add_degenerate_line(q);
            self.recursion_depth -= 1;
            return true;
        }
        if !self.cubic_stroke(c, &mut half) {
            return false;
        }
        self.recursion_depth -= 1;
        true
    }

    fn conic_stroke(&mut self, c: &Conic, q: &mut QuadConstruct) -> bool {
        let r = self.compare_quad_conic(c, q);
        if r == ResultType::Quad {
            let (c1, p) = (q.quad[1], q.quad[2]);
            self.sink().quad_to(c1, p);
            return true;
        }
        if r == ResultType::Degenerate {
            self.add_degenerate_line(q);
            return true;
        }
        self.recursion_depth += 1;
        if self.recursion_depth > RECURSIVE_LIMITS[CONIC_LIMIT] {
            self.add_degenerate_line(q);
            return true;
        }
        let mut half = QuadConstruct::default();
        let _ = half.init_with_start(q);
        if !self.conic_stroke(c, &mut half) {
            return false;
        }
        let _ = half.init_with_end(q);
        if !self.conic_stroke(c, &mut half) {
            return false;
        }
        self.recursion_depth -= 1;
        true
    }

    fn quad_stroke(&mut self, qd: [V; 3], q: &mut QuadConstruct) -> bool {
        let r = self.compare_quad_quad(qd, q);
        if r == ResultType::Quad {
            let (c1, p) = (q.quad[1], q.quad[2]);
            self.sink().quad_to(c1, p);
            return true;
        }
        if r == ResultType::Degenerate {
            self.add_degenerate_line(q);
            return true;
        }
        self.recursion_depth += 1;
        if self.recursion_depth > RECURSIVE_LIMITS[QUAD_LIMIT] {
            self.add_degenerate_line(q);
            return true;
        }
        let mut half = QuadConstruct::default();
        let _ = half.init_with_start(q);
        if !self.quad_stroke(qd, &mut half) {
            return false;
        }
        let _ = half.init_with_end(q);
        if !self.quad_stroke(qd, &mut half) {
            return false;
        }
        self.recursion_depth -= 1;
        true
    }

    /// A degenerate curve as lines through its reduction points, joined
    /// round.
    fn lines_through(&mut self, points: &[V], end: V) {
        let save = self.join;
        let mut first = true;
        for &p in points {
            self.line_to(p, None);
            if first {
                self.join = Join::Round;
                first = false;
            }
        }
        self.line_to(end, None);
        self.join = save;
    }

    fn quad_to(&mut self, p1: V, p2: V) {
        let quad = [self.prev_pt, p1, p2];
        let (reduction, point) = check_quad_linear(quad);
        match reduction {
            Reduction::Point | Reduction::Line => return self.line_to(p2, None),
            Reduction::Degenerate => return self.lines_through(&[point], p2),
            _ => {}
        }
        let Some((normal_ab, unit_ab)) = self.pre_join_to(p1, false) else {
            return self.line_to(p2, None);
        };
        let mut q = QuadConstruct::default();
        self.init(1.0, &mut q, 0.0, 1.0);
        self.quad_stroke(quad, &mut q);
        self.init(-1.0, &mut q, 0.0, 1.0);
        self.quad_stroke(quad, &mut q);
        let (normal_bc, unit_bc) = self.set_quad_end_normal(quad, normal_ab, unit_ab);
        self.post_join_to(p2, normal_bc, unit_bc);
    }

    fn conic_to(&mut self, p1: V, p2: V, w: f32) {
        let conic = Conic {
            p: [self.prev_pt, p1, p2],
            w,
        };
        let (reduction, point) = check_conic_linear(&conic);
        match reduction {
            Reduction::Point | Reduction::Line => return self.line_to(p2, None),
            Reduction::Degenerate => return self.lines_through(&[point], p2),
            _ => {}
        }
        let Some((normal_ab, unit_ab)) = self.pre_join_to(p1, false) else {
            return self.line_to(p2, None);
        };
        let mut q = QuadConstruct::default();
        self.init(1.0, &mut q, 0.0, 1.0);
        self.conic_stroke(&conic, &mut q);
        self.init(-1.0, &mut q, 0.0, 1.0);
        self.conic_stroke(&conic, &mut q);
        let (normal_bc, unit_bc) = self.set_quad_end_normal(conic.p, normal_ab, unit_ab);
        self.post_join_to(p2, normal_bc, unit_bc);
    }

    fn cubic_to(&mut self, p1: V, p2: V, p3: V) {
        let cubic = [self.prev_pt, p1, p2, p3];
        let (reduction, points, tangent_pt) = check_cubic_linear(cubic);
        match reduction {
            Reduction::Point | Reduction::Line => return self.line_to(p3, None),
            Reduction::Degenerate | Reduction::Degenerate2 | Reduction::Degenerate3 => {
                return self.lines_through(&points, p3)
            }
            Reduction::Quad => {}
        }
        let Some((normal_ab, unit_ab)) = self.pre_join_to(tangent_pt, false) else {
            return self.line_to(p3, None);
        };
        let ts = find_cubic_inflections(cubic);
        let mut last_t = 0.0;
        for index in 0..=ts.len() {
            let next_t = if index < ts.len() { ts[index] } else { 1.0 };
            let mut q = QuadConstruct::default();
            self.init(1.0, &mut q, last_t, next_t);
            self.cubic_stroke(cubic, &mut q);
            self.init(-1.0, &mut q, last_t, next_t);
            self.cubic_stroke(cubic, &mut q);
            last_t = next_t;
        }
        let cusp = find_cubic_cusp(cubic);
        if cusp > 0.0 {
            let loc = eval_cubic_at(cubic, cusp);
            let r = self.radius;
            self.cusper
                .add_oval([loc.0 - r, loc.1 - r, loc.0 + r, loc.1 + r], true, 1);
        }
        let (normal_cd, unit_cd) = self.set_cubic_end_normal(cubic, normal_ab, unit_ab);
        self.post_join_to(p3, normal_cd, unit_cd);
    }

    fn has_only_move_to(&self) -> bool {
        self.segment_count == 0
    }

    fn is_current_contour_empty(&self) -> bool {
        let zero_since = |pts: &[V], start: usize| {
            let rest = pts.get(start..).unwrap_or(&[]);
            rest.len() < 2 || rest[1..].iter().all(|p| *p == rest[0])
        };
        zero_since(&self.inner.points(), 0)
            && zero_since(&self.outer.points(), self.first_outer_pt_index_in_contour)
    }
}

/// How a path is stroked: `SkStrokeRec`'s width, miter limit, cap, join
/// and the resolution scale of the matrix it is drawn under.
pub(crate) struct StrokeStyle {
    pub width: f32,
    pub miter_limit: f32,
    pub cap: Cap,
    pub join: Join,
    pub res_scale: f32,
}

/// The source path's verbs as `SkPath::Iter` (no forced close) yields them:
/// a close whose contour ends away from its start first yields the line
/// back.
fn iter_verbs(segs: &[Seg]) -> Vec<Verb> {
    let f = |p: (f64, f64)| (p.0 as f32, p.1 as f32);
    let mut out = Vec::new();
    let mut start = (0.0, 0.0);
    let mut last = (0.0, 0.0);
    for s in segs {
        match *s {
            Seg::Move(p) => {
                start = f(p);
                last = start;
                out.push(Verb::Move(start));
            }
            Seg::Line(p) => {
                last = f(p);
                out.push(Verb::Line(last));
            }
            Seg::Quad(c, p) => {
                last = f(p);
                out.push(Verb::Quad(f(c), last));
            }
            Seg::Conic(c, p, w) => {
                last = f(p);
                out.push(Verb::Conic(f(c), last, w as f32));
            }
            Seg::Cubic(a, b, p) => {
                last = f(p);
                out.push(Verb::Cubic(f(a), f(b), last));
            }
            Seg::Close => {
                if last != start {
                    out.push(Verb::Line(start));
                }
                out.push(Verb::Close);
                last = start;
            }
        }
    }
    out
}

/// `SkPath::isRect` with its closed flag and direction (clockwise).
fn rect_of(verbs: &[Verb]) -> Option<([f32; 4], bool, bool)> {
    let segs: Vec<Seg> = verbs
        .iter()
        .map(|v| {
            let p = |a: V| (f64::from(a.0), f64::from(a.1));
            match *v {
                Verb::Move(a) => Seg::Move(p(a)),
                Verb::Line(a) => Seg::Line(p(a)),
                Verb::Quad(a, b) => Seg::Quad(p(a), p(b)),
                Verb::Conic(a, b, w) => Seg::Conic(p(a), p(b), f64::from(w)),
                Verb::Cubic(a, b, c) => Seg::Cubic(p(a), p(b), p(c)),
                Verb::Close => Seg::Close,
            }
        })
        .collect();
    crate::aaa::rect_contour(&segs)
}

/// `SkStroke::strokeRect` (not filled).
fn stroke_rect(rect: [f32; 4], mut cw: bool, style: &StrokeStyle) -> Builder {
    let mut dst = Builder::default();
    let radius = style.width / 2.0;
    if radius <= 0.0 {
        return dst;
    }
    let (rw, rh) = (rect[2] - rect[0], rect[3] - rect[1]);
    if (rw < 0.0) ^ (rh < 0.0) {
        cw = !cw;
    }
    let rect = [
        rect[0].min(rect[2]),
        rect[1].min(rect[3]),
        rect[0].max(rect[2]),
        rect[1].max(rect[3]),
    ];
    let (rw, rh) = (rect[2] - rect[0], rect[3] - rect[1]);
    let r = [
        rect[0] - radius,
        rect[1] - radius,
        rect[2] + radius,
        rect[3] + radius,
    ];
    let mut join = style.join;
    if join == Join::Miter && style.miter_limit < std::f32::consts::SQRT_2 {
        join = Join::Bevel;
    }
    match join {
        Join::Miter => dst.add_rect(r, cw, 0),
        Join::Bevel => {
            let (a, o) = (rect, r);
            let pts = if cw {
                [
                    (a[0], o[1]),
                    (a[2], o[1]),
                    (o[2], a[1]),
                    (o[2], a[3]),
                    (a[2], o[3]),
                    (a[0], o[3]),
                    (o[0], a[3]),
                    (o[0], a[1]),
                ]
            } else {
                [
                    (o[0], a[1]),
                    (o[0], a[3]),
                    (a[0], o[3]),
                    (a[2], o[3]),
                    (o[2], a[3]),
                    (o[2], a[1]),
                    (a[2], o[1]),
                    (a[0], o[1]),
                ]
            };
            dst.add_polygon(&pts);
        }
        Join::Round => dst.add_rrect(r, radius, cw),
    }
    if style.width < rw.min(rh) {
        let inner = [
            rect[0] + radius,
            rect[1] + radius,
            rect[2] - radius,
            rect[3] - radius,
        ];
        dst.add_rect(inner, !cw, 0);
    }
    dst
}

/// `SkStroke::strokePath`: the outline of `src` (user space) stroked with
/// `style`, as a fill path.
pub(crate) fn stroke_path(src: &[Seg], style: &StrokeStyle) -> Vec<Seg> {
    let radius = style.width / 2.0;
    if radius <= 0.0 {
        return Vec::new();
    }
    let verbs = iter_verbs(src);
    if let Some((rect, closed, cw)) = rect_of(&verbs) {
        if closed {
            return stroke_rect(rect, cw, style).into_segs();
        }
    }
    let mut stroker = Stroker::new(
        radius,
        style.miter_limit,
        style.cap,
        style.join,
        style.res_scale,
    );
    let mut last_is_line = false;
    let mut prev = (0.0f32, 0.0f32);
    for (i, v) in verbs.iter().enumerate() {
        match *v {
            Verb::Move(p) => {
                stroker.move_to(p);
                prev = p;
            }
            Verb::Line(p) => {
                // has_valid_tangent: a later segment of this contour with a
                // direction.
                let mut later = prev;
                let mut valid = false;
                for w in &verbs[i + 1..] {
                    match *w {
                        Verb::Move(_) | Verb::Close => break,
                        Verb::Line(q) => {
                            if later != q {
                                valid = true;
                                break;
                            }
                            later = q;
                        }
                        Verb::Quad(a, q) | Verb::Conic(a, q, _) => {
                            if !(later == a && later == q) {
                                valid = true;
                                break;
                            }
                            later = q;
                        }
                        Verb::Cubic(a, b, q) => {
                            if !(later == a && later == b && later == q) {
                                valid = true;
                                break;
                            }
                            later = q;
                        }
                    }
                }
                let _ = later;
                stroker.line_to(p, Some(valid));
                last_is_line = true;
                prev = p;
            }
            Verb::Quad(a, p) => {
                stroker.quad_to(a, p);
                last_is_line = false;
                prev = p;
            }
            Verb::Conic(a, p, w) => {
                stroker.conic_to(a, p, w);
                last_is_line = false;
                prev = p;
            }
            Verb::Cubic(a, b, p) => {
                stroker.cubic_to(a, b, p);
                last_is_line = false;
                prev = p;
            }
            Verb::Close => {
                if style.cap != Cap::Butt {
                    if stroker.has_only_move_to() {
                        let p = stroker.first_pt;
                        stroker.line_to(p, None);
                        last_is_line = true;
                        continue;
                    }
                    if stroker.is_current_contour_empty() {
                        last_is_line = true;
                        continue;
                    }
                }
                stroker.finish_contour(true, last_is_line);
            }
        }
    }
    stroker.finish_contour(false, last_is_line);
    let _ = TANGENT_LIMIT;
    std::mem::take(&mut stroker.outer).into_segs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(width: f32, cap: Cap, join: Join) -> StrokeStyle {
        StrokeStyle {
            width,
            miter_limit: 10.0,
            cap,
            join,
            res_scale: 1.0,
        }
    }

    fn conics(segs: &[Seg]) -> usize {
        segs.iter().filter(|s| matches!(s, Seg::Conic(..))).count()
    }

    #[test]
    fn round_caps_are_two_quarter_conics_each() {
        let line = [Seg::Move((4.0, 18.0)), Seg::Line((12.0, 18.0))];
        let out = stroke_path(&line, &style(4.0, Cap::Round, Join::Miter));
        assert_eq!(
            out,
            vec![
                Seg::Move((4.0, 16.0)),
                Seg::Line((12.0, 16.0)),
                Seg::Conic((14.0, 16.0), (14.0, 18.0), f64::from(FRAC_1_SQRT_2)),
                Seg::Conic((14.0, 20.0), (12.0, 20.0), f64::from(FRAC_1_SQRT_2)),
                Seg::Line((4.0, 20.0)),
                Seg::Conic((2.0, 20.0), (2.0, 18.0), f64::from(FRAC_1_SQRT_2)),
                Seg::Conic((2.0, 16.0), (4.0, 16.0), f64::from(FRAC_1_SQRT_2)),
                Seg::Close,
            ]
        );
    }

    #[test]
    fn butt_and_square_caps() {
        let line = [Seg::Move((0.0, 0.0)), Seg::Line((10.0, 0.0))];
        let butt = stroke_path(&line, &style(2.0, Cap::Butt, Join::Miter));
        assert_eq!(
            butt,
            vec![
                Seg::Move((0.0, -1.0)),
                Seg::Line((10.0, -1.0)),
                Seg::Line((10.0, 1.0)),
                Seg::Line((0.0, 1.0)),
                Seg::Line((0.0, -1.0)),
                Seg::Close,
            ]
        );
        let square = stroke_path(&line, &style(2.0, Cap::Square, Join::Miter));
        assert!(square.contains(&Seg::Line((11.0, 1.0))));
        assert!(square.contains(&Seg::Line((-1.0, -1.0))));
    }

    #[test]
    fn a_zero_length_round_capped_line_is_a_dot() {
        let dot = [Seg::Move((5.0, 5.0)), Seg::Line((5.0, 5.0))];
        assert_eq!(
            conics(&stroke_path(&dot, &style(4.0, Cap::Round, Join::Round))),
            4
        );
        assert!(stroke_path(&dot, &style(4.0, Cap::Butt, Join::Round)).is_empty());
    }

    #[test]
    fn joins() {
        let corner = [
            Seg::Move((0.0, 0.0)),
            Seg::Line((10.0, 0.0)),
            Seg::Line((10.0, 10.0)),
        ];
        // A right angle mitered: the outer corner at (11, -1).
        let miter = stroke_path(&corner, &style(2.0, Cap::Butt, Join::Miter));
        assert!(miter.contains(&Seg::Line((11.0, -1.0))), "{miter:?}");
        // Round: a quarter conic about the corner.
        let round = stroke_path(&corner, &style(2.0, Cap::Butt, Join::Round));
        assert_eq!(conics(&round), 1);
        // Bevel: a straight line between the offsets.
        let bevel = stroke_path(&corner, &style(2.0, Cap::Butt, Join::Bevel));
        assert!(bevel.contains(&Seg::Line((11.0, 0.0))), "{bevel:?}");
        // A turn sharper than the miter limit is beveled.
        let sharp = [
            Seg::Move((0.0, 0.0)),
            Seg::Line((20.0, 0.0)),
            Seg::Line((0.0, 1.0)),
        ];
        let out = stroke_path(&sharp, &style(2.0, Cap::Butt, Join::Miter));
        assert!(out.iter().all(|s| match s {
            Seg::Line(p) | Seg::Move(p) => p.0 < 21.0,
            _ => true,
        }));
    }

    #[test]
    fn closed_rectangles_use_stroke_rect() {
        let rect = [
            Seg::Move((0.0, 0.0)),
            Seg::Line((10.0, 0.0)),
            Seg::Line((10.0, 6.0)),
            Seg::Line((0.0, 6.0)),
            Seg::Close,
        ];
        // Miter: the outer rectangle, then the inner one reversed.
        let out = stroke_path(&rect, &style(2.0, Cap::Butt, Join::Miter));
        assert_eq!(out[0], Seg::Move((-1.0, -1.0)));
        assert_eq!(out.iter().filter(|s| **s == Seg::Close).count(), 2);
        // Round: a rounded rectangle, four corner conics.
        let out = stroke_path(&rect, &style(2.0, Cap::Butt, Join::Round));
        assert_eq!(conics(&out), 4);
    }

    #[test]
    fn curves_are_offset_by_quadratics() {
        let arc = [
            Seg::Move((10.0, 0.0)),
            Seg::Conic((10.0, 10.0), (0.0, 10.0), f64::from(FRAC_1_SQRT_2)),
        ];
        let out = stroke_path(&arc, &style(2.0, Cap::Butt, Join::Round));
        assert!(out.iter().any(|s| matches!(s, Seg::Quad(..))));
        let cubic = [
            Seg::Move((0.0, 0.0)),
            Seg::Cubic((10.0, 20.0), (30.0, -20.0), (40.0, 0.0)),
        ];
        let out = stroke_path(&cubic, &style(3.0, Cap::Round, Join::Round));
        assert!(out.iter().filter(|s| matches!(s, Seg::Quad(..))).count() >= 4);
    }
}
