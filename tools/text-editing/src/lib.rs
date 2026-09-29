//! ex-512 harness: the text editor overlay in the browser.
//!
//! [`TextEditing`] loads one session of upstream's text editing fixture
//! (`crates/excali-editor/tests/fixtures/text-editing.json`, written by
//! `tools/goldens/text-editing.mjs`): its scene and app state go into an
//! `excali_editor` session with upstream's test metric (10 px per UTF-16
//! code unit), and [`TextEditing::start`] starts text editing as the case
//! does and mounts `excali_ui::text_editor`'s textarea in the page's
//! editor box. The Playwright suite `tests/web/text-editing` then types
//! into the textarea with the keyboard and reads the elements, the app
//! state, the editor and what it asked of the app back as JSON, to hold
//! them to upstream's records. `scripts/web/text-editing.sh` builds it.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_editor::mutate::mutate_element;
use excali_editor::session::Session;
use excali_editor::store::HistoryEnv;
use excali_editor::text_editing::{
    start_text_editing, StartTextEditing, TextEditingContext, TextEditingHost, TextTarget,
};
use excali_editor::text_layout::TextLayouter;
use excali_text::text_measurements::TextMetricsProvider;
use excali_ui::text_editor::{
    measure_caret_offset, TextEditingApp, TextEditorOverlay, TextareaState, TEXT_EDITOR_CSS,
};
use serde_json::{json, Map, Value};
use wasm_bindgen::prelude::*;

/// The generator's fixed clock.
const NOW: f64 = 1_700_000_000_000.0;

/// Upstream's test metric: 10 px per UTF-16 code unit.
struct TenPxPerCodeUnit;

impl TextMetricsProvider for TenPxPerCodeUnit {
    fn get_line_width(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

/// The generator's environment: element ids in the order upstream handed
/// them out, the clock fixed; nonces and seeds are drawn and not compared.
struct Env {
    ids: VecDeque<String>,
    deltas: u32,
    nonce: f64,
}

impl ChangeStamp for Env {
    fn version_nonce(&mut self) -> f64 {
        self.nonce += 1.0;
        self.nonce
    }

    fn updated(&mut self) -> f64 {
        NOW
    }
}

impl HistoryEnv for Env {
    fn random_id(&mut self) -> String {
        self.ids.pop_front().unwrap_or_else(|| {
            self.deltas += 1;
            format!("delta-{}", self.deltas)
        })
    }

    fn redraw_text_bounding_box(
        &mut self,
        _: &mut SceneElementsMap,
        _: &str,
        _: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    fn update_bound_elements(
        &mut self,
        _: &mut SceneElementsMap,
        _: &str,
        _: &SceneElementsMap,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// What the case's stand-in App answered: the sidebar and the hit tests.
struct Host {
    sidebar: (f64, f64),
    hit_text: Option<String>,
    hit_frame: Option<String>,
}

impl TextEditingHost for Host {
    fn sidebar_insets(&self) -> (f64, f64) {
        self.sidebar
    }

    fn text_element_at(&self, _: &[Element], _: f64, _: f64) -> Option<String> {
        self.hit_text.clone()
    }

    fn top_layer_frame_at(&self, _: &[Element], _: f64, _: f64) -> Option<String> {
        self.hit_frame.clone()
    }
}

type App = TextEditingApp<Env, TenPxPerCodeUnit, Host>;

fn error(message: impl std::fmt::Display) -> JsError {
    JsError::new(&message.to_string())
}

fn parse(json: &str) -> Result<Value, JsError> {
    serde_json::from_str(json).map_err(error)
}

/// An element of the fixture, with the drawn keys filled in.
fn element(value: &Value) -> Result<Element, JsError> {
    let mut map = value
        .as_object()
        .ok_or_else(|| error("an element is not an object"))?
        .clone();
    map.insert("seed".into(), json!(1));
    map.insert("versionNonce".into(), json!(0));
    map.insert("updated".into(), json!(NOW));
    Element::from_map(map).map_err(error)
}

/// The editor box's and the textarea's stylesheet rules.
#[wasm_bindgen(js_name = textEditorCss)]
pub fn text_editor_css() -> String {
    TEXT_EDITOR_CSS.to_owned()
}

/// One fixture session in the page.
#[wasm_bindgen]
pub struct TextEditing {
    app: Rc<RefCell<App>>,
    case: Value,
    editor_box: web_sys::HtmlElement,
    overlay: Option<TextEditorOverlay>,
}

#[wasm_bindgen]
impl TextEditing {
    /// `new TextEditing(editorBox, caseJson)`: the session's scene and app
    /// state loaded; `editorBox` is the editor's
    /// `.excalidraw-textEditorContainer`.
    #[wasm_bindgen(constructor)]
    pub fn new(editor_box: web_sys::HtmlElement, case_json: &str) -> Result<TextEditing, JsError> {
        let case = parse(case_json)?;
        let initial = &case["initial"];
        let elements = initial["elements"]
            .as_array()
            .ok_or_else(|| error("no elements"))?
            .iter()
            .map(element)
            .collect::<Result<Vec<_>, _>>()?;
        let mut state = Map::new();
        for (key, value) in initial["state"].as_object().cloned().unwrap_or_default() {
            let value = if key == "zoom" {
                json!({ "value": value })
            } else {
                value
            };
            state.insert(key, value);
        }
        let ids = case["ids"]
            .as_array()
            .map(|ids| {
                ids.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let env = Env {
            ids,
            deltas: 0,
            nonce: 0.0,
        };
        let mut session = Session::new(env, AppState::default());
        session.initialize_scene(elements, state).map_err(error)?;
        let sidebar = &case["sidebar"];
        let host = Host {
            sidebar: (
                sidebar["left"].as_f64().unwrap_or(0.0),
                sidebar["right"].as_f64().unwrap_or(0.0),
            ),
            hit_text: case["hitText"].as_str().map(str::to_owned),
            hit_frame: case["hitFrame"].as_str().map(str::to_owned),
        };
        let app = TextEditingApp::new(session, TextLayouter::new(TenPxPerCodeUnit), host);
        Ok(TextEditing {
            app: Rc::new(RefCell::new(app)),
            case,
            editor_box,
            overlay: None,
        })
    }

    /// `startTextEditing` with the case's arguments; the textarea mounted
    /// (and a caret the case asks for measured in the page). Whether an
    /// editor opened.
    pub fn start(&mut self) -> Result<bool, JsError> {
        let start = &self.case["start"];
        let mut args = StartTextEditing::at(
            start["sceneX"].as_f64().unwrap_or(0.0),
            start["sceneY"].as_f64().unwrap_or(0.0),
        );
        args.container = start["container"].as_str().map(str::to_owned);
        if let Some(id) = start["textElement"].as_str() {
            args.text_element = TextTarget::Existing(id.to_owned());
        }
        if let Some(caret) = start["initialCaretSceneCoords"].as_object() {
            args.initial_caret = Some([
                caret["x"].as_f64().unwrap_or(0.0),
                caret["y"].as_f64().unwrap_or(0.0),
            ]);
        }
        let state = {
            let mut app = self.app.borrow_mut();
            let app = &mut *app;
            let editor = start_text_editing(
                &mut TextEditingContext {
                    session: &mut app.session,
                    layouter: &mut app.layouter,
                    host: &mut app.host,
                },
                &args,
            )
            .map_err(error)?;
            app.editor = editor;
            app.editor.as_ref().map(TextareaState::of)
        };
        let Some(state) = state else {
            return Ok(false);
        };
        let overlay = TextEditorOverlay::mount(&self.editor_box, None, &state, self.app.clone())
            .map_err(|e| error(format!("{e:?}")))?;
        let document = self
            .editor_box
            .owner_document()
            .ok_or_else(|| error("no document"))?;
        let caret = {
            let app = self.app.borrow();
            app.editor.as_ref().and_then(|e| e.caret_request().cloned())
        };
        if let Some(request) = caret {
            let offset = measure_caret_offset(&document, &request);
            if let Some(editor) = self.app.borrow_mut().editor.as_mut() {
                editor.resolve_caret(offset);
            }
        }
        self.overlay = Some(overlay);
        Ok(true)
    }

    /// The textarea (`undefined` before [`TextEditing::start`]).
    pub fn textarea(&self) -> Option<web_sys::HtmlTextAreaElement> {
        self.overlay.as_ref().map(|o| o.textarea().clone())
    }

    /// Every element as JSON, without the drawn `seed`, `versionNonce`
    /// and `updated`.
    pub fn elements(&self) -> String {
        let app = self.app.borrow();
        let list: Vec<Value> = app
            .session
            .elements()
            .iter()
            .map(|e| {
                let mut map = e.to_map();
                for key in ["seed", "versionNonce", "updated"] {
                    map.shift_remove(key);
                }
                Value::Object(map)
            })
            .collect();
        Value::Array(list).to_string()
    }

    /// The app state's keys (a JSON array of names) as the fixture records
    /// them: the zoom's value, an element in the state by its id.
    pub fn state(&self, keys_json: &str) -> Result<String, JsError> {
        let keys = parse(keys_json)?;
        let app = self.app.borrow();
        let mut out = Map::new();
        for key in keys
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let value = app
                .session
                .app_state()
                .get(key)
                .cloned()
                .unwrap_or(Value::Null);
            let value = match key {
                "zoom" => value.get("value").cloned().unwrap_or(json!(1)),
                "editingTextElement" | "newElement" | "multiElement" => {
                    value.get("id").cloned().unwrap_or(Value::Null)
                }
                _ => value,
            };
            out.insert(key.to_owned(), value);
        }
        Ok(Value::Object(out).to_string())
    }

    /// The editor: `{ open, value, selection, style }` (`null` without one).
    pub fn editor(&self) -> String {
        let app = self.app.borrow();
        match &app.editor {
            Some(editor) => {
                let (start, end) = editor.selection();
                let style: Map<String, Value> = editor
                    .style()
                    .iter()
                    .map(|(k, v)| (k.clone(), json!(v)))
                    .collect();
                json!({
                    "open": editor.is_open(),
                    "value": editor.value(),
                    "selection": [start, end],
                    "style": style,
                })
                .to_string()
            }
            None => "null".into(),
        }
    }

    /// What the editor asked of the app since the last call (JSON array).
    #[wasm_bindgen(js_name = takeCalls)]
    pub fn take_calls(&self) -> String {
        let calls = std::mem::take(&mut self.app.borrow_mut().calls);
        json!(calls).to_string()
    }

    /// Errors the editor reported (JSON array).
    pub fn errors(&self) -> String {
        json!(self.app.borrow().errors).to_string()
    }

    /// The original container heights, as upstream's cache holds them.
    #[wasm_bindgen(js_name = containerCache)]
    pub fn container_cache(&self) -> String {
        self.app
            .borrow()
            .layouter
            .container_cache
            .to_json()
            .to_string()
    }

    fn after<F>(&mut self, f: F) -> Result<(), JsError>
    where
        F: FnOnce(
            &mut excali_editor::text_editing::TextEditor,
            &mut TextEditingContext<'_, Env, TenPxPerCodeUnit>,
        ) -> Result<(), excali_editor::text_editing::TextEditingError>,
    {
        let state = self.app.borrow_mut().with_editor(f).map(|(_, s)| s);
        let errors = self.app.borrow().errors.clone();
        if let Some(e) = errors.last() {
            return Err(error(e));
        }
        if let (Some(state), Some(overlay)) = (state, &self.overlay) {
            overlay.apply(&state);
        }
        Ok(())
    }

    /// The theme changed (`setState({theme})`, then the app's change).
    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(&mut self, theme: &str) -> Result<(), JsError> {
        let mut state = Map::new();
        state.insert("theme".into(), json!(theme));
        self.app.borrow_mut().session.set_state(state);
        self.after(|editor, ctx| editor.app_changed(ctx))
    }

    /// The canvas was resized to `width` × `height`.
    pub fn resize(&mut self, width: f64, height: f64) -> Result<(), JsError> {
        let mut state = Map::new();
        state.insert("width".into(), json!(width));
        state.insert("height".into(), json!(height));
        self.app.borrow_mut().session.set_state(state);
        self.after(|editor, ctx| editor.relayout(ctx))
    }

    /// An element changed elsewhere (a collaborator): `updates` (JSON)
    /// applied to `id`.
    pub fn mutate(&mut self, id: &str, updates_json: &str) -> Result<(), JsError> {
        let updates = parse(updates_json)?
            .as_object()
            .cloned()
            .ok_or_else(|| error("updates are not an object"))?;
        let result = self.app.borrow_mut().session.edit_elements(|map, env| {
            let mut element = map
                .get(id)
                .cloned()
                .ok_or_else(|| format!("no element {id}"))?;
            mutate_element(&mut element, map, updates, env).map_err(|e| e.to_string())?;
            map.insert(id.to_owned(), element);
            Ok::<(), String>(())
        });
        result.map_err(error)?;
        self.after(|editor, ctx| editor.relayout(ctx))
    }

    /// The editor box scrolled by `left` × `top` to reveal the caret.
    #[wasm_bindgen(js_name = boxScrolled)]
    pub fn box_scrolled(&mut self, left: f64, top: f64) -> Result<(), JsError> {
        self.after(|editor, ctx| editor.editor_box_scrolled(ctx, left, top))
    }
}
