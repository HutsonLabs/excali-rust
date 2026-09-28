//! Skia's hairline curve subdivision, ported from Skia at `chrome/m153`
//! (`SkScan_Hairline.cpp`: `hair_quad`, `hair_cubic`, `compute_quad_level`,
//! `compute_cubic_segs`, `quick_cubic_niceness_check`), for the curves
//! tiny-skia's port cannot take.
//!
//! tiny-skia tests a point with `(x * y).is_finite()`, where Skia tests
//! each coordinate (`SkIsFinite`). A curve whose coordinates are finite but
//! past `sqrt(f32::MAX)` (a dashed or undashed hairline of a cubic with
//! control points at 1e20) fails tiny-skia's debug assertions, and its
//! cubic subdivision drops points Skia draws. Such a curve is handed to
//! tiny-skia as the lines Skia's hairliner would draw it with; tiny-skia's
//! line hairliner is Skia's.

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

/// A stretch of a flattened curve: lines through the points (the start is
/// the curve's current point), or, where Skia's evaluated points are not
/// finite and it draws nothing, a move to the end.
#[derive(Debug, PartialEq)]
pub(crate) enum Piece {
    Lines(Vec<V>),
    Skip(V),
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

/// `hair_quad`: `1 << compute_quad_level` lines, evaluated with
/// `SkQuadCoeff`.
pub(crate) fn quad(q: [V; 3]) -> Vec<Piece> {
    let lines = 1u32 << quad_level(q);
    let two = |v: V| (v.0 + v.0, v.1 + v.1);
    let p1x2 = two(q[1]);
    let a = (q[2].0 - p1x2.0 + q[0].0, q[2].1 - p1x2.1 + q[0].1);
    let b = two((q[1].0 - q[0].0, q[1].1 - q[0].1));
    let c = q[0];
    let dt = 1.0 / lines as f32;
    let mut t = 0.0f32;
    let mut out = Vec::with_capacity(lines as usize);
    for _ in 1..lines {
        t += dt;
        out.push(((a.0 * t + b.0) * t + c.0, (a.1 * t + b.1) * t + c.1));
    }
    out.push(q[2]);
    vec![Piece::Lines(out)]
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

/// `hair_cubic2`: the cubic as lines through points evaluated with
/// `SkCubicCoeff`, or nothing when one of them is not finite.
fn cubic_lines(c: [V; 4]) -> Piece {
    let lines = cubic_segs(c);
    if lines == 1 {
        return Piece::Lines(vec![c[3]]);
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
    let mut out = Vec::with_capacity(lines as usize);
    for _ in 1..lines {
        t += dt;
        out.push((
            ((a.0 * t + b.0) * t + cc.0) * t + d.0,
            ((a.1 * t + b.1) * t + cc.1) * t + d.1,
        ));
    }
    if !out.iter().all(|p| p.0.is_finite() && p.1.is_finite()) {
        return Piece::Skip(c[3]);
    }
    out.push(c[3]);
    Piece::Lines(out)
}

/// `hair_cubic`: a cubic that fails the niceness check is chopped at its
/// points of maximum curvature (`SkChopCubicAtMaxCurvature`) first.
pub(crate) fn cubic(c: [V; 4]) -> Vec<Piece> {
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
        assert_eq!(flat, vec![Piece::Lines(vec![(2.0, 0.0)])]);
        let Piece::Lines(pts) = &quad([(0.0, 0.0), (1e20, 1e20), (10.0, 0.0)])[0] else {
            panic!("lines");
        };
        assert_eq!(pts.len(), 32);
        assert_eq!(pts.last(), Some(&(10.0, 0.0)));
    }

    #[test]
    fn a_huge_cubic_is_512_finite_lines() {
        // The cubic's maximum curvature solves to NaN, which SkTPin takes
        // to 0: no chop. Every evaluated point is finite, so Skia draws
        // the 512 lines (the first leaves (0.5, 0.5) up and to the right).
        let pieces = cubic([(0.5, 0.5), (1e20, -1e20), (-1e20, 1e20), (150.0, 150.0)]);
        assert_eq!(pieces.len(), 1);
        let Piece::Lines(pts) = &pieces[0] else {
            panic!("lines");
        };
        assert_eq!(pts.len(), 512);
        assert!(pts[0].0 > 1e17 && pts[0].1 < -1e17);
        assert_eq!(pts.last(), Some(&(150.0, 150.0)));
    }

    #[test]
    fn a_cubic_with_points_past_the_range_draws_nothing() {
        let c = [(0.0, 0.0), (3e38, 3e38), (-3e38, 3e38), (1.0, 0.0)];
        assert!(cubic(c)
            .iter()
            .any(|p| matches!(p, Piece::Skip(end) if *end == (1.0, 0.0))));
    }
}
