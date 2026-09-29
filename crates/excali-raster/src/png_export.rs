//! The PNG a canvas export writes: the canvas `exportToCanvas` sized and
//! drew, encoded as `canvas.toBlob()` encodes it, with the embedded scene
//! (`encodePngMetadata`, `packages/excalidraw/data/image.ts:25-47`) as a
//! `tEXt` chunk inserted before `IEND`.
//!
//! The canvas starts transparent; the list paints the background when
//! there is one. Chrome's encoder writes the unpremultiplied colour of
//! each pixel as 8-bit RGBA; so does this one (tiny-skia's
//! demultiplication). Pixel data aside, a PNG decoder sees the same
//! image, and upstream's `decodePngMetadata` the same scene.

use excali_scene::display::{CanvasDocument, PngPayload};
use tiny_skia::Pixmap;

use crate::{render, ImageStore, TextRasterizer};

/// Why no PNG came out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PngExportError {
    /// `canvas.toBlob()` gave no blob: the canvas has no pixels (a width or
    /// height of 0), or it is past a browser's canvas limits
    /// ([`MAX_CANVAS_SIDE`], [`MAX_CANVAS_AREA`]). Upstream rejects with
    /// `CANVAS_POSSIBLY_TOO_BIG` (`data/blob.ts:237-257`).
    CanvasTooBig,
    /// The PNG encoder failed.
    Encoding(String),
}

impl std::fmt::Display for PngExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PngExportError::CanvasTooBig => f.write_str("Error: Canvas too big"),
            PngExportError::Encoding(why) => write!(f, "PNG encoding failed: {why}"),
        }
    }
}

impl std::error::Error for PngExportError {}

impl From<png::EncodingError> for PngExportError {
    fn from(e: png::EncodingError) -> Self {
        PngExportError::Encoding(e.to_string())
    }
}

/// The widest or tallest canvas exported: 32767 px, the side limit upstream
/// assumes for browsers (`WIDTH_HEIGHT_LIMIT`,
/// `packages/element/src/renderElement.ts:232-233`, "safari width/height
/// limit based on developer.mozilla.org") and Firefox's (canvas-size test
/// results, <https://jhildenbiddle.github.io/canvas-size/#/?id=test-results>,
/// read 2026-09-28). It also keeps every device coordinate inside the 16.16
/// fixed point the rasterizer's edge setup uses.
pub const MAX_CANVAS_SIDE: u32 = 32_767;

/// The most pixels a canvas exported has: 268,435,456 (16384 x 16384), the
/// area limit of desktop Chrome, Edge and Safari in the canvas-size test
/// results (same page). A bigger canvas fails `toBlob` in the browser; here
/// it would allocate gigabytes before failing.
pub const MAX_CANVAS_AREA: u64 = 268_435_456;

/// Paint `doc` on a transparent canvas of its size and encode it: `IHDR`
/// (8-bit RGBA), the image data, the scene's `tEXt` chunk when it has one,
/// `IEND`. [`paint_canvas`] then [`encode_png`].
pub fn export_png<I: ImageStore, T: TextRasterizer>(
    doc: &CanvasDocument,
    images: &I,
    text: &mut T,
) -> Result<Vec<u8>, PngExportError> {
    let pixmap = paint_canvas(doc, images, text)?;
    encode_png(&pixmap, doc.payload.as_ref())
}

/// The canvas `doc` describes, painted: a transparent pixmap of its size
/// with the list drawn on it, as a caller that draws the canvas somewhere
/// else (`ctx.drawImage(canvas, ...)`) holds it. A canvas without pixels
/// or past the browser limits is [`PngExportError::CanvasTooBig`], checked
/// before anything is allocated.
pub fn paint_canvas<I: ImageStore, T: TextRasterizer>(
    doc: &CanvasDocument,
    images: &I,
    text: &mut T,
) -> Result<Pixmap, PngExportError> {
    if doc.width == 0
        || doc.height == 0
        || doc.width > MAX_CANVAS_SIDE
        || doc.height > MAX_CANVAS_SIDE
        || u64::from(doc.width) * u64::from(doc.height) > MAX_CANVAS_AREA
    {
        return Err(PngExportError::CanvasTooBig);
    }
    let mut pixmap = Pixmap::new(doc.width, doc.height).ok_or(PngExportError::CanvasTooBig)?;
    render(&doc.list, &mut pixmap, images, text);
    Ok(pixmap)
}

/// `canvas.toBlob()` of a painted canvas: 8-bit RGBA, each pixel's
/// unpremultiplied colour, with `payload`'s `tEXt` chunk before `IEND`.
pub fn encode_png(
    pixmap: &Pixmap,
    payload: Option<&PngPayload>,
) -> Result<Vec<u8>, PngExportError> {
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, pixmap.width(), pixmap.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&rgba)?;
        if let Some(payload) = payload {
            // chunks.splice(-1, 0, metadataChunk): before IEND, which
            // finish() writes
            writer.write_chunk(png::chunk::tEXt, &payload.chunk_data())?;
        }
        writer.finish()?;
    }
    Ok(out)
}
