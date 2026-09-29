//! What `exportToSvg` (`packages/excalidraw/scene/export.ts:293-508`)
//! computes from the elements before it builds the document, as a
//! [`SvgDocument`] the SVG backend writes (`site/content/research/
//! rendering.md` section 6):
//!
//! - the frame rendering config ([`get_frame_rendering_config`],
//!   `:141-154`): exporting a frame forces clipping on and outlines and
//!   names off;
//! - the frame name labels ([`frame_labels`], `addFrameLabelsAsTextElements`
//!   and `truncateText`, `:64-139`), which render as text above each frame
//!   and so take part in the size;
//! - the canvas: the common bounds of the root elements and labels (or of
//!   the frame being exported) with the padding around them
//!   ([`canvas_size`], `getCanvasSize`, `:566-576`), 10 by default and 0
//!   when exporting a frame;
//! - the embedded scene (`serializeAsJSON(elements, appState, files,
//!   "local")` through `encodeSvgBase64Payload`), the clip path of every
//!   frame (rounded by `FRAME_STYLE.radius` unless it is the frame being
//!   exported), the font faces to inline (`generateFontFaceDeclarations`)
//!   and the background colour through the dark-mode filter;
//! - the drawing: `renderSceneToSvg` over the elements to render (the
//!   frame's overlapping elements when exporting a frame, else the elements
//!   with each frame's name label before it), `crate::svg_scene`.
//!
//! Text is measured by the caller's [`TextMetrics`], upstream's canvas
//! `measureText`.

use serde_json::{Map, Value};

use excali_core::app_state::clean_app_state_for_export;
use excali_core::color::apply_dark_mode_filter;
use excali_core::document::{filter_out_deleted_files, Document};
use excali_core::element::{Element, ElementKind, FontFamily};
use excali_core::svg_payload::{svg_base64_payload, PAYLOAD_MIME_TYPE, SVG_PAYLOAD_VERSION};
use excali_math::js;
use excali_text::font_assets::font_face_declarations;
use excali_text::font_metadata::{get_font_string, get_line_height_in_px};

use excali_text::text_measurements::TextMetricsProvider;

use crate::bounds::{get_common_bounds, get_element_absolute_coords, ElementsMap};
use crate::canvas_export::{get_elements_overlapping_frame, label_element};
use crate::display::{FontFaceSource, FrameClip, SvgDocument, SvgPayload};
use crate::frame::is_frame_like;
use crate::shape::Theme;
use crate::svg_scene::{label_id, render_scene_to_svg, SvgRenderConfig};

/// `DEFAULT_EXPORT_PADDING` (`common/src/constants.ts:402`), in pixels.
pub const DEFAULT_EXPORT_PADDING: f64 = 10.0;

/// `FRAME_STYLE` (`common/src/constants.ts:206-220`): what export reads of
/// it.
pub mod frame_style {
    /// `strokeColor`: the outline's colour (`renderElement.ts:1033-1036`).
    pub const STROKE_COLOR: &str = "#bbb";
    /// `strokeWidth`: the outline's width at zoom 1.
    pub const STROKE_WIDTH: f64 = 2.0;
    /// `radius`: the clip path's and the outline's corner radius.
    pub const RADIUS: f64 = 8.0;
    /// `nameOffsetY`: the gap between a frame's name and its top edge.
    pub const NAME_OFFSET_Y: f64 = 3.0;
    /// `nameColorLightTheme`.
    pub const NAME_COLOR_LIGHT_THEME: &str = "#999999";
    /// `nameColorDarkTheme`.
    pub const NAME_COLOR_DARK_THEME: &str = "#7a7a7a";
    /// `nameFontSize`.
    pub const NAME_FONT_SIZE: f64 = 14.0;
    /// `nameLineHeight`.
    pub const NAME_LINE_HEIGHT: f64 = 1.25;
}

/// `DEFAULT_FRAME_NAME` and `DEFAULT_AI_FRAME_NAME` (`frame.ts:974-977`).
const DEFAULT_FRAME_NAME: &str = "Frame";
const DEFAULT_AI_FRAME_NAME: &str = "AI Frame";

/// `AppState["frameRendering"]`: whether frames draw at all, clip their
/// children, show their outline and show their name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRendering {
    pub enabled: bool,
    pub clip: bool,
    pub name: bool,
    pub outline: bool,
}

impl Default for FrameRendering {
    /// `getDefaultAppState().frameRendering` (`appState.ts:112`): all on.
    fn default() -> Self {
        FrameRendering {
            enabled: true,
            clip: true,
            name: true,
            outline: true,
        }
    }
}

/// `getFrameRenderingConfig(exportingFrame, frameRendering)`
/// (`export.ts:141-154`): the app state's config (the default when there
/// is none), except that exporting a frame renders frames, clips, and
/// shows neither outline nor name.
pub fn get_frame_rendering_config(
    exporting_frame: Option<&Element>,
    frame_rendering: Option<FrameRendering>,
) -> FrameRendering {
    let f = frame_rendering.unwrap_or_default();
    let exporting = exporting_frame.is_some();
    FrameRendering {
        enabled: exporting || f.enabled,
        outline: !exporting && f.outline,
        name: !exporting && f.name,
        clip: exporting || f.clip,
    }
}

/// How wide a text is in a font: the canvas `measureText(text).width`
/// upstream measures with (`CanvasTextMetricsProvider`,
/// `textMeasurements.ts:121-150`, and `truncateText`). `font` is a CSS font
/// string (`getFontString`), `text` may hold line breaks.
pub trait TextMetrics {
    fn measure(&self, text: &str, font: &str) -> f64;
}

/// [`TextMetrics`] as the text layout measures a line.
struct LineMetrics<'a>(&'a dyn TextMetrics);

impl TextMetricsProvider for LineMetrics<'_> {
    fn get_line_width(&self, text: &str, font: &str) -> f64 {
        self.0.measure(text, font)
    }
}

/// A frame's name as the text element export draws above it
/// (`addFrameLabelsAsTextElements`, `export.ts:98-139`): Helvetica at the
/// frame name size, left and top aligned, its bottom `NAME_OFFSET_Y` above
/// the frame, no wider than the frame.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameLabel {
    /// The frame the label names.
    pub frame_id: String,
    /// The name, cut to fit with `...` when it is wider than the frame.
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
    pub font_family: FontFamily,
    pub line_height: f64,
    pub stroke_color: String,
}

/// `getFrameLikeTitle(element)` (`frame.ts:979-981`): the frame's name, or
/// "Frame" / "AI Frame" when it has none.
pub fn get_frame_like_title(element: &Element) -> Option<&str> {
    match &element.kind {
        ElementKind::Frame(frame) => Some(frame.name.as_deref().unwrap_or(DEFAULT_FRAME_NAME)),
        ElementKind::MagicFrame(frame) => {
            Some(frame.name.as_deref().unwrap_or(DEFAULT_AI_FRAME_NAME))
        }
        _ => None,
    }
}

/// `normalizeText(text)` (`textMeasurements.ts:64-70`): line breaks as
/// `\n`, tabs as eight spaces.
fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "        ")
}

/// The labels `prepareElementsForRender` adds for the frames among
/// `elements`, in element order (`addFrameLabelsAsTextElements`,
/// `export.ts:98-139`): `newTextElement` measures the name
/// (`measureText`: the widest line, every line `fontSize * lineHeight`
/// high, an empty line measured as a space), the label sits its height
/// above `y - NAME_OFFSET_Y`, and `truncateText` (`:64-96`) cuts a name
/// wider than the frame to the longest prefix (in UTF-16 code units) that
/// fits with `...` and gives the label the frame's width.
///
/// A prefix that ends inside a surrogate pair is measured and kept with
/// U+FFFD in place of the lone half.
pub fn frame_labels(
    elements: &[Element],
    export_with_dark_mode: bool,
    metrics: &dyn TextMetrics,
) -> Vec<FrameLabel> {
    let font_family = FontFamily::HELVETICA;
    let font_size = frame_style::NAME_FONT_SIZE;
    let line_height = frame_style::NAME_LINE_HEIGHT;
    let font = get_font_string(font_size, font_family);
    let stroke_color = if export_with_dark_mode {
        frame_style::NAME_COLOR_DARK_THEME
    } else {
        frame_style::NAME_COLOR_LIGHT_THEME
    };
    let mut labels = Vec::new();
    for element in elements {
        let Some(title) = get_frame_like_title(element) else {
            continue;
        };
        let text = normalize_text(title);

        // measureText: empty lines measure as a space
        let measured = text
            .split('\n')
            .map(|line| if line.is_empty() { " " } else { line })
            .collect::<Vec<_>>()
            .join("\n");
        let lines: Vec<&str> = measured.split('\n').collect();
        let height = get_line_height_in_px(font_size, line_height) * lines.len() as f64;
        let mut width: f64 = 0.0;
        for line in &lines {
            width = js::max(width, metrics.measure(line, &font));
        }

        let b = &element.base;
        let x = b.x;
        let y = b.y - frame_style::NAME_OFFSET_Y - height;
        let (text, width) = truncate_text(text, width, b.width, &font, metrics);
        labels.push(FrameLabel {
            frame_id: b.id.clone(),
            text,
            x,
            y,
            width,
            height,
            font_size,
            font_family,
            line_height,
            stroke_color: stroke_color.to_owned(),
        });
    }
    labels
}

/// `truncateText(element, maxWidth)` (`export.ts:64-96`).
fn truncate_text(
    text: String,
    width: f64,
    max_width: f64,
    font: &str,
    metrics: &dyn TextMetrics,
) -> (String, f64) {
    if width <= max_width {
        return (text, width);
    }
    let mut text = text;
    if metrics.measure(&text, font) > max_width {
        let units: Vec<u16> = text.encode_utf16().collect();
        for i in (1..=units.len()).rev() {
            let candidate = format!("{}...", String::from_utf16_lossy(&units[..i]));
            if metrics.measure(&candidate, font) <= max_width {
                text = candidate;
                break;
            }
        }
    }
    (text, max_width)
}

/// `getFrameLikeElements(elements)` (`frame.ts:255-261`).
pub fn get_frame_like_elements(elements: &[Element]) -> Vec<&Element> {
    elements.iter().filter(|e| is_frame_like(e)).collect()
}

/// `getRootElements(elements)` (`frame.ts:271-281`): the frames and every
/// element not inside one of the frames among `elements` (no `frameId`, or
/// one naming a frame that is not there).
pub fn get_root_elements(elements: &[Element]) -> Vec<&Element> {
    get_root_elements_of(&elements.iter().collect::<Vec<_>>())
}

/// [`get_root_elements`] of a list of references.
pub fn get_root_elements_of<'a>(elements: &[&'a Element]) -> Vec<&'a Element> {
    let frames: Vec<&str> = elements
        .iter()
        .filter(|e| is_frame_like(e))
        .map(|f| f.base.id.as_str())
        .collect();
    elements
        .iter()
        .copied()
        .filter(|e| {
            frames.contains(&e.base.id.as_str())
                || e.base
                    .frame_id
                    .as_deref()
                    .is_none_or(|id| id.is_empty() || !frames.contains(&id))
        })
        .collect()
}

/// `getCanvasSize(elements, exportPadding)` (`export.ts:566-576`) of the
/// root elements and the frame labels: `[minX, minY, width, height]`, the
/// top left corner of their common bounds and their size with `padding` on
/// every side. No elements and no labels give `[0, 0, 2 * padding, 2 *
/// padding]`.
pub fn canvas_size(elements: &[&Element], labels: &[FrameLabel], padding: f64) -> [f64; 4] {
    let [mut min_x, mut min_y, mut max_x, mut max_y] = if elements.is_empty() && !labels.is_empty()
    {
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ]
    } else {
        get_common_bounds(elements)
    };
    // A label is an unrotated text element with no container: its bounds
    // are its box.
    for label in labels {
        min_x = js::min(min_x, label.x);
        min_y = js::min(min_y, label.y);
        max_x = js::max(max_x, label.x + label.width);
        max_y = js::max(max_y, label.y + label.height);
    }
    // distance(a, b) = Math.abs(a - b)
    let width = (min_x - max_x).abs() + padding * 2.0;
    let height = (min_y - max_y).abs() + padding * 2.0;
    [min_x, min_y, width, height]
}

/// The `appState` argument of `exportToSvg` (`export.ts:293-304`). The
/// editor passes exactly these keys (`data/index.ts:123-136`); `rest` holds
/// the object as a caller passed it when it passes more (utils'
/// `exportToSvg` passes a whole `AppState`), for the embedded scene, whose
/// `appState` is `cleanAppStateForExport` of the object.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgExportAppState {
    /// Draw the background rectangle.
    pub export_background: bool,
    /// `exportPadding`; `None` is [`DEFAULT_EXPORT_PADDING`].
    pub export_padding: Option<f64>,
    /// `exportScale`; `None` is 1.
    pub export_scale: Option<f64>,
    pub view_background_color: String,
    pub export_with_dark_mode: bool,
    /// Embed the scene in `<metadata>`.
    pub export_embed_scene: bool,
    /// `None` is the default config.
    pub frame_rendering: Option<FrameRendering>,
    /// The app state object as passed, in its key order; the typed fields
    /// above replace the values of their keys.
    pub rest: Map<String, Value>,
}

/// `!!value` in JS for a JSON value (`false` when absent).
pub(crate) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

/// `Number(value)` for a JSON value, as the arithmetic reads it: `null` is
/// 0, a boolean 0 or 1, a string its numeric value (0 when blank, NaN when
/// not a number); arrays and objects are NaN.
fn to_number(value: &Value) -> f64 {
    match value {
        Value::Null => 0.0,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse().unwrap_or(f64::NAN)
            }
        }
        Value::Array(_) | Value::Object(_) => f64::NAN,
    }
}

impl SvgExportAppState {
    /// The editor's export state for a background colour: no background
    /// rectangle, default padding and scale, light, no embedded scene.
    pub fn new(view_background_color: impl Into<String>) -> SvgExportAppState {
        SvgExportAppState {
            export_background: false,
            export_padding: None,
            export_scale: None,
            view_background_color: view_background_color.into(),
            export_with_dark_mode: false,
            export_embed_scene: false,
            frame_rendering: None,
            rest: Map::new(),
        }
    }

    /// Read an app state object as `exportToSvg` reads it: the flags by
    /// truthiness; padding and scale absent for their defaults, otherwise as
    /// numbers (`null` is 0; a string reads as its number, where upstream
    /// would concatenate it into the padding); the background colour when
    /// it is a string; `frameRendering` when truthy, its flags by
    /// truthiness.
    pub fn from_app_state(app_state: &Map<String, Value>) -> SvgExportAppState {
        let frame_rendering = app_state
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
            });
        SvgExportAppState {
            export_background: truthy(app_state.get("exportBackground")),
            export_padding: app_state.get("exportPadding").map(to_number),
            export_scale: app_state.get("exportScale").map(to_number),
            view_background_color: app_state
                .get("viewBackgroundColor")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            export_with_dark_mode: truthy(app_state.get("exportWithDarkMode")),
            export_embed_scene: truthy(app_state.get("exportEmbedScene")),
            frame_rendering,
            rest: app_state.clone(),
        }
    }

    /// The typed fields as the object's values, in the order the editor
    /// builds the object (`data/index.ts:126-133`).
    fn typed(&self) -> [(&'static str, Option<Value>); 7] {
        let number = |n: Option<f64>| {
            n.map(|n| serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number))
        };
        [
            (
                "exportBackground",
                Some(Value::Bool(self.export_background)),
            ),
            (
                "exportWithDarkMode",
                Some(Value::Bool(self.export_with_dark_mode)),
            ),
            (
                "viewBackgroundColor",
                Some(Value::String(self.view_background_color.clone())),
            ),
            ("exportPadding", number(self.export_padding)),
            ("exportScale", number(self.export_scale)),
            (
                "exportEmbedScene",
                Some(Value::Bool(self.export_embed_scene)),
            ),
            (
                "frameRendering",
                self.frame_rendering.map(|f| {
                    serde_json::json!({
                        "enabled": f.enabled,
                        "clip": f.clip,
                        "name": f.name,
                        "outline": f.outline,
                    })
                }),
            ),
        ]
    }

    /// The object `exportToSvg` was given: [`Self::rest`] in its order with
    /// the typed values in place, then the typed keys it lacks.
    pub fn to_app_state(&self) -> Map<String, Value> {
        let typed = self.typed();
        let mut out = Map::new();
        for (key, value) in &self.rest {
            match typed.iter().find(|(k, _)| k == key) {
                Some((_, Some(v))) => {
                    out.insert(key.clone(), v.clone());
                }
                Some((_, None)) => {}
                None => {
                    out.insert(key.clone(), value.clone());
                }
            }
        }
        for (key, value) in typed {
            if let Some(value) = value {
                if !out.contains_key(key) {
                    out.insert(key.to_owned(), value);
                }
            }
        }
        out
    }
}

/// The rest of `exportToSvg`'s arguments.
pub struct SvgExportOptions<'a> {
    /// `getExportSource()`: the embedded scene's `source`
    /// (`window.EXCALIDRAW_EXPORT_SOURCE` or the page's origin upstream).
    pub source: &'a str,
    /// `opts.exportingFrame`: export this frame alone, cropped to it.
    pub exporting_frame: Option<&'a Element>,
    /// `opts.skipInliningFonts`.
    pub skip_inlining_fonts: bool,
    /// `opts.renderEmbeddables` (upstream's default is `false`): draw an
    /// embeddable's content as an `<iframe>` in a `<foreignObject>` rather
    /// than a link.
    pub render_embeddables: bool,
    /// `opts.reuseImages` (upstream's default is `true`): images of the
    /// same file (and crop) share one `<symbol>`.
    pub reuse_images: bool,
    /// `isTestEnv()`: every element's node carries its id as `data-id`,
    /// and the text elements export makes (frame names, embeddable
    /// placeholders) are named by `randomId`'s test sequence, `id0`, `id1`,
    /// ... Upstream's own snapshots are written this way.
    pub data_ids: bool,
    /// `window.location.origin`, which `toValidURL` puts before a link
    /// starting with `/` (an embeddable's `<iframe>` source).
    pub origin: &'a str,
    /// Measures the frame names and embeddable placeholders.
    pub text_metrics: &'a dyn TextMetrics,
}

impl<'a> SvgExportOptions<'a> {
    /// `exportToSvg`'s defaults: no frame, fonts inlined, embeddables as
    /// links, images reused, no `data-id`, the export source as the origin.
    pub fn new(source: &'a str, text_metrics: &'a dyn TextMetrics) -> SvgExportOptions<'a> {
        SvgExportOptions {
            source,
            exporting_frame: None,
            skip_inlining_fonts: false,
            render_embeddables: false,
            reuse_images: true,
            data_ids: false,
            origin: source,
            text_metrics,
        }
    }
}

/// `serializeAsJSON(elements, appState, files || {}, "local")`
/// (`data/json.ts:52-75`): the scene `exportEmbedScene` embeds.
pub fn serialize_as_json(
    elements: &[Element],
    app_state: &Map<String, Value>,
    files: Option<&Map<String, Value>>,
    source: &str,
) -> String {
    let files = Value::Object(files.cloned().unwrap_or_default());
    Document::new(
        source,
        elements.to_vec(),
        clean_app_state_for_export(app_state),
        Some(filter_out_deleted_files(elements, &files)),
    )
    .to_json()
}

/// The document `exportToSvg(elements, appState, files, opts)` builds
/// around the elements (`export.ts:293-470`), for the SVG backend to write.
pub fn svg_document(
    elements: &[Element],
    app_state: &SvgExportAppState,
    files: Option<&Map<String, Value>>,
    opts: &SvgExportOptions<'_>,
) -> SvgDocument {
    let exporting_frame = opts.exporting_frame;
    let frame_rendering = get_frame_rendering_config(exporting_frame, app_state.frame_rendering);
    let export_with_dark_mode = app_state.export_with_dark_mode;
    let labels = if exporting_frame.is_none() && frame_rendering.enabled && frame_rendering.name {
        frame_labels(elements, export_with_dark_mode, opts.text_metrics)
    } else {
        Vec::new()
    };

    let padding = if exporting_frame.is_some() {
        0.0
    } else {
        app_state.export_padding.unwrap_or(DEFAULT_EXPORT_PADDING)
    };
    let [min_x, min_y, width, height] = match exporting_frame {
        Some(frame) => canvas_size(&[frame], &[], padding),
        None => canvas_size(&get_root_elements(elements), &labels, padding),
    };
    let offset_x = -min_x + padding;
    let offset_y = -min_y + padding;

    let payload = app_state.export_embed_scene.then(|| {
        let json = serialize_as_json(elements, &app_state.to_app_state(), files, opts.source);
        SvgPayload {
            mime_type: PAYLOAD_MIME_TYPE.to_owned(),
            version: SVG_PAYLOAD_VERSION,
            base64: svg_base64_payload(&json),
        }
    });

    let elements_map = ElementsMap::new(elements);
    let frame_clips: Vec<FrameClip> = get_frame_like_elements(elements)
        .into_iter()
        .map(|frame| {
            let b = &frame.base;
            let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(frame, &elements_map, false);
            FrameClip {
                id: b.id.clone(),
                x: b.x + offset_x,
                y: b.y + offset_y,
                angle: b.angle.0,
                cx: (x2 - x1) / 2.0 - (b.x - x1),
                cy: (y2 - y1) / 2.0 - (b.y - y1),
                width: b.width,
                height: b.height,
                radius: exporting_frame.is_none().then_some(frame_style::RADIUS),
            }
        })
        .collect();

    let font_faces = if opts.skip_inlining_fonts {
        Vec::new()
    } else {
        font_face_declarations(elements)
            .into_iter()
            .map(|d| FontFaceSource {
                family: d.face.family.to_owned(),
                file: d.face.file.to_owned(),
                upstream_file: d.face.upstream_file.to_owned(),
                format: d.face.format.css(),
                characters: d.characters,
                fallback_url: d.face.fallback_url(),
            })
            .collect()
    };

    let background = (app_state.export_background && !app_state.view_background_color.is_empty())
        .then(|| apply_dark_mode_filter(&app_state.view_background_color, export_with_dark_mode));

    // prepareElementsForRender (export.ts:156-178): the labels are text
    // elements, each before its frame
    let label_elements: Vec<Element> = labels
        .iter()
        .enumerate()
        .map(|(n, label)| {
            let id = label_id(opts.data_ids, n, || {
                format!("{}:frame-label", label.frame_id)
            });
            label_element(label, id)
        })
        .collect();
    let elements_for_render: Vec<&Element> = match exporting_frame {
        Some(frame) => get_elements_overlapping_frame(elements, frame, &elements_map),
        None if !label_elements.is_empty() => {
            let mut labels_left = label_elements.iter();
            let mut out = Vec::with_capacity(elements.len() + label_elements.len());
            for element in elements {
                if is_frame_like(element) {
                    out.extend(labels_left.next());
                }
                out.push(element);
            }
            out
        }
        None => elements.iter().collect(),
    };
    let render_map = ElementsMap::new(elements_for_render.iter().copied());
    let empty_files = Map::new();
    let metrics = LineMetrics(opts.text_metrics);
    let drawing = render_scene_to_svg(
        &elements_for_render,
        &render_map,
        files.unwrap_or(&empty_files),
        frame_clips
            .iter()
            .map(|c: &FrameClip| c.id.clone())
            .collect(),
        label_elements.len(),
        &SvgRenderConfig {
            offset_x,
            offset_y,
            theme: if export_with_dark_mode {
                Theme::Dark
            } else {
                Theme::Light
            },
            frame_rendering,
            render_embeddables: opts.render_embeddables,
            reuse_images: opts.reuse_images,
            canvas_background_color: &app_state.view_background_color,
            data_ids: opts.data_ids,
            origin: opts.origin,
            text_metrics: &metrics,
        },
    );

    SvgDocument {
        width,
        height,
        scale: app_state.export_scale.unwrap_or(1.0),
        offset_x,
        offset_y,
        payload,
        frame_clips,
        font_faces,
        background,
        symbols: drawing.symbols,
        nodes: drawing.nodes,
    }
}
