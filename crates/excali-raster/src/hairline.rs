//! Skia's hairline path walk and curve subdivision, ported from Skia at
//! `chrome/m153` (`SkScan_Hairline.cpp`: `hair_path`, `extend_pts`,
//! `is_next_contour_closed`, `hair_quad`, `hair_cubic`, `haircubic`,
//! `hairconic`, `compute_quad_level`, `compute_cubic_segs`,
//! `quick_cubic_niceness_check`), for the paths tiny-skia's port cannot
//! take.
//!
//! tiny-skia tests a point with `(x * y).is_finite()`, where Skia tests
//! each coordinate (`SkIsFinite`). A curve whose coordinates are finite but
//! past `sqrt(f32::MAX)` (a dashed or undashed hairline of a cubic with
//! control points at 1e20) fails tiny-skia's debug assertions, and its
//! cubic subdivision drops points Skia draws. A device path holding such a
//! curve is walked here as `hair_path` walks it: caps are Skia's
//! `extend_pts` on each segment (a closed contour takes none), a curve is
//! cut into the lines Skia's hairliner draws it with, and a curve with a
//! point that evaluates to infinity is skipped alone, the rest of its
//! contour (the closing line to the contour's first point included) drawn
//! as Skia draws it. What comes out is drawn by tiny-skia's hairliner
//! (Skia's) with butt caps, each draw its own contour, since the caps are
//! already applied.

use crate::edges::Seg;

type V = (f32, f32);

const MAX_CUBIC_SUBDIVIDE_LEVEL: u32 = 9;
const MAX_QUAD_SUBDIVIDE_LEVEL: u32 = 5;

/// A curve tiny-skia's hairliner misjudges: a coordinate whose square is
/// not finite (so a product of two coordinates, or of a point on the
/// curve, may overflow).
pub(crate) fn overflows(pts: &[V]) -> bool {
    pts.iter().any(|p| {
        let m = p.0.abs().max(p.1.abs());
        !(m * m).is_finite()
    })
}

/// The hairline's cap (`SkPaint::Cap`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cap {
    Butt,
    Round,
    Square,
}

/// One draw of `hair_path`, for tiny-skia's hairliner with butt caps:
/// lines through the points, or a curve tiny-skia's hairliner takes as it
/// is (its coordinates do not overflow its finiteness test).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Draw {
    Lines(Vec<V>),
    Quad([V; 3]),
    Cubic([V; 4]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verb {
    Move,
    Line,
    Quad,
    Conic,
    Cubic,
    Close,
}

/// A path record as `SkPathIter` yields it: the verb and its points, the
/// start of a segment included.
struct Rec {
    verb: Verb,
    pts: Vec<V>,
    weight: f32,
}

/// The segments as an `SkPath` holds them: a move injected before a
/// segment that follows a close, or starts the path, at the last move's
/// point (`SkPathBuilder::injectMoveToIfNeeded`, from (0, 0)); a move
/// after a move replaces it; a close after a close, or at the start of
/// the path, is dropped (`SkPathBuilder::close`).
fn records(segs: &[Seg]) -> Vec<Rec> {
    let f = |p: crate::edges::P| (p.0 as f32, p.1 as f32);
    let mut recs: Vec<Rec> = Vec::new();
    let mut move_pt = (0.0f32, 0.0f32);
    let mut last = move_pt;
    let mut need_move = true;
    let rec = |verb, pts: Vec<V>| Rec {
        verb,
        pts,
        weight: 1.0,
    };
    for &seg in segs {
        if let Seg::Move(p) = seg {
            let p = f(p);
            if recs.last().is_some_and(|r| r.verb == Verb::Move) {
                recs.pop();
            }
            recs.push(rec(Verb::Move, vec![p]));
            move_pt = p;
            last = p;
            need_move = false;
            continue;
        }
        if seg == Seg::Close {
            if recs.last().is_some_and(|r| r.verb != Verb::Close) {
                recs.push(rec(Verb::Close, Vec::new()));
            }
            need_move = true;
            last = move_pt;
            continue;
        }
        if need_move {
            if recs.last().is_some_and(|r| r.verb == Verb::Move) {
                recs.pop();
            }
            recs.push(rec(Verb::Move, vec![move_pt]));
            last = move_pt;
            need_move = false;
        }
        let r = match seg {
            Seg::Line(p) => rec(Verb::Line, vec![last, f(p)]),
            Seg::Quad(c, p) => rec(Verb::Quad, vec![last, f(c), f(p)]),
            Seg::Conic(c, p, w) => Rec {
                verb: Verb::Conic,
                pts: vec![last, f(c), f(p)],
                weight: w as f32,
            },
            Seg::Cubic(a, b, p) => rec(Verb::Cubic, vec![last, f(a), f(b), f(p)]),
            Seg::Move(_) | Seg::Close => unreachable!("handled above"),
        };
        last = *r.pts.last().expect("a segment has points");
        recs.push(r);
    }
    recs
}

/// `is_next_contour_closed`, for the move at `recs[i]`.
fn next_contour_closed(recs: &[Rec], i: usize) -> bool {
    for r in &recs[i + 1..] {
        match r.verb {
            Verb::Close => return true,
            Verb::Move => return false,
            _ => {}
        }
    }
    false
}

fn is_zero(v: V) -> bool {
    v.0 == 0.0 && v.1 == 0.0
}

/// `SkPoint::normalize` (`set_point_length` in doubles): the zero vector
/// when the result is not finite or is zero.
fn normalize(v: V) -> V {
    let (x, y) = (f64::from(v.0), f64::from(v.1));
    let scale = 1.0 / (x * x + y * y).sqrt();
    let (nx, ny) = ((x * scale) as f32, (y * scale) as f32);
    if !nx.is_finite() || !ny.is_finite() || (nx == 0.0 && ny == 0.0) {
        return (0.0, 0.0);
    }
    (nx, ny)
}

/// `extend_pts`: the segment's start (after a move) and end (before a
/// move, a close or the end of the path) pushed out along their tangents
/// by the cap's outset, control points equal to an end moving with it.
fn extend_pts(cap: Cap, prev: Option<Verb>, next: Option<Verb>, pts: &mut [V]) {
    let outset = if cap == Cap::Square {
        0.5
    } else {
        std::f32::consts::PI / 8.0
    };
    let n = pts.len();
    if prev == Some(Verb::Move) {
        let mut ctrl = 0;
        let mut controls = n - 1;
        let mut tangent;
        loop {
            ctrl += 1;
            tangent = (pts[0].0 - pts[ctrl].0, pts[0].1 - pts[ctrl].1);
            if !is_zero(tangent) {
                break;
            }
            controls -= 1;
            if controls == 0 {
                break;
            }
        }
        if is_zero(tangent) {
            tangent = (1.0, 0.0);
            controls = n - 1;
        } else {
            tangent = normalize(tangent);
        }
        let mut k = 0;
        loop {
            pts[k].0 += tangent.0 * outset;
            pts[k].1 += tangent.1 * outset;
            k += 1;
            controls += 1;
            if controls >= n {
                break;
            }
        }
    }
    if matches!(next, None | Some(Verb::Move) | Some(Verb::Close)) {
        let last = n - 1;
        let mut ctrl = last;
        let mut controls = n - 1;
        let mut tangent;
        loop {
            ctrl -= 1;
            tangent = (pts[last].0 - pts[ctrl].0, pts[last].1 - pts[ctrl].1);
            if !is_zero(tangent) {
                break;
            }
            controls -= 1;
            if controls == 0 {
                break;
            }
        }
        if is_zero(tangent) {
            tangent = (-1.0, 0.0);
            controls = n - 1;
        } else {
            tangent = normalize(tangent);
        }
        let mut k = last;
        loop {
            pts[k].0 += tangent.0 * outset;
            pts[k].1 += tangent.1 * outset;
            controls += 1;
            if controls >= n {
                break;
            }
            k -= 1;
        }
    }
}

/// `hair_path`: the device path's hairline as tiny-skia draws it with butt
/// caps.
pub(crate) fn hair_path(segs: &[Seg], cap: Cap) -> Vec<Draw> {
    let recs = records(segs);
    let butt = cap == Cap::Butt;
    let mut out = Vec::new();
    let mut prev: Option<Verb> = None;
    let (mut first, mut last) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    let mut closed = false;
    for (i, rec) in recs.iter().enumerate() {
        let next = recs.get(i + 1).map(|r| r.verb);
        let mut pts = rec.pts.clone();
        match rec.verb {
            Verb::Move => {
                first = pts[0];
                last = pts[0];
                closed = !butt && next_contour_closed(&recs, i);
            }
            Verb::Close => {
                pts = vec![last, first];
                if !butt && prev == Some(Verb::Move) {
                    // A move then a close: capped as SVG caps a degenerate
                    // segment.
                    extend_pts(cap, prev, next, &mut pts);
                }
                out.push(Draw::Lines(pts.clone()));
            }
            verb => {
                // SkPath::Is*Degenerate(..., exact = true): every point
                // the same.
                let degenerate = pts.iter().all(|&p| p == pts[0]);
                if !butt && (!closed || degenerate) {
                    extend_pts(cap, prev, next, &mut pts);
                }
                match verb {
                    Verb::Line => out.push(Draw::Lines(pts.clone())),
                    Verb::Quad => hair_quad(&mut out, [pts[0], pts[1], pts[2]]),
                    Verb::Conic => {
                        let d = |p: V| (f64::from(p.0), f64::from(p.1));
                        let quads = crate::edges::conic_quads(
                            d(pts[0]),
                            d(pts[1]),
                            d(pts[2]),
                            rec.weight.into(),
                        );
                        for (s, c, e) in quads {
                            let f = |p: crate::edges::P| (p.0 as f32, p.1 as f32);
                            hair_quad(&mut out, [f(s), f(c), f(e)]);
                        }
                    }
                    _ => {
                        let c = [pts[0], pts[1], pts[2], pts[3]];
                        if overflows(&c) {
                            out.extend(cubic(c).into_iter().flatten().map(Draw::Lines));
                        } else {
                            out.push(Draw::Cubic(c));
                        }
                    }
                }
                last = *pts.last().expect("a segment has points");
            }
        }
        if !butt {
            let segment = matches!(
                rec.verb,
                Verb::Line | Verb::Quad | Verb::Conic | Verb::Cubic
            );
            if prev == Some(Verb::Move) && segment {
                // The cap moved the contour's start: close to it instead.
                first = pts[0];
            }
            prev = Some(rec.verb);
        }
    }
    out
}

fn hair_quad(out: &mut Vec<Draw>, q: [V; 3]) {
    if overflows(&q) {
        out.extend(quad(q).map(Draw::Lines));
    } else {
        out.push(Draw::Quad(q));
    }
}

/// `compute_int_quad_dist` and `compute_quad_level`.
fn quad_level(q: [V; 3]) -> u32 {
    // SkScalarCeilToInt saturates.
    let ceil = |v: f32| v.ceil().clamp(i32::MIN as f32, i32::MAX as f32) as i32 as u32;
    let dx = ((q[0].0 + q[2].0) * 0.5 - q[1].0).abs();
    let dy = ((q[0].1 + q[2].1) * 0.5 - q[1].1).abs();
    let (idx, idy) = (ceil(dx), ceil(dy));
    let d = if idx > idy {
        idx + (idy >> 1)
    } else {
        idy + (idx >> 1)
    };
    ((33 - d.leading_zeros()) >> 1).min(MAX_QUAD_SUBDIVIDE_LEVEL)
}

/// `hair_quad`: the lines through the quadratic's start, the points
/// `SkQuadCoeff` evaluates at `1 << compute_quad_level` steps and its end,
/// or `None` (nothing drawn) when an evaluated point is not finite.
pub(crate) fn quad(q: [V; 3]) -> Option<Vec<V>> {
    let lines = 1u32 << quad_level(q);
    let two = |v: V| (v.0 + v.0, v.1 + v.1);
    let p1x2 = two(q[1]);
    let a = (q[2].0 - p1x2.0 + q[0].0, q[2].1 - p1x2.1 + q[0].1);
    let b = two((q[1].0 - q[0].0, q[1].1 - q[0].1));
    let c = q[0];
    let dt = 1.0 / lines as f32;
    let mut t = 0.0f32;
    let mut out = Vec::with_capacity(lines as usize + 1);
    out.push(q[0]);
    for _ in 1..lines {
        t += dt;
        out.push(((a.0 * t + b.0) * t + c.0, (a.1 * t + b.1) * t + c.1));
    }
    if !out.iter().all(|p| p.0.is_finite() && p.1.is_finite()) {
        return None;
    }
    out.push(q[2]);
    Some(out)
}

fn lt_90(p0: V, pivot: V, p2: V) -> bool {
    let a = (p0.0 - pivot.0, p0.1 - pivot.1);
    let b = (p2.0 - pivot.0, p2.1 - pivot.1);
    a.0 * b.0 + a.1 * b.1 >= 0.0
}

/// `quick_cubic_niceness_check`: the off-curve points lie within the
/// limits of the on-curve points.
fn nice(c: [V; 4]) -> bool {
    lt_90(c[1], c[0], c[3])
        && lt_90(c[2], c[0], c[3])
        && lt_90(c[1], c[3], c[0])
        && lt_90(c[2], c[3], c[0])
}

/// `compute_cubic_segs`.
fn cubic_segs(c: [V; 4]) -> u32 {
    let (third, two_thirds) = (1.0f32 / 3.0, 2.0f32 / 3.0);
    let p13 = (
        third * c[3].0 + two_thirds * c[0].0,
        third * c[3].1 + two_thirds * c[0].1,
    );
    let p23 = (
        third * c[0].0 + two_thirds * c[3].0,
        third * c[0].1 + two_thirds * c[3].1,
    );
    // skvx::max and max_component: `a > b ? a : b` lane by lane.
    let max = |a: f32, b: f32| if a > b { a } else { b };
    let diff = max(
        max((c[1].0 - p13.0).abs(), (c[2].0 - p23.0).abs()),
        max((c[1].1 - p13.1).abs(), (c[2].1 - p23.1).abs()),
    );
    let mut tol = 1.0f32 / 8.0;
    for i in 0..MAX_CUBIC_SUBDIVIDE_LEVEL {
        if diff < tol {
            return 1 << i;
        }
        tol *= 4.0;
    }
    1 << MAX_CUBIC_SUBDIVIDE_LEVEL
}

/// `hair_cubic`: the lines through the cubic's start, the points
/// `SkCubicCoeff` evaluates and its end, or `None` (nothing drawn) when
/// one of them is not finite.
fn cubic_lines(c: [V; 4]) -> Option<Vec<V>> {
    let lines = cubic_segs(c);
    if lines == 1 {
        return Some(vec![c[0], c[3]]);
    }
    let three = |v: V| (3.0 * v.0, 3.0 * v.1);
    let a = (
        c[3].0 + 3.0 * (c[1].0 - c[2].0) - c[0].0,
        c[3].1 + 3.0 * (c[1].1 - c[2].1) - c[0].1,
    );
    let b = three((
        c[2].0 - (c[1].0 + c[1].0) + c[0].0,
        c[2].1 - (c[1].1 + c[1].1) + c[0].1,
    ));
    let cc = three((c[1].0 - c[0].0, c[1].1 - c[0].1));
    let d = c[0];
    let dt = 1.0 / lines as f32;
    let mut t = 0.0f32;
    let mut out = Vec::with_capacity(lines as usize + 1);
    out.push(c[0]);
    for _ in 1..lines {
        t += dt;
        out.push((
            ((a.0 * t + b.0) * t + cc.0) * t + d.0,
            ((a.1 * t + b.1) * t + cc.1) * t + d.1,
        ));
    }
    if !out.iter().all(|p| p.0.is_finite() && p.1.is_finite()) {
        return None;
    }
    out.push(c[3]);
    Some(out)
}

/// `haircubic`: a cubic that fails the niceness check is chopped at its
/// points of maximum curvature (`SkChopCubicAtMaxCurvature`) first; each
/// piece is drawn or, with a point that is not finite, skipped (`None`).
pub(crate) fn cubic(c: [V; 4]) -> Vec<Option<Vec<V>>> {
    if nice(c) {
        return vec![cubic_lines(c)];
    }
    let ts: Vec<f32> = crate::stroke::find_cubic_max_curvature(c)
        .into_iter()
        .filter(|&t| 0.0 < t && t < 1.0)
        .collect();
    // SkChopCubicAt(src, dst, tValues, count): each later t rescaled to the
    // remainder and pinned.
    let mut pieces = Vec::with_capacity(ts.len() + 1);
    let mut rest = c;
    let mut last_t = 0.0f32;
    for &t in &ts {
        let local = if last_t == 0.0 {
            t
        } else {
            crate::stroke::sk_pin((t - last_t) / (1.0 - last_t), 0.0, 1.0)
        };
        let chopped = crate::stroke::chop_cubic_at(rest, local);
        pieces.push(cubic_lines([
            chopped[0], chopped[1], chopped[2], chopped[3],
        ]));
        rest = [chopped[3], chopped[4], chopped[5], chopped[6]];
        last_t = t;
    }
    pieces.push(cubic_lines(rest));
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_is_a_coordinate_past_the_square_root_of_the_f32_range() {
        assert!(!overflows(&[(0.5, 0.5), (1e19, -1e19), (150.0, 150.0)]));
        assert!(overflows(&[(0.5, 0.5), (1e20, -1e20), (150.0, 150.0)]));
        // A single coordinate is enough: a point inside the hull may pair
        // it with another.
        assert!(overflows(&[(1e30, 0.0), (0.0, 1.0)]));
    }

    #[test]
    fn a_quad_is_one_to_32_lines() {
        let flat = quad([(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)]);
        assert_eq!(flat, Some(vec![(0.0, 0.0), (2.0, 0.0)]));
        let pts = quad([(0.0, 0.0), (1e20, 1e20), (10.0, 0.0)]).expect("finite");
        assert_eq!(pts.len(), 33);
        assert_eq!(pts.first(), Some(&(0.0, 0.0)));
        assert_eq!(pts.last(), Some(&(10.0, 0.0)));
    }

    #[test]
    fn a_huge_cubic_is_512_finite_lines() {
        // The cubic's maximum curvature solves to NaN, which SkTPin takes
        // to 0: no chop. Every evaluated point is finite, so Skia draws
        // the 512 lines (the first leaves (0.5, 0.5) up and to the right).
        let pieces = cubic([(0.5, 0.5), (1e20, -1e20), (-1e20, 1e20), (150.0, 150.0)]);
        assert_eq!(pieces.len(), 1);
        let pts = pieces[0].as_ref().expect("lines");
        assert_eq!(pts.len(), 513);
        assert_eq!(pts[0], (0.5, 0.5));
        assert!(pts[1].0 > 1e17 && pts[1].1 < -1e17);
        assert_eq!(pts.last(), Some(&(150.0, 150.0)));
    }

    #[test]
    fn a_quad_with_points_past_the_range_draws_nothing() {
        // hair_quad draws only when every evaluated point is finite: here
        // the coefficient A is 1 - 6e38 + 0, which is -inf.
        assert_eq!(quad([(0.0, 0.0), (3e38, 3e38), (1.0, 0.0)]), None);
    }

    #[test]
    fn a_cubic_with_points_past_the_range_draws_nothing() {
        let c = [(0.0, 0.0), (3e38, 3e38), (-3e38, 3e38), (1.0, 0.0)];
        assert!(cubic(c).iter().any(Option::is_none));
        // At +-8e37 (inside SkPathPriv::TooBigForMath's limit) the
        // coefficient A is 3 * 1.6e38, past the range, too.
        let c = [(10.0, 10.0), (8e37, 8e37), (-8e37, 8e37), (100.0, 10.0)];
        assert_eq!(cubic(c), vec![None]);
    }

    fn lines(pts: &[V]) -> Draw {
        Draw::Lines(pts.to_vec())
    }

    #[test]
    fn a_skipped_curve_leaves_its_contour_and_the_close_goes_to_its_start() {
        let segs = [
            Seg::Move((10.0, 10.0)),
            Seg::Cubic((8e37, 8e37), (-8e37, 8e37), (100.0, 10.0)),
            Seg::Line((100.0, 100.0)),
            Seg::Close,
        ];
        for cap in [Cap::Butt, Cap::Round, Cap::Square] {
            // Round and square caps leave a closed contour's segments as
            // they are.
            assert_eq!(
                hair_path(&segs, cap),
                vec![
                    lines(&[(100.0, 10.0), (100.0, 100.0)]),
                    lines(&[(100.0, 100.0), (10.0, 10.0)]),
                ],
                "{cap:?}"
            );
        }
    }

    #[test]
    fn caps_extend_only_a_contours_ends() {
        let segs = [
            Seg::Move((10.0, 50.0)),
            Seg::Line((50.0, 50.0)),
            Seg::Cubic((8e37, 8e37), (-8e37, 8e37), (70.0, 50.0)),
            Seg::Line((110.0, 50.0)),
        ];
        assert_eq!(
            hair_path(&segs, Cap::Square),
            vec![
                lines(&[(9.5, 50.0), (50.0, 50.0)]),
                lines(&[(70.0, 50.0), (110.5, 50.0)]),
            ]
        );
        let o = std::f32::consts::PI / 8.0;
        assert_eq!(
            hair_path(&segs, Cap::Round),
            vec![
                lines(&[(10.0 - o, 50.0), (50.0, 50.0)]),
                lines(&[(70.0, 50.0), (110.0 + o, 50.0)]),
            ]
        );
    }

    #[test]
    fn an_open_contour_takes_caps_and_a_move_then_close_is_a_dot() {
        // The first contour is open (is_next_contour_closed meets the move
        // first), so its line is capped at both ends. A move then a close
        // is kept (SkPathBuilder::close) and capped at both ends as SVG
        // caps a degenerate segment: a half-unit each way.
        let segs = [
            Seg::Move((10.0, 10.0)),
            Seg::Line((20.0, 10.0)),
            Seg::Move((30.0, 30.0)),
            Seg::Close,
        ];
        assert_eq!(
            hair_path(&segs, Cap::Square),
            vec![
                lines(&[(9.5, 10.0), (20.5, 10.0)]),
                lines(&[(30.5, 30.0), (29.5, 30.0)]),
            ]
        );
        assert!(hair_path(&segs, Cap::Butt).contains(&lines(&[(30.0, 30.0), (30.0, 30.0)])));
    }

    #[test]
    fn the_close_of_a_capped_first_segment_goes_to_its_moved_start() {
        // A degenerate first segment of a closed contour is capped, and the
        // close goes to where the cap moved the start (hair_path's
        // `firstPt = pts[0]`).
        let segs = [
            Seg::Move((10.0, 10.0)),
            Seg::Line((10.0, 10.0)),
            Seg::Line((20.0, 10.0)),
            Seg::Close,
        ];
        assert_eq!(
            hair_path(&segs, Cap::Square),
            vec![
                lines(&[(10.5, 10.0), (10.0, 10.0)]),
                lines(&[(10.0, 10.0), (20.0, 10.0)]),
                lines(&[(20.0, 10.0), (10.5, 10.0)]),
            ]
        );
    }

    #[test]
    fn curves_inside_tiny_skias_range_stay_curves() {
        let segs = [
            Seg::Move((0.0, 0.0)),
            Seg::Quad((5.0, 5.0), (10.0, 0.0)),
            Seg::Cubic((1e20, 1e20), (-1e20, 1e20), (20.0, 0.0)),
            Seg::Cubic((25.0, 5.0), (30.0, 5.0), (35.0, 0.0)),
        ];
        let draws = hair_path(&segs, Cap::Butt);
        assert_eq!(draws.len(), 3);
        assert_eq!(draws[0], Draw::Quad([(0.0, 0.0), (5.0, 5.0), (10.0, 0.0)]));
        assert!(matches!(&draws[1], Draw::Lines(p) if p.len() == 513));
        assert_eq!(
            draws[2],
            Draw::Cubic([(20.0, 0.0), (25.0, 5.0), (30.0, 5.0), (35.0, 0.0)])
        );
    }

    #[test]
    fn a_segment_after_a_close_starts_at_the_contours_move() {
        let segs = [
            Seg::Move((10.0, 10.0)),
            Seg::Line((20.0, 10.0)),
            Seg::Close,
            Seg::Line((10.0, 20.0)),
        ];
        assert_eq!(
            hair_path(&segs, Cap::Butt),
            vec![
                lines(&[(10.0, 10.0), (20.0, 10.0)]),
                lines(&[(20.0, 10.0), (10.0, 10.0)]),
                lines(&[(10.0, 10.0), (10.0, 20.0)]),
            ]
        );
    }
}
