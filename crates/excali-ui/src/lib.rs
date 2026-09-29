//! DOM chrome (toolbar, panels, dialogs) that applies editor effects.
//!
//! Upstream counterpart: `packages/excalidraw/components/*`; for
//! [`primitives`] the UI kit (`Island`, `Stack`, `Button`, `IconButton`,
//! `RadioGroup`, `Range`, `TextField`, `Popover`, `Modal`, `Dialog`,
//! `Tooltip`) built with [`dom`]; for [`layers`]
//! the canvases (`components/canvases/*`) and the helpers they render
//! through (`renderer/helpers.ts`); for [`fonts`] the scene font loading of
//! `packages/excalidraw/fonts/Fonts.ts`; for [`keyboard`] the key events
//! as `App.onKeyDown` reads them (`common/src/utils.ts`); for [`text_editor`]
//! the DOM half of the text editor overlay (`wysiwyg/textWysiwyg.tsx`); for
//! [`theme`] the light and dark tokens of `css/theme.scss` and the
//! container's `theme--dark` class.
//!
//! Targets: wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-canvas2d`, `excali-editor`.

pub mod dom;
pub mod fonts;
pub mod keyboard;
pub mod layers;
pub mod primitives;
pub mod text_editor;
pub mod theme;
