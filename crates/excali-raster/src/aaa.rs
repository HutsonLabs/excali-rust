//! Skia's analytic anti-aliasing, the scan converter Chrome fills every
//! canvas path with, ported from Skia at `chrome/m153`
//! (f8b66b7597c4cc859d3ed190e9c6872241e6721c):
//!
//! - `SkAnalyticEdge.cpp`: lines, quadratics and cubics as analytic edges
//!   in 16.16 fixed point, curves cut into `2^shift` lines on the fly with
//!   their y snapped to quarter pixels (`SnapY`), slopes through
//!   `quick_div`;
//! - `SkEdgeBuilder.cpp`: the path's edges (contours closed, conics as
//!   quadratics within 1/4 px, curves chopped where they turn in y,
//!   vertical lines combined);
//! - `SkScan_AAAPath.cpp`: the convex and general edge walkers, the
//!   trapezoid coverage of each strip between scan lines, and the three
//!   additive blitters (a mask for small paths, run-length rows otherwise,
//!   clamped for concave paths), with their alpha rounding and snapping;
//! - `SkPathPriv.cpp`: `ComputeConvexity` (the convex walker's condition)
//!   and `IsRectContour` (small rectangles go through
//!   `SkBlitter::blitFatAntiRect`);
//! - `SkEdgeClipper.cpp`, `SkLineClipper.cpp`: a path that leaves the clip
//!   bounds is cut to them before its edges are built, the parts outside
//!   becoming vertical edges on the bounds;
//! - `SkScan_Antihair.cpp`: `AntiFillRect`, which axis-aligned
//!   anti-aliased rectangles (images) take instead of a path.
//!
//! The result is a coverage mask; the painter composites it with the paint
//! through tiny-skia. A write the scan converter hands straight to Chrome's
//! real blitter composites over what the same draw already wrote there;
//! the mask records that as `a + b - ab`.

use crate::edges::{
    chop_cubic_at, chop_quad, conic_quads, cubic_monotonic_pieces, cubic_monotonic_pieces_x,
    mono_quad_t, quad_monotonic_pieces, quad_monotonic_pieces_x, Seg, P,
};

type Fixed = i32;
type FDot6 = i32;

const FIXED1: Fixed = 1 << 16;
const MAX_S32: i32 = i32::MAX;
/// `SK_MinS32` is `-SK_MaxS32`.
const MIN_S32: i32 = -i32::MAX;
/// `SkAnalyticEdge::kDefaultAccuracy`.
const ACCURACY: i32 = 2;
/// `MAX_COEFF_SHIFT`.
const MAX_COEFF_SHIFT: i32 = 6;

// ---------------------------------------------------------------------------
// Fixed point (SkFixed.h, SkFDot6.h)

fn fdot6_to_fixed(x: FDot6) -> Fixed {
    x.wrapping_shl(10)
}

fn fixed_to_fdot6(x: Fixed) -> FDot6 {
    x >> 10
}

fn fixed_mul(a: Fixed, b: Fixed) -> Fixed {
    ((i64::from(a) * i64::from(b)) >> 16) as i32
}

fn fixed_div(numer: i32, denom: i32) -> Fixed {
    ((i64::from(numer) << 16) / i64::from(denom)).clamp(i64::from(MIN_S32), i64::from(MAX_S32))
        as i32
}

fn fdot6_div(a: FDot6, b: FDot6) -> Fixed {
    if i16::try_from(a).is_ok() {
        (a << 16) / b
    } else {
        fixed_div(a, b)
    }
}

/// `quick_inverse`: Skia's table holds `(1 << 22) / x`, truncated.
fn quick_inverse(x: FDot6) -> Fixed {
    if x == 0 {
        0
    } else {
        (1 << 22) / x
    }
}

fn quick_div(a: FDot6, b: FDot6) -> Fixed {
    const MIN_BITS: i32 = 3;
    const MAX_ABS_A: i32 = 1 << (31 - (22 - MIN_BITS));
    let (abs_a, abs_b) = (a.abs(), b.abs());
    if ((1 << MIN_BITS)..1024).contains(&abs_b) && abs_a < MAX_ABS_A {
        (a * quick_inverse(b)) >> 6
    } else {
        fdot6_div(a, b)
    }
}

fn snap_y(y: Fixed) -> Fixed {
    let shift = 16 - ACCURACY;
    (((y as u32).wrapping_add((FIXED1 >> (ACCURACY + 1)) as u32) >> shift) << shift) as i32
}

fn fixed_round_to_fixed(x: Fixed) -> Fixed {
    x.wrapping_add(FIXED1 >> 1) & !0xFFFF
}

fn fixed_floor_to_int(x: Fixed) -> i32 {
    x >> 16
}

fn fixed_ceil_to_int(x: Fixed) -> i32 {
    x.wrapping_add(FIXED1 - 1) >> 16
}

fn fixed_ceil_to_fixed(x: Fixed) -> Fixed {
    x.wrapping_add(FIXED1 - 1) & !0xFFFF
}

fn fixed_floor_to_fixed(x: Fixed) -> Fixed {
    x & !0xFFFF
}

fn fixed_round_to_int(x: Fixed) -> i32 {
    x.wrapping_add(FIXED1 >> 1) >> 16
}

fn int_to_fixed(n: i32) -> Fixed {
    n.wrapping_shl(16)
}

fn scalar_to_fdot6(x: f32) -> FDot6 {
    (x * 64.0) as i32
}

fn fdot6_round(x: FDot6) -> i32 {
    (x + 32) >> 6
}

fn sat_add(a: i32, b: i32) -> i32 {
    a.saturating_add(b)
}

fn sat_sub(a: i32, b: i32) -> i32 {
    a.saturating_sub(b)
}

fn cheap_distance(dx: FDot6, dy: FDot6) -> FDot6 {
    let (dx, dy) = (dx.abs(), dy.abs());
    if dx > dy {
        dx + (dy >> 1)
    } else {
        dy + (dx >> 1)
    }
}

fn diff_to_shift(dx: FDot6, dy: FDot6, shift_aa: i32) -> i32 {
    let dist = (cheap_distance(dx, dy) + (1 << (2 + shift_aa))) >> (3 + shift_aa);
    (32 - dist.leading_zeros() as i32) >> 1
}

fn cubic_delta_from_line(a: FDot6, b: FDot6, c: FDot6, d: FDot6) -> FDot6 {
    let one_third = ((a * 8 - b * 15 + 6 * c + d) * 19) >> 9;
    let two_third = ((a + 6 * b - c * 15 + d * 8) * 19) >> 9;
    one_third.abs().max(two_third.abs())
}

// ---------------------------------------------------------------------------
// Analytic edges (SkAnalyticEdge.h, SkAnalyticEdge.cpp)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Line,
    Quad,
    Cubic,
}

#[derive(Clone, Copy, Debug)]
struct Edge {
    next: usize,
    prev: usize,
    x: Fixed,
    dx: Fixed,
    upper_x: Fixed,
    y: Fixed,
    upper_y: Fixed,
    lower_y: Fixed,
    dy: Fixed,
    kind: Kind,
    /// Lines 0; quadratics count down from `2^shift`, cubics up from
    /// `-2^shift`.
    curve_count: i32,
    curve_shift: i32,
    winding: i32,
    // Quadratic state.
    qx: Fixed,
    qy: Fixed,
    qdx: Fixed,
    qdy: Fixed,
    qddx: Fixed,
    qddy: Fixed,
    q_last_x: Fixed,
    q_last_y: Fixed,
    snapped_x: Fixed,
    snapped_y: Fixed,
    // Cubic state (snapped_y shared).
    cx: Fixed,
    cy: Fixed,
    cdx: Fixed,
    cdy: Fixed,
    cddx: Fixed,
    cddy: Fixed,
    cdddx: Fixed,
    cdddy: Fixed,
    c_last_x: Fixed,
    c_last_y: Fixed,
    to_fixed_shift: i32,
}

impl Edge {
    fn blank() -> Self {
        Edge {
            next: usize::MAX,
            prev: usize::MAX,
            x: 0,
            dx: 0,
            upper_x: 0,
            y: 0,
            upper_y: 0,
            lower_y: 0,
            dy: 0,
            kind: Kind::Line,
            curve_count: 0,
            curve_shift: 0,
            winding: 1,
            qx: 0,
            qy: 0,
            qdx: 0,
            qdy: 0,
            qddx: 0,
            qddy: 0,
            q_last_x: 0,
            q_last_y: 0,
            snapped_x: 0,
            snapped_y: 0,
            cx: 0,
            cy: 0,
            cdx: 0,
            cdy: 0,
            cddx: 0,
            cddy: 0,
            cdddx: 0,
            cdddy: 0,
            c_last_x: 0,
            c_last_y: 0,
            to_fixed_shift: 0,
        }
    }

    fn go_y(&mut self, y: Fixed) {
        if y == self.y.wrapping_add(FIXED1) {
            self.x = self.x.wrapping_add(self.dx);
            self.y = y;
        } else if y != self.y {
            self.x = self
                .upper_x
                .wrapping_add(fixed_mul(self.dx, y.wrapping_sub(self.upper_y)));
            self.y = y;
        }
    }

    fn go_y_shift(&mut self, y: Fixed, y_shift: i32) {
        self.y = y;
        self.x = self.x.wrapping_add(self.dx >> y_shift);
    }

    fn set_line(&mut self, p0: (f32, f32), p1: (f32, f32)) -> bool {
        let m = (1 << ACCURACY) as f32;
        let fx = |v: f32| fdot6_to_fixed(scalar_to_fdot6(v * m)) >> ACCURACY;
        let fy = |v: f32| snap_y(fdot6_to_fixed(scalar_to_fdot6(v * m)) >> ACCURACY);
        let (mut x0, mut y0, mut x1, mut y1) = (fx(p0.0), fy(p0.1), fx(p1.0), fy(p1.1));
        let mut winding = 1;
        if y0 > y1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
            winding = -1;
        }
        let dy = fixed_to_fdot6(y1 - y0);
        if dy == 0 {
            return false;
        }
        let dx = fixed_to_fdot6(x1.wrapping_sub(x0));
        let slope = quick_div(dx, dy);
        let abs_slope = slope.abs();
        self.x = x0;
        self.dx = slope;
        self.upper_x = x0;
        self.y = y0;
        self.upper_y = y0;
        self.lower_y = y1;
        self.dy = if dx == 0 || slope == 0 {
            MAX_S32
        } else if abs_slope < 1024 {
            quick_inverse(abs_slope)
        } else {
            quick_div(dy, dx).abs()
        };
        self.kind = Kind::Line;
        self.curve_count = 0;
        self.winding = winding;
        self.curve_shift = 0;
        true
    }

    fn update_line(
        &mut self,
        mut x0: Fixed,
        mut y0: Fixed,
        mut x1: Fixed,
        mut y1: Fixed,
        slope: Fixed,
    ) -> bool {
        if y0 > y1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
            self.winding = -self.winding;
        }
        let dx = fixed_to_fdot6(x1.wrapping_sub(x0));
        let dy = fixed_to_fdot6(y1 - y0);
        if dy == 0 {
            return false;
        }
        let abs_slope = fixed_to_fdot6(slope).abs();
        self.x = x0;
        self.dx = slope;
        self.upper_x = x0;
        self.y = y0;
        self.upper_y = y0;
        self.lower_y = y1;
        self.dy = if dx == 0 || slope == 0 {
            MAX_S32
        } else if abs_slope < 1024 {
            quick_inverse(abs_slope)
        } else {
            quick_div(dy, dx).abs()
        };
        true
    }

    fn update(&mut self) -> bool {
        if self.curve_count < 0 {
            self.update_cubic()
        } else if self.curve_count > 0 {
            self.update_quadratic()
        } else {
            false
        }
    }

    fn set_quadratic(&mut self, pts: [(f32, f32); 3]) -> bool {
        let scale = (1 << (ACCURACY + 6)) as f32;
        let (mut x0, mut y0) = ((pts[0].0 * scale) as i32, (pts[0].1 * scale) as i32);
        let (x1, y1) = ((pts[1].0 * scale) as i32, (pts[1].1 * scale) as i32);
        let (mut x2, mut y2) = ((pts[2].0 * scale) as i32, (pts[2].1 * scale) as i32);
        let mut winding = 1;
        if y0 > y2 {
            std::mem::swap(&mut x0, &mut x2);
            std::mem::swap(&mut y0, &mut y2);
            winding = -1;
        }
        if fdot6_round(y0) == fdot6_round(y2) {
            return false;
        }
        let mut shift = {
            let dx = (x1.wrapping_shl(1) - x0 - x2) >> 2;
            let dy = (y1.wrapping_shl(1) - y0 - y2) >> 2;
            diff_to_shift(dx, dy, ACCURACY)
        };
        if shift == 0 {
            shift = 1;
        } else if shift > MAX_COEFF_SHIFT {
            shift = MAX_COEFF_SHIFT;
        }
        self.winding = winding;
        self.kind = Kind::Quad;
        self.curve_count = 1 << shift;
        self.curve_shift = shift - 1;
        let div2 = |v: FDot6| v.wrapping_shl(16 - 6 - 1);
        let a = div2(x0 - x1 - x1 + x2);
        let b = fdot6_to_fixed(x1 - x0);
        self.qx = fdot6_to_fixed(x0);
        self.qdx = b + (a >> shift);
        self.qddx = a >> (shift - 1);
        let a = div2(y0 - y1 - y1 + y2);
        let b = fdot6_to_fixed(y1 - y0);
        self.qy = fdot6_to_fixed(y0);
        self.qdy = b + (a >> shift);
        self.qddy = a >> (shift - 1);
        self.q_last_x = fdot6_to_fixed(x2);
        self.q_last_y = fdot6_to_fixed(y2);

        self.qx >>= ACCURACY;
        self.qy >>= ACCURACY;
        self.qdx >>= ACCURACY;
        self.qdy >>= ACCURACY;
        self.qddx >>= ACCURACY;
        self.qddy >>= ACCURACY;
        self.q_last_x >>= ACCURACY;
        self.q_last_y >>= ACCURACY;
        self.qy = snap_y(self.qy);
        self.q_last_y = snap_y(self.q_last_y);
        self.snapped_x = self.qx;
        self.snapped_y = self.qy;
        self.update_quadratic()
    }

    fn update_quadratic(&mut self) -> bool {
        let mut success = false;
        let mut count = self.curve_count;
        let mut oldx = self.qx;
        let mut oldy = self.qy;
        let mut dx = self.qdx;
        let mut dy = self.qdy;
        let shift = self.curve_shift;
        let (mut newx, mut newy, mut new_snapped_x, mut new_snapped_y);
        loop {
            let slope;
            count -= 1;
            if count > 0 {
                newx = oldx.wrapping_add(dx >> shift);
                newy = oldy.wrapping_add(dy >> shift);
                if (dy >> shift).abs() >= FIXED1 * 2
                    && (i64::from(dy.abs()) << 6) > i64::from(dx.abs())
                {
                    let diff_y = fixed_to_fdot6(newy - self.snapped_y);
                    slope = if diff_y != 0 {
                        quick_div(fixed_to_fdot6(newx.wrapping_sub(self.snapped_x)), diff_y)
                    } else {
                        MAX_S32
                    };
                    new_snapped_y = self.q_last_y.min(fixed_round_to_fixed(newy));
                    new_snapped_x = newx.wrapping_sub(fixed_mul(slope, newy - new_snapped_y));
                } else {
                    new_snapped_y = self.q_last_y.min(snap_y(newy));
                    new_snapped_x = newx;
                    let diff_y = fixed_to_fdot6(new_snapped_y - self.snapped_y);
                    slope = if diff_y != 0 {
                        quick_div(fixed_to_fdot6(newx.wrapping_sub(self.snapped_x)), diff_y)
                    } else {
                        MAX_S32
                    };
                }
                dx = dx.wrapping_add(self.qddx);
                dy = dy.wrapping_add(self.qddy);
            } else {
                newx = self.q_last_x;
                newy = self.q_last_y;
                new_snapped_y = newy;
                new_snapped_x = newx;
                let diff_y = fixed_to_fdot6(newy - self.snapped_y);
                slope = if diff_y != 0 {
                    quick_div(fixed_to_fdot6(newx.wrapping_sub(self.snapped_x)), diff_y)
                } else {
                    MAX_S32
                };
            }
            if slope < MAX_S32 {
                success = self.update_line(
                    self.snapped_x,
                    self.snapped_y,
                    new_snapped_x,
                    new_snapped_y,
                    slope,
                );
            }
            oldx = newx;
            oldy = newy;
            if !(count > 0 && !success) {
                break;
            }
        }
        self.qx = newx;
        self.qy = newy;
        self.qdx = dx;
        self.qdy = dy;
        self.snapped_x = new_snapped_x;
        self.snapped_y = new_snapped_y;
        self.curve_count = count;
        success
    }

    fn set_cubic(&mut self, pts: [(f32, f32); 4]) -> bool {
        let scale = (1 << (ACCURACY + 6)) as f32;
        let mut x: [FDot6; 4] = pts.map(|p| (p.0 * scale) as i32);
        let mut y: [FDot6; 4] = pts.map(|p| (p.1 * scale) as i32);
        let mut winding = 1;
        if y[0] > y[3] {
            x.swap(0, 3);
            x.swap(1, 2);
            y.swap(0, 3);
            y.swap(1, 2);
            winding = -1;
        }
        if fdot6_round(y[0]) == fdot6_round(y[3]) {
            return false;
        }
        let shift = {
            let dx = cubic_delta_from_line(x[0], x[1], x[2], x[3]);
            let dy = cubic_delta_from_line(y[0], y[1], y[2], y[3]);
            (diff_to_shift(dx, dy, 2) + 1).min(MAX_COEFF_SHIFT)
        };
        let mut up_shift = 6;
        let mut down_shift = shift + up_shift - 10;
        if down_shift < 0 {
            down_shift = 0;
            up_shift = 10 - shift;
        }
        self.winding = winding;
        self.kind = Kind::Cubic;
        self.curve_count = -(1 << shift);
        self.curve_shift = shift;
        self.to_fixed_shift = down_shift;
        let up = |v: FDot6| v.wrapping_shl(up_shift as u32);
        let coeffs = |v: [FDot6; 4]| {
            let b = up(3 * (v[1] - v[0]));
            let c = up(3 * (v[0] - v[1] - v[1] + v[2]));
            let d = up(v[3] + 3 * (v[1] - v[2]) - v[0]);
            (
                fdot6_to_fixed(v[0]),
                b.wrapping_add(c >> shift).wrapping_add(d >> (2 * shift)),
                (2 * i64::from(c) + ((3 * i64::from(d)) >> (shift - 1))) as i32,
                ((3 * i64::from(d)) >> (shift - 1)) as i32,
                fdot6_to_fixed(v[3]),
            )
        };
        let (cx, cdx, cddx, cdddx, lx) = coeffs(x);
        let (cy, cdy, cddy, cdddy, ly) = coeffs(y);
        self.cx = cx >> ACCURACY;
        self.cy = cy >> ACCURACY;
        self.cdx = cdx >> ACCURACY;
        self.cdy = cdy >> ACCURACY;
        self.cddx = cddx >> ACCURACY;
        self.cddy = cddy >> ACCURACY;
        self.cdddx = cdddx >> ACCURACY;
        self.cdddy = cdddy >> ACCURACY;
        self.c_last_x = lx >> ACCURACY;
        self.c_last_y = ly >> ACCURACY;
        self.cy = snap_y(self.cy);
        self.snapped_y = self.cy;
        self.c_last_y = snap_y(self.c_last_y);
        self.update_cubic()
    }

    fn update_cubic(&mut self) -> bool {
        let mut success;
        let mut count = self.curve_count;
        let mut oldx = self.cx;
        let mut oldy = self.cy;
        let (mut newx, mut newy);
        let dd_shift = self.curve_shift;
        let d_shift = self.to_fixed_shift;
        loop {
            count += 1;
            if count < 0 {
                newx = oldx.wrapping_add(self.cdx >> d_shift);
                self.cdx = self.cdx.wrapping_add(self.cddx >> dd_shift);
                self.cddx = self.cddx.wrapping_add(self.cdddx);
                newy = oldy.wrapping_add(self.cdy >> d_shift);
                self.cdy = self.cdy.wrapping_add(self.cddy >> dd_shift);
                self.cddy = self.cddy.wrapping_add(self.cdddy);
            } else {
                newx = self.c_last_x;
                newy = self.c_last_y;
            }
            if newy < oldy {
                newy = oldy;
            }
            let mut new_snapped_y = snap_y(newy);
            if self.c_last_y < new_snapped_y {
                new_snapped_y = self.c_last_y;
                count = 0;
            }
            let slope = if fixed_to_fdot6(new_snapped_y - self.snapped_y) == 0 {
                MAX_S32
            } else {
                fdot6_div(
                    fixed_to_fdot6(newx.wrapping_sub(oldx)),
                    fixed_to_fdot6(new_snapped_y - self.snapped_y),
                )
            };
            success = self.update_line(oldx, self.snapped_y, newx, new_snapped_y, slope);
            oldx = newx;
            oldy = newy;
            self.snapped_y = new_snapped_y;
            if !(count < 0 && !success) {
                break;
            }
        }
        self.cx = newx;
        self.cy = newy;
        self.curve_count = count;
        success
    }

    /// `keepContinuous`, before a curve edge moves to its next line in the
    /// general walker.
    fn keep_continuous(&mut self) {
        match self.kind {
            Kind::Quad => {
                self.snapped_x = self.x;
                self.snapped_y = self.y;
            }
            Kind::Cubic => {
                self.cx = self.x;
                self.snapped_y = self.y;
            }
            Kind::Line => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Edge builder (SkEdgeBuilder.cpp, analytic)

fn f32p(p: P) -> (f32, f32) {
    (p.0 as f32, p.1 as f32)
}

enum Combine {
    No,
    Partial,
    Total,
}

fn combine_vertical(edge: &Edge, last: &mut Edge) -> Combine {
    let approx = |a: Fixed, b: Fixed| (a.wrapping_sub(b)).abs() < 0x100;
    if last.kind != Kind::Line || last.dx != 0 || edge.x != last.x {
        return Combine::No;
    }
    if edge.winding == last.winding {
        if edge.lower_y == last.upper_y {
            last.upper_y = edge.upper_y;
            last.y = last.upper_y;
            return Combine::Partial;
        }
        if approx(edge.upper_y, last.lower_y) {
            last.lower_y = edge.lower_y;
            return Combine::Partial;
        }
        return Combine::No;
    }
    if approx(edge.upper_y, last.upper_y) {
        if approx(edge.lower_y, last.lower_y) {
            return Combine::Total;
        }
        if edge.lower_y < last.lower_y {
            last.upper_y = edge.lower_y;
            last.y = last.upper_y;
            return Combine::Partial;
        }
        last.upper_y = last.lower_y;
        last.y = last.upper_y;
        last.lower_y = edge.lower_y;
        last.winding = edge.winding;
        return Combine::Partial;
    }
    if approx(edge.lower_y, last.lower_y) {
        if edge.upper_y > last.upper_y {
            last.lower_y = edge.upper_y;
            return Combine::Partial;
        }
        last.lower_y = last.upper_y;
        last.upper_y = edge.upper_y;
        last.y = last.upper_y;
        last.winding = edge.winding;
        return Combine::Partial;
    }
    Combine::No
}

struct Builder {
    edges: Vec<Edge>,
}

impl Builder {
    fn add_line(&mut self, a: P, b: P) {
        let mut e = Edge::blank();
        if !e.set_line(f32p(a), f32p(b)) {
            return;
        }
        let vertical = e.dx == 0 && e.kind == Kind::Line;
        let combine = match self.edges.last_mut() {
            Some(last) if vertical => combine_vertical(&e, last),
            _ => Combine::No,
        };
        match combine {
            Combine::Total => {
                self.edges.pop();
            }
            Combine::Partial => {}
            Combine::No => self.edges.push(e),
        }
    }

    /// `addQuad` of a quadratic already monotonic in y.
    fn add_mono_quad(&mut self, p: [P; 3]) {
        let mut e = Edge::blank();
        if e.set_quadratic(p.map(f32p)) {
            self.edges.push(e);
        }
    }

    /// `addCubic` of a cubic already monotonic in y.
    fn add_mono_cubic(&mut self, p: [P; 4]) {
        let mut e = Edge::blank();
        if e.set_cubic(p.map(f32p)) {
            self.edges.push(e);
        }
    }
}

/// One edge of `SkPathEdgeIter`: contours closed with a line, conics
/// already quadratics.
enum PathEdge {
    Line(P, P),
    Quad([P; 3]),
    Cubic([P; 4]),
}

fn path_edges(segs: &[Seg]) -> Vec<PathEdge> {
    let mut out = Vec::new();
    let mut start: Option<P> = None;
    let mut current: Option<P> = None;
    let close = |out: &mut Vec<PathEdge>, start: Option<P>, current: Option<P>| {
        if let (Some(s), Some(c)) = (start, current) {
            if f32p(s) != f32p(c) {
                out.push(PathEdge::Line(c, s));
            }
        }
    };
    for seg in segs {
        match *seg {
            Seg::Move(p) => {
                close(&mut out, start, current);
                start = Some(p);
                current = Some(p);
            }
            Seg::Close => {
                close(&mut out, start, current);
                current = start;
            }
            Seg::Line(p) => {
                if let Some(p0) = current {
                    out.push(PathEdge::Line(p0, p));
                }
                current = Some(p);
            }
            Seg::Quad(c, p) => {
                if let Some(p0) = current {
                    out.push(PathEdge::Quad([p0, c, p]));
                }
                current = Some(p);
            }
            Seg::Conic(c, p, w) => {
                if let Some(p0) = current {
                    for (a, q, e) in conic_quads(p0, c, p, w) {
                        out.push(PathEdge::Quad([a, q, e]));
                    }
                }
                current = Some(p);
            }
            Seg::Cubic(c1, c2, p) => {
                if let Some(p0) = current {
                    out.push(PathEdge::Cubic([p0, c1, c2, p]));
                }
                current = Some(p);
            }
        }
    }
    close(&mut out, start, current);
    out
}

/// `SkEdgeBuilder::buildEdges`: without a clip the curves are cut where
/// they turn in y; with one, `SkEdgeClipper` cuts them to it.
fn build_edges(segs: &[Seg], clip: Option<(ClipRect, bool)>) -> Vec<Edge> {
    let mut b = Builder { edges: Vec::new() };
    for edge in path_edges(segs) {
        match clip {
            None => match edge {
                PathEdge::Line(a, c) => b.add_line(a, c),
                PathEdge::Quad(q) => {
                    let mut pieces = Vec::new();
                    quad_monotonic_pieces(q[0], q[1], q[2], &mut pieces);
                    for (a, m, c) in pieces {
                        b.add_mono_quad([a, m, c]);
                    }
                }
                PathEdge::Cubic(c) => {
                    let mut pieces = Vec::new();
                    cubic_monotonic_pieces(c, &mut pieces);
                    for piece in pieces {
                        b.add_mono_cubic(piece);
                    }
                }
            },
            Some((rect, cull_right)) => {
                let mut clipper = Clipper {
                    clip: rect,
                    cull_right,
                    out: Vec::new(),
                };
                match edge {
                    PathEdge::Line(a, c) => clipper.clip_line(a, c),
                    PathEdge::Quad(q) => clipper.clip_quad(q),
                    PathEdge::Cubic(c) => clipper.clip_cubic(c),
                }
                for piece in clipper.out {
                    match piece {
                        PathEdge::Line(a, c) => b.add_line(a, c),
                        PathEdge::Quad(q) => b.add_mono_quad(q),
                        PathEdge::Cubic(c) => b.add_mono_cubic(c),
                    }
                }
            }
        }
    }
    b.edges
}

// ---------------------------------------------------------------------------
// Clipping edges to the clip bounds (SkEdgeClipper.cpp, SkLineClipper.cpp)

/// Left, top, right, bottom.
#[derive(Clone, Copy)]
struct ClipRect {
    l: f64,
    t: f64,
    r: f64,
    b: f64,
}

struct Clipper {
    clip: ClipRect,
    cull_right: bool,
    out: Vec<PathEdge>,
}

fn pin_unsorted(v: f64, a: f64, b: f64) -> f64 {
    let (lo, hi) = if b < a { (b, a) } else { (a, b) };
    v.clamp(lo, hi)
}

const NEARLY_ZERO: f64 = 1.0 / 4096.0;

fn sect_with_horizontal(p: [P; 2], y: f64) -> f64 {
    let dy = p[1].1 - p[0].1;
    if dy.abs() <= NEARLY_ZERO {
        (p[0].0 + p[1].0) / 2.0
    } else {
        let r = p[0].0 + (y - p[0].1) * (p[1].0 - p[0].0) / (p[1].1 - p[0].1);
        f64::from(pin_unsorted(r, p[0].0, p[1].0) as f32)
    }
}

fn sect_clamp_with_vertical(p: [P; 2], x: f64) -> f64 {
    let dx = p[1].0 - p[0].0;
    let y = if dx.abs() <= NEARLY_ZERO {
        (p[0].1 + p[1].1) / 2.0
    } else {
        f64::from((p[0].1 + (x - p[0].0) * (p[1].1 - p[0].1) / (p[1].0 - p[0].0)) as f32)
    };
    pin_unsorted(y, p[0].1, p[1].1)
}

impl Clipper {
    fn v_line(&mut self, x: f64, y0: f64, y1: f64, reverse: bool) {
        let (y0, y1) = if reverse { (y1, y0) } else { (y0, y1) };
        self.out.push(PathEdge::Line((x, y0), (x, y1)));
    }

    fn quad(&mut self, p: [P; 3], reverse: bool) {
        self.out
            .push(PathEdge::Quad(if reverse { [p[2], p[1], p[0]] } else { p }));
    }

    fn cubic(&mut self, p: [P; 4], reverse: bool) {
        self.out.push(PathEdge::Cubic(if reverse {
            [p[3], p[2], p[1], p[0]]
        } else {
            p
        }));
    }

    /// `SkLineClipper::ClipLine`.
    fn clip_line(&mut self, p0: P, p1: P) {
        let c = self.clip;
        let pts = [p0, p1];
        let (i0, i1) = if pts[0].1 < pts[1].1 { (0, 1) } else { (1, 0) };
        if pts[i1].1 <= c.t || pts[i0].1 >= c.b {
            return;
        }
        let mut tmp = pts;
        if pts[i0].1 < c.t {
            tmp[i0] = (sect_with_horizontal(pts, c.t), c.t);
        }
        if tmp[i1].1 > c.b {
            tmp[i1] = (sect_with_horizontal(pts, c.b), c.b);
        }
        let (i0, i1, mut reverse) = if pts[0].0 < pts[1].0 {
            (0, 1, false)
        } else {
            (1, 0, true)
        };
        let result: Vec<P>;
        if tmp[i1].0 <= c.l {
            result = vec![(c.l, tmp[0].1), (c.l, tmp[1].1)];
            reverse = false;
        } else if tmp[i0].0 >= c.r {
            if self.cull_right {
                return;
            }
            result = vec![(c.r, tmp[0].1), (c.r, tmp[1].1)];
            reverse = false;
        } else {
            let mut r = Vec::with_capacity(4);
            if tmp[i0].0 < c.l {
                r.push((c.l, tmp[i0].1));
                r.push((c.l, sect_clamp_with_vertical(tmp, c.l)));
            } else {
                r.push(tmp[i0]);
            }
            if tmp[i1].0 > c.r {
                r.push((c.r, sect_clamp_with_vertical(tmp, c.r)));
                r.push((c.r, tmp[i1].1));
            } else {
                r.push(tmp[i1]);
            }
            result = r;
        }
        let pts: Vec<P> = if reverse {
            result.into_iter().rev().collect()
        } else {
            result
        };
        for w in pts.windows(2) {
            self.out.push(PathEdge::Line(w[0], w[1]));
        }
    }

    /// `SkEdgeClipper::clipQuad`.
    fn clip_quad(&mut self, src: [P; 3]) {
        let (t, b) = (
            src.iter().map(|p| p.1).fold(f64::INFINITY, f64::min),
            src.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max),
        );
        if t >= self.clip.b || b <= self.clip.t {
            return;
        }
        let mut mono_y = Vec::new();
        quad_monotonic_pieces(src[0], src[1], src[2], &mut mono_y);
        for (a, m, c) in mono_y {
            let mut mono_x = Vec::new();
            quad_monotonic_pieces_x(a, m, c, &mut mono_x);
            for (a, m, c) in mono_x {
                self.clip_mono_quad([a, m, c]);
            }
        }
    }

    fn clip_mono_quad(&mut self, src: [P; 3]) {
        let c = self.clip;
        let (mut pts, mut reverse) = if src[0].1 > src[2].1 {
            ([src[2], src[1], src[0]], true)
        } else {
            (src, false)
        };
        if pts[2].1 <= c.t || pts[0].1 >= c.b {
            return;
        }
        // chop_quad_in_Y.
        if pts[0].1 < c.t {
            if let Some(t) = mono_quad_t(pts[0].1, pts[1].1, pts[2].1, c.t) {
                let (_, mut tail) = chop_quad(pts, t);
                tail[0].1 = c.t;
                tail[1].1 = tail[1].1.max(c.t);
                pts[0] = tail[0];
                pts[1] = tail[1];
            } else {
                for p in &mut pts {
                    p.1 = p.1.max(c.t);
                }
            }
        }
        if pts[2].1 > c.b {
            if let Some(t) = mono_quad_t(pts[0].1, pts[1].1, pts[2].1, c.b) {
                let (mut head, _) = chop_quad(pts, t);
                head[1].1 = head[1].1.min(c.b);
                head[2].1 = c.b;
                pts[1] = head[1];
                pts[2] = head[2];
            } else {
                for p in &mut pts {
                    p.1 = p.1.min(c.b);
                }
            }
        }
        if pts[0].0 > pts[2].0 {
            pts.swap(0, 2);
            reverse = !reverse;
        }
        if pts[2].0 <= c.l {
            self.v_line(c.l, pts[0].1, pts[2].1, reverse);
            return;
        }
        if pts[0].0 >= c.r {
            if !self.cull_right {
                self.v_line(c.r, pts[0].1, pts[2].1, reverse);
            }
            return;
        }
        if pts[0].0 < c.l {
            if let Some(t) = mono_quad_t(pts[0].0, pts[1].0, pts[2].0, c.l) {
                let (head, mut tail) = chop_quad(pts, t);
                self.v_line(c.l, head[0].1, head[2].1, reverse);
                tail[0].0 = c.l;
                tail[1].0 = tail[1].0.max(c.l);
                pts[0] = tail[0];
                pts[1] = tail[1];
            } else {
                self.v_line(c.l, pts[0].1, pts[2].1, reverse);
                return;
            }
        }
        if pts[2].0 > c.r {
            if let Some(t) = mono_quad_t(pts[0].0, pts[1].0, pts[2].0, c.r) {
                let (mut head, tail) = chop_quad(pts, t);
                head[1].0 = head[1].0.min(c.r);
                head[2].0 = c.r;
                self.quad(head, reverse);
                self.v_line(c.r, head[2].1, tail[2].1, reverse);
            } else {
                pts[1].0 = pts[1].0.min(c.r);
                pts[2].0 = pts[2].0.min(c.r);
                self.quad(pts, reverse);
            }
        } else {
            self.quad(pts, reverse);
        }
    }

    /// `SkEdgeClipper::clipCubic`.
    fn clip_cubic(&mut self, src: [P; 4]) {
        let ys = src.map(|p| p.1);
        let xs = src.map(|p| p.0);
        let (t, b) = (
            ys.iter().copied().fold(f64::INFINITY, f64::min),
            ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        if !(b > self.clip.t && t < self.clip.b) {
            return;
        }
        let limit = f64::from(1 << 22);
        let (l, r) = (
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        if l < -limit || t < -limit || r > limit || b > limit {
            self.clip_line(src[0], src[3]);
            return;
        }
        let mut mono_y = Vec::new();
        cubic_monotonic_pieces(src, &mut mono_y);
        for piece in mono_y {
            let mut mono_x = Vec::new();
            cubic_monotonic_pieces_x(piece, &mut mono_x);
            for piece in mono_x {
                self.clip_mono_cubic(piece);
            }
        }
    }

    fn clip_mono_cubic(&mut self, src: [P; 4]) {
        let c = self.clip;
        let (mut pts, mut reverse) = if src[0].1 > src[3].1 {
            ([src[3], src[2], src[1], src[0]], true)
        } else {
            (src, false)
        };
        if pts[3].1 <= c.t || pts[0].1 >= c.b {
            return;
        }
        // chop_cubic_in_Y.
        if pts[0].1 < c.t {
            let t = mono_cubic_t(pts, true, c.t);
            let (_, mut tail) = chop_cubic_at(pts, t);
            tail[0].1 = c.t;
            tail[1].1 = tail[1].1.max(c.t);
            pts[0] = tail[0];
            pts[1] = tail[1];
            pts[2] = tail[2];
        }
        if pts[3].1 > c.b {
            let t = mono_cubic_t(pts, true, c.b);
            let (mut head, _) = chop_cubic_at(pts, t);
            head[3].1 = c.b;
            head[2].1 = head[2].1.min(c.b);
            pts[1] = head[1];
            pts[2] = head[2];
            pts[3] = head[3];
        }
        if pts[0].0 > pts[3].0 {
            pts.swap(0, 3);
            pts.swap(1, 2);
            reverse = !reverse;
        }
        if pts[3].0 <= c.l {
            self.v_line(c.l, pts[0].1, pts[3].1, reverse);
            return;
        }
        if pts[0].0 >= c.r {
            if !self.cull_right {
                self.v_line(c.r, pts[0].1, pts[3].1, reverse);
            }
            return;
        }
        if pts[0].0 < c.l {
            let t = mono_cubic_t(pts, false, c.l);
            let (head, mut tail) = chop_cubic_at(pts, t);
            self.v_line(c.l, head[0].1, head[3].1, reverse);
            tail[0].0 = c.l;
            tail[1].0 = tail[1].0.max(c.l);
            pts[0] = tail[0];
            pts[1] = tail[1];
            pts[2] = tail[2];
        }
        if pts[3].0 > c.r {
            let t = mono_cubic_t(pts, false, c.r);
            let (mut head, tail) = chop_cubic_at(pts, t);
            head[3].0 = c.r;
            head[2].0 = head[2].0.min(c.r);
            self.cubic(head, reverse);
            self.v_line(c.r, head[3].1, tail[3].1, reverse);
        } else {
            self.cubic(pts, reverse);
        }
    }
}

/// Where a cubic monotonic in the coordinate reaches `target`
/// (`SkChopMonoCubicAtY`/`X`, which solve in doubles): bisection to the
/// double's precision.
fn mono_cubic_t(p: [P; 4], y: bool, target: f64) -> f64 {
    let c = |q: P| if y { q.1 } else { q.0 };
    let (a, b, cc, d) = (c(p[0]), c(p[1]), c(p[2]), c(p[3]));
    let at = |t: f64| {
        let u = 1.0 - t;
        u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * cc + t * t * t * d
    };
    let increasing = d >= a;
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..64 {
        let mid = 0.5 * (lo + hi);
        let v = at(mid);
        if (v < target) == increasing {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

// ---------------------------------------------------------------------------
// Path facts: points, bounds, convexity, rectangles (SkPathPriv.cpp)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verb {
    Move,
    Line,
    Quad,
    Conic,
    Cubic,
    Close,
}

/// The path's verbs and points as Skia stores them (f32), trailing moves
/// kept.
fn verbs_and_points(segs: &[Seg]) -> (Vec<Verb>, Vec<(f32, f32)>) {
    let mut verbs = Vec::with_capacity(segs.len());
    let mut pts = Vec::with_capacity(segs.len() * 2);
    for seg in segs {
        match *seg {
            Seg::Move(p) => {
                verbs.push(Verb::Move);
                pts.push(f32p(p));
            }
            Seg::Line(p) => {
                verbs.push(Verb::Line);
                pts.push(f32p(p));
            }
            Seg::Quad(c, p) => {
                verbs.push(Verb::Quad);
                pts.extend([f32p(c), f32p(p)]);
            }
            Seg::Conic(c, p, _) => {
                verbs.push(Verb::Conic);
                pts.extend([f32p(c), f32p(p)]);
            }
            Seg::Cubic(a, b, p) => {
                verbs.push(Verb::Cubic);
                pts.extend([f32p(a), f32p(b), f32p(p)]);
            }
            Seg::Close => verbs.push(Verb::Close),
        }
    }
    (verbs, pts)
}

fn pts_in_verb(v: Verb) -> usize {
    match v {
        Verb::Move | Verb::Line => 1,
        Verb::Quad | Verb::Conic => 2,
        Verb::Cubic => 3,
        Verb::Close => 0,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DirChange {
    Left,
    Right,
    Straight,
    Backwards,
    Unknown,
    Invalid,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FirstDirection {
    Cw,
    Ccw,
    Unknown,
}

/// `Convexicator`.
struct Convexicator {
    first_pt: (f32, f32),
    first_vec: (f32, f32),
    last_pt: (f32, f32),
    last_vec: (f32, f32),
    expected_dir: DirChange,
    first_direction: FirstDirection,
    reversals: i32,
    is_finite: bool,
}

impl Convexicator {
    fn new() -> Self {
        Convexicator {
            first_pt: (0.0, 0.0),
            first_vec: (0.0, 0.0),
            last_pt: (0.0, 0.0),
            last_vec: (0.0, 0.0),
            expected_dir: DirChange::Invalid,
            first_direction: FirstDirection::Unknown,
            reversals: 0,
            is_finite: true,
        }
    }

    fn set_move_pt(&mut self, p: (f32, f32)) {
        self.first_pt = p;
        self.last_pt = p;
        self.expected_dir = DirChange::Invalid;
    }

    fn add_pt(&mut self, p: (f32, f32)) -> bool {
        if self.last_pt == p {
            return true;
        }
        let v = (p.0 - self.last_pt.0, p.1 - self.last_pt.1);
        if self.first_pt == self.last_pt
            && self.expected_dir == DirChange::Invalid
            && self.last_vec == (0.0, 0.0)
        {
            self.last_vec = v;
            self.first_vec = v;
        } else if !self.add_vec(v) {
            return false;
        }
        self.last_pt = p;
        true
    }

    fn direction_change(&self, v: (f32, f32)) -> DirChange {
        let cross = self.last_vec.0 * v.1 - self.last_vec.1 * v.0;
        if !cross.is_finite() {
            return DirChange::Unknown;
        }
        if cross == 0.0 {
            let dot = self.last_vec.0 * v.0 + self.last_vec.1 * v.1;
            return if dot < 0.0 {
                DirChange::Backwards
            } else {
                DirChange::Straight
            };
        }
        if cross > 0.0 {
            DirChange::Right
        } else {
            DirChange::Left
        }
    }

    fn add_vec(&mut self, v: (f32, f32)) -> bool {
        match self.direction_change(v) {
            dir @ (DirChange::Left | DirChange::Right) => {
                if self.expected_dir == DirChange::Invalid {
                    self.expected_dir = dir;
                    self.first_direction = if dir == DirChange::Right {
                        FirstDirection::Cw
                    } else {
                        FirstDirection::Ccw
                    };
                } else if dir != self.expected_dir {
                    self.first_direction = FirstDirection::Unknown;
                    return false;
                }
                self.last_vec = v;
                true
            }
            DirChange::Straight => true,
            DirChange::Backwards => {
                self.last_vec = v;
                self.reversals += 1;
                self.reversals < 3
            }
            DirChange::Unknown => {
                self.is_finite = false;
                false
            }
            DirChange::Invalid => false,
        }
    }

    fn close(&mut self) -> bool {
        let first = self.first_pt;
        let first_vec = self.first_vec;
        self.add_pt(first) && self.add_vec(first_vec)
    }

    fn is_concave_by_sign(points: &[(f32, f32)]) -> bool {
        if points.len() <= 3 {
            return false;
        }
        let sign = |x: f32| i32::from(x < 0.0);
        let mut idx = 1;
        let mut curr = points[0];
        let first = points[0];
        let (mut dxes, mut dyes) = (0, 0);
        let (mut last_sx, mut last_sy) = (2, 2);
        for outer in 0..2 {
            loop {
                let p = if outer == 0 {
                    if idx >= points.len() {
                        break;
                    }
                    points[idx]
                } else {
                    first
                };
                let v = (p.0 - curr.0, p.1 - curr.1);
                if v != (0.0, 0.0) {
                    if !(v.0.is_finite() && v.1.is_finite()) {
                        return true;
                    }
                    let (sx, sy) = (sign(v.0), sign(v.1));
                    dxes += i32::from(sx != last_sx);
                    dyes += i32::from(sy != last_sy);
                    if dxes > 3 || dyes > 3 {
                        return true;
                    }
                    last_sx = sx;
                    last_sy = sy;
                }
                curr = p;
                idx += 1;
                if outer == 1 {
                    break;
                }
            }
        }
        false
    }
}

/// `SkPathPriv::ComputeConvexity`: whether Skia treats the path as convex.
fn is_convex(verbs: &[Verb], pts: &[(f32, f32)]) -> bool {
    let mut vcount = verbs.len();
    let mut pcount = pts.len();
    while vcount > 0 && verbs[vcount - 1] == Verb::Move {
        vcount -= 1;
        pcount -= 1;
    }
    let (verbs, pts) = (&verbs[..vcount], &pts[..pcount]);
    if verbs.is_empty() {
        return true;
    }
    if Convexicator::is_concave_by_sign(pts) {
        return false;
    }
    let mut contour_count = 0;
    let mut needs_close = false;
    let mut state = Convexicator::new();
    let mut pi = 0;
    let mut last = (0.0f32, 0.0f32);
    for &verb in verbs {
        let n = pts_in_verb(verb);
        let seg: Vec<(f32, f32)> = match verb {
            Verb::Move => vec![pts[pi]],
            Verb::Close => vec![],
            _ => std::iter::once(last)
                .chain(pts[pi..pi + n].iter().copied())
                .collect(),
        };
        if contour_count == 0 {
            if verb == Verb::Move {
                state.set_move_pt(seg[0]);
            } else {
                contour_count += 1;
                needs_close = true;
            }
        }
        if contour_count == 1 {
            if verb == Verb::Close || verb == Verb::Move {
                if !state.close() {
                    return false;
                }
                needs_close = false;
                contour_count += 1;
            } else {
                for p in &seg[1..=n] {
                    if !state.add_pt(*p) {
                        return false;
                    }
                }
            }
        } else if verb != Verb::Move {
            return false;
        }
        if n > 0 {
            last = pts[pi + n - 1];
        }
        pi += n;
    }
    if needs_close && !state.close() {
        return false;
    }
    !(state.first_direction == FirstDirection::Unknown && state.reversals >= 3)
}

fn rect_make_dir(dx: f32, dy: f32) -> i32 {
    i32::from(dx != 0.0) | (i32::from(dx > 0.0 || dy > 0.0) << 1)
}

/// `SkPathPriv::IsRectContour` (not partial): the path's rectangle when it
/// is one closed axis-aligned rectangle of lines.
fn is_rect(verbs: &[Verb], pts: &[(f32, f32)]) -> Option<[f32; 4]> {
    rect_contour_of(verbs, pts).map(|(r, _, _)| r)
}

/// `IsRectContour` with the closed flag (an explicit close) and the
/// direction (clockwise).
fn rect_contour_of(verbs: &[Verb], pts: &[(f32, f32)]) -> Option<([f32; 4], bool, bool)> {
    if verbs
        .iter()
        .any(|v| matches!(v, Verb::Quad | Verb::Conic | Verb::Cubic))
        || pts.len() < 4
        || verbs.len() < 4
    {
        return None;
    }
    // trivial_rect is an optimisation of the same answer.
    let mut corners = 0;
    let mut line_start = (0.0f32, 0.0f32);
    let mut first_pt: Option<usize> = None;
    let mut last_pt: Option<usize> = None;
    let mut first_corner = (0.0f32, 0.0f32);
    let mut third_corner = (0.0f32, 0.0f32);
    let mut directions = [-1i32; 5];
    let mut closed_or_moved = false;
    let mut auto_close = false;
    let mut pi = 0usize;
    for &verb in verbs {
        match verb {
            Verb::Close | Verb::Line => {
                if verb == Verb::Close {
                    auto_close = true;
                } else {
                    last_pt = Some(pi);
                }
                let line_end = if verb == Verb::Close {
                    pts[first_pt?]
                } else {
                    let p = pts[pi];
                    pi += 1;
                    p
                };
                let delta = (line_end.0 - line_start.0, line_end.1 - line_start.1);
                if delta.0 != 0.0 && delta.1 != 0.0 {
                    return None;
                }
                if !(delta.0.is_finite() && delta.1.is_finite()) {
                    return None;
                }
                if line_start == line_end {
                    continue;
                }
                let next_direction = rect_make_dir(delta.0, delta.1);
                if corners == 0 {
                    directions[0] = next_direction;
                    corners = 1;
                    closed_or_moved = false;
                    line_start = line_end;
                    continue;
                }
                if closed_or_moved {
                    return None;
                }
                if auto_close && next_direction == directions[0] {
                    continue;
                }
                closed_or_moved = auto_close;
                if directions[corners - 1] == next_direction {
                    if corners == 3 && verb == Verb::Line {
                        third_corner = line_end;
                    }
                    line_start = line_end;
                    continue;
                }
                directions[corners] = next_direction;
                corners += 1;
                match corners {
                    2 => first_corner = line_start,
                    3 => {
                        if (directions[0] ^ directions[2]) != 2 {
                            return None;
                        }
                        third_corner = line_end;
                    }
                    4 => {
                        if (directions[1] ^ directions[3]) != 2 {
                            return None;
                        }
                    }
                    _ => return None,
                }
                line_start = line_end;
            }
            Verb::Move => {
                if corners == 0 {
                    first_pt = Some(pi);
                } else {
                    let (f, l) = (pts[first_pt?], pts[last_pt?]);
                    if f.0 - l.0 != 0.0 && f.1 - l.1 != 0.0 {
                        return None;
                    }
                }
                line_start = pts[pi];
                pi += 1;
                closed_or_moved = true;
            }
            Verb::Quad | Verb::Conic | Verb::Cubic => return None,
        }
    }
    if !(3..=4).contains(&corners) {
        return None;
    }
    let (f, l) = (pts[first_pt?], pts[last_pt?]);
    if f.0 - l.0 != 0.0 && f.1 - l.1 != 0.0 {
        return None;
    }
    let (l, r) = (
        first_corner.0.min(third_corner.0),
        first_corner.0.max(third_corner.0),
    );
    let (t, b) = (
        first_corner.1.min(third_corner.1),
        first_corner.1.max(third_corner.1),
    );
    let cw = directions[0] == ((directions[1] + 1) & 3);
    Some(([l, t, r, b], auto_close, cw))
}

// ---------------------------------------------------------------------------
// Blitters (SkScan_AAAPath.cpp additive blitters, the real blitter)

/// Where the coverage goes: the draw's own mask over `rect` (the
/// intersection of the path's bounds and the clip bounds).
struct Target {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    data: Vec<u8>,
}

impl Target {
    fn composite(&mut self, x: i32, y: i32, alpha: u8) {
        if alpha == 0
            || x < self.left
            || y < self.top
            || x >= self.left + self.width
            || y >= self.top + self.height
        {
            return;
        }
        let i = ((y - self.top) * self.width + (x - self.left)) as usize;
        let d = u32::from(self.data[i]);
        let a = u32::from(alpha);
        // Source-over of coverage: a + d - a*d/255.
        let prod = a * d + 128;
        self.data[i] = (a + d - ((prod + (prod >> 8)) >> 8)).min(255) as u8;
    }

    /// The real blitter's `blitV`.
    fn blit_v(&mut self, x: i32, y: i32, height: i32, alpha: u8) {
        for j in 0..height {
            self.composite(x, y + j, alpha);
        }
    }

    fn blit_rect(&mut self, x: i32, y: i32, width: i32, height: i32) {
        for j in 0..height {
            for i in 0..width {
                self.composite(x + i, y + j, 255);
            }
        }
    }

    /// `SkBlitter::blitAntiRect`.
    fn blit_anti_rect(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        left_alpha: u8,
        right_alpha: u8,
    ) {
        if left_alpha > 0 {
            self.blit_v(x, y, height, left_alpha);
        }
        if width > 0 {
            self.blit_rect(x + 1, y, width, height);
        }
        if right_alpha > 0 {
            self.blit_v(x + 1 + width, y, height, right_alpha);
        }
    }
}

/// `CatchOverflow`: 256 becomes 255.
fn catch_overflow(a: i32) -> u8 {
    (a - (a >> 8)) as u8
}

fn add_alpha(alpha: &mut u8, delta: u8) {
    *alpha = catch_overflow(i32::from(*alpha) + i32::from(delta));
}

fn safely_add_alpha(alpha: &mut u8, delta: u8) {
    *alpha = (i32::from(*alpha) + i32::from(delta)).min(255) as u8;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlitterKind {
    /// `MaskAdditiveBlitter`: paths up to 32 px wide and 1024 bytes.
    Mask,
    /// `RunBasedAdditiveBlitter`: convex paths.
    RunBased,
    /// `SafeRLEAdditiveBlitter`: the rest, alphas clamped.
    SafeRle,
}

struct Additive {
    kind: BlitterKind,
    target: Target,
    // Mask: the mask over the path bounds, one slack byte either side.
    mask_left: i32,
    mask_top: i32,
    mask_width: i32,
    mask: Vec<u8>,
    // Run-based: the row being accumulated, over [rle_left, rle_left + rle_width).
    rle_left: i32,
    rle_width: i32,
    rle_top: i32,
    curr_y: i32,
    row: Vec<u8>,
}

impl Additive {
    fn mask_index(&self, x: i32, y: i32) -> usize {
        ((y - self.mask_top) * (self.mask_width + 2) + (x - self.mask_left + 1)) as usize
    }

    fn mask_add(&mut self, x: i32, y: i32, alpha: u8) {
        let i = self.mask_index(x, y);
        add_alpha(&mut self.mask[i], alpha);
    }

    fn mask_set(&mut self, x: i32, y: i32, alpha: u8) {
        let i = self.mask_index(x, y);
        self.mask[i] = alpha;
    }

    /// `snapAlpha` and the flush of a run-based row to the real blitter.
    fn flush(&mut self) {
        if self.kind == BlitterKind::Mask {
            return;
        }
        if self.curr_y >= self.rle_top {
            for i in 0..self.rle_width as usize {
                let a = self.row[i];
                let a = if a > 247 {
                    255
                } else if a < 8 {
                    0
                } else {
                    a
                };
                self.target
                    .composite(self.rle_left + i as i32, self.curr_y, a);
                self.row[i] = 0;
            }
            self.curr_y = self.rle_top - 1;
        }
    }

    fn check_y(&mut self, y: i32) {
        if y != self.curr_y {
            self.flush();
            self.curr_y = y;
        }
    }

    fn rle_add(&mut self, x: i32, alpha: u8) {
        let i = (x - self.rle_left) as usize;
        if self.kind == BlitterKind::SafeRle {
            safely_add_alpha(&mut self.row[i], alpha);
        } else {
            add_alpha(&mut self.row[i], alpha);
        }
    }

    fn flush_if_y_changed(&mut self, y: Fixed, next_y: Fixed) {
        if self.kind != BlitterKind::Mask && fixed_floor_to_int(y) != fixed_floor_to_int(next_y) {
            self.flush();
        }
    }

    /// `blitAntiH(x, y, alpha)`.
    fn blit_anti_h1(&mut self, x: i32, y: i32, alpha: u8) {
        match self.kind {
            BlitterKind::Mask => self.mask_add(x, y, alpha),
            _ => {
                self.check_y(y);
                if x >= self.rle_left && x < self.rle_left + self.rle_width {
                    self.rle_add(x, alpha);
                }
            }
        }
    }

    /// `blitAntiH(x, y, width, alpha)`.
    fn blit_anti_hn(&mut self, x: i32, y: i32, width: i32, alpha: u8) {
        match self.kind {
            BlitterKind::Mask => {
                for i in 0..width {
                    self.mask_add(x + i, y, alpha);
                }
            }
            _ => {
                self.check_y(y);
                if x >= self.rle_left && x + width <= self.rle_left + self.rle_width {
                    for i in 0..width {
                        self.rle_add(x + i, alpha);
                    }
                }
            }
        }
    }

    /// `blitAntiH(x, y, antialias[], len)` (run-based; the mask blitter
    /// writes its row directly instead).
    fn blit_anti_h_array(&mut self, x: i32, y: i32, alphas: &[u8]) {
        self.check_y(y);
        for (i, &a) in alphas.iter().enumerate() {
            let px = x + i as i32;
            if px >= self.rle_left && px < self.rle_left + self.rle_width {
                self.rle_add(px, a);
            }
        }
    }

    /// The real blitter's `blitV` (the mask blitter sets its own alpha).
    fn real_blit_v(&mut self, x: i32, y: i32, height: i32, alpha: u8) {
        match self.kind {
            BlitterKind::Mask => {
                if alpha == 0 {
                    return;
                }
                for j in 0..height {
                    self.mask_set(x, y + j, alpha);
                }
            }
            _ => self.target.blit_v(x, y, height, alpha),
        }
    }

    fn real_blit_rect(&mut self, x: i32, y: i32, width: i32, height: i32) {
        match self.kind {
            BlitterKind::Mask => {
                for j in 0..height {
                    for i in 0..width {
                        self.mask_set(x + i, y + j, 255);
                    }
                }
            }
            _ => self.target.blit_rect(x, y, width, height),
        }
    }

    fn real_blit_anti_rect(&mut self, x: i32, y: i32, width: i32, height: i32, la: u8, ra: u8) {
        match self.kind {
            BlitterKind::Mask => {
                self.real_blit_v(x, y, height, la);
                self.real_blit_v(x + 1 + width, y, height, ra);
                self.real_blit_rect(x + 1, y, width, height);
            }
            _ => self.target.blit_anti_rect(x, y, width, height, la, ra),
        }
    }

    fn real_blit_anti_h(&mut self, x: i32, y: i32, alphas: &[u8]) {
        for (i, &a) in alphas.iter().enumerate() {
            self.target.composite(x + i as i32, y, a);
        }
    }

    fn finish(mut self) -> Target {
        match self.kind {
            BlitterKind::Mask => {
                // blitMask(fMask, fClipRect): the mask within the target.
                let rows = self.mask.len() as i32 / (self.mask_width + 2);
                for j in 0..rows {
                    for i in 0..self.mask_width {
                        let a = self.mask[self.mask_index(self.mask_left + i, self.mask_top + j)];
                        self.target
                            .composite(self.mask_left + i, self.mask_top + j, a);
                    }
                }
            }
            _ => self.flush(),
        }
        self.target
    }
}

// ---------------------------------------------------------------------------
// Trapezoid coverage (SkScan_AAAPath.cpp)

fn trapezoid_to_alpha(l1: Fixed, l2: Fixed) -> u8 {
    let area = (l1 + l2) / 2;
    (area >> 8) as u8
}

fn partial_triangle_to_alpha(a: Fixed, b: Fixed) -> u8 {
    let area = (a >> 11).wrapping_mul(a >> 11).wrapping_mul(b >> 11);
    ((area >> 8) & 0xFF) as u8
}

fn get_partial_alpha_h(alpha: u8, partial_height: Fixed) -> u8 {
    fixed_round_to_int(i32::from(alpha).wrapping_mul(partial_height)) as u8
}

fn get_partial_alpha(alpha: u8, full_alpha: u8) -> u8 {
    ((u32::from(alpha) * u32::from(full_alpha)) >> 8) as u8
}

fn fixed_to_alpha(f: Fixed) -> u8 {
    get_partial_alpha_h(0xFF, f)
}

fn approximate_intersection(mut l1: Fixed, mut r1: Fixed, mut l2: Fixed, mut r2: Fixed) -> Fixed {
    if l1 > r1 {
        std::mem::swap(&mut l1, &mut r1);
    }
    if l2 > r2 {
        std::mem::swap(&mut l2, &mut r2);
    }
    (l1.max(l2) + r1.min(r2)) / 2
}

fn compute_alpha_above_line(alphas: &mut [u8], l: Fixed, r: Fixed, dy: Fixed, full_alpha: u8) {
    let big_r = fixed_ceil_to_int(r);
    if big_r == 0 {
    } else if big_r == 1 {
        alphas[0] = get_partial_alpha((((big_r << 17) - l - r) >> 9) as u8, full_alpha);
    } else {
        let first = FIXED1 - l;
        let last = r - ((big_r - 1) << 16);
        let first_h = fixed_mul(first, dy);
        alphas[0] = (fixed_mul(first, first_h) >> 9) as u8;
        let mut alpha16 = sat_add(first_h, dy >> 1);
        for a in alphas.iter_mut().take((big_r - 1) as usize).skip(1) {
            *a = (alpha16 >> 8) as u8;
            alpha16 = sat_add(alpha16, dy);
        }
        alphas[(big_r - 1) as usize] = full_alpha.wrapping_sub(partial_triangle_to_alpha(last, dy));
    }
}

fn compute_alpha_below_line(alphas: &mut [u8], l: Fixed, r: Fixed, dy: Fixed, full_alpha: u8) {
    let big_r = fixed_ceil_to_int(r);
    if big_r == 0 {
    } else if big_r == 1 {
        alphas[0] = get_partial_alpha(trapezoid_to_alpha(l, r), full_alpha);
    } else {
        let first = FIXED1 - l;
        let last = r - ((big_r - 1) << 16);
        let last_h = fixed_mul(last, dy);
        alphas[(big_r - 1) as usize] = (fixed_mul(last, last_h) >> 9) as u8;
        let mut alpha16 = sat_add(last_h, dy >> 1);
        let mut i = big_r - 2;
        while i > 0 {
            alphas[i as usize] = ((alpha16 >> 8) & 0xFF) as u8;
            alpha16 = sat_add(alpha16, dy);
            i -= 1;
        }
        alphas[0] = full_alpha.wrapping_sub(partial_triangle_to_alpha(first, dy));
    }
}

/// The mask row the walkers write straight into (mask blitter only).
fn mask_row_write(b: &mut Additive, y: i32, x: i32, f: impl FnOnce(&mut u8)) {
    let i = b.mask_index(x, y);
    f(&mut b.mask[i]);
}

#[allow(clippy::too_many_arguments)]
fn blit_single_alpha(
    b: &mut Additive,
    y: i32,
    x: i32,
    alpha: u8,
    full_alpha: u8,
    use_mask: bool,
    no_real: bool,
) {
    if use_mask {
        if full_alpha == 0xFF && !no_real {
            mask_row_write(b, y, x, |m| *m = alpha);
        } else {
            let a = get_partial_alpha(alpha, full_alpha);
            mask_row_write(b, y, x, |m| safely_add_alpha(m, a));
        }
    } else if full_alpha == 0xFF && !no_real {
        b.real_blit_v(x, y, 1, alpha);
    } else {
        b.blit_anti_h1(x, y, get_partial_alpha(alpha, full_alpha));
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_two_alphas(
    b: &mut Additive,
    y: i32,
    x: i32,
    a1: u8,
    a2: u8,
    full_alpha: u8,
    use_mask: bool,
    no_real: bool,
) {
    if use_mask {
        mask_row_write(b, y, x, |m| safely_add_alpha(m, a1));
        mask_row_write(b, y, x + 1, |m| safely_add_alpha(m, a2));
    } else if full_alpha == 0xFF && !no_real {
        b.real_blit_anti_h(x, y, &[a1, a2]);
    } else {
        b.blit_anti_h1(x, y, a1);
        b.blit_anti_h1(x + 1, y, a2);
    }
}

fn blit_full_alpha(
    b: &mut Additive,
    y: i32,
    x: i32,
    len: i32,
    full_alpha: u8,
    use_mask: bool,
    no_real: bool,
) {
    if use_mask {
        for i in 0..len {
            mask_row_write(b, y, x + i, |m| safely_add_alpha(m, full_alpha));
        }
    } else if full_alpha == 0xFF && !no_real {
        b.real_blit_anti_h(x, y, &vec![255; len.max(0) as usize]);
    } else {
        b.blit_anti_hn(x, y, len, full_alpha);
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_aaa_trapezoid_row(
    b: &mut Additive,
    y: i32,
    ul: Fixed,
    ur: Fixed,
    ll: Fixed,
    lr: Fixed,
    l_dy: Fixed,
    r_dy: Fixed,
    full_alpha: u8,
    use_mask: bool,
    no_real: bool,
) {
    let big_l = fixed_floor_to_int(ul);
    let big_r = fixed_ceil_to_int(lr);
    let len = big_r - big_l;
    if len == 1 {
        let alpha = trapezoid_to_alpha(ur - ul, lr - ll);
        blit_single_alpha(b, y, big_l, alpha, full_alpha, use_mask, no_real);
        return;
    }
    if len <= 0 {
        return;
    }
    let len_u = len as usize;
    let mut alphas = vec![full_alpha; len_u + 1];
    let mut temp = vec![0u8; len_u + 1];

    let u_l = fixed_floor_to_int(ul);
    let l_l = fixed_ceil_to_int(ll);
    if u_l + 2 == l_l {
        let first = int_to_fixed(u_l) + FIXED1 - ul;
        let second = ll - ul - first;
        let a1 = full_alpha.wrapping_sub(partial_triangle_to_alpha(first, l_dy));
        let a2 = partial_triangle_to_alpha(second, l_dy);
        alphas[0] = alphas[0].saturating_sub(a1);
        alphas[1] = alphas[1].saturating_sub(a2);
    } else {
        let off = (u_l - big_l) as usize;
        compute_alpha_below_line(
            &mut temp[off..],
            ul - int_to_fixed(u_l),
            ll - int_to_fixed(u_l),
            l_dy,
            full_alpha,
        );
        for i in u_l..l_l {
            let k = (i - big_l) as usize;
            alphas[k] = alphas[k].saturating_sub(temp[k]);
        }
    }

    let u_r = fixed_floor_to_int(ur);
    let l_r = fixed_ceil_to_int(lr);
    if u_r + 2 == l_r {
        let first = int_to_fixed(u_r) + FIXED1 - ur;
        let second = lr - ur - first;
        let a1 = partial_triangle_to_alpha(first, r_dy);
        let a2 = full_alpha.wrapping_sub(partial_triangle_to_alpha(second, r_dy));
        alphas[len_u - 2] = alphas[len_u - 2].saturating_sub(a1);
        alphas[len_u - 1] = alphas[len_u - 1].saturating_sub(a2);
    } else {
        let off = (u_r - big_l) as usize;
        for t in temp.iter_mut() {
            *t = 0;
        }
        compute_alpha_above_line(
            &mut temp[off..],
            ur - int_to_fixed(u_r),
            lr - int_to_fixed(u_r),
            r_dy,
            full_alpha,
        );
        for i in u_r..l_r {
            let k = (i - big_l) as usize;
            alphas[k] = alphas[k].saturating_sub(temp[k]);
        }
    }

    if use_mask {
        for i in 0..len {
            let a = alphas[i as usize];
            mask_row_write(b, y, big_l + i, |m| safely_add_alpha(m, a));
        }
    } else if full_alpha == 0xFF && !no_real {
        b.real_blit_anti_h(big_l, y, &alphas[..len_u]);
    } else {
        b.blit_anti_h_array(big_l, y, &alphas[..len_u]);
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_trapezoid_row(
    b: &mut Additive,
    y: i32,
    mut ul: Fixed,
    mut ur: Fixed,
    mut ll: Fixed,
    mut lr: Fixed,
    l_dy: Fixed,
    r_dy: Fixed,
    full_alpha: u8,
    use_mask: bool,
    no_real: bool,
) {
    if ul > ur {
        return;
    }
    if ll > lr {
        let v = approximate_intersection(ul, ll, ur, lr);
        ll = v;
        lr = v;
    }
    if ul == ur && ll == lr {
        return;
    }
    if ul > ll {
        std::mem::swap(&mut ul, &mut ll);
    }
    if ur > lr {
        std::mem::swap(&mut ur, &mut lr);
    }
    let join_left = fixed_ceil_to_fixed(ll);
    let join_rite = fixed_floor_to_fixed(ur);
    if join_left <= join_rite {
        if ul < join_left {
            let len = fixed_ceil_to_int(join_left - ul);
            if len == 1 {
                let alpha = trapezoid_to_alpha(join_left - ul, join_left - ll);
                blit_single_alpha(b, y, ul >> 16, alpha, full_alpha, use_mask, no_real);
            } else if len == 2 {
                let first = join_left - FIXED1 - ul;
                let second = ll - ul - first;
                let a1 = partial_triangle_to_alpha(first, l_dy);
                let a2 = full_alpha.wrapping_sub(partial_triangle_to_alpha(second, l_dy));
                blit_two_alphas(b, y, ul >> 16, a1, a2, full_alpha, use_mask, no_real);
            } else {
                blit_aaa_trapezoid_row(
                    b, y, ul, join_left, ll, join_left, l_dy, MAX_S32, full_alpha, use_mask,
                    no_real,
                );
            }
        }
        if join_left < join_rite {
            blit_full_alpha(
                b,
                y,
                fixed_floor_to_int(join_left),
                fixed_floor_to_int(join_rite - join_left),
                full_alpha,
                use_mask,
                no_real,
            );
        }
        if lr > join_rite {
            let len = fixed_ceil_to_int(lr - join_rite);
            if len == 1 {
                let alpha = trapezoid_to_alpha(ur - join_rite, lr - join_rite);
                blit_single_alpha(b, y, join_rite >> 16, alpha, full_alpha, use_mask, no_real);
            } else if len == 2 {
                let first = join_rite + FIXED1 - ur;
                let second = lr - ur - first;
                let a1 = full_alpha.wrapping_sub(partial_triangle_to_alpha(first, r_dy));
                let a2 = partial_triangle_to_alpha(second, r_dy);
                blit_two_alphas(b, y, join_rite >> 16, a1, a2, full_alpha, use_mask, no_real);
            } else {
                blit_aaa_trapezoid_row(
                    b, y, join_rite, ur, join_rite, lr, MAX_S32, r_dy, full_alpha, use_mask,
                    no_real,
                );
            }
        }
    } else {
        blit_aaa_trapezoid_row(
            b, y, ul, ur, ll, lr, l_dy, r_dy, full_alpha, use_mask, no_real,
        );
    }
}

// ---------------------------------------------------------------------------
// Edge list (SkScanPriv.h)

const HEAD: usize = 0;
const TAIL: usize = 1;

fn remove_edge(e: &mut [Edge], i: usize) {
    let (p, n) = (e[i].prev, e[i].next);
    e[p].next = n;
    e[n].prev = p;
}

fn insert_edge_after(e: &mut [Edge], i: usize, after: usize) {
    e[i].prev = after;
    e[i].next = e[after].next;
    let n = e[after].next;
    e[n].prev = i;
    e[after].next = i;
}

fn backward_insert_edge_based_on_x(e: &mut [Edge], i: usize) {
    let x = e[i].x;
    let mut prev = e[i].prev;
    while e[prev].prev != usize::MAX && e[prev].x > x {
        prev = e[prev].prev;
    }
    if e[prev].next != i {
        remove_edge(e, i);
        insert_edge_after(e, i, prev);
    }
}

fn backward_insert_start(e: &[Edge], mut prev: usize, x: Fixed) -> usize {
    while e[prev].prev != usize::MAX && e[prev].x > x {
        prev = e[prev].prev;
    }
    prev
}

// ---------------------------------------------------------------------------
// Walkers

fn is_smooth_enough_edge(e: &[Edge], this: usize, next: usize) -> bool {
    let t = &e[this];
    if t.curve_count < 0 {
        let dd = t.curve_shift;
        (t.cdx.abs() >> 1) >= (t.cddx.abs() >> dd)
            && (t.cdy.abs() >> 1) >= (t.cddy.abs() >> dd)
            && ((t.cdy.wrapping_sub(t.cddy >> dd)) >> t.to_fixed_shift) >= FIXED1
    } else if t.curve_count > 0 {
        (t.qdx.abs() >> 1) >= t.qddx.abs()
            && (t.qdy.abs() >> 1) >= t.qddy.abs()
            && ((t.qdy.wrapping_sub(t.qddy)) >> t.curve_shift) >= FIXED1
    } else {
        let n = &e[next];
        sat_sub(n.dx, t.dx).abs() <= FIXED1 && n.lower_y.wrapping_sub(n.upper_y) >= FIXED1
    }
}

fn is_smooth_enough(e: &[Edge], left: usize, rite: usize, mut curr: usize, stop_y: i32) -> bool {
    if e[curr].upper_y >= stop_y.wrapping_shl(16) {
        return false;
    }
    if e[left].lower_y.saturating_add(FIXED1) < e[rite].lower_y {
        return is_smooth_enough_edge(e, left, curr);
    } else if e[left].lower_y > e[rite].lower_y.saturating_add(FIXED1) {
        return is_smooth_enough_edge(e, rite, curr);
    }
    let mut next_curr = e[curr].next;
    if e[next_curr].upper_y >= stop_y.wrapping_shl(16) {
        return false;
    }
    if e[next_curr].upper_x < e[curr].upper_x {
        std::mem::swap(&mut curr, &mut next_curr);
    }
    is_smooth_enough_edge(e, left, curr) && is_smooth_enough_edge(e, rite, next_curr)
}

#[allow(clippy::too_many_arguments)]
fn walk_convex_edges(
    e: &mut [Edge],
    b: &mut Additive,
    stop_y: i32,
    left_bound: Fixed,
    rite_bound: Fixed,
    use_mask: bool,
) {
    let mut left_e = e[HEAD].next;
    let mut rite_e = e[left_e].next;
    let mut curr_e = e[rite_e].next;
    let mut y = e[left_e].upper_y.max(e[rite_e].upper_y);
    'walk: loop {
        while e[left_e].lower_y <= y {
            if !e[left_e].update() {
                if fixed_floor_to_int(e[curr_e].upper_y) >= stop_y {
                    break 'walk;
                }
                left_e = curr_e;
                curr_e = e[curr_e].next;
            }
        }
        while e[rite_e].lower_y <= y {
            if !e[rite_e].update() {
                if fixed_floor_to_int(e[curr_e].upper_y) >= stop_y {
                    break 'walk;
                }
                rite_e = curr_e;
                curr_e = e[curr_e].next;
            }
        }
        if fixed_floor_to_int(y) >= stop_y {
            break;
        }
        e[left_e].go_y(y);
        e[rite_e].go_y(y);
        if e[left_e].x > e[rite_e].x || (e[left_e].x == e[rite_e].x && e[left_e].dx > e[rite_e].dx)
        {
            std::mem::swap(&mut left_e, &mut rite_e);
        }
        let mut local_bot = e[left_e].lower_y.min(e[rite_e].lower_y);
        if is_smooth_enough(e, left_e, rite_e, curr_e, stop_y) {
            local_bot = fixed_ceil_to_fixed(local_bot);
        }
        local_bot = local_bot.min(int_to_fixed(stop_y));

        let mut left = left_bound.max(e[left_e].x);
        let d_left = e[left_e].dx;
        let mut rite = rite_bound.min(e[rite_e].x);
        let d_rite = e[rite_e].dx;
        if (d_left | d_rite) == 0 {
            let full_left = fixed_ceil_to_int(left);
            let full_rite = fixed_floor_to_int(rite);
            let partial_left = int_to_fixed(full_left) - left;
            let partial_rite = rite - int_to_fixed(full_rite);
            let full_top = fixed_ceil_to_int(y);
            let full_bot = fixed_floor_to_int(local_bot);
            let mut partial_top = int_to_fixed(full_top) - y;
            let mut partial_bot = local_bot - int_to_fixed(full_bot);
            if full_top > full_bot {
                partial_top -= FIXED1 - partial_bot;
                partial_bot = 0;
            }
            if full_rite >= full_left {
                if partial_top > 0 {
                    if partial_left > 0 {
                        b.blit_anti_h1(
                            full_left - 1,
                            full_top - 1,
                            fixed_to_alpha(fixed_mul(partial_top, partial_left)),
                        );
                    }
                    b.blit_anti_hn(
                        full_left,
                        full_top - 1,
                        full_rite - full_left,
                        fixed_to_alpha(partial_top),
                    );
                    if partial_rite > 0 {
                        b.blit_anti_h1(
                            full_rite,
                            full_top - 1,
                            fixed_to_alpha(fixed_mul(partial_top, partial_rite)),
                        );
                    }
                    b.flush_if_y_changed(y, y + partial_top);
                }
                if full_bot > full_top
                    && (full_rite > full_left
                        || fixed_to_alpha(partial_left) > 0
                        || fixed_to_alpha(partial_rite) > 0)
                {
                    b.real_blit_anti_rect(
                        full_left - 1,
                        full_top,
                        full_rite - full_left,
                        full_bot - full_top,
                        fixed_to_alpha(partial_left),
                        fixed_to_alpha(partial_rite),
                    );
                }
                if partial_bot > 0 {
                    if partial_left > 0 {
                        b.blit_anti_h1(
                            full_left - 1,
                            full_bot,
                            fixed_to_alpha(fixed_mul(partial_bot, partial_left)),
                        );
                    }
                    b.blit_anti_hn(
                        full_left,
                        full_bot,
                        full_rite - full_left,
                        fixed_to_alpha(partial_bot),
                    );
                    if partial_rite > 0 {
                        b.blit_anti_h1(
                            full_rite,
                            full_bot,
                            fixed_to_alpha(fixed_mul(partial_bot, partial_rite)),
                        );
                    }
                }
            } else {
                let width = rite - left;
                if width > 0 {
                    if partial_top > 0 {
                        b.blit_anti_hn(
                            full_left - 1,
                            full_top - 1,
                            1,
                            fixed_to_alpha(fixed_mul(partial_top, width)),
                        );
                        b.flush_if_y_changed(y, y + partial_top);
                    }
                    if full_bot > full_top {
                        b.real_blit_v(
                            full_left - 1,
                            full_top,
                            full_bot - full_top,
                            fixed_to_alpha(width),
                        );
                    }
                    if partial_bot > 0 {
                        b.blit_anti_hn(
                            full_left - 1,
                            full_bot,
                            1,
                            fixed_to_alpha(fixed_mul(partial_bot, width)),
                        );
                    }
                }
            }
            y = local_bot;
        } else {
            const SNAP_DIGIT: Fixed = FIXED1 >> 4;
            const SNAP_HALF: Fixed = SNAP_DIGIT >> 1;
            const SNAP_MASK: Fixed = -1 ^ (SNAP_DIGIT - 1);
            left += SNAP_HALF;
            rite += SNAP_HALF;
            let mut count = fixed_ceil_to_int(local_bot) - fixed_floor_to_int(y);
            let l_dy = e[left_e].dy;
            let r_dy = e[rite_e].dy;
            if count > 1 {
                if (y & !0xFFFF) != y {
                    count -= 1;
                    let next_y = fixed_ceil_to_fixed(y + 1);
                    let dy = next_y - y;
                    let next_left = left.wrapping_add(fixed_mul(d_left, dy));
                    let next_rite = rite.wrapping_add(fixed_mul(d_rite, dy));
                    blit_trapezoid_row(
                        b,
                        y >> 16,
                        left & SNAP_MASK,
                        rite & SNAP_MASK,
                        next_left & SNAP_MASK,
                        next_rite & SNAP_MASK,
                        l_dy,
                        r_dy,
                        get_partial_alpha_h(0xFF, dy),
                        use_mask,
                        false,
                    );
                    b.flush_if_y_changed(y, next_y);
                    left = next_left;
                    rite = next_rite;
                    y = next_y;
                }
                while count > 1 {
                    count -= 1;
                    let next_y = y + FIXED1;
                    let next_left = left.wrapping_add(d_left);
                    let next_rite = rite.wrapping_add(d_rite);
                    blit_trapezoid_row(
                        b,
                        y >> 16,
                        left & SNAP_MASK,
                        rite & SNAP_MASK,
                        next_left & SNAP_MASK,
                        next_rite & SNAP_MASK,
                        l_dy,
                        r_dy,
                        0xFF,
                        use_mask,
                        false,
                    );
                    b.flush_if_y_changed(y, next_y);
                    left = next_left;
                    rite = next_rite;
                    y = next_y;
                }
            }
            let dy = local_bot - y;
            let next_left = left
                .wrapping_add(fixed_mul(d_left, dy))
                .max(left_bound + SNAP_HALF);
            let next_rite = rite
                .wrapping_add(fixed_mul(d_rite, dy))
                .min(rite_bound + SNAP_HALF);
            blit_trapezoid_row(
                b,
                y >> 16,
                left & SNAP_MASK,
                rite & SNAP_MASK,
                next_left & SNAP_MASK,
                next_rite & SNAP_MASK,
                l_dy,
                r_dy,
                get_partial_alpha_h(0xFF, dy),
                use_mask,
                false,
            );
            b.flush_if_y_changed(y, local_bot);
            left = next_left - SNAP_HALF;
            rite = next_rite - SNAP_HALF;
            y = local_bot;
        }
        e[left_e].x = left;
        e[rite_e].x = rite;
        e[left_e].y = y;
        e[rite_e].y = y;
    }
}

fn update_next_next_y(y: Fixed, next_y: Fixed, next_next_y: &mut Fixed) {
    if y > next_y && y < *next_next_y {
        *next_next_y = y;
    }
}

fn check_intersection(e: &[Edge], i: usize, next_y: Fixed, next_next_y: &mut Fixed) {
    let p = e[i].prev;
    if e[p].prev != usize::MAX && e[p].x.wrapping_add(e[p].dx) > e[i].x.wrapping_add(e[i].dx) {
        *next_next_y = next_y + (FIXED1 >> ACCURACY);
    }
}

fn check_intersection_fwd(e: &[Edge], i: usize, next_y: Fixed, next_next_y: &mut Fixed) {
    let n = e[i].next;
    if e[n].next != usize::MAX && e[i].x.wrapping_add(e[i].dx) > e[n].x.wrapping_add(e[n].dx) {
        *next_next_y = next_y + (FIXED1 >> ACCURACY);
    }
}

fn insert_new_edges(e: &mut [Edge], mut new_edge: usize, y: Fixed, next_next_y: &mut Fixed) {
    if e[new_edge].upper_y > y {
        update_next_next_y(e[new_edge].upper_y, y, next_next_y);
        return;
    }
    let prev = e[new_edge].prev;
    if e[prev].x <= e[new_edge].x {
        while e[new_edge].upper_y <= y {
            check_intersection(e, new_edge, y, next_next_y);
            update_next_next_y(e[new_edge].lower_y, y, next_next_y);
            new_edge = e[new_edge].next;
        }
        update_next_next_y(e[new_edge].upper_y, y, next_next_y);
        return;
    }
    let mut start = backward_insert_start(e, prev, e[new_edge].x);
    loop {
        let next = e[new_edge].next;
        let mut insert = true;
        loop {
            if e[start].next == new_edge {
                insert = false;
                break;
            }
            let after = e[start].next;
            if e[after].x >= e[new_edge].x {
                break;
            }
            start = after;
        }
        if insert {
            remove_edge(e, new_edge);
            insert_edge_after(e, new_edge, start);
        }
        check_intersection(e, new_edge, y, next_next_y);
        check_intersection_fwd(e, new_edge, y, next_next_y);
        update_next_next_y(e[new_edge].lower_y, y, next_next_y);
        start = new_edge;
        new_edge = next;
        if e[new_edge].upper_y > y {
            break;
        }
    }
    update_next_next_y(e[new_edge].upper_y, y, next_next_y);
}

fn edges_too_close(e: &[Edge], prev: usize, next: usize, lower_y: Fixed) -> bool {
    next != usize::MAX
        && prev != usize::MAX
        && e[next].upper_y < lower_y
        && e[prev].x.saturating_add(FIXED1) >= e[next].x.saturating_sub(e[next].dx.abs())
}

fn edges_too_close_rite(prev_rite: i32, ul: Fixed, ll: Fixed) -> bool {
    prev_rite > fixed_floor_to_int(ul) || prev_rite > fixed_floor_to_int(ll)
}

#[allow(clippy::too_many_arguments)]
fn walk_edges(
    e: &mut [Edge],
    even_odd: bool,
    b: &mut Additive,
    start_y: i32,
    stop_y: i32,
    left_clip: Fixed,
    right_clip: Fixed,
    use_mask: bool,
    force_rle: bool,
    skip_intersect: bool,
) {
    e[HEAD].x = left_clip;
    e[HEAD].upper_x = left_clip;
    e[TAIL].x = right_clip;
    e[TAIL].upper_x = right_clip;
    let mut y = e[e[HEAD].next].upper_y.max(int_to_fixed(start_y));
    let mut next_next_y = MAX_S32;
    {
        let mut edge = e[HEAD].next;
        while e[edge].upper_y <= y {
            e[edge].go_y(y);
            update_next_next_y(e[edge].lower_y, y, &mut next_next_y);
            edge = e[edge].next;
        }
        update_next_next_y(e[edge].upper_y, y, &mut next_next_y);
    }
    let winding_mask = if even_odd { 1 } else { -1 };
    loop {
        let mut w = 0;
        let mut in_interval = false;
        let mut prev_x = e[HEAD].x;
        let mut next_y = next_next_y.min(fixed_ceil_to_fixed(y + 1));
        let mut curr_e = e[HEAD].next;
        let mut left_e = HEAD;
        let mut left = left_clip;
        let mut left_dy = 0;
        let mut prev_rite = fixed_floor_to_int(left_clip);
        next_next_y = MAX_S32;
        let mut y_shift = 0;
        if (next_y - y) & (FIXED1 >> 2) != 0 {
            y_shift = 2;
            next_y = y + (FIXED1 >> 2);
        } else if (next_y - y) & (FIXED1 >> 1) != 0 {
            y_shift = 1;
        }
        let full_alpha = fixed_to_alpha(next_y - y);
        let no_real = force_rle;
        while e[curr_e].upper_y <= y {
            w += e[curr_e].winding;
            let prev_in = in_interval;
            in_interval = (w & winding_mask) != 0;
            let is_left = in_interval && !prev_in;
            let is_rite = !in_interval && prev_in;
            if is_rite {
                let mut rite = e[curr_e].x;
                e[curr_e].go_y_shift(next_y, y_shift);
                let next_left = left_clip.max(e[left_e].x);
                rite = right_clip.min(rite);
                let next_rite = right_clip.min(e[curr_e].x);
                let too_close = full_alpha == 0xFF
                    && (edges_too_close_rite(prev_rite, left, e[left_e].x)
                        || edges_too_close(e, curr_e, e[curr_e].next, next_y));
                blit_trapezoid_row(
                    b,
                    y >> 16,
                    left,
                    rite,
                    next_left,
                    next_rite,
                    left_dy,
                    e[curr_e].dy,
                    full_alpha,
                    use_mask,
                    no_real || too_close,
                );
                prev_rite = fixed_ceil_to_int(rite.max(e[curr_e].x));
            } else {
                if is_left {
                    left = e[curr_e].x.max(left_clip);
                    left_dy = e[curr_e].dy;
                    left_e = curr_e;
                }
                e[curr_e].go_y_shift(next_y, y_shift);
            }
            let next = e[curr_e].next;
            while e[curr_e].lower_y <= next_y {
                if e[curr_e].curve_count == 0 {
                    break;
                }
                e[curr_e].keep_continuous();
                if !e[curr_e].update() {
                    break;
                }
            }
            if e[curr_e].lower_y <= next_y {
                remove_edge(e, curr_e);
            } else {
                update_next_next_y(e[curr_e].lower_y, next_y, &mut next_next_y);
                let new_x = e[curr_e].x;
                if new_x < prev_x {
                    backward_insert_edge_based_on_x(e, curr_e);
                } else {
                    prev_x = new_x;
                }
                if !skip_intersect {
                    check_intersection(e, curr_e, next_y, &mut next_next_y);
                }
            }
            curr_e = next;
        }
        if in_interval {
            let lp = e[left_e].prev;
            let too_close = full_alpha == 0xFF && edges_too_close(e, lp, left_e, next_y);
            blit_trapezoid_row(
                b,
                y >> 16,
                left,
                right_clip,
                left_clip.max(e[left_e].x),
                right_clip,
                left_dy,
                0,
                full_alpha,
                use_mask,
                no_real || too_close,
            );
        }
        if force_rle {
            b.flush_if_y_changed(y, next_y);
        }
        y = next_y;
        if y >= int_to_fixed(stop_y) {
            break;
        }
        insert_new_edges(e, curr_e, y, &mut next_next_y);
    }
}

// ---------------------------------------------------------------------------
// Entry points

/// A rectangle in pixels: left, top, right, bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl IRect {
    pub(crate) fn intersect(&self, o: &IRect) -> Option<IRect> {
        let r = IRect {
            left: self.left.max(o.left),
            top: self.top.max(o.top),
            right: self.right.min(o.right),
            bottom: self.bottom.min(o.bottom),
        };
        (r.left < r.right && r.top < r.bottom).then_some(r)
    }

    fn contains(&self, o: &IRect) -> bool {
        self.left <= o.left && self.top <= o.top && self.right >= o.right && self.bottom >= o.bottom
    }
}

/// Anti-aliased coverage of a path, as `SkScan::AntiFillPath` computes it:
/// over `clip` (the canvas, or the bounds of an anti-aliased clip), and
/// with `force_rle` when drawing under an anti-aliased clip (Skia then
/// always uses the run-length blitters), or when building one.
pub(crate) struct Fill {
    pub rect: IRect,
    pub data: Vec<u8>,
}

fn float_bounds(pts: &[(f32, f32)]) -> Option<[f32; 4]> {
    let mut it = pts.iter();
    let first = it.next()?;
    let (mut l, mut t, mut r, mut b) = (first.0, first.1, first.0, first.1);
    for p in it {
        l = l.min(p.0);
        t = t.min(p.1);
        r = r.max(p.0);
        b = b.max(p.1);
    }
    [l, t, r, b]
        .iter()
        .all(|v| v.is_finite())
        .then_some([l, t, r, b])
}

/// `SkRect::roundOut`, saturated to i32.
fn round_out(r: [f32; 4]) -> IRect {
    let sat = |v: f32| v.clamp(i32::MIN as f32, i32::MAX as f32) as i32;
    IRect {
        left: sat(r[0].floor()),
        top: sat(r[1].floor()),
        right: sat(r[2].ceil()),
        bottom: sat(r[3].ceil()),
    }
}

/// `SkScan::AntiFillPath` + `SkScan::AAAFillPath` for a device-space path.
pub(crate) fn fill_path(
    segs: &[Seg],
    even_odd: bool,
    clip: IRect,
    force_rle: bool,
) -> Option<Fill> {
    let (verbs, pts) = verbs_and_points(segs);
    // Trailing moves add nothing to the bounds of a Skia path's edges, but
    // SkPath::getBounds includes every point.
    let bounds = float_bounds(&pts)?;
    let ir = round_out(bounds);
    if ir.left >= ir.right || ir.top >= ir.bottom {
        return None;
    }
    let clipped = ir.intersect(&clip)?;
    let contained = clip.contains(&ir);
    let convex = is_convex(&verbs, &pts);

    let target = Target {
        left: clipped.left,
        top: clipped.top,
        width: clipped.right - clipped.left,
        height: clipped.bottom - clipped.top,
        data: vec![0; ((clipped.right - clipped.left) * (clipped.bottom - clipped.top)) as usize],
    };

    let width = ir.right - ir.left;
    let can_mask =
        width <= 32 && (i64::from((width + 3) & !3) * i64::from(ir.bottom - ir.top)) <= 1024;
    if can_mask && !force_rle {
        if let Some(r) = is_rect(&verbs, &pts) {
            // try_blit_fat_anti_rect.
            let cr = [
                r[0].max(clip.left as f32),
                r[1].max(clip.top as f32),
                r[2].min(clip.right as f32),
                r[3].min(clip.bottom as f32),
            ];
            if !(cr[0] < cr[2] && cr[1] < cr[3]) {
                return None;
            }
            let rb = round_out(cr);
            if rb.right - rb.left >= 3 {
                let mut t = target;
                blit_fat_anti_rect(&mut t, cr);
                return Some(Fill {
                    rect: clipped,
                    data: t.data,
                });
            }
        }
    }
    let kind = if can_mask && !force_rle {
        BlitterKind::Mask
    } else if convex {
        BlitterKind::RunBased
    } else {
        BlitterKind::SafeRle
    };
    let use_mask = kind == BlitterKind::Mask;
    let mut additive = Additive {
        kind,
        target,
        mask_left: ir.left,
        mask_top: ir.top,
        mask_width: width,
        mask: if use_mask {
            vec![0; ((width + 2) * (ir.bottom - ir.top)) as usize]
        } else {
            Vec::new()
        },
        rle_left: clipped.left,
        rle_width: clipped.right - clipped.left,
        rle_top: clipped.top,
        curr_y: clipped.top - 1,
        row: if use_mask {
            Vec::new()
        } else {
            vec![0; (clipped.right - clipped.left) as usize]
        },
    };

    // aaa_fill_path: edges clipped to the clip bounds unless the path lies
    // within them; a convex path cannot drop its right edges.
    let clip_rect = (!contained).then_some((
        ClipRect {
            l: f64::from(clip.left),
            t: f64::from(clip.top),
            r: f64::from(clip.right),
            b: f64::from(clip.bottom),
        },
        !convex,
    ));
    let built = build_edges(segs, clip_rect);
    if built.is_empty() {
        return None;
    }
    let count = built.len();
    let mut edges = Vec::with_capacity(count + 2);
    let mut head = Edge::blank();
    head.upper_y = MIN_S32;
    head.lower_y = MIN_S32;
    head.x = MIN_S32;
    head.dx = 0;
    head.dy = MAX_S32;
    head.upper_x = MIN_S32;
    let mut tail = Edge::blank();
    tail.upper_y = MAX_S32;
    tail.lower_y = MAX_S32;
    tail.x = MAX_S32;
    tail.dx = 0;
    tail.dy = MAX_S32;
    tail.upper_x = MAX_S32;
    edges.push(head);
    edges.push(tail);
    let mut sorted = built;
    sorted.sort_by(|a, b| {
        a.upper_y
            .cmp(&b.upper_y)
            .then(a.x.cmp(&b.x))
            .then(a.dx.cmp(&b.dx))
    });
    edges.extend(sorted);
    for i in 2..edges.len() {
        edges[i].prev = if i == 2 { HEAD } else { i - 1 };
        edges[i].next = if i + 1 == edges.len() { TAIL } else { i + 1 };
    }
    edges[HEAD].prev = usize::MAX;
    edges[HEAD].next = 2;
    edges[TAIL].prev = edges.len() - 1;
    edges[TAIL].next = usize::MAX;

    let mut start_y = ir.top;
    let mut stop_y = ir.bottom;
    if !contained && start_y < clip.top {
        start_y = clip.top;
    }
    if !contained && stop_y > clip.bottom {
        stop_y = clip.bottom;
    }
    let mut left_bound = int_to_fixed(clip.left);
    let mut right_bound = int_to_fixed(clip.right);
    if use_mask {
        left_bound = left_bound.max(int_to_fixed(ir.left));
        right_bound = right_bound.min(int_to_fixed(ir.right));
    }
    if convex && count >= 2 {
        walk_convex_edges(
            &mut edges,
            &mut additive,
            stop_y,
            left_bound,
            right_bound,
            use_mask,
        );
    } else {
        let skip_intersect = pts.len() as i64 > i64::from(stop_y - start_y) * 2;
        walk_edges(
            &mut edges,
            even_odd,
            &mut additive,
            start_y,
            stop_y,
            left_bound,
            right_bound,
            use_mask,
            force_rle,
            skip_intersect,
        );
    }
    let t = additive.finish();
    Some(Fill {
        rect: clipped,
        data: t.data,
    })
}

/// `SkPath::isRect(&rect, &isClosed, &direction)`: the rectangle, whether
/// it ends with a close, and whether it runs clockwise.
pub(crate) fn rect_contour(segs: &[Seg]) -> Option<([f32; 4], bool, bool)> {
    let (verbs, pts) = verbs_and_points(segs);
    rect_contour_of(&verbs, &pts)
}

/// The path's rectangle when Skia's `SkPath::isRect` sees one.
pub(crate) fn rect_of(segs: &[Seg]) -> Option<[f32; 4]> {
    let (verbs, pts) = verbs_and_points(segs);
    is_rect(&verbs, &pts)
}

/// `scalar_to_alpha` in `SkBlitter.cpp`.
fn scalar_to_alpha(a: f32) -> u8 {
    let alpha = (a * 255.0) as u8;
    if alpha > 247 {
        255
    } else if alpha < 8 {
        0
    } else {
        alpha
    }
}

/// `SkBlitter::blitFatAntiRect` on the real blitter.
fn blit_fat_anti_rect(t: &mut Target, rect: [f32; 4]) {
    let b = round_out(rect);
    let (w, h) = (b.right - b.left, b.bottom - b.top);
    if h == 0 {
        return;
    }
    let partial_l = (b.left + 1) as f32 - rect[0];
    let partial_r = rect[2] - (b.right - 1) as f32;
    let mut partial_t = (b.top + 1) as f32 - rect[1];
    let partial_b = rect[3] - (b.bottom - 1) as f32;
    if h == 1 {
        partial_t = rect[3] - rect[1];
    }
    let row = |t: &mut Target, y: i32, first: u8, middle: u8, last: u8| {
        t.composite(b.left, y, first);
        for i in 1..w - 1 {
            t.composite(b.left + i, y, middle);
        }
        t.composite(b.left + w - 1, y, last);
    };
    row(
        t,
        b.top,
        scalar_to_alpha(partial_l * partial_t),
        scalar_to_alpha(partial_t),
        scalar_to_alpha(partial_r * partial_t),
    );
    if h > 2 {
        t.blit_anti_rect(
            b.left,
            b.top + 1,
            w - 2,
            h - 2,
            scalar_to_alpha(partial_l),
            scalar_to_alpha(partial_r),
        );
    }
    if h > 1 {
        row(
            t,
            b.bottom - 1,
            scalar_to_alpha(partial_l * partial_b),
            scalar_to_alpha(partial_b),
            scalar_to_alpha(partial_r * partial_b),
        );
    }
}

/// `SkScan::AntiFillRect` (`antifilldot8`) for a device rectangle, within
/// `clip`.
pub(crate) fn anti_fill_rect(rect: [f32; 4], clip: IRect) -> Option<Fill> {
    let r = [
        rect[0].max(clip.left as f32),
        rect[1].max(clip.top as f32),
        rect[2].min(clip.right as f32),
        rect[3].min(clip.bottom as f32),
    ];
    if !(r[0] < r[2] && r[1] < r[3]) {
        return None;
    }
    let outer = round_out(r).intersect(&clip)?;
    let mut t = Target {
        left: outer.left,
        top: outer.top,
        width: outer.right - outer.left,
        height: outer.bottom - outer.top,
        data: vec![0; ((outer.right - outer.left) * (outer.bottom - outer.top)) as usize],
    };
    // XRect_set then SkFixedToFDot8.
    let to_dot8 = |v: f32| -> i32 { (((v * 65536.0) as i32) + 0x80) >> 8 };
    let (l, tp, rr, bb) = (to_dot8(r[0]), to_dot8(r[1]), to_dot8(r[2]), to_dot8(r[3]));
    antifilldot8(&mut t, l, tp, rr, bb);
    Some(Fill {
        rect: outer,
        data: t.data,
    })
}

fn alpha_mul(alpha: i32, scale: i32) -> u8 {
    ((alpha * scale) >> 8) as u8
}

fn do_scanline(t: &mut Target, l: i32, top: i32, r: i32, alpha: i32) {
    if (l >> 8) == ((r - 1) >> 8) {
        t.blit_v(l >> 8, top, 1, alpha_mul(alpha, r - l));
        return;
    }
    let mut left = l >> 8;
    if l & 0xFF != 0 {
        t.blit_v(left, top, 1, alpha_mul(alpha, 256 - (l & 0xFF)));
        left += 1;
    }
    let rite = r >> 8;
    let width = rite - left;
    for i in 0..width {
        t.composite(left + i, top, alpha.min(255) as u8);
    }
    if r & 0xFF != 0 {
        t.blit_v(rite, top, 1, alpha_mul(alpha, r & 0xFF));
    }
}

fn antifilldot8(t: &mut Target, l: i32, tp: i32, r: i32, b: i32) {
    if l >= r || tp >= b {
        return;
    }
    let mut top = tp >> 8;
    if top == ((b - 1) >> 8) {
        do_scanline(t, l, top, r, b - tp - 1);
        return;
    }
    if tp & 0xFF != 0 {
        do_scanline(t, l, top, r, 256 - (tp & 0xFF));
        top += 1;
    }
    let bot = b >> 8;
    let height = bot - top;
    if height > 0 {
        let mut left = l >> 8;
        if left == ((r - 1) >> 8) {
            t.blit_v(left, top, height, (r - l - 1) as u8);
        } else {
            if l & 0xFF != 0 {
                t.blit_v(left, top, height, (256 - (l & 0xFF)) as u8);
                left += 1;
            }
            let rite = r >> 8;
            let width = rite - left;
            if width > 0 {
                t.blit_rect(left, top, width, height);
            }
            if r & 0xFF != 0 {
                t.blit_v(rite, top, height, (r & 0xFF) as u8);
            }
        }
    }
    if b & 0xFF != 0 {
        do_scanline(t, l, bot, r, b & 0xFF);
    }
}

/// `SkScan::FillRect`: the rectangle rounded to whole pixels, fully
/// covered.
pub(crate) fn fill_rect(rect: [f32; 4], clip: IRect) -> Option<Fill> {
    let round = |v: f32| (v + 0.5).floor() as i32;
    let ir = IRect {
        left: round(rect[0]),
        top: round(rect[1]),
        right: round(rect[2]),
        bottom: round(rect[3]),
    }
    .intersect(&clip)?;
    Some(Fill {
        rect: ir,
        data: vec![255; ((ir.right - ir.left) * (ir.bottom - ir.top)) as usize],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_segs(l: f64, t: f64, r: f64, b: f64) -> Vec<Seg> {
        vec![
            Seg::Move((l, t)),
            Seg::Line((r, t)),
            Seg::Line((r, b)),
            Seg::Line((l, b)),
            Seg::Close,
        ]
    }

    const CANVAS: IRect = IRect {
        left: 0,
        top: 0,
        right: 64,
        bottom: 64,
    };

    fn at(f: &Fill, x: i32, y: i32) -> u8 {
        let w = f.rect.right - f.rect.left;
        f.data[((y - f.rect.top) * w + (x - f.rect.left)) as usize]
    }

    #[test]
    fn quick_inverse_is_the_table() {
        // Entries of Skia's table: -4096 at 1024, -4100 at 1023, -4161 at
        // 1008, -4194304 at 1.
        assert_eq!(quick_inverse(1024), 4096);
        assert_eq!(quick_inverse(1023), 4100);
        assert_eq!(quick_inverse(1008), 4161);
        assert_eq!(quick_inverse(1), 4194304);
        assert_eq!(quick_inverse(-2), -2097152);
    }

    #[test]
    fn snap_y_rounds_to_quarters() {
        assert_eq!(
            snap_y(int_to_fixed(3) + FIXED1 / 8),
            int_to_fixed(3) + FIXED1 / 4
        );
        assert_eq!(snap_y(int_to_fixed(3) + FIXED1 / 8 - 1), int_to_fixed(3));
    }

    #[test]
    fn convexity() {
        let (v, p) = verbs_and_points(&rect_segs(1.0, 1.0, 5.0, 5.0));
        assert!(is_convex(&v, &p));
        let star = [
            Seg::Move((24.0, 2.0)),
            Seg::Line((37.0, 42.0)),
            Seg::Line((3.0, 17.0)),
            Seg::Line((45.0, 17.0)),
            Seg::Line((11.0, 42.0)),
            Seg::Close,
        ];
        let (v, p) = verbs_and_points(&star);
        assert!(!is_convex(&v, &p));
        let mut two = rect_segs(1.0, 1.0, 5.0, 5.0);
        two.extend(rect_segs(10.0, 1.0, 15.0, 5.0));
        let (v, p) = verbs_and_points(&two);
        assert!(!is_convex(&v, &p));
    }

    #[test]
    fn rect_contours() {
        let (v, p) = verbs_and_points(&rect_segs(1.5, 2.0, 5.0, 9.25));
        assert_eq!(is_rect(&v, &p), Some([1.5, 2.0, 5.0, 9.25]));
        // A trailing move (the canvas's rect() leaves one) is allowed.
        let mut s = rect_segs(1.0, 1.0, 5.0, 5.0);
        s.push(Seg::Move((1.0, 1.0)));
        let (v, p) = verbs_and_points(&s);
        assert!(is_rect(&v, &p).is_some());
        let tri = [
            Seg::Move((0.0, 0.0)),
            Seg::Line((4.0, 0.0)),
            Seg::Line((0.0, 4.0)),
            Seg::Close,
        ];
        let (v, p) = verbs_and_points(&tri);
        assert_eq!(is_rect(&v, &p), None);
    }

    #[test]
    fn whole_pixel_rectangles_are_solid() {
        let f = fill_path(&rect_segs(4.0, 4.0, 24.0, 16.0), false, CANVAS, false).unwrap();
        assert_eq!(
            f.rect,
            IRect {
                left: 4,
                top: 4,
                right: 24,
                bottom: 16
            }
        );
        assert!(f.data.iter().all(|&a| a == 255));
    }

    #[test]
    fn half_pixel_edges_are_half_covered() {
        // Wider than 32 px: the run-based blitter and the convex walker.
        let f = fill_path(&rect_segs(2.5, 2.5, 50.5, 10.5), false, CANVAS, false).unwrap();
        assert_eq!(at(&f, 2, 5), 128);
        assert_eq!(at(&f, 10, 5), 255);
        assert_eq!(at(&f, 10, 2), 128);
        assert_eq!(at(&f, 2, 2), 64);
        // Small: the fat-rect blit (scalar_to_alpha truncates 127.5).
        let f = fill_path(&rect_segs(2.5, 2.5, 10.5, 10.5), false, CANVAS, false).unwrap();
        assert_eq!(at(&f, 2, 5), 127);
        assert_eq!(at(&f, 2, 2), 63);
    }

    #[test]
    fn even_odd_leaves_the_hole() {
        let mut s = rect_segs(0.0, 0.0, 40.0, 40.0);
        s.extend(rect_segs(10.0, 10.0, 30.0, 30.0));
        let f = fill_path(&s, true, CANVAS, false).unwrap();
        assert_eq!(at(&f, 5, 5), 255);
        assert_eq!(at(&f, 20, 20), 0);
        let f = fill_path(&s, false, CANVAS, false).unwrap();
        assert_eq!(at(&f, 20, 20), 255);
    }

    #[test]
    fn anti_fill_rect_uses_eighth_bit_edges() {
        let f = anti_fill_rect([1.5, 1.0, 4.0, 3.0], CANVAS).unwrap();
        assert_eq!(at(&f, 1, 1), 128);
        assert_eq!(at(&f, 2, 1), 255);
        let f = fill_rect([1.5, 1.4, 4.0, 3.0], CANVAS).unwrap();
        assert_eq!(
            f.rect,
            IRect {
                left: 2,
                top: 1,
                right: 4,
                bottom: 3
            }
        );
    }

    /// A convex path running past the clip bounds is cut to them
    /// (SkEdgeClipper): the parts outside become vertical edges on the
    /// bounds, so the convex walker does not carry a clamped edge along its
    /// slope.
    #[test]
    fn edges_past_the_clip_are_cut_to_it() {
        use crate::edges::from_display;
        use excali_scene::display::{Path, Transform};
        let t = Transform::translate(72.0, 64.0)
            .concat(&Transform::rotate(std::f64::consts::FRAC_PI_8));
        let segs = from_display(&Path::rect(-20.0, -20.0, 40.0, 40.0), &t);
        let clip = IRect {
            left: 63,
            top: 54,
            right: 84,
            bottom: 74,
        };
        for force_rle in [false, true] {
            let f = fill_path(&segs, false, clip, force_rle).unwrap();
            assert_eq!(f.rect, clip);
            assert!(f.data.iter().all(|&a| a == 255), "{force_rle}");
        }
    }
}
