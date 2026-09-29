//! The editor behind `<excali-editor>`, without the DOM: the scene with its
//! store and history, the tools, the keyboard, pointer selection and
//! dragging, and the host API of the term.hut integration page
//! (`site/content/architecture/termhut-integration.md`): `load`, `save`,
//! `export`, `importLibrary`, `getState`, and the `change`, `save-request`
//! and `open-link` events.
//!
//! Upstream counterpart: `App` (`packages/excalidraw/components/App.tsx`)
//! as the `Excalidraw` component mounts it, reduced to what the host API
//! needs. Every step goes through the ported code:
//!
//! - **Load** is `loadFromBlob` (`data/blob.ts:137-216`,
//!   [`load_scene_json`]) then `initializeScene`'s `updateScene` without
//!   capture ([`Session::initialize_scene`]).
//! - **Save** is `serializeAsJSON(elements, appState, files, "local")`
//!   (`data/json.ts:52-75`, [`LoadedScene::to_document`]): the elements
//!   deleted ones included, as `actionSaveToActiveFile` passes them.
//! - **Keys** go through `App.onKeyDown` ([`on_key_down`]); the undo and
//!   redo actions it names run [`Session::undo`] and [`Session::redo`].
//!   Cmd+S (Ctrl+S elsewhere) is the host's: upstream's
//!   `saveToActiveFile` writes to the file handle, which the host owns, so
//!   the editor asks with `save-request`.
//! - **Pointer**: a press selects the topmost element hit
//!   (`getElementAtPosition` over `App.hitElement`, [`hit_element`]); a move
//!   drags the selection ([`drag_selected_elements`]); the release captures
//!   when something is selected or the selection changed
//!   (`App.tsx:12553-12566`). A press and release on an element's link icon
//!   (`isPointHittingLink`, `hyperlink/helpers.ts:61-105`) is upstream's
//!   `onLinkOpen`: the `open-link` event, the host deciding.
//! - After every step the store commits (`componentDidUpdate`), and a
//!   `change` event reports whether the file [`Editor::save`] would write
//!   differs from the one loaded or last saved.
//!
//! The history's leaf layouts are the real ones ([`EditorEnv`]), so an
//! undo re-wraps and re-centres bound text and re-routes bound arrows.

use std::collections::HashMap;

use excali_core::app_state::{AppState, AppStateEnv};
use excali_core::document::{load_scene_json, LoadSceneError, LoadedScene};
use excali_core::element::{Element, ElementKind};
use excali_core::library::{
    merge_library_items, parse_library_json, serialize_library_as_json, LibraryItem,
    LibraryItemStatus,
};
use excali_core::library_url::{parse_library_tokens_from_url, validate_library_url};
use excali_core::restore::{LegacyBinding, LegacyBindingRequest, RestoreEnv};
use excali_editor::actions::{ActionEnv, ActionManager, ActionName, AppProps, KeyDownOutcome};
use excali_editor::collision::{hit_element, HitTestCache};
use excali_editor::keyboard::{
    get_selected_elements, on_key_down, on_key_up, KeyEffect, KeyOutcome, KeyboardEditor,
    KeyboardState, Keystroke,
};
use excali_editor::restore_env::RoutingEnv;
use excali_editor::scene::Scene;
use excali_editor::session::Session;
use excali_editor::tools::ToolState;
use excali_editor::viewport::{viewport_coords_to_scene_coords, ViewportState};
use excali_scene::bounds::{get_element_absolute_coords, ElementsMap};
use excali_scene::canvas_export::{export_canvas_png, CanvasExportOptions, CanvasSizing};
use excali_scene::display::{CanvasDocument, DisplayList};
use excali_scene::export::{svg_document, SvgExportAppState, SvgExportOptions};
use excali_scene::render_element::get_link_handle_from_coords;
use excali_scene::shape::Theme;
use excali_scene::static_scene::{
    render_static_scene, StaticCanvasAppState, StaticCanvasRenderConfig, StaticScene,
};
use excali_svg::{export_to_svg, to_svg_file, FontContent};
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{json, Map, Value};

use crate::drag::drag_selected_elements;
use crate::env::EditorEnv;

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

/// A press on the canvas, until its release (`pointerDownState`).
#[derive(Clone, Debug)]
struct Gesture {
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
    session: Session<EditorEnv<P>>,
    /// The loaded file: what the save writes around the scene (its unknown
    /// top-level keys, the form of its `files`).
    file: LoadedScene,
    library: Vec<LibraryItem>,
    tools: ToolState,
    keyboard: KeyboardState,
    actions: ActionManager,
    props: AppProps,
    action_env: ActionEnv,
    /// `getExportSource()`: the page's origin.
    source: String,
    /// What [`Editor::save`] wrote, or the load gave.
    clean: String,
    /// The viewport keys the host measured (`width`, `height`,
    /// `offsetLeft`, `offsetTop`), kept across loads.
    viewport: Map<String, Value>,
    gesture: Option<Gesture>,
    hit_cache: HitTestCache,
    events: Vec<HostEvent>,
    reported: (f64, bool),
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
            hit_cache: HitTestCache::new(),
            events: Vec::new(),
            reported: (0.0, false),
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

    fn selected_ids(&self) -> Vec<String> {
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
    fn report(&mut self) {
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
    fn apply(&mut self, scene: Scene, app_state: AppState) {
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
                KeyEffect::Action(KeyDownOutcome::Perform(ActionName::Undo)) => self.undo(),
                KeyEffect::Action(KeyDownOutcome::Perform(ActionName::Redo)) => self.redo(),
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

    fn scene_point(&self, client_x: f64, client_y: f64) -> [f64; 2] {
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
    fn element_at(&mut self, point: [f64; 2]) -> Option<String> {
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

    fn set_selection(&mut self, ids: &[String]) {
        let selection: Map<String, Value> = ids
            .iter()
            .map(|id| (id.clone(), Value::Bool(true)))
            .collect();
        let mut patch = Map::new();
        patch.insert("selectedElementIds".into(), Value::Object(selection));
        self.session.set_state(patch);
    }

    /// A primary `pointerdown` on the canvas at client coordinates, with
    /// Shift held or not.
    pub fn pointer_down(&mut self, client_x: f64, client_y: f64, shift: bool) {
        if !self.tools.is_interaction_enabled() {
            return;
        }
        let origin = self.scene_point(client_x, client_y);
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
                Some(id) if shift => selected.push(id.clone()),
                Some(id) => selected = vec![id.clone()],
                None if shift => {}
                None => selected.clear(),
            }
            self.set_selection(&selected);
            self.session.commit();
        }
        self.gesture = Some(Gesture {
            origin,
            originals,
            hit,
            link,
            previous_selection,
            dragged: false,
        });
        self.report();
    }

    /// A `pointermove` at client coordinates: drags the selection while a
    /// press that hit an element lasts.
    pub fn pointer_move(&mut self, client_x: f64, client_y: f64) {
        let point = self.scene_point(client_x, client_y);
        let Some(gesture) = self.gesture.as_mut() else {
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
        let grid = self
            .props
            .grid_mode_enabled
            .or_else(|| self.session.app_state().grid_mode_enabled())
            .unwrap_or(false)
            .then(|| self.session.app_state().grid_size())
            .flatten();
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

    /// The `pointerup` ending the press, at client coordinates.
    pub fn pointer_up(&mut self, client_x: f64, client_y: f64) {
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        let point = self.scene_point(client_x, client_y);
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
