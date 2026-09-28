//! DOM chrome (toolbar, panels, dialogs) that applies editor effects.
//!
//! Upstream counterpart: `packages/excalidraw/components/*`, and for
//! [`fonts`] the scene font loading of `packages/excalidraw/fonts/Fonts.ts`.
//!
//! Targets: wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-canvas2d`, `excali-editor`.

pub mod fonts;
