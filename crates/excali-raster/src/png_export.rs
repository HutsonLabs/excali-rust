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

use excali_scene::display::CanvasDocument;
use tiny_skia::Pixmap;

use crate::{render, ImageStore, TextRasterizer};

/// Why no PNG came out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PngExportError {
    /// `canvas.toBlob()` gave no blob: the canvas has no pixels (a width or
    /// height of 0), or it is larger than a pixmap can be. Upstream rejects
    /// with `CANVAS_POSSIBLY_TOO_BIG` (`data/blob.ts:237-257`).
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

/// Paint `doc` on a transparent canvas of its size and encode it: `IHDR`
/// (8-bit RGBA), the image data, the scene's `tEXt` chunk when it has one,
/// `IEND`.
pub fn export_png<I: ImageStore, T: TextRasterizer>(
    doc: &CanvasDocument,
    images: &I,
    text: &mut T,
) -> Result<Vec<u8>, PngExportError> {
    if doc.width == 0 || doc.height == 0 {
        return Err(PngExportError::CanvasTooBig);
    }
    let mut pixmap = Pixmap::new(doc.width, doc.height).ok_or(PngExportError::CanvasTooBig)?;
    render(&doc.list, &mut pixmap, images, text);

    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, doc.width, doc.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&rgba)?;
        if let Some(payload) = &doc.payload {
            // chunks.splice(-1, 0, metadataChunk): before IEND, which
            // finish() writes
            writer.write_chunk(png::chunk::tEXt, &payload.chunk_data())?;
        }
        writer.finish()?;
    }
    Ok(out)
}
