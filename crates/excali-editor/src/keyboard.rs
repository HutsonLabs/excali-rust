//! Keyboard handling: `App.onKeyDown` and `App.onKeyUp`
//! (`packages/excalidraw/components/App.tsx:5585-6207`), the keys the
//! actions registry does not own, and the modifier helpers of
//! `packages/common/src/keys.ts:138-153`.
//!
//! [`on_key_down`] runs the handler's table in upstream's order: image
//! cropping, shape switching (Tab), the flowchart keys
//! (`App.flowchart.ts`), the command palette hint (Ctrl+P), the plain
//! paste flag (Ctrl+Shift+V), the browser zoom guard, the text field bail,
//! `?` and Ctrl+Shift+E, PgUp/PgDn, Alt, the action manager
//! ([`ActionManager::handle_key_down`]), the tool letters
//! ([`ToolState::handle_tool_key`]), Ctrl held (binding off), the arrow
//! key nudges, Enter, Space, S/G, Shift+F, Ctrl+Backspace/Delete and the
//! eyedropper keys.
//!
//! What the handler writes to the app state, the scene or the tool state
//! it writes here. What it hands to another part of `App` (a toast, a
//! cursor, text editing, the eyedropper, the flowchart creator, an
//! action's `perform`) comes back as a [`KeyEffect`] for the host, in the
//! order upstream runs it.
//!
//! See `site/content/research/ui-design-system.md` section 6 and the
//! shortcuts page (`site/content/design-system/shortcuts.md`).

use std::collections::HashSet;

use excali_core::app_state::AppState;
use excali_core::element::{BindMode, BoundElementType, Element, ElementKind, FixedPointBinding};
use serde_json::{json, Map, Value};

use crate::actions::{
    has_background, ActionContext, ActionEnv, ActionManager, ActionName, AppProps, KeyDownOutcome,
    KeyEvent,
};
use crate::binding::{
    bind_or_unbind_binding_elements, calculate_fixed_point_for_non_elbow_arrow_binding,
    update_bound_elements, BindingAppState, BindingEnv,
};
use crate::js_value::truthy;
use crate::scene::{ElementUpdate, Scene};
use crate::tools::{ArrowType, ToolKeyContext, ToolKeyEvent, ToolKeyOutcome, ToolState, ToolType};
use crate::viewport::{translate, TranslateOptions, Translation, ViewportState, ViewportUpdate};

/// `ELEMENT_TRANSLATE_AMOUNT` (`common/src/constants.ts:32`).
pub const ELEMENT_TRANSLATE_AMOUNT: f64 = 1.0;
/// `ELEMENT_SHIFT_TRANSLATE_AMOUNT` (`common/src/constants.ts:31`).
pub const ELEMENT_SHIFT_TRANSLATE_AMOUNT: f64 = 5.0;
/// How long `IS_PLAIN_PASTE` stays set after Ctrl+V, in milliseconds
/// (`App.tsx:5693-5697`).
pub const PLAIN_PASTE_RESET_MS: u32 = 100;

/// The modifier keys of a keyboard, mouse or pointer event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift_key: bool,
    pub alt_key: bool,
    pub ctrl_key: bool,
    pub meta_key: bool,
}

impl Modifiers {
    /// `event[KEYS.CTRL_OR_CMD]` (`keys.ts:39`): `metaKey` on a Mac,
    /// `ctrlKey` elsewhere.
    pub fn ctrl_or_cmd(self, is_darwin: bool) -> bool {
        if is_darwin {
            self.meta_key
        } else {
            self.ctrl_key
        }
    }
}

/// `shouldResizeFromCenter(event)` (`keys.ts:145-146`): Alt.
pub fn should_resize_from_center(m: Modifiers) -> bool {
    m.alt_key
}

/// `shouldMaintainAspectRatio(event)` (`keys.ts:148-149`): Shift.
pub fn should_maintain_aspect_ratio(m: Modifiers) -> bool {
    m.shift_key
}

/// `shouldRotateWithDiscreteAngle(event)` (`keys.ts:151-153`): Shift.
pub fn should_rotate_with_discrete_angle(m: Modifiers) -> bool {
    m.shift_key
}

/// `isArrowKey(key)` (`keys.ts:138-142`).
pub fn is_arrow_key(key: &str) -> bool {
    matches!(key, "ArrowLeft" | "ArrowRight" | "ArrowDown" | "ArrowUp")
}

/// What the key went to, as the handler asks about `event.target` and the
/// focused element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyTarget {
    /// `isWritableElement(event.target)` (`common/src/utils.ts:99-121`):
    /// the text editor, a textarea, a text-like input or a code editor.
    pub writable: bool,
    /// `isInputLike(event.target)` (`common/src/utils.ts:69-87`): the text
    /// editor, any input, textarea or select.
    pub input_like: bool,
    /// `document.activeElement` is the editor's container or the convert
    /// element type popup (`App.tsx:5640-5645`).
    pub editor_focused: bool,
}

/// A `keydown` or `keyup` event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Keystroke {
    /// `event.key`.
    pub key: String,
    /// `event.code`.
    pub code: String,
    pub modifiers: Modifiers,
    /// `event.repeat`.
    pub repeat: bool,
    pub target: KeyTarget,
}

impl Keystroke {
    /// A key with no modifier, sent to the canvas.
    pub fn new(key: &str, code: &str) -> Keystroke {
        Keystroke {
            key: key.to_owned(),
            code: code.to_owned(),
            ..Keystroke::default()
        }
    }

    pub fn shift(mut self) -> Keystroke {
        self.modifiers.shift_key = true;
        self
    }

    pub fn alt(mut self) -> Keystroke {
        self.modifiers.alt_key = true;
        self
    }

    pub fn ctrl(mut self) -> Keystroke {
        self.modifiers.ctrl_key = true;
        self
    }

    pub fn meta(mut self) -> Keystroke {
        self.modifiers.meta_key = true;
        self
    }

    pub fn with_target(mut self, target: KeyTarget) -> Keystroke {
        self.target = target;
        self
    }

    /// The event as the actions' `keyTest`s read it.
    pub fn key_event(&self) -> KeyEvent<'_> {
        KeyEvent {
            key: &self.key,
            code: &self.code,
            shift_key: self.modifiers.shift_key,
            alt_key: self.modifiers.alt_key,
            ctrl_key: self.modifiers.ctrl_key,
            meta_key: self.modifiers.meta_key,
            target_is_writable: self.target.writable,
        }
    }

    fn ctrl_or_cmd(&self, is_darwin: bool) -> bool {
        self.modifiers.ctrl_or_cmd(is_darwin)
    }
}

/// The CapsLock fix (`App.tsx:5591-5613`, #2372): a lone letter whose case
/// disagrees with Shift has its `key` re-cased to match Shift.
pub fn normalize_caps_lock(event: &Keystroke) -> Keystroke {
    let mut chars = event.key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return event.clone();
    };
    let shift = event.modifiers.shift_key;
    if (!shift && c.is_ascii_uppercase()) || (shift && c.is_ascii_lowercase()) {
        let mut next = event.clone();
        next.key = if shift {
            c.to_ascii_uppercase().to_string()
        } else {
            c.to_ascii_lowercase().to_string()
        };
        return next;
    }
    event.clone()
}

/// A flowchart link direction (`LinkDirection`, `flowchart.ts:55`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkDirection {
    Up,
    Right,
    Down,
    Left,
}

impl LinkDirection {
    /// `AppFlowchart.getLinkDirectionFromKey` (`App.flowchart.ts:184-197`).
    pub fn from_key(key: &str) -> LinkDirection {
        match key {
            "ArrowUp" => LinkDirection::Up,
            "ArrowDown" => LinkDirection::Down,
            "ArrowLeft" => LinkDirection::Left,
            _ => LinkDirection::Right,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LinkDirection::Up => "up",
            LinkDirection::Right => "right",
            LinkDirection::Down => "down",
            LinkDirection::Left => "left",
        }
    }
}

/// The flowchart session flags the keys read (`FlowChartCreator
/// .isCreatingChart`, `FlowChartNavigator.isExploring`). The host answers
/// [`KeyEffect::FlowchartCreate`], [`KeyEffect::FlowchartNavigate`] and the
/// keyup effects with [`crate::flowchart::AppFlowchart::answer`]
/// (`flowchart.ts`), which keeps these flags in step with the creator and
/// the navigator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlowchartKeys {
    pub is_creating_chart: bool,
    pub is_exploring: bool,
}

/// `getConversionTypeFromElements(elements)`
/// (`ConvertElementTypePopup.tsx:641-664`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionType {
    Generic,
    Linear,
}

/// Which way Tab cycles the element type (`"left"` with Shift).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertDirection {
    Left,
    Right,
}

/// `openEyeDropper({ type })`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EyeDropperKind {
    Stroke,
    Background,
}

/// What the handler does to the canvas cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorChange {
    /// `cursor.set(CURSOR_TYPE.GRAB)`.
    Grab,
    /// `cursor.reset()`.
    Reset,
    /// `cursor.applyForTool()`.
    ApplyForTool,
}

/// Work the handler hands to the rest of `App`, in upstream's order.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyEffect {
    /// Escape or Enter while cropping: `finishImageCropping()`.
    FinishImageCropping,
    /// Enter on a lone image: `startImageCropping(image)`.
    StartImageCropping {
        element_id: String,
    },
    /// Tab with the convert panel open: `convertElementTypes(app,
    /// { conversionType, direction })`, then `store.scheduleCapture()` when
    /// it converted something.
    ConvertElementType {
        conversion: Option<ConversionType>,
        direction: ConvertDirection,
    },
    /// Escape while creating a flowchart: the pending nodes are dropped.
    FlowchartCanceled,
    /// Ctrl+Arrow: `creator.createNodes(start, appState, direction,
    /// scene)` when one flowchart node is selected (`start`), then
    /// `revealIfHidden(pendingNodes)`.
    FlowchartCreate {
        start: Option<String>,
        direction: LinkDirection,
    },
    /// Alt+Arrow with one element selected:
    /// `navigator.exploreByDirection(element, elementsMap, direction)`,
    /// then select and reveal the node it returns.
    FlowchartNavigate {
        from: String,
        direction: LinkDirection,
    },
    /// Ctrl released while creating: insert the pending nodes, select and
    /// reveal the first, capture.
    FlowchartCommit,
    /// Alt released while exploring: capture.
    FlowchartNavigationEnded,
    /// Ctrl+P: the toast `commandPalette.shortcutHint`.
    CommandPaletteHint,
    /// Ctrl+V: `IS_PLAIN_PASTE` was set; reset it after
    /// [`PLAIN_PASTE_RESET_MS`].
    PlainPasteTimer,
    /// PgUp / PgDn: the viewport moved (`viewport.translate`).
    Scrolled(Translation),
    /// Alt with the bucket fill: `bucketFill.openTemporaryEyeDropper()`.
    OpenTemporaryEyeDropper,
    /// Alt released: `bucketFill.closeTemporaryEyeDropper()`.
    CloseTemporaryEyeDropper,
    /// `maybeHandleArrowPointlikeDrag({ app, event })`
    /// (`element/src/arrows/helpers.ts:7-50`): re-run a point or focus
    /// point drag of the selected linear element with the new modifiers.
    ArrowPointlikeDrag,
    /// The action manager took the key (`KeyDownOutcome::Perform` runs
    /// the action's `perform` with source `"keyboard"`).
    Action(KeyDownOutcome),
    /// A tool key ([`ToolState::handle_tool_key`]); its tool switch was
    /// applied to the tool state and the app state.
    Tool(ToolKeyOutcome),
    /// The text tool's affordance follows the binding toggle
    /// (`textTool.refresh(event)`).
    TextToolRefresh,
    /// `store.scheduleCapture()`.
    ScheduleCapture,
    /// `actionManager.executeAction(action)` (Ctrl+Enter:
    /// `toggleLinearEditor`).
    ExecuteAction(ActionName),
    /// The arrow keys moved elements: `scene.triggerUpdate()`.
    SceneUpdated,
    /// Enter on a text element or container: `startTextEditing({ sceneX,
    /// sceneY, container })`.
    StartTextEditing {
        scene_x: f64,
        scene_y: f64,
        container: Option<String>,
    },
    Cursor(CursorChange),
    /// I, Shift+S, Shift+G: `openEyeDropper({ type })`.
    OpenEyeDropper(EyeDropperKind),
}

/// The editor state the handler reads and writes that `AppState` does not
/// hold: editor atoms and `App` fields.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KeyboardState {
    /// `IS_PLAIN_PASTE`.
    pub is_plain_paste: bool,
    /// `convertElementTypePopupAtom` is `{ type: "panel" }`.
    pub convert_popup_open: bool,
    /// `activeConfirmDialogAtom` (`"clearCanvas"`).
    pub active_confirm_dialog: Option<String>,
    pub flowchart: FlowchartKeys,
    /// `gesture.pointers.size`: pointers down on the canvas.
    pub pointers_down: usize,
}

/// What a key event did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KeyOutcome {
    /// `event.preventDefault()`.
    pub prevent_default: bool,
    /// `event.stopPropagation()`.
    pub stop_propagation: bool,
    pub effects: Vec<KeyEffect>,
}

impl KeyOutcome {
    fn push(&mut self, effect: KeyEffect) {
        self.effects.push(effect);
    }
}

/// The editor the handler works on.
pub struct KeyboardEditor<'a> {
    pub scene: &'a mut Scene,
    pub app_state: &'a mut AppState,
    pub tools: &'a mut ToolState,
    pub keyboard: &'a mut KeyboardState,
    pub actions: &'a ActionManager,
    pub props: &'a AppProps,
    pub env: &'a ActionEnv,
}

// ---------------------------------------------------------------------------
// element predicates

fn is_text(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Text(_))
}

fn is_image(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Image(_))
}

fn is_arrow(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Arrow(_))
}

fn is_elbow_arrow(e: &Element) -> bool {
    matches!(&e.kind, ElementKind::Arrow(a) if a.elbowed)
}

/// `isLinearElement`: a line or an arrow.
fn is_linear(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Arrow(_) | ElementKind::Line(_))
}

fn is_line(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Line(_))
}

/// `isFlowchartNodeElement` (`typeChecks.ts:286-295`).
fn is_flowchart_node(e: &Element) -> bool {
    matches!(
        e.kind,
        ElementKind::Rectangle
            | ElementKind::StickyNote(_)
            | ElementKind::Ellipse
            | ElementKind::Diamond
    )
}

/// `isValidTextContainer` / `isTextBindableContainer`
/// (`textElement.ts:508-519`, `typeChecks.ts:240-253`).
fn is_text_container(e: &Element) -> bool {
    matches!(
        e.kind,
        ElementKind::Rectangle
            | ElementKind::StickyNote(_)
            | ElementKind::Ellipse
            | ElementKind::Diamond
            | ElementKind::Arrow(_)
    )
}

fn is_frame_like(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Frame(_) | ElementKind::MagicFrame(_))
}

fn type_name(e: &Element) -> &'static str {
    e.element_type().as_str()
}

fn container_id(e: &Element) -> Option<&str> {
    match &e.kind {
        ElementKind::Text(t) => t.container_id.as_deref(),
        _ => None,
    }
}

fn bindings(e: &Element) -> (Option<&FixedPointBinding>, Option<&FixedPointBinding>) {
    match e.kind.linear() {
        Some(l) => (l.start_binding.as_ref(), l.end_binding.as_ref()),
        None => (None, None),
    }
}

/// `getBoundTextElement(element, elementsMap)` is not null: a bound text
/// that exists in the scene and is not deleted.
fn has_live_bound_text(scene: &Scene, e: &Element) -> bool {
    e.base
        .bound_elements
        .iter()
        .flatten()
        .any(|b| b.kind == BoundElementType::Text && scene.get_non_deleted(&b.id).is_some())
}

pub use crate::convert_element_type::get_conversion_type;

// ---------------------------------------------------------------------------
// app state

fn state_flag(app_state: &AppState, key: &str) -> bool {
    truthy(app_state.get(key))
}

fn state_str<'s>(app_state: &'s AppState, key: &str) -> Option<&'s str> {
    app_state.get(key).and_then(Value::as_str)
}

fn is_selected(app_state: &AppState, id: &str) -> bool {
    truthy(
        app_state
            .get("selectedElementIds")
            .and_then(|ids| ids.get(id)),
    )
}

/// `getSelectedElements(elements, appState, { includeBoundTextElement,
/// includeElementsInFrames })` (`selection.ts:161-215`) over the
/// non-deleted elements, as `scene.getSelectedElements` calls it.
pub fn get_selected_elements<'s>(
    scene: &'s Scene,
    app_state: &AppState,
    include_bound_text: bool,
    include_elements_in_frames: bool,
) -> Vec<&'s Element> {
    let live: Vec<&Element> = scene.non_deleted();
    let mut selected: Vec<&Element> = Vec::new();
    let mut added: HashSet<&str> = HashSet::new();
    for &e in &live {
        if is_selected(app_state, &e.base.id) {
            selected.push(e);
            added.insert(&e.base.id);
            continue;
        }
        if include_bound_text && container_id(e).is_some_and(|c| is_selected(app_state, c)) {
            selected.push(e);
            added.insert(&e.base.id);
        }
    }
    if !include_elements_in_frames {
        return selected;
    }
    let mut with_children = Vec::new();
    for e in selected {
        if is_frame_like(e) {
            for &child in &live {
                if child.base.frame_id.as_deref() == Some(e.base.id.as_str())
                    && !added.contains(child.base.id.as_str())
                {
                    with_children.push(child);
                }
            }
        }
        with_children.push(e);
    }
    with_children
}

/// `getEffectiveGridSize()` (`App.tsx:1518-1526`, `isGridModeEnabled`,
/// `snapping.ts:158-160`): the grid size while grid mode is on.
pub fn effective_grid_size(app_state: &AppState, props: &AppProps) -> Option<f64> {
    let enabled = props
        .grid_mode_enabled
        .unwrap_or_else(|| app_state.grid_mode_enabled().unwrap_or(false));
    if enabled {
        app_state.grid_size()
    } else {
        None
    }
}

/// The nudge step (`App.tsx:5903-5910`): the grid size (1 with Shift)
/// while the grid is on, else 1 (5 with Shift).
pub fn nudge_step(grid_size: Option<f64>, shift_key: bool) -> f64 {
    let grid = grid_size.filter(|&g| g != 0.0 && !g.is_nan());
    match grid {
        Some(g) => {
            if shift_key {
                ELEMENT_TRANSLATE_AMOUNT
            } else {
                g
            }
        }
        None => {
            if shift_key {
                ELEMENT_SHIFT_TRANSLATE_AMOUNT
            } else {
                ELEMENT_TRANSLATE_AMOUNT
            }
        }
    }
}

fn arrow_type(app_state: &AppState) -> ArrowType {
    match state_str(app_state, "currentItemArrowType") {
        Some("sharp") => ArrowType::Sharp,
        Some("elbow") => ArrowType::Elbow,
        _ => ArrowType::Round,
    }
}

fn arrow_type_name(t: ArrowType) -> &'static str {
    match t {
        ArrowType::Sharp => "sharp",
        ArrowType::Round => "round",
        ArrowType::Elbow => "elbow",
    }
}

pub(crate) fn binding_app_state(app_state: &AppState) -> BindingAppState {
    let defaults = BindingAppState::default();
    let flag = |key: &str, default: bool| {
        app_state
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    };
    BindingAppState {
        zoom: app_state.zoom().unwrap_or(1.0),
        is_binding_enabled: flag("isBindingEnabled", defaults.is_binding_enabled),
        is_midpoint_snapping_enabled: flag(
            "isMidpointSnappingEnabled",
            defaults.is_midpoint_snapping_enabled,
        ),
        grid_mode_enabled: flag("gridModeEnabled", defaults.grid_mode_enabled),
        grid_size: app_state.grid_size(),
        bind_mode: match state_str(app_state, "bindMode") {
            Some("inside") => BindMode::Inside,
            Some("skip") => BindMode::Skip,
            _ => BindMode::Orbit,
        },
        selected_linear_element: None,
        complex_bindings: false,
    }
}

/// `isBindingEnabled(appState)`.
fn binding_enabled(app_state: &AppState) -> bool {
    app_state
        .get("isBindingEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// `makeNextSelectedElementIds({}, appState)` and the other keys the
/// Space release and a tool switch clear.
fn clear_selection(app_state: &mut AppState) {
    app_state.insert("selectedElementIds", Value::Object(Map::new()));
    app_state.insert("selectedGroupIds", Value::Object(Map::new()));
    app_state.insert("editingGroupId", Value::Null);
}

fn sync_active_tool(ed: &mut KeyboardEditor<'_>) {
    ed.app_state
        .insert("activeTool", ed.tools.active_tool.to_json());
}

fn active_tool_type(ed: &KeyboardEditor<'_>) -> String {
    ed.tools.active_tool.tool.type_name().to_owned()
}

// ---------------------------------------------------------------------------
// the flowchart keys (App.flowchart.ts)

/// `AppFlowchart.handleKeyEvent(event)` for a keydown
/// (`App.flowchart.ts:53-150`); returns whether it took the key.
fn flowchart_key_down(
    ed: &mut KeyboardEditor<'_>,
    event: &Keystroke,
    out: &mut KeyOutcome,
) -> bool {
    let is_darwin = ed.env.is_darwin;
    if event.key == "Escape" && ed.keyboard.flowchart.is_creating_chart {
        ed.keyboard.flowchart.is_creating_chart = false;
        out.push(KeyEffect::FlowchartCanceled);
        return true;
    }
    if !is_arrow_key(&event.key) {
        return false;
    }
    let direction = LinkDirection::from_key(&event.key);
    if event.ctrl_or_cmd(is_darwin) && !event.modifiers.shift_key {
        let selected = get_selected_elements(ed.scene, ed.app_state, false, false);
        let start = match selected.as_slice() {
            [one] if is_flowchart_node(one) => Some(one.base.id.clone()),
            _ => None,
        };
        if start.is_some() {
            ed.keyboard.flowchart.is_creating_chart = true;
        }
        out.prevent_default = true;
        out.push(KeyEffect::FlowchartCreate { start, direction });
        return true;
    }
    if event.modifiers.alt_key {
        let selected = get_selected_elements(ed.scene, ed.app_state, false, false);
        if let [one] = selected.as_slice() {
            out.prevent_default = true;
            out.push(KeyEffect::FlowchartNavigate {
                from: one.base.id.clone(),
                direction,
            });
            return true;
        }
    }
    false
}

/// `AppFlowchart.handleKeyEvent(event)` for a keyup
/// (`App.flowchart.ts:150-167`).
fn flowchart_key_up(ed: &mut KeyboardEditor<'_>, event: &Keystroke, out: &mut KeyOutcome) {
    let flowchart = &mut ed.keyboard.flowchart;
    let navigation_ended = !event.modifiers.alt_key && flowchart.is_exploring;
    if navigation_ended {
        flowchart.is_exploring = false;
    }
    if !event.ctrl_or_cmd(ed.env.is_darwin) && flowchart.is_creating_chart {
        flowchart.is_creating_chart = false;
        out.push(KeyEffect::FlowchartCommit);
        return;
    }
    if navigation_ended {
        out.push(KeyEffect::FlowchartNavigationEnded);
    }
}

// ---------------------------------------------------------------------------
// onKeyDown

/// `App.onKeyDown(event)` (`App.tsx:5585-6073`).
pub fn on_key_down(
    ed: &mut KeyboardEditor<'_>,
    binding_env: &mut dyn BindingEnv,
    event: &Keystroke,
) -> KeyOutcome {
    let mut out = KeyOutcome::default();
    if !ed.tools.is_interaction_enabled() {
        return out;
    }
    let event = &normalize_caps_lock(event);
    let is_darwin = ed.env.is_darwin;
    let ctrl_or_cmd = event.ctrl_or_cmd(is_darwin);
    let shift = event.modifiers.shift_key;
    let alt = event.modifiers.alt_key;
    let key = event.key.as_str();

    if !event.target.input_like {
        if (key == "Escape" || key == "Enter")
            && ed
                .app_state
                .get("croppingElementId")
                .is_some_and(|v| truthy(Some(v)))
        {
            out.push(KeyEffect::FinishImageCropping);
            return out;
        }
        let selected = get_selected_elements(ed.scene, ed.app_state, false, false);
        if let [one] = selected.as_slice() {
            if is_image(one) && key == "Enter" {
                out.push(KeyEffect::StartImageCropping {
                    element_id: one.base.id.clone(),
                });
                return out;
            }
        }
        // shape switching
        if key == "Escape" {
            ed.keyboard.convert_popup_open = false;
        } else if key == "Tab" && event.target.editor_focused {
            out.prevent_default = true;
            let conversion = get_conversion_type(&selected);
            if ed.keyboard.convert_popup_open {
                out.push(KeyEffect::ConvertElementType {
                    conversion,
                    direction: if shift {
                        ConvertDirection::Left
                    } else {
                        ConvertDirection::Right
                    },
                });
            }
            if conversion.is_some() {
                ed.keyboard.convert_popup_open = true;
            }
        }
        if flowchart_key_down(ed, event, &mut out) {
            return out;
        }
    }

    if ctrl_or_cmd && key == "p" && !shift && !alt {
        out.push(KeyEffect::CommandPaletteHint);
        out.prevent_default = true;
        return out;
    }

    if ctrl_or_cmd && key.to_lowercase() == "v" {
        ed.keyboard.is_plain_paste = shift;
        out.push(KeyEffect::PlainPasteTimer);
    }

    // prevent browser zoom in input fields
    if ctrl_or_cmd && event.target.writable && (event.code == "Minus" || event.code == "Equal") {
        out.prevent_default = true;
        return out;
    }

    if (event.target.writable && key != "Escape") || (is_arrow_key(key) && event.target.input_like)
    {
        return out;
    }

    if key == "?" {
        ed.app_state.insert("openDialog", json!({ "name": "help" }));
        return out;
    } else if key.to_lowercase() == "e" && shift && ctrl_or_cmd {
        out.prevent_default = true;
        ed.app_state
            .insert("openDialog", json!({ "name": "imageExport" }));
        return out;
    }

    if let Some(translation) = page_scroll(ed.app_state, event) {
        out.push(KeyEffect::Scrolled(translation));
        out.prevent_default = true;
        return out;
    }

    if ed
        .app_state
        .get("openDialog")
        .and_then(|d| d.get("name"))
        .and_then(Value::as_str)
        == Some("elementLinkSelector")
    {
        return out;
    }

    if key == "Alt" {
        if ed.tools.active_tool.tool.builtin() == Some(ToolType::Bucketfill) {
            out.push(KeyEffect::OpenTemporaryEyeDropper);
            out.prevent_default = true;
            return out;
        }
        // getFeatureFlag("COMPLEX_BINDINGS") is off by default
        // (common/src/utils.ts:1190)
        out.push(KeyEffect::ArrowPointlikeDrag);
    }

    {
        let ctx = ActionContext {
            elements: ed.scene.elements(),
            app_state: ed.app_state,
            props: ed.props,
            env: ed.env,
        };
        match ed.actions.handle_key_down(&event.key_event(), &ctx) {
            KeyDownOutcome::Unhandled => {}
            ambiguous @ KeyDownOutcome::Ambiguous(_) => out.push(KeyEffect::Action(ambiguous)),
            taken => {
                out.prevent_default = true;
                out.stop_propagation = true;
                out.push(KeyEffect::Action(taken));
                return out;
            }
        }
    }

    // the tool keys (App.tsx:5768-5846)
    let tool_ctx = ToolKeyContext {
        prevent_tool_switching: ed.props.view_mode_enabled == Some(true),
        view_mode_enabled: state_flag(ed.app_state, "viewModeEnabled"),
        gesture_in_progress: state_flag(ed.app_state, "newElement")
            || state_flag(ed.app_state, "selectionElement")
            || state_flag(ed.app_state, "selectedElementsAreBeingDragged"),
        arrow_type: arrow_type(ed.app_state),
    };
    let tool_event = ToolKeyEvent {
        key,
        shift,
        ctrl: event.modifiers.ctrl_key,
        alt,
        meta: event.modifiers.meta_key,
    };
    let tool_outcome = ed.tools.handle_tool_key(tool_event, &tool_ctx);
    match tool_outcome {
        ToolKeyOutcome::NotHandled => {}
        ToolKeyOutcome::Ignored => return out,
        other => {
            apply_tool_outcome(ed, &other);
            if matches!(
                other,
                ToolKeyOutcome::Tool { .. } | ToolKeyOutcome::ToggledLock { .. }
            ) {
                out.stop_propagation = true;
            }
            out.push(KeyEffect::Tool(other));
            return out;
        }
    }

    if state_flag(ed.app_state, "viewModeEnabled") {
        return out;
    }

    if ctrl_or_cmd && !event.repeat {
        let preference_enabled = state_str(ed.app_state, "bindingPreference") == Some("enabled");
        ed.app_state
            .insert("isBindingEnabled", Value::Bool(!preference_enabled));
        out.push(KeyEffect::TextToolRefresh);
        out.push(KeyEffect::ArrowPointlikeDrag);
    }

    if is_arrow_key(key) {
        nudge(ed, binding_env, event);
        out.push(KeyEffect::SceneUpdated);
        out.prevent_default = true;
    } else if key == "Enter" {
        let selected: Vec<Element> = get_selected_elements(ed.scene, ed.app_state, false, false)
            .into_iter()
            .cloned()
            .collect();
        if let [element] = selected.as_slice() {
            if ctrl_or_cmd || is_line(element) {
                if is_linear(element) {
                    let editing = ed.app_state.get("selectedLinearElement");
                    let editing_this = editing
                        .and_then(|l| l.get("isEditing"))
                        .is_some_and(|v| truthy(Some(v)))
                        && editing
                            .and_then(|l| l.get("elementId"))
                            .and_then(Value::as_str)
                            == Some(element.base.id.as_str());
                    if !editing_this {
                        out.push(KeyEffect::ScheduleCapture);
                        if !is_elbow_arrow(element) {
                            out.push(KeyEffect::ExecuteAction(ActionName::ToggleLinearEditor));
                        }
                    }
                }
            } else if is_text(element) || is_text_container(element) {
                let container = (!is_text(element)).then(|| element.base.id.clone());
                let [scene_x, scene_y] = container_center(ed.scene, element);
                out.push(KeyEffect::StartTextEditing {
                    scene_x,
                    scene_y,
                    container,
                });
                out.prevent_default = true;
                return out;
            } else if is_frame_like(element) {
                ed.app_state
                    .insert("editingFrame", Value::String(element.base.id.clone()));
            }
        }
    }

    if key == " " && ed.keyboard.pointers_down == 0 {
        ed.tools.space_held = true;
        out.push(KeyEffect::Cursor(CursorChange::Grab));
        out.prevent_default = true;
    }

    if (key == "g" || key == "s") && !alt && !ctrl_or_cmd {
        let tool = active_tool_type(ed);
        let selected = get_selected_elements(ed.scene, ed.app_state, false, false);
        if tool == "selection" && selected.is_empty() {
            return out;
        }
        if key == "g"
            && (has_background(&tool) || selected.iter().any(|e| has_background(type_name(e))))
        {
            ed.app_state
                .insert("openPopup", Value::String("elementBackground".into()));
            out.stop_propagation = true;
        }
        if key == "s" {
            ed.app_state
                .insert("openPopup", Value::String("elementStroke".into()));
            out.stop_propagation = true;
        }
    }

    if !ctrl_or_cmd && shift && key.to_lowercase() == "f" {
        let tool = active_tool_type(ed);
        let selected = get_selected_elements(ed.scene, ed.app_state, false, false);
        if tool == "selection" && selected.is_empty() {
            return out;
        }
        if tool == "text"
            || selected
                .iter()
                .any(|e| is_text(e) || has_live_bound_text(ed.scene, e))
        {
            out.prevent_default = true;
            ed.app_state
                .insert("openPopup", Value::String("fontFamily".into()));
        }
    }

    if ctrl_or_cmd && (key == "Backspace" || key == "Delete") {
        ed.keyboard.active_confirm_dialog = Some("clearCanvas".into());
    }

    // eye dropper
    let lower = key.to_lowercase();
    let picking_stroke = lower == "s" && shift && !ctrl_or_cmd;
    let picking_background = key == "i" || (lower == "g" && shift);
    if picking_stroke || picking_background {
        out.push(KeyEffect::OpenEyeDropper(if picking_stroke {
            EyeDropperKind::Stroke
        } else {
            EyeDropperKind::Background
        }));
    }
    out
}

/// Applies a tool key's outcome to the app state: the arrow type, the
/// active tool and what the tool switch clears.
fn apply_tool_outcome(ed: &mut KeyboardEditor<'_>, outcome: &ToolKeyOutcome) {
    let switch = match outcome {
        ToolKeyOutcome::Tool {
            next_arrow_type,
            action,
            ..
        } => {
            if let Some(next) = next_arrow_type {
                ed.app_state.insert(
                    "currentItemArrowType",
                    Value::String(arrow_type_name(*next).into()),
                );
            }
            match action {
                crate::tools::ToolKeyAction::SetTool(Ok(switch)) => Some(*switch),
                _ => None,
            }
        }
        ToolKeyOutcome::ViewModeEscape(Ok(switch)) => Some(*switch),
        _ => None,
    };
    if let Some(switch) = switch {
        if switch.clear_selection {
            clear_selection(ed.app_state);
            ed.app_state.insert("multiElement", Value::Null);
        }
        if switch.clear_suggested_binding {
            ed.app_state.insert("suggestedBinding", Value::Null);
        }
        if !switch.keep_selected_linear_element {
            ed.app_state.insert("selectedLinearElement", Value::Null);
        }
        ed.app_state.insert("activeEmbeddable", Value::Null);
    }
    sync_active_tool(ed);
}

/// `maybeHandlePageScrollKeyDown(event)` (`App.tsx:3296-3318`): PgUp /
/// PgDn scroll a page, horizontally with Shift, through
/// `viewport.translate`.
fn page_scroll(app_state: &mut AppState, event: &Keystroke) -> Option<Translation> {
    let key = event.key.as_str();
    if key != "PageUp" && key != "PageDown" {
        return None;
    }
    let state = ViewportState::from_app_state(app_state);
    let shift = event.modifiers.shift_key;
    let mut offset = (if shift { state.width } else { state.height }) / state.zoom;
    if key == "PageDown" {
        offset = -offset;
    }
    let update = if shift {
        ViewportUpdate {
            scroll_x: Some(state.scroll_x + offset),
            ..ViewportUpdate::default()
        }
    } else {
        ViewportUpdate {
            scroll_y: Some(state.scroll_y + offset),
            ..ViewportUpdate::default()
        }
    };
    let translation = translate(&state, Some(update), TranslateOptions::default());
    translation.viewport.write_to(app_state);
    Some(translation)
}

/// The arrow key nudge (`App.tsx:5871-5942`): the selection, bound texts
/// and frame children included, less the arrows bound to an element left
/// out of it, moves by the step; each moved element's bound arrows follow.
fn nudge(ed: &mut KeyboardEditor<'_>, binding_env: &mut dyn BindingEnv, event: &Keystroke) {
    let selected: Vec<Element> = get_selected_elements(ed.scene, ed.app_state, true, true)
        .into_iter()
        .cloned()
        .collect();
    let ids: HashSet<&str> = selected.iter().map(|e| e.base.id.as_str()).collect();
    let remove: HashSet<String> = selected
        .iter()
        .filter(|e| is_arrow(e))
        .filter(|arrow| {
            let (start, end) = bindings(arrow);
            let outside = |b: Option<&FixedPointBinding>| {
                b.is_some_and(|b| !ids.contains(b.element_id.as_str()))
            };
            outside(start) || outside(end)
        })
        .map(|arrow| arrow.base.id.clone())
        .collect();
    let moved: Vec<Element> = selected
        .into_iter()
        .filter(|e| !remove.contains(&e.base.id))
        .collect();

    let step = nudge_step(
        effective_grid_size(ed.app_state, ed.props),
        event.modifiers.shift_key,
    );
    let (dx, dy) = match event.key.as_str() {
        "ArrowLeft" => (-step, 0.0),
        "ArrowRight" => (step, 0.0),
        "ArrowUp" => (0.0, -step),
        "ArrowDown" => (0.0, step),
        _ => (0.0, 0.0),
    };
    let moved_ids: Vec<String> = moved.iter().map(|e| e.base.id.clone()).collect();
    for element in &moved {
        // the element as the selection captured it, as upstream reads
        // `element.x` from the array it collected before the loop
        ed.scene.mutate_element(
            &element.base.id,
            ElementUpdate::position(element.base.x + dx, element.base.y + dy),
            binding_env,
        );
        update_bound_elements(
            ed.scene,
            binding_env,
            &element.base.id,
            Some(&moved_ids),
            None,
        );
    }
}

/// `getContainerCenter(container, elementsMap)` (`textElement.ts:377-394`);
/// a text element is its own box.
fn container_center(scene: &Scene, e: &Element) -> [f64; 2] {
    if !is_arrow(e) {
        return [
            e.base.x + e.base.width / 2.0,
            e.base.y + e.base.height / 2.0,
        ];
    }
    excali_scene::linear_element::get_bound_text_element_center(e, &scene.elements_map())
}

// ---------------------------------------------------------------------------
// onKeyUp

/// `App.onKeyUp(event)` (`App.tsx:6076-6207`).
pub fn on_key_up(
    ed: &mut KeyboardEditor<'_>,
    binding_env: &mut dyn BindingEnv,
    event: &Keystroke,
) -> KeyOutcome {
    let mut out = KeyOutcome::default();
    if !ed.tools.is_interaction_enabled() {
        return out;
    }
    let ctrl_or_cmd = event.ctrl_or_cmd(ed.env.is_darwin);
    if event.key == " " {
        let tool = ed.tools.active_tool.tool.clone();
        let dialog = ed
            .app_state
            .get("openDialog")
            .and_then(|d| d.get("name"))
            .and_then(Value::as_str);
        if (state_flag(ed.app_state, "viewModeEnabled") && tool.builtin() != Some(ToolType::Laser))
            || dialog == Some("elementLinkSelector")
        {
            out.push(KeyEffect::Cursor(CursorChange::Grab));
        } else if crate::tools::is_selection_like_tool(&tool) {
            out.push(KeyEffect::Cursor(CursorChange::Reset));
        } else {
            out.push(KeyEffect::Cursor(CursorChange::ApplyForTool));
            clear_selection(ed.app_state);
            ed.app_state.insert("activeEmbeddable", Value::Null);
        }
        ed.tools.space_held = false;
    }

    if event.key == "Alt" {
        out.push(KeyEffect::CloseTemporaryEyeDropper);
        out.push(KeyEffect::ArrowPointlikeDrag);
    }

    if (event.key == "Alt" && state_str(ed.app_state, "bindMode") == Some("skip"))
        || (!ctrl_or_cmd && !binding_enabled(ed.app_state))
    {
        // the delayed bind mode timer only runs under COMPLEX_BINDINGS
        ed.app_state
            .insert("bindMode", Value::String("orbit".into()));
    }
    if !ctrl_or_cmd {
        let preference_enabled = state_str(ed.app_state, "bindingPreference") == Some("enabled");
        if binding_enabled(ed.app_state) != preference_enabled {
            ed.app_state
                .insert("isBindingEnabled", Value::Bool(preference_enabled));
            out.push(KeyEffect::TextToolRefresh);
        }
        out.push(KeyEffect::ArrowPointlikeDrag);
    }
    if is_arrow_key(&event.key) {
        let arrows: Vec<String> = get_selected_elements(ed.scene, ed.app_state, false, false)
            .into_iter()
            .filter(|e| is_arrow(e))
            .map(|e| e.base.id.clone())
            .collect();
        let binding_state = binding_app_state(ed.app_state);
        bind_or_unbind_binding_elements(ed.scene, binding_env, &arrows, &binding_state);

        let simple: Vec<String> = get_selected_elements(ed.scene, ed.app_state, false, false)
            .into_iter()
            .filter(|e| is_arrow(e) && !is_elbow_arrow(e))
            .map(|e| e.base.id.clone())
            .collect();
        for id in simple {
            refix_binding(
                ed.scene,
                binding_env,
                &id,
                crate::binding::BindingEnd::Start,
            );
            refix_binding(ed.scene, binding_env, &id, crate::binding::BindingEnd::End);
        }
        ed.app_state.insert("suggestedBinding", Value::Null);
    }

    flowchart_key_up(ed, event, &mut out);
    out
}

/// The fixed point of a simple arrow's bound end, recomputed where the end
/// now is (`App.tsx:6166-6200`).
fn refix_binding(
    scene: &mut Scene,
    env: &mut dyn BindingEnv,
    arrow_id: &str,
    end: crate::binding::BindingEnd,
) {
    let Some(arrow) = scene.get(arrow_id) else {
        return;
    };
    let (start, finish) = bindings(arrow);
    let Some(binding) = (match end {
        crate::binding::BindingEnd::Start => start,
        crate::binding::BindingEnd::End => finish,
    })
    .cloned() else {
        return;
    };
    let Some(target) = scene.get_non_deleted(&binding.element_id) else {
        return;
    };
    let fixed_point = calculate_fixed_point_for_non_elbow_arrow_binding(
        arrow,
        target,
        end,
        &scene.elements_map(),
        None,
    );
    let next = Some(Some(FixedPointBinding {
        fixed_point,
        ..binding
    }));
    let update = match end {
        crate::binding::BindingEnd::Start => ElementUpdate {
            start_binding: next,
            ..ElementUpdate::default()
        },
        crate::binding::BindingEnd::End => ElementUpdate {
            end_binding: next,
            ..ElementUpdate::default()
        },
    };
    scene.mutate_element(arrow_id, update, env);
}

// ---------------------------------------------------------------------------
// the command palette toggle (CommandPalette.tsx)

/// `isCommandPaletteToggleShortcut(event)`
/// (`components/CommandPalette/CommandPalette.tsx:141-148`): Ctrl+/ or
/// Ctrl+Shift+P, without Alt.
pub fn is_command_palette_toggle_shortcut(event: &Keystroke, is_darwin: bool) -> bool {
    !event.modifiers.alt_key
        && event.ctrl_or_cmd(is_darwin)
        && ((event.modifiers.shift_key && event.key.to_lowercase() == "p") || event.key == "/")
}

/// The command palette's window keydown listener (capture phase,
/// `CommandPalette.tsx:158-186`): the shortcut opens the palette, or
/// closes it when open. It runs before [`on_key_down`].
pub fn command_palette_key_down(
    app_state: &mut AppState,
    event: &Keystroke,
    is_darwin: bool,
) -> KeyOutcome {
    let mut out = KeyOutcome::default();
    if !is_command_palette_toggle_shortcut(event, is_darwin) {
        return out;
    }
    out.prevent_default = true;
    out.stop_propagation = true;
    let open = app_state
        .get("openDialog")
        .and_then(|d| d.get("name"))
        .and_then(Value::as_str)
        == Some("commandPalette");
    app_state.insert(
        "openDialog",
        if open {
            Value::Null
        } else {
            json!({ "name": "commandPalette" })
        },
    );
    out
}

// ---------------------------------------------------------------------------
// clipboard events

/// A document `copy`, `cut` or `paste` event (the browser fires them for
/// Ctrl+C, Ctrl+X and Ctrl+V).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardEventKind {
    Copy,
    Cut,
    Paste,
}

/// Where a clipboard event happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClipboardTarget {
    /// The editor's container holds `document.activeElement`.
    pub editor_active: bool,
    /// `isWritableElement(event.target)` (for paste: of the active
    /// element).
    pub writable: bool,
    /// The element under the last pointer position is a canvas (paste
    /// only).
    pub canvas_under_pointer: bool,
}

/// What `onCopy`, `onCut` or `pasteFromClipboard` does with the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOutcome {
    /// Left to the browser.
    Ignored,
    /// `executeAction(actionCopy | actionCut, "keyboard", event)`; the
    /// event is prevented and stopped.
    Action(ActionName),
    /// Parse the clipboard and insert it; `plain` is `IS_PLAIN_PASTE`
    /// (Ctrl+Shift+V).
    Paste { plain: bool },
}

/// `App.onCopy`, `App.onCut` (`App.tsx:4542-4570`) and the gate of
/// `App.pasteFromClipboard` (`App.tsx:4828-4854`).
pub fn on_clipboard_event(
    tools: &ToolState,
    keyboard: &KeyboardState,
    kind: ClipboardEventKind,
    target: ClipboardTarget,
) -> ClipboardOutcome {
    if !tools.is_interaction_enabled() || !target.editor_active {
        return ClipboardOutcome::Ignored;
    }
    match kind {
        ClipboardEventKind::Copy | ClipboardEventKind::Cut => {
            if target.writable {
                return ClipboardOutcome::Ignored;
            }
            ClipboardOutcome::Action(if kind == ClipboardEventKind::Copy {
                ActionName::Copy
            } else {
                ActionName::Cut
            })
        }
        ClipboardEventKind::Paste => {
            if !target.canvas_under_pointer || target.writable {
                return ClipboardOutcome::Ignored;
            }
            ClipboardOutcome::Paste {
                plain: keyboard.is_plain_paste,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// panning (App.pan.ts)

/// `POINTER_BUTTON` (`common/src/constants.ts`).
pub mod pointer_button {
    pub const MAIN: i16 = 0;
    pub const WHEEL: i16 = 1;
    pub const SECONDARY: i16 = 2;
}

/// What `AppPan.start` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PanStart {
    /// `event.button`.
    pub button: i16,
    /// Pointers down, this one included.
    pub pointer_count: usize,
    pub interaction_enabled: bool,
    pub navigation_enabled: bool,
    pub view_mode_enabled: bool,
    /// `isActiveToolPointerCapturing()` (the laser in view mode).
    pub active_tool_pointer_capturing: bool,
}

/// Whether a pointerdown starts a canvas pan (`AppPan.start`,
/// `App.pan.ts:100-120`): the wheel button, the secondary button, the main
/// button with Space held, or the hand tool; or any press in view mode.
pub fn pan_starts(tools: &ToolState, p: PanStart) -> bool {
    let hand = tools.active_tool.tool.builtin() == Some(ToolType::Hand);
    p.pointer_count <= 1
        && (((p.button == pointer_button::WHEEL
            || p.button == pointer_button::SECONDARY
            || (p.button == pointer_button::MAIN && tools.space_held)
            || hand)
            && (p.interaction_enabled || p.navigation_enabled))
            || (p.view_mode_enabled && !p.active_tool_pointer_capturing))
}
