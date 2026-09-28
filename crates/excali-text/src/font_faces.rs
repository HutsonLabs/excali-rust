//! The font faces upstream registers, and the file the port vendors for each.
//!
//! Upstream: `Fonts.init()` (`packages/excalidraw/fonts/Fonts.ts:375-416`)
//! registers every family's faces from `packages/excalidraw/fonts/<Dir>/index.ts`
//! (`ExcalidrawFontFaceDescriptor`: a `uri` and optional `unicodeRange` and
//! `weight` descriptors); the UI family Assistant is declared in
//! `fonts/fonts.css`. [`FONT_FACES`] is generated from those sources by
//! `scripts/fonts/font_faces.py`, which CI runs against the pinned checkout.
//!
//! The vendored files live under `crates/excali-text/assets/fonts/<Dir>/`,
//! next to their licences (ADR-004, ex-307). They are upstream's files, except Liberation
//! Sans: upstream's 1.05 file is a licence gap and the port ships the OFL
//! build 2.1.5 (ADR-004, Vendored builds).

pub use crate::font_faces_table::FONT_FACES;

/// One registered font face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontFaceDescriptor {
    /// The CSS family name the face registers under (`"Excalifont"`,
    /// `"Lilita One"`, `"Xiaolai"`).
    pub family: &'static str,
    /// Upstream's file, relative to `packages/excalidraw/fonts/`; `None` for
    /// a `local:` face (Helvetica, Segoe UI Emoji), which upstream never
    /// bundles.
    pub upstream: Option<&'static str>,
    /// The port's file, relative to `crates/excali-text/assets/fonts/`;
    /// `None` for a `local:` face.
    pub vendored: Option<&'static str>,
    /// The CSS `unicode-range` descriptor; `None` covers every code point.
    pub unicode_range: Option<&'static str>,
    /// The CSS `font-weight` descriptor; `None` is `normal` (400).
    pub weight: Option<&'static str>,
}

impl FontFaceDescriptor {
    /// Whether upstream loads this face from the host (`LOCAL_FONT_PROTOCOL`).
    pub fn is_local(&self) -> bool {
        self.upstream.is_none()
    }
}

/// The faces registered under `family` (compared ASCII case-insensitively,
/// as CSS family names are), in registration order.
pub fn font_faces(family: &str) -> impl Iterator<Item = &'static FontFaceDescriptor> + '_ {
    FONT_FACES
        .iter()
        .filter(move |f| f.family.eq_ignore_ascii_case(family))
}
