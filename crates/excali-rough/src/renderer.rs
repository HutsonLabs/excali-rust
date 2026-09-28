//! rough.js 4.6.4 `bin/renderer.js`: the stroke primitives and the fills.
//!
//! Every function takes the resolved options and the RNG separately. In
//! rough.js the RNG lives on the options object (`o.randomizer`, created from
//! `o.seed` on the first draw) and every helper called with the same `o`
//! shares it, so one generator call draws from one sequence; the port passes
//! that sequence explicitly. The order of the draws is the contract: each
//! `rng` use below is in the order JavaScript evaluates the original
//! expression (left to right, arguments before the call).

use excali_math::js;

use crate::core::{Op, OpSet};
use crate::fillers;
use crate::hachure_fill::PolygonList;
use crate::path_data::{absolutize, normalize, parse_path, PathError};
use crate::{Options, Point, Random};

const PI: f64 = std::f64::consts::PI;

/// JavaScript `x || 0` for a number: 0 when `x` is 0 or NaN.
fn or_zero(x: f64) -> f64 {
    if x == 0.0 || x.is_nan() {
        0.0
    } else {
        x
    }
}

/// `line(x1, y1, x2, y2, o)`.
pub fn line(x1: f64, y1: f64, x2: f64, y2: f64, o: &Options, rng: &mut Random) -> OpSet {
    OpSet::path(double_line(x1, y1, x2, y2, o, rng, false))
}

/// `linearPath(points, close, o)`.
pub fn linear_path(points: &[Point], close: bool, o: &Options, rng: &mut Random) -> OpSet {
    let len = points.len();
    if len > 2 {
        let mut ops = Vec::new();
        for i in 0..len - 1 {
            let (a, b) = (points[i], points[i + 1]);
            ops.extend(double_line(a[0], a[1], b[0], b[1], o, rng, false));
        }
        if close {
            let (a, b) = (points[len - 1], points[0]);
            ops.extend(double_line(a[0], a[1], b[0], b[1], o, rng, false));
        }
        OpSet::path(ops)
    } else if len == 2 {
        line(
            points[0][0],
            points[0][1],
            points[1][0],
            points[1][1],
            o,
            rng,
        )
    } else {
        OpSet::path(Vec::new())
    }
}

/// `polygon(points, o)`.
pub fn polygon(points: &[Point], o: &Options, rng: &mut Random) -> OpSet {
    linear_path(points, true, o, rng)
}

/// `rectangle(x, y, width, height, o)`.
pub fn rectangle(x: f64, y: f64, width: f64, height: f64, o: &Options, rng: &mut Random) -> OpSet {
    let points = [
        [x, y],
        [x + width, y],
        [x + width, y + height],
        [x, y + height],
    ];
    polygon(&points, o, rng)
}

/// `curve(points, o)`: a Catmull-Rom curve through `points`, drawn twice
/// unless `disableMultiStroke`. The second stroke uses a fresh RNG seeded
/// with `seed + 1` (`cloneOptionsAlterSeed`), or `Math.random` for seed 0.
///
/// rough.js throws a TypeError for an empty list; the port returns an empty
/// path.
pub fn curve(points: &[Point], o: &Options, rng: &mut Random) -> OpSet {
    if points.is_empty() {
        return OpSet::path(Vec::new());
    }
    let mut o1 = curve_with_offset(points, 1.0 * (1.0 + o.roughness * 0.2), o, rng);
    if !o.disable_multi_stroke {
        // cloneOptionsAlterSeed: `if (ops.seed) result.seed = ops.seed + 1`,
        // and the clone gets its own randomizer. seed + 1 past 2^31 - 1 is
        // read by Math.imul as -2^31, which is what wrapping_add gives.
        let mut rng2 = Random::new(if o.seed != 0 {
            o.seed.wrapping_add(1)
        } else {
            0
        });
        let o2 = curve_with_offset(points, 1.5 * (1.0 + o.roughness * 0.22), o, &mut rng2);
        o1.extend(o2);
    }
    OpSet::path(o1)
}

/// `ellipse(x, y, width, height, o)`.
pub fn ellipse(x: f64, y: f64, width: f64, height: f64, o: &Options, rng: &mut Random) -> OpSet {
    let params = generate_ellipse_params(width, height, o, rng);
    ellipse_with_params(x, y, o, &params, rng).opset
}

/// `EllipseParams`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipseParams {
    pub increment: f64,
    pub rx: f64,
    pub ry: f64,
}

/// `EllipseResult`.
#[derive(Clone, Debug, PartialEq)]
pub struct EllipseResult {
    /// The points on the first stroke (the fill outline for pattern fills).
    pub estimated_points: Vec<Point>,
    pub opset: OpSet,
}

/// `generateEllipseParams(width, height, o)`.
pub fn generate_ellipse_params(
    width: f64,
    height: f64,
    o: &Options,
    rng: &mut Random,
) -> EllipseParams {
    let psq = (PI * 2.0 * (((width / 2.0).powi(2) + (height / 2.0).powi(2)) / 2.0).sqrt()).sqrt();
    let step_count = js::max(
        o.curve_step_count,
        (o.curve_step_count / 200f64.sqrt()) * psq,
    )
    .ceil();
    let increment = (PI * 2.0) / step_count;
    let mut rx = (width / 2.0).abs();
    let mut ry = (height / 2.0).abs();
    let curve_fit_randomness = 1.0 - o.curve_fitting;
    rx += offset_opt(rx * curve_fit_randomness, o, rng, 1.0);
    ry += offset_opt(ry * curve_fit_randomness, o, rng, 1.0);
    EllipseParams { increment, rx, ry }
}

/// `ellipseWithParams(x, y, o, ellipseParams)`.
pub fn ellipse_with_params(
    x: f64,
    y: f64,
    o: &Options,
    params: &EllipseParams,
    rng: &mut Random,
) -> EllipseResult {
    // _offset(0.1, _offset(0.4, 1, o), o): the inner call draws first
    let inner = offset(0.4, 1.0, o, rng, 1.0);
    let overlap = params.increment * offset(0.1, inner, o, rng, 1.0);
    let (ap1, cp1) = compute_ellipse_points(
        params.increment,
        x,
        y,
        params.rx,
        params.ry,
        1.0,
        overlap,
        o,
        rng,
    );
    let mut o1 = curve_ops(&ap1, o, rng);
    if !o.disable_multi_stroke && o.roughness != 0.0 {
        let (ap2, _) = compute_ellipse_points(
            params.increment,
            x,
            y,
            params.rx,
            params.ry,
            1.5,
            0.0,
            o,
            rng,
        );
        o1.extend(curve_ops(&ap2, o, rng));
    }
    EllipseResult {
        estimated_points: cp1,
        opset: OpSet::path(o1),
    }
}

/// `arc(x, y, width, height, start, stop, closed, roughClosure, o)`.
#[allow(clippy::too_many_arguments)]
pub fn arc(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    start: f64,
    stop: f64,
    closed: bool,
    rough_closure: bool,
    o: &Options,
    rng: &mut Random,
) -> OpSet {
    let cx = x;
    let cy = y;
    let mut rx = (width / 2.0).abs();
    let mut ry = (height / 2.0).abs();
    rx += offset_opt(rx * 0.01, o, rng, 1.0);
    ry += offset_opt(ry * 0.01, o, rng, 1.0);
    let mut strt = start;
    let mut stp = stop;
    while strt < 0.0 {
        strt += PI * 2.0;
        stp += PI * 2.0;
    }
    if (stp - strt) > (PI * 2.0) {
        strt = 0.0;
        stp = PI * 2.0;
    }
    let ellipse_inc = (PI * 2.0) / o.curve_step_count;
    let arc_inc = js::min(ellipse_inc / 2.0, (stp - strt) / 2.0);
    let mut ops = arc_ops(arc_inc, cx, cy, rx, ry, strt, stp, 1.0, o, rng);
    if !o.disable_multi_stroke {
        let o2 = arc_ops(arc_inc, cx, cy, rx, ry, strt, stp, 1.5, o, rng);
        ops.extend(o2);
    }
    if closed {
        if rough_closure {
            ops.extend(double_line(
                cx,
                cy,
                cx + rx * strt.cos(),
                cy + ry * strt.sin(),
                o,
                rng,
                false,
            ));
            ops.extend(double_line(
                cx,
                cy,
                cx + rx * stp.cos(),
                cy + ry * stp.sin(),
                o,
                rng,
                false,
            ));
        } else {
            ops.push(Op::LineTo([cx, cy]));
            ops.push(Op::LineTo([cx + rx * strt.cos(), cy + ry * strt.sin()]));
        }
    }
    OpSet::path(ops)
}

/// `svgPath(path, o)`: the path parsed and normalised to `M`, `L`, `C`, `Z`,
/// with every segment drawn rough.
pub fn svg_path(path: &str, o: &Options, rng: &mut Random) -> Result<OpSet, PathError> {
    let segments = normalize(&absolutize(&parse_path(path)?));
    let mut ops = Vec::new();
    let mut first = [0.0, 0.0];
    let mut current = [0.0, 0.0];
    for s in &segments {
        let data = &s.data;
        match s.key {
            'M' => {
                current = [data[0], data[1]];
                first = [data[0], data[1]];
            }
            'L' => {
                ops.extend(double_line(
                    current[0], current[1], data[0], data[1], o, rng, false,
                ));
                current = [data[0], data[1]];
            }
            'C' => {
                let (x1, y1, x2, y2, x, y) = (data[0], data[1], data[2], data[3], data[4], data[5]);
                ops.extend(bezier_to(x1, y1, x2, y2, x, y, current, o, rng));
                current = [x, y];
            }
            'Z' => {
                ops.extend(double_line(
                    current[0], current[1], first[0], first[1], o, rng, false,
                ));
                current = [first[0], first[1]];
            }
            _ => {}
        }
    }
    Ok(OpSet::path(ops))
}

/// `solidFillPolygon(polygonList, o)`: each polygon of more than two points
/// as a `fillPath` outline, every vertex jittered by up to
/// `maxRandomnessOffset`.
pub fn solid_fill_polygon(polygons: &[Vec<Point>], o: &Options, rng: &mut Random) -> OpSet {
    let mut ops = Vec::new();
    for points in polygons {
        // `o.maxRandomnessOffset || 0`
        let off = or_zero(o.max_randomness_offset);
        let len = points.len();
        if len > 2 {
            let x = points[0][0] + offset_opt(off, o, rng, 1.0);
            let y = points[0][1] + offset_opt(off, o, rng, 1.0);
            ops.push(Op::Move([x, y]));
            for p in &points[1..] {
                let x = p[0] + offset_opt(off, o, rng, 1.0);
                let y = p[1] + offset_opt(off, o, rng, 1.0);
                ops.push(Op::LineTo([x, y]));
            }
        }
    }
    OpSet::fill_path(ops)
}

/// `patternFillPolygons(polygonList, o)`: the `fillSketch` for
/// `o.fillStyle` (any style but `solid`; an unknown name is `hachure`).
///
/// As in rough.js, the fillers rotate the polygons by the hachure angle and
/// back in place, which moves the points in the last bits; `polygons` is
/// left as rough.js leaves its argument.
pub fn pattern_fill_polygons(polygons: &mut [Vec<Point>], o: &Options, rng: &mut Random) -> OpSet {
    let mut list = PolygonList::new(polygons);
    let set = pattern_fill_list(&mut list, o, rng);
    for (i, polygon) in polygons.iter_mut().enumerate() {
        *polygon = list.polygon(i);
    }
    set
}

/// [`pattern_fill_polygons`] on a [`PolygonList`].
pub(crate) fn pattern_fill_list(list: &mut PolygonList, o: &Options, rng: &mut Random) -> OpSet {
    fillers::fill_polygons(list, o, rng)
}

/// `patternFillArc(x, y, width, height, start, stop, o)`: the pattern fill
/// of the pie slice, from `curveStepCount` points on the arc and the centre.
///
/// With `start == stop` (or a step count that is not positive) rough.js's
/// point loop never ends; the port stops when the angle stops advancing.
#[allow(clippy::too_many_arguments)]
pub fn pattern_fill_arc(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    start: f64,
    stop: f64,
    o: &Options,
    rng: &mut Random,
) -> OpSet {
    let cx = x;
    let cy = y;
    let mut rx = (width / 2.0).abs();
    let mut ry = (height / 2.0).abs();
    rx += offset_opt(rx * 0.01, o, rng, 1.0);
    ry += offset_opt(ry * 0.01, o, rng, 1.0);
    let mut strt = start;
    let mut stp = stop;
    while strt < 0.0 {
        strt += PI * 2.0;
        stp += PI * 2.0;
    }
    if (stp - strt) > (PI * 2.0) {
        strt = 0.0;
        stp = PI * 2.0;
    }
    let increment = (stp - strt) / o.curve_step_count;
    let mut points: Vec<Point> = Vec::new();
    let mut angle = strt;
    while angle <= stp {
        points.push([cx + rx * angle.cos(), cy + ry * angle.sin()]);
        let next = angle + increment;
        if next <= angle || next.is_nan() {
            break;
        }
        angle = next;
    }
    points.push([cx + rx * stp.cos(), cy + ry * stp.sin()]);
    points.push([cx, cy]);
    pattern_fill_polygons(&mut [points], o, rng)
}

/// `randOffset(x, o)`: a draw in `[-x, x)` scaled by the roughness.
pub fn rand_offset(x: f64, o: &Options, rng: &mut Random) -> f64 {
    offset_opt(x, o, rng, 1.0)
}

/// `randOffsetWithRange(min, max, o)`.
pub fn rand_offset_with_range(min: f64, max: f64, o: &Options, rng: &mut Random) -> f64 {
    offset(min, max, o, rng, 1.0)
}

/// `doubleLineFillOps(x1, y1, x2, y2, o)`: a line drawn as fill strokes
/// (single when `disableMultiStrokeFill`).
pub fn double_line_fill_ops(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    o: &Options,
    rng: &mut Random,
) -> Vec<Op> {
    double_line(x1, y1, x2, y2, o, rng, true)
}

// -- private helpers ------------------------------------------------------------

/// `_offset(min, max, ops, roughnessGain)`. The draw happens whatever the
/// roughness, so a roughness of 0 still advances the sequence.
fn offset(min: f64, max: f64, o: &Options, rng: &mut Random, roughness_gain: f64) -> f64 {
    o.roughness * roughness_gain * ((rng.next() * (max - min)) + min)
}

/// `_offsetOpt(x, ops, roughnessGain)`.
fn offset_opt(x: f64, o: &Options, rng: &mut Random, roughness_gain: f64) -> f64 {
    offset(-x, x, o, rng, roughness_gain)
}

/// `_doubleLine(x1, y1, x2, y2, o, filling)`.
#[allow(clippy::too_many_arguments)]
fn double_line(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    o: &Options,
    rng: &mut Random,
    filling: bool,
) -> Vec<Op> {
    let single_stroke = if filling {
        o.disable_multi_stroke_fill
    } else {
        o.disable_multi_stroke
    };
    let mut o1 = line_ops(x1, y1, x2, y2, o, rng, true, false);
    if single_stroke {
        return o1;
    }
    let o2 = line_ops(x1, y1, x2, y2, o, rng, true, true);
    o1.extend(o2);
    o1
}

/// `_line(x1, y1, x2, y2, o, move, overlay)`.
#[allow(clippy::too_many_arguments)]
fn line_ops(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    o: &Options,
    rng: &mut Random,
    mv: bool,
    overlay: bool,
) -> Vec<Op> {
    let length_sq = (x1 - x2).powi(2) + (y1 - y2).powi(2);
    let length = length_sq.sqrt();
    let roughness_gain = if length < 200.0 {
        1.0
    } else if length > 500.0 {
        0.4
    } else {
        (-0.0016668) * length + 1.233334
    };
    let mut off = or_zero(o.max_randomness_offset);
    if (off * off * 100.0) > length_sq {
        off = length / 10.0;
    }
    let half_offset = off / 2.0;
    let diverge_point = 0.2 + rng.next() * 0.2;
    let mut mid_disp_x = o.bowing * o.max_randomness_offset * (y2 - y1) / 200.0;
    let mut mid_disp_y = o.bowing * o.max_randomness_offset * (x1 - x2) / 200.0;
    mid_disp_x = offset_opt(mid_disp_x, o, rng, roughness_gain);
    mid_disp_y = offset_opt(mid_disp_y, o, rng, roughness_gain);
    let mut ops = Vec::with_capacity(2);
    let preserve = o.preserve_vertices;
    // randomHalf / randomFull
    let rand = |rng: &mut Random, x: f64| offset_opt(x, o, rng, roughness_gain);
    let jitter = if overlay { half_offset } else { off };
    if mv {
        let dx = if preserve { 0.0 } else { rand(rng, jitter) };
        let dy = if preserve { 0.0 } else { rand(rng, jitter) };
        ops.push(Op::Move([x1 + dx, y1 + dy]));
    }
    let c1x = mid_disp_x + x1 + (x2 - x1) * diverge_point + rand(rng, jitter);
    let c1y = mid_disp_y + y1 + (y2 - y1) * diverge_point + rand(rng, jitter);
    let c2x = mid_disp_x + x1 + 2.0 * (x2 - x1) * diverge_point + rand(rng, jitter);
    let c2y = mid_disp_y + y1 + 2.0 * (y2 - y1) * diverge_point + rand(rng, jitter);
    let ex = x2 + if preserve { 0.0 } else { rand(rng, jitter) };
    let ey = y2 + if preserve { 0.0 } else { rand(rng, jitter) };
    ops.push(Op::BCurveTo([c1x, c1y, c2x, c2y, ex, ey]));
    ops
}

/// `_curveWithOffset(points, offset, o)`: the points jittered, with the first
/// and last doubled as Catmull-Rom end conditions.
fn curve_with_offset(points: &[Point], off: f64, o: &Options, rng: &mut Random) -> Vec<Op> {
    let mut ps: Vec<Point> = Vec::with_capacity(points.len() + 2);
    let jittered = |p: Point, rng: &mut Random| {
        let x = p[0] + offset_opt(off, o, rng, 1.0);
        let y = p[1] + offset_opt(off, o, rng, 1.0);
        [x, y]
    };
    ps.push(jittered(points[0], rng));
    ps.push(jittered(points[0], rng));
    for i in 1..points.len() {
        ps.push(jittered(points[i], rng));
        if i == points.len() - 1 {
            ps.push(jittered(points[i], rng));
        }
    }
    curve_ops(&ps, o, rng)
}

/// `_curve(points, null, o)`. `closePoint` is always `null` in rough.js
/// 4.6.4, so the port has no parameter for it.
fn curve_ops(points: &[Point], o: &Options, rng: &mut Random) -> Vec<Op> {
    match catmull_rom(points, o) {
        Some(ops) => ops,
        None if points.len() == 2 => double_line(
            points[0][0],
            points[0][1],
            points[1][0],
            points[1][1],
            o,
            rng,
            false,
        ),
        None => Vec::new(),
    }
}

/// The deterministic branches of `_curve` (more than two points); `None`
/// for two points or fewer.
fn catmull_rom(points: &[Point], o: &Options) -> Option<Vec<Op>> {
    let len = points.len();
    if len > 3 {
        let mut ops = Vec::with_capacity(len - 2);
        let s = 1.0 - o.curve_tightness;
        ops.push(Op::Move([points[1][0], points[1][1]]));
        let mut i = 1;
        while i + 2 < len {
            let v = points[i];
            let b1 = [
                v[0] + (s * points[i + 1][0] - s * points[i - 1][0]) / 6.0,
                v[1] + (s * points[i + 1][1] - s * points[i - 1][1]) / 6.0,
            ];
            let b2 = [
                points[i + 1][0] + (s * points[i][0] - s * points[i + 2][0]) / 6.0,
                points[i + 1][1] + (s * points[i][1] - s * points[i + 2][1]) / 6.0,
            ];
            let b3 = [points[i + 1][0], points[i + 1][1]];
            ops.push(Op::BCurveTo([b1[0], b1[1], b2[0], b2[1], b3[0], b3[1]]));
            i += 1;
        }
        Some(ops)
    } else if len == 3 {
        Some(vec![
            Op::Move([points[1][0], points[1][1]]),
            Op::BCurveTo([
                points[1][0],
                points[1][1],
                points[2][0],
                points[2][1],
                points[2][0],
                points[2][1],
            ]),
        ])
    } else {
        None
    }
}

/// `_computeEllipsePoints(increment, cx, cy, rx, ry, offset, overlap, o)`:
/// `(allPoints, corePoints)`.
#[allow(clippy::too_many_arguments)]
fn compute_ellipse_points(
    increment: f64,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    off: f64,
    overlap: f64,
    o: &Options,
    rng: &mut Random,
) -> (Vec<Point>, Vec<Point>) {
    let core_only = o.roughness == 0.0;
    let mut core_points = Vec::new();
    let mut all_points = Vec::new();
    if core_only {
        let increment = increment / 4.0;
        all_points.push([cx + rx * (-increment).cos(), cy + ry * (-increment).sin()]);
        let mut angle = 0.0;
        while angle <= PI * 2.0 {
            let p = [cx + rx * f64::cos(angle), cy + ry * f64::sin(angle)];
            core_points.push(p);
            all_points.push(p);
            angle += increment;
        }
        all_points.push([cx + rx * 0f64.cos(), cy + ry * 0f64.sin()]);
        all_points.push([cx + rx * increment.cos(), cy + ry * increment.sin()]);
    } else {
        let rad_offset = offset_opt(0.5, o, rng, 1.0) - (PI / 2.0);
        let jitter = |rng: &mut Random| offset_opt(off, o, rng, 1.0);
        let x = jitter(rng) + cx + 0.9 * rx * (rad_offset - increment).cos();
        let y = jitter(rng) + cy + 0.9 * ry * (rad_offset - increment).sin();
        all_points.push([x, y]);
        let end_angle = PI * 2.0 + rad_offset - 0.01;
        let mut angle = rad_offset;
        while angle < end_angle {
            let x = jitter(rng) + cx + rx * angle.cos();
            let y = jitter(rng) + cy + ry * angle.sin();
            core_points.push([x, y]);
            all_points.push([x, y]);
            angle += increment;
        }
        let a = rad_offset + PI * 2.0 + overlap * 0.5;
        let x = jitter(rng) + cx + rx * a.cos();
        let y = jitter(rng) + cy + ry * a.sin();
        all_points.push([x, y]);
        let a = rad_offset + overlap;
        let x = jitter(rng) + cx + 0.98 * rx * a.cos();
        let y = jitter(rng) + cy + 0.98 * ry * a.sin();
        all_points.push([x, y]);
        let a = rad_offset + overlap * 0.5;
        let x = jitter(rng) + cx + 0.9 * rx * a.cos();
        let y = jitter(rng) + cy + 0.9 * ry * a.sin();
        all_points.push([x, y]);
    }
    (all_points, core_points)
}

/// `_arc(increment, cx, cy, rx, ry, strt, stp, offset, o)`.
#[allow(clippy::too_many_arguments)]
fn arc_ops(
    increment: f64,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    strt: f64,
    stp: f64,
    off: f64,
    o: &Options,
    rng: &mut Random,
) -> Vec<Op> {
    let rad_offset = strt + offset_opt(0.1, o, rng, 1.0);
    let mut points: Vec<Point> = Vec::new();
    let jitter = |rng: &mut Random| offset_opt(off, o, rng, 1.0);
    let x = jitter(rng) + cx + 0.9 * rx * (rad_offset - increment).cos();
    let y = jitter(rng) + cy + 0.9 * ry * (rad_offset - increment).sin();
    points.push([x, y]);
    let mut angle = rad_offset;
    while angle <= stp {
        let x = jitter(rng) + cx + rx * angle.cos();
        let y = jitter(rng) + cy + ry * angle.sin();
        points.push([x, y]);
        angle += increment;
    }
    points.push([cx + rx * stp.cos(), cy + ry * stp.sin()]);
    points.push([cx + rx * stp.cos(), cy + ry * stp.sin()]);
    curve_ops(&points, o, rng)
}

/// `_bezierTo(x1, y1, x2, y2, x, y, current, o)`.
#[allow(clippy::too_many_arguments)]
fn bezier_to(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x: f64,
    y: f64,
    current: Point,
    o: &Options,
    rng: &mut Random,
) -> Vec<Op> {
    let mut ops = Vec::new();
    // `o.maxRandomnessOffset || 1`
    let base = if o.max_randomness_offset == 0.0 || o.max_randomness_offset.is_nan() {
        1.0
    } else {
        o.max_randomness_offset
    };
    let ros = [base, base + 0.3];
    let iterations = if o.disable_multi_stroke { 1 } else { 2 };
    let preserve = o.preserve_vertices;
    let rand = |rng: &mut Random, x: f64| offset_opt(x, o, rng, 1.0);
    for (i, &ro) in ros.iter().enumerate().take(iterations) {
        if i == 0 {
            ops.push(Op::Move([current[0], current[1]]));
        } else {
            let dx = if preserve { 0.0 } else { rand(rng, ros[0]) };
            let dy = if preserve { 0.0 } else { rand(rng, ros[0]) };
            ops.push(Op::Move([current[0] + dx, current[1] + dy]));
        }
        let f = if preserve {
            [x, y]
        } else {
            let fx = x + rand(rng, ro);
            let fy = y + rand(rng, ro);
            [fx, fy]
        };
        let c1x = x1 + rand(rng, ro);
        let c1y = y1 + rand(rng, ro);
        let c2x = x2 + rand(rng, ro);
        let c2y = y2 + rand(rng, ro);
        ops.push(Op::BCurveTo([c1x, c1y, c2x, c2y, f[0], f[1]]));
    }
    ops
}
