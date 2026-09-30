//! The editor behind `<excali-editor>`, without the DOM: the scene with its
//! store and history, the tools, the keyboard, the pointer and the wheel,
//! and the host API of the term.hut integration page
//! (`site/content/architecture/termhut-integration.md`): `load`, `save`,
//! `export`, `importLibrary`, `getState`, and the `change`, `save-request`
//! and `open-link` events.
//!
//! Upstream counterpart: `App` (`packages/excalidraw/components/App.tsx`)
//! as the `Excalidraw` component mounts it, at the pinned commit. Every
//! step goes through the ported code:
//!
//! - **Load** is `loadFromBlob` (`data/blob.ts:137-216`,
//!   [`load_scene_json`]) then `initializeScene`'s `updateScene` without
//!   capture ([`Session::initialize_scene`]).
//! - **Save** is `serializeAsJSON(elements, appState, files, "local")`
//!   (`data/json.ts:52-75`, [`LoadedScene::to_document`]): the elements
//!   deleted ones included, as `actionSaveToActiveFile` passes them.
//! - **Keys** go through `App.onKeyDown` ([`on_key_down`]); the actions it
//!   names run their `perform` ([`Editor::perform_action`]: history, zoom,
//!   select all, the edit actions of `excali_editor::edit_actions`).
//!   Cmd+S (Ctrl+S elsewhere) is the host's: upstream's
//!   `saveToActiveFile` writes to the file handle, which the host owns, so
//!   the editor asks with `save-request`.
//! - **The viewport**: `AppPan` (the wheel or secondary button, Space held,
//!   the hand tool) and `AppWheel` ([`handle_wheel`]) through
//!   `viewport.translate`, the zoom actions through [`perform_zoom_action`].
//! - **The pointer** (`handleCanvasPointerDown`,
//!   `onPointerMoveFromPointerDownHandler`,
//!   `onPointerUpFromPointerDownHandler`): with the selection tool a press
//!   on a handle of the selection resizes or rotates it (`TransformSession`),
//!   on an element selects it (`getElementAtPosition` over `App.hitElement`,
//!   [`hit_element`]) and drags the selection ([`drag_selected_elements`]),
//!   on empty canvas draws the selection box (`getElementsWithinSelection`);
//!   the drawing tools create an element on the press, size it on the
//!   moves and finalize it on the release (`excali_editor::new_element`,
//!   an arrow's ends bound through `bindOrUnbindBindingElement`); the
//!   eraser erases what its trail crosses (`excali_editor::eraser`); the
//!   lasso selects what its path encloses or crosses
//!   (`excali_editor::lasso`) and acts as the selection tool over the
//!   selection; the text tool and the double-click edit text (`crate::text`). A press and
//!   release on an element's link icon (`isPointHittingLink`,
//!   `hyperlink/helpers.ts:61-105`) is upstream's `onLinkOpen`: the
//!   `open-link` event, the host deciding.
//! - After every step the store commits (`componentDidUpdate`); a gesture
//!   is captured on its release, and a `change` event reports whether the
//!   file [`Editor::save`] would write differs from the one loaded or last
//!   saved.
//!
//! - The interaction state the interactive canvas draws
//!   ([`Editor::interactive_scene`], `renderInteractiveScene`): the
//!   selection box (`selectionElement`), snap lines, the frame and
//!   elements to highlight, the suggested binding, the linear element
//!   editor's hovered and selected points, the locked element pressed
//!   (`activeLockedId`), the image being cropped.
//! - ex-713's interactions: dragging, drawing and resizing snap to the
//!   other elements when snapping is on (`snapDraggedElements`,
//!   `snapNewElement`, `snapResizingElements`, and the pointer's snap
//!   before a press); frame membership follows drawing, dragging and
//!   resizing (`crate::interact`); Alt+drag duplicates
//!   (`duplicateDraggedSelection`); a click with the line or arrow tool
//!   draws point by point, Enter or Escape finalizing
//!   (`crate::multi`); the selected line or arrow's points and midpoints,
//!   the line editor and elbow arrow segments (`crate::linear`); image
//!   cropping (`crate::cropping`).
//!
//! Reduced from upstream: an arrow's end binds on the release of a drag
//! rather than live while it moves; a press on an arrow's label does not
//! drag the label along the arrow; the focus point handles of bound arrow
//! ends are not offered; hovering with Alt in the line editor does not
//! preview the next point; the renderer's preview of a dragged element's
//! layer above a frame's children (`getRenderableElements`) is not drawn.
//!
//! The history's leaf layouts are the real ones ([`EditorEnv`]), so an
//! undo re-wraps and re-centres bound text and re-routes bound arrows.

use std::collections::{HashMap, HashSet};

use excali_core::app_state::{AppState, AppStateEnv};
use excali_core::constants::{DEFAULT_COLLISION_THRESHOLD, DEFAULT_TRANSFORM_HANDLE_SPACING};
use excali_core::document::{load_scene_json, LoadSceneError, LoadedScene};
use excali_core::element::{BindMode, BoundElement, BoundElementType, Element, ElementKind};
use excali_core::library::{
    merge_library_items, parse_library_json, serialize_library_as_json, LibraryItem,
    LibraryItemStatus,
};
use excali_core::library_url::{parse_library_tokens_from_url, validate_library_url};
use excali_core::restore::{
    LegacyBinding, LegacyBindingRequest, RestoreEnv, StickyNoteLayout, StickyNoteLayoutRequest,
    TextDimensionsRequest,
};
use excali_editor::actions::{
    ActionContext, ActionEnv, ActionManager, ActionName, AppProps, ContextMenuKind, KeyDownOutcome,
};
use excali_editor::binding::{
    bind_or_unbind_binding_element, BindingAppState, BindingOpts, LinearElementInitialState,
};
use excali_editor::collision::{hit_element, HitTestCache};
use excali_editor::convert_element_type::{ConvertElementTypePopup, ConvertPanel, ConvertibleType};
use excali_editor::edit_actions::duplicate::duplicate_dragged_selection;
use excali_editor::edit_actions::{
    bring_forward, bring_to_front, copy_selected, delete_selected, duplicate_selection, group,
    insert_library_items, paste_elements, select_all, selected_elements, send_backward,
    send_to_back, ungroup, ActionResult,
};
use excali_editor::eraser::EraserTrail;
use excali_editor::flowchart::{insertion_index, insertion_runs, AppFlowchart, FlowchartOperation};
use excali_editor::groups::select_groups_for_selected_elements;
use excali_editor::interactive_scene::{
    render_interactive_scene, InteractiveCanvasAppState, InteractiveScene,
};
use excali_editor::keyboard::{
    get_selected_elements, on_clipboard_event, on_key_down, on_key_up, pan_starts,
    ClipboardEventKind, ClipboardOutcome, ClipboardTarget, ConversionType, ConvertDirection,
    KeyEffect, KeyOutcome, KeyboardEditor, KeyboardState, Keystroke, PanStart,
};
use excali_editor::lasso::{LassoScene, LassoSelection, LassoTrail};
use excali_editor::linear_element_editor::create_point_at;
use excali_editor::mutate::bump_version;
use excali_editor::new_element::{
    drag_new_element, get_locked_linear_cursor_align_size, new_element_for_tool, DragNewElement,
    MINIMUM_ARROW_SIZE,
};
use excali_editor::restore_env::RoutingEnv;
use excali_editor::scene::ElementUpdate;
use excali_editor::scene::Scene;
use excali_editor::selection::{get_elements_within_selection, BoxSelectionMode};
use excali_editor::session::Session;
use excali_editor::snapping::{
    is_element_in_viewport, snap_dragged_elements, snap_new_element, snap_resizing_elements,
    SnapCache, SnapEvent,
};
use excali_editor::store::CaptureUpdateAction;
use excali_editor::tools::{PointerType, ToolState};
use excali_editor::transform::{get_grid_point, TransformModifiers, TransformSession};
use excali_editor::transform_handles::{
    EditorInterface, SelectedLinearElementState, TransformHandleType,
};
use excali_editor::viewport::{
    handle_wheel, perform_zoom_action, translate, viewport_coords_to_scene_coords,
    zoom_to_fit_bounds, AppViewport, InputDevice, Offsets, SetViewportOptions, TranslateOptions,
    Viewport, ViewportPatch, ViewportState, ViewportUpdate, WheelContext, WheelEvent, WheelTarget,
    ZoomAction, ZoomToFit,
};
use excali_math::js;
use excali_scene::bounds::{get_common_bounds, get_element_absolute_coords, ElementsMap};
use excali_scene::canvas_export::{export_canvas_png, CanvasExportOptions, CanvasSizing};
use excali_scene::display::{CanvasDocument, DisplayList};
use excali_scene::export::{svg_document, SvgExportAppState, SvgExportOptions};
use excali_scene::new_element_scene::is_invisibly_small_element;
use excali_scene::render_element::get_link_handle_from_coords;
use excali_scene::shape::Theme;
use excali_scene::static_scene::{
    render_static_scene, StaticCanvasAppState, StaticCanvasRenderConfig, StaticScene,
};
use excali_svg::{export_to_svg, to_svg_file, FontContent};
use excali_text::text_measurements::TextMetricsProvider;
use excali_ui::footer::{toggle_shortcuts, toggle_zen_mode};
use excali_ui::search_menu::{toggle_search_menu, SearchContext, SearchToggle};
use serde_json::{json, Map, Value};

use crate::cropping::CropPress;
use crate::drag::drag_selected_elements;
use crate::env::EditorEnv;
use crate::interact;
use crate::linear::{LinearPress, LinearState};
use crate::multi::MultiPoint;
use excali_editor::binding::update_bound_elements;
use excali_editor::frame::{
    add_elements_to_frame, get_elements_in_new_frame, get_frame_children_insertion_index,
};
use excali_editor::text_editing::TextEditor;
use excali_scene::frame::is_frame_like;

/// What the editor asks of its host, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent {
    /// `change`: the scene changed; `dirty` is whether [`Editor::save`]
    /// would write something other than what was loaded or last saved.
    Change { dirty: bool },
    /// `save-request`: Cmd+S (Ctrl+S) inside the editor.
    SaveRequest,
    /// `open-link`: the user opened an element's link.
    OpenLink { href: String },
}

/// `export(type, options)`'s options: the export dialog's choices.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportOptions {
    /// `exportScale` (PNG), default 1.
    pub scale: f64,
    /// `exportBackground`, default on.
    pub background: bool,
    /// `exportWithDarkMode`, default off.
    pub dark: bool,
    /// `exportEmbedScene`, default off.
    pub embed_scene: bool,
    /// `exportPadding`; `None` is upstream's default (10).
    pub padding: Option<f64>,
}

impl Default for ExportOptions {
    fn default() -> ExportOptions {
        ExportOptions {
            scale: 1.0,
            background: true,
            dark: false,
            embed_scene: false,
            padding: None,
        }
    }
}

impl ExportOptions {
    /// The options object of `export(type, options)`: `scale`,
    /// `background`, `dark`, `embedScene`, `padding`; missing keys keep
    /// their defaults.
    pub fn from_json(value: &Value) -> ExportOptions {
        let d = ExportOptions::default();
        let flag =
            |k: &str, default: bool| value.get(k).and_then(Value::as_bool).unwrap_or(default);
        ExportOptions {
            scale: value
                .get("scale")
                .and_then(Value::as_f64)
                .filter(|s| *s > 0.0)
                .unwrap_or(d.scale),
            background: flag("background", d.background),
            dark: flag("dark", d.dark),
            embed_scene: flag("embedScene", d.embed_scene),
            padding: value.get("padding").and_then(Value::as_f64),
        }
    }
}

/// What `importLibrary(textOrUrl)` was given.
#[derive(Clone, Debug, PartialEq)]
pub enum LibrarySource {
    /// `.excalidrawlib` text.
    Text,
    /// A library URL to fetch (a libraries.excalidraw.com
    /// `#addLibrary=<url>` link already resolved to `<url>`), allowed by
    /// upstream's list.
    Url(String),
}

/// Whether `input` is a library URL, and which: a link with an
/// `addLibrary` parameter is parsed as `parseLibraryTokensFromUrl` does
/// (`data/library.ts`), and every URL is held to `validateLibraryUrl`'s
/// allow-list (`ALLOWED_LIBRARY_URLS`). Anything not starting with
/// `http://` or `https://` is library text.
pub fn library_source(input: &str) -> Result<LibrarySource, String> {
    let trimmed = input.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Ok(LibrarySource::Text);
    }
    let url = parse_library_tokens_from_url(trimmed)
        .map(|t| t.library_url)
        .unwrap_or_else(|| trimmed.to_owned());
    validate_library_url(&url).map_err(|e| format!("{e}."))?;
    Ok(LibrarySource::Url(url))
}

/// The one-sentence reason `load` throws with.
fn load_error(e: &LoadSceneError) -> String {
    let cause = std::error::Error::source(e)
        .map(|s| s.to_string())
        .unwrap_or_default();
    match e {
        LoadSceneError::Json(_) => format!("The text is not valid JSON ({cause})."),
        LoadSceneError::NotAScene => "The JSON is not an Excalidraw scene.".to_owned(),
        _ => format!("The scene could not be restored ({cause})."),
    }
}

/// The editor's environment as restore's, borrowed.
struct Restore<'a, E>(&'a mut E);

impl<E: RestoreEnv> RestoreEnv for Restore<'_, E> {
    fn now(&mut self) -> f64 {
        self.0.now()
    }

    fn random_id(&mut self) -> String {
        self.0.random_id()
    }

    fn random_integer(&mut self) -> f64 {
        self.0.random_integer()
    }

    fn migrate_legacy_binding(
        &mut self,
        request: LegacyBindingRequest<'_>,
    ) -> Option<LegacyBinding> {
        self.0.migrate_legacy_binding(request)
    }

    fn refresh_text_dimensions(
        &mut self,
        request: TextDimensionsRequest<'_>,
    ) -> Option<Map<String, Value>> {
        self.0.refresh_text_dimensions(request)
    }

    fn sticky_note_layout(
        &mut self,
        request: StickyNoteLayoutRequest<'_>,
    ) -> Option<StickyNoteLayout> {
        self.0.sticky_note_layout(request)
    }
}

/// A pointer event on the canvas, as the editor reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointerInput {
    pub client_x: f64,
    pub client_y: f64,
    /// `event.button` (`POINTER_BUTTON`: 0 main, 1 wheel, 2 secondary).
    pub button: i16,
    pub shift_key: bool,
    pub alt_key: bool,
    /// `event[KEYS.CTRL_OR_CMD]`.
    pub ctrl_or_cmd: bool,
}

impl PointerInput {
    /// The main button at client coordinates, no modifier.
    pub fn at(client_x: f64, client_y: f64) -> PointerInput {
        PointerInput {
            client_x,
            client_y,
            ..PointerInput::default()
        }
    }

    /// With Shift held.
    pub fn shift(self) -> PointerInput {
        PointerInput {
            shift_key: true,
            ..self
        }
    }
}

/// A `wheel` event on the canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelInput {
    pub delta_x: f64,
    pub delta_y: f64,
    pub ctrl_key: bool,
    pub meta_key: bool,
    pub shift_key: bool,
    /// `MouseEvent.buttons`.
    pub buttons: u16,
}

/// A press on the canvas, until its release.
#[derive(Clone, Debug)]
pub(crate) enum Gesture {
    /// `AppPan`'s session (`App.pan.ts:100-285`): the last client position.
    Pan { last: [f64; 2] },
    /// The selection tool's press (`pointerDownState`).
    Select(Box<SelectGesture>),
    /// A press on a resize or rotation handle of the selection
    /// (`pointerDownState.resize`, `maybeHandleResize`).
    /// `pointerDownState.originInGrid` rides along for the snapping.
    Transform(TransformSession, [f64; 2]),
    /// A drawing tool's press: the element being drawn (`newElement`).
    Create(CreateGesture),
    /// The eraser's press: its trail and what it erases
    /// (`eraserTrail`, `elementsPendingErasure`).
    Erase {
        trail: EraserTrail,
        start: [f64; 2],
        pending: Vec<String>,
    },
    /// The lasso's press: its trail (`lassoTrail`).
    Lasso(LassoTrail),
    /// The text tool's press that started a new text (`newElement`),
    /// opened on release.
    TextCreate {
        id: String,
        /// `pointerDownState.originInGrid`.
        origin_in_grid: [f64; 2],
    },
    /// The text tool's press on an empty container's centre, decided on
    /// release (`AppTextTool.pending`).
    TextLabel { container: String, origin: [f64; 2] },
    /// A press that finished a line or arrow drawn point by point; its
    /// release reverts the tool (`App.tsx:12594-12620`).
    Finalized,
    /// A press on a crop handle of the image being cropped.
    Crop(Box<CropPress>),
    /// A press handled whole on the press (Alt adding a point in the
    /// linear element editor); its release only commits.
    Inert,
}

/// The element a drawing tool's press created (`appState.newElement`) and
/// the press (`pointerDownState`).
#[derive(Clone, Debug)]
pub(crate) struct CreateGesture {
    pub(crate) id: String,
    /// `activeTool.type`.
    pub(crate) tool: String,
    /// `pointerDownState.origin`.
    pub(crate) origin: [f64; 2],
    /// `pointerDownState.originInGrid`.
    pub(crate) origin_in_grid: [f64; 2],
    /// `pointerDownState.drag.hasOccurred` (linear elements).
    pub(crate) dragged: bool,
    /// A press while drawing point by point (`multiElement`).
    pub(crate) multi: bool,
}

/// The selection tool's press (`pointerDownState`).
#[derive(Clone, Debug)]
pub(crate) struct SelectGesture {
    /// Scene coordinates of the press.
    origin: [f64; 2],
    /// The elements at the press (`pointerDownState.originalElements`).
    originals: HashMap<String, Element>,
    /// The element hit, which the drag moves with the selection.
    hit: Option<String>,
    /// The link icon pressed, and its element.
    link: Option<(String, String)>,
    /// `previousSelectedElementIds`.
    previous_selection: Value,
    dragged: bool,
    /// The selection box's corner at the press (`selectionElement`, on
    /// the grid), while no element was hit.
    box_origin: Option<[f64; 2]>,
    /// The box has been dragged (`boxSelection.hasOccurred`).
    box_selected: bool,
    /// `pointerDownState.withCmdOrCtrl`: Ctrl/Cmd was held at the press,
    /// which blocks the drag.
    with_cmd_or_ctrl: bool,
    /// `hit.wasAddedToSelection`.
    was_added_to_selection: bool,
    /// `hit.hasBeenDuplicated`: Alt+drag duplicated the selection.
    has_been_duplicated: bool,
    /// The press on the selected line or arrow, for its editor.
    linear: Option<LinearPress>,
    /// `previousPointerMoveCoords`: the pointer at the last move.
    last_point: [f64; 2],
}

/// `getSceneVersion`: the sum of the elements' versions.
fn scene_version(elements: &[Element]) -> f64 {
    elements.iter().map(|e| e.base.version).sum()
}

/// `isBoundToContainer`.
fn is_bound_text(e: &Element) -> bool {
    matches!(&e.kind, ElementKind::Text(t) if t.container_id.is_some())
}

/// The editor over the text metrics `P`.
pub struct Editor<P: TextMetricsProvider + Clone> {
    pub(crate) session: Session<EditorEnv<P>>,
    /// The loaded file: what the save writes around the scene (its unknown
    /// top-level keys, the form of its `files`).
    pub(crate) file: LoadedScene,
    pub(crate) library: Vec<LibraryItem>,
    pub(crate) tools: ToolState,
    pub(crate) keyboard: KeyboardState,
    /// The convert element type popup's state while it is open
    /// (`keyboard.convert_popup_open`).
    pub(crate) convert_popup: ConvertElementTypePopup,
    /// `App.flowchart`: the pending nodes of Ctrl+Arrow and the Alt+Arrow
    /// walk.
    pub(crate) flowchart: AppFlowchart,
    pub(crate) actions: ActionManager,
    pub(crate) props: AppProps,
    pub(crate) action_env: ActionEnv,
    /// `getExportSource()`: the page's origin.
    pub(crate) source: String,
    /// What [`Editor::save`] wrote, or the load gave.
    pub(crate) clean: String,
    /// The viewport keys the host measured (`width`, `height`,
    /// `offsetLeft`, `offsetTop`), kept across loads.
    pub(crate) viewport: Map<String, Value>,
    pub(crate) gesture: Option<Gesture>,
    /// `viewport.lastPosition`: the last pointer position in the page,
    /// which a wheel zoom zooms around.
    pub(crate) last_pointer: [f64; 2],
    /// The animated `setViewport` navigation (`AppViewport`).
    pub(crate) navigation: AppViewport,
    pub(crate) hit_cache: HitTestCache,
    pub(crate) events: Vec<HostEvent>,
    pub(crate) reported: (f64, bool),
    /// The open text editor (`textWysiwyg`), if any.
    pub(crate) text_editor: Option<TextEditor>,
    /// `SnapCache`: the reference points and gaps of the gesture.
    pub(crate) snap_cache: SnapCache,
    /// The line or arrow drawn point by point (`multiElement`).
    pub(crate) multi: Option<MultiPoint>,
    /// Natural image sizes the host measured, by file id.
    pub(crate) image_sizes: HashMap<String, (f64, f64)>,
    /// `searchMenu` ran with the search tab open: the host focuses the
    /// search field ([`Editor::take_search_focus_request`]).
    pub(crate) search_focus_requested: bool,
}

const EMPTY_SCENE: &str =
    r#"{"type":"excalidraw","version":2,"source":"","elements":[],"appState":{},"files":{}}"#;

impl<P: TextMetricsProvider + Clone> Editor<P> {
    /// An empty scene. `env` measures text and stamps changes; `source` is
    /// `getExportSource()`; `is_darwin` picks Cmd or Ctrl.
    pub fn new(env: EditorEnv<P>, source: &str, is_darwin: bool) -> Editor<P> {
        let mut session = Session::new(env, AppState::default());
        let file = load_scene_json(
            EMPTY_SCENE,
            &mut Restore(&mut session.env),
            &AppStateEnv::default(),
        )
        .expect("the empty scene loads");
        let mut props = AppProps::default();
        props.canvas_actions.normalize(false, false);
        let mut editor = Editor {
            session,
            file,
            library: Vec::new(),
            tools: ToolState::default(),
            keyboard: KeyboardState::default(),
            convert_popup: ConvertElementTypePopup::default(),
            flowchart: AppFlowchart::default(),
            actions: ActionManager::new(),
            props,
            action_env: ActionEnv {
                is_darwin,
                ..ActionEnv::default()
            },
            source: source.to_owned(),
            clean: String::new(),
            viewport: Map::new(),
            gesture: None,
            last_pointer: [0.0, 0.0],
            navigation: AppViewport::default(),
            hit_cache: HitTestCache::new(),
            events: Vec::new(),
            reported: (0.0, false),
            text_editor: None,
            snap_cache: SnapCache::default(),
            multi: None,
            image_sizes: HashMap::new(),
            search_focus_requested: false,
        };
        editor.start(editor.file.clone());
        editor
    }

    /// The scene of `file` loaded, store and history reset, clean.
    fn start(&mut self, file: LoadedScene) {
        let mut app_state = file.app_state.clone().into_map();
        for (k, v) in &self.viewport {
            app_state.insert(k.clone(), v.clone());
        }
        // Ok: initializing only syncs indices, which restore made valid
        let _ = self
            .session
            .initialize_scene(file.elements.clone(), app_state);
        self.file = file;
        self.tools = ToolState::default();
        self.keyboard = KeyboardState::default();
        self.convert_popup.reset();
        self.flowchart.clear();
        self.gesture = None;
        self.clean = self.serialize();
        self.reported = (scene_version(self.session.elements()), false);
    }

    /// `load(text)`: the `.excalidraw` text as the scene, history cleared.
    /// The error is one sentence.
    pub fn load(&mut self, text: &str) -> Result<(), String> {
        let file = load_scene_json(
            text,
            &mut RoutingEnv::new(Restore(&mut self.session.env)),
            &AppStateEnv::default(),
        )
        .map_err(|e| load_error(&e))?;
        self.start(file);
        self.events.push(HostEvent::Change { dirty: false });
        Ok(())
    }

    fn serialize(&self) -> String {
        let mut file = self.file.clone();
        file.elements = self.session.elements().to_vec();
        file.app_state = self.session.app_state().clone();
        file.to_document(&self.source).to_json()
    }

    /// `save()`: the scene as a `.excalidraw` file (two-space JSON in
    /// upstream's key order). What it returns is the new clean state.
    pub fn save(&mut self) -> String {
        let text = self.serialize();
        self.clean.clone_from(&text);
        self.report();
        text
    }

    /// The file [`Editor::save`] would write, the clean state unchanged.
    pub fn scene_text(&self) -> String {
        self.serialize()
    }

    /// Whether the file [`Editor::save`] would write differs from the clean
    /// one.
    pub fn dirty(&self) -> bool {
        self.serialize() != self.clean
    }

    /// The elements, deleted ones included, in order.
    pub fn elements(&self) -> &[Element] {
        self.session.elements()
    }

    /// The app state.
    pub fn app_state(&self) -> &AppState {
        self.session.app_state()
    }

    /// The tools.
    pub fn tools(&self) -> &ToolState {
        &self.tools
    }

    /// The tools, for the toolbar to switch.
    pub fn tools_mut(&mut self) -> &mut ToolState {
        &mut self.tools
    }

    /// The personal library.
    pub fn library(&self) -> &[LibraryItem] {
        &self.library
    }

    /// The library as a `.excalidrawlib` file (`serializeLibraryAsJSON`).
    pub fn library_json(&self) -> String {
        serialize_library_as_json(&self.library, &self.source)
    }

    /// These items as a `.excalidrawlib` file (`serializeLibraryAsJSON`),
    /// as `saveLibraryAsJSON` and the publish dialog's submission write it.
    pub fn library_items_json(&self, items: &[LibraryItem]) -> String {
        serialize_library_as_json(items, &self.source)
    }

    pub(crate) fn selected_ids(&self) -> Vec<String> {
        self.session
            .app_state()
            .get("selectedElementIds")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter(|(_, v)| v.as_bool() == Some(true))
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `getState()`: `{ dirty, elementCount, zoom, selectionCount,
    /// activeTool }`.
    pub fn state(&self) -> Value {
        let scene = Scene::new(self.session.elements().to_vec());
        let selected = get_selected_elements(&scene, self.session.app_state(), false, false);
        json!({
            "dirty": self.dirty(),
            "elementCount": scene.non_deleted().len(),
            "zoom": self.session.app_state().zoom().unwrap_or(1.0),
            "selectionCount": selected.len(),
            "activeTool": self.tools.active_tool.tool.type_name(),
        })
    }

    /// The host events since the last call.
    pub fn take_events(&mut self) -> Vec<HostEvent> {
        std::mem::take(&mut self.events)
    }

    /// The convert popup's effects after an event (`ConvertElementTypePopup`
    /// and its Panel, `ConvertElementTypePopup.tsx:157-255`): closed when
    /// the selection empties or changes kind, its caches primed while open.
    fn sync_convert_popup(&mut self) {
        if !self.keyboard.convert_popup_open {
            self.convert_popup.reset();
            return;
        }
        let scene = Scene::new(self.session.elements().to_vec());
        self.convert_popup.sync(
            &mut self.keyboard.convert_popup_open,
            &scene,
            self.session.app_state(),
        );
    }

    /// The open convert popup's panel (`App.tsx:2770-2774`), `None` while it
    /// is closed or the selection converts to nothing.
    pub fn convert_panel(&self) -> Option<ConvertPanel> {
        if !self.keyboard.convert_popup_open {
            return None;
        }
        let scene = Scene::new(self.session.elements().to_vec());
        ConvertElementTypePopup::panel(&scene, self.session.app_state())
    }

    /// `convertElementTypes(app, { conversionType, nextType, direction })`
    /// on the scene, then `store.scheduleCapture()` when it converted
    /// (`App.tsx:5655-5665`).
    pub(crate) fn convert_element_types(
        &mut self,
        conversion: Option<ConversionType>,
        next_type: Option<ConvertibleType>,
        direction: ConvertDirection,
    ) -> bool {
        let mut scene = Scene::new(self.session.elements().to_vec());
        let mut app_state = self.session.app_state().clone();
        self.convert_popup.prime(&scene, &app_state);
        let converted = self.convert_popup.convert(
            &mut scene,
            &mut app_state,
            &mut self.tools.active_tool,
            &mut self.session.env,
            conversion,
            next_type,
            direction,
        );
        if converted {
            self.session.store.schedule_capture();
            self.apply(scene, app_state);
        }
        converted
    }

    /// A click on the convert popup's button for `kind` (`onSelect`,
    /// `ConvertElementTypePopup.tsx:307-324`): the checked type does
    /// nothing, another converts the selection to it and schedules a
    /// capture. Returns whether it converted.
    pub fn convert_popup_select(&mut self, kind: ConvertibleType) -> bool {
        let mut scene = Scene::new(self.session.elements().to_vec());
        let mut app_state = self.session.app_state().clone();
        let converted = self.convert_popup.select(
            &mut scene,
            &mut app_state,
            &mut self.tools.active_tool,
            &mut self.session.env,
            kind,
        );
        if converted {
            self.session.store.schedule_capture();
            self.apply(scene, app_state);
        }
        self.report();
        converted
    }

    /// A `change` event when the scene or the dirty flag moved since the
    /// last one.
    pub(crate) fn report(&mut self) {
        self.sync_convert_popup();
        let now = (scene_version(self.session.elements()), self.dirty());
        if now != self.reported {
            self.reported = now;
            self.events.push(HostEvent::Change { dirty: now.1 });
        }
    }

    /// The viewport the host measured: the canvas size and its offset in
    /// the page (`width`, `height`, `offsetLeft`, `offsetTop`).
    pub fn set_viewport(&mut self, width: f64, height: f64, offset_left: f64, offset_top: f64) {
        let keys = [
            ("width", width),
            ("height", height),
            ("offsetLeft", offset_left),
            ("offsetTop", offset_top),
        ];
        let mut patch = Map::new();
        for (k, v) in keys {
            patch.insert(k.to_owned(), json!(v));
        }
        self.viewport.clone_from(&patch);
        self.session.set_state(patch);
        self.session.commit();
    }

    /// `appState.theme`: `"light"` or `"dark"`.
    pub fn set_theme(&mut self, dark: bool) {
        let mut patch = Map::new();
        patch.insert("theme".into(), json!(if dark { "dark" } else { "light" }));
        self.session.set_state(patch);
        self.session.commit();
    }

    /// The working scene written back: the elements when they changed, the
    /// app state keys that changed, then the commit.
    pub(crate) fn apply(&mut self, scene: Scene, app_state: AppState) {
        if scene.elements() != self.session.elements() {
            // Ok: the scene only moved elements, their indices stay valid
            let _ = self.session.replace_all_elements(scene.elements().to_vec());
        }
        let current = self.session.app_state().as_map();
        let patch: Map<String, Value> = app_state
            .into_map()
            .into_iter()
            .filter(|(k, v)| current.get(k) != Some(v))
            .collect();
        if !patch.is_empty() {
            self.session.set_state(patch);
        }
        self.session.commit();
    }

    /// Whether `stroke` is Cmd+S (Ctrl+S elsewhere) without Shift.
    fn is_save(&self, stroke: &Keystroke) -> bool {
        stroke.key.eq_ignore_ascii_case("s")
            && stroke.modifiers.ctrl_or_cmd(self.action_env.is_darwin)
            && !stroke.modifiers.shift_key
            && !stroke.modifiers.alt_key
            && !stroke.target.writable
    }

    /// The document's `keydown`: `App.onKeyDown`, then the undo or redo it
    /// names; Cmd+S is `save-request`.
    pub fn key_down(&mut self, stroke: &Keystroke) -> KeyOutcome {
        if self.is_save(stroke) {
            self.events.push(HostEvent::SaveRequest);
            return KeyOutcome {
                prevent_default: true,
                stop_propagation: true,
                effects: Vec::new(),
            };
        }
        // image cropping's keys (App.tsx:5616-5636)
        if !stroke.target.writable && !stroke.target.input_like {
            let enter = stroke.key == "Enter";
            if (enter || stroke.key == "Escape") && self.cropping_id().is_some() {
                self.finish_image_cropping();
                return KeyOutcome::default();
            }
            if enter {
                if let Some(id) = self.selected_image() {
                    self.start_image_cropping(&id);
                    return KeyOutcome::default();
                }
            }
        }
        let mut scene = Scene::new(self.session.elements().to_vec());
        let mut app_state = self.session.app_state().clone();
        let out = {
            let mut ed = KeyboardEditor {
                scene: &mut scene,
                app_state: &mut app_state,
                tools: &mut self.tools,
                keyboard: &mut self.keyboard,
                actions: &self.actions,
                props: &self.props,
                env: &self.action_env,
            };
            on_key_down(&mut ed, &mut self.session.env, stroke)
        };
        let ops = self.answer_flowchart(&out, &mut scene, &app_state);
        self.apply(scene, app_state);
        self.apply_flowchart(ops);
        for effect in &out.effects {
            match effect {
                KeyEffect::Action(KeyDownOutcome::Perform(name)) => self.perform_action(*name),
                KeyEffect::ExecuteAction(name) => self.perform_action(*name),
                KeyEffect::Scrolled(translation) => self.set_viewport_to(translation.viewport),
                KeyEffect::ConvertElementType {
                    conversion,
                    direction,
                } => {
                    self.convert_element_types(*conversion, None, *direction);
                }
                _ => {}
            }
        }
        self.report();
        out
    }

    /// The document's `keyup`: `App.onKeyUp`.
    pub fn key_up(&mut self, stroke: &Keystroke) -> KeyOutcome {
        let mut scene = Scene::new(self.session.elements().to_vec());
        let mut app_state = self.session.app_state().clone();
        let out = {
            let mut ed = KeyboardEditor {
                scene: &mut scene,
                app_state: &mut app_state,
                tools: &mut self.tools,
                keyboard: &mut self.keyboard,
                actions: &self.actions,
                props: &self.props,
                env: &self.action_env,
            };
            on_key_up(&mut ed, &mut self.session.env, stroke)
        };
        let ops = self.answer_flowchart(&out, &mut scene, &app_state);
        self.flowchart.after_key_up(&self.keyboard.flowchart);
        self.apply(scene, app_state);
        self.apply_flowchart(ops);
        self.report();
        out
    }

    /// `AppFlowchart.resolveKeyboardEventToOperation`: the creator and the
    /// navigator answer the keyboard's flowchart effects on the working
    /// scene (a Ctrl+Arrow binds the new arrows to the start node there).
    fn answer_flowchart(
        &mut self,
        out: &KeyOutcome,
        scene: &mut Scene,
        app_state: &AppState,
    ) -> Vec<FlowchartOperation> {
        out.effects
            .iter()
            .filter_map(|effect| {
                self.flowchart.answer(
                    effect,
                    scene,
                    app_state,
                    &mut self.keyboard.flowchart,
                    &mut self.session.env,
                )
            })
            .collect()
    }

    /// `AppFlowchart.handleKeyEvent`'s cases (`App.flowchart.ts:53-100`).
    fn apply_flowchart(&mut self, ops: Vec<FlowchartOperation>) {
        for op in ops {
            match op {
                FlowchartOperation::Canceled => {}
                FlowchartOperation::Creating { pending } => self.reveal_if_hidden(&pending),
                FlowchartOperation::Navigating { node_id } => {
                    if let Some(id) = node_id {
                        self.select_and_reveal(&id);
                    }
                }
                FlowchartOperation::Committed { nodes } => {
                    // one update: the inserted nodes, the selection and the
                    // capture (syncActionResult IMMEDIATELY)
                    self.session.store.schedule_capture();
                    let first = nodes.first().map(|n| n.base.id.clone());
                    // insertNewElements: each run of one frame above the
                    // frame's children, the rest on top
                    for run in insertion_runs(nodes) {
                        let at = insertion_index(self.session.elements(), &run);
                        // Ok: the indices are generated between neighbours
                        let _ = self.session.insert_elements_at_index(run, at);
                    }
                    if let Some(id) = first {
                        self.select_and_reveal(&id);
                    }
                    self.session.commit();
                }
                FlowchartOperation::NavigationEnded => {
                    self.session.store.schedule_capture();
                    self.session.commit();
                }
            }
        }
    }

    /// `AppFlowchart.selectAndReveal(node)`: the node selected alone, then
    /// revealed.
    fn select_and_reveal(&mut self, id: &str) {
        let Some(node) = self
            .session
            .elements()
            .iter()
            .find(|e| e.base.id == id)
            .cloned()
        else {
            return;
        };
        let mut patch = Map::new();
        patch.insert("selectedElementIds".into(), json!({ id: true }));
        self.session.set_state(patch);
        self.reveal_if_hidden(&[node]);
        self.session.commit();
    }

    /// `App.revealIfHidden(elements)` (`App.tsx:5286-5307`): unless their
    /// common bounds lie in the viewport (`isElementCompletelyInViewport`),
    /// the viewport fits them, scaling down only. The port does not animate
    /// the move, and fits within the whole canvas (upstream leaves out the
    /// UI's insets, `offsets: { ui: true }`).
    fn reveal_if_hidden(&mut self, elements: &[Element]) {
        if elements.is_empty() {
            return;
        }
        let refs: Vec<&Element> = elements.iter().collect();
        let [x1, y1, x2, y2] = get_common_bounds(&refs);
        let state = self.viewport_state();
        let top_left = viewport_coords_to_scene_coords(state.offset_left, state.offset_top, &state);
        let bottom_right = viewport_coords_to_scene_coords(
            state.offset_left + state.width,
            state.offset_top + state.height,
            &state,
        );
        if x1 >= top_left.0 && y1 >= top_left.1 && x2 <= bottom_right.0 && y2 <= bottom_right.1 {
            return;
        }
        let viewport = zoom_to_fit_bounds(&ZoomToFit::new([x1, y1, x2, y2]), &state);
        self.set_viewport_to(viewport);
    }

    /// Whether there is something to undo (`!history.isUndoStackEmpty`).
    pub fn can_undo(&self) -> bool {
        !self.session.history.is_undo_stack_empty()
    }

    /// Whether there is something to redo.
    pub fn can_redo(&self) -> bool {
        !self.session.history.is_redo_stack_empty()
    }

    /// Runs the action `name` as the chrome does (`executeAction`): the
    /// history, the zoom actions, and the app state toggles of the help
    /// dialog, zen mode and the stats panel.
    pub fn perform_action(&mut self, name: ActionName) {
        match name {
            ActionName::Undo => return self.undo(),
            ActionName::Redo => return self.redo(),
            ActionName::Finalize => return self.finalize(None),
            ActionName::ToggleLinearEditor => return self.toggle_linear_editor(),
            ActionName::SearchMenu => return self.toggle_search_menu(),
            _ => {}
        }
        if let Some(action) = ZoomAction::from_name(name.as_str()) {
            self.zoom(action);
        } else {
            let mut app_state = self.session.app_state().clone();
            let elements = self.session.elements().to_vec();
            let env = &mut self.session.env;
            let result = match name {
                ActionName::SelectAll => select_all(&elements, &app_state),
                ActionName::DeleteSelectedElements => {
                    let result = delete_selected(&elements, &app_state, env);
                    if result.is_some() {
                        // updateActiveTool(appState, { type: preferredSelectionTool })
                        self.tools.active_tool = self.tools.tool_after_finalize();
                    }
                    result
                }
                ActionName::DuplicateSelection => duplicate_selection(&elements, &app_state, env),
                ActionName::Group => group(&elements, &app_state, env),
                ActionName::Ungroup => ungroup(&elements, &app_state, env),
                ActionName::BringToFront => bring_to_front(&elements, &app_state, env),
                ActionName::BringForward => bring_forward(&elements, &app_state, env),
                ActionName::SendToBack => send_to_back(&elements, &app_state, env),
                ActionName::SendBackward => send_backward(&elements, &app_state, env),
                ActionName::Copy => {
                    self.copy();
                    return;
                }
                ActionName::Cut => {
                    self.cut();
                    return;
                }
                ActionName::ToggleShortcuts => {
                    toggle_shortcuts(&mut app_state);
                    None
                }
                ActionName::ZenMode => {
                    toggle_zen_mode(&mut app_state);
                    None
                }
                ActionName::Stats => {
                    // actionToggleStats.perform (actionToggleStats.tsx:17-24)
                    let mut stats = app_state.get("stats").cloned().unwrap_or(json!({}));
                    let open = stats.get("open").and_then(Value::as_bool).unwrap_or(false);
                    stats["open"] = json!(!open);
                    app_state.insert("stats", stats);
                    None
                }
                ActionName::ObjectsSnapMode => {
                    // actionToggleObjectsSnapMode.perform
                    // (actionToggleObjectsSnapMode.tsx:18-27)
                    let on = app_state
                        .get("objectsSnapModeEnabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    app_state.insert("objectsSnapModeEnabled", json!(!on));
                    app_state.insert("gridModeEnabled", json!(false));
                    None
                }
                _ => return,
            };
            if let Some(result) = result {
                return self.apply_action(result);
            }
            let scene = Scene::new(self.session.elements().to_vec());
            self.apply(scene, app_state);
        }
        self.report();
    }

    /// `actionToggleSearchMenu.perform` (`actionToggleSearchMenu.ts:26-51`):
    /// the default sidebar opened on its search tab (no capture), or, when
    /// that tab is open, the search field focused by the host.
    fn toggle_search_menu(&mut self) {
        match toggle_search_menu(self.session.app_state().as_map()) {
            SearchToggle::Open(patch) => self.set_app_state(patch),
            SearchToggle::FocusInput => self.search_focus_requested = true,
            SearchToggle::None => {}
        }
    }

    /// Whether `searchMenu` asked for the search field's focus since the
    /// last call.
    pub fn take_search_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.search_focus_requested)
    }

    /// The search menu's `app` (`SearchMenu.tsx`) for `f`: the scene, its
    /// nonce (the scene version), `app.visibleElements` (the elements in
    /// the viewport, `isElementInViewport`), the view, `padding` as
    /// `app.viewport.getOffsets()`, and the text metrics.
    pub fn with_search_context<R>(
        &self,
        padding: Offsets,
        f: impl FnOnce(&SearchContext<'_>) -> R,
    ) -> R {
        let elements = self.session.elements();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let view = self.viewport_state();
        let visible: Vec<String> = live
            .iter()
            .filter(|e| is_element_in_viewport(e, &view, &map))
            .map(|e| e.base.id.clone())
            .collect();
        let cx = SearchContext {
            elements,
            scene_nonce: scene_version(elements).to_bits(),
            visible_ids: &visible,
            view,
            padding,
            metrics: &self.session.env.layouter.provider,
        };
        f(&cx)
    }

    /// `app.viewport.setViewport(opts)` for a box (`App.viewport.ts:
    /// 672-760`, [`AppViewport::set_viewport`]): the viewport that fits it
    /// (with the scroll lock asked for), set at once or, animated, its
    /// first frame; the host then runs [`Editor::viewport_frame`] on each
    /// animation frame while [`Editor::is_viewport_animating`].
    pub fn navigate_to(&mut self, opts: SetViewportOptions) {
        let state = self.viewport_state();
        if let Some(patch) = self.navigation.set_viewport(&state, &opts) {
            self.apply_viewport_patch(&patch);
        }
    }

    /// An animation frame of the navigation at `now` (ms, the frame's
    /// `performance.now()` timestamp).
    pub fn viewport_frame(&mut self, now: f64) {
        if let Some(patch) = self.navigation.frame(now) {
            self.apply_viewport_patch(&patch);
        }
    }

    /// Whether a navigation is running.
    pub fn is_viewport_animating(&self) -> bool {
        self.navigation.is_animating()
    }

    /// A navigation step's keys set, without capture.
    fn apply_viewport_patch(&mut self, patch: &ViewportPatch) {
        let current = self.session.app_state().as_map();
        let changed: Map<String, Value> = patch
            .to_map()
            .into_iter()
            .filter(|(k, v)| current.get(k) != Some(v))
            .collect();
        if !changed.is_empty() {
            self.session.set_state(changed);
            self.session.commit();
        }
        self.report();
    }

    /// The start of a user pan or zoom (`AppViewport.translate`): `false`
    /// while a navigation into a locked viewport is pending (the
    /// translation is dropped); a running navigation stops and
    /// `shouldCacheIgnoreZoom` goes back to `false`.
    fn interrupt_navigation(&mut self) -> bool {
        match self.navigation.interrupt() {
            None => false,
            Some(stopped) => {
                if stopped {
                    let mut patch = Map::new();
                    patch.insert("shouldCacheIgnoreZoom".into(), Value::Bool(false));
                    self.session.set_state(patch);
                }
                true
            }
        }
    }

    /// An action's result written back (`syncActionResult`): the
    /// elements, the app state keys, and a capture when it asks for one.
    fn apply_action(&mut self, result: ActionResult) {
        if let Some(elements) = result.elements {
            // Ok: the actions keep the fractional indices valid
            let _ = self.session.replace_all_elements(elements);
        }
        let current = self.session.app_state().as_map();
        let patch: Map<String, Value> = result
            .app_state
            .into_iter()
            .filter(|(k, v)| current.get(k) != Some(v))
            .collect();
        if !patch.is_empty() {
            self.session.set_state(patch);
        }
        if result.capture {
            self.session.store.schedule_capture();
        }
        self.session.commit();
        self.report();
    }

    /// `actionCopy` (`actions/actionClipboard.tsx`, `copyToClipboard`): the
    /// selection (with its bound text and frame children) as upstream's
    /// clipboard JSON (an empty selection copies no elements, as upstream's
    /// does).
    pub fn copy(&mut self) -> Option<String> {
        let files = self.file.files.as_object().cloned();
        let elements = self.session.elements().to_vec();
        let app_state = self.session.app_state().clone();
        Some(copy_selected(
            &elements,
            &app_state,
            files.as_ref(),
            &mut self.session.env,
        ))
    }

    /// `actionCut`: the selection copied, then deleted.
    pub fn cut(&mut self) -> Option<String> {
        let text = self.copy()?;
        self.perform_action(ActionName::DeleteSelectedElements);
        Some(text)
    }

    /// `pasteFromClipboard` with the clipboard's `text/plain`: upstream's
    /// clipboard elements inserted centred on the pointer
    /// (`addElementsFromPasteOrLibrary`) and selected. Text that is not
    /// elements pastes nothing (upstream makes a text element of it).
    pub fn paste(&mut self, text: &str, _plain: bool) {
        let [x, y] = self.last_pointer;
        let pointer = self.scene_point(x, y);
        let grid = self.grid_size(false);
        let elements = self.session.elements().to_vec();
        let app_state = self.session.app_state().clone();
        if let Some(result) = paste_elements(
            text,
            &elements,
            &app_state,
            pointer,
            grid,
            &mut self.session.env,
        ) {
            self.tools.active_tool = self.tools.tool_after_finalize();
            self.apply_action(result);
        }
    }

    /// Inserts the library's items `ids` (in library order), as a click in
    /// the library sidebar (`onInsertElements`, at the viewport's centre,
    /// `client` `None`) or a drop on the canvas (at the drop's client
    /// point) does: duplicated, laid out on a square grid, added and
    /// selected ([`insert_library_items`]); the sidebar stays open only
    /// when `sidebar_docked_and_fits`. Returns whether anything was added.
    pub fn insert_library(
        &mut self,
        ids: &[String],
        client: Option<[f64; 2]>,
        sidebar_docked_and_fits: bool,
    ) -> bool {
        let items: Vec<Vec<Element>> = self
            .library
            .iter()
            .filter(|i| ids.contains(&i.id))
            .map(|i| i.elements.clone())
            .collect();
        let app_state = self.session.app_state().clone();
        let [x, y] = client.unwrap_or_else(|| {
            let n = |k: &str| app_state.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            [
                n("width") / 2.0 + n("offsetLeft"),
                n("height") / 2.0 + n("offsetTop"),
            ]
        });
        let pointer = self.scene_point(x, y);
        let grid = self.grid_size(false);
        let elements = self.session.elements().to_vec();
        match insert_library_items(
            &items,
            &elements,
            &app_state,
            pointer,
            grid,
            sidebar_docked_and_fits,
            &mut self.session.env,
        ) {
            Some(result) => {
                self.tools.active_tool = self.tools.tool_after_finalize();
                self.apply_action(result);
                true
            }
            None => false,
        }
    }

    /// Runs `f` with the editor's id and clock (`randomId()`, `Date.now()`),
    /// as the library sidebar's handlers draw them.
    pub fn with_restore_env<R>(&mut self, f: impl FnOnce(&mut dyn RestoreEnv) -> R) -> R {
        f(&mut Restore(&mut self.session.env))
    }

    /// The library's items after the sidebar changed them (an item added,
    /// items removed, the library reset).
    pub fn set_library(&mut self, items: Vec<LibraryItem>) {
        self.library = items;
    }

    /// The elements the library offers to add (`getPendingElements`): the
    /// selection with its bound text and frames' children.
    pub fn pending_library_elements(&self) -> Vec<Element> {
        let selected = self
            .session
            .app_state()
            .get("selectedElementIds")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        selected_elements(self.session.elements(), &selected, true, true)
    }

    /// A library item's preview (`exportLibraryItemToSvg`,
    /// `hooks/useLibraryItemSvg.ts`): `exportToSvg` of its elements with no
    /// background on white, no embeddables, fonts not inlined; the `<svg>`
    /// element's markup.
    pub fn library_item_svg(&self, elements: &[Element]) -> String {
        self.item_svg(elements, false)
    }

    /// A publish dialog item's preview (`SingleLibraryItem`,
    /// `PublishLibrary.tsx:124-141`): `exportToSvg` of its elements on the
    /// dialog's white background, fonts not inlined.
    pub fn publish_item_svg(&self, elements: &[Element]) -> String {
        self.item_svg(elements, true)
    }

    fn item_svg(&self, elements: &[Element], background: bool) -> String {
        struct NoFonts;
        impl FontContent for NoFonts {
            fn content(&self, _: &excali_scene::display::FontFaceSource) -> String {
                String::new()
            }
        }
        let mut state = SvgExportAppState::new("#ffffff");
        state.export_background = background;
        let metrics = Measure(&self.session.env.layouter.provider);
        let mut options = SvgExportOptions::new(&self.source, &metrics);
        options.clock = self.session.env.render_clock();
        options.skip_inlining_fonts = true;
        options.render_embeddables = false;
        let document = svg_document(elements, &state, None, &options);
        export_to_svg(&document, &NoFonts).outer_html()
    }

    /// `app.activeResizeHandle`: the transform handle being dragged, as
    /// `TransformHandleType` names it (`"se"`, `"rotation"`).
    pub fn active_resize_handle(&self) -> Option<&'static str> {
        match self.gesture.as_ref() {
            Some(Gesture::Transform(session, _)) => session.handle().map(|h| h.as_str()),
            _ => None,
        }
    }

    /// `viewport.lastPosition`: where the pointer last was, in the page.
    pub fn last_pointer(&self) -> [f64; 2] {
        self.last_pointer
    }

    /// `App.onCopy`, `App.onCut` and the gate of `pasteFromClipboard`
    /// ([`on_clipboard_event`]) for a document clipboard event.
    pub fn clipboard_outcome(
        &self,
        kind: ClipboardEventKind,
        target: ClipboardTarget,
    ) -> ClipboardOutcome {
        on_clipboard_event(&self.tools, &self.keyboard, kind, target)
    }

    /// The actions' context: the scene, the app state, the props.
    pub fn action_context(&self) -> ActionContext<'_> {
        ActionContext {
            elements: self.session.elements(),
            app_state: self.session.app_state(),
            props: &self.props,
            env: &self.action_env,
        }
    }

    /// The action manager.
    pub fn action_manager(&self) -> &ActionManager {
        &self.actions
    }

    /// `setState(patch)` from the chrome (a menu opening or closing):
    /// the keys merged, nothing captured.
    pub fn set_app_state(&mut self, patch: Map<String, Value>) {
        let current = self.session.app_state().as_map();
        let patch: Map<String, Value> = patch
            .into_iter()
            .filter(|(k, v)| current.get(k) != Some(v))
            .collect();
        if !patch.is_empty() {
            self.session.set_state(patch);
            self.session.commit();
        }
        self.report();
    }

    /// The undo action.
    pub fn undo(&mut self) {
        // Err: only debug builds' checks fail an undo; the scene stays
        let _ = self.session.undo();
        self.report();
    }

    /// The redo action.
    pub fn redo(&mut self) {
        let _ = self.session.redo();
        self.report();
    }

    pub(crate) fn scene_point(&self, client_x: f64, client_y: f64) -> [f64; 2] {
        let state = ViewportState::from_app_state(self.session.app_state());
        let (x, y) = viewport_coords_to_scene_coords(client_x, client_y, &state);
        [x, y]
    }

    /// `hasBoundingBox(selectedElements, appState)`
    /// (`transformHandles.ts`): more than one element, or one that is not
    /// a line or an arrow, or a line or non-elbow arrow of more than two
    /// points.
    pub(crate) fn has_bounding_box(selected: &[&Element]) -> bool {
        match selected {
            [] => false,
            [one] => match &one.kind {
                ElementKind::Arrow(a) if a.elbowed => false,
                _ => one.kind.linear().is_none_or(|l| l.points.len() > 2),
            },
            _ => true,
        }
    }

    /// `getElementAtPosition(x, y)`: the topmost element hit, bound text
    /// counting as its container's.
    pub(crate) fn element_at(&mut self, point: [f64; 2]) -> Option<String> {
        self.element_at_with(point, false)
    }

    /// `getElementAtPosition(x, y, { includeLockedElements })`.
    pub(crate) fn element_at_with(
        &mut self,
        point: [f64; 2],
        include_locked: bool,
    ) -> Option<String> {
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let selected: Vec<String> = self.selected_ids();
        let elements = self.session.elements();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let selected_elements: Vec<&Element> = live
            .iter()
            .copied()
            .filter(|e| selected.contains(&e.base.id))
            .collect();
        let with_box = Self::has_bounding_box(&selected_elements);
        live.iter()
            .rev()
            .filter(|e| !is_bound_text(e) && (include_locked || !e.base.locked))
            .find(|e| {
                hit_element(
                    &mut self.hit_cache,
                    point,
                    e,
                    &map,
                    zoom,
                    true,
                    with_box && selected.contains(&e.base.id),
                    None,
                )
            })
            .map(|e| e.base.id.clone())
    }

    /// `isHittingCommonBoundingBoxOfSelectedElements` (`App.tsx:9964-9986`):
    /// two or more selected elements whose common bounds, padded by the
    /// transform handles' spacing and the collision threshold, hold `point`.
    fn is_hitting_common_bounding_box(&self, point: [f64; 2], selected: &[&Element]) -> bool {
        if selected.len() < 2 {
            return false;
        }
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let threshold = (DEFAULT_COLLISION_THRESHOLD / zoom).max(1.0);
        let padding = (DEFAULT_TRANSFORM_HANDLE_SPACING * 2.0) / zoom;
        let [x1, y1, x2, y2] = get_common_bounds(selected);
        let [x, y] = point;
        x > x1 - padding - threshold
            && x < x2 + padding + threshold
            && y > y1 - padding - threshold
            && y < y2 + padding + threshold
    }

    /// `App.openContextMenu` (`App.tsx:13387-13465`) for a pointer at
    /// `client`: the element menu over an element or the selection's
    /// common bounding box, else the canvas menu. An element that is not
    /// selected becomes the selection (with its groups,
    /// `selectGroupsForSelectedElements`), and the hyperlink popup closes.
    pub fn open_context_menu(&mut self, client_x: f64, client_y: f64) -> ContextMenuKind {
        let point = self.scene_point(client_x, client_y);
        let hit = self.element_at_with(point, true);
        let selected_ids = self.selected_ids();
        let hitting_box = {
            let selected: Vec<&Element> = self
                .session
                .elements()
                .iter()
                .filter(|e| !e.base.is_deleted && selected_ids.contains(&e.base.id))
                .collect();
            self.is_hitting_common_bounding_box(point, &selected)
        };
        let mut patch = Map::new();
        if let Some(id) = hit.as_ref().filter(|id| !selected_ids.contains(id)) {
            let live: Vec<&Element> = self
                .session
                .elements()
                .iter()
                .filter(|e| !e.base.is_deleted)
                .collect();
            let mut selection = Map::new();
            selection.insert(id.clone(), Value::Bool(true));
            let editing = self
                .session
                .app_state()
                .get("editingGroupId")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let groups = select_groups_for_selected_elements(&selection, editing.as_deref(), &live);
            patch.insert(
                "selectedElementIds".into(),
                Value::Object(groups.selected_element_ids),
            );
            patch.insert(
                "selectedGroupIds".into(),
                Value::Object(groups.selected_group_ids),
            );
            patch.insert(
                "editingGroupId".into(),
                groups.editing_group_id.map_or(Value::Null, Value::String),
            );
        }
        patch.insert("showHyperlinkPopup".into(), Value::Bool(false));
        let current = self.session.app_state().as_map();
        patch.retain(|k, v| current.get(k) != Some(v));
        if !patch.is_empty() {
            self.session.set_state(patch);
            self.session.commit();
        }
        self.report();
        if hit.is_some() || hitting_box {
            ContextMenuKind::Element
        } else {
            ContextMenuKind::Canvas
        }
    }

    /// `isPointHittingLink(element, elementsMap, appState, point)` for the
    /// topmost element whose link icon holds `point`: its id and link.
    fn link_at(&self, point: [f64; 2]) -> Option<(String, String)> {
        let zoom = self.session.app_state().zoom().unwrap_or(1.0);
        let selected = self.selected_ids();
        let elements = self.session.elements();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        live.iter().rev().find_map(|e| {
            let link = e.base.link.as_deref().filter(|l| !l.is_empty())?;
            if selected.contains(&e.base.id) {
                return None;
            }
            let [x1, y1, x2, y2, ..] = get_element_absolute_coords(e, &map, false);
            let [lx, ly, lw, lh] =
                get_link_handle_from_coords([x1, y1, x2, y2], e.base.angle.0, zoom);
            let threshold = 4.0 / zoom;
            let [x, y] = point;
            (x > lx - threshold
                && x < lx + threshold + lw
                && y > ly - threshold
                && y < ly + lh + threshold)
                .then(|| (e.base.id.clone(), link.to_owned()))
        })
    }

    pub(crate) fn set_selection(&mut self, ids: &[String]) {
        let selection: Map<String, Value> = ids
            .iter()
            .map(|id| (id.clone(), Value::Bool(true)))
            .collect();
        let mut patch = Map::new();
        patch.insert("selectedElementIds".into(), Value::Object(selection));
        self.session.set_state(patch);
    }

    /// The viewport as the app state holds it.
    fn viewport_state(&self) -> ViewportState {
        ViewportState::from_app_state(self.session.app_state())
    }

    /// `scrollX`, `scrollY` and `zoom` set, without capture (the viewport
    /// is not history).
    fn set_viewport_to(&mut self, viewport: Viewport) {
        let mut app_state = self.session.app_state().clone();
        viewport.write_to(&mut app_state);
        let current = self.session.app_state().as_map();
        let patch: Map<String, Value> = app_state
            .into_map()
            .into_iter()
            .filter(|(k, v)| current.get(k) != Some(v))
            .collect();
        if !patch.is_empty() {
            self.session.set_state(patch);
            self.session.commit();
        }
    }

    /// A zoom action's `perform` (`actions/actionCanvas.tsx`, through
    /// [`perform_zoom_action`]), navigation being enabled.
    fn zoom(&mut self, action: ZoomAction) {
        let state = self.viewport_state();
        let elements: Vec<&Element> = self.session.elements().iter().collect();
        let selected = self
            .session
            .app_state()
            .get("selectedElementIds")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let viewport =
            perform_zoom_action(action, &state, &elements, &selected, &Offsets::default());
        self.set_viewport_to(viewport);
    }

    /// A zoom action by name (the footer's zoom buttons, the keys).
    pub fn zoom_action(&mut self, name: &str) {
        if let Some(action) = ZoomAction::from_name(name) {
            self.zoom(action);
            self.report();
        }
    }

    /// The canvas's `wheel`: `AppWheel.handle` ([`handle_wheel`]), zooming
    /// around the last pointer position. Returns whether to prevent the
    /// browser's default.
    pub fn wheel(&mut self, input: &WheelInput) -> bool {
        let state = self.viewport_state();
        let ctx = WheelContext {
            navigation_enabled: true,
            pan_active: matches!(self.gesture, Some(Gesture::Pan { .. })),
            input_device: InputDevice::from_app_state(self.session.app_state()),
            last_position: (self.last_pointer[0], self.last_pointer[1]),
            is_darwin: self.action_env.is_darwin,
        };
        let event = WheelEvent {
            delta_x: input.delta_x,
            delta_y: input.delta_y,
            ctrl_key: input.ctrl_key,
            meta_key: input.meta_key,
            shift_key: input.shift_key,
            buttons: input.buttons,
            target: WheelTarget::Canvas,
        };
        let outcome = handle_wheel(&state, &ctx, &event);
        if let Some(translation) = outcome.translation {
            if self.interrupt_navigation() {
                self.set_viewport_to(translation.viewport);
            }
        }
        outcome.prevent_default
    }

    /// A `pointerdown` on the canvas.
    pub fn pointer_down(&mut self, input: PointerInput) {
        self.last_pointer = [input.client_x, input.client_y];
        // a press on the canvas closes the convert popup (App.tsx:8756-8758)
        self.keyboard.convert_popup_open = false;
        // a press without the previous one's release ends it first
        // (`maybeCleanupAfterMissingPointerUp`)
        if self.gesture.is_some() {
            self.pointer_up(input);
        }
        let view_mode = self
            .session
            .app_state()
            .get("viewModeEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if pan_starts(
            &self.tools,
            PanStart {
                button: input.button,
                pointer_count: 1,
                interaction_enabled: self.tools.is_interaction_enabled(),
                navigation_enabled: true,
                view_mode_enabled: view_mode,
                active_tool_pointer_capturing: false,
            },
        ) {
            self.gesture = Some(Gesture::Pan {
                last: [input.client_x, input.client_y],
            });
            return;
        }
        // only the main button (or touch) acts on the scene
        if input.button != 0 || !self.tools.is_interaction_enabled() {
            return;
        }
        let tool = self.tools.active_tool.tool.type_name().to_owned();
        match tool.as_str() {
            "selection" => self.select_pointer_down(input, true),
            "lasso" => self.lasso_pointer_down(input),
            "eraser" => {
                let origin = self.scene_point(input.client_x, input.client_y);
                let mut trail = EraserTrail::default();
                trail.start_path(origin[0], origin[1]);
                self.gesture = Some(Gesture::Erase {
                    trail,
                    start: [input.client_x, input.client_y],
                    pending: Vec::new(),
                });
            }
            "arrow" | "line" if self.multi.is_some() => self.multi_pointer_down(input, &tool),
            "rectangle" | "diamond" | "ellipse" | "arrow" | "line" | "freedraw" | "frame" => {
                self.create_pointer_down(input, &tool)
            }
            "text" => self.text_pointer_down(input),
            _ => {}
        }
    }

    /// `clearSelectionIfNotUsingSelection` (`App.tsx:9501-9510`).
    fn clear_selection(&mut self) {
        let mut patch = Map::new();
        patch.insert("selectedElementIds".into(), json!({}));
        patch.insert("selectedGroupIds".into(), json!({}));
        patch.insert("editingGroupId".into(), Value::Null);
        patch.insert("activeEmbeddable".into(), Value::Null);
        let current = self.session.app_state().as_map();
        patch.retain(|k, v| current.get(k) != Some(v));
        if !patch.is_empty() {
            self.session.set_state(patch);
        }
    }

    /// The binding state of the app (`isBindingEnabled`, the grid, the
    /// zoom), a new arrow's press at `origin`.
    pub(crate) fn binding_app_state(&self, origin: [f64; 2], alt: bool) -> BindingAppState {
        let app = self.session.app_state();
        let flag = |k: &str, d: bool| app.get(k).and_then(Value::as_bool).unwrap_or(d);
        BindingAppState {
            zoom: app.zoom().unwrap_or(1.0),
            is_binding_enabled: flag("isBindingEnabled", true),
            is_midpoint_snapping_enabled: flag("isMidpointSnappingEnabled", true),
            grid_mode_enabled: self.grid_size(false).is_some(),
            grid_size: app.grid_size(),
            bind_mode: BindMode::Orbit,
            selected_linear_element: Some(LinearElementInitialState {
                origin: Some([origin[0], origin[1]]),
                arrow_start_is_inside: alt,
                ..LinearElementInitialState::default()
            }),
            complex_bindings: false,
        }
    }

    /// A drawing tool's press (`createGenericElementOnPointerDown`,
    /// `handleLinearElementOnPointerDown`, `handleFreeDrawElementOnPointerDown`,
    /// `createFrameElementOnPointerDown`): the element created at the press
    /// (on the grid, but a freedraw) and inserted on top; a line or arrow
    /// gets its two points and an arrow its start binding; the selection is
    /// the new element unless the tool is locked (a freedraw is never
    /// selected).
    fn create_pointer_down(&mut self, input: PointerInput, tool: &str) {
        let origin = self.scene_point(input.client_x, input.client_y);
        self.clear_selection();
        let grid = if tool == "freedraw" {
            None
        } else {
            self.grid_size(input.ctrl_or_cmd)
        };
        let origin_in_grid = get_grid_point(origin[0], origin[1], grid);
        // a frame goes in no frame (createFrameElementOnPointerDown)
        let frame_id = if tool == "frame" {
            None
        } else {
            self.top_layer_frame_at(origin_in_grid, None, None)
        };
        let id = self.session.env.random_id();
        let seed = self.session.env.random_integer();
        let now = RestoreEnv::now(&mut self.session.env);
        let Some(element) = new_element_for_tool(
            tool,
            self.session.app_state(),
            origin_in_grid,
            frame_id.as_deref(),
            &id,
            seed,
            now,
        ) else {
            return;
        };
        let linear = matches!(tool, "arrow" | "line");
        // insertNewElements (App.tsx:7922-7950): a frame's new child above
        // its highest child
        let index = frame_id.as_deref().and_then(|f| {
            let all: Vec<&Element> = self.session.elements().iter().collect();
            get_frame_children_insertion_index(&all, f)
        });
        // Ok: the indices around the insertion are valid
        let _ = self.session.insert_elements_at_index(vec![element], index);
        let mut scene = Scene::new(self.session.elements().to_vec());
        if linear {
            scene.mutate_element(
                &id,
                ElementUpdate {
                    points: Some(vec![[0.0, 0.0], [0.0, 0.0]]),
                    ..ElementUpdate::default()
                },
                &mut self.session.env,
            );
        }
        if tool == "arrow" {
            // the initial binding, so the strategy has the start's state
            let binding_state = self.binding_app_state(origin, input.alt_key);
            let _ = bind_or_unbind_binding_element(
                &mut scene,
                &mut self.session.env,
                &id,
                &[(0, [0.0, 0.0])],
                [origin_in_grid[0], origin_in_grid[1]],
                &binding_state,
                &BindingOpts {
                    new_arrow: true,
                    alt_key: input.alt_key,
                    initial_binding: true,
                    angle_locked: input.shift_key,
                    ..BindingOpts::default()
                },
            );
        }
        let mut app_state = self.session.app_state().clone();
        if linear && !self.tools.is_tool_locked() {
            app_state.insert("selectedElementIds", json!({ id.clone(): true }));
        }
        self.apply(scene, app_state);
        // insertNewElement's updateFrameToHighlight, and newElement
        let frame = self.element_value(frame_id.as_deref());
        let new_element = self.element_value(Some(&id));
        self.set_keys(vec![
            ("frameToHighlight", frame),
            ("newElement", new_element),
        ]);
        self.session.commit();
        self.gesture = Some(Gesture::Create(CreateGesture {
            id,
            tool: tool.to_owned(),
            origin,
            origin_in_grid,
            dragged: false,
            multi: false,
        }));
        self.report();
    }

    /// The selection tool's press: selects the topmost element hit
    /// (`handleSelectionOnPointerDown`), or starts a link click.
    fn select_pointer_down(&mut self, input: PointerInput, with_box: bool) {
        let origin = self.scene_point(input.client_x, input.client_y);
        // a handle of the selection (`handleSelectionOnPointerDown`)
        let selected = self.selected_ids();
        if !selected.is_empty() {
            let scene = Scene::new(self.session.elements().to_vec());
            let session = TransformSession::begin(
                &scene,
                &selected,
                origin,
                self.session.app_state().zoom().unwrap_or(1.0),
                PointerType::Mouse,
                &EditorInterface::desktop(),
                self.linear_state().map(|l| SelectedLinearElementState {
                    is_editing: l.is_editing,
                    is_dragging: l.is_dragging,
                    hover_point_index: l.hover_point_index as i64,
                }),
            );
            if let Some(handle) = session.handle() {
                let origin_in_grid =
                    get_grid_point(origin[0], origin[1], self.grid_size(input.ctrl_or_cmd));
                // while cropping the handles crop (App.tsx:9555-9563)
                if let Some(id) = self.cropping_id().filter(|c| selected == [c.clone()]) {
                    let original = session
                        .original_elements()
                        .iter()
                        .find(|e| e.base.id == id)
                        .cloned();
                    if let Some(original) = original {
                        self.gesture = Some(Gesture::Crop(Box::new(CropPress {
                            id,
                            handle,
                            offset: session.offset(),
                            origin_in_grid,
                            original,
                        })));
                        return;
                    }
                }
                self.gesture = Some(Gesture::Transform(session, origin_in_grid));
                return;
            }
        }
        let previous_selection = self
            .session
            .app_state()
            .get("selectedElementIds")
            .cloned()
            .unwrap_or(Value::Object(Map::new()));
        let originals: HashMap<String, Element> = self
            .session
            .elements()
            .iter()
            .filter(|e| !e.base.is_deleted)
            .map(|e| (e.base.id.clone(), e.clone()))
            .collect();
        // the selected line or arrow's points and midpoints
        // (LinearElementEditor.handlePointerDown, App.tsx:9600-9623)
        let linear = match self.linear_pointer_down(input, origin) {
            Some((_, true)) => {
                self.gesture = Some(Gesture::Inert);
                self.session.commit();
                return self.report();
            }
            Some((press, false)) => Some(press),
            None => None,
        };
        let link = self.link_at(origin);
        // a locked element on top takes the press, unless an element under
        // it is selected (App.tsx:9662-9708)
        let selected_now = self.selected_ids();
        let on_top = self.element_at_with(origin, true);
        let locked_on_top = on_top.as_ref().is_some_and(|id| self.is_locked(id));
        let active_locked = self
            .session
            .app_state()
            .get("activeLockedId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if on_top.is_none() || on_top != active_locked {
            self.set_keys(vec![("activeLockedId", Value::Null)]);
        }
        let covered_selected = self
            .elements_at(origin, false)
            .iter()
            .any(|id| selected_now.contains(id));
        let hit = if link.is_some() || (locked_on_top && !covered_selected) {
            None
        } else if let Some(press) = linear.as_ref().filter(|p| p.hit) {
            Some(press.id.clone())
        } else {
            self.element_at(origin)
        };
        // a press off the image being cropped ends the cropping
        // (App.tsx:9718-9723)
        if let Some(c) = self.cropping_id() {
            if hit.as_deref() != Some(c.as_str()) {
                self.finish_image_cropping();
            }
        }
        let was_added_to_selection = hit
            .as_ref()
            .is_some_and(|id| !self.selected_ids().contains(id));
        if link.is_none() {
            let editing = self.linear_state().filter(|l| l.is_editing);
            if let Some(mut state) = editing {
                // the editor stays open only for a press on its element
                // (App.tsx:9764-9782)
                state.is_editing = hit.as_deref() == Some(state.element_id.as_str());
                let id = state.element_id.clone();
                self.set_selection(std::slice::from_ref(&id));
                self.set_linear_state(Some(&state));
            } else {
                let mut selected = self.selected_ids();
                match &hit {
                    Some(id) if selected.contains(id) => {}
                    Some(id) if input.shift_key => selected.push(id.clone()),
                    Some(id) => selected = vec![id.clone()],
                    None if input.shift_key => {}
                    None => selected.clear(),
                }
                self.set_selection(&selected);
                // selectedLinearElement follows a lone line or arrow
                // (App.tsx:9771-9790, 12428-12440), and a cleared
                // selection clears it (clearSelection, App.tsx:13005-13024)
                let current = self.linear_state();
                let next = match (&hit, selected.as_slice()) {
                    (_, []) => None,
                    (Some(h), [only]) if h == only && self.is_linear_id(h) => Some(
                        current
                            .filter(|c| &c.element_id == h)
                            .unwrap_or_else(|| LinearState::new(h, false)),
                    ),
                    _ => current,
                };
                self.set_linear_state(next.as_ref());
            }
            self.session.commit();
        }
        // the selection element (`createGenericElementOnPointerDown`)
        let box_origin = (with_box && hit.is_none() && link.is_none())
            .then(|| get_grid_point(origin[0], origin[1], self.grid_size(input.ctrl_or_cmd)));
        if let Some(corner) = box_origin {
            // appState.selectionElement, which the interactive canvas draws
            let id = self.session.env.random_id();
            let seed = self.session.env.random_integer();
            let now = RestoreEnv::now(&mut self.session.env);
            let element = new_element_for_tool(
                "selection",
                self.session.app_state(),
                corner,
                None,
                &id,
                seed,
                now,
            );
            if let Some(element) = element {
                self.set_keys(vec![("selectionElement", Value::Object(element.to_map()))]);
            }
        }
        self.gesture = Some(Gesture::Select(Box::new(SelectGesture {
            origin,
            originals,
            hit,
            link,
            previous_selection,
            dragged: false,
            box_origin,
            box_selected: false,
            with_cmd_or_ctrl: input.ctrl_or_cmd,
            was_added_to_selection,
            has_been_duplicated: false,
            linear,
            last_point: origin,
        })));
        self.report();
    }

    /// The lasso's press (`App.handleCanvasPointerDown`, `:8975-9004`): on
    /// a handle of the selection, one of its elements or the common box of
    /// several, it acts as the selection tool does (without a selection
    /// box); anywhere else a lasso path starts, clearing the selection
    /// unless Shift is held. Ctrl/Cmd+Alt lassos even over the selection.
    fn lasso_pointer_down(&mut self, input: PointerInput) {
        let origin = self.scene_point(input.client_x, input.client_y);
        let selected = self.selected_ids();
        let live: Vec<&Element> = self
            .session
            .elements()
            .iter()
            .filter(|e| !e.base.is_deleted && selected.contains(&e.base.id))
            .collect();
        let on_handle = !selected.is_empty() && {
            let scene = Scene::new(self.session.elements().to_vec());
            TransformSession::begin(
                &scene,
                &selected,
                origin,
                self.session.app_state().zoom().unwrap_or(1.0),
                PointerType::Mouse,
                &EditorInterface::desktop(),
                None,
            )
            .handle()
            .is_some()
        };
        let force = input.alt_key && input.ctrl_or_cmd;
        let on_selection = self.is_hitting_common_bounding_box(origin, &live)
            || self
                .element_at(origin)
                .is_some_and(|id| selected.contains(&id));
        if on_handle || (on_selection && !force) {
            return self.select_pointer_down(input, false);
        }
        let mut trail = LassoTrail::default();
        if let Some(cleared) = trail.start_path(origin[0], origin[1], input.shift_key) {
            self.apply_lasso_selection(cleared);
        }
        self.gesture = Some(Gesture::Lasso(trail));
        self.report();
    }

    /// A move of the lasso (`App.onPointerMove`, `:11254-11269`): the
    /// point added to the trail and the selection it now takes.
    fn lasso_pointer_move(&mut self, input: PointerInput, trail: &mut LassoTrail) {
        let point = self.scene_point(input.client_x, input.client_y);
        let app_state = self.session.app_state();
        let viewport = ViewportState::from_app_state(app_state);
        let live: Vec<&Element> = self
            .session
            .elements()
            .iter()
            .filter(|e| !e.base.is_deleted)
            .collect();
        let scene = LassoScene {
            elements: live.clone(),
            visible: live,
            zoom: viewport.zoom,
            scroll_x: viewport.scroll_x,
            scroll_y: viewport.scroll_y,
            mode: BoxSelectionMode::from_name(
                app_state
                    .get("boxSelectionMode")
                    .and_then(Value::as_str)
                    .unwrap_or("contain"),
            ),
            selected_element_ids: app_state
                .get("selectedElementIds")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            editing_group_id: app_state
                .get("editingGroupId")
                .and_then(Value::as_str)
                .map(str::to_owned),
        };
        let selection = trail.add_point_to_path(point[0], point[1], input.shift_key, &scene);
        if let Some(selection) = selection {
            self.apply_lasso_selection(selection);
            self.report();
        }
    }

    /// The selection a lasso op leaves, with the linear element editor of
    /// a lone line or arrow (kept when it already holds that element).
    fn apply_lasso_selection(&mut self, selection: LassoSelection) {
        let linear = selection.selected_linear_element.map(|id| {
            self.linear_state()
                .filter(|l| l.element_id == id)
                .unwrap_or_else(|| LinearState::new(&id, false))
        });
        self.set_keys(vec![
            (
                "selectedElementIds",
                Value::Object(selection.selected_element_ids),
            ),
            (
                "selectedGroupIds",
                Value::Object(selection.selected_group_ids),
            ),
        ]);
        self.set_linear_state(linear.as_ref());
    }

    /// Whether `id` is a locked element.
    fn is_locked(&self, id: &str) -> bool {
        self.session
            .elements()
            .iter()
            .any(|e| e.base.id == id && e.base.locked)
    }

    /// Whether `id` is a line or an arrow in the scene.
    fn is_linear_id(&self, id: &str) -> bool {
        self.session
            .elements()
            .iter()
            .any(|e| e.base.id == id && !e.base.is_deleted && e.kind.linear().is_some())
    }

    /// A `pointermove`: pans during a pan, drags the selection while a press
    /// that hit an element lasts; otherwise only the pointer's position is
    /// kept.
    pub fn pointer_move(&mut self, input: PointerInput) {
        self.last_pointer = [input.client_x, input.client_y];
        match &mut self.gesture {
            Some(Gesture::Pan { last }) => {
                let delta = [last[0] - input.client_x, last[1] - input.client_y];
                *last = [input.client_x, input.client_y];
                let state = self.viewport_state();
                let update = ViewportUpdate {
                    scroll_x: Some(state.scroll_x - delta[0] / state.zoom),
                    scroll_y: Some(state.scroll_y - delta[1] / state.zoom),
                    zoom: None,
                };
                if self.interrupt_navigation() {
                    let translation = translate(&state, Some(update), TranslateOptions::default());
                    self.set_viewport_to(translation.viewport);
                }
            }
            Some(Gesture::Select(g)) if g.box_origin.is_some() => self.box_select(input),
            Some(Gesture::Select(_)) => self.drag_selection(input),
            Some(Gesture::Transform(..)) => self.transform(input),
            Some(Gesture::Crop(press)) => {
                let press = press.clone();
                self.crop_move(&press, input);
            }
            Some(Gesture::Create(_)) => self.create_pointer_move(input),
            Some(Gesture::Lasso(_)) => {
                if let Some(Gesture::Lasso(mut trail)) = self.gesture.take() {
                    self.lasso_pointer_move(input, &mut trail);
                    self.gesture = Some(Gesture::Lasso(trail));
                }
            }
            Some(Gesture::Erase { trail, pending, .. }) => {
                let point = viewport_coords_to_scene_coords(
                    input.client_x,
                    input.client_y,
                    &ViewportState::from_app_state(self.session.app_state()),
                );
                let zoom = self.session.app_state().zoom().unwrap_or(1.0);
                let visible: Vec<&Element> = self
                    .session
                    .elements()
                    .iter()
                    .filter(|e| !e.base.is_deleted)
                    .collect();
                *pending = trail.add_point_to_path(point.0, point.1, input.alt_key, &visible, zoom);
            }
            Some(Gesture::TextCreate { .. } | Gesture::TextLabel { .. }) => self.text_drag(input),
            None if self.multi.is_some() => self.multi_hover(input),
            None => self.hover(input),
            Some(Gesture::Finalized | Gesture::Inert) => {}
        }
    }

    /// A move while drawing (`App.onPointerMove`, `:11262-11366`): a
    /// freedraw takes the pointer as its next point; a line or arrow's last
    /// point follows the pointer (on the grid, at 15 degree steps with
    /// Shift); any other shape spans the press and the pointer
    /// (`maybeDragNewGenericElement`, `dragNewElement`), square with Shift,
    /// about the press with Alt.
    fn create_pointer_move(&mut self, input: PointerInput) {
        let pointer = self.scene_point(input.client_x, input.client_y);
        let Some(Gesture::Create(gesture)) = self.gesture.as_mut() else {
            return;
        };
        let origin = gesture.origin;
        let Some(element) = self
            .session
            .elements()
            .iter()
            .find(|e| e.base.id == gesture.id && !e.base.is_deleted)
            .cloned()
        else {
            return;
        };
        let mut scene = Scene::new(self.session.elements().to_vec());
        let update = match (&element.kind, gesture.tool.as_str()) {
            (ElementKind::Freedraw(f), _) => {
                let point = [pointer[0] - element.base.x, pointer[1] - element.base.y];
                if f.points.last() == Some(&point) {
                    return;
                }
                let mut points = f.points.clone();
                points.push(point);
                ElementUpdate {
                    points: Some(points),
                    ..ElementUpdate::default()
                }
            }
            (_, "arrow" | "line") => {
                gesture.dragged = true;
                let Some(points) = element.kind.points() else {
                    return;
                };
                let grid = if input.ctrl_or_cmd {
                    None
                } else {
                    self.props
                        .grid_mode_enabled
                        .or_else(|| self.session.app_state().grid_mode_enabled())
                        .unwrap_or(false)
                        .then(|| self.session.app_state().grid_size())
                        .flatten()
                };
                let map = scene.elements_map();
                let mut last = create_point_at(&element, &map, pointer[0], pointer[1], grid);
                if input.shift_key {
                    // getLockedLinearCursorAlignSize from the previous point
                    let prev = points[points.len().saturating_sub(2)];
                    let (w, h) = get_locked_linear_cursor_align_size(
                        element.base.x + prev[0],
                        element.base.y + prev[1],
                        pointer[0],
                        pointer[1],
                    );
                    last = [prev[0] + w, prev[1] + h];
                }
                let mut next = points.to_vec();
                let n = next.len();
                next[n - 1] = last;
                ElementUpdate {
                    points: Some(next),
                    ..ElementUpdate::default()
                }
            }
            _ => {
                let grid = if input.ctrl_or_cmd {
                    None
                } else {
                    self.props
                        .grid_mode_enabled
                        .or_else(|| self.session.app_state().grid_mode_enabled())
                        .unwrap_or(false)
                        .then(|| self.session.app_state().grid_size())
                        .flatten()
                };
                let [mut gx, mut gy] = get_grid_point(pointer[0], pointer[1], grid);
                let [ox, oy] = gesture.origin_in_grid;
                let tool = gesture.tool.clone();
                // snapNewElement (App.tsx:13518-13545)
                let origin_offset = self.origin_snap_offset();
                let [sx, sy] = origin_offset.unwrap_or([0.0, 0.0]);
                let snap_state = self.snap_state();
                let event = Some(SnapEvent {
                    ctrl_or_cmd: input.ctrl_or_cmd,
                });
                let live: Vec<&Element> = self
                    .session
                    .elements()
                    .iter()
                    .filter(|e| !e.base.is_deleted)
                    .collect();
                let map = ElementsMap::new(live.iter().copied());
                self.snap_cache.maybe_cache_reference_snap_points(
                    &snap_state,
                    event,
                    &[&element],
                    &live,
                    &map,
                );
                let snapped = snap_new_element(
                    &element,
                    &self.snap_cache,
                    &snap_state,
                    event,
                    [ox + sx, oy + sy],
                    [gx - ox, gy - oy],
                    &map,
                );
                gx += snapped.snap_offset[0];
                gy += snapped.snap_offset[1];
                let lines = interact::snap_lines_json(&snapped.snap_lines);
                self.set_keys(vec![("snapLines", lines)]);
                let Some([x, y, width, height]) = drag_new_element(&DragNewElement {
                    element: &element,
                    element_type: &tool,
                    origin: [ox, oy],
                    pointer: [gx, gy],
                    width: (gx - ox).abs(),
                    height: (gy - oy).abs(),
                    maintain_aspect_ratio: input.shift_key,
                    resize_from_center: input.alt_key,
                    width_aspect_ratio: None,
                    origin_offset,
                }) else {
                    self.session.commit();
                    return;
                };
                ElementUpdate {
                    x: Some(x),
                    y: Some(y),
                    width: Some(width),
                    height: Some(height),
                    ..ElementUpdate::default()
                }
            }
        };
        let dragged_end = update.points.as_ref().and_then(|p| p.last().copied());
        scene.mutate_element(&element.base.id, update, &mut self.session.env);
        let app_state = self.session.app_state().clone();
        self.apply(scene, app_state);
        let id = element.base.id.clone();
        if let (Some(point), ElementKind::Arrow(_)) = (dragged_end, &element.kind) {
            let last = element.kind.points().map_or(1, <[_]>::len) - 1;
            self.suggest_binding(&id, last, point, pointer, origin, true, input.alt_key);
        }
        let new_element = self.element_value(Some(&id));
        let mut keys = vec![("newElement", new_element)];
        // what a frame being drawn would take in (App.tsx:13574-13588)
        if is_frame_like(&element) {
            let inside = self.elements_in_resizing_frame(&id);
            keys.push(("elementsToHighlight", self.elements_value(&inside)));
        }
        self.set_keys(keys);
        self.session.commit();
        self.report();
    }

    /// `getEffectiveGridSize()`: the grid size in grid mode, none with
    /// Ctrl/Cmd held.
    pub(crate) fn grid_size(&self, ctrl_or_cmd: bool) -> Option<f64> {
        if ctrl_or_cmd {
            return None;
        }
        self.props
            .grid_mode_enabled
            .or_else(|| self.session.app_state().grid_mode_enabled())
            .unwrap_or(false)
            .then(|| self.session.app_state().grid_size())
            .flatten()
    }

    /// A move while box selecting (`App.onPointerMove`, `:11368-11470`):
    /// the box grows to the pointer (`dragNewElement` on the grid) and the
    /// selection becomes what it takes, with the previous selection kept
    /// when Shift is held or nothing was selected, groups taken whole.
    fn box_select(&mut self, input: PointerInput) {
        let point = self.scene_point(input.client_x, input.client_y);
        let grid = self.grid_size(input.ctrl_or_cmd);
        let Some(Gesture::Select(gesture)) = self.gesture.as_mut() else {
            return;
        };
        let Some(origin) = gesture.box_origin else {
            return;
        };
        gesture.box_selected = true;
        let press = gesture.origin;
        let corner = get_grid_point(point[0], point[1], grid);
        let elements = self.session.elements();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let mode = BoxSelectionMode::from_name(
            self.session
                .app_state()
                .get("boxSelectionMode")
                .and_then(Value::as_str)
                .unwrap_or("contain"),
        );
        let within: Vec<String> = get_elements_within_selection(
            &live,
            [origin[0], origin[1], corner[0], corner[1]],
            &map,
            false,
            mode,
        )
        .iter()
        .map(|e| e.base.id.clone())
        .collect();
        let current = self.selected_ids();
        let reuse = input.shift_key || current.is_empty();
        let mut next: Map<String, Value> = if reuse {
            self.session
                .app_state()
                .get("selectedElementIds")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
        } else {
            Map::new()
        };
        for id in within {
            next.insert(id, Value::Bool(true));
        }
        let editing = if reuse {
            self.session
                .app_state()
                .get("editingGroupId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        } else {
            None
        };
        let groups = select_groups_for_selected_elements(&next, editing.as_deref(), &live);
        let mut patch = Map::new();
        patch.insert(
            "selectedElementIds".into(),
            Value::Object(groups.selected_element_ids),
        );
        patch.insert(
            "selectedGroupIds".into(),
            Value::Object(groups.selected_group_ids),
        );
        patch.insert(
            "editingGroupId".into(),
            groups.editing_group_id.map_or(Value::Null, Value::String),
        );
        let current = self.session.app_state().as_map();
        patch.retain(|k, v| current.get(k) != Some(v));
        if !patch.is_empty() {
            self.session.set_state(patch);
        }
        // the box follows the pointer (maybeDragNewGenericElement,
        // App.tsx:13474-13497)
        let selection = self
            .session
            .app_state()
            .get("selectionElement")
            .and_then(Value::as_object)
            .and_then(|m| Element::from_map(m.clone()).ok());
        if let Some(mut selection) = selection {
            if let Some([x, y, width, height]) = drag_new_element(&DragNewElement {
                element: &selection,
                element_type: "selection",
                origin: press,
                pointer: point,
                width: (point[0] - press[0]).abs(),
                height: (point[1] - press[1]).abs(),
                maintain_aspect_ratio: false,
                resize_from_center: false,
                width_aspect_ratio: None,
                origin_offset: None,
            }) {
                selection.base.x = x;
                selection.base.y = y;
                selection.base.width = width;
                selection.base.height = height;
                self.set_keys(vec![(
                    "selectionElement",
                    Value::Object(selection.to_map()),
                )]);
            }
        }
        self.session.commit();
        self.report();
    }

    /// A move on a handle: `maybeHandleResize` through the transform
    /// session, the pointer on the grid unless Ctrl/Cmd is held; Shift
    /// keeps the aspect ratio and snaps the angle, Alt resizes from the
    /// centre.
    fn transform(&mut self, input: PointerInput) {
        let point = self.scene_point(input.client_x, input.client_y);
        let grid = self.grid_size(false);
        let Some(Gesture::Transform(session, origin_in_grid)) = self.gesture.as_ref() else {
            return;
        };
        let (session, origin_in_grid) = (session.clone(), *origin_in_grid);
        let handle = session.handle();
        let rotating = handle == Some(TransformHandleType::Rotation);
        self.set_keys(vec![
            ("isResizing", json!(handle.is_some() && !rotating)),
            ("isRotating", json!(rotating)),
        ]);
        // snapResizingElements (App.tsx:13742-13779)
        let mut snap_offset = [0.0, 0.0];
        if !self
            .session
            .app_state()
            .get("selectedElementsAreBeingDragged")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let [gx, gy] = get_grid_point(point[0], point[1], self.grid_size(input.ctrl_or_cmd));
            let drag_offset = [gx - origin_in_grid[0], gy - origin_in_grid[1]];
            let snap_state = self.snap_state();
            let event = Some(SnapEvent {
                ctrl_or_cmd: input.ctrl_or_cmd,
            });
            let elements = self.session.elements().to_vec();
            let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
            let map = ElementsMap::new(live.iter().copied());
            let selected: Vec<&Element> = live
                .iter()
                .copied()
                .filter(|e| session.selected().contains(&e.base.id))
                .collect();
            let originals: Vec<&Element> = session
                .original_elements()
                .iter()
                .filter(|e| session.selected().contains(&e.base.id))
                .collect();
            self.snap_cache.maybe_cache_reference_snap_points(
                &snap_state,
                event,
                &selected,
                &live,
                &map,
            );
            let snapped = snap_resizing_elements(
                &selected,
                &originals,
                &self.snap_cache,
                &snap_state,
                event,
                drag_offset,
                handle,
            );
            snap_offset = snapped.snap_offset;
            self.set_keys(vec![(
                "snapLines",
                interact::snap_lines_json(&snapped.snap_lines),
            )]);
        }
        let mut scene = Scene::new(self.session.elements().to_vec());
        let app_state = self.session.app_state().clone();
        let transformed = session.update_snapped(
            &mut scene,
            &mut self.session.env,
            point,
            TransformModifiers {
                shift: input.shift_key,
                alt: input.alt_key,
                ctrl: input.ctrl_or_cmd,
            },
            grid,
            snap_offset,
        );
        if transformed {
            self.apply(scene, app_state);
            // the elements a resized frame would hold (App.tsx:13800-13817)
            let frames: Vec<String> = self
                .session
                .elements()
                .iter()
                .filter(|e| {
                    !e.base.is_deleted
                        && is_frame_like(e)
                        && session.selected().contains(&e.base.id)
                })
                .map(|e| e.base.id.clone())
                .collect();
            let mut highlight: Vec<String> = Vec::new();
            for frame in frames {
                for id in self.elements_in_resizing_frame(&frame) {
                    if !highlight.contains(&id) {
                        highlight.push(id);
                    }
                }
            }
            let highlight = self.elements_value(&highlight);
            self.set_keys(vec![("elementsToHighlight", highlight)]);
        }
        self.session.commit();
        self.report();
    }

    /// A move while a press on a selected element lasts
    /// (`App.tsx:10988-11232`): the frame under the pointer is highlighted
    /// (`getTopLayerFrameAtSceneCoords`); unless Ctrl/Cmd was held at the
    /// press, the selection is dragged by the pointer's offset from the
    /// press (one axis only with Shift), snapped to the other elements
    /// when snapping is on (`snapDraggedElements`) and otherwise to the
    /// grid; the first move with Alt held duplicates the selection and
    /// drags the duplicates (`duplicateDraggedSelection`).
    fn drag_selection(&mut self, input: PointerInput) {
        let point = self.scene_point(input.client_x, input.client_y);
        // the linear element editor's points and midpoints first
        // (App.tsx:10716-10985)
        if let Some(Gesture::Select(gesture)) = self.gesture.as_mut() {
            if let Some(mut press) = gesture.linear.take() {
                let handled = self.linear_drag(&mut press, input);
                if let Some(Gesture::Select(gesture)) = self.gesture.as_mut() {
                    gesture.linear = Some(press);
                    if handled {
                        gesture.dragged = true;
                    }
                }
                if handled {
                    self.session.commit();
                    return self.report();
                }
            }
        }
        // the crop moves over the image being cropped
        if let Some(Gesture::Select(gesture)) = self.gesture.as_mut() {
            let last = std::mem::replace(&mut gesture.last_point, point);
            let hit = gesture.hit.clone();
            if let (Some(c), Some(h)) = (self.cropping_id(), hit) {
                if c == h && !input.alt_key && self.crop_region_drag(&c, last, point) {
                    if let Some(Gesture::Select(gesture)) = self.gesture.as_mut() {
                        gesture.dragged = true;
                    }
                    return;
                }
            }
        }
        let Some(Gesture::Select(gesture)) = self.gesture.as_ref() else {
            return;
        };
        if gesture.hit.is_none() {
            return;
        }
        let mut offset = [point[0] - gesture.origin[0], point[1] - gesture.origin[1]];
        if offset == [0.0, 0.0] && !gesture.dragged {
            return;
        }
        let with_cmd_or_ctrl = gesture.with_cmd_or_ctrl;
        let selected = self.selected_ids();
        let selected_set: HashSet<String> = selected.iter().cloned().collect();
        let elements = self.session.elements().to_vec();
        let selected_elements: Vec<&Element> = elements
            .iter()
            .filter(|e| !e.base.is_deleted && selected_set.contains(&e.base.id))
            .collect();
        if !selected_elements.is_empty() && selected_elements.iter().all(|e| e.base.locked) {
            return;
        }
        let frame = if selected_elements.iter().any(|e| is_frame_like(e)) {
            None
        } else {
            let current = excali_editor::frame::get_common_frame_id(&selected_elements);
            self.top_layer_frame_at(point, Some(&selected_set), current.as_deref())
        };
        let frame = self.element_value(frame.as_deref());
        self.set_keys(vec![("frameToHighlight", frame)]);
        if let Some(Gesture::Select(gesture)) = self.gesture.as_mut() {
            gesture.dragged = true;
        }
        if selected_elements.is_empty() || with_cmd_or_ctrl {
            self.session.commit();
            self.report();
            return;
        }
        if input.shift_key {
            // lockDirection: the smaller offset is dropped
            let (dx, dy) = (offset[0].abs(), offset[1].abs());
            if dx < dy {
                offset[0] = 0.0;
            }
            if dx > dy {
                offset[1] = 0.0;
            }
        }
        let Some(Gesture::Select(gesture)) = self.gesture.as_ref() else {
            return;
        };
        let originals = gesture.originals.clone();
        // the snap cache is filled before the first drag
        let snap_state = self.snap_state();
        let event = Some(SnapEvent {
            ctrl_or_cmd: input.ctrl_or_cmd,
        });
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        self.snap_cache.maybe_cache_visible_gaps(
            &snap_state,
            event,
            &selected_elements,
            &live,
            &map,
        );
        self.snap_cache.maybe_cache_reference_snap_points(
            &snap_state,
            event,
            &selected_elements,
            &live,
            &map,
        );
        let original_list: Vec<&Element> = originals.values().collect();
        let original_ids: HashSet<&str> = selected_set.iter().map(String::as_str).collect();
        let snapped = snap_dragged_elements(
            &original_list,
            &original_ids,
            &mut offset,
            &self.snap_cache,
            &snap_state,
            event,
            &map,
        );
        let grid = self.grid_size(input.ctrl_or_cmd);
        let mut scene = Scene::new(elements.clone());
        let app_state = self.session.app_state().clone();
        drag_selected_elements(
            &originals,
            &selected,
            offset,
            &mut scene,
            snapped.snap_offset,
            grid,
            &mut self.session.env,
        );
        self.apply(scene, app_state);
        self.set_keys(vec![
            ("snapLines", interact::snap_lines_json(&snapped.snap_lines)),
            ("selectedElementsAreBeingDragged", json!(true)),
            ("selectionElement", Value::Null),
        ]);
        let duplicate = input.alt_key
            && matches!(self.gesture.as_ref(), Some(Gesture::Select(g)) if !g.has_been_duplicated);
        if duplicate {
            self.duplicate_dragged_selection(point, &selected, event);
        }
        self.session.commit();
        self.report();
    }

    /// `duplicateDraggedSelection` (`App.duplicate.ts:163-318`) at the
    /// pointer `point`: the originals back where the press found them, the
    /// duplicates selected and dragged from here on.
    fn duplicate_dragged_selection(
        &mut self,
        point: [f64; 2],
        dragged: &[String],
        event: Option<SnapEvent>,
    ) {
        let Some(Gesture::Select(gesture)) = self.gesture.as_mut() else {
            return;
        };
        gesture.has_been_duplicated = true;
        let hit = gesture.hit.clone();
        let was_added = gesture.was_added_to_selection;
        let originals = gesture.originals.clone();
        let elements = self.session.elements().to_vec();
        let app_state = self.session.app_state().clone();
        let Some(dup) = duplicate_dragged_selection(
            &elements,
            &app_state,
            hit.as_deref(),
            was_added,
            &originals,
            &mut self.session.env,
        ) else {
            return;
        };
        if dup.duplicated_elements.is_empty() {
            return;
        }
        if let Some(Gesture::Select(gesture)) = self.gesture.as_mut() {
            for d in &dup.duplicated_elements {
                gesture.originals.insert(d.base.id.clone(), d.clone());
            }
            gesture.hit = hit.and_then(|h| dup.orig_id_to_duplicate_id.get(&h).cloned());
            gesture.origin = point;
        }
        let _ = self.session.replace_all_elements(dup.elements);
        self.set_patch(dup.selection);
        // arrows bound to the originals follow them back
        let mut scene = Scene::new(self.session.elements().to_vec());
        let selected: Vec<String> = originals
            .keys()
            .filter(|id| {
                app_state
                    .get("selectedElementIds")
                    .and_then(|m| m.get(id.as_str()))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        for id in &selected {
            let arrows = scene.get(id).is_some_and(|e| {
                e.base
                    .bound_elements
                    .as_ref()
                    .is_some_and(|b| b.iter().any(|b| b.kind == BoundElementType::Arrow))
            });
            if arrows {
                update_bound_elements(&mut scene, &mut self.session.env, id, None, None);
            }
        }
        let app_state = self.session.app_state().clone();
        self.apply(scene, app_state);
        // the caches are computed again, the dragged originals left out
        // (`maybeCacheVisibleGaps(event, selectedElements, true)`)
        self.snap_cache.destroy();
        let snap_state = self.snap_state();
        let elements = self.session.elements().to_vec();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let originals: Vec<&Element> = live
            .iter()
            .copied()
            .filter(|e| dragged.contains(&e.base.id))
            .collect();
        self.snap_cache
            .maybe_cache_visible_gaps(&snap_state, event, &originals, &live, &map);
        self.snap_cache.maybe_cache_reference_snap_points(
            &snap_state,
            event,
            &originals,
            &live,
            &map,
        );
    }

    /// The `pointerup` ending the press.
    pub fn pointer_up(&mut self, input: PointerInput) {
        self.last_pointer = [input.client_x, input.client_y];
        let gesture = self.gesture.take();
        if let Some(Gesture::Select(g)) = &gesture {
            // the frames of the dragged selection (App.tsx:12060-12182),
            // while the drag's state is still the app's
            let dragging_points = self.linear_state().is_some_and(|l| l.is_dragging);
            if g.dragged && g.hit.is_some() && !g.with_cmd_or_ctrl && !dragging_points {
                let point = self.scene_point(input.client_x, input.client_y);
                self.frame_membership_after_drag(point);
            }
        }
        if let Some(Gesture::Transform(session, _)) = &gesture {
            if session.handle().is_some() {
                self.frame_membership_after_resize();
            }
        }
        self.reset_after_release();
        match gesture {
            None | Some(Gesture::Pan { .. }) => {
                self.session.commit();
            }
            Some(Gesture::Select(gesture)) => self.select_pointer_up(input, *gesture),
            Some(Gesture::Transform(..)) => {
                self.session.store.schedule_capture();
                self.session.commit();
                self.report();
            }
            Some(Gesture::Inert) => {
                self.session.commit();
                self.report();
            }
            Some(Gesture::Crop(_)) => {
                self.session.store.schedule_capture();
                self.session.commit();
                self.report();
            }
            Some(Gesture::Finalized) => {
                // the release of the press that finished the element
                // reverts the tool (App.tsx:12594-12620)
                if !self.tools.is_tool_locked() {
                    self.tools.active_tool = self.tools.tool_after_finalize();
                }
                self.set_keys(vec![
                    ("newElement", Value::Null),
                    ("suggestedBinding", Value::Null),
                ]);
                self.session.commit();
                self.report();
            }
            Some(Gesture::Create(gesture)) => self.create_pointer_up(input, gesture),
            Some(Gesture::Erase { start, pending, .. }) => {
                self.erase_pointer_up(input, start, pending);
            }
            Some(Gesture::Lasso(mut trail)) => {
                trail.end_path();
                self.session.commit();
                self.report();
            }
            Some(gesture @ (Gesture::TextCreate { .. } | Gesture::TextLabel { .. })) => {
                self.text_pointer_up(input, gesture);
            }
        }
    }

    /// The release of a drawing tool's press
    /// (`onPointerUpFromPointerDownHandler`, `:11772-12060`, `:12531-12620`):
    /// a freedraw takes the pointer as its last point (a dot nudged so it
    /// is not invisibly small); a line or arrow dragged far enough is
    /// finalized (`actionFinalize`: the end bound where it was released);
    /// an element too small to see is removed without a trace; the element
    /// is selected and the tool reverts to the selection tool unless it is
    /// locked (a freedraw keeps its tool and is not selected). One undoable
    /// step.
    fn create_pointer_up(&mut self, input: PointerInput, gesture: CreateGesture) {
        let pointer = self.scene_point(input.client_x, input.client_y);
        let Some(element) = self
            .session
            .elements()
            .iter()
            .find(|e| e.base.id == gesture.id && !e.base.is_deleted)
            .cloned()
        else {
            return;
        };
        let mut scene = Scene::new(self.session.elements().to_vec());
        let linear = matches!(gesture.tool.as_str(), "arrow" | "line");
        if let ElementKind::Freedraw(f) = &element.kind {
            let mut dx = pointer[0] - element.base.x;
            let mut dy = pointer[1] - element.base.y;
            // dots are not infinitely small
            if f.points.first() == Some(&[dx, dy]) {
                dx += 0.0001;
                dy += 0.0001;
            }
            let mut points = f.points.clone();
            points.push([dx, dy]);
            scene.mutate_element(
                &element.base.id,
                ElementUpdate {
                    points: Some(points),
                    ..ElementUpdate::default()
                },
                &mut self.session.env,
            );
        } else if linear {
            let zoom = self.session.app_state().zoom().unwrap_or(1.0);
            let distance = js::hypot(
                pointer[0] - gesture.origin[0],
                pointer[1] - gesture.origin[1],
            ) * zoom;
            if gesture.multi {
                // the point is committed (App.tsx:11728-11745)
                let last = element
                    .kind
                    .points()
                    .map_or(0, <[_]>::len)
                    .saturating_sub(1);
                if let Some(m) = self.multi.as_mut() {
                    m.last_committed = Some(last);
                }
                self.session.commit();
                return self.report();
            }
            if !gesture.dragged || distance < MINIMUM_ARROW_SIZE {
                // the element is drawn point by point from here
                return self.start_multi_point(&element.base.id);
            }
            if gesture.tool == "arrow" {
                let map = scene.elements_map();
                let grid = self.grid_size(input.ctrl_or_cmd);
                let last = element
                    .kind
                    .points()
                    .map_or(0, <[_]>::len)
                    .saturating_sub(1);
                let dragged = if input.shift_key {
                    element.kind.points().and_then(|p| p.last().copied())
                } else {
                    Some(create_point_at(
                        &element, &map, pointer[0], pointer[1], grid,
                    ))
                };
                if let Some(point) = dragged {
                    let binding_state = self.binding_app_state(gesture.origin, input.alt_key);
                    let opts = BindingOpts {
                        new_arrow: true,
                        alt_key: input.alt_key,
                        angle_locked: input.shift_key,
                        grid_size: self.grid_size(false),
                        ..BindingOpts::default()
                    };
                    let _ = bind_or_unbind_binding_element(
                        &mut scene,
                        &mut self.session.env,
                        &element.base.id,
                        &[(last, [point[0], point[1]])],
                        [pointer[0], pointer[1]],
                        &binding_state,
                        &opts,
                    );
                }
            }
        }
        let drawn = scene.get(&element.base.id).cloned().unwrap_or(element);
        if is_invisibly_small_element(&drawn) {
            return self.discard_new_element(&drawn.base.id);
        }
        if is_frame_like(&drawn) {
            // getElementsInNewFrame then addElementsToFrame
            // (App.tsx:12022-12036)
            let elements = scene.elements().to_vec();
            let inside = {
                let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
                let map = ElementsMap::new(live.iter().copied());
                get_elements_in_new_frame(&elements, &drawn, &map)
            };
            let next = add_elements_to_frame(elements, &inside, &drawn, &mut self.session.env);
            scene = Scene::new(next);
        }
        let mut app_state = self.session.app_state().clone();
        let locked = self.tools.is_tool_locked();
        if gesture.tool != "freedraw" {
            if !locked {
                let mut ids = app_state
                    .get("selectedElementIds")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                ids.insert(drawn.base.id.clone(), Value::Bool(true));
                app_state.insert("selectedElementIds", Value::Object(ids));
                self.tools.active_tool = self.tools.tool_after_finalize();
            } else if linear {
                app_state.insert("selectedElementIds", json!({}));
            }
        }
        if linear && !locked {
            app_state.insert(
                "selectedLinearElement",
                json!({ "elementId": drawn.base.id, "isEditing": false }),
            );
        }
        app_state.insert("newElement", Value::Null);
        self.apply(scene, app_state);
        self.session.store.schedule_capture();
        self.session.commit();
        self.report();
    }

    /// A new element too small to see removed, the store's snapshot
    /// updated so nothing records it (`captureUpdate: NEVER`).
    fn discard_new_element(&mut self, id: &str) {
        let elements: Vec<Element> = self
            .session
            .elements()
            .iter()
            .filter(|e| e.base.id != id)
            .cloned()
            .collect();
        let mut patch = Map::new();
        patch.insert("selectedElementIds".into(), json!({}));
        patch.insert("newElement".into(), Value::Null);
        let _ = self.session.update_scene(
            Some(elements),
            Some(patch),
            Some(CaptureUpdateAction::Never),
        );
        self.report();
    }

    /// The eraser's release (`App.tsx:12266-12295`, `eraseElements`): a
    /// click erases what is under the pointer; the elements taken are
    /// deleted with their labels and frame children, arrows bound to them
    /// unbound; one undoable step.
    fn erase_pointer_up(&mut self, input: PointerInput, start: [f64; 2], pending: Vec<String>) {
        let mut pending: Vec<String> = pending;
        if start == [input.client_x, input.client_y] {
            let point = self.scene_point(input.client_x, input.client_y);
            if let Some(id) = self.element_at(point) {
                pending.push(id);
            }
        }
        if pending.is_empty() {
            return;
        }
        let mut scene = Scene::new(self.session.elements().to_vec());
        for id in &pending {
            let Some(element) = scene.get(id).cloned() else {
                continue;
            };
            if let Some(linear) = element.kind.linear() {
                for binding in [&linear.start_binding, &linear.end_binding]
                    .into_iter()
                    .flatten()
                {
                    if let Some(bindable) = scene.get(&binding.element_id).cloned() {
                        let bound: Vec<BoundElement> = bindable
                            .base
                            .bound_elements
                            .clone()
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|b| b.id != element.base.id)
                            .collect();
                        scene.mutate_element(
                            &bindable.base.id,
                            ElementUpdate {
                                bound_elements: Some(Some(bound)),
                                ..ElementUpdate::default()
                            },
                            &mut self.session.env,
                        );
                    }
                }
            } else {
                for bound in element.base.bound_elements.clone().unwrap_or_default() {
                    if bound.kind != BoundElementType::Arrow {
                        continue;
                    }
                    let Some(arrow) = scene.get(&bound.id).and_then(|a| a.kind.linear().cloned())
                    else {
                        continue;
                    };
                    let mut update = ElementUpdate::default();
                    if arrow
                        .start_binding
                        .as_ref()
                        .is_some_and(|b| b.element_id == *id)
                    {
                        update.start_binding = Some(None);
                    }
                    if arrow
                        .end_binding
                        .as_ref()
                        .is_some_and(|b| b.element_id == *id)
                    {
                        update.end_binding = Some(None);
                    }
                    if !update.is_empty() {
                        scene.mutate_element(&bound.id, update, &mut self.session.env);
                    }
                }
            }
        }
        let mut elements = scene.elements().to_vec();
        let mut changed = false;
        for e in &mut elements {
            let container = match &e.kind {
                ElementKind::Text(t) => t.container_id.clone(),
                _ => None,
            };
            if pending.contains(&e.base.id)
                || e.base
                    .frame_id
                    .as_ref()
                    .is_some_and(|f| pending.contains(f))
                || container.is_some_and(|c| pending.contains(&c))
            {
                // newElementWith(ele, { isDeleted: true })
                e.base.is_deleted = true;
                bump_version(e, None, &mut self.session.env);
                changed = true;
            }
        }
        if changed {
            let _ = self.session.replace_all_elements(elements);
            self.session.store.schedule_capture();
            self.session.commit();
        }
        self.report();
    }

    fn select_pointer_up(&mut self, input: PointerInput, gesture: SelectGesture) {
        let point = self.scene_point(input.client_x, input.client_y);
        if let Some((id, href)) = gesture.link {
            if self.link_at(point).is_some_and(|(at, _)| at == id) {
                self.events.push(HostEvent::OpenLink { href });
            }
            return;
        }
        if let Some(press) = &gesture.linear {
            if press.hit || self.linear_state().is_some_and(|l| l.is_dragging) {
                self.linear_pointer_up(press, input);
            }
        }
        // a click on a locked element marks it (App.tsx:11578-11614)
        let hits_selected = self
            .elements_at(gesture.origin, false)
            .iter()
            .any(|id| self.selected_ids().contains(id));
        let active_locked = if !gesture.box_selected && !hits_selected {
            self.element_at_with(point, true).and_then(|id| {
                let e = self.session.elements().iter().find(|e| e.base.id == id)?;
                e.base.locked.then(|| {
                    e.base
                        .group_ids
                        .last()
                        .cloned()
                        .unwrap_or_else(|| e.base.id.clone())
                })
            })
        } else {
            None
        };
        self.set_keys(vec![(
            "activeLockedId",
            active_locked.map_or(Value::Null, Value::String),
        )]);
        let selection = self
            .session
            .app_state()
            .get("selectedElementIds")
            .cloned()
            .unwrap_or(Value::Object(Map::new()));
        if !self.selected_ids().is_empty() || selection != gesture.previous_selection {
            self.session.store.schedule_capture();
        }
        self.session.commit();
        self.report();
        // a click on the text that was already the selection edits it
        let was_selected = gesture.hit.as_ref().is_some_and(|id| {
            gesture
                .previous_selection
                .get(id)
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
        if !gesture.dragged && was_selected {
            self.maybe_edit_selected_text(input, gesture.hit.as_deref());
        }
    }

    /// `updateLibrary({ libraryItems, merge })` with the items of a
    /// `.excalidrawlib` file (`parseLibraryJSON`, restored as an import
    /// restores them, unpublished). Returns the library's item count.
    pub fn import_library(&mut self, text: &str, merge: bool) -> Result<usize, String> {
        let items = parse_library_json(
            text,
            LibraryItemStatus::Unpublished,
            &mut Restore(&mut self.session.env),
        )
        .map_err(|e| format!("The library could not be imported ({e})."))?;
        self.library = if merge {
            merge_library_items(&self.library, &items)
        } else {
            items
        };
        Ok(self.library.len())
    }

    /// The non-deleted elements, which the export draws.
    fn exported(&self) -> Result<Vec<Element>, String> {
        let elements: Vec<Element> = self
            .session
            .elements()
            .iter()
            .filter(|e| !e.base.is_deleted)
            .cloned()
            .collect();
        if elements.is_empty() {
            return Err("Cannot export empty canvas.".to_owned());
        }
        Ok(elements)
    }

    fn files(&self) -> Map<String, Value> {
        self.file.files.as_object().cloned().unwrap_or_default()
    }

    /// The app state `exportCanvas` exports with.
    fn export_app_state(&self, opts: &ExportOptions) -> Map<String, Value> {
        let mut app = self.session.app_state().clone().into_map();
        app.insert("exportBackground".into(), json!(opts.background));
        app.insert("exportWithDarkMode".into(), json!(opts.dark));
        app.insert("exportEmbedScene".into(), json!(opts.embed_scene));
        app.insert("exportScale".into(), json!(opts.scale));
        app
    }

    /// `export("svg", options)`: `exportToSvg` with the keys `exportCanvas`
    /// passes; `fonts` answers each `@font-face`'s `url()`.
    pub fn export_svg(
        &self,
        opts: &ExportOptions,
        fonts: &dyn FontContent,
    ) -> Result<String, String> {
        let elements = self.exported()?;
        let files = self.files();
        let app = self.export_app_state(opts);
        let mut state = SvgExportAppState::new(
            app.get("viewBackgroundColor")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        state.export_background = opts.background;
        state.export_with_dark_mode = opts.dark;
        state.export_embed_scene = opts.embed_scene;
        state.export_scale = Some(opts.scale);
        state.export_padding = opts.padding;
        let metrics = Measure(&self.session.env.layouter.provider);
        let mut options = SvgExportOptions::new(&self.source, &metrics);
        options.clock = self.session.env.render_clock();
        let document = svg_document(&elements, &state, Some(&files), &options);
        Ok(to_svg_file(&export_to_svg(&document, fonts)))
    }

    /// `export("png", options)` up to the pixels: the canvas
    /// `exportCanvas("png")` draws, with the scene to embed when asked.
    pub fn export_png(&self, opts: &ExportOptions) -> Result<CanvasDocument, String> {
        let elements = self.exported()?;
        let files = self.files();
        let app = self.export_app_state(opts);
        let provider = &self.session.env.layouter.provider;
        let options = CanvasExportOptions {
            export_background: opts.background,
            export_padding: opts.padding,
            view_background_color: app
                .get("viewBackgroundColor")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            exporting_frame: None,
            sizing: CanvasSizing::ExportScale,
            text_metrics: provider,
            // images are not decoded in the element yet: placeholders
            image_loads: &|_| false,
            clock: self.session.env.render_clock(),
        };
        export_canvas_png(&elements, &app, &files, &options, &self.source)
            .map_err(|e| e.to_string())
    }

    /// The static canvas's display list at `width` × `height` device
    /// pixels and device pixel ratio `scale` (`renderStaticScene`).
    pub fn static_scene(&self, width: f64, height: f64, scale: f64) -> DisplayList {
        let elements = self.session.elements();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let all = ElementsMap::new(elements.iter());
        let app = self.session.app_state();
        let dark = app.get("theme").and_then(Value::as_str) == Some("dark");
        let state = StaticCanvasAppState {
            zoom: app.zoom().unwrap_or(1.0),
            scroll_x: app.get("scrollX").and_then(Value::as_f64).unwrap_or(0.0),
            scroll_y: app.get("scrollY").and_then(Value::as_f64).unwrap_or(0.0),
            view_background_color: app.view_background_color().map(str::to_owned),
            theme: if dark { Theme::Dark } else { Theme::Light },
            // the crop editor's preview of the image being cropped
            cropping_element_id: self.cropping_id(),
            ..StaticCanvasAppState::default()
        };
        let config = StaticCanvasRenderConfig {
            pending_flowchart_nodes: self.flowchart.pending_nodes().to_vec(),
            clock: self.session.env.render_clock(),
            ..StaticCanvasRenderConfig::default()
        };
        render_static_scene(&StaticScene {
            canvas_width: width,
            canvas_height: height,
            scale,
            elements_map: &map,
            all_elements_map: &all,
            visible_elements: &live,
            app_state: &state,
            render_config: &config,
            text_metrics: &self.session.env.layouter.provider,
        })
    }
}

impl<P: TextMetricsProvider + Clone> Editor<P> {
    /// The interactive canvas's display list at `width` × `height` device
    /// pixels and device pixel ratio `scale` (`renderInteractiveScene`,
    /// `renderer/interactiveScene.ts`), with `selection_color` the
    /// container's `--color-selection` (`getSelectionColor`): the
    /// selection's borders and handles, the selection box, the linear
    /// element editor's points, the binding, frame and element
    /// highlights and the snap lines.
    pub fn interactive_scene(
        &self,
        width: f64,
        height: f64,
        scale: f64,
        selection_color: &str,
    ) -> DisplayList {
        let elements = self.session.elements();
        let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
        let map = ElementsMap::new(live.iter().copied());
        let selected_ids: HashSet<String> = self.selected_ids().into_iter().collect();
        let selected: Vec<&Element> = live
            .iter()
            .copied()
            .filter(|e| selected_ids.contains(&e.base.id))
            .collect();
        let app_state =
            InteractiveCanvasAppState::from_app_state(self.session.app_state().as_map());
        let pointer = self.scene_point(self.last_pointer[0], self.last_pointer[1]);
        render_interactive_scene(&InteractiveScene {
            canvas_width: width,
            canvas_height: height,
            scale,
            elements_map: &map,
            elements: &live,
            all_elements_map: &map,
            all_elements: &live,
            visible_elements: &live,
            selected_elements: &selected,
            app_state: &app_state,
            selection_color,
            editor_interface: EditorInterface::desktop(),
            pointer: Some(pointer),
            angle_locked: false,
        })
    }
}

/// A [`TextMetricsProvider`] as the SVG export's frame labels measure.
struct Measure<'a, P>(&'a P);

impl<P: TextMetricsProvider> excali_scene::export::TextMetrics for Measure<'_, P> {
    fn measure(&self, text: &str, font: &str) -> f64 {
        self.0.get_line_width(text, font)
    }
}
