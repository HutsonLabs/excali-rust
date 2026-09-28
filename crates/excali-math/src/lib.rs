//! Geometry primitives: points, vectors, segments, curves, polygons.
//!
//! Upstream counterpart: `packages/math`. Every function exported by
//! `packages/math/src` except `pca.ts` (shape recognition) is ported here
//! under its snake_case name (`pointRotateRads` -> [`point_rotate_rads`]).
//! Where upstream has an optional trailing parameter with a default, the
//! plain function uses the default and a `_with` variant takes it
//! ([`points_equal`] / [`points_equal_with`]). `goldens/math.json`, generated
//! from upstream's own code, pins the results bit for bit.
//!
//! Arithmetic is kept in upstream's order, and the `Math` functions whose
//! results differ from Rust's (`Math.hypot`, `Math.round`, `Math.min`,
//! `Math.max`, `**`) go through [`js`].
//!
//! Targets: native, wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): none (std only).

mod angle;
mod constants;
mod curve;
mod ellipse;
pub mod js;
mod line;
mod point;
mod polygon;
mod range;
mod rectangle;
mod segment;
mod triangle;
mod types;
mod utils;
mod vector;

pub use angle::*;
pub use constants::*;
pub use curve::*;
pub use ellipse::*;
pub use line::*;
pub use point::*;
pub use polygon::*;
pub use range::*;
pub use rectangle::*;
pub use segment::*;
pub use triangle::*;
pub use types::*;
pub use utils::*;
pub use vector::*;
