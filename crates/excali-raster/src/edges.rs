//! Paths as Chrome hands them to Skia, and the curve geometry Skia's edge
//! builder applies before scan conversion.
//!
//! - Blink builds `arc()` from conics (`CanvasPath::arc`, then
//!   `PathBuilder::AddEllipse`, `SkPathBuilder::arcTo`,
//!   `SkConic::BuildUnitArc`), all in `f32`; a path that is one `arc()`
//!   and nothing else is filled as `drawArc` (`blink_arc_fill`).
//! - A path reaches Skia in user space as floats and is mapped by the
//!   canvas matrix as `SkMatrix::mapPoints` does (`transform_segs`).
//! - The edge builder turns conics into quadratics within 1/4 pixel
//!   (`SkAutoConicToQuads`: `computeQuadPOW2`, `chopIntoQuadsPOW2`) and
//!   cuts quadratics and cubics where they turn in y or x
//!   (`SkChopQuadAtYExtrema`, `SkChopCubicAtYExtrema` and their x
//!   counterparts for the edge clipper).
//!
//! Points are stored as `f64` pairs holding `f32` values; every computation
//! is done in `f32` with Skia's operation order (Skia chrome/m153,
//! f8b66b7597c4cc859d3ed190e9c6872241e6721c, `SkGeometry.cpp`,
//! `SkPathBuilder.cpp`, `SkMatrix.cpp`; Blink `canvas_path.cc`,
//! `path_builder.cc`), so the scan converter sees Chrome's coordinates.

use excali_scene::display::{Path, PathCommand, Transform};

pub(crate) type P = (f64, f64);

/// A path segment, as Skia's path iterator yields it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Seg {
    Move(P),
    Line(P),
    Quad(P, P),
    /// Control point, end point, weight.
    Conic(P, P, f64),
    Cubic(P, P, P),
    Close,
}

type F = (f32, f32);

fn f(p: P) -> F {
    (p.0 as f32, p.1 as f32)
}

fn d(p: F) -> P {
    (f64::from(p.0), f64::from(p.1))
}

fn fadd(a: F, b: F) -> F {
    (a.0 + b.0, a.1 + b.1)
}

fn fsub(a: F, b: F) -> F {
    (a.0 - b.0, a.1 - b.1)
}

fn fmul(a: F, s: f32) -> F {
    (a.0 * s, a.1 * s)
}

/// `SK_ScalarNearlyZero`.
const NEARLY_ZERO: f32 = 1.0 / 4096.0;
const ROOT2_OVER2: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// Blink's `kTwoPiFloat`.
const TWO_PI: f32 = std::f32::consts::TAU;

fn nearly_equal(a: f32, b: f32) -> bool {
    (a - b).abs() <= NEARLY_ZERO
}

// ---------------------------------------------------------------------------
// Matrices (SkMatrix::mapPoints)

/// The canvas matrix as the `SkMatrix` Chrome draws with.
#[derive(Clone, Copy)]
struct Matrix {
    sx: f32,
    ky: f32,
    kx: f32,
    sy: f32,
    tx: f32,
    ty: f32,
}

impl Matrix {
    fn new(t: &Transform) -> Self {
        Matrix {
            sx: t.a as f32,
            ky: t.b as f32,
            kx: t.c as f32,
            sy: t.d as f32,
            tx: t.e as f32,
            ty: t.f as f32,
        }
    }

    /// The map proc for the matrix's type: identity, translate,
    /// scale-translate or affine.
    fn map(&self, p: F) -> F {
        let affine = self.kx != 0.0 || self.ky != 0.0;
        if affine {
            (
                p.0 * self.sx + p.1 * self.kx + self.tx,
                p.0 * self.ky + p.1 * self.sy + self.ty,
            )
        } else if self.sx != 1.0 || self.sy != 1.0 {
            (p.0 * self.sx + self.tx, p.1 * self.sy + self.ty)
        } else if self.tx != 0.0 || self.ty != 0.0 {
            (p.0 + self.tx, p.1 + self.ty)
        } else {
            p
        }
    }
}

/// `segs` under `t`, as `SkPath::transform` maps a path (every point; conic
/// weights unchanged).
pub(crate) fn transform_segs(segs: &[Seg], t: &Transform) -> Vec<Seg> {
    let m = Matrix::new(t);
    let a = |p: P| d(m.map(f(p)));
    segs.iter()
        .map(|s| match *s {
            Seg::Move(p) => Seg::Move(a(p)),
            Seg::Line(p) => Seg::Line(a(p)),
            Seg::Quad(c, p) => Seg::Quad(a(c), a(p)),
            Seg::Conic(c, p, w) => Seg::Conic(a(c), a(p), w),
            Seg::Cubic(c1, c2, p) => Seg::Cubic(a(c1), a(c2), a(p)),
            Seg::Close => Seg::Close,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Arcs (Blink canvas_path.cc, path_builder.cc; SkPathBuilder::arcTo)

/// The start of an arc and what follows it: conics (control, end,
/// weight), or a single point to line to.
struct ArcTo {
    start: F,
    conics: Vec<(F, F, f32)>,
}

fn sin_snap(r: f32) -> f32 {
    let v = r.sin();
    if v.abs() <= NEARLY_ZERO {
        0.0
    } else {
        v
    }
}

fn cos_snap(r: f32) -> f32 {
    let v = r.cos();
    if v.abs() <= NEARLY_ZERO {
        0.0
    } else {
        v
    }
}

fn deg_to_rad(deg: f32) -> f32 {
    deg * (std::f32::consts::PI / 180.0)
}

/// `sk_float_midpoint`.
fn midpoint(a: f32, b: f32) -> f32 {
    (0.5 * (f64::from(a) + f64::from(b))) as f32
}

/// `SkConic::BuildUnitArc` under `matrix` (Skia builds the rotation, the
/// anticlockwise flip and the oval's scale and translation into one
/// matrix, entries rounded to `f32`).
fn build_unit_arc(u_start: F, u_stop: F, cw: bool, user: Matrix) -> Vec<(F, F, f32)> {
    let x = u_start.0 * u_stop.0 + u_start.1 * u_stop.1;
    let mut y = u_start.0 * u_stop.1 - u_start.1 * u_stop.0;
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
    const QUADRANT: [F; 8] = [
        (1.0, 0.0),
        (1.0, 1.0),
        (0.0, 1.0),
        (-1.0, 1.0),
        (-1.0, 0.0),
        (-1.0, -1.0),
        (0.0, -1.0),
        (1.0, -1.0),
    ];
    let mut conics: Vec<([F; 3], f32)> = (0..quadrant)
        .map(|i| {
            (
                [
                    QUADRANT[i * 2],
                    QUADRANT[i * 2 + 1],
                    QUADRANT[(i * 2 + 2) % 8],
                ],
                ROOT2_OVER2,
            )
        })
        .collect();
    let final_p = (x, y);
    let last_q = QUADRANT[(quadrant * 2) % 8];
    let dot = last_q.0 * final_p.0 + last_q.1 * final_p.1;
    if dot.is_nan() {
        return Vec::new();
    }
    if dot < 1.0 {
        let off = fadd(last_q, final_p);
        let cos_half = ((1.0 + dot) / 2.0).sqrt();
        // offCurve.setLength(1 / cosThetaOver2), in doubles.
        let (ox, oy) = (f64::from(off.0), f64::from(off.1));
        let scale = f64::from(1.0 / cos_half) / (ox * ox + oy * oy).sqrt();
        let off = ((ox * scale) as f32, (oy * scale) as f32);
        let within =
            (last_q.0 - off.0).abs() <= NEARLY_ZERO && (last_q.1 - off.1).abs() <= NEARLY_ZERO;
        if !within {
            conics.push(([last_q, off, final_p], cos_half));
        }
    }
    // matrix.setSinCos(uStart.y, uStart.x); preScale(1, -1) when
    // anticlockwise; postConcat(user).
    let (sin, cos) = (u_start.1, u_start.0);
    let (r00, r01, r10, r11) = if cw {
        (cos, -sin, sin, cos)
    } else {
        (cos, sin, sin, -cos)
    };
    let m = Matrix {
        sx: user.sx * r00,
        kx: user.sx * r01,
        ky: user.sy * r10,
        sy: user.sy * r11,
        tx: user.tx,
        ty: user.ty,
    };
    let map = |p: F| {
        (
            p.0 * m.sx + p.1 * m.kx + m.tx,
            p.0 * m.ky + p.1 * m.sy + m.ty,
        )
    };
    conics
        .into_iter()
        .map(|(p, w)| (map(p[1]), map(p[2]), w))
        .collect()
}

/// `SkPathBuilder::arcTo(oval, startAngle, sweepAngle, forceMoveTo)` for
/// the arc's own points: where it starts and what it draws.
fn sk_arc_to(oval: [f32; 4], start_deg: f32, sweep_deg: f32) -> ArcTo {
    let start_deg = start_deg % 360.0;
    let half_w = (oval[2] - oval[0]) * 0.5;
    let half_h = (oval[3] - oval[1]) * 0.5;
    let (cx, cy) = (midpoint(oval[0], oval[2]), midpoint(oval[1], oval[3]));
    // arc_is_lone_point.
    if sweep_deg == 0.0 && (start_deg == 0.0 || start_deg == 360.0) {
        return ArcTo {
            start: (oval[2], cy),
            conics: Vec::new(),
        };
    }
    if oval[2] - oval[0] == 0.0 && oval[3] - oval[1] == 0.0 {
        return ArcTo {
            start: (oval[2], oval[1]),
            conics: Vec::new(),
        };
    }
    // angles_to_unit_vectors.
    let start_rad = deg_to_rad(start_deg);
    let mut stop_rad = deg_to_rad(start_deg + sweep_deg);
    let start_v = (cos_snap(start_rad), sin_snap(start_rad));
    let mut stop_v = (cos_snap(stop_rad), sin_snap(stop_rad));
    if start_v == stop_v {
        let sw = sweep_deg.abs();
        if sw < 360.0 && sw > 359.0 {
            let delta = (1.0f32 / 512.0).copysign(sweep_deg);
            while start_v == stop_v {
                stop_rad -= delta;
                stop_v = (cos_snap(stop_rad), sin_snap(stop_rad));
            }
        }
    }
    let cw = sweep_deg > 0.0;
    if start_v == stop_v {
        let end = deg_to_rad(start_deg + sweep_deg);
        return ArcTo {
            start: (cx + half_w * end.cos(), cy + half_h * end.sin()),
            conics: Vec::new(),
        };
    }
    let user = Matrix {
        sx: half_w,
        ky: 0.0,
        kx: 0.0,
        sy: half_h,
        tx: cx,
        ty: cy,
    };
    let conics = build_unit_arc(start_v, stop_v, cw, user);
    if conics.is_empty() {
        let single = (stop_v.0 * half_w + cx, stop_v.1 * half_h + cy);
        return ArcTo {
            start: single,
            conics,
        };
    }
    // The first conic's start: the quadrant point (1, 0) under the
    // combined matrix, which is the start vector on the oval.
    let (sin, cos) = (start_v.1, start_v.0);
    let (r00, r10) = (cos, sin);
    let start = (half_w * r00 + cx, half_h * r10 + cy);
    ArcTo { start, conics }
}

/// Blink's `arc()` after its checks, for an arc of `sweep` radians from
/// `start` (`CanonicalizeAngle`, `AdjustEndAngle`, `AddEllipse`): the
/// start point and the conics, a whole turn as two half turns.
fn blink_arc(cx: f64, cy: f64, radius: f64, start: f64, sweep: f64) -> ArcTo {
    let (x, y, r) = (cx as f32, cy as f32, radius as f32);
    let start_f = start as f32;
    let end_f = (start + sweep) as f32;
    let anticlockwise = sweep < 0.0;
    // CanonicalizeAngle.
    let mut s = start_f % TWO_PI;
    if s < 0.0 {
        s += TWO_PI;
        if s >= TWO_PI {
            s -= TWO_PI;
        }
    }
    let e = end_f + (s - start_f);
    // AdjustEndAngle.
    let e = if !anticlockwise && e - s >= TWO_PI {
        s + TWO_PI
    } else if anticlockwise && s - e >= TWO_PI {
        s - TWO_PI
    } else if !anticlockwise && s > e {
        s + (TWO_PI - (s - e) % TWO_PI)
    } else if anticlockwise && s < e {
        s - (TWO_PI - (e - s) % TWO_PI)
    } else {
        e
    };
    // AddEllipse.
    let oval = [x - r, y - r, x + r, y + r];
    let rad_to_deg = (180.0 / std::f64::consts::PI) as f32;
    let start_deg = s * rad_to_deg;
    let sweep_deg = (e - s) * rad_to_deg;
    if nearly_equal(sweep_deg.abs(), 360.0) {
        let half = 180.0f32.copysign(sweep_deg);
        let first = sk_arc_to(oval, start_deg, half);
        let second = sk_arc_to(oval, start_deg + half, half);
        let mut conics = first.conics;
        conics.extend(second.conics);
        ArcTo {
            start: first.start,
            conics,
        }
    } else {
        sk_arc_to(oval, start_deg, sweep_deg)
    }
}

// ---------------------------------------------------------------------------
// Display paths

fn end_point(s: &Seg) -> Option<P> {
    match *s {
        Seg::Move(p)
        | Seg::Line(p)
        | Seg::Quad(_, p)
        | Seg::Conic(_, p, _)
        | Seg::Cubic(_, _, p) => Some(p),
        Seg::Close => None,
    }
}

/// The path Blink builds from the display path's canvas calls, in device
/// space under `t`: the canvas's rules applied (`Path::canonical_arcs`),
/// each arc as Skia's conics with its start where Skia puts it (a line to
/// it only when the last point is not already there, as `arcTo`'s
/// `addPt`).
pub(crate) fn from_display(path: &Path, t: &Transform) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::new();
    let p = |x: f64, y: f64| d(f((x, y)));
    for command in path.canonical_arcs().commands {
        match command {
            PathCommand::MoveTo(x, y) => out.push(Seg::Move(p(x, y))),
            PathCommand::LineTo(x, y) => out.push(Seg::Line(p(x, y))),
            PathCommand::QuadTo(cx, cy, x, y) => out.push(Seg::Quad(p(cx, cy), p(x, y))),
            PathCommand::CubicTo(ax, ay, bx, by, x, y) => {
                out.push(Seg::Cubic(p(ax, ay), p(bx, by), p(x, y)))
            }
            PathCommand::Arc {
                cx,
                cy,
                radius,
                start,
                end,
                ..
            } => {
                let arc = blink_arc(cx, cy, radius, start, end - start);
                let start_pt = d(arc.start);
                // canonical_arcs put a move or a line to the arc's start
                // just before it.
                match out.last().copied() {
                    Some(Seg::Move(_)) => {
                        let n = out.len();
                        out[n - 1] = Seg::Move(start_pt);
                    }
                    Some(Seg::Line(_)) => {
                        let n = out.len();
                        let before = if n >= 2 { end_point(&out[n - 2]) } else { None };
                        let near = before.is_some_and(|b| {
                            let (b, s) = (f(b), arc.start);
                            nearly_equal(b.0, s.0) && nearly_equal(b.1, s.1)
                        });
                        if near {
                            out.pop();
                        } else {
                            out[n - 1] = Seg::Line(start_pt);
                        }
                    }
                    _ => out.push(Seg::Move(start_pt)),
                }
                for (c, e, w) in arc.conics {
                    out.push(Seg::Conic(d(c), d(e), f64::from(w)));
                }
            }
            PathCommand::Close => out.push(Seg::Close),
        }
    }
    // Blink's rect() ends its contour with a close; the display path's
    // rect() also moves back to the corner, which a canvas path only
    // records when a drawing call follows (SkPathBuilder injects it).
    if let [.., Seg::Close, Seg::Move(_)] = out.as_slice() {
        out.pop();
    }
    transform_segs(&out, t)
}

/// Blink's arc fast path for fills: a path whose only call is one `arc()`
/// (optionally closed) with a radius of 1 or more is drawn with
/// `drawArc` (`CanvasPath::arc`'s `arc_builder_`,
/// `Canvas2DRecorderContext::DrawPathInternal`), and Skia fills a sweep of
/// a whole turn as `addOval` (`SkPathPriv::CreateDrawArcPath`): four
/// quarter conics clockwise from the rightmost point, whatever angle the
/// arc started at. `None` for any other path, which fills as built.
pub(crate) fn blink_arc_fill(path: &Path, t: &Transform) -> Option<Vec<Seg>> {
    let arc = match path.commands.as_slice() {
        [a @ PathCommand::Arc { .. }] | [a @ PathCommand::Arc { .. }, PathCommand::Close] => *a,
        _ => return None,
    };
    let PathCommand::Arc {
        cx, cy, radius: r, ..
    } = arc
    else {
        return None;
    };
    if r.is_nan() || r < 1.0 {
        return None;
    }
    let sweep = path
        .canonical_arcs()
        .commands
        .iter()
        .find_map(|c| match *c {
            PathCommand::Arc { start, end, .. } => Some(end - start),
            _ => None,
        })?;
    if sweep.abs() < std::f64::consts::TAU {
        return None;
    }
    let (x, y, r) = (cx as f32, cy as f32, r as f32);
    let (l, tp, rt, b) = (x - r, y - r, x + r, y + r);
    let (mx, my) = (midpoint(l, rt), midpoint(tp, b));
    let q = |a: f32, c: f32| d((a, c));
    let segs = vec![
        Seg::Move(q(rt, my)),
        Seg::Conic(q(rt, b), q(mx, b), f64::from(ROOT2_OVER2)),
        Seg::Conic(q(l, b), q(l, my), f64::from(ROOT2_OVER2)),
        Seg::Conic(q(l, tp), q(mx, tp), f64::from(ROOT2_OVER2)),
        Seg::Conic(q(rt, tp), q(rt, my), f64::from(ROOT2_OVER2)),
        Seg::Close,
    ];
    Some(transform_segs(&segs, t))
}

// ---------------------------------------------------------------------------
// Conics to quadratics (SkConic::computeQuadPOW2, chop, chopIntoQuadsPOW2)

/// `kMaxConicToQuadPOW2`.
const MAX_CONIC_TO_QUAD_POW2: u32 = 5;

fn conic_quad_pow2(p: [F; 3], w: f32, tol: f32) -> u32 {
    if w < 0.0 || !w.is_finite() {
        return 0;
    }
    let a = w - 1.0;
    let k = a / (4.0 * (2.0 + a));
    let x = k * (p[0].0 - 2.0 * p[1].0 + p[2].0);
    let y = k * (p[0].1 - 2.0 * p[1].1 + p[2].1);
    let mut error = (x * x + y * y).sqrt();
    let mut pow2 = 0;
    while pow2 < MAX_CONIC_TO_QUAD_POW2 {
        if error <= tol {
            break;
        }
        error *= 0.25;
        pow2 += 1;
    }
    pow2
}

/// `SkConic::chop` (the current, non-legacy version).
fn chop_conic(p: [F; 3], w: f32) -> ([F; 3], [F; 3], f32) {
    let scale = 1.0 / (1.0 + w);
    let t0 = fmul(p[0], scale);
    let t1 = fmul(p[1], w * scale);
    let t2 = fmul(p[2], scale);
    let p1 = fadd(t0, t1);
    let p3 = fadd(t1, t2);
    let p2 = fadd(fadd(fmul(t0, 0.5), t1), fmul(t2, 0.5));
    let nw = (0.5 + w * 0.5).sqrt();
    ([p[0], p1, p2], [p2, p3, p[2]], nw)
}

fn between(a: f32, b: f32, c: f32) -> bool {
    (a - b) * (c - b) <= 0.0
}

/// `subdivide`: the quads of a conic at `level`, with the y-order fixes
/// Skia applies so a monotonic conic stays monotonic.
fn subdivide(p: [F; 3], w: f32, level: u32, out: &mut Vec<F>) {
    if level == 0 {
        out.push(p[1]);
        out.push(p[2]);
        return;
    }
    let (mut a, mut b, nw) = chop_conic(p, w);
    let (start_y, end_y) = (p[0].1, p[2].1);
    if between(start_y, p[1].1, end_y) {
        let mid_y = a[2].1;
        if !between(start_y, mid_y, end_y) {
            let closer = if (mid_y - start_y).abs() < (mid_y - end_y).abs() {
                start_y
            } else {
                end_y
            };
            a[2].1 = closer;
            b[0].1 = closer;
        }
        if !between(start_y, a[1].1, a[2].1) {
            a[1].1 = start_y;
        }
        if !between(b[0].1, b[1].1, end_y) {
            b[1].1 = end_y;
        }
    }
    subdivide(a, nw, level - 1, out);
    subdivide(b, nw, level - 1, out);
}

/// The conic as the quadratics Skia draws it with (`SkAutoConicToQuads`
/// at a tolerance of 1/4).
pub(crate) fn conic_quads(p0: P, p1: P, p2: P, w: f64) -> Vec<(P, P, P)> {
    let p = [f(p0), f(p1), f(p2)];
    let w = w as f32;
    let mut pow2 = conic_quad_pow2(p, w, 0.25);
    if w < 0.0 || !w.is_finite() {
        pow2 = 0;
    }
    let mut pts = vec![p[0]];
    let mut special = false;
    if pow2 == MAX_CONIC_TO_QUAD_POW2 {
        let (a, b, _) = chop_conic(p, w);
        let within =
            |x: F, y: F| (x.0 - y.0).abs() <= NEARLY_ZERO && (x.1 - y.1).abs() <= NEARLY_ZERO;
        if within(a[1], a[2]) && within(b[0], b[1]) {
            pts.extend([a[1], a[1], a[1], b[2]]);
            pow2 = 1;
            special = true;
        }
    }
    if !special {
        subdivide(p, w, pow2, &mut pts);
    }
    if pts.iter().any(|q| !q.0.is_finite() || !q.1.is_finite()) {
        let n = pts.len();
        for q in pts.iter_mut().take(n - 1).skip(1) {
            *q = p[1];
        }
    }
    pts.windows(3)
        .step_by(2)
        .map(|q| (d(q[0]), d(q[1]), d(q[2])))
        .collect()
}

// ---------------------------------------------------------------------------
// Monotonic pieces (SkChopQuadAtYExtrema, SkChopCubicAtYExtrema, and the
// x versions the edge clipper uses)

fn valid_unit_divide(numer: f32, denom: f32) -> Option<f32> {
    let (mut n, mut dd) = (numer, denom);
    if n < 0.0 {
        n = -n;
        dd = -dd;
    }
    if dd == 0.0 || n == 0.0 || n >= dd {
        return None;
    }
    let r = n / dd;
    if r.is_nan() || r == 0.0 {
        return None;
    }
    Some(r)
}

/// `SkFindUnitQuadRoots` (the discriminant in doubles).
fn unit_quad_roots(a: f32, b: f32, c: f32) -> Vec<f32> {
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

fn interp(a: F, b: F, t: f32) -> F {
    fadd(a, fmul(fsub(b, a), t))
}

/// `SkChopQuadAt`.
fn chop_quad_f(p: [F; 3], t: f32) -> ([F; 3], [F; 3]) {
    let p01 = interp(p[0], p[1], t);
    let p12 = interp(p[1], p[2], t);
    let m = interp(p01, p12, t);
    ([p[0], p01, m], [m, p12, p[2]])
}

/// `SkChopQuadAt` on stored points.
pub(crate) fn chop_quad(p: [P; 3], t: f64) -> ([P; 3], [P; 3]) {
    let (a, b) = chop_quad_f(p.map(f), t as f32);
    (a.map(d), b.map(d))
}

/// `SkChopQuadAtYExtrema` (the y of the points; `x_axis` swaps roles for
/// `SkChopQuadAtXExtrema`).
fn quad_monotonic(p: [F; 3], x_axis: bool) -> Vec<[F; 3]> {
    let get = |q: F| if x_axis { q.0 } else { q.1 };
    let (a, mut b, c) = (get(p[0]), get(p[1]), get(p[2]));
    let ab = a - b;
    let bc = if ab < 0.0 { -(b - c) } else { b - c };
    if ab == 0.0 || bc < 0.0 {
        if let Some(t) = valid_unit_divide(a - b, a - b - b + c) {
            let (mut h, mut k) = chop_quad_f(p, t);
            // flatten_double_quad_extrema: dst[1] = dst[3] = dst[2].
            let m = get(h[2]);
            if x_axis {
                h[1].0 = m;
                k[1].0 = m;
            } else {
                h[1].1 = m;
                k[1].1 = m;
            }
            return vec![h, k];
        }
        b = if (a - b).abs() < (b - c).abs() { a } else { c };
    }
    let mut q = p;
    if x_axis {
        q[1].0 = b;
    } else {
        q[1].1 = b;
    }
    vec![q]
}

pub(crate) fn quad_monotonic_pieces(p0: P, p1: P, p2: P, out: &mut Vec<(P, P, P)>) {
    for q in quad_monotonic([f(p0), f(p1), f(p2)], false) {
        out.push((d(q[0]), d(q[1]), d(q[2])));
    }
}

/// `SkChopQuadAtXExtrema`.
pub(crate) fn quad_monotonic_pieces_x(p0: P, p1: P, p2: P, out: &mut Vec<(P, P, P)>) {
    for q in quad_monotonic([f(p0), f(p1), f(p2)], true) {
        out.push((d(q[0]), d(q[1]), d(q[2])));
    }
}

/// `SkChopCubicAt(src, dst[7], t)`.
fn chop_cubic_f(c: [F; 4], t: f32) -> ([F; 4], [F; 4]) {
    if t == 1.0 {
        return (c, [c[3], c[3], c[3], c[3]]);
    }
    let mix = |a: F, b: F| fadd(fmul(fsub(b, a), t), a);
    let ab = mix(c[0], c[1]);
    let bc = mix(c[1], c[2]);
    let cd = mix(c[2], c[3]);
    let abc = mix(ab, bc);
    let bcd = mix(bc, cd);
    let abcd = mix(abc, bcd);
    ([c[0], ab, abc, abcd], [abcd, bcd, cd, c[3]])
}

/// `SkChopCubicAt(src, dst[10], t0, t1)`: both chops of the original cubic
/// at once.
fn chop_cubic_2(c: [F; 4], t0: f32, t1: f32) -> [[F; 4]; 3] {
    if t1 == 1.0 {
        let (a, b) = chop_cubic_f(c, t0);
        return [a, b, [c[3], c[3], c[3], c[3]]];
    }
    let mix = |a: F, b: F, t: f32| fadd(fmul(fsub(b, a), t), a);
    let lane = |t: f32| {
        let ab = mix(c[0], c[1], t);
        let bc = mix(c[1], c[2], t);
        let cd = mix(c[2], c[3], t);
        let abc = mix(ab, bc, t);
        let bcd = mix(bc, cd, t);
        let abcd = mix(abc, bcd, t);
        (ab, bc, cd, abc, bcd, abcd)
    };
    let (ab0, _, _, abc0, bcd0, abcd0) = lane(t0);
    let (_, _, cd1, abc1, bcd1, abcd1) = lane(t1);
    let middle_lo = mix(abc0, bcd0, t1);
    let middle_hi = mix(abc1, bcd1, t0);
    [
        [c[0], ab0, abc0, abcd0],
        [abcd0, middle_lo, middle_hi, abcd1],
        [abcd1, bcd1, cd1, c[3]],
    ]
}

/// `SkChopCubicAt` on stored points.
pub(crate) fn chop_cubic_at(p: [P; 4], t: f64) -> ([P; 4], [P; 4]) {
    let (a, b) = chop_cubic_f(p.map(f), t as f32);
    (a.map(d), b.map(d))
}

/// `SkChopCubicAtYExtrema` (`x_axis` for `SkChopCubicAtXExtrema`).
fn cubic_monotonic(c: [F; 4], x_axis: bool) -> Vec<[F; 4]> {
    let get = |q: F| if x_axis { q.0 } else { q.1 };
    let (a, b, cc, dd) = (get(c[0]), get(c[1]), get(c[2]), get(c[3]));
    // SkFindCubicExtrema.
    let roots = unit_quad_roots(dd - a + 3.0 * (b - cc), 2.0 * (a - b - b + cc), b - a);
    let mut pieces: Vec<[F; 4]> = match roots.len() {
        0 => vec![c],
        1 => {
            let (h, k) = chop_cubic_f(c, roots[0]);
            vec![h, k]
        }
        _ => chop_cubic_2(c, roots[0], roots[1]).to_vec(),
    };
    // flatten_double_cubic_extrema.
    for i in 0..pieces.len() - 1 {
        let m = get(pieces[i][3]);
        if x_axis {
            pieces[i][2].0 = m;
            pieces[i + 1][1].0 = m;
        } else {
            pieces[i][2].1 = m;
            pieces[i + 1][1].1 = m;
        }
    }
    pieces
}

pub(crate) fn cubic_monotonic_pieces(p: [P; 4], out: &mut Vec<[P; 4]>) {
    out.extend(
        cubic_monotonic(p.map(f), false)
            .into_iter()
            .map(|c| c.map(d)),
    );
}

/// `SkChopCubicAtXExtrema`.
pub(crate) fn cubic_monotonic_pieces_x(p: [P; 4], out: &mut Vec<[P; 4]>) {
    out.extend(
        cubic_monotonic(p.map(f), true)
            .into_iter()
            .map(|c| c.map(d)),
    );
}

/// The `t` in (0, 1) where the quadratic's coordinate (`c0, c1, c2`)
/// reaches `target` (`chopMonoQuadAt`).
pub(crate) fn mono_quad_t(c0: f64, c1: f64, c2: f64, target: f64) -> Option<f64> {
    let (c0, c1, c2, target) = (c0 as f32, c1 as f32, c2 as f32, target as f32);
    let a = c0 - c1 - c1 + c2;
    let b = 2.0 * (c1 - c0);
    let c = c0 - target;
    unit_quad_roots(a, b, c).first().map(|&t| f64::from(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: F, b: F) -> bool {
        (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4
    }

    #[test]
    fn a_quarter_turn_is_one_conic_of_weight_root_half() {
        let a = blink_arc(0.0, 0.0, 10.0, 0.0, std::f64::consts::FRAC_PI_2);
        assert!(close(a.start, (10.0, 0.0)));
        assert_eq!(a.conics.len(), 1);
        let (c, e, w) = a.conics[0];
        assert!(close(c, (10.0, 10.0)) && close(e, (0.0, 10.0)));
        assert_eq!(w, ROOT2_OVER2);
    }

    #[test]
    fn arcs_are_whole_quadrants_then_the_remainder() {
        // 1.9 rad: one quadrant and the remainder.
        let a = blink_arc(0.0, 0.0, 1.0, 0.0, 1.9);
        assert_eq!(a.conics.len(), 2);
        assert!(close(a.conics[1].1, (1.9f32.cos(), 1.9f32.sin())));
        assert!(
            (a.conics[1].2 - ((1.9f32 - std::f32::consts::FRAC_PI_2) / 2.0).cos()).abs() < 1e-5
        );
        // Anticlockwise by π: two quadrants through the top.
        let a = blink_arc(0.0, 0.0, 1.0, 0.0, -std::f64::consts::PI);
        assert_eq!(a.conics.len(), 2);
        assert!(close(a.conics[0].1, (0.0, -1.0)));
        assert!(close(a.conics[1].1, (-1.0, 0.0)));
        // A whole turn: two half turns, four conics.
        assert_eq!(
            blink_arc(5.0, 5.0, 2.0, 0.3, std::f64::consts::TAU)
                .conics
                .len(),
            4
        );
    }

    #[test]
    fn conics_become_quads_within_a_quarter_pixel() {
        // A quarter of a radius-12 circle: two quadratics (error 0.73 px,
        // then 0.18 px); radius 100: 2^3.
        let arc = |r: f64| {
            let a = blink_arc(0.0, 0.0, r, 0.0, std::f64::consts::FRAC_PI_2);
            (
                d(a.start),
                d(a.conics[0].0),
                d(a.conics[0].1),
                f64::from(a.conics[0].2),
            )
        };
        let (p0, c, p, w) = arc(12.0);
        assert_eq!(conic_quads(p0, c, p, w).len(), 2);
        let (p0, c, p, w) = arc(100.0);
        let quads = conic_quads(p0, c, p, w);
        assert_eq!(quads.len(), 8);
        assert_eq!(quads[7].2, p);
        assert_eq!(quads[0].0, p0);
    }

    #[test]
    fn curves_are_cut_where_they_turn() {
        let mut pieces = Vec::new();
        quad_monotonic_pieces((0.0, 0.0), (5.0, 10.0), (10.0, 0.0), &mut pieces);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].2, (5.0, 5.0));
        assert_eq!(pieces[0].1 .1, 5.0);
        let mut pieces = Vec::new();
        quad_monotonic_pieces_x((0.0, 0.0), (10.0, 5.0), (0.0, 10.0), &mut pieces);
        assert_eq!(pieces.len(), 2);
        let mut cubics = Vec::new();
        cubic_monotonic_pieces(
            [(0.0, 0.0), (0.0, 10.0), (10.0, -10.0), (10.0, 0.0)],
            &mut cubics,
        );
        assert_eq!(cubics.len(), 3);
        assert_eq!(cubics[0][3], cubics[1][0]);
        assert_eq!(cubics[1][3], cubics[2][0]);
    }

    #[test]
    fn matrices_map_as_skia() {
        let m = Matrix::new(&Transform::new(2.0, 0.0, 0.0, 0.5, 1.0, 3.0));
        assert_eq!(m.map((3.0, 4.0)), (7.0, 5.0));
        let m = Matrix::new(&Transform::rotate(std::f64::consts::FRAC_PI_2));
        let (x, y) = m.map((1.0, 0.0));
        assert!(x.abs() < 1e-7 && (y - 1.0).abs() < 1e-7);
    }
}
