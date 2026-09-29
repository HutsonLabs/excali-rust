//! Editor state machine: tools, hit testing, transforms, binding, snapping and history, returning effects instead of touching the DOM.
//!
//! Upstream counterpart: `App.tsx` interaction code, `collision.ts`, `transformHandles.ts`, `binding.ts`, `snapping.ts`, `linearElementEditor.ts`, `store.ts`, `history.ts`, `actions/*`.
//!
//! Targets: native (tests), wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-scene`.
//!
//! - [`actions`]: the actions registry as data (`actions/*`): the 99
//!   action names, each registered action's key test, predicate, label and
//!   flags, the action manager's key and gate logic, and the context
//!   menus, command palette commands, styles panel and main menu built
//!   from it.
//! - [`binding`]: arrow binding (`binding.ts`): the gap and distance,
//!   fixed points, the binding strategies for dragged ends (modes inside,
//!   orbit and skip), binding and unbinding, and bound arrows following
//!   their elements; [`binding_highlight`]: the outline and midpoints the
//!   interactive canvas draws around the element an end would bind to;
//!   [`linear_element_editor`]: the point geometry and `movePoints` of
//!   `LinearElementEditor` binding uses.
//! - [`collision`]: hit testing (`packages/element/src/collision.ts`,
//!   `App.getElementHitThreshold`, `App.hitElement`): thresholds, the
//!   inside and outline rules, per-shape intersections and binding hit
//!   tests; [`distance`]: distance to an element's outline
//!   (`distance.ts`).
//! - [`keyboard`]: `App.onKeyDown` and `App.onKeyUp`: the keys the
//!   actions do not own (tool letters, arrow nudges, PgUp/PgDn, Enter,
//!   Space, S/G/Shift+F pickers, the eyedropper, Tab conversion, the
//!   flowchart keys, Ctrl held for binding) and the modifier helpers of
//!   `keys.ts`.
//! - [`mutate`]: `mutateElement`, `newElementWith` and `bumpVersion`, the
//!   only ways an element changes, each bumping `version`, `versionNonce`
//!   and `updated`.
//! - [`delta`]: element and app state deltas (`delta.ts`): calculate,
//!   inverse, squash, apply with binding repair and visibility.
//! - [`store`]: snapshots and capture actions (`store.ts`), emitting the
//!   increments history records.
//! - [`history`]: the undo and redo stacks (`history.ts`).
//! - [`edit_actions`]: the `perform`s of the editing actions (select
//!   all, delete, duplicate, group, z-order, paste).
//! - [`new_element`]: the element a drawing tool creates and its size as
//!   the pointer drags (`newElement.ts`, `dragNewElement`); [`eraser`]:
//!   what an eraser path erases (`EraserTrail`).
//! - [`selection`]: box selection (`getElementsWithinSelection`,
//!   `elementsOverlappingBBox`); [`groups`]: selecting whole groups
//!   (`groups.ts`).
//! - [`session`]: the scene, app state, store and history wired as `App`
//!   wires them (`updateScene`, `syncActionResult`, undo and redo).
//! - [`elbow_arrow`]: elbow arrow routing, `updateElbowArrowPoints` and the
//!   A* search over a non-uniform grid (`packages/element/src/elbowArrow.ts`).
//! - [`geometry`]: what the router reads off a binding target (bounds,
//!   centre, outline distance, side headings, fixed points).
//! - [`restore_env`]: [`restore_env::RoutingEnv`], the restore environment
//!   that answers `restoreElements`' elbow arrow re-route with the router.
//! - [`transform_handles`]: where a selection's resize and rotation
//!   handles sit, by pointer type and zoom (`transformHandles.ts`);
//!   [`resize_test`]: which handle a pointer is on and its cursor
//!   (`resizeTest.ts`); [`resize_elements`]: resizing and rotating
//!   elements, aspect lock and centre resize (`resizeElements.ts`);
//!   [`transform`]: the gesture that drives them (`App.tsx`'s pointer-down
//!   and `maybeHandleResize`); [`scene`]: the elements and
//!   `mutateElement`.
//! - [`snapping`]: object snapping (`snapping.ts`): point and gap snaps
//!   at 8 / zoom for dragged, resized and new elements, the pointer's snap
//!   lines before drawing, and the canvas calls that draw the snap lines
//!   (`renderSnaps.ts`).
//! - [`tools`]: the tool registry (`TOOLS`, `findShapeByKey`), the active
//!   tool, the tool lock and pen mode (`components/Tools.tsx`,
//!   `setActiveTool`, `toggleLock`, `togglePenMode`).
//! - [`viewport`]: zoom limits and normalisation, the viewport/scene
//!   coordinate transforms, scroll locks, the wheel, the zoom actions and
//!   zoom-to-fit (`viewport.ts`, `App.wheel.ts`, `App.viewport.ts`,
//!   `actionCanvas.tsx`).
//! - [`text_layout`]: laying out bound text after an edit
//!   (`redrawTextBoundingBox`, the sticky note fit, arrow label
//!   positions, the original container heights), the layout undo and redo
//!   run through `HistoryEnv::redraw_text_bounding_box`.
//! - [`text_editing`]: the text editor overlay (`textWysiwyg.tsx`) and
//!   `App.startTextEditing` / `handleTextWysiwyg`: creating or picking the
//!   text, the textarea's style, typing, indenting, submitting.

pub mod actions;
mod binary_heap;
pub mod binding;
pub mod binding_highlight;
pub mod collision;
pub mod delta;
pub mod distance;
pub mod edit_actions;
pub mod elbow_arrow;
pub mod eraser;
pub mod geometry;
pub mod groups;
pub mod history;
mod js_value;
pub mod keyboard;
pub mod linear_element_editor;
pub mod mutate;
pub mod new_element;
pub mod resize_elements;
pub mod resize_test;
pub mod restore_env;
pub mod scene;
pub mod selection;
pub mod session;
pub mod snapping;
pub mod store;
pub mod text_editing;
pub mod text_layout;
pub mod tools;
pub mod transform;
pub mod transform_handles;
pub mod viewport;
