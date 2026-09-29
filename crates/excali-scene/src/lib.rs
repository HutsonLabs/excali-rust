//! Element shapes and the display list the render backends consume.
//!
//! - [`display`]: the [`display::DisplayList`], the renderer-independent
//!   list of fills, strokes, images, text runs and groups (transform,
//!   opacity, clip) that `excali-raster`, `excali-canvas2d` and
//!   `excali-svg` paint without knowing about elements (ADR-008).
//! - [`bounds`]: element geometry: diamond vertices, arrowhead points, and
//!   the boxes of elements (`getElementAbsoluteCoords`, `getElementBounds`,
//!   `getCommonBounds`); [`linear_element`]: the boxes of lines and arrows
//!   and where an arrow's label sits.
//! - [`frame`]: frame membership and the clip of a frame's children
//!   (`frame.ts`, `clipElementToFrame`), over the outlines of
//!   [`bounds::get_element_line_segments`] and [`geometric_shape`]
//!   (`packages/utils/src/shape.ts`, `getElementShape`).
//! - [`export`]: what `exportToSvg` computes from the elements (frame
//!   labels, canvas size, embedded scene, frame clips, fonts, background)
//!   as the [`display::SvgDocument`] `excali-svg` writes.
//! - [`freedraw`]: a freedraw outline as the SVG path upstream fills with
//!   the stroke colour (`getSvgPathFromStroke`, two-decimal trimming), and
//!   the element entry points `getFreedrawOutlinePoints` /
//!   `getFreeDrawSvgPath` (perfect-freehand for variable width, the laser
//!   pointer for constant width).
//! - [`svg_scene`]: `renderSceneToSvg`, the elements of an SVG export as
//!   the markup upstream builds, [`display::SvgNode`] trees.
//! - [`static_scene`]: `renderStaticScene`, the canvas's background, grid
//!   and elements in upstream's order as a display list, drawing each
//!   element with [`render_element`] (`renderElement`);
//!   [`new_element_scene`]: `renderNewElementScene`, the element being
//!   created alone on the canvas above it.
//! - [`rough_canvas`]: roughjs's `RoughCanvas.draw`, producing display items.
//! - [`rough_options`]: `generateRoughOptions` and `adjustRoughness`.
//! - [`shape`]: the rough.js shapes of boxes, lines and arrows, and a
//!   freedraw's loop fill under its stroke path;
//!   [`elbow_arrow`] and [`heading`]: the elbow arrow path and
//!   `validateElbowPoints`.
//!
//! Upstream counterpart: `packages/element/src/shape.ts`, `bounds.ts`, `renderElement.ts`, `packages/excalidraw/renderer/staticScene.ts`, `renderer/renderNewElementScene.ts`, `packages/excalidraw/scene/export.ts`.
//!
//! Targets: native, wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-core`, `excali-freehand`, `excali-rough`, `excali-text`.

pub mod bounds;
pub mod canvas_export;
pub mod display;
pub mod elbow_arrow;
pub mod element_canvas;
pub mod export;
pub mod frame;
pub mod freedraw;
pub mod geometric_shape;
pub mod heading;
pub mod linear_element;
pub mod new_element_scene;
pub mod render_element;
pub mod rough_canvas;
pub mod rough_options;
pub mod shape;
pub mod static_scene;
pub mod svg_scene;
pub mod utils;
