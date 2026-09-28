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
//! - [`elbow_arrow`]: elbow arrow routing, `updateElbowArrowPoints` and the
//!   A* search over a non-uniform grid (`packages/element/src/elbowArrow.ts`).
//! - [`geometry`]: what the router reads off a binding target (bounds,
//!   centre, outline distance, side headings, fixed points).
//! - [`restore_env`]: [`restore_env::RoutingEnv`], the restore environment
//!   that answers `restoreElements`' elbow arrow re-route with the router.
//! - [`tools`]: the tool registry (`TOOLS`, `findShapeByKey`), the active
//!   tool, the tool lock and pen mode (`components/Tools.tsx`,
//!   `setActiveTool`, `toggleLock`, `togglePenMode`).

mod binary_heap;
pub mod collision;
pub mod distance;
pub mod elbow_arrow;
pub mod geometry;
pub mod restore_env;
pub mod tools;
