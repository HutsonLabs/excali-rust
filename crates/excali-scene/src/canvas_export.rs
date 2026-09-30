//! What a PNG export computes from the elements, as a [`CanvasDocument`]
//! the raster backend paints and encodes (`site/content/research/
//! rendering.md` section 6):
//!
//! - `exportToCanvas` (`packages/excalidraw/scene/export.ts:180-285`,
//!   [`export_to_canvas`]): the frame rendering config (exporting a frame
//!   also turns clipping off), the elements to draw (the frame's
//!   overlapping elements when exporting a frame, else the elements with
//!   each frame's name label before it), the canvas size (`getCanvasSize`,
//!   `:566-576`: the common bounds of the root elements, or of the exported
//!   frame, plus `exportPadding` on every side, `DEFAULT_EXPORT_PADDING`
//!   = 10 by default and 0 for a frame), the canvas itself (`createCanvas`,
//!   [`CanvasSizing`]: by default that size times `appState.exportScale`,
//!   drawn at that scale), the image cache of the files, and
//!   `renderStaticScene` with the scroll at `-min + padding`, zoom 1, no
//!   grid, `isExporting`, the export theme and the background colour only
//!   with `exportBackground`;
//! - the utils wrapper's `createCanvas` (`packages/utils/src/export.ts:
//!   64-105`): `maxWidthOrHeight` or `getDimensions`;
//! - the scene a PNG embeds ([`png_payload`], [`export_canvas_png`]): the
//!   editor's `exportCanvas` (`data/index.ts:98-186`) serializes the
//!   exported elements with the app state (`serializeAsJSON(..., "local")`)
//!   when `exportEmbedScene` is set, and `encodePngMetadata`
//!   (`data/image.ts:25-47`) stores it compressed in a `tEXt` chunk keyed
//!   `application/vnd.excalidraw+json`.
//!
//! The canvas's `width` and `height` are what the attributes hold after
//! `canvas.width = value` ([`canvas_width`], [`canvas_height`]): the value
//! as a WebIDL `unsigned long` (truncated, modulo 2^32), and the default
//! (300 wide, 150 high) when that is past 2^31 - 1.
//!
//! Loading images is the caller's (upstream's `updateImageCache` loads
//! each file's data URL): [`CanvasExportOptions::image_loads`] says which
//! files load. A file that does not load, a binary file and a missing file
//! draw upstream's placeholder. Frame clipping inside the static scene is
//! ex-403's; exporting a frame draws without it, as upstream does.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use excali_core::constants::{DEFAULT_GRID_SIZE, DEFAULT_GRID_STEP, MIME_TYPE_EXCALIDRAW};
use excali_core::element::{Element, ElementBase, ElementKind, TextFields};
use excali_core::encode::encode;
use excali_math::js;
use excali_text::text_measurements::TextMetricsProvider;

use crate::bounds::{do_bounds_intersect, get_element_bounds, ElementsMap};
use crate::display::{CanvasDocument, PngPayload};
use crate::export::{
    canvas_size, frame_labels, get_frame_rendering_config, get_root_elements_of, serialize_as_json,
    truthy, FrameLabel, FrameRendering, TextMetrics, DEFAULT_EXPORT_PADDING,
};
use crate::shape::Theme;
use crate::static_scene::{
    render_static_scene, CachedImage, StaticCanvasAppState, StaticCanvasRenderConfig, StaticScene,
};
use crate::sticky_note::Clock;

/// `MIME_TYPES.binary`: a file that is not an image, which
/// `updateImageCache` refuses.
const MIME_TYPE_BINARY: &str = "application/octet-stream";

/// A canvas: its size in device pixels and the scale the scene is drawn at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// What a utils caller's `getDimensions(width, height)` returns: the
/// canvas size and, optionally, the scale (1 when absent).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dimensions {
    pub width: f64,
    pub height: f64,
    pub scale: Option<f64>,
}

/// `createCanvas(width, height)`: how the canvas is made for content of a
/// size (padding included).
pub enum CanvasSizing<'a> {
    /// `exportToCanvas`'s default (`export.ts:197-205`): the size times
    /// `appState.exportScale`, drawn at `exportScale`. Without an
    /// `exportScale` (not a number) the canvas is 0 × 0, drawn at 1.
    ExportScale,
    /// The utils wrapper's (`utils/src/export.ts:64-105`):
    ///
    /// - with a `maxWidthOrHeight` (truthy: not 0 or NaN), the scale that
    ///   brings the larger side down to it when the content is larger, else
    ///   the caller's `exportScale` (1 when it passed none), and the canvas
    ///   the size times that scale; `getDimensions` is ignored;
    /// - else the canvas `getDimensions` gives and its scale (1 when it
    ///   gives none);
    /// - else the content size, drawn at 1 whatever `exportScale` is.
    Utils {
        max_width_or_height: Option<f64>,
        /// `appState?.exportScale` of what the caller passed (not
        /// restored).
        export_scale: Option<f64>,
        get_dimensions: Option<&'a dyn Fn(f64, f64) -> Dimensions>,
    },
}

/// `ToUint32` (WebIDL `unsigned long` conversion without `[EnforceRange]`):
/// NaN and the infinities are 0, anything else truncated modulo 2^32.
fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    // exact: the truncated value modulo 2^32 is an integer below 2^32
    value.trunc().rem_euclid(4_294_967_296.0) as u32
}

/// A reflected `unsigned long` attribute set to `value` (HTML "reflecting
/// content attributes in IDL attributes"): the converted value when it is
/// at most 2^31 - 1, else the default.
fn reflect_unsigned_long(value: f64, default: u32) -> u32 {
    let n = to_uint32(value);
    if n > 2_147_483_647 {
        default
    } else {
        n
    }
}

/// `canvas.width` after `canvas.width = value`: 300 by default.
pub fn canvas_width(value: f64) -> u32 {
    reflect_unsigned_long(value, 300)
}

/// `canvas.height` after `canvas.height = value`: 150 by default.
pub fn canvas_height(value: f64) -> u32 {
    reflect_unsigned_long(value, 150)
}

/// `value` when it is a JSON number.
fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64)
}

impl CanvasSizing<'_> {
    /// The canvas for content `width` × `height` (CSS pixels, padding
    /// included); `app_state` is `exportToCanvas`'s, whose `exportScale`
    /// [`CanvasSizing::ExportScale`] reads.
    pub fn create_canvas(&self, width: f64, height: f64, app_state: &Map<String, Value>) -> Canvas {
        match self {
            CanvasSizing::ExportScale => {
                // width * undefined is NaN; `scale = 1` fills in undefined
                let export_scale = number(app_state.get("exportScale"));
                let factor = export_scale.unwrap_or(f64::NAN);
                Canvas {
                    width: canvas_width(width * factor),
                    height: canvas_height(height * factor),
                    scale: export_scale.unwrap_or(1.0),
                }
            }
            CanvasSizing::Utils {
                max_width_or_height,
                export_scale,
                get_dimensions,
            } => {
                if let Some(max_side) = max_width_or_height.filter(|m| *m != 0.0 && !m.is_nan()) {
                    let max = js::max(width, height);
                    // if content is less then maxWidthOrHeight, fallback on
                    // supplied scale
                    let scale = if max_side < max {
                        max_side / max
                    } else {
                        export_scale.unwrap_or(1.0)
                    };
                    return Canvas {
                        width: canvas_width(width * scale),
                        height: canvas_height(height * scale),
                        scale,
                    };
                }
                let ret = get_dimensions.map_or(
                    Dimensions {
                        width,
                        height,
                        scale: None,
                    },
                    |f| f(width, height),
                );
                Canvas {
                    width: canvas_width(ret.width),
                    height: canvas_height(ret.height),
                    scale: ret.scale.unwrap_or(1.0),
                }
            }
        }
    }
}

/// `exportToCanvas`'s options object and what upstream finds elsewhere:
/// the sizing (its `createCanvas` argument), text measurement (the canvas's
/// `measureText`) and image loading (`updateImageCache`).
pub struct CanvasExportOptions<'a> {
    /// Draw the background colour.
    pub export_background: bool,
    /// `exportPadding`; `None` is [`DEFAULT_EXPORT_PADDING`]. Ignored when
    /// exporting a frame (0).
    pub export_padding: Option<f64>,
    /// The background colour, and the fill of outline arrowheads.
    pub view_background_color: String,
    /// Export this frame alone: its overlapping elements, cropped to it.
    pub exporting_frame: Option<&'a Element>,
    pub sizing: CanvasSizing<'a>,
    /// Measures frame names and embeddable labels.
    pub text_metrics: &'a dyn TextMetricsProvider,
    /// Whether a file's data URL loads as an image, by file id
    /// (`loadHTMLImageElement` resolving); only files present in `files`
    /// and not binary are asked.
    pub image_loads: &'a dyn Fn(&str) -> bool,
    /// `Date.now()` and the viewer's time zone, for sticky note footers.
    pub clock: Clock,
}

/// [`TextMetricsProvider`] as the frame labels measure.
struct ProviderMetrics<'a>(&'a dyn TextMetricsProvider);

impl TextMetrics for ProviderMetrics<'_> {
    fn measure(&self, text: &str, font: &str) -> f64 {
        self.0.get_line_width(text, font)
    }
}

/// `appState.frameRendering ?? null`: `None` when it is not an object with
/// truthy fields to read (a falsy value takes the default).
fn frame_rendering_of(app_state: &Map<String, Value>) -> Option<FrameRendering> {
    app_state
        .get("frameRendering")
        .filter(|v| truthy(Some(v)))
        .map(|v| {
            let flag = |key: &str| truthy(v.as_object().and_then(|o| o.get(key)));
            FrameRendering {
                enabled: flag("enabled"),
                clip: flag("clip"),
                name: flag("name"),
                outline: flag("outline"),
            }
        })
}

/// The keys of a `{ id: true }` map whose values are truthy.
fn id_set(value: Option<&Value>) -> HashSet<String> {
    value
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter(|(_, v)| truthy(Some(v)))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// A frame's name label as the text element
/// `addFrameLabelsAsTextElements` adds (`export.ts:98-139`): a
/// `newTextElement` at the label's box, Helvetica, left and top aligned,
/// in the name colour, named `id` (upstream's is random).
pub(crate) fn label_element(label: &FrameLabel, id: String) -> Element {
    let mut base = ElementBase::new(id, label.x, label.y, 0.0, 0.0);
    base.width = label.width;
    base.height = label.height;
    base.stroke_color = label.stroke_color.clone();
    let mut fields = TextFields::new(label.text.clone(), label.font_family, label.line_height);
    fields.font_size = label.font_size;
    Element::new(base, ElementKind::Text(fields))
}

/// `getElementsOverlappingFrame(elements, frame, elementsMap)`
/// (`frame.ts:983-998`): the elements in no frame or in this one whose
/// bounds intersect the frame's.
pub fn get_elements_overlapping_frame<'a>(
    elements: &'a [Element],
    frame: &Element,
    elements_map: &ElementsMap<'_>,
) -> Vec<&'a Element> {
    let frame_bounds = get_element_bounds(frame, elements_map);
    elements
        .iter()
        .filter(|el| {
            // exclude elements which are overlapping, but are in a
            // different frame, and thus invisible in target frame
            el.base
                .frame_id
                .as_deref()
                .is_none_or(|id| id.is_empty() || id == frame.base.id)
                && do_bounds_intersect(get_element_bounds(el, elements_map), frame_bounds)
        })
        .collect()
}

/// `exportToCanvas(elements, appState, files, opts, createCanvas)`
/// (`export.ts:180-285`): the canvas and what is drawn on it. `elements`
/// are the non-deleted elements to export; `app_state` the editor's
/// (`exportScale`, `exportWithDarkMode`, `frameRendering` and what the
/// static scene reads of it); `files` the `BinaryFiles` object.
pub fn export_to_canvas(
    elements: &[Element],
    app_state: &Map<String, Value>,
    files: &Map<String, Value>,
    opts: &CanvasExportOptions<'_>,
) -> CanvasDocument {
    let exporting_frame = opts.exporting_frame;
    let mut frame_rendering =
        get_frame_rendering_config(exporting_frame, frame_rendering_of(app_state));
    // for canvas export, don't clip if exporting a specific frame as it
    // would clip the corners of the content
    if exporting_frame.is_some() {
        frame_rendering.clip = false;
    }
    let export_with_dark_mode = truthy(app_state.get("exportWithDarkMode"));

    // prepareElementsForRender (export.ts:156-178)
    let all_map = ElementsMap::new(elements);
    let labels: Vec<Element>;
    let elements_for_render: Vec<&Element> = match exporting_frame {
        Some(frame) => get_elements_overlapping_frame(elements, frame, &all_map),
        None if frame_rendering.enabled && frame_rendering.name => {
            let metrics = ProviderMetrics(opts.text_metrics);
            labels = frame_labels(elements, export_with_dark_mode, &metrics)
                .iter()
                .map(|label| label_element(label, format!("{}:frame-label", label.frame_id)))
                .collect();
            let mut labels_left = labels.iter();
            let mut out = Vec::with_capacity(elements.len() + labels.len());
            for element in elements {
                if matches!(
                    element.kind,
                    ElementKind::Frame(_) | ElementKind::MagicFrame(_)
                ) {
                    out.extend(labels_left.next());
                }
                out.push(element);
            }
            out
        }
        None => elements.iter().collect(),
    };

    let padding = if exporting_frame.is_some() {
        0.0
    } else {
        opts.export_padding.unwrap_or(DEFAULT_EXPORT_PADDING)
    };
    let [min_x, min_y, width, height] = match exporting_frame {
        Some(frame) => canvas_size(&[frame], &[], padding),
        // getRootElements of what is rendered: the labels are root text
        // elements
        None => canvas_size(&get_root_elements_of(&elements_for_render), &[], padding),
    };

    let canvas = opts.sizing.create_canvas(width, height, app_state);

    // updateImageCache over the initialized image elements
    let mut image_cache = HashMap::new();
    for element in &elements_for_render {
        let ElementKind::Image(image) = &element.kind else {
            continue;
        };
        let Some(file_id) = image.file_id.as_ref().map(|f| f.0.as_str()) else {
            continue;
        };
        if file_id.is_empty() || image_cache.contains_key(file_id) {
            continue;
        }
        let Some(file) = files.get(file_id).filter(|f| truthy(Some(f))) else {
            continue;
        };
        let mime_type = file.get("mimeType").and_then(Value::as_str).unwrap_or("");
        if mime_type == MIME_TYPE_BINARY || !(opts.image_loads)(file_id) {
            continue;
        }
        image_cache.insert(
            file_id.to_owned(),
            CachedImage {
                mime_type: mime_type.to_owned(),
            },
        );
    }

    let theme = if export_with_dark_mode {
        Theme::Dark
    } else {
        Theme::Light
    };
    let static_state = StaticCanvasAppState {
        zoom: 1.0,
        scroll_x: -min_x + padding,
        scroll_y: -min_y + padding,
        theme,
        view_background_color: opts
            .export_background
            .then(|| opts.view_background_color.clone()),
        grid_size: number(app_state.get("gridSize")).unwrap_or(DEFAULT_GRID_SIZE),
        grid_step: number(app_state.get("gridStep")).unwrap_or(DEFAULT_GRID_STEP),
        frame_rendering,
        selected_element_ids: id_set(app_state.get("selectedElementIds")),
        hovered_element_ids: id_set(app_state.get("hoveredElementIds")),
        open_dialog: app_state
            .get("openDialog")
            .and_then(|d| d.get("name"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        // the rest of `...appState`, which getTargetFrame reads
        frame_to_highlight: app_state
            .get("frameToHighlight")
            .and_then(Value::as_object)
            .and_then(|frame| Element::from_map(frame.clone()).ok()),
        selected_elements_are_being_dragged: truthy(
            app_state.get("selectedElementsAreBeingDragged"),
        ),
        editing_group_id: app_state
            .get("editingGroupId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        // exporting: no zoom gesture
        should_cache_ignore_zoom: false,
    };
    let render_config = StaticCanvasRenderConfig {
        canvas_background_color: opts.view_background_color.clone(),
        image_cache,
        render_grid: false,
        is_exporting: true,
        // empty disables embeddable rendering
        embeds_validation_status: HashMap::new(),
        theme,
        clock: opts.clock,
        ..StaticCanvasRenderConfig::default()
    };
    let render_map = ElementsMap::new(elements_for_render.iter().copied());
    let list = render_static_scene(&StaticScene {
        canvas_width: f64::from(canvas.width),
        canvas_height: f64::from(canvas.height),
        scale: canvas.scale,
        elements_map: &render_map,
        all_elements_map: &all_map,
        visible_elements: &elements_for_render,
        app_state: &static_state,
        render_config: &render_config,
        text_metrics: opts.text_metrics,
    });

    CanvasDocument {
        width: canvas.width,
        height: canvas.height,
        list,
        payload: None,
    }
}

/// The scene a PNG embeds: `encodePngMetadata`'s `tEXt` chunk
/// (`data/image.ts:25-47`) of `serializeAsJSON(elements, appState, files,
/// "local")` (`data/json.ts:52-75`) compressed in the payload wrapper.
/// `source` is `getExportSource()`.
pub fn png_payload(
    elements: &[Element],
    app_state: &Map<String, Value>,
    files: Option<&Map<String, Value>>,
    source: &str,
) -> PngPayload {
    let json = serialize_as_json(elements, app_state, files, source);
    PngPayload {
        keyword: MIME_TYPE_EXCALIDRAW.to_owned(),
        text: encode(&json, true).to_json(),
    }
}

/// Why the editor's PNG export stops before drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportCanvasError {
    /// No elements: `alerts.cannotExportEmptyCanvas`.
    EmptyCanvas,
}

impl std::fmt::Display for ExportCanvasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportCanvasError::EmptyCanvas => f.write_str("Cannot export empty canvas."),
        }
    }
}

impl std::error::Error for ExportCanvasError {}

/// The editor's `exportCanvas("png", elements, appState, files, opts)`
/// (`data/index.ts:98-192`) up to the blob: no elements is an error;
/// otherwise [`export_to_canvas`], with the scene embedded
/// ([`png_payload`] of the exported elements, the app state and the files)
/// when `appState.exportEmbedScene` is set.
pub fn export_canvas_png(
    elements: &[Element],
    app_state: &Map<String, Value>,
    files: &Map<String, Value>,
    opts: &CanvasExportOptions<'_>,
    source: &str,
) -> Result<CanvasDocument, ExportCanvasError> {
    if elements.is_empty() {
        return Err(ExportCanvasError::EmptyCanvas);
    }
    let mut doc = export_to_canvas(elements, app_state, files, opts);
    if truthy(app_state.get("exportEmbedScene")) {
        doc.payload = Some(png_payload(elements, app_state, Some(files), source));
    }
    Ok(doc)
}
