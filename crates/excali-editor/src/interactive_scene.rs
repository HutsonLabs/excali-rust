//! The interactive scene: `renderInteractiveScene`
//! (`packages/excalidraw/renderer/interactiveScene.ts:1614-2166`), the
//! canvas above the static scene where the editor draws selections,
//! handles and highlights, with `renderSnaps` (`renderer/renderSnaps.ts`)
//! and the helpers it calls from `renderer/helpers.ts`, as a
//! [`DisplayList`].
//!
//! The order of work is upstream's (`_renderInteractiveScene`):
//!
//! 1. The scroll is snapped to whole device pixels, as the static scene
//!    snaps it ([`snap_scroll_to_device_pixels`], `helpers.ts:47-63`), so
//!    the overlays sit on the content.
//! 2. `bootstrapCanvas` (`helpers.ts:73-127`) with no background: the
//!    device pixel ratio scale and a clear (a display list starts on a
//!    clear canvas); then the zoom scale.
//! 3. The point handles of the linear element being edited
//!    (`renderLinearPointHandles`, `:1056-1179`).
//! 4. The selection box (`renderSelectionElement`,
//!    `packages/element/src/renderElement.ts:936-961`), unless an image is
//!    being cropped.
//! 5. For a text being edited, or a lone selected text, that does not
//!    auto-resize: the reset handle beside it
//!    (`renderResetAutoResizeHandle`, `:1579-1612`, with
//!    `textAutoResizeHandle.ts`); for the text being edited, the dashed box
//!    around it (`renderTextBox`, `:1476-1498`).
//! 6. The binding highlight of `suggestedBinding` when binding is enabled
//!    (`renderBindingHighlightForBindableElement_simple`, `:258-523`,
//!    through [`binding_highlight`]).
//! 7. The frame being dragged into (`renderFrameHighlight`, `:975-1003`),
//!    the elements to highlight and the locked element or group under the
//!    pointer (`renderElementsBoxHighlight`, `:1005-1054`).
//! 8. The point handles again when the lone selected element is the one
//!    being edited (upstream draws them twice, and so does the port); the
//!    hovered point or elbow midpoint of the selected linear element
//!    (`renderLinearElementPointHighlight`, `:164-193`, and
//!    `renderElbowArrowMidPointHighlight`, `:151-162`), and for an arrow the
//!    focus point of each bound end (`renderFocusPointIndicator`,
//!    `:1236-1326`).
//! 9. When no element is being drawn and no line edited: the selected
//!    line's point handles, the selection borders of the selected elements,
//!    the selected groups and the group being edited
//!    (`renderSelectionBorder`, `:920-973`), and the transform handles of a
//!    lone element (`renderTransformHandles`, `:1328-1369`) or the dashed
//!    box and handles of several; the crop handles of an image being
//!    cropped (`renderCropHandles`, `:1371-1474`).
//! 10. The search matches (`appState.searchMatches`, `:2071-2103`): every
//!     matched line of each match's element, in the element's frame
//!     (rotated about its centre), the focused match in the stronger
//!     colour; a frame name's lines are in viewport px (divided by the
//!     zoom) and drawn only when `showOnCanvas` or focused.
//! 11. The snap lines ([`crate::snapping::render_snaps`], `renderSnaps.ts`).
//!
//! The drawing goes through a small model of the 2D context (`Canvas`):
//! upstream sets styles and transforms as it goes, sometimes outside a
//! `save()`/`restore()` pair (`renderFrameHighlight` leaves its stroke
//! style and line width to what follows), and the model keeps that state,
//! so every draw carries the style, matrix and alpha upstream's has.
//!
//! Left out, as the editor (single user) never draws them:
//! collaborators' selections and cursors (`renderRemoteCursors`,
//! `remoteSelectedElementIds`), the scrollbars (App asks for them only when its `renderScrollbars` prop is
//! set) and the text tool's hover affordance (`textToolHover`,
//! `renderTextToolHover`). The binding highlight is drawn settled: the
//! `_complex` variant behind the `COMPLEX_BINDINGS` feature flag (off by
//! default) is the one that animates, so `animationState` and `deltaTime`
//! change nothing here. Where upstream reads an element held by the app
//! state (`suggestedBinding.element`, `activeEmbeddable.element`) the port
//! goes by its id, looking the element up in the map, and draws no
//! binding highlight when the map does not have it. The ellipse outline of
//! the binding highlight is the canvas's `ellipse()` as
//! [`Path::ellipse`] writes it (cubic Béziers of the unit circle's arc,
//! mapped onto the ellipse).
//!
//! Tested draw for draw against upstream's own `renderInteractiveScene`
//! (`tests/interactive_scene.rs`, fixture from
//! `tools/goldens/interactive-scene.mjs`).

use std::collections::HashSet;

use excali_core::color::apply_dark_mode_filter;
use excali_core::constants::DEFAULT_TRANSFORM_HANDLE_SPACING;
use excali_core::element::{Element, ElementKind};
use excali_math::{js, point_from, point_rotate_rads, GlobalPoint, Radians};
use excali_scene::bounds::{
    get_common_bounds, get_common_bounds_in, get_element_absolute_coords, ElementsMap,
};
use excali_scene::display::{
    Clip, Color, Dash, DisplayItem, DisplayList, FillRule, Group, LineCap, Path, Rect, Stroke,
    Transform,
};
use excali_scene::frame::is_frame_like;
use excali_scene::shape::Theme;
use excali_scene::static_scene::snap_scroll_to_device_pixels;
use serde_json::{Map, Value};

use crate::binding::FOCUS_POINT_SIZE;
use crate::binding_highlight::{
    binding_highlight, HighlightAppState, HighlightOutline, SuggestedBinding,
};
use crate::collision::{hit_element_itself, HitTestArgs, HitTestCache};
use crate::geometry::{get_binding_gap, get_global_fixed_point_for_bindable_element};
use crate::linear_element_editor::{
    get_editor_mid_points, get_point_at_index_global_coordinates, get_points_global_coordinates,
    is_segment_too_short, POINT_HANDLE_SIZE,
};
use crate::snapping::{render_snaps, SnapCanvasCall, SnapLine, SnapLineDirection, SnapRenderState};
use crate::tools::PointerType;
use crate::transform_handles::{
    get_omit_sides_for_editor_interface, get_transform_handles, get_transform_handles_from_coords,
    has_bounding_box, EditorInterface, FormFactor, SelectedLinearElementState, TransformHandleType,
    TransformHandles,
};

type P = [f64; 2];

/// `DEFAULT_SELECTION_COLOR` (`renderer/helpers.ts:6`): the theme's
/// `--color-selection` when the container sets none.
pub const DEFAULT_SELECTION_COLOR: &str = "#6965db";

/// `FRAME_STYLE.strokeWidth` (`common/src/constants.ts:208`).
const FRAME_STROKE_WIDTH: f64 = 2.0;
/// `FRAME_STYLE.radius` (`common/src/constants.ts:214`).
const FRAME_RADIUS: f64 = 8.0;

/// `SEARCH_MATCH_COLOR` (`interactiveScene.ts:134-143`): per theme, the
/// focused match's colour and the others'.
const SEARCH_MATCH_COLOR: [(&str, &str); 2] = [
    ("rgba(255, 124, 0, 0.4)", "rgba(255, 226, 0, 0.4)"),
    ("rgba(250, 123, 53, 0.4)", "rgba(221, 181, 136, 0.4)"),
];

/// `TEXT_AUTO_RESIZE_HANDLE_GAP` (`textAutoResizeHandle.ts:13`).
const TEXT_AUTO_RESIZE_HANDLE_GAP: f64 = 12.0;
/// `TEXT_AUTO_RESIZE_HANDLE_LENGTH` (`textAutoResizeHandle.ts:14`).
const TEXT_AUTO_RESIZE_HANDLE_LENGTH: f64 = 16.0;
/// `MAX_HANDLE_HEIGHT_RATIO` (`textAutoResizeHandle.ts:18`).
const MAX_HANDLE_HEIGHT_RATIO: f64 = 0.8;

// ---------------------------------------------------------------------------
// The app state

/// One end of an arrow (`"start" | "end"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowEnd {
    Start,
    End,
}

impl ArrowEnd {
    fn parse(value: Option<&Value>) -> Option<ArrowEnd> {
        match value?.as_str()? {
            "start" => Some(ArrowEnd::Start),
            "end" => Some(ArrowEnd::End),
            _ => None,
        }
    }
}

/// What the renderer reads of `appState.selectedLinearElement`, a
/// `LinearElementEditor` (`linearElementEditor.ts:143-231`).
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedLinearElement {
    pub element_id: String,
    pub is_editing: bool,
    pub is_dragging: bool,
    /// `selectedPointsIndices`: `None` for `null`.
    pub selected_points_indices: Option<Vec<i64>>,
    /// `hoverPointIndex`: -1 for none.
    pub hover_point_index: i64,
    pub segment_mid_point_hovered_coords: Option<P>,
    pub hovered_focus_point_binding: Option<ArrowEnd>,
    pub dragged_focus_point_binding: Option<ArrowEnd>,
}

impl SelectedLinearElement {
    fn includes(&self, index: i64) -> bool {
        self.selected_points_indices
            .as_ref()
            .is_some_and(|list| list.contains(&index))
    }
}

/// A matched line of a [`SearchMatch`]: its box relative to the element's
/// top left (for a frame, its name's, in viewport px), and whether it is
/// drawn when its match is not focused.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchedLine {
    pub offset_x: f64,
    pub offset_y: f64,
    pub width: f64,
    pub height: f64,
    pub show_on_canvas: bool,
}

/// `SearchMatch` (`packages/excalidraw/types.ts:589-599`): a search hit in
/// a text or a frame's name, as the search menu writes it to
/// `appState.searchMatches.matches`.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchMatch {
    pub id: String,
    pub focus: bool,
    pub matched_lines: Vec<MatchedLine>,
}

/// What the interactive canvas reads of the app state
/// (`InteractiveCanvasAppState`, `packages/excalidraw/types.ts:223-259`).
#[derive(Clone, Debug, PartialEq)]
pub struct InteractiveCanvasAppState {
    pub zoom: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub theme: Theme,
    pub view_mode_enabled: bool,
    pub zen_mode_enabled: bool,
    /// The selection box being dragged (a `selection` element).
    pub selection_element: Option<Element>,
    pub is_cropping: bool,
    pub cropping_element_id: Option<String>,
    /// The text being edited, as the app state holds it.
    pub editing_text_element: Option<Element>,
    /// The ids `selectedElementIds` maps to a truthy value.
    pub selected_element_ids: HashSet<String>,
    /// `selectedGroupIds` in the object's order, with each value's
    /// truthiness.
    pub selected_group_ids: Vec<(String, bool)>,
    pub editing_group_id: Option<String>,
    pub selected_linear_element: Option<SelectedLinearElement>,
    /// The id of `multiElement`, the line being drawn point by point.
    pub multi_element: Option<String>,
    /// The id of `newElement`, the element being drawn.
    pub new_element: Option<String>,
    pub is_binding_enabled: bool,
    pub suggested_binding: Option<SuggestedBinding>,
    pub is_midpoint_snapping_enabled: bool,
    pub grid_mode_enabled: bool,
    /// `activeTool.type`.
    pub active_tool_type: String,
    pub current_item_arrow_type: String,
    pub frame_to_highlight: Option<Element>,
    pub elements_to_highlight: Option<Vec<Element>>,
    pub active_locked_id: Option<String>,
    /// The id of `activeEmbeddable.element` when its state is `"active"`.
    pub active_embeddable_id: Option<String>,
    pub is_rotating: bool,
    pub snap_lines: Vec<SnapLine>,
    /// `searchMatches.matches` (none for `null`).
    pub search_matches: Vec<SearchMatch>,
}

impl Default for InteractiveCanvasAppState {
    /// The default app state's values (`appState.ts`).
    fn default() -> Self {
        InteractiveCanvasAppState {
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            theme: Theme::Light,
            view_mode_enabled: false,
            zen_mode_enabled: false,
            selection_element: None,
            is_cropping: false,
            cropping_element_id: None,
            editing_text_element: None,
            selected_element_ids: HashSet::new(),
            selected_group_ids: Vec::new(),
            editing_group_id: None,
            selected_linear_element: None,
            multi_element: None,
            new_element: None,
            is_binding_enabled: true,
            suggested_binding: None,
            is_midpoint_snapping_enabled: true,
            grid_mode_enabled: false,
            active_tool_type: "selection".to_owned(),
            current_item_arrow_type: "round".to_owned(),
            frame_to_highlight: None,
            elements_to_highlight: None,
            active_locked_id: None,
            active_embeddable_id: None,
            is_rotating: false,
            snap_lines: Vec::new(),
            search_matches: Vec::new(),
        }
    }
}

/// JavaScript truthiness of a JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| v != 0.0 && !v.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn non_empty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn element(value: Option<&Value>) -> Option<Element> {
    Element::from_map(value?.as_object()?.clone()).ok()
}

/// An element the app state holds (`multiElement`, `newElement`,
/// `activeEmbeddable.element`): its id, or the id itself.
fn element_id(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Object(map) => non_empty_string(map.get("id")),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn point(value: Option<&Value>) -> Option<P> {
    let list = value?.as_array()?;
    Some([list.first()?.as_f64()?, list.get(1)?.as_f64()?])
}

fn snap_line(value: &Value) -> Option<SnapLine> {
    let points: Vec<P> = value
        .get("points")?
        .as_array()?
        .iter()
        .map(|p| point(Some(p)))
        .collect::<Option<_>>()?;
    let direction = || match value.get("direction").and_then(Value::as_str) {
        Some("horizontal") => Some(SnapLineDirection::Horizontal),
        Some("vertical") => Some(SnapLineDirection::Vertical),
        _ => None,
    };
    let pair = || -> Option<[P; 2]> { Some([*points.first()?, *points.get(1)?]) };
    match value.get("type")?.as_str()? {
        "points" => Some(SnapLine::Points {
            points: points.clone(),
        }),
        "pointer" => Some(SnapLine::Pointer {
            points: pair()?,
            direction: direction()?,
        }),
        "gap" => Some(SnapLine::Gap {
            direction: direction()?,
            points: pair()?,
        }),
        _ => None,
    }
}

fn search_match(value: &Value) -> Option<SearchMatch> {
    let number = |line: &Value, key: &str| line.get(key).and_then(Value::as_f64);
    Some(SearchMatch {
        id: value.get("id")?.as_str()?.to_owned(),
        focus: truthy(value.get("focus")),
        matched_lines: value
            .get("matchedLines")?
            .as_array()?
            .iter()
            .map(|line| {
                Some(MatchedLine {
                    offset_x: number(line, "offsetX")?,
                    offset_y: number(line, "offsetY")?,
                    width: number(line, "width")?,
                    height: number(line, "height")?,
                    show_on_canvas: truthy(line.get("showOnCanvas")),
                })
            })
            .collect::<Option<_>>()?,
    })
}

fn selected_linear_element(value: Option<&Value>) -> Option<SelectedLinearElement> {
    let v = value?.as_object()?;
    Some(SelectedLinearElement {
        element_id: v.get("elementId")?.as_str()?.to_owned(),
        is_editing: truthy(v.get("isEditing")),
        is_dragging: truthy(v.get("isDragging")),
        selected_points_indices: v
            .get("selectedPointsIndices")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_i64).collect()),
        hover_point_index: v
            .get("hoverPointIndex")
            .and_then(Value::as_i64)
            .unwrap_or(-1),
        segment_mid_point_hovered_coords: point(v.get("segmentMidPointHoveredCoords")),
        hovered_focus_point_binding: ArrowEnd::parse(v.get("hoveredFocusPointBinding")),
        dragged_focus_point_binding: ArrowEnd::parse(v.get("draggedFocusPointBinding")),
    })
}

impl InteractiveCanvasAppState {
    /// The interactive canvas's app state from the app state as upstream's
    /// JSON holds it (`excali_core::app_state::AppState::as_map`): missing
    /// fields take [`InteractiveCanvasAppState::default`]'s values, and
    /// elements the app state holds by reference are read whole (the
    /// selection box, the text being edited, the highlighted frame and
    /// elements) or by id.
    pub fn from_app_state(state: &Map<String, Value>) -> InteractiveCanvasAppState {
        let d = InteractiveCanvasAppState::default();
        let get = |key: &str| state.get(key);
        let number = |key: &str, default: f64| get(key).and_then(Value::as_f64).unwrap_or(default);
        let flag = |key: &str, default: bool| get(key).map_or(default, |v| truthy(Some(v)));
        let ids = |key: &str| -> Vec<(String, bool)> {
            get(key)
                .and_then(Value::as_object)
                .map(|map| {
                    map.iter()
                        .map(|(k, v)| (k.clone(), truthy(Some(v))))
                        .collect()
                })
                .unwrap_or_default()
        };
        let suggested_binding =
            get("suggestedBinding")
                .and_then(Value::as_object)
                .and_then(|binding| {
                    Some(SuggestedBinding {
                        element_id: element_id(binding.get("element"))?,
                        mid_point: point(binding.get("midPoint")),
                    })
                });
        let active_embeddable_id = get("activeEmbeddable")
            .and_then(Value::as_object)
            .filter(|a| a.get("state").and_then(Value::as_str) == Some("active"))
            .and_then(|a| element_id(a.get("element")));
        InteractiveCanvasAppState {
            zoom: get("zoom")
                .and_then(|z| z.get("value"))
                .and_then(Value::as_f64)
                .unwrap_or(d.zoom),
            scroll_x: number("scrollX", d.scroll_x),
            scroll_y: number("scrollY", d.scroll_y),
            theme: match get("theme").and_then(Value::as_str) {
                Some("dark") => Theme::Dark,
                _ => Theme::Light,
            },
            view_mode_enabled: flag("viewModeEnabled", d.view_mode_enabled),
            zen_mode_enabled: flag("zenModeEnabled", d.zen_mode_enabled),
            selection_element: element(get("selectionElement")),
            is_cropping: flag("isCropping", d.is_cropping),
            cropping_element_id: non_empty_string(get("croppingElementId")),
            editing_text_element: element(get("editingTextElement")),
            selected_element_ids: ids("selectedElementIds")
                .into_iter()
                .filter(|(_, v)| *v)
                .map(|(k, _)| k)
                .collect(),
            selected_group_ids: ids("selectedGroupIds"),
            editing_group_id: non_empty_string(get("editingGroupId")),
            selected_linear_element: selected_linear_element(get("selectedLinearElement")),
            multi_element: element_id(get("multiElement")),
            new_element: element_id(get("newElement")),
            is_binding_enabled: flag("isBindingEnabled", d.is_binding_enabled),
            suggested_binding,
            is_midpoint_snapping_enabled: flag(
                "isMidpointSnappingEnabled",
                d.is_midpoint_snapping_enabled,
            ),
            grid_mode_enabled: flag("gridModeEnabled", d.grid_mode_enabled),
            active_tool_type: get("activeTool")
                .and_then(|t| t.get("type"))
                .and_then(Value::as_str)
                .map_or(d.active_tool_type, str::to_owned),
            current_item_arrow_type: get("currentItemArrowType")
                .and_then(Value::as_str)
                .map_or(d.current_item_arrow_type, str::to_owned),
            frame_to_highlight: element(get("frameToHighlight")),
            elements_to_highlight: get("elementsToHighlight")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(|e| element(Some(e))).collect()),
            active_locked_id: non_empty_string(get("activeLockedId")),
            active_embeddable_id,
            is_rotating: flag("isRotating", d.is_rotating),
            snap_lines: get("snapLines")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(snap_line).collect())
                .unwrap_or_default(),
            search_matches: get("searchMatches")
                .and_then(|m| m.get("matches"))
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(search_match).collect())
                .unwrap_or_default(),
        }
    }

    fn selected_group_ids(&self) -> impl Iterator<Item = &str> {
        self.selected_group_ids
            .iter()
            .filter(|(_, selected)| *selected)
            .map(|(id, _)| id.as_str())
    }
}

/// `InteractiveSceneRenderConfig` (`scene/types.ts:100-114`, with the
/// `selectionColor` of `InteractiveCanvasRenderConfig`, `:66-78`): the
/// canvas and what to draw on it.
pub struct InteractiveScene<'a> {
    /// `canvas.width` and `canvas.height`, in device pixels.
    pub canvas_width: f64,
    pub canvas_height: f64,
    /// The device pixel ratio.
    pub scale: f64,
    /// The renderable elements (`RenderableElementsMap`).
    pub elements_map: &'a ElementsMap<'a>,
    /// The same elements in scene order, the order upstream's map
    /// iterates in (selection borders, group members).
    pub elements: &'a [&'a Element],
    /// Every non-deleted element.
    pub all_elements_map: &'a ElementsMap<'a>,
    /// The same elements in scene order.
    pub all_elements: &'a [&'a Element],
    /// The elements in the viewport, where the line being edited is found.
    pub visible_elements: &'a [&'a Element],
    pub selected_elements: &'a [&'a Element],
    pub app_state: &'a InteractiveCanvasAppState,
    /// `renderConfig.selectionColor`.
    pub selection_color: &'a str,
    pub editor_interface: EditorInterface,
    /// `app.lastPointerMoveCoords`, in scene coordinates.
    pub pointer: Option<P>,
    /// `shouldRotateWithDiscreteAngle(app.lastPointerMoveEvent)`: shift
    /// held.
    pub angle_locked: bool,
}

// ---------------------------------------------------------------------------
// The 2D context

/// The context's drawing state (`save()` copies it).
#[derive(Clone)]
struct State {
    /// The matrix, relative to the innermost clip's group.
    m: Transform,
    alpha: f64,
    fill: String,
    stroke: String,
    line_width: f64,
    line_cap: LineCap,
    dash: Vec<f64>,
    dash_offset: f64,
    /// How many clips are in effect.
    clips: usize,
}

/// An open group: its items so far, and for a clip's group the matrix and
/// path of the clip.
type Level = (Vec<DisplayItem>, Option<(Transform, Clip)>);

/// A small model of `CanvasRenderingContext2D` that turns upstream's calls
/// into display items: it keeps the state the canvas specification
/// defines (an assignment the canvas ignores is ignored) and the path, and
/// each draw becomes an item under the matrix and alpha in effect. A clip
/// opens a group with the clip's matrix and path, which the `restore()`
/// that drops it closes; matrices inside it are relative to it, built by
/// the same products the canvas makes.
struct Canvas {
    s: State,
    stack: Vec<State>,
    path: Path,
    /// The open groups: the root, then one per clip in effect.
    levels: Vec<Level>,
}

impl Canvas {
    fn new() -> Canvas {
        Canvas {
            s: State {
                m: Transform::IDENTITY,
                alpha: 1.0,
                fill: "#000000".to_owned(),
                stroke: "#000000".to_owned(),
                line_width: 1.0,
                line_cap: LineCap::Butt,
                dash: Vec::new(),
                dash_offset: 0.0,
                clips: 0,
            },
            stack: Vec::new(),
            path: Path::new(),
            levels: vec![(Vec::new(), None)],
        }
    }

    fn finish(mut self) -> DisplayList {
        while self.levels.len() > 1 {
            self.close_clip();
        }
        DisplayList::from_iter(
            self.levels
                .pop()
                .map(|(items, _)| items)
                .unwrap_or_default(),
        )
    }

    fn close_clip(&mut self) {
        if let Some((items, Some((transform, clip)))) = self.levels.pop() {
            let group = Group {
                transform,
                clip: Some(clip),
                ..Group::new(items)
            };
            if let Some((parent, _)) = self.levels.last_mut() {
                parent.push(DisplayItem::Group(group));
            }
        }
    }

    fn save(&mut self) {
        self.stack.push(self.s.clone());
    }

    fn restore(&mut self) {
        let Some(outer) = self.stack.pop() else {
            return;
        };
        while self.levels.len() - 1 > outer.clips {
            self.close_clip();
        }
        self.s = outer;
    }

    fn transform(&mut self, t: Transform) {
        if [t.a, t.b, t.c, t.d, t.e, t.f].iter().all(|v| v.is_finite()) {
            self.s.m = self.s.m.concat(&t);
        }
    }

    fn translate(&mut self, x: f64, y: f64) {
        self.transform(Transform::translate(x, y));
    }

    fn scale(&mut self, x: f64, y: f64) {
        self.transform(Transform::scale(x, y));
    }

    fn rotate(&mut self, angle: f64) {
        if angle.is_finite() {
            self.transform(Transform::rotate(angle));
        }
    }

    fn set_fill_style(&mut self, color: &str) {
        if Color::new(color).rgba().is_some() {
            self.s.fill = color.to_owned();
        }
    }

    fn set_stroke_style(&mut self, color: &str) {
        if Color::new(color).rgba().is_some() {
            self.s.stroke = color.to_owned();
        }
    }

    fn set_global_alpha(&mut self, alpha: f64) {
        if (0.0..=1.0).contains(&alpha) {
            self.s.alpha = alpha;
        }
    }

    fn set_line_width(&mut self, width: f64) {
        if width.is_finite() && width > 0.0 {
            self.s.line_width = width;
        }
    }

    fn set_line_dash(&mut self, list: &[f64]) {
        if list.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return;
        }
        self.s.dash = if list.len() % 2 == 1 {
            [list, list].concat()
        } else {
            list.to_vec()
        };
    }

    fn set_line_dash_offset(&mut self, offset: f64) {
        if offset.is_finite() {
            self.s.dash_offset = offset;
        }
    }

    fn begin_path(&mut self) {
        self.path = Path::new();
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.path.move_to(x, y);
    }

    fn line_to(&mut self, x: f64, y: f64) {
        self.path.line_to(x, y);
    }

    fn arc(&mut self, x: f64, y: f64, radius: f64, start: f64, end: f64) {
        self.path.arc(x, y, radius, start, end, false);
    }

    fn round_rect(&mut self, x: f64, y: f64, w: f64, h: f64, radius: f64) {
        self.path
            .commands
            .extend(Path::round_rect(x, y, w, h, radius).commands);
    }

    fn close_path(&mut self) {
        self.path.close();
    }

    /// A draw under the current matrix and alpha.
    fn draw(&mut self, item: DisplayItem) {
        let group = Group {
            transform: self.s.m,
            opacity: self.s.alpha,
            ..Group::new(vec![item])
        };
        if let Some((items, _)) = self.levels.last_mut() {
            items.push(DisplayItem::Group(group));
        }
    }

    fn stroke_of(&self) -> Stroke {
        Stroke::new(Color::new(self.s.stroke.as_str()), self.s.line_width)
            .with_cap(self.s.line_cap)
            .with_dash(Dash::new(&self.s.dash, self.s.dash_offset))
    }

    fn fill(&mut self) {
        let item = DisplayItem::Fill {
            path: self.path.clone(),
            color: Color::new(self.s.fill.as_str()),
            rule: FillRule::NonZero,
        };
        self.draw(item);
    }

    fn stroke(&mut self) {
        let item = DisplayItem::Stroke {
            path: self.path.clone(),
            stroke: self.stroke_of(),
        };
        self.draw(item);
    }

    fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if [x, y, w, h].iter().all(|v| v.is_finite()) {
            let item = DisplayItem::FillRect {
                rect: Rect::new(x, y, w, h),
                color: Color::new(self.s.fill.as_str()),
            };
            self.draw(item);
        }
    }

    fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if [x, y, w, h].iter().all(|v| v.is_finite()) {
            let item = DisplayItem::Stroke {
                path: Path::rect(x, y, w, h),
                stroke: self.stroke_of(),
            };
            self.draw(item);
        }
    }

    /// `clip()`: the path, under the current matrix, clips every draw until
    /// the `restore()` that drops it.
    fn clip(&mut self) {
        let clip = Clip {
            path: self.path.clone(),
            rule: FillRule::NonZero,
        };
        self.levels.push((Vec::new(), Some((self.s.m, clip))));
        self.s.m = Transform::IDENTITY;
        self.s.clips += 1;
    }
}

// ---------------------------------------------------------------------------
// Helpers (renderer/helpers.ts)

/// `getThemedColor(color, theme)` (`interactiveScene.ts:146-149`): a light
/// UI colour through the dark filter in the dark theme.
fn themed(color: &str, theme: Theme) -> String {
    apply_dark_mode_filter(color, theme == Theme::Dark)
}

/// `fillCircle(context, cx, cy, radius, stroke, fill)` (`helpers.ts:18-35`).
fn fill_circle(ctx: &mut Canvas, cx: f64, cy: f64, radius: f64, stroke: bool, fill: bool) {
    ctx.begin_path();
    ctx.arc(cx, cy, radius, 0.0, std::f64::consts::PI * 2.0);
    if fill {
        ctx.fill();
    }
    if stroke {
        ctx.stroke();
    }
}

/// `strokeRectWithRotation_simple(context, x, y, width, height, cx, cy,
/// angle, fill, radius)` (`helpers.ts:129-157`).
#[allow(clippy::too_many_arguments)]
fn stroke_rect_with_rotation(
    ctx: &mut Canvas,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    cx: f64,
    cy: f64,
    angle: f64,
    fill: bool,
    radius: f64,
) {
    ctx.save();
    ctx.translate(cx, cy);
    ctx.rotate(angle);
    if fill {
        ctx.fill_rect(x - cx, y - cy, width, height);
    }
    if radius != 0.0 && !radius.is_nan() {
        ctx.begin_path();
        ctx.round_rect(x - cx, y - cy, width, height, radius);
        ctx.stroke();
        ctx.close_path();
    } else {
        ctx.stroke_rect(x - cx, y - cy, width, height);
    }
    ctx.restore();
}

// ---------------------------------------------------------------------------
// Groups (packages/element/src/groups.ts)

/// `isSelectedViaGroup(appState, element)` (`groups.ts:215-232`).
fn is_selected_via_group(app_state: &InteractiveCanvasAppState, element: &Element) -> bool {
    element
        .base
        .group_ids
        .iter()
        .filter(|g| app_state.editing_group_id.as_deref() != Some(g.as_str()))
        .any(|g| {
            app_state
                .selected_group_ids
                .iter()
                .any(|(id, selected)| id == g && *selected)
        })
}

/// Whether a key is an array index, which a JavaScript object enumerates
/// first, in numeric order.
fn is_array_index(key: &str) -> bool {
    key.parse::<u32>()
        .is_ok_and(|n| n != u32::MAX && n.to_string() == key)
}

/// `selectGroupsFromGivenElements(elements, appState)`
/// (`groups.ts:241-270`, with `selectGroup`, `:24-64`): the outermost
/// group (below the group being edited) of each element, selected when the
/// given elements hold two or more of it; `Object.entries` order.
fn select_groups_from_given_elements(
    elements: &[&Element],
    app_state: &InteractiveCanvasAppState,
) -> Vec<(String, bool)> {
    let mut selected: Vec<(String, bool)> = Vec::new();
    let mut editing_group_id = app_state.editing_group_id.clone();
    let set = |list: &mut Vec<(String, bool)>, id: &str, value: bool| match list
        .iter_mut()
        .find(|(k, _)| k == id)
    {
        Some(entry) => entry.1 = value,
        None => list.push((id.to_owned(), value)),
    };
    for element in elements {
        let mut group_ids: &[String] = &element.base.group_ids;
        if let Some(editing) = app_state.editing_group_id.as_deref() {
            if let Some(i) = group_ids.iter().position(|g| g == editing) {
                group_ids = &group_ids[..i];
            }
        }
        let Some(group_id) = group_ids.last() else {
            continue;
        };
        let count = elements
            .iter()
            .filter(|e| e.base.group_ids.contains(group_id))
            .count();
        if count < 2 {
            let is_selected = selected.iter().any(|(k, v)| k == group_id && *v);
            if is_selected || editing_group_id.as_deref() == Some(group_id.as_str()) {
                set(&mut selected, group_id, false);
                editing_group_id = None;
            }
        } else {
            set(&mut selected, group_id, true);
        }
    }
    // array-index keys first, ascending, as an object enumerates them
    let (mut indices, rest): (Vec<_>, Vec<_>) =
        selected.into_iter().partition(|(k, _)| is_array_index(k));
    indices.sort_by_key(|(k, _)| k.parse::<u32>().unwrap_or(0));
    indices.extend(rest);
    indices
}

/// `getElementsInGroup(elements, groupId)` (`groups.ts:289-304`).
fn elements_in_group<'a>(elements: &[&'a Element], group_id: &str) -> Vec<&'a Element> {
    elements
        .iter()
        .copied()
        .filter(|e| e.base.group_ids.iter().any(|g| g == group_id))
        .collect()
}

// ---------------------------------------------------------------------------
// Linear elements

fn is_elbow_arrow(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Arrow(a) if a.elbowed)
}

fn points_of(element: &Element) -> &[[f64; 2]] {
    element.kind.points().unwrap_or(&[])
}

/// `pointsEqual(a, b, tolerance)` (`math/src/point.ts:108-115`).
fn points_equal(a: P, b: P, tolerance: f64) -> bool {
    (a[0] - b[0]).abs() < tolerance && (a[1] - b[1]).abs() < tolerance
}

/// `highlightPoint(point, context, appState)` (`interactiveScene.ts:195-217`).
fn highlight_point(ctx: &mut Canvas, point: P, app_state: &InteractiveCanvasAppState) {
    ctx.save();
    ctx.translate(app_state.scroll_x, app_state.scroll_y);
    ctx.set_fill_style(&themed("rgba(105, 101, 219, 0.4)", app_state.theme));
    fill_circle(
        ctx,
        point[0],
        point[1],
        POINT_HANDLE_SIZE / app_state.zoom,
        false,
        true,
    );
    ctx.restore();
}

/// `renderSingleLinearPoint(context, appState, point, radius, isSelected,
/// isPhantomPoint, isOverlappingPoint)` (`interactiveScene.ts:219-256`).
fn render_single_linear_point(
    ctx: &mut Canvas,
    app_state: &InteractiveCanvasAppState,
    point: P,
    radius: f64,
    is_selected: bool,
    is_phantom_point: bool,
    is_overlapping_point: bool,
) {
    let theme = app_state.theme;
    ctx.set_stroke_style(&themed("#5e5ad8", theme));
    ctx.set_line_dash(&[]);
    ctx.set_fill_style(&themed("rgba(255, 255, 255, 0.9)", theme));
    if is_selected {
        ctx.set_fill_style(&themed("rgba(134, 131, 226, 0.9)", theme));
    } else if is_phantom_point {
        ctx.set_fill_style(&themed("rgba(177, 151, 252, 0.7)", theme));
    }
    let editing = app_state
        .selected_linear_element
        .as_ref()
        .is_some_and(|l| l.is_editing);
    let r = if is_overlapping_point {
        radius * if editing { 1.5 } else { 2.0 }
    } else {
        radius
    };
    fill_circle(
        ctx,
        point[0],
        point[1],
        r / app_state.zoom,
        !is_phantom_point,
        !is_overlapping_point || is_selected,
    );
}

/// `renderLinearPointHandles(context, appState, element, elementsMap)`
/// (`interactiveScene.ts:1056-1179`): a handle on each point (an elbow
/// arrow's ends only), larger while editing, filled when selected, a ring
/// when it overlaps the point before it; then the segment midpoints: an
/// elbow arrow's where the segment is long enough (phantom unless the
/// segment is fixed), a line's while editing or when it has two points.
fn linear_point_handles(
    ctx: &mut Canvas,
    app_state: &InteractiveCanvasAppState,
    element: &Element,
    elements_map: &ElementsMap<'_>,
) {
    let Some(linear) = app_state.selected_linear_element.as_ref() else {
        return;
    };
    let zoom = app_state.zoom;
    ctx.save();
    ctx.translate(app_state.scroll_x, app_state.scroll_y);
    ctx.set_line_width(1.0 / zoom);
    let points = get_points_global_coordinates(element, elements_map);
    let radius = if linear.is_editing {
        POINT_HANDLE_SIZE
    } else {
        POINT_HANDLE_SIZE / 2.0
    };
    let elbow = is_elbow_arrow(element);
    let (is_line, polygon) = match &element.kind {
        ElementKind::Line(l) => (true, l.polygon),
        _ => (false, false),
    };
    let last = points.len().saturating_sub(1);
    for (idx, &point) in points.iter().enumerate() {
        if elbow && idx != 0 && idx != last {
            continue;
        }
        let is_overlapping_point = idx > 0
            && (idx != last || !is_line || !polygon)
            && points_equal(
                point,
                if idx == last {
                    points[0]
                } else {
                    points[idx - 1]
                },
                2.0 / zoom,
            );
        let mut is_selected = linear.is_editing && linear.includes(idx as i64);
        // when element is a polygon, highlight the last point as well if
        // first point is selected since they overlap and the last point
        // tends to be rendered on top
        if is_line
            && polygon
            && !is_selected
            && idx + 1 == points_of(element).len()
            && linear.is_editing
            && linear.includes(0)
        {
            is_selected = true;
        }
        render_single_linear_point(
            ctx,
            app_state,
            point,
            radius,
            is_selected,
            false,
            is_overlapping_point,
        );
    }

    // Rendering segment mid points
    if elbow {
        let fixed_segments: Vec<f64> = match &element.kind {
            ElementKind::Arrow(a) => a
                .fixed_segments
                .clone()
                .flatten()
                .unwrap_or_default()
                .iter()
                .map(|s| s.index)
                .collect(),
            _ => Vec::new(),
        };
        for idx in 0..points.len().saturating_sub(1) {
            let (p, next) = (points[idx], points[idx + 1]);
            if !is_segment_too_short(element, next, p, idx, zoom, elements_map) {
                render_single_linear_point(
                    ctx,
                    app_state,
                    [(p[0] + next[0]) / 2.0, (p[1] + next[1]) / 2.0],
                    POINT_HANDLE_SIZE / 2.0,
                    false,
                    !fixed_segments.contains(&((idx + 1) as f64)),
                    false,
                );
            }
        }
    } else {
        let mid_points = get_editor_mid_points(element, elements_map, linear.is_editing, zoom);
        for mid_point in mid_points.into_iter().flatten() {
            if linear.is_editing || points.len() == 2 {
                render_single_linear_point(
                    ctx,
                    app_state,
                    mid_point,
                    POINT_HANDLE_SIZE / 2.0,
                    false,
                    true,
                    false,
                );
            }
        }
    }
    ctx.restore();
}

/// `renderLinearElementPointHighlight(context, appState, elementsMap)`
/// (`interactiveScene.ts:164-193`): the hovered point, unless it is
/// selected in the editor or a point is being dragged.
fn render_linear_element_point_highlight(
    ctx: &mut Canvas,
    app_state: &InteractiveCanvasAppState,
    elements_map: &ElementsMap<'_>,
) {
    let Some(linear) = app_state.selected_linear_element.as_ref() else {
        return;
    };
    let hover = linear.hover_point_index;
    if linear.is_editing && linear.includes(hover) {
        return;
    }
    if linear.is_dragging {
        return;
    }
    let Some(element) = elements_map.get(&linear.element_id) else {
        return;
    };
    let point = get_point_at_index_global_coordinates(element, hover as isize, elements_map);
    highlight_point(ctx, point, app_state);
}

/// `isFocusPointVisible(focusPoint, arrow, bindableElement, elementsMap,
/// appState, startOrEnd)` (`packages/element/src/arrows/focus.ts:37-100`),
/// which the port did not have yet:
/// a two-point, non-elbow arrow with binding enabled, the focus point off
/// the arrow's end by more than 1.5 focus points and on the element (within
/// the binding gap).
fn is_focus_point_visible(
    focus_point: P,
    arrow: &Element,
    bindable: &Element,
    elements_map: &ElementsMap<'_>,
    app_state: &InteractiveCanvasAppState,
    end: ArrowEnd,
) -> bool {
    let points = points_of(arrow);
    if is_elbow_arrow(arrow) || !app_state.is_binding_enabled || points.len() != 2 {
        return false;
    }
    let distance = |a: P, b: P| js::hypot(b[0] - a[0], b[1] - a[1]);
    let min = FOCUS_POINT_SIZE * 1.5 / app_state.zoom;
    let start_bound_here = match &arrow.kind {
        ElementKind::Arrow(a) => a
            .linear
            .start_binding
            .as_ref()
            .is_some_and(|b| b.element_id == bindable.base.id),
        _ => false,
    };
    let associated = if start_bound_here {
        0
    } else {
        points.len() as isize - 1
    };
    let associated_point = get_point_at_index_global_coordinates(arrow, associated, elements_map);
    if distance(focus_point, associated_point) < min {
        return false;
    }
    let index = match end {
        ArrowEnd::End => points.len() as isize - 1,
        ArrowEnd::Start => 0,
    };
    let arrow_point = get_point_at_index_global_coordinates(arrow, index, elements_map);
    distance(focus_point, arrow_point) >= min
        && hit_element_itself(
            &mut HitTestCache::new(),
            &HitTestArgs {
                point: focus_point,
                element: bindable,
                threshold: get_binding_gap(bindable),
                elements_map,
                frame_name_bound: None,
                override_should_test_inside: true,
            },
        )
}

/// `renderFocusPointIndicator({ arrow, appState, type, context,
/// elementsMap })` (`interactiveScene.ts:1181-1326`): where the bound end
/// aims inside its element, joined to the end by a dashed line, filled
/// when hovered.
fn render_focus_point_indicator(
    ctx: &mut Canvas,
    arrow: &Element,
    app_state: &InteractiveCanvasAppState,
    end: ArrowEnd,
    elements_map: &ElementsMap<'_>,
) {
    let ElementKind::Arrow(fields) = &arrow.kind else {
        return;
    };
    let binding = match end {
        ArrowEnd::Start => fields.linear.start_binding.as_ref(),
        ArrowEnd::End => fields.linear.end_binding.as_ref(),
    };
    let Some(binding) = binding.filter(|b| !b.element_id.is_empty()) else {
        return;
    };
    let Some(bindable) = elements_map.get(&binding.element_id) else {
        return;
    };
    if !bindable.is_bindable() || bindable.base.is_deleted {
        return;
    }
    let focus_point = get_global_fixed_point_for_bindable_element(binding.fixed_point, bindable);
    if !is_focus_point_visible(focus_point, arrow, bindable, elements_map, app_state, end) {
        return;
    }
    let linear = app_state.selected_linear_element.as_ref();
    let is_dragging = linear.is_some_and(|l| l.is_dragging);
    let point_index = match end {
        ArrowEnd::Start => 0,
        ArrowEnd::End => points_of(arrow).len() as i64 - 1,
    };
    let point_selected = linear.is_some_and(|l| l.includes(point_index));
    let hovered = linear.is_some_and(|l| l.hovered_focus_point_binding == Some(end));

    // render focus point highlight
    if hovered && linear.is_some_and(|l| l.dragged_focus_point_binding.is_none()) {
        highlight_point(ctx, focus_point, app_state);
    }

    // render focus point
    if !(point_selected && is_dragging) {
        let theme = app_state.theme;
        let zoom = app_state.zoom;
        let arrow_point =
            get_point_at_index_global_coordinates(arrow, point_index as isize, elements_map);
        // renderFocusPointConnectionLine
        ctx.save();
        ctx.translate(app_state.scroll_x, app_state.scroll_y);
        ctx.set_stroke_style(&themed("rgba(134, 131, 226, 0.6)", theme));
        ctx.set_line_width(1.0 / zoom);
        ctx.set_line_dash(&[4.0 / zoom, 4.0 / zoom]);
        ctx.begin_path();
        ctx.move_to(arrow_point[0], arrow_point[1]);
        ctx.line_to(focus_point[0], focus_point[1]);
        ctx.stroke();
        ctx.restore();
        // renderFocusPointCicle
        ctx.save();
        ctx.translate(app_state.scroll_x, app_state.scroll_y);
        ctx.set_stroke_style(&themed("rgba(134, 131, 226, 0.6)", theme));
        ctx.set_line_width(1.0 / zoom);
        ctx.set_line_dash(&[]);
        ctx.set_fill_style(&themed(
            if hovered {
                "rgba(134, 131, 226, 0.9)"
            } else {
                "rgba(255, 255, 255, 0.9)"
            },
            theme,
        ));
        fill_circle(
            ctx,
            focus_point[0],
            focus_point[1],
            FOCUS_POINT_SIZE / 1.5 / zoom,
            true,
            true,
        );
        ctx.restore();
    }
}

// ---------------------------------------------------------------------------
// Highlights and borders

/// `renderBindingHighlightForBindableElement(app, context,
/// suggestedBinding, allElementsMap, appState)` without the feature flag
/// (`interactiveScene.ts:859-904`): the scroll, then
/// `renderBindingHighlightForBindableElement_simple` (`:258-523`) as
/// [`binding_highlight`] lays it out.
fn render_binding_highlight(
    ctx: &mut Canvas,
    suggested: &SuggestedBinding,
    scene: &InteractiveScene<'_>,
    app_state: &InteractiveCanvasAppState,
) {
    let elements_map = scene.all_elements_map;
    let arrow_is_elbow = app_state
        .selected_linear_element
        .as_ref()
        .and_then(|l| elements_map.get(&l.element_id))
        .is_some_and(is_elbow_arrow);
    let elbow = arrow_is_elbow
        || (app_state.active_tool_type == "arrow" && app_state.current_item_arrow_type == "elbow");
    let state = HighlightAppState {
        zoom: app_state.zoom,
        theme: app_state.theme,
        is_midpoint_snapping_enabled: app_state.is_midpoint_snapping_enabled,
        grid_mode_enabled: app_state.grid_mode_enabled,
        elbow,
    };
    let Some(h) = binding_highlight(
        suggested,
        elements_map,
        &state,
        scene.pointer,
        scene.angle_locked,
    ) else {
        return;
    };
    ctx.save();
    ctx.translate(app_state.scroll_x, app_state.scroll_y);
    if let Some(f) = h.frame_clip {
        ctx.translate(f.x, f.y);
        ctx.begin_path();
        ctx.round_rect(-1.0, -1.0, f.width + 1.0, f.height + 1.0, f.radius);
        ctx.clip();
        ctx.translate(-f.x, -f.y);
    }
    ctx.save();
    match &h.outline {
        HighlightOutline::Frame {
            x,
            y,
            width,
            height,
            radius,
        } => {
            ctx.translate(*x, *y);
            ctx.set_line_width(h.line_width);
            ctx.set_stroke_style(&h.color);
            ctx.begin_path();
            ctx.round_rect(0.0, 0.0, *width, *height, *radius);
            ctx.stroke();
            ctx.close_path();
        }
        HighlightOutline::Ellipse {
            center,
            angle,
            x,
            y,
            width,
            height,
        } => {
            ctx.translate(center[0], center[1]);
            ctx.rotate(*angle);
            ctx.translate(-center[0], -center[1]);
            ctx.translate(*x, *y);
            ctx.set_line_width(h.line_width);
            ctx.set_stroke_style(&h.color);
            ctx.begin_path();
            ctx.path.ellipse(
                width / 2.0,
                height / 2.0,
                width / 2.0,
                height / 2.0,
                0.0,
                0.0,
                2.0 * std::f64::consts::PI,
                false,
            );
            ctx.close_path();
            ctx.stroke();
        }
        HighlightOutline::Outline {
            center,
            angle,
            x,
            y,
            sides,
            corners,
        } => {
            ctx.translate(center[0], center[1]);
            ctx.rotate(*angle);
            ctx.translate(-center[0], -center[1]);
            ctx.translate(*x, *y);
            ctx.set_line_width(h.line_width);
            ctx.set_stroke_style(&h.color);
            // Draw each line segment individually
            for [a, b] in sides {
                ctx.begin_path();
                ctx.move_to(a[0] - x, a[1] - y);
                ctx.line_to(b[0] - x, b[1] - y);
                ctx.stroke();
            }
            // Draw each curve individually (for rounded corners)
            for [start, c1, c2, end] in corners {
                ctx.begin_path();
                ctx.move_to(start[0] - x, start[1] - y);
                ctx.path.cubic_to(
                    c1[0] - x,
                    c1[1] - y,
                    c2[0] - x,
                    c2[1] - y,
                    end[0] - x,
                    end[1] - y,
                );
                ctx.stroke();
            }
        }
    }
    ctx.restore();
    // Draw midpoint indicators
    if h.draws_midpoints {
        ctx.save();
        ctx.set_fill_style(h.midpoint_color);
        for m in &h.midpoints {
            ctx.begin_path();
            ctx.arc(
                m[0],
                m[1],
                h.midpoint_radius,
                0.0,
                2.0 * std::f64::consts::PI,
            );
            ctx.fill();
        }
        if let Some(m) = h.highlighted_midpoint {
            ctx.set_fill_style(&h.color);
            ctx.begin_path();
            ctx.arc(
                m[0],
                m[1],
                h.midpoint_radius,
                0.0,
                2.0 * std::f64::consts::PI,
            );
            ctx.fill();
        }
        ctx.restore();
    }
    ctx.restore();
}

/// `ElementSelectionBorder` (`interactiveScene.ts:906-918`).
struct SelectionBorder {
    angle: f64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    selection_colors: Vec<String>,
    dashed: bool,
    cx: f64,
    cy: f64,
    active_embeddable: bool,
    padding: Option<f64>,
}

/// A border around the bounds of `elements`, unrotated
/// (`getSelectionFromElements`, `addSelectionForGroupId`).
fn border_of_bounds(
    [x1, y1, x2, y2]: [f64; 4],
    selection_colors: Vec<String>,
    dashed: bool,
) -> SelectionBorder {
    SelectionBorder {
        angle: 0.0,
        x1,
        y1,
        x2,
        y2,
        selection_colors,
        dashed,
        cx: x1 + (x2 - x1) / 2.0,
        cy: y1 + (y2 - y1) / 2.0,
        active_embeddable: false,
        padding: None,
    }
}

/// `renderSelectionBorder(context, appState, elementProperties)`
/// (`interactiveScene.ts:920-973`): the box around an element or group,
/// padded by 4 / zoom (or `padding`), once per colour; dashed borders
/// interleave their colours.
fn render_selection_border(
    ctx: &mut Canvas,
    app_state: &InteractiveCanvasAppState,
    border: &SelectionBorder,
) {
    let zoom = app_state.zoom;
    let element_width = border.x2 - border.x1;
    let element_height = border.y2 - border.y1;
    let padding = border
        .padding
        .unwrap_or(DEFAULT_TRANSFORM_HANDLE_SPACING * 2.0);
    let line_padding = padding / zoom;
    let line_width = 8.0 / zoom;
    let space_width = 4.0 / zoom;

    ctx.save();
    ctx.translate(app_state.scroll_x, app_state.scroll_y);
    ctx.set_line_width(if border.active_embeddable { 4.0 } else { 1.0 } / zoom);
    let count = border.selection_colors.len();
    for (index, color) in border.selection_colors.iter().enumerate() {
        ctx.set_stroke_style(color);
        if border.dashed {
            ctx.set_line_dash(&[
                line_width,
                space_width + (line_width + space_width) * (count as f64 - 1.0),
            ]);
        }
        ctx.set_line_dash_offset((line_width + space_width) * index as f64);
        stroke_rect_with_rotation(
            ctx,
            border.x1 - line_padding,
            border.y1 - line_padding,
            element_width + line_padding * 2.0,
            element_height + line_padding * 2.0,
            border.cx,
            border.cy,
            border.angle,
            false,
            0.0,
        );
    }
    ctx.restore();
}

/// `renderFrameHighlight(context, appState, frame, elementsMap)`
/// (`interactiveScene.ts:975-1003`). The stroke style and line width are
/// set outside its `save()`, so they stay for what follows.
fn render_frame_highlight(
    ctx: &mut Canvas,
    app_state: &InteractiveCanvasAppState,
    frame: &Element,
    elements_map: &ElementsMap<'_>,
) {
    let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(frame, elements_map, false);
    let width = x2 - x1;
    let height = y2 - y1;
    ctx.set_stroke_style(&themed("rgb(0,118,255)", app_state.theme));
    ctx.set_line_width(FRAME_STROKE_WIDTH / app_state.zoom);
    ctx.save();
    ctx.translate(app_state.scroll_x, app_state.scroll_y);
    stroke_rect_with_rotation(
        ctx,
        x1,
        y1,
        width,
        height,
        x1 + width / 2.0,
        y1 + height / 2.0,
        frame.base.angle.0,
        false,
        FRAME_RADIUS / app_state.zoom,
    );
    ctx.restore();
}

/// `renderElementsBoxHighlight(context, appState, elements, config)`
/// (`interactiveScene.ts:1005-1054`): a border around each whole group the
/// elements select, then around each ungrouped element.
fn render_elements_box_highlight(
    ctx: &mut Canvas,
    app_state: &InteractiveCanvasAppState,
    elements: &[&Element],
    colors: Option<Vec<String>>,
    dashed: bool,
) {
    let colors = colors.unwrap_or_else(|| vec![themed("rgb(0,118,255)", app_state.theme)]);
    let individual: Vec<&Element> = elements
        .iter()
        .copied()
        .filter(|e| e.base.group_ids.is_empty())
        .collect();
    let in_groups: Vec<&Element> = elements
        .iter()
        .copied()
        .filter(|e| !e.base.group_ids.is_empty())
        .collect();
    let mut borders: Vec<SelectionBorder> =
        select_groups_from_given_elements(&in_groups, app_state)
            .into_iter()
            .filter(|(_, selected)| *selected)
            .map(|(group_id, _)| {
                let group = elements_in_group(elements, &group_id);
                border_of_bounds(get_common_bounds(&group), colors.clone(), dashed)
            })
            .collect();
    borders.extend(
        individual
            .iter()
            .map(|e| border_of_bounds(get_common_bounds(&[e]), colors.clone(), dashed)),
    );
    for border in &borders {
        render_selection_border(ctx, app_state, border);
    }
}

/// `renderTransformHandles(context, renderConfig, appState,
/// transformHandles, angle)` (`interactiveScene.ts:1328-1369`): a circle
/// for the rotation handle, a rounded square for the others, in
/// `Object.keys` order.
fn render_transform_handles(
    ctx: &mut Canvas,
    selection_color: &str,
    app_state: &InteractiveCanvasAppState,
    handles: &TransformHandles,
) {
    for (kind, [x, y, width, height]) in handles.iter() {
        ctx.save();
        ctx.set_line_width(1.0 / app_state.zoom);
        if !selection_color.is_empty() {
            ctx.set_stroke_style(selection_color);
        }
        if kind == TransformHandleType::Rotation {
            fill_circle(
                ctx,
                x + width / 2.0,
                y + height / 2.0,
                width / 2.0,
                true,
                true,
            );
        } else {
            // prefer round corners if roundRect API is available
            ctx.begin_path();
            ctx.round_rect(x, y, width, height, 2.0 / app_state.zoom);
            ctx.fill();
            ctx.stroke();
        }
        ctx.restore();
    }
}

/// `renderCropHandles(context, renderConfig, appState, croppingElement,
/// elementsMap)` (`interactiveScene.ts:1371-1474`): an L at each corner of
/// the image, 3 / zoom wide and up to 20 / zoom long, turned with it.
fn render_crop_handles(
    ctx: &mut Canvas,
    selection_color: &str,
    app_state: &InteractiveCanvasAppState,
    cropping_element: &Element,
    elements_map: &ElementsMap<'_>,
) {
    let [x1, y1, _, _, cx, cy] = get_element_absolute_coords(cropping_element, elements_map, false);
    const LINE_WIDTH: f64 = 3.0;
    const LINE_LENGTH: f64 = 20.0;
    let zoom = app_state.zoom;
    let zoomed_line_width = LINE_WIDTH / zoom;
    let half = zoomed_line_width / 2.0;
    let half_width = cx - x1 + zoomed_line_width;
    let half_height = cy - y1 + zoomed_line_width;
    let horizontal = js::min(LINE_LENGTH / zoom, half_width);
    let vertical = js::min(LINE_LENGTH / zoom, half_height);

    ctx.save();
    ctx.set_fill_style(selection_color);
    ctx.set_stroke_style(selection_color);
    ctx.set_line_width(zoomed_line_width);
    let handles: [[P; 5]; 4] = [
        [
            // x, y
            [-half_width, -half_height],
            // horizontal line: first start and to
            [0.0, half],
            [horizontal, half],
            // vertical line: second start and to
            [half, 0.0],
            [half, vertical],
        ],
        [
            [half_width - half, -half_height],
            [half, half],
            [-horizontal + half, half],
            [0.0, 0.0],
            [0.0, vertical],
        ],
        [
            [-half_width, half_height],
            [0.0, -half],
            [horizontal, -half],
            [half, 0.0],
            [half, -vertical],
        ],
        [
            [half_width - half, half_height],
            [half, -half],
            [-horizontal + half, -half],
            [0.0, 0.0],
            [0.0, -vertical],
        ],
    ];
    for [[x, y], [x1s, y1s], [x1t, y1t], [x2s, y2s], [x2t, y2t]] in handles {
        ctx.save();
        ctx.translate(cx, cy);
        ctx.rotate(cropping_element.base.angle.0);
        ctx.begin_path();
        ctx.move_to(x + x1s, y + y1s);
        ctx.line_to(x + x1t, y + y1t);
        ctx.stroke();
        ctx.begin_path();
        ctx.move_to(x + x2s, y + y2s);
        ctx.line_to(x + x2t, y + y2t);
        ctx.stroke();
        ctx.restore();
    }
    ctx.restore();
}

// ---------------------------------------------------------------------------
// Text

/// `getTextBoxPadding(zoom)` (`textAutoResizeHandle.ts:20-21`).
pub fn get_text_box_padding(zoom: f64) -> f64 {
    DEFAULT_TRANSFORM_HANDLE_SPACING * 2.0 / zoom
}

/// The auto-resize handle beside a fixed-width text
/// (`getTextAutoResizeHandle`, `textAutoResizeHandle.ts:23-65`): the
/// centre and the two ends of the vertical bar, turned with the text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextAutoResizeHandle {
    pub center: P,
    pub start: P,
    pub end: P,
}

/// `getTextAutoResizeHandle(textElement, zoomValue, formFactor)`: `None`
/// off the desktop, or when the bar would take more than 80% of the text's
/// padded height on screen.
pub fn get_text_auto_resize_handle(
    text: &Element,
    zoom: f64,
    form_factor: FormFactor,
) -> Option<TextAutoResizeHandle> {
    let b = &text.base;
    let padding = get_text_box_padding(zoom);
    if form_factor != FormFactor::Desktop
        || TEXT_AUTO_RESIZE_HANDLE_LENGTH
            > (b.height + padding * 2.0) * zoom * MAX_HANDLE_HEIGHT_RATIO
    {
        return None;
    }
    let gap = TEXT_AUTO_RESIZE_HANDLE_GAP / zoom;
    let length = TEXT_AUTO_RESIZE_HANDLE_LENGTH / zoom;
    let gp = |p: P| -> GlobalPoint { point_from(p[0], p[1]) };
    let xy = |p: GlobalPoint| [p.x, p.y];
    let angle = Radians(b.angle.0);
    let center = [b.x + b.width / 2.0, b.y + b.height / 2.0];
    let handle_center = xy(point_rotate_rads(
        gp([center[0] + b.width / 2.0 + padding + gap, center[1]]),
        gp(center),
        angle,
    ));
    Some(TextAutoResizeHandle {
        center: handle_center,
        start: xy(point_rotate_rads(
            gp([handle_center[0], handle_center[1] - length / 2.0]),
            gp(handle_center),
            angle,
        )),
        end: xy(point_rotate_rads(
            gp([handle_center[0], handle_center[1] + length / 2.0]),
            gp(handle_center),
            angle,
        )),
    })
}

/// `renderTextBox(text, context, appState, selectionColor)`
/// (`interactiveScene.ts:1476-1498`): the dashed box around a text being
/// edited, half transparent, turned with it.
fn render_text_box(
    ctx: &mut Canvas,
    text: &Element,
    app_state: &InteractiveCanvasAppState,
    selection_color: &str,
) {
    let b = &text.base;
    let zoom = app_state.zoom;
    ctx.save();
    let padding = get_text_box_padding(zoom);
    let width = b.width + padding * 2.0;
    let height = b.height + padding * 2.0;
    let cx = b.x + b.width / 2.0;
    let cy = b.y + b.height / 2.0;
    let shift_x = -(b.width / 2.0 + padding);
    let shift_y = -(b.height / 2.0 + padding);
    ctx.translate(cx + app_state.scroll_x, cy + app_state.scroll_y);
    ctx.rotate(b.angle.0);
    ctx.set_line_width(1.0 / zoom);
    ctx.set_stroke_style(selection_color);
    ctx.set_global_alpha(0.5);
    ctx.set_line_dash(&[6.0 / zoom, 4.0 / zoom]);
    ctx.stroke_rect(shift_x, shift_y, width, height);
    ctx.restore();
}

/// `renderResetAutoResizeHandle(text, context, appState, selectionColor,
/// formFactor)` (`interactiveScene.ts:1579-1612`).
fn render_reset_auto_resize_handle(
    ctx: &mut Canvas,
    text: &Element,
    app_state: &InteractiveCanvasAppState,
    selection_color: &str,
    form_factor: FormFactor,
) {
    let Some(handle) = get_text_auto_resize_handle(text, app_state.zoom, form_factor) else {
        return;
    };
    ctx.save();
    ctx.set_global_alpha(0.5);
    ctx.set_line_width(1.5 / app_state.zoom);
    ctx.s.line_cap = LineCap::Round;
    ctx.set_stroke_style(selection_color);
    ctx.begin_path();
    ctx.move_to(
        handle.start[0] + app_state.scroll_x,
        handle.start[1] + app_state.scroll_y,
    );
    ctx.line_to(
        handle.end[0] + app_state.scroll_x,
        handle.end[1] + app_state.scroll_y,
    );
    ctx.stroke();
    ctx.restore();
}

fn is_text(element: &Element) -> bool {
    matches!(element.kind, ElementKind::Text(_))
}

fn auto_resize(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Text(t) if t.auto_resize)
}

// ---------------------------------------------------------------------------
// Snaps

/// `renderSnaps(context, appState)` (`renderSnaps.ts:16-213`), from the
/// calls [`render_snaps`] lists.
fn draw_snaps(ctx: &mut Canvas, app_state: &InteractiveCanvasAppState) {
    let calls = render_snaps(
        &app_state.snap_lines,
        &SnapRenderState {
            theme: app_state.theme,
            zen_mode_enabled: app_state.zen_mode_enabled,
            zoom: app_state.zoom,
            scroll_x: app_state.scroll_x,
            scroll_y: app_state.scroll_y,
        },
    );
    for call in calls {
        match call {
            SnapCanvasCall::Save => ctx.save(),
            SnapCanvasCall::Restore => ctx.restore(),
            SnapCanvasCall::Translate(x, y) => ctx.translate(x, y),
            SnapCanvasCall::LineWidth(w) => ctx.set_line_width(w),
            SnapCanvasCall::StrokeStyle(color) => ctx.set_stroke_style(color),
            SnapCanvasCall::BeginPath => ctx.begin_path(),
            SnapCanvasCall::MoveTo(x, y) => ctx.move_to(x, y),
            SnapCanvasCall::LineTo(x, y) => ctx.line_to(x, y),
            SnapCanvasCall::Stroke => ctx.stroke(),
        }
    }
}

// ---------------------------------------------------------------------------
// The scene

/// The search matches (`interactiveScene.ts:2071-2103`): for each match
/// whose element is in the map, its matched lines filled in the element's
/// rotated frame, those of a frame name scaled back from viewport px, and
/// those not shown on the canvas only when the match is focused.
fn render_search_matches(
    ctx: &mut Canvas,
    app_state: &InteractiveCanvasAppState,
    elements_map: &ElementsMap<'_>,
) {
    let (focus_color, match_color) =
        SEARCH_MATCH_COLOR[usize::from(app_state.theme == Theme::Dark)];
    for m in &app_state.search_matches {
        let Some(element) = elements_map.get(&m.id) else {
            continue;
        };
        let [x1, y1, _, _, cx, cy] = get_element_absolute_coords(element, elements_map, true);
        ctx.save();
        ctx.set_fill_style(if m.focus { focus_color } else { match_color });
        let zoom_factor = if is_frame_like(element) {
            app_state.zoom
        } else {
            1.0
        };
        ctx.translate(app_state.scroll_x, app_state.scroll_y);
        ctx.translate(cx, cy);
        ctx.rotate(element.base.angle.0);
        for line in &m.matched_lines {
            if line.show_on_canvas || m.focus {
                ctx.fill_rect(
                    x1 + line.offset_x / zoom_factor - cx,
                    y1 + line.offset_y / zoom_factor - cy,
                    line.width / zoom_factor,
                    line.height / zoom_factor,
                );
            }
        }
        ctx.restore();
    }
}

/// `renderInteractiveScene(renderConfig)` (`interactiveScene.ts:1614-2166`):
/// the interactive canvas as a display list to replay from a fresh
/// context, in the order of work described in the module documentation.
pub fn render_interactive_scene(scene: &InteractiveScene<'_>) -> DisplayList {
    // the same whole-device-pixel scroll the static scene draws at, so the
    // overlays sit exactly on the content
    let (scroll_x, scroll_y) = snap_scroll_to_device_pixels(
        scene.app_state.scroll_x,
        scene.app_state.scroll_y,
        scene.app_state.zoom,
        scene.scale,
    );
    let snapped = InteractiveCanvasAppState {
        scroll_x,
        scroll_y,
        ..scene.app_state.clone()
    };
    let app_state = &snapped;
    let zoom = app_state.zoom;
    let theme = app_state.theme;
    let elements_map = scene.elements_map;
    let selected = scene.selected_elements;
    let selection_color = scene.selection_color;

    // bootstrapCanvas: setTransform(1, 0, 0, 1, 0, 0), scale(dpr), clear
    let mut ctx = Canvas::new();
    ctx.scale(scene.scale, scene.scale);

    // Apply zoom
    ctx.save();
    ctx.scale(zoom, zoom);

    let linear = app_state.selected_linear_element.as_ref();
    let editing_linear_element = linear.filter(|l| l.is_editing).and_then(|l| {
        scene
            .visible_elements
            .iter()
            .rev()
            .copied()
            .find(|e| e.base.id == l.element_id)
    });
    if let Some(element) = editing_linear_element {
        linear_point_handles(&mut ctx, app_state, element, elements_map);
    }

    // Paint selection element
    if let Some(selection) = app_state.selection_element.as_ref() {
        if !app_state.is_cropping {
            let b = &selection.base;
            ctx.save();
            ctx.translate(b.x + app_state.scroll_x, b.y + app_state.scroll_y);
            ctx.set_fill_style(&themed("rgba(0, 0, 200, 0.04)", theme));
            // render from 0.5px offset to get 1px wide line
            let offset = 0.5 / zoom;
            ctx.fill_rect(offset, offset, b.width, b.height);
            ctx.set_line_width(1.0 / zoom);
            ctx.set_stroke_style(selection_color);
            ctx.stroke_rect(offset, offset, b.width, b.height);
            ctx.restore();
        }
    }

    // getActiveTextElement(selectedElements, appState)
    let active_text = app_state.editing_text_element.as_ref().or(match selected {
        [only] if is_text(only) => Some(*only),
        _ => None,
    });
    if let Some(text) = active_text {
        if !auto_resize(text) {
            render_reset_auto_resize_handle(
                &mut ctx,
                text,
                app_state,
                selection_color,
                scene.editor_interface.form_factor,
            );
        }
    }

    if let Some(editing) = app_state.editing_text_element.as_ref() {
        if let Some(text) = scene
            .all_elements_map
            .get(&editing.base.id)
            .filter(|t| is_text(t) && !auto_resize(t))
        {
            render_text_box(&mut ctx, text, app_state, selection_color);
        }
    }

    if app_state.is_binding_enabled {
        if let Some(suggested) = app_state.suggested_binding.as_ref() {
            render_binding_highlight(&mut ctx, suggested, scene, app_state);
        }
    }

    if let Some(frame) = app_state.frame_to_highlight.as_ref() {
        render_frame_highlight(&mut ctx, app_state, frame, elements_map);
    }

    if let Some(elements) = app_state.elements_to_highlight.as_ref() {
        let refs: Vec<&Element> = elements.iter().collect();
        render_elements_box_highlight(&mut ctx, app_state, &refs, None, false);
    }

    if let Some(id) = app_state.active_locked_id.as_deref() {
        let elements = match scene.all_elements_map.get(id) {
            Some(element) => vec![element],
            None => elements_in_group(scene.all_elements, id),
        };
        render_elements_box_highlight(
            &mut ctx,
            app_state,
            &elements,
            Some(vec![themed("#ced4da", theme)]),
            true,
        );
    }

    let is_frame_selected = selected.iter().any(|e| is_frame_like(e));

    if let ([only], Some(l)) = (selected, linear) {
        if l.is_editing && l.element_id == only.base.id {
            linear_point_handles(&mut ctx, app_state, only, elements_map);
        }
    }

    // Arrows have a different highlight behavior when they are the only
    // selected element
    if let Some((l, element)) =
        linear.and_then(|l| Some((l, scene.all_elements_map.get(&l.element_id)?)))
    {
        if !l.is_dragging {
            if let Some(coords) = l.segment_mid_point_hovered_coords {
                // renderElbowArrowMidPointHighlight
                highlight_point(&mut ctx, coords, app_state);
            } else if if is_elbow_arrow(element) {
                l.hover_point_index == 0
                    || l.hover_point_index == points_of(element).len() as i64 - 1
            } else {
                l.hover_point_index >= 0
            } {
                render_linear_element_point_highlight(&mut ctx, app_state, elements_map);
            }
        }
        if matches!(element.kind, ElementKind::Arrow(_)) {
            for end in [ArrowEnd::Start, ArrowEnd::End] {
                render_focus_point_indicator(
                    &mut ctx,
                    element,
                    app_state,
                    end,
                    scene.all_elements_map,
                );
            }
        }
    }

    // Paint selected elements
    if app_state.multi_element.is_none()
        && app_state.new_element.is_none()
        && !linear.is_some_and(|l| l.is_editing)
    {
        let show_bounding_box = has_bounding_box(
            selected,
            linear.map(|l| SelectedLinearElementState {
                is_editing: l.is_editing,
                is_dragging: l.is_dragging,
                hover_point_index: l.hover_point_index,
            }),
            &scene.editor_interface,
        );
        let is_linear =
            |e: &Element| matches!(e.kind, ElementKind::Line(_) | ElementKind::Arrow(_));
        let single_linear = matches!(selected, [only] if is_linear(only));
        // render selected linear element points
        if let ([only], true) = (selected, single_linear) {
            if linear.is_some_and(|l| l.element_id == only.base.id) && !only.base.locked {
                linear_point_handles(&mut ctx, app_state, only, elements_map);
            }
        }
        let selection_color = if selection_color.is_empty() {
            themed("#000", theme)
        } else {
            selection_color.to_owned()
        };
        let locked_selection_color = themed("#ced4da", theme);
        let group_selection_color = themed("#000", theme);

        if show_bounding_box {
            let selected_ids: HashSet<&str> = selected.iter().map(|e| e.base.id.as_str()).collect();
            let mut selections = Vec::new();
            for &element in scene.elements {
                let mut selection_colors = Vec::new();
                let bound_elbow = single_linear
                    && is_elbow_arrow(element)
                    && matches!(&element.kind, ElementKind::Arrow(a)
                        if a.linear.start_binding.is_some() || a.linear.end_binding.is_some());
                // local user
                if !bound_elbow
                    && selected_ids.contains(element.base.id.as_str())
                    && !is_selected_via_group(app_state, element)
                {
                    selection_colors.push(selection_color.clone());
                }
                if !selection_colors.is_empty() {
                    let [x1, y1, x2, y2, cx, cy] =
                        get_element_absolute_coords(element, elements_map, true);
                    let locked = element.base.locked;
                    selections.push(SelectionBorder {
                        angle: element.base.angle.0,
                        x1,
                        y1,
                        x2,
                        y2,
                        selection_colors: if locked {
                            vec![locked_selection_color.clone()]
                        } else {
                            selection_colors
                        },
                        dashed: locked,
                        cx,
                        cy,
                        active_embeddable: app_state.active_embeddable_id.as_deref()
                            == Some(element.base.id.as_str()),
                        padding: (app_state.cropping_element_id.as_deref()
                            == Some(element.base.id.as_str())
                            || matches!(element.kind, ElementKind::Image(_)))
                        .then_some(0.0),
                    });
                }
            }
            let group_border = |group_id: &str| {
                let group = elements_in_group(scene.elements, group_id);
                border_of_bounds(
                    get_common_bounds(&group),
                    if group.iter().any(|e| e.base.locked) {
                        vec![locked_selection_color.clone()]
                    } else {
                        vec![group_selection_color.clone()]
                    },
                    true,
                )
            };
            for group_id in app_state.selected_group_ids() {
                selections.push(group_border(group_id));
            }
            if let Some(editing) = app_state.editing_group_id.as_deref() {
                selections.push(group_border(editing));
            }
            for selection in &selections {
                render_selection_border(&mut ctx, app_state, selection);
            }
        }

        // Paint resize transformHandles
        ctx.save();
        ctx.translate(app_state.scroll_x, app_state.scroll_y);
        let omit = get_omit_sides_for_editor_interface(&scene.editor_interface);
        if let [only] = selected {
            ctx.set_fill_style(&themed("#fff", theme));
            let handles =
                get_transform_handles(only, zoom, elements_map, PointerType::Mouse, &omit);
            let editing_text = app_state.editing_text_element.as_ref().is_some_and(is_text);
            if !app_state.view_mode_enabled
                && show_bounding_box
                // do not show transform handles when text is being edited
                && !editing_text
                // do not show transform handles when image is being cropped
                && app_state.cropping_element_id.is_none()
            {
                render_transform_handles(&mut ctx, &selection_color, app_state, &handles);
            }
            if let Some(id) = app_state.cropping_element_id.as_deref() {
                if !app_state.is_cropping {
                    if let Some(image) = elements_map
                        .get(id)
                        .filter(|e| matches!(e.kind, ElementKind::Image(_)))
                    {
                        render_crop_handles(
                            &mut ctx,
                            &selection_color,
                            app_state,
                            image,
                            elements_map,
                        );
                    }
                }
            }
        } else if selected.len() > 1
            && !app_state.is_rotating
            && !selected.iter().any(|e| e.base.locked)
        {
            let dashed_line_padding = DEFAULT_TRANSFORM_HANDLE_SPACING * 2.0 / zoom;
            ctx.set_fill_style(&themed("#fff", theme));
            let [x1, y1, x2, y2] = get_common_bounds_in(selected, elements_map);
            let initial_line_dash = ctx.s.dash.clone();
            ctx.set_line_dash(&[2.0 / zoom]);
            let line_width = ctx.s.line_width;
            ctx.set_line_width(1.0 / zoom);
            ctx.set_stroke_style(&selection_color);
            stroke_rect_with_rotation(
                &mut ctx,
                x1 - dashed_line_padding,
                y1 - dashed_line_padding,
                x2 - x1 + dashed_line_padding * 2.0,
                y2 - y1 + dashed_line_padding * 2.0,
                (x1 + x2) / 2.0,
                (y1 + y2) / 2.0,
                0.0,
                false,
                0.0,
            );
            ctx.set_line_width(line_width);
            ctx.set_line_dash(&initial_line_dash);
            let mut omit = omit;
            if is_frame_selected {
                omit.rotation = true;
            }
            let handles = get_transform_handles_from_coords(
                [x1, y1, x2, y2, (x1 + x2) / 2.0, (y1 + y2) / 2.0],
                0.0,
                zoom,
                PointerType::Mouse,
                &omit,
                None,
                None,
            );
            if selected.iter().any(|e| !e.base.locked) {
                render_transform_handles(&mut ctx, &selection_color, app_state, &handles);
            }
        }
        ctx.restore();
    }

    render_search_matches(&mut ctx, app_state, elements_map);

    draw_snaps(&mut ctx, app_state);

    ctx.restore();
    ctx.finish()
}
