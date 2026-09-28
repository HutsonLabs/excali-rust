//! Element shapes and the display list the render backends consume.
//!
//! - [`display`]: the [`display::DisplayList`], the renderer-independent
//!   list of fills, strokes, images, text runs and groups (transform,
//!   opacity, clip) that `excali-raster`, `excali-canvas2d` and
//!   `excali-svg` paint without knowing about elements (ADR-008).
//! - [`rough_canvas`]: roughjs's `RoughCanvas.draw`, producing display items.
//! - [`rough_options`]: `generateRoughOptions` and `adjustRoughness`.
//!
//! Upstream counterpart: `packages/element/src/shape.ts`, `renderElement.ts`, `packages/excalidraw/renderer/staticScene.ts`.
//!
//! Targets: native, wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-core`, `excali-freehand`, `excali-rough`, `excali-text`.

pub mod bounds;
pub mod display;
pub mod rough_canvas;
pub mod rough_options;
pub mod shape;
pub mod utils;
