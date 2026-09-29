//! 2D affine matrices with the canvas's composition rule.

use excali_math::js;

/// A 2D affine matrix in the canvas's `[a b c d e f]` layout: a point
/// `(x, y)` maps to `(a·x + c·y + e, b·x + d·y + f)`, as
/// `CanvasRenderingContext2D.setTransform(a, b, c, d, e, f)` defines it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// `[1 0 0 1 0 0]`, a fresh context's matrix.
    pub const IDENTITY: Transform = Transform::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    /// The matrix `setTransform(a, b, c, d, e, f)` sets.
    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    /// The matrix `translate(x, y)` multiplies in.
    pub const fn translate(x: f64, y: f64) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    /// The matrix `scale(x, y)` multiplies in.
    pub const fn scale(x: f64, y: f64) -> Self {
        Self::new(x, 0.0, 0.0, y, 0.0, 0.0)
    }

    /// The matrix `rotate(angle)` multiplies in: `angle` in radians,
    /// clockwise on screen (the canvas's y axis points down).
    pub fn rotate(angle: f64) -> Self {
        let (sin, cos) = (js::sin(angle), js::cos(angle));
        Self::new(cos, sin, -sin, cos, 0.0, 0.0)
    }

    /// `self × m`: the current matrix after `ctx.transform(m)` (or
    /// `translate`, `scale`, `rotate`) when it was `self`. `m` applies to a
    /// point first, then `self`.
    pub fn concat(&self, m: &Transform) -> Transform {
        Transform {
            a: self.a * m.a + self.c * m.b,
            b: self.b * m.a + self.d * m.b,
            c: self.a * m.c + self.c * m.d,
            d: self.b * m.c + self.d * m.d,
            e: self.a * m.e + self.c * m.f + self.e,
            f: self.b * m.e + self.d * m.f + self.f,
        }
    }

    /// The image of the point `(x, y)`.
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Whether this is exactly the identity.
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }
}
