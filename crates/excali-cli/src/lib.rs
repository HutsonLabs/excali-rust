//! Command-line validation, export and library tools for `.excalidraw`
//! files: the `excali` binary.
//!
//! Upstream counterpart: none (new). Each command runs upstream's code
//! paths as the port has them:
//!
//! - `validate`: `loadSceneOrLibraryFromBlob`
//!   (`packages/excalidraw/data/blob.ts:138-196`) on a file: a `.excalidraw`
//!   or `.json` scene, a `.png` or `.svg` with an embedded scene, or an
//!   `.excalidrawlib` library, whose items are then restored as an import
//!   restores them (`parseLibraryJSON`, `blob.ts:218-228`) — [`input`];
//! - `render`: the editor's "Export image" to PNG, `exportCanvas("png")`
//!   (`data/index.ts:98-192`) with the text drawn from the vendored font
//!   files — [`export`], [`fonts`];
//! - `export`: the same export to SVG (`exportToSvg`, `scene/export.ts:293-508`);
//! - `lib list` and `lib merge`: a library's items, and libraries imported
//!   one into another as `mergeLibraryItems` merges them
//!   (`data/library.ts:145-157`), written by `serializeLibraryAsJSON`
//!   (`data/json.ts:137-145`) — [`library`].
//!
//! The commands and exit codes are documented in
//! `site/content/architecture/cli.md` ([`cli`]).
//!
//! Targets: native. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008):
//! `excali-raster`, `excali-svg`, and the crates below them.

pub mod cli;
pub mod env;
pub mod error;
pub mod export;
pub mod fonts;
pub mod input;
pub mod library;
