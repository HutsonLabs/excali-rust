//! Text editing: the `<textarea>` overlay a text is typed into, and the
//! app's side of it.
//!
//! Upstream: `packages/excalidraw/wysiwyg/textWysiwyg.tsx` (the overlay)
//! and the `App` members around it in `components/App.tsx`
//! (`startTextEditing`, `handleTextWysiwyg`, `getMaxTextWidth`,
//! `getTextViewportOffsets`, `getTextWysiwygSnappedToCenterPosition`,
//! `getTextCreationGridPoint`, `getSelectedTextElement`,
//! `insertNewElements`), at the pinned commit. Research page
//! `site/content/research/rendering.md`, section 5, "Text editing".
//!
//! [`start_text_editing`] creates a text (at a point, or as the label of a
//! container, which grows to hold a line) or picks the one to edit, and
//! opens a [`TextEditor`] on it. The editor is a model of the overlay, with
//! no DOM: `excali-ui` mounts the textarea, applies [`TextEditor::style`]
//! and [`TextEditor::attributes`], and hands the textarea's events back
//! ([`TextEditor::input`], [`TextEditor::keydown`], [`TextEditor::submit`],
//! [`TextEditor::editor_box_scrolled`], ...).
//!
//! - **The textarea.** `dir="auto"`, `wrap="off"` (no line wrapping in
//!   Safari), `data-type="wysiwyg"`, class `excalidraw-wysiwyg`, in the
//!   editor's `.excalidraw-textEditorContainer` box. It is absolutely
//!   positioned at the text's top-left corner in viewport coordinates,
//!   its size the text's (half a pixel wider in a container) with a 5%
//!   height buffer so it does not jump as lines are added, in the text's
//!   font, line height, alignment, colour (through the dark mode filter)
//!   and opacity, and transformed by
//!   `translate(w(z-1)/2, h(z-1)/2) scale(z) rotate(angle)` about the
//!   text's centre ([`get_transform`]).
//! - **Typing.** Every `input` normalises the value (line endings, tabs to
//!   spaces) and writes it to the text's `originalText`; the text is
//!   re-wrapped and re-measured (`refreshTextDimensions`, or the sticky
//!   note fit), a free text stops growing at the view's width and wraps
//!   from there, and a label's container grows as its text does and
//!   shrinks back to its original height as text is removed.
//! - **Keys.** Tab and Shift+Tab (or Ctrl/Cmd+] and [) indent and outdent
//!   the selected lines by four spaces; Escape and Ctrl/Cmd+Enter submit;
//!   the zoom keys zoom; the font size and save keys go to the app's
//!   actions ([`AppCall::ExecuteAction`]).
//! - **Submit.** The text is bound to (or, when empty, unbound from) its
//!   container and laid out in it (`redrawTextBoundingBox`); an empty text
//!   is deleted; a keyboard submit selects the text (or its container);
//!   the edit is captured as one undoable step.
//!
//! The steps are upstream's, down to which element is mutated when, so
//! every element ends with upstream's fields (`version` included). What
//! upstream reads from the DOM (the sidebar insets), from hit testing (the
//! text and the frame under the pointer) and the arrows bound to a
//! resized element (arrow binding) come from a [`TextEditingHost`].

use excali_core::app_state::AppState;
use excali_core::color::apply_dark_mode_filter;
use excali_core::constants::{stroke_width_by_key, DEFAULT_FONT_SIZE, DEFAULT_VERTICAL_ALIGN};
use excali_core::element::{
    BoundElement, BoundElementType, Element, ElementBase, ElementKind, ElementType, FontFamily,
    StrokeWidthKey, TextAlign, TextFields, VerticalAlign,
};
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_core::json::number_to_string;
use excali_math::js;
use excali_scene::bounds::{get_bound_text_element_id, get_element_bounds, ElementsMap};
use excali_scene::linear_element::{
    get_bound_text_element_center, get_bound_text_element_position,
};
use excali_scene::render_element::is_rtl;
use excali_text::font_metadata::{
    get_font_family_string, get_font_string, get_line_height, get_line_height_in_px,
};
use excali_text::new_element::{new_text_element, refresh_text_dimensions, NewTextElementOptions};
use excali_text::text_element::{
    compute_bound_text_position, compute_container_dimension_for_bound_text,
    get_bound_text_max_height,
};
use excali_text::text_measurements::{
    get_approx_min_line_width, get_line_width, normalize_text, TextMetricsProvider,
};
use excali_text::text_wrapping::get_wrapped_text_lines;
use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use crate::js_value::{num, truthy};
use crate::mutate::{bump_version, new_element_with};
use crate::session::{Session, SessionError};
use crate::store::{DynStamp, HistoryEnv};
use crate::text_layout::{
    get_sticky_note_layout, mutate_in, one, SceneArrowGeometry, StickyNoteLayoutOpts, TextLayouter,
};
use crate::viewport::{
    perform_zoom_action, scene_coords_to_viewport_coords, scroll_bounds_into_view, translate,
    Offsets, TooLarge, TranslateOptions, ViewportState, ViewportUpdate, ZoomAction, ZoomKeyEvent,
};

/// `TEXT_VIEWPORT_PADDING` (`constants.ts:27`): the room a text keeps from
/// the edges of the view, in viewport px.
pub const TEXT_VIEWPORT_PADDING: f64 = 20.0;
/// `TEXT_TO_CENTER_SNAP_THRESHOLD` (`constants.ts:33`): how close to a
/// container's centre (scene px) a new label snaps to it.
pub const TEXT_TO_CENTER_SNAP_THRESHOLD: f64 = 30.0;
/// `CARET_FOLLOW_PADDING` (`textWysiwyg.tsx:78-83`): the room the canvas
/// keeps past a caret it pans to, in viewport px.
pub const CARET_FOLLOW_PADDING: f64 = 5.0;
/// `DEFAULT_BOUND_TEXT_LABEL_POSITION` (`textElement.ts:617`): where a new
/// arrow label sits along the arrow.
pub const DEFAULT_BOUND_TEXT_LABEL_POSITION: f64 = 0.5;
/// The indent Tab inserts (`TAB_SIZE`, `textWysiwyg.tsx:773`).
pub const TAB_SIZE: usize = 4;

/// The textarea's attributes (`textWysiwyg.tsx:487-493`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextareaAttributes {
    pub dir: &'static str,
    pub tab_index: i32,
    /// `data-type`.
    pub data_type: &'static str,
    /// No line wrapping in Safari.
    pub wrap: &'static str,
    pub class_name: &'static str,
    /// The class of the box the textarea is appended to.
    pub container_class_name: &'static str,
}

/// The textarea every editor mounts.
pub const TEXTAREA_ATTRIBUTES: TextareaAttributes = TextareaAttributes {
    dir: "auto",
    tab_index: 0,
    data_type: "wysiwyg",
    wrap: "off",
    class_name: "excalidraw-wysiwyg",
    container_class_name: "excalidraw-textEditorContainer",
};

/// `getTransform(width, height, angle, appState)` (`textWysiwyg.tsx:93-104`):
/// the textarea's CSS transform. The editor is scaled and rotated about the
/// text's centre; the translate makes up for the zoom putting that centre
/// at half the scaled size from the box's corner.
pub fn get_transform(width: f64, height: f64, angle: f64, zoom: f64) -> String {
    let degree = (180.0 * angle) / std::f64::consts::PI;
    let translate_x = (width * (zoom - 1.0)) / 2.0;
    let translate_y = (height * (zoom - 1.0)) / 2.0;
    format!(
        "translate({}px, {}px) scale({}) rotate({}deg)",
        number_to_string(translate_x),
        number_to_string(translate_y),
        number_to_string(zoom),
        number_to_string(degree)
    )
}

fn px(x: f64) -> String {
    format!("{}px", number_to_string(x))
}

/// What the editor asks of the app, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCall {
    /// `store.scheduleCapture()`: the edit becomes one undoable step (done
    /// on the session; reported for the record).
    ScheduleCapture,
    /// `focusContainer()`: focus back on the editor's container.
    FocusContainer,
    /// `cursor.applyForTool()`: a tool that survives the submit gets its
    /// cursor back.
    ApplyToolCursor,
    /// `actionManager.executeAction(action)`. The zoom actions are
    /// performed here; the font size and save actions are the app's.
    ExecuteAction(&'static str),
    /// `viewport.translate(...)`: the canvas was panned (done on the
    /// session).
    Translate,
}

impl AppCall {
    /// The name the fixture records.
    pub fn name(&self) -> String {
        match self {
            AppCall::ScheduleCapture => "scheduleCapture".into(),
            AppCall::FocusContainer => "focusContainer".into(),
            AppCall::ApplyToolCursor => "applyToolCursor".into(),
            AppCall::ExecuteAction(name) => format!("executeAction:{name}"),
            AppCall::Translate => "translate".into(),
        }
    }
}

/// What the editor reads from the page and from the app's other parts.
pub trait TextEditingHost {
    /// The width of a sidebar docked over the canvas's left and right
    /// edges (`app.viewport.getSidebarInsets()`), which texts keep clear
    /// of and the editor's box stops at.
    fn sidebar_insets(&self) -> (f64, f64) {
        (0.0, 0.0)
    }

    /// `getTextElementAtPosition(x, y)`: the non-deleted text hit at a
    /// scene point.
    fn text_element_at(&self, elements: &[Element], x: f64, y: f64) -> Option<String> {
        let _ = (elements, x, y);
        None
    }

    /// `getTopLayerFrameAtSceneCoords({x, y})`: the unlocked frame on top
    /// at a scene point.
    fn top_layer_frame_at(&self, elements: &[Element], x: f64, y: f64) -> Option<String> {
        let _ = (elements, x, y);
        None
    }

    /// `updateBoundElements(element, scene)` (`binding.ts:1321`): the
    /// arrows bound to `id` follow it (arrow binding). Every change goes
    /// through `mutate_element`.
    fn update_bound_elements(
        &mut self,
        stamp: &mut dyn ChangeStamp,
        elements: &mut SceneElementsMap,
        id: &str,
    ) -> std::result::Result<(), String> {
        let _ = (stamp, elements, id);
        Ok(())
    }

    /// `props.gridModeEnabled`, which overrides the app state's.
    fn grid_mode_enabled(&self) -> Option<bool> {
        None
    }

    /// `editorInterface.isTouchScreen`: the text is not selected on open.
    fn is_touch_screen(&self) -> bool {
        false
    }
}

/// A host with no sidebar, hit testing, bound arrows or touch screen.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHost;

impl TextEditingHost for NoHost {}

/// Why text editing failed where upstream throws.
#[derive(Debug, Clone, PartialEq)]
pub enum TextEditingError {
    Session(SessionError),
    Layout(String),
}

impl std::fmt::Display for TextEditingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextEditingError::Session(e) => write!(f, "{e}"),
            TextEditingError::Layout(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TextEditingError {}

impl From<SessionError> for TextEditingError {
    fn from(e: SessionError) -> Self {
        TextEditingError::Session(e)
    }
}

impl From<String> for TextEditingError {
    fn from(e: String) -> Self {
        TextEditingError::Layout(e)
    }
}

type Result<T> = std::result::Result<T, TextEditingError>;

/// What text editing works on: the scene with its store and history, text
/// layout, and the host.
pub struct TextEditingContext<'a, E: HistoryEnv, P: TextMetricsProvider> {
    pub session: &'a mut Session<E>,
    pub layouter: &'a mut TextLayouter<P>,
    pub host: &'a mut dyn TextEditingHost,
}

// -- small readers ----------------------------------------------------------------------

fn text_fields(element: &Element) -> Option<&TextFields> {
    match &element.kind {
        ElementKind::Text(t) => Some(t),
        _ => None,
    }
}

fn is_arrow(element: &Element) -> bool {
    matches!(element.kind, ElementKind::Arrow(_))
}

fn is_sticky(element: &Element) -> bool {
    matches!(element.kind, ElementKind::StickyNote(_))
}

fn container_id_of(element: &Element) -> Option<&str> {
    text_fields(element)
        .and_then(|t| t.container_id.as_deref())
        .filter(|id| !id.is_empty())
}

fn find<'e>(elements: &'e [Element], id: &str) -> Option<&'e Element> {
    elements.iter().find(|e| e.base.id == id)
}

fn find_non_deleted<'e>(elements: &'e [Element], id: &str) -> Option<&'e Element> {
    find(elements, id).filter(|e| !e.base.is_deleted)
}

fn non_deleted(elements: &[Element]) -> Vec<Element> {
    elements
        .iter()
        .filter(|e| !e.base.is_deleted)
        .cloned()
        .collect()
}

fn state_number(state: &AppState, key: &str) -> Option<f64> {
    state.get(key).and_then(Value::as_f64)
}

fn state_str<'s>(state: &'s AppState, key: &str) -> Option<&'s str> {
    state.get(key).and_then(Value::as_str)
}

fn active_tool(state: &AppState) -> (Option<&str>, bool) {
    let tool = state.get("activeTool");
    (
        tool.and_then(|t| t.get("type")).and_then(Value::as_str),
        truthy(tool.and_then(|t| t.get("locked"))),
    )
}

/// `isShallowEqual(prev.selectedElementIds, next)` then the previous object
/// or the next (`makeNextSelectedElementIds`); equal content either way.
fn selected_ids(state: &AppState) -> Map<String, Value> {
    state
        .get("selectedElementIds")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// UTF-16 length.
fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The byte offset of UTF-16 offset `i` (clamped to the string).
fn byte_at(s: &str, i: usize) -> usize {
    let mut units = 0;
    for (byte, c) in s.char_indices() {
        if units >= i {
            return byte;
        }
        units += c.len_utf16();
    }
    s.len()
}

/// `s.slice(from, to)` in UTF-16 offsets.
fn slice16(s: &str, from: usize, to: usize) -> &str {
    let (a, b) = (byte_at(s, from), byte_at(s, to.max(from)));
    &s[a..b]
}

// -- starting --------------------------------------------------------------------------

/// Which text `startTextEditing` edits (its `textElement` argument).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TextTarget {
    /// `undefined`: a single selected text, the label of a selected
    /// container or of the given arrow, else the text under the point.
    #[default]
    Resolve,
    /// `null`: always create a text.
    New,
    /// That text.
    Existing(String),
}

/// `startTextEditing`'s arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct StartTextEditing {
    /// Where to insert the text, in scene coordinates.
    pub scene_x: f64,
    pub scene_y: f64,
    /// Insert at the container's centre when the point is near it.
    pub insert_at_parent_center: bool,
    pub container: Option<String>,
    /// Open the editor on a new text (else the text becomes the element
    /// being drawn, to be sized by a drag).
    pub auto_edit: bool,
    /// Where to put the caret in an existing text (scene coordinates);
    /// `None` selects the whole text.
    pub initial_caret: Option<[f64; 2]>,
    pub text_element: TextTarget,
}

impl StartTextEditing {
    /// At a scene point, upstream's defaults for the rest.
    pub fn at(scene_x: f64, scene_y: f64) -> StartTextEditing {
        StartTextEditing {
            scene_x,
            scene_y,
            insert_at_parent_center: true,
            container: None,
            auto_edit: true,
            initial_caret: None,
            text_element: TextTarget::Resolve,
        }
    }
}

/// `getTextWysiwygSnappedToCenterPosition(x, y, appState, container)`
/// (`App.tsx:13938-13970`): the container's centre (a text container's
/// middle, an arrow's label point) when the point is within
/// [`TEXT_TO_CENTER_SNAP_THRESHOLD`] of it.
pub fn snapped_to_center_position(
    x: f64,
    y: f64,
    container: Option<&Element>,
    elements: &[Element],
) -> Option<[f64; 2]> {
    let container = container?;
    let map = ElementsMap::new(elements.iter().filter(|e| !e.base.is_deleted));
    let [cx, cy] = if is_arrow(container) {
        get_bound_text_element_center(container, &map)
    } else {
        [
            container.base.x + container.base.width / 2.0,
            container.base.y + container.base.height / 2.0,
        ]
    };
    let distance = js::hypot(x - cx, y - cy);
    (distance < TEXT_TO_CENTER_SNAP_THRESHOLD).then_some([cx, cy])
}

/// `getTextViewportOffsets()` (`App.tsx:6446-6455`): the room a text keeps
/// from the view's edges, the sidebar's width added on its side.
pub fn text_viewport_offsets(sidebar: (f64, f64)) -> Offsets {
    Offsets {
        top: TEXT_VIEWPORT_PADDING,
        right: sidebar.1 + TEXT_VIEWPORT_PADDING,
        bottom: TEXT_VIEWPORT_PADDING,
        left: sidebar.0 + TEXT_VIEWPORT_PADDING,
    }
}

/// `getMaxTextWidth()` (`App.tsx:6462-6466`): the widest a free text grows
/// as it is typed, in scene units: the view less the offsets, over the
/// zoom (unbounded when nothing is left).
pub fn max_text_width(state: &AppState, sidebar: (f64, f64)) -> f64 {
    let offsets = text_viewport_offsets(sidebar);
    let width = state_number(state, "width").unwrap_or(0.0) - offsets.left - offsets.right;
    if width > 0.0 {
        width / state.zoom().unwrap_or(1.0)
    } else {
        f64::INFINITY
    }
}

/// `getTextCreationGridPoint(x, y)` (`App.tsx:1528-1546`): the grid cell's
/// top-left corner under the point, with the grid on.
fn text_creation_grid_point(
    state: &AppState,
    host: &dyn TextEditingHost,
    x: f64,
    y: f64,
) -> Option<[f64; 2]> {
    let enabled = host
        .grid_mode_enabled()
        .unwrap_or_else(|| truthy(state.get("gridModeEnabled")));
    if !enabled {
        return None;
    }
    let size = state_number(state, "gridSize")?;
    let snap = |c: f64| (c / size).floor() * size;
    Some([snap(x), snap(y)])
}

/// `getSelectedTextElement(container)` (`App.tsx:6679-6703`).
fn selected_text_element(
    state: &AppState,
    elements: &[Element],
    container: Option<&Element>,
) -> Option<String> {
    let ids = selected_ids(state);
    let selected: Vec<&Element> = elements
        .iter()
        .filter(|e| truthy(ids.get(&e.base.id)) && !e.base.is_deleted)
        .collect();
    let [only] = selected.as_slice() else {
        return None;
    };
    if matches!(only.kind, ElementKind::Text(_)) {
        return Some(only.base.id.clone());
    }
    container?;
    bound_text_of(only, elements)
}

/// `getBoundTextElement(container, nonDeletedElementsMap)`.
fn bound_text_of(container: &Element, elements: &[Element]) -> Option<String> {
    let id = get_bound_text_element_id(container)?;
    find_non_deleted(elements, id).map(|e| e.base.id.clone())
}

fn stroke_width_key(state: &AppState) -> StrokeWidthKey {
    match state_str(state, "currentItemStrokeWidthKey") {
        Some("thin") => StrokeWidthKey::Thin,
        Some("bold") => StrokeWidthKey::Bold,
        _ => StrokeWidthKey::Medium,
    }
}

fn parse<T: serde::de::DeserializeOwned>(value: Option<&Value>) -> Option<T> {
    value.and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// `startTextEditing({...})` (`App.tsx:7044-7294`): the text to edit, found
/// or created, and the editor opened on it (`None` when a new text is left
/// to be sized by a drag instead: `newElement` is set to it).
///
/// A label is created in a container whose centre the point is near (the
/// container grows to hold one line first, unless it is an arrow or a
/// sticky note); a free text is created centred vertically on the point's
/// first line, or at the grid cell's corner with the grid on, in the frame
/// under it. A new text takes the app's current style, the container's
/// angle and groups when it is a label, the note's ink in a sticky note,
/// and the middle of an arrow.
pub fn start_text_editing<E: HistoryEnv, P: TextMetricsProvider>(
    ctx: &mut TextEditingContext<'_, E, P>,
    args: &StartTextEditing,
) -> Result<Option<TextEditor>> {
    let (mut scene_x, mut scene_y) = (args.scene_x, args.scene_y);
    let mut should_bind_to_container = false;
    let elements = ctx.session.elements().to_vec();
    let container = args
        .container
        .as_deref()
        .and_then(|id| find(&elements, id))
        .cloned();

    let mut parent_center = if args.insert_at_parent_center {
        snapped_to_center_position(scene_x, scene_y, container.as_ref(), &elements)
    } else {
        None
    };
    if let (Some(c), Some(_)) = (&container, parent_center) {
        if bound_text_of(c, &elements).is_none() {
            should_bind_to_container = true;
        }
    }
    let state = ctx.session.app_state().clone();
    let existing: Option<Element> = match &args.text_element {
        TextTarget::New => None,
        TextTarget::Existing(id) => find(&elements, id).cloned(),
        TextTarget::Resolve => selected_text_element(&state, &elements, container.as_ref())
            .or_else(|| {
                container
                    .as_ref()
                    .filter(|c| is_arrow(c))
                    .and_then(|c| bound_text_of(c, &elements))
            })
            .or_else(|| {
                ctx.host
                    .text_element_at(&non_deleted(&elements), scene_x, scene_y)
            })
            .and_then(|id| find_non_deleted(&elements, &id).cloned())
            .filter(|e| matches!(e.kind, ElementKind::Text(_))),
    };

    let existing_fields = existing.as_ref().and_then(text_fields);
    let font_family = existing_fields
        .map(|t| t.font_family)
        .filter(|f| f.0 != 0)
        .or_else(|| parse::<FontFamily>(state.get("currentItemFontFamily")))
        .unwrap_or(FontFamily::DEFAULT);
    let line_height = existing_fields
        .map(|t| t.line_height)
        .filter(|l| *l != 0.0 && !l.is_nan())
        .unwrap_or_else(|| get_line_height(font_family));
    let font_size = state_number(&state, "currentItemFontSize").unwrap_or(DEFAULT_FONT_SIZE);

    let mut container = container;
    if existing.is_none() && should_bind_to_container {
        if let Some(c) = container.clone().filter(|c| !is_arrow(c) && !is_sticky(c)) {
            let font = get_font_string(font_size, font_family);
            let min_width = get_approx_min_line_width(
                &font,
                line_height,
                &ctx.layouter.provider,
                &ctx.layouter.char_widths,
            );
            let min_height = get_line_height_in_px(font_size, line_height)
                + excali_core::constants::BOUND_TEXT_PADDING * 2.0;
            let new_height = js::max(c.base.height, min_height);
            let new_width = js::max(c.base.width, min_width);
            let mut updates = one("height", num(new_height));
            updates.insert("width".into(), num(new_width));
            mutate_scene(ctx.session, &c.base.id, updates)?;
            scene_x = c.base.x + new_width / 2.0;
            scene_y = c.base.y + new_height / 2.0;
            let elements = ctx.session.elements().to_vec();
            container = find(&elements, &c.base.id).cloned();
            if parent_center.is_some() {
                parent_center =
                    snapped_to_center_position(scene_x, scene_y, container.as_ref(), &elements);
            }
        }
    }

    let grid_point = text_creation_grid_point(&state, ctx.host, scene_x, scene_y);
    let [x, y] = match (parent_center, &existing) {
        (Some(center), _) => center,
        (None, None) => [
            grid_point.map_or(scene_x, |g| g[0]),
            match grid_point {
                // free text starts from a point cursor: the first line
                // box is centred on it
                None => scene_y - get_line_height_in_px(font_size, line_height) / 2.0,
                Some(g) => g[1],
            },
        ],
        (None, Some(_)) => [scene_x, scene_y],
    };

    let elements = ctx.session.elements().to_vec();
    let top_layer_frame = ctx
        .host
        .top_layer_frame_at(&non_deleted(&elements), x, y)
        .and_then(|id| find_non_deleted(&elements, &id).cloned());
    let frame_id = top_layer_frame.filter(|frame| {
        !should_bind_to_container
            || container
                .as_ref()
                .is_none_or(|c| c.base.frame_id.as_deref() == Some(frame.base.id.as_str()))
    });
    let frame_id = frame_id.map(|f| f.base.id);

    let bind_container = container.as_ref().filter(|_| should_bind_to_container);
    let element = match &existing {
        Some(e) => e.clone(),
        None => {
            let sticky_container = bind_container.filter(|c| is_sticky(c));
            let text_align = if parent_center.is_some() {
                TextAlign::Center
            } else {
                parse(state.get("currentItemTextAlign")).unwrap_or_default()
            };
            let vertical_align = if parent_center.is_some() {
                VerticalAlign::Middle
            } else {
                DEFAULT_VERTICAL_ALIGN
            };
            let created = new_text_element(
                &NewTextElementOptions {
                    text: String::new(),
                    x,
                    y,
                    font_size: Some(font_size),
                    font_family: Some(font_family),
                    text_align: Some(text_align),
                    vertical_align: Some(vertical_align),
                    container_id: bind_container.map(|c| c.base.id.clone()),
                    line_height: Some(line_height),
                    label_position: bind_container
                        .filter(|c| is_arrow(c))
                        .map(|_| DEFAULT_BOUND_TEXT_LABEL_POSITION),
                    base_font_size: sticky_container.map(|_| font_size),
                    ..NewTextElementOptions::default()
                },
                &ctx.layouter.provider,
            );
            let env = &mut ctx.session.env;
            let id = env.random_id();
            let seed = env.version_nonce();
            let timestamp = env.updated();
            let mut base = ElementBase::new(id, created.x, created.y, seed, timestamp);
            base.width = created.width;
            base.height = created.height;
            base.stroke_color = match sticky_container {
                Some(c) => c.base.stroke_color.clone(),
                None => state_str(&state, "currentItemStrokeColor")
                    .unwrap_or(&base.stroke_color)
                    .to_owned(),
            };
            if let Some(color) = state_str(&state, "currentItemBackgroundColor") {
                base.background_color = color.to_owned();
            }
            if let Some(fill) = parse(state.get("currentItemFillStyle")) {
                base.fill_style = fill;
            }
            base.stroke_width = stroke_width_by_key(ElementType::Text, stroke_width_key(&state));
            if let Some(style) = parse(state.get("currentItemStrokeStyle")) {
                base.stroke_style = style;
            }
            if let Some(roughness) = parse(state.get("currentItemRoughness")) {
                base.roughness = roughness;
            }
            if let Some(opacity) = state_number(&state, "currentItemOpacity") {
                base.opacity = opacity;
            }
            base.group_ids = bind_container
                .map(|c| c.base.group_ids.clone())
                .unwrap_or_default();
            base.angle.0 = match bind_container {
                Some(c) if !is_arrow(c) => c.base.angle.0,
                _ => 0.0,
            };
            base.frame_id = frame_id;
            let mut fields = created.fields;
            if bind_container.is_none() {
                fields.label_position = Some(None);
            }
            Element::new(base, ElementKind::Text(fields))
        }
    };

    if existing.is_none() {
        if let Some(c) = bind_container {
            let mut bound = c.base.bound_elements.clone().unwrap_or_default();
            bound.push(BoundElement {
                id: element.base.id.clone(),
                kind: BoundElementType::Text,
            });
            mutate_scene(
                ctx.session,
                &c.base.id,
                one(
                    "boundElements",
                    serde_json::to_value(bound).unwrap_or(Value::Null),
                ),
            )?;
        }
    }
    ctx.session
        .set_state(one("editingTextElement", Value::Object(element.to_map())));

    if existing.is_none() {
        match bind_container {
            Some(c) => {
                let index = ctx
                    .session
                    .elements()
                    .iter()
                    .position(|e| e.base.id == c.base.id)
                    .map_or(0, |i| i + 1);
                ctx.session
                    .insert_elements_at_index(vec![element.clone()], Some(index))?;
            }
            None => insert_new_element(ctx.session, element.clone())?,
        }
    }

    if args.auto_edit || existing.is_some() || should_bind_to_container {
        let editor = TextEditor::open(
            ctx,
            element,
            existing.is_some(),
            if existing.is_some() {
                args.initial_caret
            } else {
                None
            },
        )?;
        Ok(Some(editor))
    } else {
        let mut state = one("newElement", Value::Object(element.to_map()));
        state.insert("multiElement".into(), Value::Null);
        ctx.session.set_state(state);
        ctx.session.commit();
        Ok(None)
    }
}

/// `insertNewElement(element)` (`App.tsx:7922-7961`): at the end of the
/// scene, or after the last child of its frame.
fn insert_new_element<E: HistoryEnv>(session: &mut Session<E>, element: Element) -> Result<()> {
    let index = element.base.frame_id.as_deref().and_then(|frame_id| {
        let elements = session.elements();
        (0..elements.len()).rev().find_map(|i| {
            let e = &elements[i];
            if e.base.id == frame_id {
                Some(i)
            } else if e.base.frame_id.as_deref() == Some(frame_id) {
                Some(i + 1)
            } else {
                None
            }
        })
    });
    session.insert_elements_at_index(vec![element], index)?;
    Ok(())
}

/// `scene.mutateElement(element, updates)`: whether the element changed.
fn mutate_scene<E: HistoryEnv>(
    session: &mut Session<E>,
    id: &str,
    updates: Map<String, Value>,
) -> Result<bool> {
    Ok(session.edit_elements(|map, env| mutate_in(&mut DynStamp(env), map, id, updates))?)
}

/// `updateBoundElements(element, scene)` through the host: whether it
/// changed anything.
fn update_bound_elements<E: HistoryEnv>(
    session: &mut Session<E>,
    host: &mut dyn TextEditingHost,
    id: &str,
) -> Result<bool> {
    let before: Vec<f64> = session.elements().iter().map(|e| e.base.version).collect();
    session.edit_elements(|map, env| host.update_bound_elements(&mut DynStamp(env), map, id))?;
    let after: Vec<f64> = session.elements().iter().map(|e| e.base.version).collect();
    Ok(before != after)
}

// -- the editor --------------------------------------------------------------------------

/// Where the caret goes in an existing text opened at a scene point:
/// the line and the offset along it the page's text layout must answer
/// (`getLineCaretOffsetFromNativeLayout`, `textWysiwyg.tsx:131-205`).
#[derive(Debug, Clone, PartialEq)]
pub struct CaretRequest {
    /// The line's text and where it starts in the value (UTF-16).
    pub line_text: String,
    pub line_start: usize,
    pub font: String,
    pub line_height_px: f64,
    /// `rtl` or `ltr`, the line's hard line's direction.
    pub direction: &'static str,
    /// The point's distance from the line's start, in CSS px.
    pub target_x: f64,
}

/// The layout the caret is placed in (`currentTextLayout`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct CurrentTextLayout {
    angle: f64,
    height: f64,
    line_height_px: f64,
    text_align: TextAlign,
    width: f64,
    x: f64,
    y: f64,
}

/// A keydown, as the editor reads it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyDown<'a> {
    pub key: &'a str,
    pub code: &'a str,
    pub shift_key: bool,
    pub alt_key: bool,
    /// `event[KEYS.CTRL_OR_CMD]`: metaKey on a Mac, ctrlKey elsewhere.
    pub ctrl_or_cmd: bool,
    pub is_composing: bool,
    /// `event.keyCode` (229 while an input method composes).
    pub key_code: u32,
}

/// What a paste did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteOutcome {
    /// The browser pastes (and sends `input`).
    Browser,
    /// `event.preventDefault()`: the editor pasted, or nothing is pasted.
    Prevented,
}

/// What a keydown did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyOutcome {
    /// `event.preventDefault()`: the browser must not edit the text.
    pub prevent_default: bool,
}

/// The text editor overlay (`textWysiwyg`) on one text element: the
/// textarea's value, selection and style, and the steps that keep the
/// element and the textarea in step.
#[derive(Debug, Clone)]
pub struct TextEditor {
    /// The element as editing started (`element`); its id names the text.
    element: Element,
    is_existing_element: bool,
    /// The textarea's `white-space` on open was `pre-wrap` (a label, or a
    /// text that wraps at its width).
    wraps_on_open: bool,
    value: String,
    /// `[selectionStart, selectionEnd]`, UTF-16 offsets.
    selection: (usize, usize),
    /// Every style property assigned so far, as the CSSOM keeps it.
    style: IndexMap<String, String>,
    /// The editor box's left and right insets (the sidebar's).
    editor_box_insets: (f64, f64),
    last_theme: Option<Value>,
    /// The scene, scroll and theme subscriptions are live.
    subscribed: bool,
    destroyed: bool,
    submitted_via_keyboard: bool,
    current_layout: Option<CurrentTextLayout>,
    caret_request: Option<CaretRequest>,
    pending_selection: Option<(usize, usize)>,
    calls: Vec<AppCall>,
}

impl TextEditor {
    /// `handleTextWysiwyg(element, {...})` (`App.tsx:6468-6668`) and the
    /// start of `textWysiwyg` (`textWysiwyg.tsx:207-...`): the textarea
    /// created and styled with the text's `originalText` in it (all of it
    /// selected, unless a caret point is given or on a touch screen), the
    /// other elements deselected, and the text laid out once.
    pub fn open<E: HistoryEnv, P: TextMetricsProvider>(
        ctx: &mut TextEditingContext<'_, E, P>,
        element: Element,
        is_existing_element: bool,
        initial_caret: Option<[f64; 2]>,
    ) -> Result<TextEditor> {
        let fields = text_fields(&element).cloned().unwrap_or_else(|| {
            TextFields::new(
                "",
                FontFamily::DEFAULT,
                get_line_height(FontFamily::DEFAULT),
            )
        });
        let wraps_on_open = container_id_of(&element).is_some() || !fields.auto_resize;
        let (white_space, word_break) = if wraps_on_open {
            ("pre-wrap", "break-word")
        } else {
            ("pre", "normal")
        };
        let mut editor = TextEditor {
            element,
            is_existing_element,
            wraps_on_open,
            value: fields.original_text.clone(),
            selection: (0, 0),
            style: IndexMap::new(),
            editor_box_insets: (0.0, 0.0),
            last_theme: None,
            subscribed: false,
            destroyed: false,
            submitted_via_keyboard: false,
            current_layout: None,
            caret_request: None,
            pending_selection: None,
            calls: Vec::new(),
        };
        for (key, value) in [
            ("position", "absolute"),
            ("display", "inline-block"),
            ("minHeight", "1em"),
            ("backfaceVisibility", "hidden"),
            ("margin", "0"),
            ("padding", "0"),
            ("border", "0"),
            ("outline", "0"),
            ("resize", "none"),
            ("background", "transparent"),
            ("overflow", "hidden"),
            // in dark mode the canvas creates a stacking context
            ("zIndex", "var(--zIndex-wysiwyg)"),
            ("wordBreak", word_break),
            // no line wrapping (`white-space: nowrap` does not work in Firefox)
            ("whiteSpace", white_space),
            ("overflowWrap", "break-word"),
            ("boxSizing", "content-box"),
        ] {
            editor.assign(key, value.to_owned());
        }
        // the textarea's caret is at the end of its value
        let end = len16(&editor.value);
        editor.selection = (end, end);
        editor.update_style(ctx)?;

        editor.caret_request = initial_caret.and_then(|point| editor.caret_request_at(ctx, point));
        if editor.caret_request.is_none() && !ctx.host.is_touch_screen() {
            // select on open
            editor.selection = (0, end);
        }
        // the scene, scroll and theme subscriptions
        editor.subscribed = true;

        // deselectElements(): makeNextSelectedElementIds({}, state) keeps
        // the previous object when it is empty, which is equal content
        let mut deselect = Map::new();
        deselect.insert("selectedElementIds".into(), json!({}));
        deselect.insert("selectedGroupIds".into(), json!({}));
        deselect.insert("editingGroupId".into(), Value::Null);
        deselect.insert("activeEmbeddable".into(), Value::Null);
        editor.set_state(ctx, deselect)?;

        // re-initialise the element's position, which the editor moved
        let original_text = editor.original_text();
        editor.update_element(ctx, &original_text, false)?;
        ctx.session.commit();
        Ok(editor)
    }

    fn original_text(&self) -> String {
        text_fields(&self.element)
            .map(|t| t.original_text.clone())
            .unwrap_or_default()
    }

    /// The id of the text being edited.
    pub fn element_id(&self) -> &str {
        &self.element.base.id
    }

    /// The textarea's value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// `[selectionStart, selectionEnd]` in UTF-16 code units.
    pub fn selection(&self) -> (usize, usize) {
        self.selection
    }

    /// Whether the editor is still open (not submitted).
    pub fn is_open(&self) -> bool {
        !self.destroyed
    }

    /// Every style property assigned to the textarea, by its CSSOM name
    /// (`fontSize`, `zIndex`, ...), in first-assignment order, each as the
    /// CSSOM keeps it.
    pub fn style(&self) -> &IndexMap<String, String> {
        &self.style
    }

    /// The textarea's attributes.
    pub fn attributes(&self) -> TextareaAttributes {
        TEXTAREA_ATTRIBUTES
    }

    /// The editor box's `left` and `right` (the sidebar's insets), in px.
    pub fn editor_box_insets(&self) -> (f64, f64) {
        self.editor_box_insets
    }

    /// Where the page must place the caret (`None` once placed or when not
    /// asked for).
    pub fn caret_request(&self) -> Option<&CaretRequest> {
        self.caret_request.as_ref()
    }

    /// What the editor asked of the app since the last call.
    pub fn take_calls(&mut self) -> Vec<AppCall> {
        std::mem::take(&mut self.calls)
    }

    /// The textarea's selection as the page reports it.
    pub fn set_selection(&mut self, start: usize, end: usize) {
        let len = len16(&self.value);
        let end = end.min(len);
        self.selection = (start.min(end), end);
    }

    fn assign(&mut self, key: &str, value: String) {
        self.style.insert(key.to_owned(), value);
    }

    fn style_value(&self, key: &str) -> &str {
        self.style.get(key).map_or("", String::as_str)
    }

    /// `editable.style.fontSize` and `fontFamily` as a browser gives them
    /// after `font` was assigned `${size}px ${families}`.
    fn font_longhands(&self) -> Option<(&str, &str)> {
        let font = self.style.get("font")?;
        let space = font.find(' ')?;
        Some((&font[..space], &font[space + 1..]))
    }

    /// `textPropertiesUpdated(element, editable)` (`textWysiwyg.tsx:274-293`):
    /// the text's font family or size differs from the textarea's.
    fn text_properties_updated(&self, fields: &TextFields) -> bool {
        let Some((size, family)) = self.font_longhands() else {
            return false;
        };
        if family.is_empty() || size.is_empty() {
            return false;
        }
        if get_font_family_string(fields.font_family) != family.replace('"', "") {
            return true;
        }
        px(fields.font_size) != size
    }

    // -- scene changes, as the editor's subscriptions see them ---------------------------

    /// `scene.mutateElement(element, updates)`, then, when it changed and
    /// the editor listens to the scene, the restyle `scene.onUpdate` runs.
    fn mutate<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
        id: &str,
        updates: Map<String, Value>,
    ) -> Result<bool> {
        let changed = mutate_scene(ctx.session, id, updates)?;
        if changed {
            self.scene_updated(ctx)?;
        }
        Ok(changed)
    }

    fn scene_updated<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
    ) -> Result<()> {
        if self.subscribed {
            self.update_style(ctx)?;
        }
        Ok(())
    }

    fn update_bound<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
        id: &str,
    ) -> Result<()> {
        if update_bound_elements(ctx.session, ctx.host, id)? {
            self.scene_updated(ctx)?;
        }
        Ok(())
    }

    /// `setState(state)`, then the restyle the scroll subscription runs
    /// when the scroll or zoom changed (`componentDidUpdate`).
    fn set_state<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
        state: Map<String, Value>,
    ) -> Result<()> {
        let before = ViewportState::from_app_state(ctx.session.app_state()).viewport();
        ctx.session.set_state(state);
        let after = ViewportState::from_app_state(ctx.session.app_state()).viewport();
        if before != after {
            self.scene_updated(ctx)?;
        }
        Ok(())
    }

    /// `app.viewport.translate(update)`.
    fn translate<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
        update: ViewportUpdate,
    ) -> Result<()> {
        self.calls.push(AppCall::Translate);
        let state = ViewportState::from_app_state(ctx.session.app_state());
        let translation = translate(&state, Some(update), TranslateOptions::default());
        let mut next = AppState::default();
        translation.viewport.write_to(&mut next);
        let mut keys = Map::new();
        for key in ["scrollX", "scrollY", "zoom"] {
            if let Some(v) = next.get(key) {
                keys.insert(key.to_owned(), v.clone());
            }
        }
        self.set_state(ctx, keys)
    }

    // -- updateWysiwygStyle --------------------------------------------------------------

    /// `updateWysiwygStyle()` (`textWysiwyg.tsx:297-479`): the textarea
    /// placed on the text. A label's container grows when the text is
    /// taller than its room (and the restyle that growth triggers places
    /// the editor), shrinks back towards its original height as the text
    /// gets shorter, and the label is placed in it; the text is moved to
    /// where the editor put it.
    pub fn update_style<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
    ) -> Result<()> {
        self.last_theme = ctx.session.app_state().get("theme").cloned();
        let elements = ctx.session.elements().to_vec();
        let Some(updated) = find(&elements, self.element_id()).cloned() else {
            return Ok(());
        };
        let Some(fields) = text_fields(&updated).cloned() else {
            return Ok(());
        };
        let (text_align, vertical_align) = (fields.text_align, fields.vertical_align);
        let scene = non_deleted(&elements);
        let map = ElementsMap::new(scene.iter());
        let (mut coord_x, mut coord_y) = (updated.base.x, updated.base.y);
        let container = container_id_of(&updated)
            .and_then(|id| find_non_deleted(&elements, id))
            .cloned();
        let mut width = updated.base.width;
        let mut height = updated.base.height;

        if let Some(container) = &container {
            if is_arrow(container) {
                [coord_x, coord_y] = get_bound_text_element_position(container, &updated, &map);
            }
            let max_height = get_bound_text_max_height(container, &updated);
            if is_sticky(container) {
                // the sticky fit owns the note's height; the editor only
                // mirrors the fitted label's position
                if let Some([x, y]) = compute_bound_text_position(
                    container,
                    &updated,
                    &scene,
                    &mut SceneArrowGeometry,
                ) {
                    [coord_x, coord_y] = [x, y];
                }
            } else {
                let cache = &mut ctx.layouter.container_cache;
                let id = container.base.id.as_str();
                let original_height = if self.text_properties_updated(&fields) {
                    cache.update(id, container.base.height)
                } else {
                    match cache.get(id) {
                        Some(h) => h,
                        None => cache.update(id, container.base.height),
                    }
                };
                if !is_arrow(container) && height > max_height {
                    // autogrow the container's height
                    let target = compute_container_dimension_for_bound_text(
                        height,
                        container.element_type(),
                    );
                    self.mutate(ctx, id, one("height", num(target)))?;
                    self.update_bound(ctx, id)?;
                    return Ok(());
                } else if !is_arrow(container)
                    && container.base.height > original_height
                    && height < max_height
                {
                    // autoshrink it until the original height is reached
                    let target = compute_container_dimension_for_bound_text(
                        height,
                        container.element_type(),
                    );
                    self.mutate(ctx, id, one("height", num(target)))?;
                    self.update_bound(ctx, id)?;
                } else if let Some([x, y]) = compute_bound_text_position(
                    container,
                    &updated,
                    &scene,
                    &mut SceneArrowGeometry,
                ) {
                    [coord_x, coord_y] = [x, y];
                }
            }
        }

        // what the steps above changed in place is read live from here on
        let elements = ctx.session.elements().to_vec();
        let Some(live) = find(&elements, self.element_id()).cloned() else {
            return Ok(());
        };
        let Some(live_fields) = text_fields(&live).cloned() else {
            return Ok(());
        };
        let live_container = container
            .as_ref()
            .and_then(|c| find(&elements, &c.base.id))
            .cloned();

        let state = ctx.session.app_state().clone();
        let viewport = ViewportState::from_app_state(&state);
        let (vx, vy) = scene_coords_to_viewport_coords(coord_x, coord_y, &viewport);
        let (viewport_x, viewport_y) = (vx - viewport.offset_left, vy - viewport.offset_top);
        // getTextElementAngle
        let angle = match &live_container {
            Some(c) if is_arrow(c) => 0.0,
            Some(c) => c.base.angle.0,
            None => live.base.angle.0,
        };
        if container.is_some() {
            width += 0.5;
        }
        // a 5% buffer, or the editor jumps
        height *= 1.05;

        let font = get_font_string(live_fields.font_size, live_fields.font_family);
        let sidebar = ctx.host.sidebar_insets();
        self.editor_box_insets = sidebar;
        let editor_box_left = sidebar.0;

        // a free text that stopped growing at the view's width wraps
        if !live_fields.auto_resize && self.style_value("whiteSpace") != "pre-wrap" {
            self.assign("whiteSpace", "pre-wrap".into());
            self.assign("wordBreak", "break-word".into());
        }
        let theme_dark = state_str(&state, "theme") == Some("dark");
        let assignments = [
            ("font", font.clone()),
            // after `font`
            ("lineHeight", number_to_string(live_fields.line_height)),
            ("width", px(width)),
            ("height", px(height)),
            ("left", px(viewport_x - editor_box_left)),
            ("top", px(viewport_y)),
            (
                "transformOrigin",
                format!(
                    "{} {}",
                    px(live.base.width / 2.0),
                    px(live.base.height / 2.0)
                ),
            ),
            (
                "transform",
                get_transform(live.base.width, live.base.height, angle, viewport.zoom),
            ),
            ("textAlign", align_name(text_align).to_owned()),
            (
                "verticalAlign",
                vertical_align_name(vertical_align).to_owned(),
            ),
            (
                "color",
                apply_dark_mode_filter(&live.base.stroke_color, theme_dark),
            ),
            ("opacity", number_to_string(live.base.opacity / 100.0)),
        ];
        for (key, value) in assignments {
            self.assign(key, value);
        }
        self.current_layout = Some(CurrentTextLayout {
            angle,
            height: live.base.height,
            line_height_px: get_line_height_in_px(live_fields.font_size, live_fields.line_height),
            text_align,
            width: live.base.width,
            x: coord_x,
            y: coord_y,
        });

        let mut position = one("x", num(coord_x));
        position.insert("y".into(), num(coord_y));
        self.mutate(ctx, &live.base.id, position)?;
        Ok(())
    }

    /// The caret request for a scene point (`getCaretIndexFromInitialSceneCoords`,
    /// `textWysiwyg.tsx:547-599`, up to the page's measurement): the point
    /// turned back by the text's angle about its centre, the wrapped line
    /// under it, and the point's offset from where that line starts.
    fn caret_request_at<E: HistoryEnv, P: TextMetricsProvider>(
        &self,
        ctx: &mut TextEditingContext<'_, E, P>,
        point: [f64; 2],
    ) -> Option<CaretRequest> {
        let layout = self.current_layout?;
        let center = [
            layout.x + layout.width / 2.0,
            layout.y + layout.height / 2.0,
        ];
        let [ux, uy] = rotate(point, center, -layout.angle);
        let (local_x, local_y) = (ux - layout.x, uy - layout.y);
        let font = self.style.get("font").cloned().unwrap_or_default();
        let wrap_width = if self.wraps_on_open {
            layout.width
        } else {
            f64::INFINITY
        };
        let lines = get_wrapped_text_lines(
            &self.value,
            &font,
            wrap_width,
            &ctx.layouter.provider,
            &mut ctx.layouter.char_widths,
        );
        let last = lines.len().saturating_sub(1) as f64;
        let index = js::max(
            0.0,
            js::min(last, (local_y / layout.line_height_px).floor()),
        );
        let line = lines.get(index as usize)?;
        let direction = line_direction(&self.value, line.start);
        let line_width = get_line_width(&line.text, &font, &ctx.layouter.provider);
        let line_start_x = match layout.text_align {
            TextAlign::Center => (layout.width - line_width) / 2.0,
            TextAlign::Right => layout.width - line_width,
            TextAlign::Left => 0.0,
        };
        Some(CaretRequest {
            line_text: line.text.clone(),
            line_start: line.start,
            font,
            line_height_px: layout.line_height_px,
            direction,
            target_x: local_x - line_start_x,
        })
    }

    /// The page's answer to [`Self::caret_request`]: the caret's offset in
    /// the line (`None` when the page could not measure it, which puts the
    /// caret at the line's start). It is placed once the editor is focused.
    pub fn resolve_caret(&mut self, offset_in_line: Option<usize>) {
        if let Some(request) = self.caret_request.take() {
            let index = if request.line_text.is_empty() {
                request.line_start
            } else {
                request.line_start + offset_in_line.unwrap_or(0)
            };
            self.pending_selection = Some((index, index));
        }
    }

    /// `bindBlurEvent`'s deferred part: the editor is focused and a
    /// pending caret placed (`textWysiwyg.tsx:920-948`).
    pub fn focused(&mut self) {
        if let Some((start, end)) = self.pending_selection.take() {
            self.set_selection(start, end);
        }
    }

    // -- editing -------------------------------------------------------------------------

    /// `updateElement(nextOriginalText, isDeleted)` (`App.tsx:6484-6574`):
    /// the text's `originalText` set and the text re-wrapped and
    /// re-measured (the sticky note fit for a note's label, whose note
    /// takes its half); a free text that grows stops at the view's width
    /// and the view scrolls to show it; the arrows bound to a note follow.
    fn update_element<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
        next_original_text: &str,
        is_deleted: bool,
    ) -> Result<()> {
        let elements = ctx.session.elements().to_vec();
        let Some(latest) = find(&elements, self.element_id()).cloned() else {
            return Ok(());
        };
        let Some(latest_fields) = text_fields(&latest).cloned() else {
            return Ok(());
        };
        let container = container_id_of(&latest)
            .and_then(|id| find(&elements, id))
            .cloned();
        let sticky_container = container.clone().filter(is_sticky);
        let sticky_layout = sticky_container.as_ref().map(|sticky| {
            ctx.layouter.with_layout(|layout, _| {
                get_sticky_note_layout(
                    layout,
                    sticky,
                    Some(&latest),
                    &StickyNoteLayoutOpts {
                        original_text: Some(next_original_text.to_owned()),
                        ..StickyNoteLayoutOpts::default()
                    },
                )
            })
        });
        let max_width = max_text_width(ctx.session.app_state(), ctx.host.sidebar_insets());

        let mut next = Vec::with_capacity(elements.len());
        for element in &elements {
            if let (Some(layout), Some(sticky)) = (&sticky_layout, &sticky_container) {
                if element.base.id == sticky.base.id && is_sticky(element) {
                    let with = new_element_with(
                        element,
                        layout.container.to_map(),
                        false,
                        &mut DynStamp(&mut ctx.session.env),
                    )
                    .map_err(|e| e.to_string())?;
                    next.push(with);
                    continue;
                }
            }
            if element.base.id == latest.base.id && matches!(element.kind, ElementKind::Text(_)) {
                let mut updates = one("originalText", Value::String(next_original_text.to_owned()));
                updates.insert("isDeleted".into(), Value::Bool(is_deleted));
                let rest = match sticky_layout.as_ref().and_then(|l| l.text.as_ref()) {
                    Some(label) => Some(label.to_map()),
                    None => {
                        let container = container_id_of(element).and_then(|id| find(&elements, id));
                        ctx.layouter.with_layout(|layout, _| {
                            refresh_text_dimensions(
                                layout,
                                element,
                                container,
                                &elements,
                                Some(next_original_text),
                                Some(max_width),
                            )
                            .map(|r| r.to_map())
                        })
                    }
                };
                for (key, value) in rest.into_iter().flatten() {
                    updates.insert(key, value);
                }
                let with =
                    new_element_with(element, updates, false, &mut DynStamp(&mut ctx.session.env))
                        .map_err(|e| e.to_string())?;
                next.push(with);
                continue;
            }
            next.push(element.clone());
        }
        ctx.session.replace_all_elements(next)?;
        self.scene_updated(ctx)?;

        let elements = ctx.session.elements().to_vec();
        let updated = find_non_deleted(&elements, self.element_id()).cloned();
        if let Some(updated) = updated {
            let wraps_now = text_fields(&updated).is_some_and(|t| !t.auto_resize);
            if latest_fields.auto_resize && wraps_now {
                // it just started wrapping at the view's width: bring all
                // of it into view (vertically only if it fits)
                let scene = non_deleted(&elements);
                let bounds = get_element_bounds(&updated, &ElementsMap::new(scene.iter()));
                let state = ViewportState::from_app_state(ctx.session.app_state());
                let offsets = text_viewport_offsets(ctx.host.sidebar_insets());
                if let Some((scroll_x, scroll_y)) =
                    scroll_bounds_into_view(bounds, &state, &offsets, TooLarge::Leave)
                {
                    self.translate(
                        ctx,
                        ViewportUpdate {
                            scroll_x: Some(scroll_x),
                            scroll_y: Some(scroll_y),
                            zoom: None,
                        },
                    )?;
                }
            }
        }

        if let Some(sticky) = &sticky_container {
            // the note may have grown or shrunk: the arrows bound to it follow
            if find_non_deleted(ctx.session.elements(), &sticky.base.id).is_some() {
                let id = sticky.base.id.clone();
                self.update_bound(ctx, &id)?;
            }
        }
        Ok(())
    }

    /// The textarea's `input` (`textWysiwyg.tsx:685-697`) with its new
    /// value and selection: the value normalised (line endings, tabs to
    /// spaces; the caret kept where it was) and written to the text
    /// (`onChange`), then the arrows bound to the text follow.
    pub fn input<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
        value: &str,
        selection: (usize, usize),
    ) -> Result<()> {
        if self.destroyed {
            return Ok(());
        }
        self.value = value.to_owned();
        self.set_selection(selection.0, selection.1);
        self.on_input(ctx)
    }

    fn on_input<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
    ) -> Result<()> {
        let normalized = normalize_text(&self.value);
        if normalized != self.value {
            let start = self.selection.0;
            self.value = normalized;
            self.set_selection(start, start);
        }
        let value = self.value.clone();
        self.update_element(ctx, &value, false)?;
        if !self.element.base.is_deleted {
            let id = self.element.base.id.clone();
            self.update_bound(ctx, &id)?;
        }
        ctx.session.commit();
        Ok(())
    }

    /// The textarea's `keydown` (`textWysiwyg.tsx:700-770`) with the value
    /// and selection before it.
    pub fn keydown<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
        event: &KeyDown<'_>,
    ) -> Result<KeyOutcome> {
        let prevent = KeyOutcome {
            prevent_default: true,
        };
        if self.destroyed {
            return Ok(KeyOutcome::default());
        }
        let zoom_event = ZoomKeyEvent {
            code: event.code,
            shift_key: event.shift_key,
            alt_key: event.alt_key,
            ctrl_or_cmd: event.ctrl_or_cmd,
        };
        for action in [
            ZoomAction::ZoomIn,
            ZoomAction::ZoomOut,
            ZoomAction::ResetZoom,
        ] {
            if !event.shift_key && action.key_test(&zoom_event) {
                self.execute_zoom(ctx, action)?;
                self.update_style(ctx)?;
                ctx.session.commit();
                return Ok(prevent);
            }
        }
        let font_size_keys = event.ctrl_or_cmd && event.shift_key;
        if font_size_keys && (event.key == "<" || event.key == ",") {
            self.calls.push(AppCall::ExecuteAction("decreaseFontSize"));
            return Ok(KeyOutcome::default());
        }
        if font_size_keys && (event.key == ">" || event.key == ".") {
            self.calls.push(AppCall::ExecuteAction("increaseFontSize"));
            return Ok(KeyOutcome::default());
        }
        if event.key == "Escape" {
            self.submitted_via_keyboard = true;
            self.submit(ctx)?;
            return Ok(prevent);
        }
        if event.key == "s" && event.ctrl_or_cmd && !event.shift_key {
            self.submit(ctx)?;
            self.calls.push(AppCall::ExecuteAction("saveToActiveFile"));
            return Ok(prevent);
        }
        if event.key.to_lowercase() == "s" && event.shift_key && event.ctrl_or_cmd {
            self.submit(ctx)?;
            self.calls.push(AppCall::ExecuteAction("saveFileToDisk"));
            return Ok(prevent);
        }
        if event.key == "Enter" && event.ctrl_or_cmd {
            if event.is_composing || event.key_code == 229 {
                return Ok(prevent);
            }
            self.submitted_via_keyboard = true;
            self.submit(ctx)?;
            return Ok(prevent);
        }
        if event.key == "Tab"
            || (event.ctrl_or_cmd && (event.code == "BracketLeft" || event.code == "BracketRight"))
        {
            if event.is_composing {
                return Ok(prevent);
            }
            if event.shift_key || event.code == "BracketLeft" {
                self.outdent();
            } else {
                self.indent();
            }
            // an input event resizes the element
            self.on_input(ctx)?;
            return Ok(prevent);
        }
        Ok(KeyOutcome::default())
    }

    /// `executeAction(actionZoomIn | actionZoomOut | actionResetZoom)`.
    fn execute_zoom<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
        action: ZoomAction,
    ) -> Result<()> {
        self.calls.push(AppCall::ExecuteAction(action.name()));
        let state = ViewportState::from_app_state(ctx.session.app_state());
        let elements: Vec<Element> = ctx.session.elements().to_vec();
        let refs: Vec<&Element> = elements.iter().collect();
        let viewport = perform_zoom_action(
            action,
            &state,
            &refs,
            &selected_ids(ctx.session.app_state()),
            &Offsets::default(),
        );
        let mut next = AppState::default();
        viewport.write_to(&mut next);
        let mut keys = Map::new();
        for key in ["scrollX", "scrollY", "zoom"] {
            if let Some(v) = next.get(key) {
                keys.insert(key.to_owned(), v.clone());
            }
        }
        self.set_state(ctx, keys)
    }

    /// `getSelectedLinesStartIndices()` (`textWysiwyg.tsx:824-850`): where
    /// each selected line starts, last line first.
    fn selected_lines_start_indices(&self) -> Vec<usize> {
        let (selection_start, selection_end) = self.selection;
        let before = slice16(&self.value, 0, selection_start);
        let start_offset = len16(before.rsplit('\n').next().unwrap_or(""));
        let start = selection_start - start_offset;
        let selected = slice16(&self.value, start, selection_end);
        let mut indices: Vec<usize> = Vec::new();
        let lines: Vec<&str> = selected.split('\n').collect();
        for i in 0..lines.len() {
            let index = if i == 0 {
                start
            } else {
                indices[i - 1] + len16(lines[i - 1]) + 1
            };
            indices.push(index);
        }
        indices.reverse();
        indices
    }

    /// `indent()` (`textWysiwyg.tsx:776-791`): four spaces at the start of
    /// every selected line.
    fn indent(&mut self) {
        let (start, end) = self.selection;
        let indices = self.selected_lines_start_indices();
        let mut value = self.value.clone();
        for index in &indices {
            let at = byte_at(&value, *index);
            value.insert_str(at, &" ".repeat(TAB_SIZE));
        }
        self.value = value;
        let len = len16(&self.value);
        let start = (start + TAB_SIZE).min(len);
        let end = (end + TAB_SIZE * indices.len()).min(len);
        self.selection = (start, end.max(start));
    }

    /// `outdent()` (`textWysiwyg.tsx:793-822`): up to four leading spaces
    /// removed from every selected line.
    fn outdent(&mut self) {
        let (selection_start, selection_end) = self.selection;
        let indices = self.selected_lines_start_indices();
        let mut removed: Vec<usize> = Vec::new();
        let mut value = self.value.clone();
        for index in &indices {
            let head = slice16(&value, *index, index + TAB_SIZE);
            let spaces = head.len() - head.trim_start_matches(' ').len();
            if spaces > 0 {
                let from = byte_at(&value, *index);
                value.replace_range(from..from + spaces, "");
                removed.push(*index);
            }
        }
        let changed = value != self.value;
        self.value = value;
        if changed {
            // setting a different value puts the caret at the end
            let len = len16(&self.value);
            self.selection = (len, len);
        }
        if let Some(&last) = removed.last() {
            let start = if selection_start > last {
                selection_start.saturating_sub(TAB_SIZE).max(last)
            } else {
                selection_start
            };
            let end = start.max(selection_end.saturating_sub(TAB_SIZE * removed.len()));
            let len = len16(&self.value);
            self.selection = (start.min(len), end.min(len).max(start.min(len)));
        }
    }

    /// The editor's box scrolled to reveal the caret
    /// (`onEditorBoxScroll`, `textWysiwyg.tsx:1087-1103`): the page puts
    /// the box back and the canvas pans by the offset instead, plus
    /// [`CARET_FOLLOW_PADDING`].
    pub fn editor_box_scrolled<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
        scroll_left: f64,
        scroll_top: f64,
    ) -> Result<()> {
        if self.destroyed || (scroll_left == 0.0 && scroll_top == 0.0) {
            return Ok(());
        }
        let pan_x = if scroll_left == 0.0 {
            0.0
        } else {
            scroll_left + CARET_FOLLOW_PADDING
        };
        let pan_y = if scroll_top == 0.0 {
            0.0
        } else {
            scroll_top + CARET_FOLLOW_PADDING
        };
        let state = ViewportState::from_app_state(ctx.session.app_state());
        self.translate(
            ctx,
            ViewportUpdate {
                scroll_x: Some(state.scroll_x - pan_x / state.zoom),
                scroll_y: Some(state.scroll_y - pan_y / state.zoom),
                zoom: None,
            },
        )?;
        ctx.session.commit();
        Ok(())
    }

    /// The canvas was resized (the `ResizeObserver`), or the scene changed
    /// outside the editor: the editor is placed again.
    pub fn relayout<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
    ) -> Result<()> {
        if self.subscribed {
            self.update_style(ctx)?;
            ctx.session.commit();
        }
        Ok(())
    }

    /// The app changed (`onChangeEmitter`): restyled when the theme did.
    pub fn app_changed<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
    ) -> Result<()> {
        if self.subscribed && ctx.session.app_state().get("theme").cloned() != self.last_theme {
            self.update_style(ctx)?;
            ctx.session.commit();
        }
        Ok(())
    }

    /// The textarea's `paste` (`textWysiwyg.tsx:611-683`), with the
    /// clipboard's MIME types and its `text/plain` string.
    ///
    /// - Excalidraw elements (their MIME types on the clipboard): only
    ///   their texts are pasted, joined by blank lines, at the selection
    ///   (then an `input`); nothing when they hold no text. The browser's
    ///   paste is prevented either way.
    /// - Plain text in a container: the textarea is widened at once to the
    ///   wrapped result, so the label does not jump; the browser pastes.
    pub fn paste<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
        types: &[&str],
        text_plain: Option<&str>,
    ) -> Result<PasteOutcome> {
        if self.destroyed {
            return Ok(PasteOutcome::Browser);
        }
        if types.iter().any(|t| {
            *t == excali_core::clipboard::MIME_TYPE_EXCALIDRAW_CLIPBOARD
                || *t == excali_core::constants::MIME_TYPE_EXCALIDRAW
        }) {
            if let excali_core::clipboard::ClipboardData::Elements(pasted) =
                excali_core::clipboard::parse_clipboard(text_plain, false)
            {
                // getTextFromElements(elements)
                let text = pasted
                    .elements
                    .iter()
                    .filter(|e| e.get("type").and_then(Value::as_str) == Some("text"))
                    .map(|e| e.get("text").and_then(Value::as_str).unwrap_or(""))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if !text.is_empty() {
                    let (start, end) = self.selection;
                    let (a, b) = (byte_at(&self.value, start), byte_at(&self.value, end));
                    self.value = format!("{}{}{}", &self.value[..a], text, &self.value[b..]);
                    let caret = start + len16(&text);
                    self.set_selection(caret, caret);
                    self.on_input(ctx)?;
                }
            }
            return Ok(PasteOutcome::Prevented);
        }
        let text = normalize_text(text_plain.unwrap_or(""));
        if text.is_empty() {
            return Ok(PasteOutcome::Browser);
        }
        let elements = ctx.session.elements().to_vec();
        let Some(container) = container_id_of(&self.element)
            .and_then(|id| find_non_deleted(&elements, id))
            .cloned()
        else {
            return Ok(PasteOutcome::Browser);
        };
        let bound =
            bound_text_of(&container, &elements).and_then(|id| find(&elements, &id).cloned());
        let state = ctx.session.app_state();
        let (font_size, font_family) = match (&bound, is_sticky(&container)) {
            (Some(label), true) => {
                let t = text_fields(label).expect("a text");
                (t.font_size, t.font_family)
            }
            _ => (
                state_number(state, "currentItemFontSize").unwrap_or(DEFAULT_FONT_SIZE),
                parse(state.get("currentItemFontFamily")).unwrap_or(FontFamily::DEFAULT),
            ),
        };
        let font = get_font_string(font_size, font_family);
        let max_width =
            excali_text::text_element::get_bound_text_max_width(&container, bound.as_ref());
        let (start, end) = self.selection;
        let next_text = format!(
            "{}{}{}",
            slice16(&self.value, 0, start),
            text,
            slice16(&self.value, end, len16(&self.value))
        );
        let wrapped = excali_text::text_wrapping::wrap_text(
            &next_text,
            &font,
            max_width,
            &ctx.layouter.provider,
            &mut ctx.layouter.char_widths,
        );
        let width = js::min(
            excali_text::text_measurements::get_text_width(&wrapped, &font, &ctx.layouter.provider),
            max_width,
        );
        // upstream reads the clipboard after an await on settled promises:
        // this runs in the microtask checkpoint after the listener, before
        // the browser pastes
        self.assign("width", px(width));
        Ok(PasteOutcome::Browser)
    }

    // -- submitting ----------------------------------------------------------------------

    /// `handleSubmit()` (`textWysiwyg.tsx:856-903`) and the app's
    /// `onSubmit` (`App.tsx:6592-6655`): the editor closed; the text bound
    /// to its container (or, empty, unbound) and laid out in it; the text
    /// written, deleted when empty (its bindings fixed); a keyboard submit
    /// selects the text or its container; the edit captured unless an
    /// empty new text is dropped.
    pub fn submit<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
    ) -> Result<()> {
        if self.destroyed {
            return Ok(());
        }
        self.destroyed = true;
        // cleanup: the subscriptions end, the textarea goes
        self.subscribed = false;
        let elements = ctx.session.elements().to_vec();
        let Some(update_element) = find(&elements, self.element_id()).cloned() else {
            return Ok(());
        };
        let container = container_id_of(&update_element)
            .and_then(|id| find_non_deleted(&elements, id))
            .cloned();
        let has_text = !is_blank(&self.value);
        if let Some(container) = &container {
            let id = container.base.id.clone();
            if has_text {
                let bound_id = get_bound_text_element_id(container);
                if bound_id != Some(self.element_id()) {
                    let mut bound = container.base.bound_elements.clone().unwrap_or_default();
                    bound.push(BoundElement {
                        id: self.element_id().to_owned(),
                        kind: BoundElementType::Text,
                    });
                    mutate_scene(
                        ctx.session,
                        &id,
                        one(
                            "boundElements",
                            serde_json::to_value(bound).unwrap_or(Value::Null),
                        ),
                    )?;
                } else if is_arrow(container) {
                    // an arrow label may change the arrow's bounds
                    ctx.session.edit_elements(|map, env| {
                        if let Some(arrow) = map.get_mut(&id) {
                            bump_version(arrow, None, &mut DynStamp(env));
                        }
                    });
                }
            } else {
                let bound = container.base.bound_elements.clone().map(|list| {
                    list.into_iter()
                        .filter(|b| b.kind != BoundElementType::Text)
                        .collect::<Vec<_>>()
                });
                let value = match bound {
                    Some(list) => serde_json::to_value(list).unwrap_or(Value::Null),
                    None => Value::Null,
                };
                mutate_scene(ctx.session, &id, one("boundElements", value))?;
            }
            let text_id = self.element_id().to_owned();
            let host = &mut *ctx.host;
            let layouter = &mut *ctx.layouter;
            ctx.session.edit_elements(|map, env| {
                layouter.redraw_text_bounding_box(
                    &mut DynStamp(env),
                    map,
                    &text_id,
                    Some(&id),
                    &mut |stamp, map, id| host.update_bound_elements(stamp, map, id),
                )
            })?;
        }
        self.on_submit(ctx)
    }

    fn on_submit<E: HistoryEnv, P: TextMetricsProvider>(
        &mut self,
        ctx: &mut TextEditingContext<'_, E, P>,
    ) -> Result<()> {
        let next_original_text = self.value.clone();
        let is_deleted = is_blank(&next_original_text);
        self.update_element(ctx, &next_original_text, is_deleted)?;

        let state = ctx.session.app_state().clone();
        let (tool, locked) = active_tool(&state);
        let element_id_to_select =
            if self.submitted_via_keyboard && !locked && tool != Some("autoshape") {
                match container_id_of(&self.element) {
                    Some(container) => Some(container.to_owned()),
                    None if !is_deleted => Some(self.element_id().to_owned()),
                    None => None,
                }
            } else {
                None
            };
        if let Some(id) = element_id_to_select {
            let mut ids = selected_ids(&state);
            ids.insert(id, Value::Bool(true));
            self.set_state(ctx, one("selectedElementIds", Value::Object(ids)))?;
        }
        if is_deleted {
            let element = self.element.clone();
            ctx.session.edit_elements(|map, env| {
                fix_bindings_after_deletion(map, &element, &mut DynStamp(env))
            })?;
        }
        if !is_deleted || self.is_existing_element {
            ctx.session.store.schedule_capture();
            self.calls.push(AppCall::ScheduleCapture);
        }
        let mut done = one("newElement", Value::Null);
        done.insert("editingTextElement".into(), Value::Null);
        self.set_state(ctx, done)?;
        if locked || tool == Some("autoshape") {
            self.calls.push(AppCall::ApplyToolCursor);
        }
        self.calls.push(AppCall::FocusContainer);
        ctx.session.commit();
        Ok(())
    }
}

/// `!text.trim()`.
fn is_blank(text: &str) -> bool {
    text.chars().all(|c| c.is_whitespace() || c == '\u{feff}')
}

fn align_name(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

fn vertical_align_name(align: VerticalAlign) -> &'static str {
    match align {
        VerticalAlign::Top => "top",
        VerticalAlign::Middle => "middle",
        VerticalAlign::Bottom => "bottom",
    }
}

/// `pointRotateRads(point, center, angle)`.
fn rotate(point: [f64; 2], center: [f64; 2], angle: f64) -> [f64; 2] {
    let (sin, cos) = (js::sin(angle), js::cos(angle));
    let [x, y] = [point[0] - center[0], point[1] - center[1]];
    [x * cos - y * sin + center[0], x * sin + y * cos + center[1]]
}

/// `getLineDirection(text, offset)` (`textWysiwyg.tsx:106-115`): the
/// direction of the hard line the offset is on.
fn line_direction(text: &str, offset: usize) -> &'static str {
    let units: Vec<u16> = text.encode_utf16().collect();
    let newline = u16::from(b'\n');
    let from = offset.saturating_sub(1).min(units.len());
    let start = units[..(from + 1).min(units.len())]
        .iter()
        .rposition(|u| *u == newline)
        .map_or(0, |i| i + 1);
    let end = units[offset.min(units.len())..]
        .iter()
        .position(|u| *u == newline)
        .map_or(units.len(), |i| offset + i);
    let line = String::from_utf16_lossy(&units[start..end.max(start)]);
    if is_rtl(&line) {
        "rtl"
    } else {
        "ltr"
    }
}

/// `isBindableElement(element)` (`typeChecks.ts:184-202`).
fn is_bindable(element: &Element) -> bool {
    element.is_bindable()
}

/// `fixBindingsAfterDeletion(nonDeletedElements, [deleted])`
/// (`binding.ts:2321-2335`): the deleted element taken out of the
/// `boundElements` of the live elements it was bound to
/// (`BoundElement.unbindAffected`), and the live elements bound to it
/// unbound (`BindableElement.unbindAffected`). `deleted` is the element as
/// it was when editing started.
fn fix_bindings_after_deletion(
    elements: &mut SceneElementsMap,
    deleted: &Element,
    stamp: &mut dyn ChangeStamp,
) -> std::result::Result<(), String> {
    let live = |elements: &SceneElementsMap, id: &str| -> Option<Element> {
        elements.get(id).filter(|e| !e.base.is_deleted).cloned()
    };
    let deleted_id = deleted.base.id.as_str();
    // BoundElement.unbindAffected
    for (_, bindable_id) in binding_refs(deleted) {
        let Some(bindable) = live(elements, &bindable_id) else {
            continue;
        };
        if !is_bindable(&bindable) {
            continue;
        }
        for bound in bindable.base.bound_elements.clone().unwrap_or_default() {
            if bound.id == deleted_id {
                let current = live(elements, &bindable_id)
                    .and_then(|b| b.base.bound_elements)
                    .map(|list| {
                        list.into_iter()
                            .filter(|b| b.id != deleted_id)
                            .collect::<Vec<_>>()
                    });
                let value = current.map_or(Value::Null, |list| {
                    serde_json::to_value(list).unwrap_or(Value::Null)
                });
                mutate_in(stamp, elements, &bindable_id, one("boundElements", value))?;
            }
        }
    }
    // BindableElement.unbindAffected
    if is_bindable(deleted) {
        for bound in deleted.base.bound_elements.clone().unwrap_or_default() {
            let Some(bound_element) = live(elements, &bound.id) else {
                continue;
            };
            for (prop, id) in binding_refs(&bound_element) {
                if id == deleted_id {
                    mutate_in(stamp, elements, &bound.id, one(prop, Value::Null))?;
                }
            }
        }
    }
    Ok(())
}

/// `bindableElementsVisitor`'s bindings (`binding.ts:2412-2442`): the
/// element's frame, container and arrow ends, by property.
fn binding_refs(element: &Element) -> Vec<(&'static str, String)> {
    let mut refs = Vec::new();
    if let Some(frame) = element.base.frame_id.as_deref().filter(|f| !f.is_empty()) {
        refs.push(("frameId", frame.to_owned()));
    }
    if let Some(container) = container_id_of(element) {
        refs.push(("containerId", container.to_owned()));
    }
    if let ElementKind::Arrow(arrow) = &element.kind {
        if let Some(b) = &arrow.linear.start_binding {
            refs.push(("startBinding", b.element_id.clone()));
        }
        if let Some(b) = &arrow.linear.end_binding {
            refs.push(("endBinding", b.element_id.clone()));
        }
    }
    refs
}
