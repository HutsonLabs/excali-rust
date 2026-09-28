//! rough.js's output types (`bin/core.d.ts`): `Op`, `OpSet`, `Drawable`.

use crate::Options;

/// One drawing operation. rough.js's `{ op, data }` with the arity fixed by
/// the kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// `move`: `[x, y]`.
    Move([f64; 2]),
    /// `lineTo`: `[x, y]`.
    LineTo([f64; 2]),
    /// `bcurveTo`: `[cp1x, cp1y, cp2x, cp2y, x, y]`.
    BCurveTo([f64; 6]),
}

impl Op {
    /// rough.js's `op` string.
    pub fn name(&self) -> &'static str {
        match self {
            Op::Move(_) => "move",
            Op::LineTo(_) => "lineTo",
            Op::BCurveTo(_) => "bcurveTo",
        }
    }

    /// rough.js's `data` array.
    pub fn data(&self) -> &[f64] {
        match self {
            Op::Move(d) | Op::LineTo(d) => d,
            Op::BCurveTo(d) => d,
        }
    }
}

/// `OpSetType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpSetType {
    /// A stroked outline.
    Path,
    /// A filled outline (solid fill).
    FillPath,
    /// Fill strokes (pattern fills), drawn with the fill colour.
    FillSketch,
}

impl OpSetType {
    /// rough.js's `type` string.
    pub fn as_str(&self) -> &'static str {
        match self {
            OpSetType::Path => "path",
            OpSetType::FillPath => "fillPath",
            OpSetType::FillSketch => "fillSketch",
        }
    }
}

/// `OpSet`: a list of ops drawn as one path.
#[derive(Clone, Debug, PartialEq)]
pub struct OpSet {
    pub kind: OpSetType,
    pub ops: Vec<Op>,
}

impl OpSet {
    /// `{ type: 'path', ops }`.
    pub fn path(ops: Vec<Op>) -> Self {
        Self {
            kind: OpSetType::Path,
            ops,
        }
    }

    /// `{ type: 'fillPath', ops }`.
    pub fn fill_path(ops: Vec<Op>) -> Self {
        Self {
            kind: OpSetType::FillPath,
            ops,
        }
    }

    /// `{ type: 'fillSketch', ops }`.
    pub fn fill_sketch(ops: Vec<Op>) -> Self {
        Self {
            kind: OpSetType::FillSketch,
            ops,
        }
    }
}

/// The generator method that made a [`Drawable`] (`Drawable.shape`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Line,
    Rectangle,
    Ellipse,
    Circle,
    LinearPath,
    Arc,
    Curve,
    Polygon,
    Path,
}

impl Shape {
    /// rough.js's `shape` string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Shape::Line => "line",
            Shape::Rectangle => "rectangle",
            Shape::Ellipse => "ellipse",
            Shape::Circle => "circle",
            Shape::LinearPath => "linearPath",
            Shape::Arc => "arc",
            Shape::Curve => "curve",
            Shape::Polygon => "polygon",
            Shape::Path => "path",
        }
    }
}

/// `Drawable`: what a generator method returns. `options` are the resolved
/// options the shape was drawn with (rough.js also keeps the RNG in them;
/// the port does not, since every call starts from `options.seed`).
#[derive(Clone, Debug, PartialEq)]
pub struct Drawable {
    pub shape: Shape,
    pub options: Options,
    pub sets: Vec<OpSet>,
}
