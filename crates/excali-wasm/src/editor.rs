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
//!   text tool and the double-click edit text (`crate::text`). A press and
//!   release on an element's link icon (`isPointHittingLink`,
//!   `hyperlink/helpers.ts:61-105`) is upstream's `onLinkOpen`: the
//!   `open-link` event, the host deciding.
//! - After every step the store commits (`componentDidUpdate`); a gesture
//!   is captured on its release, and a `change` event reports whether the
//!   file [`Editor::save`] would write differs from the one loaded or last
//!   saved.
//!
//! Reduced from upstream (ex-713): a line or arrow is drawn by dragging
//! only (no point-by-point `multiElement` drawing); nothing snaps
//! (`snapDraggedElements`, `snapNewElement`, `snapResizingElements`);
//! drawing, dragging and resizing leave frame membership as it was; Alt+drag
//! does not duplicate; the interactive canvas (selection outlines, handles,
//! the box) is not painted.
//!
//! The history's leaf layouts are the real ones ([`EditorEnv`]), so an
//! undo re-wraps and re-centres bound text and re-routes bound arrows.

use std::collections::HashMap;

use excali_core::app_state::{AppState, AppStateEnv};
use excali_core::document::{load_scene_json, LoadSceneError, LoadedScene};
use excali_core::element::{BindMode, BoundElement, BoundElementType, Element, ElementKind};
use excali_core::library::{
    merge_library_items, parse_library_json, serialize_library_as_json, LibraryItem,
    LibraryItemStatus,
};
use excali_core::library_url::{parse_library_tokens_from_url, validate_library_url};
use excali_core::restore::{LegacyBinding, LegacyBindingRequest, RestoreEnv};
use excali_editor::actions::{
    ActionContext, ActionEnv, ActionManager, ActionName, AppProps, KeyDownOutcome,
};
use excali_editor::binding::{
    bind_or_unbind_binding_element, BindingAppState, BindingOpts, LinearElementInitialState,
};
use excali_editor::collision::{hit_element, HitTestCache};
use excali_editor::edit_actions::{
    bring_forward, bring_to_front, copy_selected, delete_selected, duplicate_selection, group,
    paste_elements, select_all, send_backward, send_to_back, ungroup, ActionResult,
};
use excali_editor::eraser::EraserTrail;
use excali_editor::groups::select_groups_for_selected_elements;
use excali_editor::keyboard::{
    get_selected_elements, on_clipboard_event, on_key_down, on_key_up, pan_starts,
    ClipboardEventKind, ClipboardOutcome, ClipboardTarget, KeyEffect, KeyOutcome, KeyboardEditor,
    KeyboardState, Keystroke, PanStart,
};
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
use excali_editor::store::CaptureUpdateAction;
use excali_editor::tools::{PointerType, ToolState};
use excali_editor::transform::{get_grid_point, TransformModifiers, TransformSession};
use excali_editor::transform_handles::EditorInterface;
use excali_editor::viewport::{
    handle_wheel, perform_zoom_action, translate, viewport_coords_to_scene_coords, InputDevice,
    Offsets, TranslateOptions, Viewport, ViewportState, ViewportUpdate, WheelContext, WheelEvent,
    WheelTarget, ZoomAction,
};
use excali_math::js;
use excali_scene::bounds::{get_element_absolute_coords, ElementsMap};
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
use serde_json::{json, Map, Value};

use crate::drag::drag_selected_elements;
use crate::env::EditorEnv;
use excali_editor::text_editing::TextEditor;

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
    Select(SelectGesture),
    /// A press on a resize or rotation handle of the selection
    /// (`pointerDownState.resize`, `maybeHandleResize`).
    Transform(TransformSession),
    /// A drawing tool's press: the element being drawn (`newElement`).
    Create(CreateGesture),
    /// The eraser's press: its trail and what it erases
    /// (`eraserTrail`, `elementsPendingErasure`).
    Erase {
        trail: EraserTrail,
        start: [f64; 2],
        pending: Vec<String>,
    },
    /// The text tool's press that started a new text (`newElement`),
    /// opened on release.
    TextCreate { id: String },
    /// The text tool's press on an empty container's centre, decided on
    /// release (`AppTextTool.pending`).
    TextLabel { container: String, origin: [f64; 2] },
}

/// The element a drawing tool's press created (`appState.newElement`) and
/// the press (`pointerDownState`).
#[derive(Clone, Debug)]
pub(crate) struct CreateGesture {
    id: String,
    /// `activeTool.type`.
    tool: String,
    /// `pointerDownState.origin`.
    origin: [f64; 2],
    /// `pointerDownState.originInGrid`.
    origin_in_grid: [f64; 2],
    /// `pointerDownState.drag.hasOccurred` (linear elements).
    dragged: bool,
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
    pub(crate) hit_cache: HitTestCache,
    pub(crate) events: Vec<HostEvent>,
    pub(crate) reported: (f64, bool),
    /// The open text editor (`textWysiwyg`), if any.
    pub(crate) text_editor: Option<TextEditor>,
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
            hit_cache: HitTestCache::new(),
            events: Vec::new(),
            reported: (0.0, false),
            text_editor: None,
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

    /// A `change` event when the scene or the dirty flag moved since the
    /// last one.
    pub(crate) fn report(&mut self) {
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
        self.apply(scene, app_state);
        for effect in &out.effects {
            match effect {
                KeyEffect::Action(KeyDownOutcome::Perform(name)) => self.perform_action(*name),
                KeyEffect::Scrolled(translation) => self.set_viewport_to(translation.viewport),
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
        self.apply(scene, app_state);
        self.report();
        out
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
    /// dialog and zen mode.
    pub fn perform_action(&mut self, name: ActionName) {
        match name {
            ActionName::Undo => return self.undo(),
            ActionName::Redo => return self.redo(),
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
    fn has_bounding_box(selected: &[&Element]) -> bool {
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
            .filter(|e| !is_bound_text(e))
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
            self.set_viewport_to(translation.viewport);
        }
        outcome.prevent_default
    }

    /// A `pointerdown` on the canvas.
    pub fn pointer_down(&mut self, input: PointerInput) {
        self.last_pointer = [input.client_x, input.client_y];
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
            "selection" | "lasso" => self.select_pointer_down(input),
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
    fn binding_app_state(&self, origin: [f64; 2], alt: bool) -> BindingAppState {
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
        let id = self.session.env.random_id();
        let seed = self.session.env.random_integer();
        let now = RestoreEnv::now(&mut self.session.env);
        let Some(element) = new_element_for_tool(
            tool,
            self.session.app_state(),
            origin_in_grid,
            None,
            &id,
            seed,
            now,
        ) else {
            return;
        };
        let linear = matches!(tool, "arrow" | "line");
        // Ok: a new element at the end gets an index after the last
        let _ = self.session.insert_elements_at_index(vec![element], None);
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
        self.gesture = Some(Gesture::Create(CreateGesture {
            id,
            tool: tool.to_owned(),
            origin,
            origin_in_grid,
            dragged: false,
        }));
        self.report();
    }

    /// The selection tool's press: selects the topmost element hit
    /// (`handleSelectionOnPointerDown`), or starts a link click.
    fn select_pointer_down(&mut self, input: PointerInput) {
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
                None,
            );
            if session.handle().is_some() {
                self.gesture = Some(Gesture::Transform(session));
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
        let link = self.link_at(origin);
        let hit = if link.is_some() {
            None
        } else {
            self.element_at(origin)
        };
        if link.is_none() {
            let mut selected = self.selected_ids();
            match &hit {
                Some(id) if selected.contains(id) => {}
                Some(id) if input.shift_key => selected.push(id.clone()),
                Some(id) => selected = vec![id.clone()],
                None if input.shift_key => {}
                None => selected.clear(),
            }
            self.set_selection(&selected);
            self.session.commit();
        }
        // the selection element (`createGenericElementOnPointerDown`)
        let box_origin = (hit.is_none() && link.is_none())
            .then(|| get_grid_point(origin[0], origin[1], self.grid_size(input.ctrl_or_cmd)));
        self.gesture = Some(Gesture::Select(SelectGesture {
            origin,
            originals,
            hit,
            link,
            previous_selection,
            dragged: false,
            box_origin,
            box_selected: false,
        }));
        self.report();
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
                let translation = translate(&state, Some(update), TranslateOptions::default());
                self.set_viewport_to(translation.viewport);
            }
            Some(Gesture::Select(g)) if g.box_origin.is_some() => self.box_select(input),
            Some(Gesture::Select(_)) => self.drag_selection(input),
            Some(Gesture::Transform(_)) => self.transform(input),
            Some(Gesture::Create(_)) => self.create_pointer_move(input),
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
            Some(Gesture::TextCreate { .. } | Gesture::TextLabel { .. }) | None => {}
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
                let [gx, gy] = get_grid_point(pointer[0], pointer[1], grid);
                let [ox, oy] = gesture.origin_in_grid;
                let Some([x, y, width, height]) = drag_new_element(&DragNewElement {
                    element: &element,
                    element_type: &gesture.tool,
                    origin: [ox, oy],
                    pointer: [gx, gy],
                    width: (gx - ox).abs(),
                    height: (gy - oy).abs(),
                    maintain_aspect_ratio: input.shift_key,
                    resize_from_center: input.alt_key,
                    width_aspect_ratio: None,
                }) else {
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
        scene.mutate_element(&element.base.id, update, &mut self.session.env);
        let app_state = self.session.app_state().clone();
        self.apply(scene, app_state);
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
            self.session.commit();
        }
        self.report();
    }

    /// A move on a handle: `maybeHandleResize` through the transform
    /// session, the pointer on the grid unless Ctrl/Cmd is held; Shift
    /// keeps the aspect ratio and snaps the angle, Alt resizes from the
    /// centre.
    fn transform(&mut self, input: PointerInput) {
        let point = self.scene_point(input.client_x, input.client_y);
        let grid = self.grid_size(false);
        let Some(Gesture::Transform(session)) = self.gesture.as_ref() else {
            return;
        };
        let session = session.clone();
        let mut scene = Scene::new(self.session.elements().to_vec());
        let app_state = self.session.app_state().clone();
        let transformed = session.update(
            &mut scene,
            &mut self.session.env,
            point,
            TransformModifiers {
                shift: input.shift_key,
                alt: input.alt_key,
                ctrl: input.ctrl_or_cmd,
            },
            grid,
        );
        if transformed {
            self.apply(scene, app_state);
            self.report();
        }
    }

    fn drag_selection(&mut self, input: PointerInput) {
        let point = self.scene_point(input.client_x, input.client_y);
        let Some(Gesture::Select(gesture)) = self.gesture.as_mut() else {
            return;
        };
        if gesture.hit.is_none() {
            return;
        }
        let offset = [point[0] - gesture.origin[0], point[1] - gesture.origin[1]];
        if offset == [0.0, 0.0] && !gesture.dragged {
            return;
        }
        gesture.dragged = true;
        let originals = gesture.originals.clone();
        let selected = self.selected_ids();
        let grid = self.grid_size(input.ctrl_or_cmd);
        let mut scene = Scene::new(self.session.elements().to_vec());
        let app_state = self.session.app_state().clone();
        drag_selected_elements(
            &originals,
            &selected,
            offset,
            &mut scene,
            grid,
            &mut self.session.env,
        );
        self.apply(scene, app_state);
        self.report();
    }

    /// The `pointerup` ending the press.
    pub fn pointer_up(&mut self, input: PointerInput) {
        self.last_pointer = [input.client_x, input.client_y];
        match self.gesture.take() {
            None | Some(Gesture::Pan { .. }) => {}
            Some(Gesture::Select(gesture)) => self.select_pointer_up(input, gesture),
            Some(Gesture::Transform(_)) => {
                self.session.store.schedule_capture();
                self.session.commit();
                self.report();
            }
            Some(Gesture::Create(gesture)) => self.create_pointer_up(input, gesture),
            Some(Gesture::Erase { start, pending, .. }) => {
                self.erase_pointer_up(input, start, pending);
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
            if !gesture.dragged || distance < MINIMUM_ARROW_SIZE {
                // upstream starts drawing point by point here; the element
                // does not, and drops the element
                return self.discard_new_element(&element.base.id);
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
        let options = SvgExportOptions::new(&self.source, &metrics);
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
            ..StaticCanvasAppState::default()
        };
        let config = StaticCanvasRenderConfig::default();
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

/// A [`TextMetricsProvider`] as the SVG export's frame labels measure.
struct Measure<'a, P>(&'a P);

impl<P: TextMetricsProvider> excali_scene::export::TextMetrics for Measure<'_, P> {
    fn measure(&self, text: &str, font: &str) -> f64 {
        self.0.get_line_width(text, font)
    }
}
