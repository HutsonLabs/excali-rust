//! The display list: what the scene draws, in a form every backend can
//! paint without knowing what an Excalidraw element is (ADR-008).
//!
//! Upstream renders by calling `CanvasRenderingContext2D` directly
//! (`packages/element/src/renderElement.ts`,
//! `packages/excalidraw/renderer/staticScene.ts`) and, for SVG, by building
//! DOM nodes (`renderer/staticSvgScene.ts`). The port splits that in two:
//! `excali-scene` turns elements into a [`DisplayList`], and each backend
//! turns the list into pixels or markup:
//!
//! - `excali-canvas2d`: `CanvasRenderingContext2D` calls in the browser;
//! - `excali-raster`: tiny-skia pixmaps natively (PNG export, CLI);
//! - `excali-svg`: SVG markup.
//!
//! # Items
//!
//! [`DisplayItem`] is the whole vocabulary, one variant per canvas draw
//! upstream makes:
//!
//! | item | canvas calls |
//! |---|---|
//! | [`DisplayItem::Fill`] | `fillStyle`, the [`Path`], `fill(rule)` |
//! | [`DisplayItem::FillRect`] | `fillStyle`, `fillRect(x, y, w, h)` |
//! | [`DisplayItem::Stroke`] | `strokeStyle`, `lineWidth`, `lineCap`, `lineJoin`, `miterLimit`, `setLineDash`, `lineDashOffset`, the path, `stroke()` |
//! | [`DisplayItem::Image`] | `imageSmoothingEnabled`, `filter`, `drawImage` with a source and destination rectangle |
//! | [`DisplayItem::Text`] | `font`, `fillStyle`, `textAlign`, `direction`, `fillText` |
//! | [`DisplayItem::Group`] | `save()`, `transform(…)`, `globalAlpha *= opacity`, optionally the clip path and `clip(rule)`, the children, `restore()` |
//!
//! # Semantics
//!
//! The list means what the same calls mean on a canvas, so the Canvas 2D
//! backend is a transliteration and the others reproduce it:
//!
//! - **Order.** Items paint in list order, source-over, each on top of the
//!   ones before.
//! - **Transforms** compose as `ctx.transform` does
//!   ([`Transform::concat`]): a group's matrix applies to its children
//!   before the parent's. A group matrix with an infinite or NaN entry is
//!   ignored as a whole, as `ctx.transform` ignores such arguments. The
//!   canvas drops only the one call with the bad argument, so a producer
//!   emits each canvas call as its own nested group (see
//!   [`Group::transform`]).
//! - **Opacity** is `globalAlpha`, multiplied through nested groups and
//!   applied to each draw on its own (overlapping strokes in one element
//!   darken where they cross, as upstream's rough strokes do). An opacity
//!   whose product leaves `0..=1` is ignored, as the canvas ignores such an
//!   assignment.
//! - **Clips** are in the group's coordinate space after its transform and
//!   intersect with the enclosing clips; they end with the group.
//! - **Colours** are CSS strings kept verbatim ([`Color`]), as upstream
//!   assigns them to `fillStyle` and `strokeStyle`. The Canvas 2D backend
//!   hands the string to the browser; the others resolve it with the CSS
//!   Color 4 parser the canvas uses ([`Color::rgba`], checked against
//!   Chrome). An assignment the canvas ignores (`""`, `"none"`, a stored
//!   `"blue-ish"`) leaves the current style, so the draw paints in it:
//!   black on a fresh context, as upstream's element canvases are, or the
//!   base state's [`PaintState::fill_style`] / [`PaintState::stroke_style`].
//! - **Value rules** follow the canvas specification: [`Dash::new`] (odd
//!   dash lists repeat, invalid ones draw solid), [`Stroke::effective_width`]
//!   (a width of 0 or less draws 1), [`Path::canonical`] (implicit subpath
//!   starts, arcs, non-finite arguments), [`Path::round_rect`].
//!
//! [`DisplayList::replay`] walks the tree and hands a backend's [`Painter`]
//! each draw with its absolute matrix, alpha and resolved colour, so
//! backends share one reading of these rules.
//!
//! # Documents
//!
//! An SVG export also has a document around the drawing ([`SvgDocument`]:
//! its size, the embedded scene, the frame clip paths, the fonts to inline
//! and the background), computed from the elements by
//! `excali_scene::export`. Its drawing is not a list of draws but the
//! markup upstream's `renderSceneToSvg` builds, one group per element, as
//! [`SvgNode`] trees whose values ([`SvgValue`]) the SVG backend prints.
//! Numbers are printed as JavaScript prints them ([`number_to_string`],
//! [`to_fixed`]).

mod builtin;
mod css_color;
mod document;
mod image;
mod number;
mod paint;
mod path;
mod path_data;
mod replay;
mod svg;
mod text;
mod transform;

pub use builtin::encode_uri_component;
pub use builtin::{
    builtin_image, builtin_image_by_id, BuiltinImage, BUILTIN_IMAGE_NAMES, ELEMENT_LINK_ID,
    EXTERNAL_LINK_ID, IMAGE_ERROR_PLACEHOLDER_ID, IMAGE_PLACEHOLDER_ID,
};
pub use document::{
    CanvasDocument, FontFaceSource, FrameClip, PngPayload, SvgDocument, SvgPayload,
};
pub use image::{ImageFilter, ImageItem, Rect};
pub use number::{number_to_string, to_fixed};
pub use paint::{Color, Dash, LineCap, LineJoin, Rgba, Stroke};
pub use path::{FillRule, Path, PathCommand};
pub use replay::{PaintState, Painter};
pub use svg::{SvgNode, SvgTag, SvgValue};
pub use text::{Direction, Font, TextAlign, TextRun};
pub use transform::Transform;

/// One draw, or a group of them.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayItem {
    /// Fill a path with a colour.
    Fill {
        path: Path,
        color: Color,
        rule: FillRule,
    },
    /// `fillRect`: fill a rectangle with a colour. The canvas draws it as a
    /// rectangle (Skia's `drawRect`), which anti-aliases its edges more
    /// finely than the same rectangle filled as a path, so upstream's
    /// `fillRect` calls (the image placeholder's box) stay rectangles.
    FillRect { rect: Rect, color: Color },
    /// Stroke a path.
    Stroke { path: Path, stroke: Stroke },
    /// Draw a bitmap into a rectangle.
    Image(ImageItem),
    /// Draw one line of text.
    Text(TextRun),
    /// Draw children under a transform, opacity and clip.
    Group(Group),
}

/// Children drawn inside `save()`/`restore()` with a transform, an opacity
/// and an optional clip: how upstream draws each element
/// (`renderElement.ts:1111-1190`: translate to the centre, rotate, set
/// `globalAlpha`) and each frame's children (`staticScene.ts:165-189`).
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// Multiplied into the parent's matrix (`ctx.transform`). When any
    /// entry is infinite or NaN the whole matrix is ignored, as the canvas
    /// ignores such a call.
    ///
    /// Upstream positions an element with separate calls,
    /// `translate(cx, cy)` then `rotate(angle)` (`renderElement.ts`), and
    /// the canvas drops only the call with the non-finite argument: an
    /// element with a NaN angle still draws translated, unrotated. A single
    /// group holding `translate(cx, cy) × rotate(NaN)` would drop the
    /// translation too. Producers therefore emit one group per canvas call
    /// (a group with the translation holding a group with the rotation), or
    /// leave out only the non-finite factor, to keep upstream's result.
    pub transform: Transform,
    /// Multiplied into the parent's alpha; 1 leaves it unchanged.
    pub opacity: f64,
    /// Applied after `transform`, in the group's coordinates.
    pub clip: Option<Clip>,
    pub items: Vec<DisplayItem>,
}

impl Group {
    /// A group with an identity transform, opacity 1 and no clip.
    pub fn new(items: Vec<DisplayItem>) -> Self {
        Self {
            transform: Transform::IDENTITY,
            opacity: 1.0,
            clip: None,
            items,
        }
    }
}

/// A clip region: `clip(rule)` on a path.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub path: Path,
    pub rule: FillRule,
}

/// What the scene draws, in paint order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    pub items: Vec<DisplayItem>,
}

impl DisplayList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: DisplayItem) {
        self.items.push(item);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, DisplayItem> {
        self.items.iter()
    }
}

impl FromIterator<DisplayItem> for DisplayList {
    fn from_iter<I: IntoIterator<Item = DisplayItem>>(iter: I) -> Self {
        Self {
            items: iter.into_iter().collect(),
        }
    }
}

impl Extend<DisplayItem> for DisplayList {
    fn extend<I: IntoIterator<Item = DisplayItem>>(&mut self, iter: I) {
        self.items.extend(iter);
    }
}

impl<'a> IntoIterator for &'a DisplayList {
    type Item = &'a DisplayItem;
    type IntoIter = std::slice::Iter<'a, DisplayItem>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}
