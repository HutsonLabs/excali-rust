//! The `.excalidraw` data model: types, serde, restore, fractional index, library and payload codecs.
//!
//! Upstream counterpart: `packages/element/src/types.ts`, `packages/excalidraw/data/*`, `packages/fractional-indexing`.
//!
//! Targets: native, wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-math`.

pub mod constants;
pub mod document;
pub mod element;
pub mod json;
mod layout;
