//! DOM chrome (toolbar, panels, dialogs) that applies editor effects.
//!
//! Upstream counterpart: `packages/excalidraw/components/*`; for [`layers`]
//! the canvases (`components/canvases/*`) and the helpers they render
//! through (`renderer/helpers.ts`); for [`fonts`] the scene font loading of
//! `packages/excalidraw/fonts/Fonts.ts`.
//!
//! Targets: wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-canvas2d`, `excali-editor`.

pub mod fonts;
pub mod layers;
