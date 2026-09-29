//! The layered canvases: the static canvas (the scene), the new-element
//! canvas (the element being created, only while there is one) and the
//! interactive canvas (selection, handles, cursors) on top, each backed at
//! device-pixel scale and each drawing at the scroll snapped to whole
//! device pixels.
//!
//! Upstream counterpart: `components/canvases/StaticCanvas.tsx`,
//! `NewElementCanvas.tsx` and `InteractiveCanvas.tsx`, mounted by
//! `components/App.tsx:2654-2760` in that order, and the helpers every
//! layer renders through (`renderer/helpers.ts:47-127`):
//!
//! - **Backing size.** A canvas's backing store is its CSS size times
//!   `window.devicePixelRatio`: `canvas.width = appState.width * scale` for
//!   the static canvas (`StaticCanvas.tsx:38-41`), React's
//!   `width={appState.width * scale}` attribute for the other two
//!   (`NewElementCanvas.tsx:56-57`, `InteractiveCanvas.tsx:208-209`); the
//!   canvas keeps whole pixels ([`Layer::backing_size`],
//!   [`canvas_dimension_from_property`],
//!   [`canvas_dimension_from_attribute`]). Every layer then draws in the
//!   CSS pixels of [`normalized_canvas_dimensions`]
//!   (`getNormalizedCanvasDimensions`: the backing size over the ratio).
//! - **Scroll snapping.** Every layer draws at
//!   [`snap_scroll_to_device_pixels`] of the scroll: `round(scroll × zoom ×
//!   dpr) / (zoom × dpr)`, so a pan never moves the scene by a fraction of
//!   a device pixel and the overlays sit exactly on the content
//!   ([`SceneViewport`]). Hit testing keeps the real scroll.
//! - **Bootstrap.** Before a layer draws, [`bootstrap_canvas`] sets the
//!   ratio's scale and clears the canvas, except that the static canvas
//!   skips the clear when its background is an opaque hex colour, which its
//!   scene's first draw repaints over every pixel.
//!
//! [`CanvasLayers`] is the three canvases in a document; the painting
//! functions ([`paint_static_layer`], [`paint_new_element_layer`],
//! [`paint_interactive_layer`]) work on any [`LayerContext`], so they are
//! tested natively against upstream's recorded draws
//! (`tests/canvas_layers.rs`) and in Chromium (`tests/web/layers`).

mod dom;

use excali_canvas2d::{paint, Context2d};
use excali_scene::display::{DisplayList, Rect, Transform};

pub use dom::CanvasLayers;
pub use excali_scene::static_scene::snap_scroll_to_device_pixels;

/// The canvases, bottom to top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    /// The scene (`StaticCanvas`), inside `div.excalidraw__canvas-wrapper`.
    Static,
    /// The element being created (`NewElementCanvas`), mounted only while
    /// there is one.
    NewElement,
    /// Selection, handles, snap lines and collaborators
    /// (`InteractiveCanvas`), which takes the pointer events.
    Interactive,
}

impl Layer {
    /// Mount order (`App.tsx:2654, 2693, 2724`).
    pub const ALL: [Layer; 3] = [Layer::Static, Layer::NewElement, Layer::Interactive];

    /// The canvas's `class` attribute: `StaticCanvas.tsx:56` adds
    /// `excalidraw__canvas static`, `NewElementCanvas.tsx:52` and
    /// `InteractiveCanvas.tsx:198` set theirs.
    pub const fn class_name(self) -> &'static str {
        match self {
            Layer::Static => "excalidraw__canvas static",
            Layer::NewElement => "excalidraw__canvas",
            Layer::Interactive => "excalidraw__canvas interactive",
        }
    }

    /// The backing store of this layer's canvas at CSS size `width` ×
    /// `height` and device pixel ratio `scale`: each side `css × scale` as
    /// the layer's component assigns it (a property for the static canvas,
    /// an attribute for the others), in the whole pixels the canvas keeps.
    pub fn backing_size(self, width: f64, height: f64, scale: f64) -> BackingSize {
        let dimension = match self {
            Layer::Static => canvas_dimension_from_property,
            Layer::NewElement | Layer::Interactive => canvas_dimension_from_attribute,
        };
        BackingSize {
            width: dimension(width * scale, DEFAULT_CANVAS_WIDTH),
            height: dimension(height * scale, DEFAULT_CANVAS_HEIGHT),
        }
    }
}

/// A canvas's backing store, in device pixels (`canvas.width`,
/// `canvas.height`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackingSize {
    pub width: u32,
    pub height: u32,
}

/// The `width` a canvas has without the attribute (HTML canvas element).
pub const DEFAULT_CANVAS_WIDTH: u32 = 300;
/// The `height` a canvas has without the attribute.
pub const DEFAULT_CANVAS_HEIGHT: u32 = 150;

/// The largest value an `unsigned long` reflected attribute keeps
/// (HTML "reflecting content attributes": 0 to 2147483647).
const MAX_REFLECTED: u32 = 2_147_483_647;

/// `canvas.width = x` (or `height`): the WebIDL `unsigned long` conversion
/// (non-finite → 0, truncated, modulo 2³²), then the reflected
/// attribute's range, the default outside it.
pub fn canvas_dimension_from_property(x: f64, default: u32) -> u32 {
    let n = if x.is_finite() {
        x.trunc().rem_euclid(4_294_967_296.0) as u32
    } else {
        0
    };
    if n <= MAX_REFLECTED {
        n
    } else {
        default
    }
}

/// `<canvas width={x}>` (or `height`) as React writes it: the attribute is
/// `String(x)`, which the canvas reads with the rules for parsing
/// non-negative integers (leading whitespace, an optional `+`, the digits
/// up to the first other character); the default when there are no
/// digits, the value is negative, or it is past the reflected range.
pub fn canvas_dimension_from_attribute(x: f64, default: u32) -> u32 {
    let text = excali_core::json::number_to_string(x);
    parse_non_negative_integer(&text)
        .filter(|n| *n <= u64::from(MAX_REFLECTED))
        .map_or(default, |n| n as u32)
}

/// The HTML rules for parsing non-negative integers, saturating past
/// `u64::MAX` (any such value is past the reflected range).
fn parse_non_negative_integer(text: &str) -> Option<u64> {
    let s = text.trim_start_matches([' ', '\t', '\n', '\u{c}', '\r']);
    let (negative, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let digits: &str = &digits[..digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len())];
    if digits.is_empty() {
        return None;
    }
    let value = digits.bytes().fold(0u64, |n, d| {
        n.saturating_mul(10).saturating_add(u64::from(d - b'0'))
    });
    // "-0" is zero, any other negative number an error
    (!negative || value == 0).then_some(value)
}

/// `getNormalizedCanvasDimensions(canvas, scale)` (`helpers.ts:65-71`):
/// the backing size over the device pixel ratio, the CSS pixels a layer
/// draws in.
pub fn normalized_canvas_dimensions(size: BackingSize, scale: f64) -> (f64, f64) {
    (
        f64::from(size.width) / scale,
        f64::from(size.height) / scale,
    )
}

/// Where a layer draws the scene: the scroll, zoom and device pixel ratio,
/// with the scroll snapped to whole device pixels by [`Self::snapped`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneViewport {
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub zoom: f64,
    /// The device pixel ratio.
    pub scale: f64,
}

impl SceneViewport {
    pub const fn new(scroll_x: f64, scroll_y: f64, zoom: f64, scale: f64) -> Self {
        SceneViewport {
            scroll_x,
            scroll_y,
            zoom,
            scale,
        }
    }

    /// `snapScrollToDevicePixels(appState, scale)` (`helpers.ts:47-63`):
    /// the scroll rounded to whole device pixels.
    pub fn snapped(&self) -> SceneViewport {
        let (scroll_x, scroll_y) =
            snap_scroll_to_device_pixels(self.scroll_x, self.scroll_y, self.zoom, self.scale);
        SceneViewport {
            scroll_x,
            scroll_y,
            ..*self
        }
    }

    /// Whether the scroll is already on whole device pixels (upstream
    /// returns the app state itself).
    pub fn is_snapped(&self) -> bool {
        let s = self.snapped();
        s.scroll_x == self.scroll_x && s.scroll_y == self.scroll_y
    }

    /// The matrix a layer draws the scene under after its bootstrap: the
    /// device pixel ratio, then the zoom (`context.scale(zoom, zoom)`).
    /// Scene coordinates go through `translate(x + scrollX, y + scrollY)`
    /// beneath it.
    pub fn transform(&self) -> Transform {
        Transform::scale(self.scale, self.scale).concat(&Transform::scale(self.zoom, self.zoom))
    }
}

/// The 2D context a layer paints: the display list backend's calls and
/// `clearRect`.
pub trait LayerContext: Context2d {
    /// `clearRect(x, y, width, height)` under the current matrix.
    fn clear_rect(&mut self, rect: &Rect);
}

/// `/^#([0-9a-f]{3}|[0-9a-f]{6})$/i` (`helpers.ts:101`): an opaque hex
/// colour, whose fill repaints every pixel.
pub fn is_opaque_hex_color(color: &str) -> bool {
    color
        .strip_prefix('#')
        .is_some_and(|hex| matches!(hex.len(), 3 | 6) && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// `bootstrapCanvas` (`helpers.ts:73-127`) up to its background fill:
/// `setTransform(1, 0, 0, 1, 0, 0)` and `scale(scale, scale)`, then
/// `clearRect` over the normalized size unless `view_background_color` is
/// an opaque hex colour. The fill itself (the colour through the dark
/// filter, white when the canvas rejects it, none for `transparent`) is the
/// static scene's first draw (`excali_scene::static_scene`); the
/// new-element and interactive canvases pass no colour and are cleared.
pub fn bootstrap_canvas<C: LayerContext>(
    ctx: &mut C,
    scale: f64,
    normalized: (f64, f64),
    view_background_color: Option<&str>,
) {
    ctx.set_transform(&Transform::scale(scale, scale));
    if !view_background_color.is_some_and(is_opaque_hex_color) {
        ctx.clear_rect(&Rect::new(0.0, 0.0, normalized.0, normalized.1));
    }
}

/// The static canvas's frame (`renderStaticScene`, `staticScene.ts:274-309`):
/// the bootstrap for `view_background_color`, then `list`, the scene from
/// `excali_scene::static_scene::render_static_scene` (background, grid,
/// elements, under the ratio and the zoom, at the snapped scroll).
pub fn paint_static_layer<C: LayerContext>(
    ctx: &mut C,
    size: BackingSize,
    scale: f64,
    view_background_color: Option<&str>,
    list: &DisplayList,
) {
    bootstrap_canvas(
        ctx,
        scale,
        normalized_canvas_dimensions(size, scale),
        view_background_color,
    );
    paint(list, ctx);
}

/// The new-element canvas's frame (`renderNewElementScene.ts:30-91`): the
/// bootstrap (a clear), then `drawing`, the list of
/// `excali_scene::new_element_scene::render_new_element_scene`; when that
/// is `None` (no element, or a selection box) the canvas is cleared again
/// under the zoom, as upstream's `else` branch does.
pub fn paint_new_element_layer<C: LayerContext>(
    ctx: &mut C,
    size: BackingSize,
    scale: f64,
    zoom: f64,
    drawing: Option<&DisplayList>,
) {
    let normalized = normalized_canvas_dimensions(size, scale);
    bootstrap_canvas(ctx, scale, normalized, None);
    match drawing {
        Some(list) => paint(list, ctx),
        None => {
            ctx.save();
            ctx.set_transform(&SceneViewport::new(0.0, 0.0, zoom, scale).transform());
            ctx.clear_rect(&Rect::new(0.0, 0.0, normalized.0, normalized.1));
            ctx.restore();
        }
    }
}

/// The interactive canvas's frame (`_renderInteractiveScene`,
/// `interactiveScene.ts:1614-1650`): the bootstrap (a clear), then `list`,
/// drawn under [`SceneViewport::transform`] at the snapped scroll.
pub fn paint_interactive_layer<C: LayerContext>(
    ctx: &mut C,
    size: BackingSize,
    scale: f64,
    list: &DisplayList,
) {
    bootstrap_canvas(ctx, scale, normalized_canvas_dimensions(size, scale), None);
    paint(list, ctx);
}

/// The canvases' rules of upstream's stylesheet (`css/styles.scss:5-6`,
/// `:105-132`), under the editor's `.excalidraw` container: the stacking
/// order of the layers (the interactive canvas above the others), no touch
/// gestures, pixelated scaling, the static canvas and its wrapper
/// transparent to the pointer, and every canvas out of the document flow.
pub const CANVAS_LAYER_CSS: &str = "\
.excalidraw {
  --zIndex-canvas: 1;
  --zIndex-interactiveCanvas: 2;
}
.excalidraw canvas {
  touch-action: none;
  image-rendering: pixelated;
  image-rendering: -moz-crisp-edges;
  z-index: var(--zIndex-canvas);
}
.excalidraw canvas.interactive {
  z-index: var(--zIndex-interactiveCanvas);
}
.excalidraw .excalidraw__canvas-wrapper,
.excalidraw .excalidraw__canvas.static {
  pointer-events: none;
}
.excalidraw .excalidraw__canvas {
  position: absolute;
}
";
