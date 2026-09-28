//! Font metadata, text measurement and wrapping.
//!
//! Upstream counterpart: `packages/common/src/font-metadata.ts`, `packages/element/src/textWrapping.ts`, `textMeasurements.ts`, and the font registry of `packages/excalidraw/fonts/`.
//!
//! Targets: native, wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-core`.

pub mod font_assets;
pub mod font_metadata;
