//! A bitmap drawn under an absolute matrix: how upstream draws each
//! element from its cached canvas in the editor (`drawElementFromCanvas`,
//! `renderElement.ts:762-934`). The scene decides the bitmap and the blit
//! (`crate::element_canvas`); a backend keeps the bitmaps by id and draws
//! them.

use super::{Clip, Rect, Transform};

/// The id a backend stores the bitmap cached under `key` by
/// (`bitmap:<key>`), apart from image file ids and built-in images.
pub fn bitmap_id(key: &str) -> String {
    format!("bitmap:{key}")
}

/// One `drawImage` of a whole bitmap, inside its own
/// `save()`/`restore()`.
#[derive(Clone, Debug, PartialEq)]
pub struct Blit {
    /// The bitmap ([`bitmap_id`]); a backend without it draws nothing.
    pub id: String,
    /// `globalAlpha`.
    pub alpha: f64,
    /// `imageSmoothingEnabled` when set; `None` leaves the context's.
    pub smoothing: Option<bool>,
    /// A clip applied first, under its own matrix.
    pub clip: Option<(Clip, Transform)>,
    /// The matrix the bitmap is drawn under (`setTransform`), absolute: a
    /// snapped blit's origin is a whole device pixel.
    pub transform: Transform,
    /// Where the whole bitmap lands, in the units of `transform`.
    pub dest: Rect,
}
