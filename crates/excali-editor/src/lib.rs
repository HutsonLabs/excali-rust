//! Editor state machine: tools, hit testing, transforms, binding, snapping and history, returning effects instead of touching the DOM.
//!
//! Upstream counterpart: `App.tsx` interaction code, `collision.ts`, `transformHandles.ts`, `binding.ts`, `snapping.ts`, `linearElementEditor.ts`, `store.ts`, `history.ts`, `actions/*`.
//!
//! Targets: native (tests), wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-scene`.
