//! What an SVG export writes around the drawing: upstream's `exportToSvg`
//! (`packages/excalidraw/scene/export.ts:293-508`) builds the root, the
//! `svg-source` comment, `<metadata>` with the embedded scene, `<defs>`
//! with a clip path per frame and the `style-fonts` block, and the
//! background rectangle before it renders any element. The scene computes
//! those parts from the elements (`excali_scene::export`); the SVG backend
//! writes them. A PNG export's counterpart is the canvas `exportToCanvas`
//! sizes and draws on, with the scene `encodePngMetadata` embeds
//! (`excali_scene::canvas_export`); the raster backend encodes it.
//! Everything here is plain data.

/// The document `exportToSvg` builds around the elements.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgDocument {
    /// The canvas width and height (`getCanvasSize`, `export.ts:566-576`):
    /// the `viewBox` is `0 0 width height`.
    pub width: f64,
    pub height: f64,
    /// `exportScale`: the root's `width` and `height` attributes are the
    /// canvas size times this.
    pub scale: f64,
    /// Where scene coordinates land in the document: `-minX + padding`,
    /// `-minY + padding`.
    pub offset_x: f64,
    pub offset_y: f64,
    /// The scene embedded in `<metadata>` (`encodeSvgBase64Payload`,
    /// `export.ts:510-529`), with `exportEmbedScene`.
    pub payload: Option<SvgPayload>,
    /// One clip path per frame-like element, in element order
    /// (`export.ts:398-437`).
    pub frame_clips: Vec<FrameClip>,
    /// The font faces the `style-fonts` block inlines, in order
    /// (`Fonts.generateFontFaceDeclarations`); none with
    /// `skipInliningFonts`.
    pub font_faces: Vec<FontFaceSource>,
    /// The background rectangle's fill, after the dark-mode filter, when
    /// `exportBackground` is set and the background colour is not empty.
    pub background: Option<String>,
    /// The image `<symbol>`s `renderSceneToSvg` puts first in `<defs>`, in
    /// document order (each new one goes before the others:
    /// `defs.prepend(symbol)`, `staticSvgScene.ts:621`).
    pub symbols: Vec<super::SvgNode>,
    /// What `renderSceneToSvg` appends to the root after the background:
    /// the elements' nodes, in order (`staticSvgScene.ts:850-933`).
    pub nodes: Vec<super::SvgNode>,
}

/// The canvas a PNG export encodes: `exportToCanvas`
/// (`packages/excalidraw/scene/export.ts:180-285`) sizes it and draws the
/// scene on it; the scene computes both from the elements
/// (`excali_scene::canvas_export`), and the raster backend paints the list
/// on a pixmap of this size and encodes it as `canvas.toBlob()` does.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasDocument {
    /// `canvas.width` and `canvas.height`, in device pixels, as the
    /// canvas's `width` and `height` attributes hold them.
    pub width: u32,
    pub height: u32,
    /// What `renderStaticScene` draws on the canvas, from a fresh context:
    /// the export scale is its outermost transform.
    pub list: super::DisplayList,
    /// The scene `encodePngMetadata` embeds (`data/image.ts:25-47`), with
    /// `exportEmbedScene`.
    pub payload: Option<PngPayload>,
}

/// An embedded scene in a PNG: a `tEXt` chunk (png-chunk-text 1.0.0's
/// `encode(keyword, content)`) holding `JSON.stringify(encode({ text,
/// compress: true }))`, inserted before `IEND`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PngPayload {
    /// `MIME_TYPES.excalidraw`.
    pub keyword: String,
    /// The payload wrapper's JSON: every char below U+0100, no NUL.
    pub text: String,
}

impl PngPayload {
    /// The chunk's data: the keyword, a NUL and the text, one byte per
    /// char (Latin-1).
    pub fn chunk_data(&self) -> Vec<u8> {
        let latin1 = |s: &str| {
            s.chars()
                .map(|c| u8::try_from(c).unwrap_or(b'?'))
                .collect::<Vec<u8>>()
        };
        let mut data = latin1(&self.keyword);
        data.push(0);
        data.extend(latin1(&self.text));
        data
    }
}

/// An embedded scene: the `payload-type` and `payload-version` comments and
/// the base64 text between `payload-start` and `payload-end`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvgPayload {
    /// `MIME_TYPES.excalidraw`.
    pub mime_type: String,
    /// 2: the base64 of the compressed payload wrapper's JSON.
    pub version: u32,
    pub base64: String,
}

/// A frame's `<clipPath id=…>`: a rectangle placed by
/// `translate(x y) rotate(angle cx cy)`.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameClip {
    /// The frame's id, the clip path's `id`.
    pub id: String,
    /// `frame.x + offsetX`, `frame.y + offsetY`.
    pub x: f64,
    pub y: f64,
    /// `frame.angle`, written in the `rotate()` as it is stored (radians,
    /// where SVG reads degrees: upstream's own quirk).
    pub angle: f64,
    /// The rotation centre, relative to the frame's corner.
    pub cx: f64,
    pub cy: f64,
    pub width: f64,
    pub height: f64,
    /// `rx` and `ry` (`FRAME_STYLE.radius`), unless exporting that frame.
    pub radius: Option<f64>,
}

/// One `@font-face` rule to inline: the face and the characters it is to
/// hold. The rule's content (upstream's subset woff2 as a data URL) comes
/// from the backend's font source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFaceSource {
    /// The CSS family name.
    pub family: String,
    /// The vendored file, relative to the font asset directory.
    pub file: String,
    /// The file upstream names for this face, relative to
    /// `packages/excalidraw/fonts/`; the same as `file` except where
    /// ADR-004 substitutes a licensed file.
    pub upstream_file: String,
    /// The CSS `format()` of the file: `woff2` or `truetype`.
    pub format: &'static str,
    /// Every character of the family in the scene, in first-use order.
    pub characters: String,
    /// What upstream writes when it cannot fetch the file: the file's URL
    /// under its asset fallback (`ExcalidrawFontFace.getContent`).
    pub fallback_url: String,
}
