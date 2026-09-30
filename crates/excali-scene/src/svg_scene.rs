//! `renderSceneToSvg` (`packages/excalidraw/renderer/staticSvgScene.ts:
//! 850-933`): the elements of an SVG export as the [`SvgNode`]s upstream
//! appends to the document `exportToSvg` builds (`site/content/research/
//! rendering.md` section 6).
//!
//! The elements are drawn in order, iframes and embeddables last, a
//! container's label right after it (a label whose container is exported
//! waits for it), deleted elements not at all. `renderElementToSvg`
//! (`:97-848`) draws each at `x + offsetX`, `y + offsetY`:
//!
//! - every element is placed by `translate(offsetX offsetY) rotate(degrees
//!   cx cy)`, its centre relative to its corner; an arrow's label is placed
//!   where the arrow puts it (`getBoundTextElementPosition`);
//! - an element with a link is drawn inside `<a href=…>` (the link through
//!   `normalizeLink`), which is appended before anything is drawn;
//! - opacity (the element's times its frame's) is written as
//!   `stroke-opacity` and `fill-opacity` when it is not 1 (an image's as
//!   the `<use>`'s `opacity`, always), and the nodes of a frame's child are
//!   wrapped in `<g clip-path="url(#frame)">` while frames are clipped
//!   (`maybeWrapNodesInFrameClipPath`, `:76-95`);
//! - rectangles, diamonds and ellipses: rough.js's `RoughSVG.draw` with
//!   two-decimal path data (`roughSVGDrawWithPrecision`, `:60-74`,
//!   [`rough_group`]), `stroke-linecap="round"`;
//! - iframes and embeddables: the placeholder shape, the placeholder label
//!   (`createPlaceholderEmbeddableLabel`), then an empty group holding a
//!   link to the embed, or with `renderEmbeddables` an `<iframe>` in a
//!   `<foreignObject>` (`getEmbedLink`), a link for embeds written as a
//!   document;
//! - lines and arrows: a group of the rough.js shapes, then a `<mask>`
//!   (empty unless the element has a label, whose padded box it cuts out,
//!   `:404-515`); a line that closes into a loop with a fill fills
//!   `evenodd`;
//! - freedraw: the loop's rough.js fill and the outline path filled with
//!   the stroke colour, in one group with `stroke="none"`;
//! - images: a `<symbol>` per file (per crop, or per element without
//!   `reuseImages`) put first in `<defs>` the first time it is used, and a
//!   `<use>` of it scaled for flips, masked for crops and clipped to
//!   rounded corners (`:577-738`); SVG files in dark mode through
//!   `DARK_THEME_FILTER`;
//! - frames: their outline, a `#bbb` rounded rectangle (`:740-774`);
//! - text: a group with one `<text>` per line (`:775-842`), `x` at 0, half
//!   or all of the width, `y` on the line's baseline, `text-anchor` start,
//!   middle or end (end too for right-to-left text), `white-space: pre`.
//!
//! What upstream throws on (a selection, a shape rough.js cannot draw, an
//! image symbol id that is not a valid CSS selector) stops that element,
//! and its label, where it is: what was appended before stays, as in
//! upstream's `try`/`catch` (`:876-908`).
//!
//! Sticky notes (`staticSvgScene.ts:158-270`): a `<clipPath>` of the
//! outline added to the root first, then a group holding the shadow, the
//! filled outline, the outline stroked through that clip and the date
//! footer `<text>` ([`crate::sticky_note`]), clipped to its frame.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::fmt;

use serde_json::{Map, Value};

use excali_core::color::apply_dark_mode_filter;
use excali_core::constants::{BOUND_TEXT_PADDING, DARK_THEME_FILTER};
use excali_core::element::{Element, ElementKind, TextAlign as ElementTextAlign};
use excali_core::embeddable::{get_embed_link, EmbedType};
use excali_core::json::number_to_string;
use excali_core::library::hash_string;
use excali_core::link::{normalize_link, to_valid_url};
use excali_math::js;
use excali_rough::{Drawable, Op, OpSetType, RoughGenerator, Shape};
use excali_text::font_metadata::{
    get_font_family_string, get_line_height_in_px, get_vertical_offset,
};
use excali_text::text_measurements::TextMetricsProvider;

use crate::bounds::{
    get_bound_text_element, get_container_element, get_element_absolute_coords, ElementsMap,
};
use crate::display::{Path, SvgNode, SvgTag, SvgValue};
use crate::export::{frame_style, FrameRendering};
use crate::linear_element::get_bound_text_element_position;
use crate::render_element::{create_placeholder_embeddable_label, get_containing_frame, is_rtl};
use crate::shape::{
    generate_element_shape, generate_freedraw_shapes, generate_linear_element_shapes,
    EmbedsValidationStatus, FreedrawShape, RenderConfig, ShapeError, Theme,
};
use crate::sticky_note::{
    get_sticky_note_footer, get_sticky_note_path_commands, Clock, StickyNotePathCommand,
    STICKY_NOTE_EDGE_SHADOW_OPACITY, STICKY_NOTE_EDGE_SHADOW_WIDTH, STICKY_NOTE_FOOTER_FONT_FAMILY,
    STICKY_NOTE_FOOTER_FONT_SIZE, STICKY_NOTE_FOOTER_OPACITY, STICKY_NOTE_SHADOW_OPACITY,
};
use crate::utils::{get_corner_radius, is_path_a_loop};

/// `MAX_DECIMALS_FOR_SVG_EXPORT` (`common/src/constants.ts:399`).
const MAX_DECIMALS_FOR_SVG_EXPORT: usize = 2;

/// `MIME_TYPES.svg`.
const MIME_TYPE_SVG: &str = "image/svg+xml";

/// `SVGRenderConfig` (`scene/types.ts:47-63`) as `exportToSvg` fills it
/// (`export.ts:477-503`), with what the port needs besides.
pub struct SvgRenderConfig<'a> {
    /// Where scene coordinates land in the document.
    pub offset_x: f64,
    pub offset_y: f64,
    pub theme: Theme,
    pub frame_rendering: FrameRendering,
    pub render_embeddables: bool,
    pub reuse_images: bool,
    /// `canvasBackgroundColor`: the fill of outline arrowheads.
    pub canvas_background_color: &'a str,
    /// `isTestEnv()`: `data-id` on every element's node.
    pub data_ids: bool,
    /// `window.location.origin`, for `toValidURL`.
    pub origin: &'a str,
    /// Measures the embeddable placeholder labels.
    pub text_metrics: &'a dyn TextMetricsProvider,
    /// `Date.now()` and the viewer's time zone, for sticky note footers.
    pub clock: Clock,
}

/// What `renderSceneToSvg` adds to the document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SvgDrawing {
    /// The image symbols put first in `<defs>`, in document order.
    pub symbols: Vec<SvgNode>,
    /// The nodes appended to the root, in order.
    pub nodes: Vec<SvgNode>,
}

/// Why an element stopped drawing: what upstream throws on.
#[derive(Clone, Debug, PartialEq)]
pub enum SvgRenderError {
    /// "Selection rendering is not supported for SVG".
    Selection,
    Shape(ShapeError),
    /// `querySelector("#" + symbolId)` threw: the id is not a selector.
    InvalidSelector(String),
}

impl fmt::Display for SvgRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SvgRenderError::Selection => {
                f.write_str("Selection rendering is not supported for SVG")
            }
            SvgRenderError::Shape(e) => write!(f, "{e}"),
            SvgRenderError::InvalidSelector(s) => {
                write!(f, "'#{s}' is not a valid selector")
            }
        }
    }
}

impl std::error::Error for SvgRenderError {}

impl From<ShapeError> for SvgRenderError {
    fn from(e: ShapeError) -> Self {
        SvgRenderError::Shape(e)
    }
}

/// `renderSceneToSvg(elements, elementsMap, rsvg, svgRoot, files,
/// renderConfig)`: `elements` are `elementsForRender`, `elements_map` their
/// map, `files` the export's `BinaryFiles`. `defs_ids` are the ids already
/// in the document (the frame clip paths), which an image symbol's lookup
/// sees; placeholder labels are named from `next_label_id` on
/// ([`label_id`]).
pub fn render_scene_to_svg(
    elements: &[&Element],
    elements_map: &ElementsMap<'_>,
    files: &Map<String, Value>,
    defs_ids: Vec<String>,
    next_label_id: usize,
    config: &SvgRenderConfig<'_>,
) -> SvgDrawing {
    // embedsValidationStatus: the frame-like elements (sic) when rendering
    // embeddables, nothing otherwise
    let embeds_validation_status: EmbedsValidationStatus = if config.render_embeddables {
        elements
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Frame(_) | ElementKind::MagicFrame(_)))
            .map(|e| (e.base.id.clone(), true))
            .collect()
    } else {
        HashMap::new()
    };
    let mut renderer = Renderer {
        map: elements_map,
        files,
        config,
        embeds_validation_status,
        defs_ids,
        next_label_id,
        drawing: SvgDrawing::default(),
    };
    let is_iframe_like =
        |e: &Element| matches!(e.kind, ElementKind::Iframe | ElementKind::Embeddable);

    for element in elements.iter().filter(|e| !is_iframe_like(e)) {
        if element.base.is_deleted {
            continue;
        }
        if let ElementKind::Text(text) = &element.kind {
            if text
                .container_id
                .as_deref()
                .is_some_and(|id| !id.is_empty() && elements_map.get(id).is_some())
            {
                // will be rendered with the container
                continue;
            }
        }
        let _ = renderer.render_with_label(element);
    }

    // render embeddables on top
    for element in elements.iter().filter(|e| is_iframe_like(e)) {
        if !element.base.is_deleted {
            let _ = renderer.render_top_level(element);
        }
    }
    renderer.drawing
}

/// The id of a text element export makes: `randomId()`, which in upstream's
/// test mode is `id0`, `id1`, ... With `data_ids` the port names them so;
/// otherwise their names are never written, and the port uses `fallback`.
pub fn label_id(data_ids: bool, n: usize, fallback: impl FnOnce() -> String) -> String {
    if data_ids {
        format!("id{n}")
    } else {
        fallback()
    }
}

/// Where a node goes: the root, or inside a node appended to it (an
/// anchor), as the path of child indices.
type Target = Vec<usize>;

struct Renderer<'a, 'm> {
    map: &'a ElementsMap<'m>,
    files: &'a Map<String, Value>,
    config: &'a SvgRenderConfig<'a>,
    embeds_validation_status: EmbedsValidationStatus,
    defs_ids: Vec<String>,
    next_label_id: usize,
    drawing: SvgDrawing,
}

/// `${value || 0}`.
fn or_zero(value: f64) -> String {
    if value == 0.0 || value.is_nan() {
        "0".to_owned()
    } else {
        number_to_string(value)
    }
}

/// The `d` of a sticky note path (`getPathData`, `staticSvgScene.ts:159-171`):
/// `M x y`, `L x y` and `Q cx cy x y` joined by spaces, then ` Z`.
fn sticky_note_path_data(commands: &[StickyNotePathCommand]) -> String {
    let n = number_to_string;
    let mut parts: Vec<String> = commands
        .iter()
        .map(|command| match *command {
            StickyNotePathCommand::Move([x, y]) => format!("M {} {}", n(x), n(y)),
            StickyNotePathCommand::Line([x, y]) => format!("L {} {}", n(x), n(y)),
            StickyNotePathCommand::Quadratic {
                control: [cx, cy],
                point: [x, y],
            } => format!("Q {} {} {} {}", n(cx), n(cy), n(x), n(y)),
        })
        .collect();
    parts.push("Z".to_owned());
    parts.join(" ")
}

/// `translate(${offsetX || 0} ${offsetY || 0}) rotate(${degree} ${cx} ${cy})`.
fn placement(offset_x: f64, offset_y: f64, degree: f64, cx: f64, cy: f64) -> String {
    format!(
        "translate({} {}) rotate({} {} {})",
        or_zero(offset_x),
        or_zero(offset_y),
        number_to_string(degree),
        number_to_string(cx),
        number_to_string(cy)
    )
}

/// `String(value)` of a JSON value, `undefined` for none.
fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_owned(),
        Some(Value::Null) => "null".to_owned(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => number_to_string(n.as_f64().unwrap_or(f64::NAN)),
        Some(Value::String(s)) => s.clone(),
        // Array.prototype.toString: the items joined by commas, null and
        // undefined as empty strings
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| match v {
                Value::Null => String::new(),
                v => js_string(Some(v)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".to_owned(),
    }
}

/// `!!value` of a JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

/// A CSS length for a `style` property: `${n}px` when the CSS parser takes
/// it (a finite, non-negative number), else none, and the declaration is
/// not set.
fn css_length(n: f64) -> Option<String> {
    (n.is_finite() && n >= 0.0).then(|| format!("{}px", number_to_string(n)))
}

/// A CSS name code point (CSS Syntax 3): letters, digits, `-`, `_` and
/// anything past ASCII.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()
}

/// How `querySelector("#" + id)` reads an id: `Ok(Some(id))` when the
/// selector is that id, `Ok(None)` when it is a valid selector that asks
/// for more than an id (`#image-a.b`: an id and a class, which no node of
/// the document has), `Err` when it is not a selector.
fn id_selector(id: &str) -> Result<Option<&str>, SvgRenderError> {
    let end = id.find(|c: char| !is_name_char(c)).unwrap_or(id.len());
    let (name, rest) = id.split_at(end);
    let starts_ident = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || !c.is_ascii() || c == '-');
    if !starts_ident {
        return Err(SvgRenderError::InvalidSelector(id.to_owned()));
    }
    let rest = rest.trim_end_matches([' ', '\t', '\n', '\r', '\u{c}']);
    if rest.is_empty() {
        return Ok(Some(name));
    }
    // more simple selectors: .class and #id, each a name
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '.' && c != '#' {
            return Err(SvgRenderError::InvalidSelector(id.to_owned()));
        }
        let mut len = 0;
        while chars.peek().is_some_and(|c| is_name_char(*c)) {
            chars.next();
            len += 1;
        }
        if len == 0 {
            return Err(SvgRenderError::InvalidSelector(id.to_owned()));
        }
    }
    Ok(None)
}

/// Whether a node or one of its descendants has the id.
fn has_id(node: &SvgNode, id: &str) -> bool {
    match node {
        SvgNode::Text(_) => false,
        SvgNode::Tag(tag) => {
            matches!(tag.get("id"), Some(SvgValue::Text(v)) if v == id)
                || tag.children.iter().any(|c| has_id(c, id))
        }
    }
}

/// The node `RoughSVG.draw` makes of a drawable (roughjs 4.6.4
/// `bin/svg.js`), with its path data at `fixedDecimalPlaceDigits` 2: a
/// group with a `<path>` per op set.
///
/// - `path`: the stroke in `o.stroke`, `stroke-width`, no fill, the dash
///   when set and its offset when truthy;
/// - `fillPath`: the fill in `o.fill || ''`, no stroke, `evenodd` for
///   `curve` and `polygon` shapes;
/// - `fillSketch`: a stroke in the fill colour at `fillWeight` (half the
///   stroke width when negative), with the fill dash.
pub fn rough_group(drawable: &Drawable) -> SvgTag {
    let o = &drawable.options;
    let dash = |list: &Option<Vec<f64>>, offset: Option<f64>, path: &mut SvgTag| {
        if let Some(list) = list {
            let joined = list
                .iter()
                .map(|n| number_to_string(*n))
                .collect::<Vec<_>>()
                .join(" ");
            path.set("stroke-dasharray", joined.trim());
        }
        if let Some(offset) = offset.filter(|o| *o != 0.0 && !o.is_nan()) {
            path.set("stroke-dashoffset", number_to_string(offset));
        }
    };
    let mut g = SvgTag::new("g");
    for set in &drawable.sets {
        let mut data = Path::new();
        for op in &set.ops {
            match *op {
                Op::Move([x, y]) => data.move_to(x, y),
                Op::LineTo([x, y]) => data.line_to(x, y),
                Op::BCurveTo([a, b, c, d, e, f]) => data.cubic_to(a, b, c, d, e, f),
            };
        }
        let d = SvgValue::RoughPath {
            path: data,
            decimals: Some(MAX_DECIMALS_FOR_SVG_EXPORT),
        };
        let fill = o.fill.clone().unwrap_or_default();
        let mut path = SvgTag::new("path");
        path.set("d", d);
        match set.kind {
            OpSetType::Path => {
                path.set("stroke", o.stroke.as_str());
                path.set("stroke-width", o.stroke_width);
                path.set("fill", "none");
                dash(&o.stroke_line_dash, o.stroke_line_dash_offset, &mut path);
            }
            OpSetType::FillPath => {
                path.set("stroke", "none");
                path.set("stroke-width", "0");
                path.set("fill", fill);
                if matches!(drawable.shape, Shape::Curve | Shape::Polygon) {
                    path.set("fill-rule", "evenodd");
                }
            }
            OpSetType::FillSketch => {
                let weight = if o.fill_weight < 0.0 {
                    o.stroke_width / 2.0
                } else {
                    o.fill_weight
                };
                path.set("stroke", fill);
                path.set("stroke-width", weight);
                path.set("fill", "none");
                dash(&o.fill_line_dash, o.fill_line_dash_offset, &mut path);
            }
        }
        g.append(path);
    }
    g
}

impl Renderer<'_, '_> {
    fn render_with_label(&mut self, element: &Element) -> Result<(), SvgRenderError> {
        self.render_top_level(element)?;
        if let Some(bound_text) = get_bound_text_element(element, self.map) {
            // a deleted label is skipped (upstream logs an invariant error)
            if !bound_text.base.is_deleted {
                self.render_top_level(bound_text)?;
            }
        }
        Ok(())
    }

    fn render_top_level(&mut self, element: &Element) -> Result<(), SvgRenderError> {
        let (x, y) = (
            element.base.x + self.config.offset_x,
            element.base.y + self.config.offset_y,
        );
        self.render(element, Vec::new(), x, y)
    }

    /// The children of the node at `target`.
    fn children(&mut self, target: &[usize]) -> &mut Vec<SvgNode> {
        let mut children = &mut self.drawing.nodes;
        for &i in target {
            children = match &mut children[i] {
                SvgNode::Tag(tag) => &mut tag.children,
                SvgNode::Text(_) => unreachable!("targets are tags"),
            };
        }
        children
    }

    fn append(&mut self, target: &[usize], node: impl Into<SvgNode>) {
        self.children(target).push(node.into());
    }

    /// `addToRoot(node, element)`: the element's id as `data-id` in test
    /// mode, then appended.
    fn add_to_root(&mut self, target: &[usize], mut node: SvgTag, element: &Element) {
        if self.config.data_ids {
            node.set("data-id", element.base.id.as_str());
        }
        self.append(target, node);
    }

    /// `maybeWrapNodesInFrameClipPath`: the nodes in a group clipped to the
    /// element's frame, when frames are clipped and it has one.
    fn wrap_in_frame_clip(
        &self,
        element: &Element,
        nodes: Vec<SvgTag>,
    ) -> Result<SvgTag, Vec<SvgTag>> {
        let fr = self.config.frame_rendering;
        if !fr.enabled || !fr.clip {
            return Err(nodes);
        }
        match get_containing_frame(element, self.map) {
            Some(frame) => {
                let mut g = SvgTag::new("g");
                g.set("clip-path", format!("url(#{})", frame.base.id));
                for node in nodes {
                    g.append(node);
                }
                Ok(g)
            }
            None => Err(nodes),
        }
    }

    /// Whether `querySelector("#" + id)` finds a node in the document.
    fn find(&self, id: &str) -> Result<bool, SvgRenderError> {
        let Some(id) = id_selector(id)? else {
            return Ok(false);
        };
        Ok(self.defs_ids.iter().any(|d| d == id)
            || self.drawing.symbols.iter().any(|n| has_id(n, id))
            || self.drawing.nodes.iter().any(|n| has_id(n, id)))
    }

    fn shape_config(&self) -> RenderConfig<'_> {
        RenderConfig {
            is_exporting: true,
            canvas_background_color: self.config.canvas_background_color,
            embeds_validation_status: Some(&self.embeds_validation_status),
            theme: self.config.theme,
        }
    }

    fn dark(&self) -> bool {
        self.config.theme == Theme::Dark
    }

    /// `renderElementToSvg(element, …, offsetX, offsetY)`.
    fn render(
        &mut self,
        element: &Element,
        root: Target,
        offset_x: f64,
        offset_y: f64,
    ) -> Result<(), SvgRenderError> {
        let b = &element.base;
        let offset = (offset_x, offset_y);
        let (mut offset_x, mut offset_y) = offset;
        let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(element, self.map, false);
        let mut cx = (x2 - x1) / 2.0 - (b.x - x1);
        let mut cy = (y2 - y1) / 2.0 - (b.y - y1);
        if matches!(element.kind, ElementKind::Text(_)) {
            if let Some(container) = get_container_element(element, self.map) {
                if matches!(container.kind, ElementKind::Arrow(_)) {
                    let [x1, y1, x2, y2, _, _] =
                        get_element_absolute_coords(container, self.map, false);
                    let [bx, by] = get_bound_text_element_position(container, element, self.map);
                    cx = (x2 - x1) / 2.0 - (bx - x1);
                    cy = (y2 - y1) / 2.0 - (by - y1);
                    offset_x = offset_x + bx - b.x;
                    offset_y = offset_y + by - b.y;
                }
            }
        }
        let degree = (180.0 * b.angle.0) / PI;

        // if the element has a link, create an anchor tag and make that the
        // new root
        let mut root = root;
        if let Some(link) = b.link.as_deref().filter(|l| !l.is_empty()) {
            let anchor = SvgTag::new("a").with("href", normalize_link(link));
            let children = self.children(&root);
            children.push(anchor.into());
            root.push(children.len() - 1);
        }

        let frame_opacity =
            get_containing_frame(element, self.map).map_or(100.0, |f| f.base.opacity);
        let opacity = (frame_opacity * b.opacity) / 10000.0;
        let set_opacity = |node: &mut SvgTag, opacity: f64| {
            if opacity != 1.0 {
                node.set("stroke-opacity", opacity);
                node.set("fill-opacity", opacity);
            }
        };
        let transform = placement(offset_x, offset_y, degree, cx, cy);
        let generator = RoughGenerator::new();

        match &element.kind {
            ElementKind::Selection => Err(SvgRenderError::Selection),
            ElementKind::StickyNote(_) => {
                let dark = self.config.theme == Theme::Dark;
                let path = |commands: &[StickyNotePathCommand], fill: &str| {
                    let mut path = SvgTag::new("path");
                    path.set("d", sticky_note_path_data(commands));
                    path.set("fill", fill);
                    path
                };
                let mut group = SvgTag::new("g");
                group.set("transform", transform);
                if opacity != 1.0 {
                    group.set("opacity", opacity);
                }

                let mut shadow = path(&get_sticky_note_path_commands(element, true), "#000");
                shadow.set("fill-opacity", STICKY_NOTE_SHADOW_OPACITY);
                shadow.set("stroke", "none");
                let commands = get_sticky_note_path_commands(element, false);
                let mut rect = path(
                    &commands,
                    &apply_dark_mode_filter(&b.background_color, dark),
                );
                rect.set("stroke", "none");
                let clip_id = format!("sticky-note-clipPath-{}", b.id);
                let mut clip_path = SvgTag::new("clipPath");
                clip_path.set("id", clip_id.as_str());
                clip_path.set("clipPathUnits", "userSpaceOnUse");
                let mut clip_shape = path(&commands, "#000");
                clip_shape.set("stroke", "none");
                clip_path.append(clip_shape);
                self.add_to_root(&root, clip_path, element);

                let mut edge_shadow = path(&commands, "none");
                edge_shadow.set("stroke", "none");
                edge_shadow.set("stroke", "#000");
                edge_shadow.set("stroke-opacity", STICKY_NOTE_EDGE_SHADOW_OPACITY);
                edge_shadow.set("stroke-width", STICKY_NOTE_EDGE_SHADOW_WIDTH * 2.0);
                edge_shadow.set("clip-path", format!("url(#{clip_id})"));

                group.append(shadow);
                group.append(rect);
                group.append(edge_shadow);

                if let Some(footer) = get_sticky_note_footer(element, &self.config.clock) {
                    let mut date = SvgTag::new("text");
                    date.set("x", footer.x);
                    date.set("y", footer.y);
                    date.set("font-family", STICKY_NOTE_FOOTER_FONT_FAMILY);
                    date.set(
                        "font-size",
                        format!("{}px", number_to_string(STICKY_NOTE_FOOTER_FONT_SIZE)),
                    );
                    // `text-anchor` is logical in SVG: pin the direction so
                    // an RTL host page can't flip the label to the left edge
                    date.set("text-anchor", "end");
                    date.set("direction", "ltr");
                    date.set("fill", apply_dark_mode_filter(&b.stroke_color, dark));
                    date.set("fill-opacity", STICKY_NOTE_FOOTER_OPACITY);
                    date.append(SvgNode::Text(footer.text));
                    group.append(date);
                }

                let node = self.wrap_in_frame_clip(element, vec![group]);
                self.add_wrapped(&root, node, element);
                Ok(())
            }
            ElementKind::Rectangle | ElementKind::Diamond | ElementKind::Ellipse => {
                let shape = generate_element_shape(element, &generator, &self.shape_config())?;
                let mut node = rough_group(&shape);
                set_opacity(&mut node, opacity);
                node.set("stroke-linecap", "round");
                node.set("transform", transform);
                let node = self.wrap_in_frame_clip(element, vec![node]);
                self.add_wrapped(&root, node, element);
                Ok(())
            }
            ElementKind::Iframe | ElementKind::Embeddable => {
                // render placeholder rectangle
                let shape = generate_element_shape(element, &generator, &self.shape_config())?;
                let mut node = rough_group(&shape);
                set_opacity(&mut node, b.opacity / 100.0);
                node.set("stroke-linecap", "round");
                node.set("transform", transform.as_str());
                self.add_to_root(&root, node, element);

                let n = self.next_label_id;
                self.next_label_id += 1;
                let mut label =
                    create_placeholder_embeddable_label(element, self.config.text_metrics);
                label.base.id = label_id(self.config.data_ids, n, || {
                    format!("{}:placeholder-label", b.id)
                });
                label.base.frame_id = None;
                let (lx, ly) = (label.base.x, label.base.y);
                self.render(
                    &label,
                    root.clone(),
                    lx + offset.0 - b.x,
                    ly + offset.1 - b.y,
                )?;

                // render embeddable element + iframe
                let mut embeddable = SvgTag::new("g");
                embeddable.set("stroke-linecap", "round");
                embeddable.set("transform", transform);
                let radius = get_corner_radius(js::min(b.width, b.height), element);
                let link = b.link.as_deref().unwrap_or("");
                let embed_link = get_embed_link(&to_valid_url(link, self.config.origin));
                let is_document = embed_link
                    .as_ref()
                    .is_some_and(|l| l.kind == EmbedType::Document);
                if !self.config.render_embeddables || is_document {
                    let mut anchor = SvgTag::new("a")
                        .with("href", normalize_link(link))
                        .with("target", "_blank")
                        .with("rel", "noopener noreferrer");
                    if let Some(r) = css_length(radius) {
                        anchor.set("style", format!("border-radius: {r};"));
                    }
                    embeddable.append(anchor);
                } else {
                    embeddable.append(self.foreign_object(
                        element,
                        radius,
                        embed_link.and_then(|l| l.link),
                    ));
                }
                self.add_to_root(&root, embeddable, element);
                Ok(())
            }
            ElementKind::Line(_) | ElementKind::Arrow(_) => {
                let bound_text = get_bound_text_element(element, self.map);
                let mut mask = SvgTag::new("mask");
                if let Some(bound_text) = bound_text {
                    mask.set("id", format!("mask-{}", b.id));
                    offset_x = if offset_x.is_nan() { 0.0 } else { offset_x };
                    offset_y = if offset_y.is_nan() { 0.0 } else { offset_y };
                    let width = b.width + 100.0 + offset_x;
                    let height = b.height + 100.0 + offset_y;
                    mask.set("maskUnits", "userSpaceOnUse");
                    mask.set("x", "0");
                    mask.set("y", "0");
                    mask.set("width", width);
                    mask.set("height", height);
                    mask.append(
                        SvgTag::new("rect")
                            .with("x", "0")
                            .with("y", "0")
                            .with("fill", "#fff")
                            .with("width", width)
                            .with("height", height),
                    );
                    let [bx, by] = get_bound_text_element_position(element, bound_text, self.map);
                    // the same padded hole the canvas renderers cut around
                    // the label
                    let mask_x = offset_x + bx - b.x - BOUND_TEXT_PADDING;
                    let mask_y = offset_y + by - b.y - BOUND_TEXT_PADDING;
                    let t = &bound_text.base;
                    mask.append(
                        SvgTag::new("rect")
                            .with("x", mask_x)
                            .with("y", mask_y)
                            .with("fill", "#000")
                            .with("width", t.width + BOUND_TEXT_PADDING * 2.0)
                            .with("height", t.height + BOUND_TEXT_PADDING * 2.0)
                            .with("opacity", "1"),
                    );
                }
                let mut group = SvgTag::new("g");
                if bound_text.is_some() {
                    group.set("mask", format!("url(#mask-{})", b.id));
                }
                group.set("stroke-linecap", "round");

                let transform = placement(offset_x, offset_y, degree, cx, cy);
                let loop_fill = match &element.kind {
                    ElementKind::Line(line) => {
                        is_path_a_loop(&line.linear.points, 1.0)
                            && b.background_color != "transparent"
                    }
                    _ => false,
                };
                for shape in
                    generate_linear_element_shapes(element, &generator, &self.shape_config())?
                {
                    let mut node = rough_group(&shape);
                    set_opacity(&mut node, opacity);
                    node.set("transform", transform.as_str());
                    if loop_fill {
                        node.set("fill-rule", "evenodd");
                    }
                    group.append(node);
                }

                match self.wrap_in_frame_clip(element, vec![group, mask]) {
                    Ok(g) => self.add_to_root(&root, g, element),
                    Err(mut nodes) => {
                        let mask = nodes.pop().expect("the mask");
                        let group = nodes.pop().expect("the group");
                        self.add_to_root(&root, group, element);
                        self.append(&root, mask);
                    }
                }
                Ok(())
            }
            ElementKind::Freedraw(_) => {
                let mut wrapper = SvgTag::new("g");
                // always ordered as [background, stroke]
                for shape in generate_freedraw_shapes(element, &generator, &self.shape_config())? {
                    match shape {
                        FreedrawShape::SvgPath(d) => wrapper.append(
                            SvgTag::new("path")
                                .with("fill", apply_dark_mode_filter(&b.stroke_color, self.dark()))
                                .with("d", d),
                        ),
                        // the drawable's group unwrapped
                        FreedrawShape::Rough(shape) => {
                            wrapper.children.extend(rough_group(&shape).children)
                        }
                    }
                }
                set_opacity(&mut wrapper, opacity);
                wrapper.set("transform", transform);
                wrapper.set("stroke", "none");
                let node = self.wrap_in_frame_clip(element, vec![wrapper]);
                self.add_wrapped(&root, node, element);
                Ok(())
            }
            ElementKind::Image(image) => {
                let width = js::round(b.width);
                let height = js::round(b.height);
                let file_data = image
                    .file_id
                    .as_ref()
                    .map(|f| f.0.as_str())
                    .filter(|id| !id.is_empty())
                    .and_then(|id| self.files.get(id))
                    .filter(|f| truthy(Some(f)));
                let Some(file_data) = file_data else {
                    return Ok(());
                };
                let file_id = js_string(file_data.get("id"));
                let mut symbol_id = format!("image-{file_id}");
                let (mut uncropped_width, mut uncropped_height) = (b.width, b.height);
                if let Some(crop) = image.crop {
                    uncropped_width = b.width / (crop.width / crop.natural_width);
                    uncropped_height = b.height / (crop.height / crop.natural_height);
                    symbol_id = format!(
                        "image-crop-{file_id}-{}",
                        hash_string(&format!(
                            "{}x{}",
                            number_to_string(uncropped_width),
                            number_to_string(uncropped_height)
                        ))
                    );
                }
                if !self.config.reuse_images {
                    symbol_id = format!("image-{}", b.id);
                }

                if !self.find(&symbol_id)? {
                    let mut img = SvgTag::new("image")
                        .with("href", js_string(file_data.get("dataURL")))
                        .with("preserveAspectRatio", "none");
                    if image.crop.is_some() || !self.config.reuse_images {
                        img.set("width", uncropped_width);
                        img.set("height", uncropped_height);
                    } else {
                        img.set("width", "100%");
                        img.set("height", "100%");
                    }
                    let symbol = SvgTag::new("symbol")
                        .with("id", symbol_id.as_str())
                        .child(img);
                    // (root.querySelector("defs") || root).prepend(symbol): an
                    // anchor has no <defs>
                    if root.is_empty() {
                        self.drawing.symbols.insert(0, symbol.into());
                    } else {
                        self.children(&root).insert(0, symbol.into());
                    }
                }

                let mut normalized_crop_x = 0.0;
                let mut normalized_crop_y = 0.0;
                if let Some(crop) = image.crop {
                    normalized_crop_x = crop.x / (crop.natural_width / uncropped_width);
                    normalized_crop_y = crop.y / (crop.natural_height / uncropped_height);
                }
                let adjusted_center_x = cx + normalized_crop_x;
                let adjusted_center_y = cy + normalized_crop_y;

                let mut use_ = SvgTag::new("use")
                    .with("href", format!("#{symbol_id}"))
                    .with("width", width + normalized_crop_x)
                    .with("height", height + normalized_crop_y)
                    .with("opacity", opacity);
                // `scale` on the <use>, translation and rotation on the <g>
                if image.scale[0] != 1.0 || image.scale[1] != 1.0 {
                    use_.set(
                        "transform",
                        format!(
                            "translate({} {}) scale({} {}) translate({} {})",
                            number_to_string(adjusted_center_x),
                            number_to_string(adjusted_center_y),
                            number_to_string(image.scale[0]),
                            number_to_string(image.scale[1]),
                            number_to_string(-adjusted_center_x),
                            number_to_string(-adjusted_center_y)
                        ),
                    );
                }

                let mut g = SvgTag::new("g");
                if self.dark()
                    && file_data.get("mimeType").and_then(Value::as_str) == Some(MIME_TYPE_SVG)
                {
                    g.set("filter", DARK_THEME_FILTER);
                }
                if image.crop.is_some() {
                    let id = format!("mask-image-crop-{}", b.id);
                    let mask = SvgTag::new("mask")
                        .with("id", id.as_str())
                        .with("fill", "#fff")
                        .child(
                            SvgTag::new("rect")
                                .with("x", normalized_crop_x)
                                .with("y", normalized_crop_y)
                                .with("width", width)
                                .with("height", height),
                        );
                    self.append(&root, mask);
                    g.set("mask", format!("url(#{id})"));
                }
                g.append(use_);
                g.set(
                    "transform",
                    format!(
                        "translate({} {}) rotate({} {} {})",
                        number_to_string(offset_x - normalized_crop_x),
                        number_to_string(offset_y - normalized_crop_y),
                        number_to_string(degree),
                        number_to_string(adjusted_center_x),
                        number_to_string(adjusted_center_y)
                    ),
                );
                if b.roundness.is_some() {
                    let id = format!("image-clipPath-{}", b.id);
                    let radius = get_corner_radius(js::min(b.width, b.height), element);
                    let (clip_x, clip_y) = if image.crop.is_some() {
                        (normalized_crop_x, normalized_crop_y)
                    } else {
                        (0.0, 0.0)
                    };
                    let clip_path = SvgTag::new("clipPath")
                        .with("id", id.as_str())
                        .with("clipPathUnits", "userSpaceOnUse")
                        .child(
                            SvgTag::new("rect")
                                .with("x", clip_x)
                                .with("y", clip_y)
                                .with("width", b.width)
                                .with("height", b.height)
                                .with("rx", radius)
                                .with("ry", radius),
                        );
                    self.add_to_root(&root, clip_path, element);
                    g.set("clip-path", format!("url(#{id})"));
                }
                let node = self.wrap_in_frame_clip(element, vec![g]);
                self.add_wrapped(&root, node, element);
                Ok(())
            }
            // frames are not rendered and only act as a container
            ElementKind::Frame(_) | ElementKind::MagicFrame(_) => {
                let fr = self.config.frame_rendering;
                if fr.enabled && fr.outline {
                    let rect = SvgTag::new("rect")
                        .with("transform", transform)
                        .with("width", format!("{}px", number_to_string(b.width)))
                        .with("height", format!("{}px", number_to_string(b.height)))
                        .with("rx", number_to_string(frame_style::RADIUS))
                        .with("ry", number_to_string(frame_style::RADIUS))
                        .with("fill", "none")
                        .with(
                            "stroke",
                            apply_dark_mode_filter(frame_style::STROKE_COLOR, self.dark()),
                        )
                        .with("stroke-width", number_to_string(frame_style::STROKE_WIDTH));
                    self.add_to_root(&root, rect, element);
                }
                Ok(())
            }
            ElementKind::Text(text) => {
                let mut node = SvgTag::new("g");
                set_opacity(&mut node, opacity);
                node.set("transform", transform);
                let normalized = text.text.replace("\r\n", "\n").replace('\r', "\n");
                let line_height_px = get_line_height_in_px(text.font_size, text.line_height);
                let horizontal_offset = match text.text_align {
                    ElementTextAlign::Center => b.width / 2.0,
                    ElementTextAlign::Right => b.width,
                    ElementTextAlign::Left => 0.0,
                };
                let vertical_offset =
                    get_vertical_offset(text.font_family, text.font_size, line_height_px);
                let rtl = is_rtl(&text.text);
                let direction = if rtl { "rtl" } else { "ltr" };
                let text_anchor = match text.text_align {
                    ElementTextAlign::Center => "middle",
                    ElementTextAlign::Right => "end",
                    ElementTextAlign::Left if rtl => "end",
                    ElementTextAlign::Left => "start",
                };
                let font_family = get_font_family_string(text.font_family);
                let fill = apply_dark_mode_filter(&b.stroke_color, self.dark());
                for (i, line) in normalized.split('\n').enumerate() {
                    let mut t = SvgTag::new("text");
                    if !line.is_empty() {
                        t.append(SvgNode::Text(line.to_owned()));
                    }
                    t.set("x", horizontal_offset);
                    t.set("y", i as f64 * line_height_px + vertical_offset);
                    t.set("font-family", font_family.as_str());
                    t.set(
                        "font-size",
                        format!("{}px", number_to_string(text.font_size)),
                    );
                    t.set("fill", fill.as_str());
                    t.set("text-anchor", text_anchor);
                    t.set("style", "white-space: pre;");
                    t.set("direction", direction);
                    t.set("dominant-baseline", "alphabetic");
                    node.append(t);
                }
                let node = self.wrap_in_frame_clip(element, vec![node]);
                self.add_wrapped(&root, node, element);
                Ok(())
            }
        }
    }

    /// `addToRoot(g || node, element)` after [`Self::wrap_in_frame_clip`].
    fn add_wrapped(
        &mut self,
        root: &[usize],
        wrapped: Result<SvgTag, Vec<SvgTag>>,
        element: &Element,
    ) {
        match wrapped {
            Ok(g) => self.add_to_root(root, g, element),
            Err(nodes) => {
                for node in nodes {
                    self.add_to_root(root, node, element);
                }
            }
        }
    }

    /// The `<foreignObject>` holding an embeddable's `<iframe>`
    /// (`staticSvgScene.ts:376-399`), each `style` as a browser's CSS object
    /// model serializes the declarations upstream sets: a length the CSS
    /// parser refuses (negative, not finite) is not set, and `border: none`
    /// is written as Chrome writes it, its four longhands. Upstream's own
    /// tests run in jsdom 22.1.0, whose cssstyle drops `border: none`
    /// altogether (the rendering fidelity page lists the difference).
    fn foreign_object(&self, element: &Element, radius: f64, link: Option<String>) -> SvgTag {
        let b = &element.base;
        let style = |declarations: &[(&str, Option<String>)]| {
            declarations
                .iter()
                .filter_map(|(name, value)| value.as_ref().map(|v| format!("{name}: {v};")))
                .collect::<Vec<_>>()
                .join(" ")
        };
        // `style.border = "none"` as Chrome's CSS object model keeps it: the
        // shorthand's longhands
        let border = [
            ("border-width", Some("medium".to_owned())),
            ("border-style", Some("none".to_owned())),
            ("border-color", Some("currentcolor".to_owned())),
            ("border-image", Some("none".to_owned())),
        ];
        let mut foreign_object = SvgTag::new("foreignObject");
        let mut declarations = vec![
            ("width", css_length(b.width)),
            ("height", css_length(b.height)),
        ];
        declarations.extend(border.clone());
        foreign_object.set("style", style(&declarations));
        let hundred = Some("100%".to_owned());
        let div = SvgTag::new("div")
            .with("xmlns", "http://www.w3.org/1999/xhtml")
            .with(
                "style",
                style(&[("width", hundred.clone()), ("height", hundred.clone())]),
            );
        let iframe = SvgTag::new("iframe")
            .with("src", link.unwrap_or_default())
            .with(
                "style",
                style(
                    &[
                        &[("width", hundred.clone()), ("height", hundred)][..],
                        &border[..],
                        &[
                            ("border-radius", css_length(radius)),
                            ("top", Some("0px".to_owned())),
                            ("left", Some("0px".to_owned())),
                        ][..],
                    ]
                    .concat(),
                ),
            )
            .with("allowfullscreen", "");
        foreign_object.append(div.child(iframe));
        foreign_object
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_ids_as_selectors() {
        assert_eq!(id_selector("image-abc_1-2"), Ok(Some("image-abc_1-2")));
        assert_eq!(id_selector("image-é"), Ok(Some("image-é")));
        // trailing white space is not part of a selector
        assert_eq!(id_selector("image-a "), Ok(Some("image-a")));
        // an id and a class, or two ids: valid, and nothing matches
        assert_eq!(id_selector("image-a.b"), Ok(None));
        assert_eq!(id_selector("image-a#b.c"), Ok(None));
        for invalid in ["image-a:b", "image-a[b]", "image-a.", "image-a b", "1image"] {
            assert!(id_selector(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn values_as_upstream_stringifies_them() {
        assert_eq!(or_zero(0.0), "0");
        assert_eq!(or_zero(-0.0), "0");
        assert_eq!(or_zero(f64::NAN), "0");
        assert_eq!(or_zero(1.5), "1.5");
        assert_eq!(
            placement(10.0, f64::NAN, 28.64788975654116, 10.25, 0.1 + 0.2),
            "translate(10 0) rotate(28.64788975654116 10.25 0.30000000000000004)"
        );
        assert_eq!(js_string(None), "undefined");
        assert_eq!(js_string(Some(&Value::Null)), "null");
        assert_eq!(js_string(Some(&serde_json::json!(12))), "12");
        assert_eq!(js_string(Some(&serde_json::json!([1, null, "a"]))), "1,,a");
        assert_eq!(js_string(Some(&serde_json::json!({}))), "[object Object]");
        assert_eq!(css_length(8.0).as_deref(), Some("8px"));
        assert_eq!(css_length(0.0).as_deref(), Some("0px"));
        assert_eq!(css_length(-3.0), None);
        assert_eq!(css_length(f64::NAN), None);
        assert_eq!(label_id(true, 3, || "x".to_owned()), "id3");
        assert_eq!(label_id(false, 3, || "x".to_owned()), "x");
    }

    #[test]
    fn a_drawable_as_rough_svg_draws_it() {
        use excali_rough::{OpSet, Options};
        let drawable = Drawable {
            shape: Shape::Curve,
            options: Options {
                stroke: "#123".to_owned(),
                stroke_width: 2.5,
                fill: Some("#abc".to_owned()),
                stroke_line_dash: Some(vec![8.0, 10.0]),
                stroke_line_dash_offset: Some(0.0),
                fill_line_dash: Some(vec![]),
                fill_line_dash_offset: Some(3.0),
                ..Options::default()
            },
            sets: vec![
                OpSet::fill_path(vec![Op::Move([0.004, 1.0]), Op::LineTo([2.0, 3.125])]),
                OpSet::fill_sketch(vec![Op::Move([0.0, 0.0])]),
                OpSet::path(vec![Op::BCurveTo([1.0, 2.0, 3.0, 4.0, 5.0, 6.0])]),
            ],
        };
        let g = rough_group(&drawable);
        let paths: Vec<Vec<(&str, SvgValue)>> = g
            .children
            .iter()
            .map(|c| match c {
                SvgNode::Tag(t) => t.attributes.clone(),
                SvgNode::Text(_) => unreachable!(),
            })
            .collect();
        let names = |i: usize| paths[i].iter().map(|(n, _)| *n).collect::<Vec<_>>();
        assert_eq!(
            names(0),
            ["d", "stroke", "stroke-width", "fill", "fill-rule"]
        );
        assert_eq!(paths[0][4].1, SvgValue::from("evenodd"));
        assert_eq!(paths[0][2].1, SvgValue::from("0"));
        // fillSketch: the fill colour at half the stroke width, an empty
        // dash (an empty array is truthy) and a truthy offset
        assert_eq!(
            names(1),
            [
                "d",
                "stroke",
                "stroke-width",
                "fill",
                "stroke-dasharray",
                "stroke-dashoffset"
            ]
        );
        assert_eq!(paths[1][1].1, SvgValue::from("#abc"));
        assert_eq!(paths[1][2].1, SvgValue::Number(1.25));
        assert_eq!(paths[1][4].1, SvgValue::from(""));
        assert_eq!(paths[1][5].1, SvgValue::from("3"));
        // path: a zero offset is falsy
        assert_eq!(
            names(2),
            ["d", "stroke", "stroke-width", "fill", "stroke-dasharray"]
        );
        assert_eq!(paths[2][4].1, SvgValue::from("8 10"));
        match &paths[2][0].1 {
            SvgValue::RoughPath { decimals, .. } => assert_eq!(*decimals, Some(2)),
            other => panic!("{other:?}"),
        }
    }
}
