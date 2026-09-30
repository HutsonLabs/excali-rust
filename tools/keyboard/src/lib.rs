//! ex-515 harness: excali-editor's keyboard handling in the browser.
//!
//! [`Keyboard`] holds an editor (scene, app state, tool state, the action
//! manager) and takes the page's real events: `keydown` and `keyup`
//! through [`excali_ui::keyboard::keystroke`] into
//! [`excali_editor::keyboard::on_key_down`] and [`on_key_up`] (the
//! outcome applied back to the event), the command palette's capture
//! listener, `copy` / `cut` / `paste`, and pointer presses (which modifier
//! helpers hold, whether a pan starts). The shortcut actions' performs
//! (deselect, the flips, toggleElementLock, copy and paste styles,
//! viewMode, toggleTheme) run as `<excali-editor>` runs them
//! ([`excali_editor::edit_actions::perform_shortcut_action`]).
//! [`Keyboard::state`] reports the editor as JSON for the Playwright suite
//! `tests/web/keyboard`.
//! `scripts/web/keyboard.sh` builds it.

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_core::restore::RestoreEnv;
use excali_editor::actions::{ActionEnv, ActionManager, AppProps, KeyDownOutcome};
use excali_editor::binding::{update_bound_elements_in_map, BindingEnv};
use excali_editor::edit_actions::{perform_shortcut_action, ShortcutHost, StyleEnv};
use excali_editor::flowchart::{insertion_index, insertion_runs, AppFlowchart, FlowchartOperation};
use excali_editor::keyboard::{
    command_palette_key_down, on_clipboard_event, on_key_down, on_key_up, pan_starts,
    should_maintain_aspect_ratio, should_resize_from_center, should_rotate_with_discrete_angle,
    ClipboardEventKind, ClipboardOutcome, KeyEffect, KeyOutcome, KeyboardEditor, KeyboardState,
    PanStart,
};
use excali_editor::resize_elements::{
    sticky_note_layout, StickyNoteLayout, StickyNoteLayoutOpts, TransformEnv,
};
use excali_editor::scene::{MutationEnv, Scene};
use excali_editor::text_layout::TextLayouter;
use excali_editor::tools::{ActiveTool, ToolKeyAction, ToolKeyOutcome, ToolState};
use excali_text::text_measurements::{CharWidthCache, TextMetricsProvider};
use excali_ui::keyboard::{apply_outcome, clipboard_target, keystroke, mouse_modifiers};
use serde_json::{json, Map, Value};
use wasm_bindgen::prelude::*;
use web_sys::{Element as DomElement, Event, KeyboardEvent, PointerEvent};

/// Upstream's test text metrics: 10 px per UTF-16 code unit (arrow labels
/// re-wrapped when a nudge moves a bound arrow).
#[derive(Clone, Copy)]
struct TenPxPerCodeUnit;

impl TextMetricsProvider for TenPxPerCodeUnit {
    fn get_line_width(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

/// `randomInteger()` from a counter, `randomId()` as `id0`, `id1`, ...
/// and `getUpdatedTimestamp()` = 1, as in upstream's tests; text laid out
/// under the same metric.
struct Env {
    nonce: f64,
    ids: u32,
    char_widths: CharWidthCache,
    layouter: TextLayouter<TenPxPerCodeUnit>,
}

impl Default for Env {
    fn default() -> Env {
        Env {
            nonce: 0.0,
            ids: 0,
            char_widths: CharWidthCache::default(),
            layouter: TextLayouter::new(TenPxPerCodeUnit),
        }
    }
}

/// A version stamp from the counter, for the text layout's mutations.
struct Counter<'a>(&'a mut f64);

impl ChangeStamp for Counter<'_> {
    fn version_nonce(&mut self) -> f64 {
        *self.0 += 1.0;
        *self.0
    }

    fn updated(&mut self) -> f64 {
        1.0
    }
}

/// The counter and the metric as a [`BindingEnv`], for the arrows a
/// label's layout moves.
struct CounterBinding<'a> {
    stamp: &'a mut dyn ChangeStamp,
    char_widths: &'a mut CharWidthCache,
}

impl MutationEnv for CounterBinding<'_> {
    fn random_integer(&mut self) -> f64 {
        self.stamp.version_nonce()
    }

    fn now(&mut self) -> f64 {
        1.0
    }
}

impl BindingEnv for CounterBinding<'_> {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (&TenPxPerCodeUnit, self.char_widths)
    }
}

impl StyleEnv for Env {
    fn redraw_text_bounding_box(
        &mut self,
        elements: &mut SceneElementsMap,
        text_id: &str,
        container_id: Option<&str>,
    ) -> Result<(), String> {
        let char_widths = &mut self.char_widths;
        let mut stamp = Counter(&mut self.nonce);
        self.layouter.redraw_text_bounding_box(
            &mut stamp,
            elements,
            text_id,
            container_id,
            &mut |stamp, elements, id| {
                let mut env = CounterBinding {
                    stamp,
                    char_widths: &mut *char_widths,
                };
                update_bound_elements_in_map(elements, id, &SceneElementsMap::new(), &mut env);
                Ok(())
            },
        )
    }
}

impl TransformEnv for Env {
    fn sticky_note_layout(
        &mut self,
        container: &Element,
        text: Option<&Element>,
        opts: &StickyNoteLayoutOpts,
    ) -> StickyNoteLayout {
        self.layouter
            .with_layout(|layout, _| sticky_note_layout(layout, container, text, opts))
    }
}

impl RestoreEnv for Env {
    fn now(&mut self) -> f64 {
        1.0
    }

    fn random_id(&mut self) -> String {
        let id = format!("id{}", self.ids);
        self.ids += 1;
        id
    }

    fn random_integer(&mut self) -> f64 {
        MutationEnv::random_integer(self)
    }
}

impl ChangeStamp for Env {
    fn version_nonce(&mut self) -> f64 {
        MutationEnv::random_integer(self)
    }

    fn updated(&mut self) -> f64 {
        1.0
    }
}

impl MutationEnv for Env {
    fn random_integer(&mut self) -> f64 {
        self.nonce += 1.0;
        self.nonce
    }

    fn now(&mut self) -> f64 {
        1.0
    }
}

impl BindingEnv for Env {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (&TenPxPerCodeUnit, &mut self.char_widths)
    }
}

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Elements from a JSON array of `.excalidraw` elements.
fn parse_elements(json: &str) -> Result<Vec<Element>, JsValue> {
    let value: Value = serde_json::from_str(json).map_err(js_err)?;
    let list = value
        .as_array()
        .ok_or_else(|| js_err("expected a JSON array of elements"))?;
    list.iter()
        .map(|v| {
            let map = v
                .as_object()
                .ok_or_else(|| js_err("an element is not an object"))?;
            Element::from_map(map.clone()).map_err(js_err)
        })
        .collect()
}

/// A key effect as JSON: `{ type, ... }`.
fn effect_json(effect: &KeyEffect) -> Value {
    match effect {
        KeyEffect::Action(outcome) => match outcome {
            KeyDownOutcome::Perform(a) => json!({ "type": "action", "action": a.as_str() }),
            KeyDownOutcome::Swallowed(a) => json!({ "type": "swallowed", "action": a.as_str() }),
            KeyDownOutcome::Ambiguous(list) => json!({
                "type": "ambiguous",
                "actions": list.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
            }),
            KeyDownOutcome::Unhandled => json!({ "type": "unhandled" }),
        },
        KeyEffect::Tool(outcome) => match outcome {
            ToolKeyOutcome::Tool { tool, action, .. } => json!({
                "type": "tool",
                "tool": tool.as_str(),
                "cycleBucketFillColor": matches!(action, ToolKeyAction::CycleBucketFillColor),
            }),
            ToolKeyOutcome::ToggledLock { changed } => {
                json!({ "type": "toolLock", "changed": changed })
            }
            other => json!({ "type": "tool", "debug": format!("{other:?}") }),
        },
        KeyEffect::ExecuteAction(a) => json!({ "type": "executeAction", "action": a.as_str() }),
        KeyEffect::StartTextEditing {
            scene_x,
            scene_y,
            container,
        } => json!({
            "type": "startTextEditing",
            "sceneX": scene_x,
            "sceneY": scene_y,
            "container": container,
        }),
        KeyEffect::FlowchartCreate { start, direction } => json!({
            "type": "flowchartCreate",
            "start": start,
            "direction": direction.as_str(),
        }),
        KeyEffect::FlowchartNavigate { from, direction } => json!({
            "type": "flowchartNavigate",
            "from": from,
            "direction": direction.as_str(),
        }),
        KeyEffect::OpenEyeDropper(kind) => json!({
            "type": "openEyeDropper",
            "kind": format!("{kind:?}").to_lowercase(),
        }),
        KeyEffect::ConvertElementType {
            conversion,
            direction,
        } => json!({
            "type": "convertElementType",
            "conversion": conversion.map(|c| format!("{c:?}").to_lowercase()),
            "direction": format!("{direction:?}").to_lowercase(),
        }),
        KeyEffect::Cursor(c) => {
            json!({ "type": "cursor", "cursor": format!("{c:?}").to_lowercase() })
        }
        other => {
            // unit variants and the rest: the variant name
            let debug = format!("{other:?}");
            let name = debug.split(['(', ' ', '{']).next().unwrap_or("").to_owned();
            let mut chars = name.chars();
            let camel = match chars.next() {
                Some(c) => c.to_lowercase().chain(chars).collect::<String>(),
                None => String::new(),
            };
            json!({ "type": camel })
        }
    }
}

fn outcome_json(out: &KeyOutcome) -> String {
    json!({
        "preventDefault": out.prevent_default,
        "stopPropagation": out.stop_propagation,
        "effects": out.effects.iter().map(effect_json).collect::<Vec<_>>(),
    })
    .to_string()
}

/// The editor behind the page.
#[wasm_bindgen]
pub struct Keyboard {
    container: DomElement,
    scene: Scene,
    app_state: AppState,
    tools: ToolState,
    keyboard: KeyboardState,
    actions: ActionManager,
    props: AppProps,
    env: ActionEnv,
    binding: Env,
    /// `App.flowchart`: the creator and navigator answering the flowchart
    /// keys.
    flowchart: AppFlowchart,
    pointer: (f64, f64),
    /// `copiedStyles`, kept across loads as upstream's module keeps it.
    shortcuts: ShortcutHost,
}

#[wasm_bindgen]
impl Keyboard {
    /// An empty editor on `container` (the focusable `.excalidraw`
    /// element). `is_darwin` is upstream's `isDarwin`.
    #[wasm_bindgen(constructor)]
    pub fn new(container: DomElement, is_darwin: bool) -> Keyboard {
        Keyboard {
            container,
            scene: Scene::default(),
            app_state: AppState::default(),
            tools: ToolState::default(),
            keyboard: KeyboardState::default(),
            actions: ActionManager::new(),
            props: AppProps::default(),
            env: ActionEnv {
                is_darwin,
                ..ActionEnv::default()
            },
            binding: Env::default(),
            flowchart: AppFlowchart::default(),
            pointer: (0.0, 0.0),
            shortcuts: ShortcutHost::default(),
        }
    }

    /// Loads `elements` (a JSON array) with `app_state` (a JSON object of
    /// keys set over the default app state) and `props` (`{
    /// gridModeEnabled, viewModeEnabled }`).
    pub fn load(&mut self, elements: &str, app_state: &str, props: &str) -> Result<(), JsValue> {
        self.scene = Scene::new(parse_elements(elements)?);
        self.app_state = AppState::default();
        let keys: Map<String, Value> = serde_json::from_str(app_state).map_err(js_err)?;
        for (k, v) in keys {
            self.app_state.insert(k, v);
        }
        let props: Value = serde_json::from_str(props).map_err(js_err)?;
        self.props = AppProps {
            grid_mode_enabled: props["gridModeEnabled"].as_bool(),
            view_mode_enabled: props["viewModeEnabled"].as_bool(),
            ..AppProps::default()
        };
        // the component settles toggleTheme with no theme prop
        // (index.tsx:142-147)
        self.props.canvas_actions.normalize(false, false);
        self.tools = ToolState::default();
        self.keyboard = KeyboardState::default();
        self.flowchart = AppFlowchart::default();
        Ok(())
    }

    /// Sets upstream's `isDarwin` (which modifier is CTRL_OR_CMD, and the
    /// Mac-only shortcuts).
    #[wasm_bindgen(js_name = setDarwin)]
    pub fn set_darwin(&mut self, is_darwin: bool) {
        self.env.is_darwin = is_darwin;
    }

    fn editor(&mut self) -> (KeyboardEditor<'_>, &mut Env) {
        (
            KeyboardEditor {
                scene: &mut self.scene,
                app_state: &mut self.app_state,
                tools: &mut self.tools,
                keyboard: &mut self.keyboard,
                actions: &self.actions,
                props: &self.props,
                env: &self.env,
            },
            &mut self.binding,
        )
    }

    /// The document's `keydown`: `App.onKeyDown`. Returns the outcome.
    #[wasm_bindgen(js_name = keyDown)]
    pub fn key_down(&mut self, event: &KeyboardEvent) -> String {
        let stroke = keystroke(event, Some(&self.container));
        let (mut ed, env) = self.editor();
        let out = on_key_down(&mut ed, env, &stroke);
        apply_outcome(event, &out);
        self.answer_flowchart(&out);
        for effect in &out.effects {
            if let KeyEffect::Action(KeyDownOutcome::Perform(name)) = effect {
                self.perform(*name);
            }
        }
        outcome_json(&out)
    }

    /// `<excali-editor>`'s perform of a shortcut action (the tools live in
    /// [`ToolState`]); other actions are their features'.
    fn perform(&mut self, name: excali_editor::actions::ActionName) {
        let mut app_state = self.app_state.clone();
        app_state.insert("activeTool", self.tools.active_tool.to_json());
        let preferred = &self.tools.preferred_selection_tool;
        app_state.insert(
            "preferredSelectionTool",
            json!({
                "type": preferred.tool.tool_type().as_str(),
                "initialized": preferred.initialized,
            }),
        );
        let Some(mut result) = perform_shortcut_action(
            name,
            self.scene.elements(),
            &app_state,
            &Value::Null,
            &mut self.shortcuts,
            &mut self.binding,
        ) else {
            return;
        };
        if let Some(tool) = result.app_state.remove("activeTool") {
            if let Some(tool) = ActiveTool::from_json(&tool) {
                self.tools.active_tool = tool;
            }
        }
        if let Some(elements) = result.elements {
            self.scene = Scene::new(elements);
        }
        for (key, value) in result.app_state {
            self.app_state.insert(key, value);
        }
    }

    /// `AppFlowchart.handleKeyEvent`'s answer to the flowchart effects:
    /// the pending nodes kept, the node found selected, the committed
    /// nodes inserted and the first selected.
    fn answer_flowchart(&mut self, out: &KeyOutcome) {
        for effect in &out.effects {
            let op = self.flowchart.answer(
                effect,
                &mut self.scene,
                &self.app_state,
                &mut self.keyboard.flowchart,
                &mut self.binding,
            );
            match op {
                Some(FlowchartOperation::Navigating { node_id: Some(id) }) => self.select(&id),
                Some(FlowchartOperation::Committed { nodes }) => {
                    let first = nodes.first().map(|n| n.base.id.clone());
                    for run in insertion_runs(nodes) {
                        let mut elements = self.scene.elements().to_vec();
                        let at = insertion_index(&elements, &run).unwrap_or(elements.len());
                        elements.splice(at..at, run);
                        self.scene = Scene::new(elements);
                    }
                    if let Some(id) = first {
                        self.select(&id);
                    }
                }
                _ => {}
            }
        }
    }

    /// Selects the element `id` alone.
    pub fn select(&mut self, id: &str) {
        self.app_state
            .insert("selectedElementIds", json!({ id: true }));
    }

    /// The document's `keyup`: `App.onKeyUp`.
    #[wasm_bindgen(js_name = keyUp)]
    pub fn key_up(&mut self, event: &KeyboardEvent) -> String {
        let stroke = keystroke(event, Some(&self.container));
        let (mut ed, env) = self.editor();
        let out = on_key_up(&mut ed, env, &stroke);
        apply_outcome(event, &out);
        self.answer_flowchart(&out);
        self.flowchart.after_key_up(&self.keyboard.flowchart);
        outcome_json(&out)
    }

    /// The command palette's window keydown listener (capture phase).
    #[wasm_bindgen(js_name = commandPalette)]
    pub fn command_palette(&mut self, event: &KeyboardEvent) -> String {
        let stroke = keystroke(event, Some(&self.container));
        let out = command_palette_key_down(&mut self.app_state, &stroke, self.env.is_darwin);
        apply_outcome(event, &out);
        outcome_json(&out)
    }

    /// A document `copy`, `cut` or `paste` event.
    pub fn clipboard(&mut self, event: &Event, kind: &str) -> String {
        let kind = match kind {
            "copy" => ClipboardEventKind::Copy,
            "cut" => ClipboardEventKind::Cut,
            _ => ClipboardEventKind::Paste,
        };
        let target = clipboard_target(
            event,
            Some(&self.container),
            self.pointer,
            kind == ClipboardEventKind::Paste,
        );
        let outcome = on_clipboard_event(&self.tools, &self.keyboard, kind, target);
        match outcome {
            ClipboardOutcome::Ignored => json!({ "type": "ignored" }),
            ClipboardOutcome::Action(a) => {
                event.prevent_default();
                event.stop_propagation();
                json!({ "type": "action", "action": a.as_str() })
            }
            ClipboardOutcome::Paste { plain } => json!({ "type": "paste", "plain": plain }),
        }
        .to_string()
    }

    /// A `pointermove`: the last pointer position (`viewport.lastPosition`).
    #[wasm_bindgen(js_name = pointerMove)]
    pub fn pointer_move(&mut self, event: &PointerEvent) {
        self.pointer = (f64::from(event.client_x()), f64::from(event.client_y()));
    }

    /// A `pointerdown` on the canvas: the modifier helpers and whether it
    /// starts a pan.
    #[wasm_bindgen(js_name = pointerDown)]
    pub fn pointer_down(&mut self, event: &PointerEvent) -> String {
        self.pointer = (f64::from(event.client_x()), f64::from(event.client_y()));
        let m = mouse_modifiers(event);
        let pan = pan_starts(
            &self.tools,
            PanStart {
                button: event.button(),
                pointer_count: 1,
                interaction_enabled: self.tools.is_interaction_enabled(),
                navigation_enabled: true,
                view_mode_enabled: self
                    .app_state
                    .get("viewModeEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                active_tool_pointer_capturing: false,
            },
        );
        json!({
            "shiftKey": m.shift_key,
            "altKey": m.alt_key,
            "ctrlOrCmd": m.ctrl_or_cmd(self.env.is_darwin),
            "maintainAspectRatio": should_maintain_aspect_ratio(m),
            "resizeFromCenter": should_resize_from_center(m),
            "rotateWithDiscreteAngle": should_rotate_with_discrete_angle(m),
            "pan": pan,
        })
        .to_string()
    }

    /// The editor as JSON: the active tool, the app state keys the keyboard
    /// writes, the keyboard state and the elements' positions.
    pub fn state(&self) -> String {
        let keys = [
            "openDialog",
            "openPopup",
            "scrollX",
            "scrollY",
            "zoom",
            "currentItemArrowType",
            "isBindingEnabled",
            "bindingPreference",
            "editingFrame",
            "selectedElementIds",
            "viewModeEnabled",
            "bindMode",
            "theme",
            "activeLockedId",
            "toast",
        ];
        let app_state: Map<String, Value> = keys
            .iter()
            .map(|k| {
                (
                    (*k).to_owned(),
                    self.app_state.get(k).cloned().unwrap_or(Value::Null),
                )
            })
            .collect();
        let elements: Vec<Value> = self
            .scene
            .elements()
            .iter()
            .map(|e| {
                let map = e.to_map();
                json!({
                    "id": e.base.id,
                    "type": e.element_type().as_str(),
                    "x": e.base.x,
                    "y": e.base.y,
                    "width": e.base.width,
                    "height": e.base.height,
                    "version": e.base.version,
                    "locked": e.base.locked,
                    "strokeColor": e.base.stroke_color,
                    "strokeStyle": map.get("strokeStyle"),
                    "opacity": map.get("opacity"),
                    "boundElements": map.get("boundElements"),
                    "startBinding": map.get("startBinding"),
                    "endBinding": map.get("endBinding"),
                })
            })
            .collect();
        json!({
            "activeTool": self.tools.active_tool.to_json(),
            "appState": app_state,
            "spaceHeld": self.tools.space_held,
            "keyboard": {
                "isPlainPaste": self.keyboard.is_plain_paste,
                "convertPopupOpen": self.keyboard.convert_popup_open,
                "activeConfirmDialog": self.keyboard.active_confirm_dialog,
                "isCreatingChart": self.keyboard.flowchart.is_creating_chart,
                "isExploring": self.keyboard.flowchart.is_exploring,
            },
            "pendingFlowchartNodes": self
                .flowchart
                .pending_nodes()
                .iter()
                .map(|e| json!({ "id": e.base.id, "type": e.element_type().as_str(), "x": e.base.x, "y": e.base.y }))
                .collect::<Vec<_>>(),
            "elements": elements,
        })
        .to_string()
    }
}
