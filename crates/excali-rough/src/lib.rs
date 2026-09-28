//! Hand-drawn ("sketchy") path generation, a port of roughjs.
//!
//! Upstream counterpart: `roughjs` 4.6.4 as used by `packages/element/src/shape.ts`,
//! with the packages it bundles: `hachure-fill` 0.5.2 ([`hachure_fill`]),
//! `path-data-parser` 0.1.0 ([`path_data`]),
//! `points-on-curve` 0.2.0 ([`points_on_curve`]) and `points-on-path` 0.2.1
//! ([`points_on_path`]).
//!
//! - [`Random`]: rough.js's seeded generator. Every shape is a function of
//!   its options and the sequence of numbers drawn from `options.seed`, so
//!   the port draws the same numbers in the same order as rough.js; that
//!   order is the contract.
//! - [`RoughGenerator`]: `line`, `rectangle`, `ellipse`, `circle`,
//!   `linearPath`, `arc`, `curve`, `polygon` and `path`, returning a
//!   [`Drawable`] of op sets.
//! - [`renderer`]: the primitives the generator builds on, and the fills:
//!   `solid` (a jittered `fillPath` outline) and the pattern fills
//!   (`fillSketch` op sets) `hachure`, `cross-hatch`, `zigzag`, `dashed`,
//!   `zigzag-line` and `dots` (`bin/fillers/*`).
//! - [`hachure_fill`]: the `hachure-fill` 0.5.2 scan-line package the
//!   pattern fills draw their lines from.
//!
//! `goldens/rough-primitives.json`, `goldens/rough-generator.json`,
//! `goldens/rough-fills.json` and `goldens/rough-options.json`, generated
//! from the pinned package, pin the output op by op.
//!
//! Targets: native, wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-math`.

mod core;
mod fillers;
mod generator;
#[cfg(feature = "goldens")]
pub mod goldens;
pub mod hachure_fill;
mod options;
pub mod path_data;
pub mod points_on_curve;
pub mod points_on_path;
mod random;
pub mod renderer;

pub use crate::core::{Drawable, Op, OpSet, OpSetType, Shape};
pub use generator::RoughGenerator;
pub use options::{Options, Point};
pub use random::{random_seed, Random};
