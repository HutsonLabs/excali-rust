//! perfect-freehand 1.2.0 `src/vec.ts`: two-component vector helpers, in the
//! same operation order so every double matches.

use excali_math::js;

pub(crate) type Vec2 = [f64; 2];

/// `neg`.
pub(crate) fn neg(a: Vec2) -> Vec2 {
    [-a[0], -a[1]]
}

/// `add`.
pub(crate) fn add(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] + b[0], a[1] + b[1]]
}

/// `sub`.
pub(crate) fn sub(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] - b[0], a[1] - b[1]]
}

/// `mul`: scale by `n`.
pub(crate) fn mul(a: Vec2, n: f64) -> Vec2 {
    [a[0] * n, a[1] * n]
}

/// `div`: divide by `n`.
fn div(a: Vec2, n: f64) -> Vec2 {
    [a[0] / n, a[1] / n]
}

/// `per`: the perpendicular `[y, -x]`.
pub(crate) fn per(a: Vec2) -> Vec2 {
    [a[1], -a[0]]
}

/// `dpr`: dot product.
pub(crate) fn dpr(a: Vec2, b: Vec2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// `isEqual`: exact component equality.
pub(crate) fn is_equal(a: Vec2, b: Vec2) -> bool {
    a[0] == b[0] && a[1] == b[1]
}

/// `len`: `Math.hypot(x, y)`.
fn len(a: Vec2) -> f64 {
    js::hypot(a[0], a[1])
}

/// `len2`: squared length.
fn len2(a: Vec2) -> f64 {
    a[0] * a[0] + a[1] * a[1]
}

/// `dist2`: squared distance.
pub(crate) fn dist2(a: Vec2, b: Vec2) -> f64 {
    len2(sub(a, b))
}

/// `uni`: `a / len(a)` (NaN components for the zero vector, as in JS).
pub(crate) fn uni(a: Vec2) -> Vec2 {
    div(a, len(a))
}

/// `dist`: `Math.hypot(a.y - b.y, a.x - b.x)` (that argument order).
pub(crate) fn dist(a: Vec2, b: Vec2) -> f64 {
    js::hypot(a[1] - b[1], a[0] - b[0])
}

/// `rotAround`: rotate `a` around `c` by `r` radians.
pub(crate) fn rot_around(a: Vec2, c: Vec2, r: f64) -> Vec2 {
    let s = js::sin(r);
    let co = js::cos(r);
    let px = a[0] - c[0];
    let py = a[1] - c[1];
    let nx = px * co - py * s;
    let ny = px * s + py * co;
    [nx + c[0], ny + c[1]]
}

/// `lrp`: `a + (b - a) * t`.
pub(crate) fn lrp(a: Vec2, b: Vec2, t: f64) -> Vec2 {
    add(a, mul(sub(b, a), t))
}

/// `prj`: `a + b * c`.
pub(crate) fn prj(a: Vec2, b: Vec2, c: f64) -> Vec2 {
    add(a, mul(b, c))
}
