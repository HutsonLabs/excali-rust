//! Font metadata, text measurement and wrapping.
//!
//! Upstream counterpart: `packages/common/src/font-metadata.ts`, `packages/element/src/textWrapping.ts`, `textMeasurements.ts`, the bound-text sizing of `textElement.ts` and `refreshTextDimensions` of `newElement.ts`, and the font registry of `packages/excalidraw/fonts/`.
//!
//! Targets: native, wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-core`.

pub mod font_assets;
pub mod font_faces;
mod font_faces_table;
pub mod font_metadata;
pub mod font_store;
pub mod new_element;
pub mod restore_env;
pub mod text_element;
pub mod text_measurements;
pub mod text_wrapping;
pub mod unicode_range;
