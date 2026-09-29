//! Editor state machine: tools, hit testing, transforms, binding, snapping and history, returning effects instead of touching the DOM.
//!
//! Upstream counterpart: `App.tsx` interaction code, `collision.ts`, `transformHandles.ts`, `binding.ts`, `snapping.ts`, `linearElementEditor.ts`, `store.ts`, `history.ts`, `actions/*`.
//!
//! Targets: native (tests), wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-scene`.
//!
//! - [`collision`]: hit testing (`packages/element/src/collision.ts`,
//!   `App.getElementHitThreshold`, `App.hitElement`): thresholds, the
//!   inside and outline rules, per-shape intersections and binding hit
//!   tests; [`distance`]: distance to an element's outline
//!   (`distance.ts`).
//! - [`mutate`]: `mutateElement`, `newElementWith` and `bumpVersion`, the
//!   only ways an element changes, each bumping `version`, `versionNonce`
//!   and `updated`.
//! - [`delta`]: element and app state deltas (`delta.ts`): calculate,
//!   inverse, squash, apply with binding repair and visibility.
//! - [`store`]: snapshots and capture actions (`store.ts`), emitting the
//!   increments history records.
//! - [`history`]: the undo and redo stacks (`history.ts`).
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
//! - [`tools`]: the tool registry (`TOOLS`, `findShapeByKey`), the active
//!   tool, the tool lock and pen mode (`components/Tools.tsx`,
//!   `setActiveTool`, `toggleLock`, `togglePenMode`).
//! - [`viewport`]: zoom limits and normalisation, the viewport/scene
//!   coordinate transforms, scroll locks, the wheel, the zoom actions and
//!   zoom-to-fit (`viewport.ts`, `App.wheel.ts`, `App.viewport.ts`,
//!   `actionCanvas.tsx`).

mod binary_heap;
pub mod collision;
pub mod delta;
pub mod distance;
pub mod elbow_arrow;
pub mod geometry;
pub mod history;
mod js_value;
pub mod mutate;
pub mod resize_elements;
pub mod resize_test;
pub mod restore_env;
pub mod scene;
pub mod session;
pub mod store;
pub mod tools;
pub mod transform;
pub mod transform_handles;
pub mod viewport;
