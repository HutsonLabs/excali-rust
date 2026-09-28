//! ex-408: font subsetting for SVG export. Upstream inlines each font face
//! subset to the scene's characters (HarfBuzz in a worker); the port inlines
//! the whole vendored file. This package measures the Rust candidates
//! against upstream's own subsets, for ADR-010.
//!
//! - [`candidates`]: the subsetters and WOFF2 encoders, called as upstream
//!   calls HarfBuzz;
//! - [`fidelity`]: a subset compared with its original face (cmap, glyphs,
//!   metrics, shaping);
//! - [`report`]: every candidate on every face of `upstream-subsets.json`;
//! - [`decision`]: ADR-010's rule applied to the report and to `wasm.json`;
//! - [`probe`]: the wasm32 build probe `wasm.sh` compiles.

pub mod candidates;
pub mod decision;
pub mod fidelity;
pub mod probe;
pub mod report;
