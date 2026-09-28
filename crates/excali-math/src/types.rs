//! The value types of `packages/math/src/types.ts`.
//!
//! Upstream brands plain tuples so that a global (scene) point cannot be
//! passed where a local (element-relative) point is expected. The port keeps
//! that distinction in the type system with a zero-sized space marker:
//! [`GlobalPoint`] and [`LocalPoint`] are `Point<Global>` and `Point<Local>`,
//! and every shape built from points (segments, lines, polygons, ...) carries
//! the same marker. Coordinates are `x`/`y` fields, and points and vectors
//! also index as `p[0]`/`p[1]` like upstream's tuples.

use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::{Deref, Index, Neg};

// -- measurements -----------------------------------------------------------

/// An angle in radians (`types.ts:9`).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Radians(pub f64);

/// An angle in degrees (`types.ts:15`).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Degrees(pub f64);

impl Neg for Radians {
    type Output = Radians;
    fn neg(self) -> Radians {
        Radians(-self.0)
    }
}

impl Neg for Degrees {
    type Output = Degrees;
    fn neg(self) -> Degrees {
        Degrees(-self.0)
    }
}

/// A number range including both ends, `[start, end]` (`types.ts:24`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InclusiveRange(pub f64, pub f64);

// -- coordinate spaces --------------------------------------------------------

/// A coordinate space marker: [`Global`] or [`Local`].
pub trait Space: Copy + Clone + fmt::Debug + Default + PartialEq + Eq + Hash + 'static {}

/// World/canvas/scene space, upstream's `GlobalPoint` brand (`types.ts:34`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Global;

/// Element-relative space, upstream's `LocalPoint` brand (`types.ts:52`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Local;

impl Space for Global {}
impl Space for Local {}

// -- points and vectors -----------------------------------------------------

/// A 2D position in the space `S` (`types.ts:34-56`).
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Point<S: Space = Global> {
    pub x: f64,
    pub y: f64,
    space: PhantomData<S>,
}

/// A point in world/canvas/scene space.
pub type GlobalPoint = Point<Global>;
/// A point in an element's local space.
pub type LocalPoint = Point<Local>;

impl<S: Space> Point<S> {
    /// `[0, 0]`.
    pub const ORIGIN: Point<S> = Point::new(0.0, 0.0);

    pub const fn new(x: f64, y: f64) -> Point<S> {
        Point {
            x,
            y,
            space: PhantomData,
        }
    }

    /// The same coordinates in another space (upstream's `as` casts).
    pub const fn cast<T: Space>(self) -> Point<T> {
        Point::new(self.x, self.y)
    }
}

impl<S: Space> fmt::Debug for Point<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}, {:?}]", self.x, self.y)
    }
}

impl<S: Space> Index<usize> for Point<S> {
    type Output = f64;
    fn index(&self, i: usize) -> &f64 {
        match i {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("index out of bounds: a point has 2 coordinates but the index is {i}"),
        }
    }
}

impl<S: Space> From<[f64; 2]> for Point<S> {
    fn from([x, y]: [f64; 2]) -> Point<S> {
        Point::new(x, y)
    }
}

impl<S: Space> From<Point<S>> for [f64; 2] {
    fn from(p: Point<S>) -> [f64; 2] {
        [p.x, p.y]
    }
}

/// An `{ x, y }` coordinate object, upstream's `GlobalCoord`/`LocalCoord`
/// (`types.ts:43, 61`), accepted by [`crate::point_from_coords`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Coord<S: Space = Global> {
    pub x: f64,
    pub y: f64,
    space: PhantomData<S>,
}

impl<S: Space> Coord<S> {
    pub const fn new(x: f64, y: f64) -> Coord<S> {
        Coord {
            x,
            y,
            space: PhantomData,
        }
    }
}

/// A 2D vector `[u, v]` (`types.ts:88`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector {
    pub x: f64,
    pub y: f64,
}

impl Vector {
    /// `[0, 0]`.
    pub const ZERO: Vector = Vector { x: 0.0, y: 0.0 };
}

impl Index<usize> for Vector {
    type Output = f64;
    fn index(&self, i: usize) -> &f64 {
        match i {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("index out of bounds: a vector has 2 components but the index is {i}"),
        }
    }
}

impl From<[f64; 2]> for Vector {
    fn from([x, y]: [f64; 2]) -> Vector {
        Vector { x, y }
    }
}

/// Polar coordinates around the origin (`types.ts:141`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PolarCoords {
    pub radius: f64,
    pub angle: Radians,
}

// -- shapes ---------------------------------------------------------------------

/// An infinite line through two points (`types.ts:70`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Line<S: Space = Global>(pub Point<S>, pub Point<S>);

/// A line segment between two end points (`types.ts:79`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineSegment<S: Space = Global>(pub Point<S>, pub Point<S>);

/// A triangle (`types.ts:99`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Triangle<S: Space = Global>(pub Point<S>, pub Point<S>, pub Point<S>);

/// An axis-aligned rectangle given by its top-left and bottom-right corners
/// (`types.ts:110`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rectangle<S: Space = Global>(pub Point<S>, pub Point<S>);

/// A closed polygon: the last point repeats the first (`types.ts:122`).
/// Built by [`crate::polygon`] and [`crate::polygon_from_points`]; it
/// dereferences to its points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polygon<S: Space = Global>(pub(crate) Vec<Point<S>>);

impl<S: Space> Polygon<S> {
    pub fn into_points(self) -> Vec<Point<S>> {
        self.0
    }
}

impl<S: Space> Deref for Polygon<S> {
    type Target = [Point<S>];
    fn deref(&self) -> &[Point<S>] {
        &self.0
    }
}

impl<S: Space> AsRef<[Point<S>]> for Polygon<S> {
    fn as_ref(&self) -> &[Point<S>] {
        &self.0
    }
}

/// A cubic Bézier curve given by its four control points: start, two
/// handles, end (`types.ts:134`). Built by [`crate::curve`]; the points
/// index as `c[0]`..`c[3]` like upstream's tuple.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Curve<S: Space = Global>(pub Point<S>, pub Point<S>, pub Point<S>, pub Point<S>);

impl<S: Space> Curve<S> {
    /// The control points in order.
    pub const fn points(self) -> [Point<S>; 4] {
        [self.0, self.1, self.2, self.3]
    }
}

impl<S: Space> Index<usize> for Curve<S> {
    type Output = Point<S>;
    fn index(&self, i: usize) -> &Point<S> {
        match i {
            0 => &self.0,
            1 => &self.1,
            2 => &self.2,
            3 => &self.3,
            _ => panic!("index out of bounds: a curve has 4 points but the index is {i}"),
        }
    }
}

/// An axis-aligned ellipse given by its center and half axes (`types.ts:154`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ellipse<S: Space = Global> {
    pub center: Point<S>,
    pub half_width: f64,
    pub half_height: f64,
}

// -- untyped values ---------------------------------------------------------

/// A JavaScript value of unknown shape, as received by upstream's runtime
/// shape checks (`isPoint`, `isVector`, `isLineSegment`, `isValidPoint`,
/// `isFiniteNumber`), e.g. data read from a file. Only numbers and arrays are
/// inspected; objects are opaque.
#[derive(Clone, Debug, PartialEq)]
pub enum Unknown {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Unknown>),
    Object,
}

impl Unknown {
    pub(crate) fn as_number(&self) -> Option<f64> {
        match self {
            Unknown::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub(crate) fn as_array(&self) -> Option<&[Unknown]> {
        match self {
            Unknown::Array(items) => Some(items),
            _ => None,
        }
    }
}

impl From<f64> for Unknown {
    fn from(n: f64) -> Unknown {
        Unknown::Number(n)
    }
}

impl From<bool> for Unknown {
    fn from(b: bool) -> Unknown {
        Unknown::Bool(b)
    }
}

impl From<&str> for Unknown {
    fn from(s: &str) -> Unknown {
        Unknown::String(s.to_owned())
    }
}

impl From<[f64; 2]> for Unknown {
    fn from([x, y]: [f64; 2]) -> Unknown {
        Unknown::Array(vec![Unknown::Number(x), Unknown::Number(y)])
    }
}

impl From<Vec<Unknown>> for Unknown {
    fn from(items: Vec<Unknown>) -> Unknown {
        Unknown::Array(items)
    }
}
