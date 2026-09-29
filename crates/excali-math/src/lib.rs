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
//! `Math.max`, `**`, and every transcendental: `Math.sin`, `cos`, `tan`,
//! `asin`, `acos`, `atan`, `atan2`, `exp`, `log`, `log2`, `log10`, `cbrt`)
//! go through [`js`], which returns V8's doubles on every platform
//! (ex-009, ADR-011); the workspace `clippy.toml` rejects the `f64`
//! methods.
//!
//! Targets: native, wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): none (std,
//! and `pxfm` for the correctly rounded `pow`).

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
