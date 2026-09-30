//! `renderElement` (`packages/element/src/renderElement.ts`) as display
//! items: how the static scene draws one element.
//!
//! - [`resolve_element_render_state`] (`:157-202`): the element's alpha
//!   (its frame's opacity times its own, both clamped to 0..=100 and read
//!   from the render overrides first, times `ELEMENT_READY_TO_ERASE_OPACITY`
//!   while it, its frame or a pending flowchart node with its id is being
//!   erased) and its render offset (a bound label takes its container's).
//! - [`render_element`] (`:963-1009`): `globalAlpha` (reduced to
//!   `DEFAULT_REDUCED_GLOBAL_ALPHA` for elements neither selected nor
//!   hovered while the element link selector is open), the offset, then
//!   `drawElement` (`:1011-1273`):
//!   - frames and magic frames: the outline, a `roundRect` of radius
//!     `FRAME_STYLE.radius / zoom` stroked `FRAME_STYLE.strokeWidth / zoom`
//!     wide in `#bbb` (magic frames `#7affd7`, dark `#1d8264` filtered),
//!     when frame rendering and outlines are on;
//!   - everything else: translated to the centre of its box plus the
//!     scroll, rotated, shifted back by half the box, and drawn by
//!     `drawElementOnCanvas` (`:431-680`); images are scaled by
//!     `element.scale` after the rotation, and an arrow with a label is
//!     clipped even-odd to leave a hole of the label's size plus
//!     `BOUND_TEXT_PADDING` (`:1137-1177`).
//! - `drawElementOnCanvas`: rough.js shapes through
//!   [`crate::rough_canvas::draw`] with round caps and joins for boxes,
//!   lines and arrows; a freedraw's loop fill (butt caps and miter joins,
//!   the context's own) and its outline filled with the stroke colour as
//!   `new Path2D(svgPath)` ([`Path::from_svg_path_data`]); text one
//!   `fillText` per line at `index × lineHeightPx + verticalOffset`, `x` at
//!   0, half or all of the width by `textAlign`, the direction from
//!   [`is_rtl`]; images from the image cache, clipped to a rounded
//!   rectangle when they have roundness, with the source rectangle of their
//!   crop and the dark theme filter for SVG images in dark mode, or
//!   upstream's placeholder (`drawImagePlaceholder`, `:361-385`).
//!
//! [`render_element`] draws every element as vectors, the path upstream
//! takes when exporting (`renderConfig.isExporting`). In the editor
//! upstream draws each element once into a bitmap of its own and blits
//! that (`generateElementWithCanvas`, `drawElementFromCanvas`, `:687-934`),
//! which differs only by the bitmap's resampling and its snapping to whole
//! device pixels: that path is [`crate::element_canvas`], built on the same
//! `drawElementOnCanvas`. Here the element's render offset is a
//! translation, as the export path applies it.
//!
//! - Sticky notes (`:387-472`): the shadow outline filled black at
//!   `STICKY_NOTE_SHADOW_OPACITY`, the outline filled with the background
//!   colour, the outline stroked inside itself (clipped to it) black at
//!   `STICKY_NOTE_EDGE_SHADOW_OPACITY`, then the creation-date footer
//!   right-aligned in 12px Helvetica in the stroke colour
//!   ([`crate::sticky_note`]).
//!
//! Not drawn here: the frame clip of a frame's children
//! (`clipElementToFrame`, ex-403). Safari's pixel inversion of
//! dark SVG images (`:553-601`) is Safari's; the port applies the filter
//! as the other browsers do.

use std::fmt;

use excali_core::color::apply_dark_mode_filter;
use excali_core::constants::BOUND_TEXT_PADDING;
use excali_core::constants::COLOR_WHITE;
use excali_core::element::{
    Element, ElementBase, ElementKind, FontFamily, ImageStatus, TextAlign as ElementTextAlign,
    TextFields, VerticalAlign,
};
use excali_core::json::number_to_string;
use excali_math::js;
use excali_math::{point_from, point_rotate_rads, Global, Point, Radians};
use excali_rough::RoughGenerator;
use excali_text::font_metadata::{
    get_font_family_string, get_font_string, get_line_height, get_line_height_in_px,
    get_vertical_offset,
};
use excali_text::text_measurements::{measure_text, normalize_text, TextMetricsProvider};
use excali_text::text_wrapping::wrap_text;

use crate::bounds::{
    get_bound_text_element, get_container_element, get_element_absolute_coords, ElementsMap,
};
use crate::display::{
    Clip, Color, Direction, DisplayItem, FillRule, Font, Group, ImageFilter, ImageItem, LineCap,
    LineJoin, Path, Rect, Stroke, TextAlign, TextRun, Transform,
};
use crate::element_canvas::get_canvas_padding;
// The built-in images live with the display list, where backends resolve
// them; re-exported here, where the element drawing names them.
pub use crate::display::{
    builtin_image, builtin_image_by_id, BuiltinImage, BUILTIN_IMAGE_NAMES, ELEMENT_LINK_ID,
    EXTERNAL_LINK_ID, IMAGE_ERROR_PLACEHOLDER_ID, IMAGE_PLACEHOLDER_ID,
};
use crate::export::frame_style;
use crate::linear_element::get_bound_text_element_position;
use crate::rough_canvas::{draw, ToFixedRangeError};
use crate::shape::{
    generate_element_shape, generate_freedraw_shapes, generate_linear_element_shapes,
    FreedrawShape, RenderConfig, ShapeError, Theme,
};
use crate::static_scene::{StaticCanvasAppState, StaticCanvasRenderConfig};
use crate::sticky_note::{
    get_sticky_note_footer, get_sticky_note_path_commands, StickyNotePathCommand,
    STICKY_NOTE_EDGE_SHADOW_OPACITY, STICKY_NOTE_EDGE_SHADOW_WIDTH, STICKY_NOTE_FOOTER_FONT_FAMILY,
    STICKY_NOTE_FOOTER_FONT_SIZE, STICKY_NOTE_FOOTER_OPACITY, STICKY_NOTE_SHADOW_OPACITY,
};
use crate::utils::get_corner_radius;

/// `DEFAULT_REDUCED_GLOBAL_ALPHA` (`common/src/constants.ts:594`).
pub const DEFAULT_REDUCED_GLOBAL_ALPHA: f64 = 0.3;
/// `ELEMENT_READY_TO_ERASE_OPACITY` (`common/src/constants.ts:437`), in
/// percent.
pub const ELEMENT_READY_TO_ERASE_OPACITY: f64 = 20.0;
/// The `openDialog.name` that dims the scene (`renderElement.ts:978-981`).
pub const ELEMENT_LINK_SELECTOR_DIALOG: &str = "elementLinkSelector";

/// `ElementRenderOverride` (`packages/excalidraw/types.ts:819-825`): an
/// opacity (0-100, clamped) and a translation in scene units a host puts on
/// an element without changing it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ElementRenderOverride {
    pub opacity: Option<f64>,
    pub offset: Option<[f64; 2]>,
}

/// `ElementRenderState` (`renderElement.ts:151-155`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementRenderState {
    /// Alpha including frame opacity and pending erasure.
    pub opacity: f64,
    pub offset: [f64; 2],
}

/// Why an element was not drawn: where upstream's drawing throws, and the
/// static scene's `try`/`catch` skips the element.
#[derive(Clone, Debug, PartialEq)]
pub enum RenderError {
    /// Building the element's shape failed.
    Shape(ShapeError),
    /// `toFixed` threw while drawing a rough.js shape.
    ToFixed(ToFixedRangeError),
    /// A selection element is never part of a scene
    /// (`Unimplemented type selection`).
    Selection,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Shape(e) => e.fmt(f),
            RenderError::ToFixed(e) => e.fmt(f),
            RenderError::Selection => f.write_str("Unimplemented type selection"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<ShapeError> for RenderError {
    fn from(e: ShapeError) -> Self {
        RenderError::Shape(e)
    }
}

impl From<ToFixedRangeError> for RenderError {
    fn from(e: ToFixedRangeError) -> Self {
        RenderError::ToFixed(e)
    }
}

// ---------------------------------------------------------------------------
// Text helpers

/// `isRTL(text)` (`common/src/utils.ts:363-374`): whether the first
/// strongly directional character is right-to-left, over UTF-16 code units
/// as the regular expression reads them (so a surrogate, in the
/// left-to-right range, ends the search).
pub fn is_rtl(text: &str) -> bool {
    const LTR: [(u16, u16); 8] = [
        (0x41, 0x5A),
        (0x61, 0x7A),
        (0xC0, 0xD6),
        (0xD8, 0xF6),
        (0xF8, 0x2B8),
        (0x300, 0x590),
        (0x800, 0x1FFF),
        (0x2C00, 0xFB1C),
    ];
    const LTR_TAIL: [(u16, u16); 2] = [(0xFDFE, 0xFE6F), (0xFEFD, 0xFFFF)];
    const RTL: [(u16, u16); 3] = [(0x591, 0x7FF), (0xFB1D, 0xFDFD), (0xFE70, 0xFEFC)];
    let within = |ranges: &[(u16, u16)], u: u16| ranges.iter().any(|&(a, b)| (a..=b).contains(&u));
    for u in text.encode_utf16() {
        if within(&LTR, u) || within(&LTR_TAIL, u) {
            return false;
        }
        if within(&RTL, u) {
            return true;
        }
    }
    false
}

/// `createPlaceholderEmbeddableLabel(element)` (`element/src/embeddable.ts:
/// 402-437`): the text an iframe or embeddable shows in place of its
/// content: "IFrame element", the link, or "Empty Web-Embed", in Helvetica
/// sized to the element (`max(min(w / 2, w / length), w / 30)`), wrapped to
/// `w - 20`, centred on the element and turned with it. As upstream spreads
/// them over the new text element, it has the element's `id` and
/// `frameId`, so it takes the element's overrides and frame opacity.
pub fn create_placeholder_embeddable_label(
    element: &Element,
    text_metrics: &dyn TextMetricsProvider,
) -> Element {
    let b = &element.base;
    let text = if matches!(element.kind, ElementKind::Iframe) {
        "IFrame element".to_owned()
    } else {
        match b.link.as_deref() {
            None | Some("") => "Empty Web-Embed".to_owned(),
            Some(link) => link.to_owned(),
        }
    };
    let length = text.encode_utf16().count() as f64;
    let font_size = js::max(js::min(b.width / 2.0, b.width / length), b.width / 30.0);
    let font_family = FontFamily::HELVETICA;
    let font_string = get_font_string(font_size, font_family);
    let mut char_widths = Default::default();
    let wrapped = wrap_text(
        &text,
        &font_string,
        b.width - 20.0,
        text_metrics,
        &mut char_widths,
    );

    // newTextElement (newElement.ts:335-386)
    let line_height = get_line_height(font_family);
    let text = normalize_text(&wrapped);
    let metrics = measure_text(&text, &font_string, line_height, text_metrics);
    let x = b.x + b.width / 2.0;
    let y = b.y + b.height / 2.0;
    let mut base = ElementBase::new(
        b.id.clone(),
        x - metrics.width * 0.5,
        y - metrics.height * 0.5,
        0.0,
        0.0,
    );
    base.width = metrics.width;
    base.height = metrics.height;
    base.stroke_color = if b.stroke_color != "transparent" {
        b.stroke_color.clone()
    } else {
        "black".to_owned()
    };
    base.background_color = "transparent".to_owned();
    base.angle = b.angle;
    base.frame_id = b.frame_id.clone();
    let mut fields = TextFields::new(text, font_family, line_height);
    fields.font_size = font_size;
    fields.text_align = ElementTextAlign::Center;
    fields.vertical_align = VerticalAlign::Middle;
    Element::new(base, ElementKind::Text(fields))
}

// ---------------------------------------------------------------------------
// Render state

/// `clamp(value, min, max)` (`common/src/utils.ts`): `Math.min(Math.max(
/// value, min), max)`, NaN staying NaN.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    js::min(js::max(value, min), max)
}

/// `getContainingFrame(element, elementsMap)` (`frame.ts:436-445`): the
/// element the `frameId` names, whatever its type.
pub fn get_containing_frame<'a>(
    element: &Element,
    elements_map: &ElementsMap<'a>,
) -> Option<&'a Element> {
    elements_map.get(element.base.frame_id.as_deref()?)
}

/// `getElementRenderOffset(element, elementsMap, overrides)`
/// (`renderElement.ts:123-132`): the offset of the element's override, or
/// of its container's for a bound label.
pub fn get_element_render_offset(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    config: &StaticCanvasRenderConfig,
) -> Option<[f64; 2]> {
    let container = get_container_element(element, elements_map);
    let id = &container.unwrap_or(element).base.id;
    config.element_render_overrides.get(id)?.offset
}

/// `resolveElementRenderState(element, elementsMap, renderConfig,
/// allElementsMap)` (`renderElement.ts:157-202`).
pub fn resolve_element_render_state(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    config: &StaticCanvasRenderConfig,
    all_elements_map: &ElementsMap<'_>,
) -> ElementRenderState {
    let overrides = &config.element_render_overrides;
    let id = element.base.id.as_str();
    let own = overrides.get(id);
    let containing_frame = get_containing_frame(element, elements_map);
    let frame_opacity = match containing_frame {
        Some(frame) => clamp(
            overrides
                .get(&frame.base.id)
                .and_then(|o| o.opacity)
                .unwrap_or(frame.base.opacity),
            0.0,
            100.0,
        ),
        None => 100.0,
    };
    // Frame and element alpha multiply (50% each produces 25%).
    let mut opacity = (frame_opacity
        * clamp(
            own.and_then(|o| o.opacity).unwrap_or(element.base.opacity),
            0.0,
            100.0,
        ))
        / 10000.0;
    if config.elements_pending_erasure.contains(id)
        || config
            .pending_flowchart_nodes
            .iter()
            .any(|node| node.base.id == id)
        || containing_frame.is_some_and(|f| config.elements_pending_erasure.contains(&f.base.id))
    {
        opacity *= ELEMENT_READY_TO_ERASE_OPACITY / 100.0;
    }
    let offset = get_element_render_offset(element, all_elements_map, config).unwrap_or([0.0, 0.0]);
    ElementRenderState { opacity, offset }
}

// ---------------------------------------------------------------------------
// Drawing

/// A group that applies one canvas call's matrix to `items`.
pub(crate) fn with_transform(transform: Transform, items: Vec<DisplayItem>) -> DisplayItem {
    DisplayItem::Group(Group {
        transform,
        ..Group::new(items)
    })
}

/// The matrix `ctx.rotate(angle)` multiplies in, with V8's `Math.sin` and
/// `Math.cos`.
pub(crate) fn rotate(angle: f64) -> Transform {
    let (s, c) = (js::sin(angle), js::cos(angle));
    Transform::new(c, s, -s, c, 0.0, 0.0)
}

/// `distance(x, y)` (`common/src/utils.ts`): `Math.abs(x - y)`.
pub(crate) fn distance(x: f64, y: f64) -> f64 {
    (x - y).abs()
}

/// `renderElement(element, …, renderState)` (`renderElement.ts:963-1009`):
/// the element's items inside one group holding its alpha and offset.
/// `render_state` defaults to [`resolve_element_render_state`] of the
/// element.
pub fn render_element(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    all_elements_map: &ElementsMap<'_>,
    config: &StaticCanvasRenderConfig,
    app_state: &StaticCanvasAppState,
    render_state: Option<ElementRenderState>,
) -> Result<DisplayItem, RenderError> {
    let state = render_state.unwrap_or_else(|| {
        resolve_element_render_state(element, elements_map, config, all_elements_map)
    });
    let opacity = element_alpha(element, app_state, &state);
    let [ox, oy] = state.offset;
    // `offset.x || offset.y`: zero and NaN are falsy
    let transform = if (ox != 0.0 && !ox.is_nan()) || (oy != 0.0 && !oy.is_nan()) {
        Transform::translate(ox, oy)
    } else {
        Transform::IDENTITY
    };
    let items = draw_element(element, elements_map, config, app_state)?;
    let drawn = DisplayItem::Group(Group {
        transform,
        opacity,
        clip: None,
        items,
    });
    // the crop editor's preview (`:1220-1251`), drawn on the editor path
    // only: the whole image at alpha 0.1, not times the element's, at
    // document coordinates, then the element
    if config.is_exporting || !crate::element_canvas::is_cropping(element, app_state) {
        return Ok(drawn);
    }
    let uncropped = crate::crop::get_uncropped_image_element(element, elements_map);
    let preview = DisplayItem::Group(Group {
        transform: Transform::IDENTITY,
        opacity: crate::element_canvas::CROP_PREVIEW_ALPHA,
        clip: None,
        items: draw_element(&uncropped, all_elements_map, config, app_state)?,
    });
    Ok(DisplayItem::Group(Group {
        transform: Transform::IDENTITY,
        opacity: 1.0,
        clip: None,
        items: vec![preview, drawn],
    }))
}

/// The `globalAlpha` renderElement sets (`:978-986`): the render state's
/// opacity, reduced to `DEFAULT_REDUCED_GLOBAL_ALPHA` for an element
/// neither selected nor hovered while the element link selector is open.
pub(crate) fn element_alpha(
    element: &Element,
    app_state: &StaticCanvasAppState,
    state: &ElementRenderState,
) -> f64 {
    let id = element.base.id.as_str();
    let reduce_alpha_for_selection = app_state.open_dialog.as_deref()
        == Some(ELEMENT_LINK_SELECTOR_DIALOG)
        && !app_state.selected_element_ids.contains(id)
        && !app_state.hovered_element_ids.contains(id);
    state.opacity
        * if reduce_alpha_for_selection {
            DEFAULT_REDUCED_GLOBAL_ALPHA
        } else {
            1.0
        }
}

/// `drawElement` (`renderElement.ts:1011-1273`) on the vector path.
fn draw_element(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    config: &StaticCanvasRenderConfig,
    app_state: &StaticCanvasAppState,
) -> Result<Vec<DisplayItem>, RenderError> {
    let b = &element.base;
    match &element.kind {
        ElementKind::Frame(_) | ElementKind::MagicFrame(_) => {
            let fr = app_state.frame_rendering;
            if !(fr.enabled && fr.outline) {
                return Ok(Vec::new());
            }
            let dark = app_state.theme == Theme::Dark;
            let color = if matches!(element.kind, ElementKind::MagicFrame(_)) {
                // TODO change later to only affect AI frames
                if dark {
                    apply_dark_mode_filter("#1d8264", true)
                } else {
                    "#7affd7".to_owned()
                }
            } else {
                apply_dark_mode_filter(frame_style::STROKE_COLOR, dark)
            };
            let zoom = app_state.zoom;
            let stroke = Stroke::new(Color::new(color), frame_style::STROKE_WIDTH / zoom);
            let path = Path::round_rect(0.0, 0.0, b.width, b.height, frame_style::RADIUS / zoom);
            Ok(vec![with_transform(
                Transform::translate(b.x + app_state.scroll_x, b.y + app_state.scroll_y),
                vec![DisplayItem::Stroke { path, stroke }],
            )])
        }
        ElementKind::Selection => Err(RenderError::Selection),
        _ => {
            let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(element, elements_map, false);
            let center_x = (x1 + x2) / 2.0;
            let center_y = (y1 + y2) / 2.0;
            let cx = center_x + app_state.scroll_x;
            let cy = center_y + app_state.scroll_y;
            let mut shift_x = (x2 - x1) / 2.0 - (b.x - x1);
            let mut shift_y = (y2 - y1) / 2.0 - (b.y - y1);
            if matches!(element.kind, ElementKind::Text(_)) {
                if let Some(container) = get_container_element(element, elements_map) {
                    if matches!(container.kind, ElementKind::Arrow(_)) {
                        let [bx, by] =
                            get_bound_text_element_position(container, element, elements_map);
                        shift_x = (x2 - x1) / 2.0 - (bx - x1);
                        shift_y = (y2 - y1) / 2.0 - (by - y1);
                    }
                }
            }
            let content = draw_element_on_canvas(element, config)?;
            let angle = b.angle.0;

            let bound_text = get_bound_text_element(element, elements_map);
            let inner = match (&element.kind, bound_text) {
                (ElementKind::Arrow(_), Some(label)) => {
                    // the label "hole": the arrow's strokes clipped even-odd
                    // out of the label's box, axis-aligned in scene space
                    shift_x = b.width / 2.0 - (b.x - x1);
                    shift_y = b.height / 2.0 - (b.y - y1);
                    let [.., label_cx, label_cy] =
                        get_element_absolute_coords(label, elements_map, false);
                    let l = &label.base;
                    let hole_x = label_cx - center_x - l.width / 2.0 - BOUND_TEXT_PADDING;
                    let hole_y = label_cy - center_y - l.height / 2.0 - BOUND_TEXT_PADDING;
                    let hole_width = l.width + BOUND_TEXT_PADDING * 2.0;
                    let hole_height = l.height + BOUND_TEXT_PADDING * 2.0;
                    // generously covers the arrow's painted extent at any
                    // rotation
                    let outer_half = js::max(distance(x1, x2), distance(y1, y2))
                        + get_canvas_padding(element) * 10.0;
                    let mut clip =
                        Path::rect(-outer_half, -outer_half, outer_half * 2.0, outer_half * 2.0);
                    clip.commands
                        .extend(Path::rect(hole_x, hole_y, hole_width, hole_height).commands);
                    DisplayItem::Group(Group {
                        clip: Some(Clip {
                            path: clip,
                            rule: FillRule::EvenOdd,
                        }),
                        ..Group::new(vec![with_transform(
                            rotate(angle),
                            vec![with_transform(
                                Transform::translate(-shift_x, -shift_y),
                                content,
                            )],
                        )])
                    })
                }
                _ => {
                    let shifted = with_transform(Transform::translate(-shift_x, -shift_y), content);
                    let scaled = match &element.kind {
                        // note: scale must be applied *after* rotating
                        ElementKind::Image(image) => with_transform(
                            Transform::scale(image.scale[0], image.scale[1]),
                            vec![shifted],
                        ),
                        _ => shifted,
                    };
                    with_transform(rotate(angle), vec![scaled])
                }
            };
            Ok(vec![with_transform(
                Transform::translate(cx, cy),
                vec![inner],
            )])
        }
    }
}

/// `drawElementOnCanvas` (`renderElement.ts:431-680`): the element in its
/// own coordinates.
pub(crate) fn draw_element_on_canvas(
    element: &Element,
    config: &StaticCanvasRenderConfig,
) -> Result<Vec<DisplayItem>, RenderError> {
    let b = &element.base;
    let dark = config.theme == Theme::Dark;
    let generator = RoughGenerator::new();
    let shape_config = RenderConfig {
        is_exporting: config.is_exporting,
        canvas_background_color: config
            .host_canvas_background
            .as_deref()
            .unwrap_or(&config.canvas_background_color),
        canvas_background_unfiltered: config.host_canvas_background.is_some(),
        embeds_validation_status: Some(&config.embeds_validation_status),
        theme: config.theme,
    };
    match &element.kind {
        ElementKind::Rectangle
        | ElementKind::Iframe
        | ElementKind::Embeddable
        | ElementKind::Diamond
        | ElementKind::Ellipse => {
            let shape = generate_element_shape(element, &generator, &shape_config)?;
            Ok(draw(&shape, LineCap::Round, LineJoin::Round)?)
        }
        ElementKind::Arrow(_) | ElementKind::Line(_) => {
            let mut items = Vec::new();
            for shape in generate_linear_element_shapes(element, &generator, &shape_config)? {
                items.extend(draw(&shape, LineCap::Round, LineJoin::Round)?);
            }
            Ok(items)
        }
        ElementKind::Freedraw(_) => {
            let mut items = Vec::new();
            for shape in generate_freedraw_shapes(element, &generator, &shape_config)? {
                match shape {
                    FreedrawShape::SvgPath(d) => items.push(DisplayItem::Fill {
                        path: Path::from_svg_path_data(&d),
                        color: Color::new(apply_dark_mode_filter(&b.stroke_color, dark)),
                        rule: FillRule::NonZero,
                    }),
                    // the context's own caps and joins: butt and miter
                    FreedrawShape::Rough(shape) => {
                        items.extend(draw(&shape, LineCap::Butt, LineJoin::Miter)?)
                    }
                }
            }
            Ok(items)
        }
        ElementKind::Image(image) => {
            let cached = image
                .file_id
                .as_ref()
                .map(|id| id.0.as_str())
                .filter(|id| !id.is_empty())
                .and_then(|id| config.image_cache.get(id).map(|entry| (id, entry)));
            let Some((file_id, entry)) = cached else {
                return Ok(draw_image_placeholder(element, image.status, config.theme));
            };
            let source = image
                .crop
                .map(|crop| Rect::new(crop.x, crop.y, crop.width, crop.height));
            let filter =
                (dark && entry.mime_type == "image/svg+xml").then_some(ImageFilter::DarkTheme);
            let item = DisplayItem::Image(ImageItem {
                id: file_id.to_owned(),
                source,
                dest: Rect::new(0.0, 0.0, b.width, b.height),
                smoothing: true,
                filter,
            });
            if b.roundness.is_some() {
                let radius = get_corner_radius(js::min(b.width, b.height), element);
                return Ok(vec![DisplayItem::Group(Group {
                    clip: Some(Clip {
                        path: Path::round_rect(0.0, 0.0, b.width, b.height, radius),
                        rule: FillRule::NonZero,
                    }),
                    ..Group::new(vec![item])
                })]);
            }
            Ok(vec![item])
        }
        ElementKind::Text(text) => {
            let direction = if is_rtl(&text.text) {
                Direction::Rtl
            } else {
                Direction::Ltr
            };
            let font = Font::new(text.font_size, get_font_family_string(text.font_family));
            let color = Color::new(apply_dark_mode_filter(&b.stroke_color, dark));
            let (align, horizontal_offset) = match text.text_align {
                ElementTextAlign::Center => (TextAlign::Center, b.width / 2.0),
                ElementTextAlign::Right => (TextAlign::Right, b.width),
                ElementTextAlign::Left => (TextAlign::Left, 0.0),
            };
            let line_height_px = get_line_height_in_px(text.font_size, text.line_height);
            let vertical_offset =
                get_vertical_offset(text.font_family, text.font_size, line_height_px);
            // Canvas does not support multiline text by default
            let normalized = text.text.replace("\r\n", "\n").replace('\r', "\n");
            Ok(normalized
                .split('\n')
                .enumerate()
                .map(|(index, line)| {
                    DisplayItem::Text(TextRun {
                        text: line.to_owned(),
                        x: horizontal_offset,
                        y: index as f64 * line_height_px + vertical_offset,
                        font: font.clone(),
                        color: color.clone(),
                        align,
                        direction,
                    })
                })
                .collect())
        }
        ElementKind::StickyNote(_) => Ok(draw_sticky_note(element, config)),
        ElementKind::Frame(_) | ElementKind::MagicFrame(_) | ElementKind::Selection => {
            Ok(Vec::new())
        }
    }
}

/// `drawStickyNotePath(context, commands)` (`renderElement.ts:387-407`).
pub fn sticky_note_path(commands: &[StickyNotePathCommand]) -> Path {
    let mut path = Path::new();
    for command in commands {
        match *command {
            StickyNotePathCommand::Move([x, y]) => path.move_to(x, y),
            StickyNotePathCommand::Line([x, y]) => path.line_to(x, y),
            StickyNotePathCommand::Quadratic {
                control: [cx, cy],
                point: [x, y],
            } => path.quad_to(cx, cy, x, y),
        };
    }
    path.close();
    path
}

/// The `stickynote` case of `drawElementOnCanvas` (`renderElement.ts:438-472`).
fn draw_sticky_note(element: &Element, config: &StaticCanvasRenderConfig) -> Vec<DisplayItem> {
    let b = &element.base;
    let dark = config.theme == Theme::Dark;
    let shadow = sticky_note_path(&get_sticky_note_path_commands(element, true));
    let outline = sticky_note_path(&get_sticky_note_path_commands(element, false));
    let mut items = vec![
        DisplayItem::Fill {
            path: shadow,
            color: Color::new(format!(
                "rgba(0, 0, 0, {})",
                number_to_string(STICKY_NOTE_SHADOW_OPACITY)
            )),
            rule: FillRule::NonZero,
        },
        DisplayItem::Fill {
            path: outline.clone(),
            color: Color::new(apply_dark_mode_filter(&b.background_color, dark)),
            rule: FillRule::NonZero,
        },
        // strokeStickyNoteEdge: the inner half of the stroke, clipped to the
        // outline
        DisplayItem::Group(Group {
            clip: Some(Clip {
                path: outline.clone(),
                rule: FillRule::NonZero,
            }),
            ..Group::new(vec![DisplayItem::Stroke {
                path: outline,
                stroke: Stroke::new(
                    Color::new(format!(
                        "rgba(0, 0, 0, {})",
                        number_to_string(STICKY_NOTE_EDGE_SHADOW_OPACITY)
                    )),
                    STICKY_NOTE_EDGE_SHADOW_WIDTH * 2.0,
                ),
            }])
        }),
    ];
    // the label is absolute, so a cached bitmap only goes stale at a year
    // boundary, and is regenerated on the next zoom, theme or element change
    if let Some(footer) = get_sticky_note_footer(element, &config.clock) {
        items.push(DisplayItem::Group(Group {
            opacity: STICKY_NOTE_FOOTER_OPACITY,
            ..Group::new(vec![DisplayItem::Text(TextRun {
                align: TextAlign::Right,
                ..TextRun::new(
                    footer.text,
                    footer.x,
                    footer.y,
                    Font::new(STICKY_NOTE_FOOTER_FONT_SIZE, STICKY_NOTE_FOOTER_FONT_FAMILY),
                    Color::new(apply_dark_mode_filter(&b.stroke_color, dark)),
                )
            })])
        }));
    }
    items
}

/// `drawImagePlaceholder`'s box in the light theme (`renderElement.ts:366`).
pub const IMAGE_PLACEHOLDER_FILL_LIGHT: &str = "#E7E7E7";
/// `drawImagePlaceholder`'s box in the dark theme (`renderElement.ts:366`).
pub const IMAGE_PLACEHOLDER_FILL_DARK: &str = "#2E2E2E";

/// `drawImagePlaceholder`'s icon size (`renderElement.ts:369-374`):
/// `min(side, min(side × 0.4, 100))` where `side` is the shorter of
/// `width` and `height`, with `Math.min`'s NaN.
pub fn image_placeholder_size(width: f64, height: f64) -> f64 {
    let min_side = js::min(width, height);
    js::min(min_side, js::min(min_side * 0.4, 100.0))
}

/// `drawImagePlaceholder(element, context, theme)`
/// (`renderElement.ts:361-385`): `fillRect` of the element's box in grey
/// (a [`DisplayItem::FillRect`], which the canvas anti-aliases as a
/// rectangle), then upstream's image icon (or its broken-image icon for an
/// image whose status is `error`), centred, [`image_placeholder_size`]
/// square.
fn draw_image_placeholder(
    element: &Element,
    status: ImageStatus,
    theme: Theme,
) -> Vec<DisplayItem> {
    let b = &element.base;
    let fill = if theme == Theme::Dark {
        IMAGE_PLACEHOLDER_FILL_DARK
    } else {
        IMAGE_PLACEHOLDER_FILL_LIGHT
    };
    let size = image_placeholder_size(b.width, b.height);
    let id = if status == ImageStatus::Error {
        IMAGE_ERROR_PLACEHOLDER_ID
    } else {
        IMAGE_PLACEHOLDER_ID
    };
    vec![
        DisplayItem::FillRect {
            rect: Rect::new(0.0, 0.0, b.width, b.height),
            color: Color::new(fill),
        },
        DisplayItem::Image(ImageItem::new(
            id,
            Rect::new(
                b.width / 2.0 - size / 2.0,
                b.height / 2.0 - size / 2.0,
                size,
                size,
            ),
        )),
    ]
}

// ---------------------------------------------------------------------------
// Link icons

/// `DEFAULT_LINK_SIZE` (`components/hyperlink/helpers.ts:17`).
pub const DEFAULT_LINK_SIZE: f64 = 12.0;

/// `getLinkHandleFromCoords([x1, y1, x2, y2], angle, appState)`
/// (`components/hyperlink/helpers.ts:29-59`): where the link icon of an
/// element with that box sits, `[x, y, width, height]`: at the top right
/// corner, `DEFAULT_LINK_SIZE` scene units wide (less when zoomed in),
/// turned with the element.
pub fn get_link_handle_from_coords(bounds: [f64; 4], angle: f64, zoom: f64) -> [f64; 4] {
    let [x1, y1, x2, y2] = bounds;
    let size = DEFAULT_LINK_SIZE;
    let zoom = if zoom > 1.0 { zoom } else { 1.0 };
    let link_width = size / zoom;
    let link_height = size / zoom;
    let link_margin_y = size / zoom;
    let center_x = (x1 + x2) / 2.0;
    let center_y = (y1 + y2) / 2.0;
    let centering_offset = (size - 8.0) / (2.0 * zoom);
    let dashed_line_margin = 4.0 / zoom;

    // Same as `ne` resize handle
    let x = x2 + dashed_line_margin - centering_offset;
    let y = y1 - dashed_line_margin - link_margin_y + centering_offset;

    let rotated: Point<Global> = point_rotate_rads(
        point_from(x + link_width / 2.0, y + link_height / 2.0),
        point_from(center_x, center_y),
        Radians(angle),
    );
    [
        rotated.x - link_width / 2.0,
        rotated.y - link_height / 2.0,
        link_width,
        link_height,
    ]
}

/// `renderLinkIcon(element, context, appState, elementsMap, renderState)`
/// (`staticScene.ts:201-273`): the icon of an element with a link that is
/// not selected, at [`get_link_handle_from_coords`] of its box moved by
/// its offset, turned with it, at its render alpha. Upstream draws the icon
/// into a canvas of `width × dpr × zoom` by `height × dpr × zoom` pixels
/// (truncated), filled with the view background (white when there is none
/// or the canvas rejects it) under the icon, and draws that canvas onto the
/// icon's box; the port draws the same content through the same mapping,
/// clipped to that canvas. Upstream keeps the canvas while the zoom stays
/// the same; the port draws it afresh.
pub(crate) fn render_link_icon(
    element: &Element,
    app_state: &StaticCanvasAppState,
    elements_map: &ElementsMap<'_>,
    state: &ElementRenderState,
    config: &StaticCanvasRenderConfig,
    device_pixel_ratio: f64,
) -> Option<DisplayItem> {
    let link = element.base.link.as_deref().filter(|l| !l.is_empty())?;
    if app_state.selected_element_ids.contains(&element.base.id) {
        return None;
    }
    let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(element, elements_map, false);
    let [ox, oy] = state.offset;
    let angle = element.base.angle.0;
    let [x, y, width, height] =
        get_link_handle_from_coords([x1 + ox, y1 + oy, x2 + ox, y2 + oy], angle, app_state.zoom);
    let center_x = x + width / 2.0;
    let center_y = y + height / 2.0;

    let icon = if excali_core::link::is_element_link(link, &config.location_host) {
        ELEMENT_LINK_ID
    } else {
        EXTERNAL_LINK_ID
    };
    let resolution = device_pixel_ratio * app_state.zoom;
    // canvas.width = width × dpr × zoom: an unsigned long, truncated
    let pixels = |v: f64| {
        if v.is_finite() {
            v.trunc().max(0.0)
        } else {
            0.0
        }
    };
    let (canvas_width, canvas_height) = (pixels(width * resolution), pixels(height * resolution));
    if canvas_width == 0.0 || canvas_height == 0.0 {
        // drawImage of an empty canvas throws
        return None;
    }
    let background = match app_state.view_background_color.as_deref() {
        Some(color) if !color.is_empty() && Color::new(color).rgba().is_some() => color,
        _ => COLOR_WHITE,
    };
    let content = with_transform(
        Transform::scale(resolution, resolution),
        vec![
            // linkCanvasCacheContext.fillRect(0, 0, width, height)
            DisplayItem::FillRect {
                rect: Rect::new(0.0, 0.0, width, height),
                color: Color::new(background),
            },
            DisplayItem::Image(ImageItem::new(icon, Rect::new(0.0, 0.0, width, height))),
        ],
    );
    let blit = DisplayItem::Group(Group {
        transform: Transform::new(
            width / canvas_width,
            0.0,
            0.0,
            height / canvas_height,
            x - center_x,
            y - center_y,
        ),
        opacity: 1.0,
        clip: Some(Clip {
            path: Path::rect(0.0, 0.0, canvas_width, canvas_height),
            rule: FillRule::NonZero,
        }),
        items: vec![content],
    });
    Some(with_transform(
        Transform::translate(app_state.scroll_x + center_x, app_state.scroll_y + center_y),
        vec![with_transform(
            rotate(angle),
            vec![DisplayItem::Group(Group {
                opacity: state.opacity,
                ..Group::new(vec![blit])
            })],
        )],
    ))
}
