//! Tools: the registry (`TOOLS`), keyboard lookup, the active tool, the
//! tool lock and pen mode.
//!
//! Upstream, at the pinned commit:
//!
//! - `packages/excalidraw/components/Tools.tsx:69-231`: [`TOOLS`] (the
//!   single source of truth for icon, letter key, number key, `fillable`
//!   and `toggle`), [`TOGGLE_TOOLS`], [`get_tool_letter`],
//!   [`get_tool_shortcut`], [`find_shape_by_key`] and
//!   `isToolButtonDisabled` ([`ToolState::is_tool_button_disabled`]);
//! - `packages/common/src/utils.ts:272-307`: [`is_selection_like_tool`],
//!   [`update_active_tool`];
//! - `packages/excalidraw/types.ts:148-178, 424-441`: [`ToolType`],
//!   [`ActiveTool`], `preferredSelectionTool`, `penMode`, `penDetected`;
//! - `packages/excalidraw/components/App.tsx`: `isInteractionEnabled`
//!   (:963), `isToolSupported` (:1045), `isToolLocked` (:1079),
//!   the non-interactive tool reset of `handleInteractionStateChange`
//!   (:3486-3499), `isSameForcedTool` / `handleForcedToolChange`
//!   (:3519-3571), their order in `componentDidUpdate` (:4326-4327),
//!   `toggleLock` (:5220), `togglePenMode` (:5271), the tool keys of
//!   `onKeyDown` (:5768-5846), `setActiveTool` (:6210-6335), pen detection
//!   (:8830-8837), the pen-mode pointer gate (:8963-8969) and the pen pinch
//!   lock (:9341-9343);
//! - `packages/excalidraw/actions/actionDeselect.ts:18-33` and
//!   `actionFinalize.tsx:343-355`: the tool Esc / finalize return to.
//!
//! See `site/content/research/ui-design-system.md` section 3.1.
//!
//! Upstream's `setActiveTool` also writes cursors, focus and unrelated
//! app-state keys. Here [`ToolState::set_active_tool`] updates the tool
//! state and returns a [`ToolSwitch`] naming the rest for the caller to
//! apply; nothing in this module touches the DOM.

use excali_core::element::StrokeVariability;
use serde_json::{json, Map, Value};

/// `ToolType` (`types.ts:148-167`), in upstream's declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolType {
    Selection,
    Lasso,
    Rectangle,
    Diamond,
    Ellipse,
    Arrow,
    Line,
    Freedraw,
    Text,
    Image,
    Eraser,
    Hand,
    Frame,
    Magicframe,
    Stickynote,
    Embeddable,
    Laser,
    Autoshape,
    Bucketfill,
}

impl ToolType {
    /// Every tool type, in upstream's declaration order.
    pub const ALL: [ToolType; 19] = [
        ToolType::Selection,
        ToolType::Lasso,
        ToolType::Rectangle,
        ToolType::Diamond,
        ToolType::Ellipse,
        ToolType::Arrow,
        ToolType::Line,
        ToolType::Freedraw,
        ToolType::Text,
        ToolType::Image,
        ToolType::Eraser,
        ToolType::Hand,
        ToolType::Frame,
        ToolType::Magicframe,
        ToolType::Stickynote,
        ToolType::Embeddable,
        ToolType::Laser,
        ToolType::Autoshape,
        ToolType::Bucketfill,
    ];

    /// The string upstream uses for the tool (`activeTool.type`).
    pub fn as_str(self) -> &'static str {
        match self {
            ToolType::Selection => "selection",
            ToolType::Lasso => "lasso",
            ToolType::Rectangle => "rectangle",
            ToolType::Diamond => "diamond",
            ToolType::Ellipse => "ellipse",
            ToolType::Arrow => "arrow",
            ToolType::Line => "line",
            ToolType::Freedraw => "freedraw",
            ToolType::Text => "text",
            ToolType::Image => "image",
            ToolType::Eraser => "eraser",
            ToolType::Hand => "hand",
            ToolType::Frame => "frame",
            ToolType::Magicframe => "magicframe",
            ToolType::Stickynote => "stickynote",
            ToolType::Embeddable => "embeddable",
            ToolType::Laser => "laser",
            ToolType::Autoshape => "autoshape",
            ToolType::Bucketfill => "bucketfill",
        }
    }

    /// The tool type named `name` (case-sensitive, as upstream compares).
    pub fn from_name(name: &str) -> Option<ToolType> {
        ToolType::ALL.into_iter().find(|t| t.as_str() == name)
    }
}

/// `ActiveTool`'s `type`/`customType` pair (`types.ts:171-178`): a built-in
/// tool, or a host-implemented `"custom"` tool with its `customType`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Tool {
    Builtin(ToolType),
    Custom(String),
}

impl Tool {
    /// `activeTool.type`: the built-in name, or `"custom"`.
    pub fn type_name(&self) -> &str {
        match self {
            Tool::Builtin(t) => t.as_str(),
            Tool::Custom(_) => "custom",
        }
    }

    /// `activeTool.customType`: `null` for built-in tools.
    pub fn custom_type(&self) -> Option<&str> {
        match self {
            Tool::Builtin(_) => None,
            Tool::Custom(c) => Some(c),
        }
    }

    /// The built-in tool type, if this is one.
    pub fn builtin(&self) -> Option<ToolType> {
        match self {
            Tool::Builtin(t) => Some(*t),
            Tool::Custom(_) => None,
        }
    }

    fn is(&self, ty: ToolType) -> bool {
        self.builtin() == Some(ty)
    }
}

impl From<ToolType> for Tool {
    fn from(ty: ToolType) -> Tool {
        Tool::Builtin(ty)
    }
}

/// `ToolConfig` (`Tools.tsx:42-57`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolConfig {
    /// The `icons.tsx` export upstream renders for the tool.
    pub icon: &'static str,
    /// Letter shortcut(s), lower case; the first one is shown in tooltips
    /// and badges. Empty when the tool has none.
    pub letter_key: &'static [&'static str],
    /// Whether `letter_key` requires Shift (e.g. Shift+X).
    pub shift_key: bool,
    /// Number-row shortcut.
    pub numeric_key: Option<&'static str>,
    /// Whether the tool's shapes can be filled; `None` where upstream leaves
    /// the key unset (read as false).
    pub fillable: Option<bool>,
    /// Re-activating the tool switches back to the previously active one.
    pub toggle: bool,
}

impl ToolConfig {
    /// `fillable` as upstream reads it (unset is false).
    pub fn is_fillable(&self) -> bool {
        self.fillable == Some(true)
    }
}

const fn cfg(
    icon: &'static str,
    letter_key: &'static [&'static str],
    numeric_key: Option<&'static str>,
) -> ToolConfig {
    ToolConfig {
        icon,
        letter_key,
        shift_key: false,
        numeric_key,
        fillable: None,
        toggle: false,
    }
}

const fn fillable(config: ToolConfig, value: bool) -> ToolConfig {
    ToolConfig {
        fillable: Some(value),
        ..config
    }
}

const fn toggle(config: ToolConfig) -> ToolConfig {
    ToolConfig {
        toggle: true,
        ..config
    }
}

const fn shift(config: ToolConfig) -> ToolConfig {
    ToolConfig {
        shift_key: true,
        ..config
    }
}

/// `TOOLS` (`Tools.tsx:69-160`), in upstream's key order. Every tool type
/// but `magicframe` has an entry (`ToolbarToolType`).
pub static TOOLS: [(ToolType, ToolConfig); 18] = [
    (ToolType::Hand, toggle(cfg("handIcon", &["h"], None))),
    (
        ToolType::Selection,
        fillable(cfg("SelectionIcon", &["v"], Some("1")), true),
    ),
    (
        ToolType::Rectangle,
        fillable(cfg("RectangleIcon", &["r"], Some("2")), true),
    ),
    (
        ToolType::Diamond,
        fillable(cfg("DiamondIcon", &["d"], Some("3")), true),
    ),
    (
        ToolType::Ellipse,
        fillable(cfg("EllipseIcon", &["o"], Some("4")), true),
    ),
    (
        ToolType::Arrow,
        fillable(cfg("ArrowIcon", &["a"], Some("5")), true),
    ),
    (
        ToolType::Line,
        fillable(cfg("LineIcon", &["l"], Some("6")), true),
    ),
    (
        ToolType::Freedraw,
        cfg("FreedrawIcon", &["p", "x"], Some("7")),
    ),
    (ToolType::Text, cfg("TextIcon", &["t"], Some("8"))),
    (
        ToolType::Stickynote,
        cfg("stickyNoteToolIcon", &["n"], None),
    ),
    (ToolType::Image, cfg("ImageIcon", &[], Some("9"))),
    (
        ToolType::Eraser,
        toggle(cfg("EraserIcon", &["e"], Some("0"))),
    ),
    (ToolType::Frame, cfg("frameToolIcon", &["f"], None)),
    (
        ToolType::Autoshape,
        fillable(shift(cfg("drawShapeToolIcon", &["x"], None)), false),
    ),
    (ToolType::Embeddable, cfg("EmbedIcon", &[], None)),
    (ToolType::Laser, cfg("laserPointerToolIcon", &["k"], None)),
    (ToolType::Bucketfill, cfg("bucketFillIcon", &["b"], None)),
    (
        ToolType::Lasso,
        fillable(cfg("LassoIcon", &[], None), false),
    ),
];

/// The registry entry for `ty` (`None` for `magicframe`).
pub fn tool_config(ty: ToolType) -> Option<&'static ToolConfig> {
    TOOLS.iter().find(|(t, _)| *t == ty).map(|(_, c)| c)
}

/// `TOGGLE_TOOLS` (`Tools.tsx:166-168`): the entries with `toggle` set.
pub const TOGGLE_TOOLS: [ToolType; 2] = [ToolType::Hand, ToolType::Eraser];

/// `TOGGLE_TOOLS.includes(tool.type)`.
pub fn is_toggle_tool(tool: &Tool) -> bool {
    tool.builtin().is_some_and(|t| TOGGLE_TOOLS.contains(&t))
}

/// `isSelectionLikeTool` (`utils.ts:272-274`).
pub fn is_selection_like_tool(tool: &Tool) -> bool {
    tool.is(ToolType::Selection) || tool.is(ToolType::Lasso)
}

/// The localized strings the shortcut hints use: `helpDialog.or` and
/// `keys.shift` (`getShortcutKey`, `shortcut.ts:5-19`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutLabels<'a> {
    pub or: &'a str,
    pub shift: &'a str,
}

impl ShortcutLabels<'static> {
    /// `en.json`: `helpDialog.or` = "or", `keys.shift` = "Shift".
    pub const EN: ShortcutLabels<'static> = ShortcutLabels {
        or: "or",
        shift: "Shift",
    };
}

/// JS `capitalizeString`: upper-cases the first character.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `getToolLetter` (`Tools.tsx:171-180`): the first letter key,
/// capitalized, prefixed by the localized Shift for shift-bound tools.
pub fn get_tool_letter(ty: ToolType, labels: &ShortcutLabels<'_>) -> Option<String> {
    let config = tool_config(ty)?;
    let letter = capitalize(config.letter_key.first()?);
    Some(if config.shift_key {
        format!("{}+{letter}", labels.shift)
    } else {
        letter
    })
}

/// `getToolShortcut` (`Tools.tsx:183-190`): "R or 2", used in tooltips and
/// aria. With neither key set upstream interpolates `undefined`, so the
/// result is the string `"undefined"`, as there.
pub fn get_tool_shortcut(ty: ToolType, labels: &ShortcutLabels<'_>) -> String {
    let letter = get_tool_letter(ty, labels);
    let numeric = tool_config(ty).and_then(|c| c.numeric_key);
    match (letter, numeric) {
        (Some(letter), Some(numeric)) => format!("{letter} {} {numeric}", labels.or),
        (Some(letter), None) => letter,
        (None, Some(numeric)) => numeric.to_owned(),
        (None, None) => "undefined".to_owned(),
    }
}

/// `preferredSelectionTool.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionTool {
    #[default]
    Selection,
    Lasso,
}

impl SelectionTool {
    pub fn tool_type(self) -> ToolType {
        match self {
            SelectionTool::Selection => ToolType::Selection,
            SelectionTool::Lasso => ToolType::Lasso,
        }
    }
}

/// `findShapeByKey` (`Tools.tsx:192-223`): the tool bound to `key`.
///
/// Letter keys are CapsLock-insensitive: the caller excludes every modifier
/// but Shift, and Shift is matched explicitly (shift-bound tools require it,
/// the rest require its absence), so a capital letter on its own means
/// CapsLock. Number keys compare exactly. The selection shortcut returns the
/// preferred selection tool.
pub fn find_shape_by_key(key: &str, preferred: SelectionTool, shift_key: bool) -> Option<ToolType> {
    let lower = key.to_lowercase();
    TOOLS
        .iter()
        .filter(|(_, c)| c.shift_key == shift_key)
        .find(|(_, c)| c.numeric_key == Some(key) || c.letter_key.contains(&lower.as_str()))
        .map(|(ty, _)| match ty {
            ToolType::Selection => preferred.tool_type(),
            other => *other,
        })
}

/// `AppState.activeTool` (`types.ts:424-433`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTool {
    pub tool: Tool,
    /// The tool to revert to when a toggle tool is left (the whole
    /// `activeTool` that was current when it was activated).
    pub last_active_tool: Option<Box<ActiveTool>>,
    /// The tool lock (padlock / Q).
    pub locked: bool,
    /// The tool was temporarily switched on from the selection tool.
    pub from_selection: bool,
}

impl Default for ActiveTool {
    /// `getDefaultAppState().activeTool` (`appState.ts`).
    fn default() -> ActiveTool {
        ActiveTool {
            tool: Tool::Builtin(ToolType::Selection),
            last_active_tool: None,
            locked: false,
            from_selection: false,
        }
    }
}

impl ActiveTool {
    /// The app-state JSON, in upstream's key order.
    pub fn to_json(&self) -> Value {
        json!({
            "type": self.tool.type_name(),
            "customType": self.tool.custom_type(),
            "locked": self.locked,
            "fromSelection": self.from_selection,
            "lastActiveTool": self.last_active_tool.as_deref().map(ActiveTool::to_json),
        })
    }

    /// Reads the app-state JSON; `None` for an unknown `type` or a
    /// `"custom"` tool without a string `customType`. Missing booleans read
    /// as false.
    pub fn from_json(value: &Value) -> Option<ActiveTool> {
        let obj: &Map<String, Value> = value.as_object()?;
        let ty = obj.get("type")?.as_str()?;
        let tool = if ty == "custom" {
            Tool::Custom(obj.get("customType")?.as_str()?.to_owned())
        } else {
            Tool::Builtin(ToolType::from_name(ty)?)
        };
        let last_active_tool = match obj.get("lastActiveTool") {
            None | Some(Value::Null) => None,
            Some(v) => Some(Box::new(ActiveTool::from_json(v)?)),
        };
        let flag = |k: &str| obj.get(k).and_then(Value::as_bool).unwrap_or(false);
        Some(ActiveTool {
            tool,
            last_active_tool,
            locked: flag("locked"),
            from_selection: flag("fromSelection"),
        })
    }
}

/// The `data` argument of `updateActiveTool`: `None` fields are
/// `undefined` there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveToolUpdate {
    pub tool: Tool,
    pub locked: Option<bool>,
    pub from_selection: Option<bool>,
    /// `Some(None)` sets `lastActiveTool: null`; `None` keeps the current.
    pub last_active_tool: Option<Option<ActiveTool>>,
}

impl ActiveToolUpdate {
    /// `{ type }` alone.
    pub fn to(tool: Tool) -> ActiveToolUpdate {
        ActiveToolUpdate {
            tool,
            locked: None,
            from_selection: None,
            last_active_tool: None,
        }
    }

    /// `{ ...activeTool, lastActiveTool }`: spreading a whole active tool
    /// (upstream does so when switching back from a toggle tool, carrying
    /// the recorded tool's `locked` and `fromSelection`).
    fn spread(tool: &ActiveTool, last_active_tool: Option<Option<ActiveTool>>) -> ActiveToolUpdate {
        ActiveToolUpdate {
            tool: tool.tool.clone(),
            locked: Some(tool.locked),
            from_selection: Some(tool.from_selection),
            last_active_tool,
        }
    }
}

/// `updateActiveTool` (`utils.ts:276-307`).
///
/// A custom tool keeps `lastActiveTool` and `fromSelection`; a built-in
/// tool takes `lastActiveTool` from `update` when given and resets
/// `fromSelection` unless given. `locked` is kept unless given.
pub fn update_active_tool(current: &ActiveTool, update: ActiveToolUpdate) -> ActiveTool {
    let locked = update.locked.unwrap_or(current.locked);
    match update.tool {
        Tool::Custom(_) => ActiveTool {
            tool: update.tool,
            locked,
            ..current.clone()
        },
        Tool::Builtin(_) => ActiveTool {
            last_active_tool: match update.last_active_tool {
                None => current.last_active_tool.clone(),
                Some(last) => last.map(Box::new),
            },
            tool: update.tool,
            locked,
            from_selection: update.from_selection.unwrap_or(false),
        },
    }
}

/// `AppState.preferredSelectionTool` (`types.ts:434-437`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PreferredSelectionTool {
    pub tool: SelectionTool,
    pub initialized: bool,
}

/// `AppProps.interaction` as `isInteractionEnabled` / `isToolSupported`
/// read it (`App.tsx:963-1069`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interaction {
    /// `undefined` or `true`: fully interactive.
    #[default]
    Enabled,
    /// `false`: fully inert.
    Disabled,
    /// An object: non-interactive, except the tools opted in via
    /// `enabled.tools`.
    Restricted { laser: bool, custom: bool },
}

/// The host props the tool state reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOptions {
    /// `UIOptions.tools.image` (default true).
    pub image_tool: bool,
    /// `props.interaction`.
    pub interaction: Interaction,
    /// `props.activeTool`: a host-forced tool. Set through
    /// [`ToolState::force_tool`].
    pub forced_tool: Option<Tool>,
}

impl Default for ToolOptions {
    fn default() -> ToolOptions {
        ToolOptions {
            image_tool: true,
            interaction: Interaction::Enabled,
            forced_tool: None,
        }
    }
}

/// Why [`ToolState::set_active_tool`] refused (upstream warns and returns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRefusal {
    /// Disabled via `UIOptions.tools`, or not enabled while non-interactive.
    Unsupported,
    /// The active tool is host-controlled and this is not the forced tool.
    Forced,
}

/// The argument of `setActiveTool`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRequest {
    pub tool: Tool,
    pub locked: Option<bool>,
    pub from_selection: Option<bool>,
}

impl ToolRequest {
    pub fn new(tool: Tool) -> ToolRequest {
        ToolRequest {
            tool,
            locked: None,
            from_selection: None,
        }
    }

    fn update(self, last_active_tool: Option<Option<ActiveTool>>) -> ActiveToolUpdate {
        ActiveToolUpdate {
            tool: self.tool,
            locked: self.locked,
            from_selection: self.from_selection,
            last_active_tool,
        }
    }
}

/// `setActiveTool`'s options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SetActiveToolOptions {
    /// Keep the selection when switching to the lasso.
    pub keep_selection: bool,
    /// Re-activating an active toggle tool switches back to the previous
    /// tool (keyboard shortcuts pass it).
    pub toggle: bool,
}

/// What `setActiveTool` does to the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorEffect {
    /// `CURSOR_TYPE.GRAB` (the hand tool).
    Grab,
    /// `cursor.applyForTool(activeTool)`.
    ForTool,
    /// Space-panning keeps its cursor.
    Unchanged,
}

/// The rest of `setActiveTool`, for the caller to apply. Every switch also
/// resets `snapLines`, `originSnapOffset`, `activeEmbeddable` and
/// `frameToHighlight`, and moves focus back to the container when a tool
/// button has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSwitch {
    /// A drawShape gesture was pending: run `actionFinalize` first, while
    /// the old tool is still active.
    pub finalize_pending_gesture: bool,
    pub cursor: CursorEffect,
    /// Clear `suggestedBinding` (the new tool is not linear).
    pub clear_suggested_binding: bool,
    /// Open the image file picker (`onImageToolbarButtonClick`).
    pub open_image_picker: bool,
    /// `store.scheduleCapture()` (the freedraw tool).
    pub schedule_capture: bool,
    /// Clear `selectedElementIds`, `selectedGroupIds`, `editingGroupId` and
    /// `multiElement`.
    pub clear_selection: bool,
    /// Keep `selectedLinearElement` (selection-like tools); cleared
    /// otherwise.
    pub keep_selected_linear_element: bool,
}

/// `PointerEvent.pointerType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerType {
    Mouse,
    Pen,
    Touch,
}

/// `AppState.currentItemArrowType` (`ARROW_TYPE`, `constants.ts:588-592`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrowType {
    Sharp,
    /// The default (`appState.ts:45`).
    #[default]
    Round,
    Elbow,
}

impl ArrowType {
    /// sharp → round → elbow → sharp (`App.tsx:5803-5808`).
    pub fn next(self) -> ArrowType {
        match self {
            ArrowType::Sharp => ArrowType::Round,
            ArrowType::Round => ArrowType::Elbow,
            ArrowType::Elbow => ArrowType::Sharp,
        }
    }
}

/// A keydown, as the tool keys read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToolKeyEvent<'a> {
    /// `event.key`.
    pub key: &'a str,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// The rest of the editor state the tool keys read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToolKeyContext {
    /// `props.viewModeEnabled === true`: the host pins view mode.
    pub prevent_tool_switching: bool,
    /// `state.viewModeEnabled`.
    pub view_mode_enabled: bool,
    /// `newElement`, `selectionElement` or a drag of the selection is in
    /// progress.
    pub gesture_in_progress: bool,
    /// `state.currentItemArrowType`.
    pub arrow_type: ArrowType,
}

/// Which key form picked the arrow or line tool, for the cursor hint
/// (`cursorHints.onToolShortcut`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyHintKind {
    Digit,
    Letter,
}

/// What a tool key did besides the arrow-type cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKeyAction {
    /// B with the bucket fill already active: cycle its background colour
    /// (`bucketFill.cycleBackgroundColor`).
    CycleBucketFillColor,
    /// The tool switch (or its refusal).
    SetTool(Result<ToolSwitch, ToolRefusal>),
}

/// The outcome of [`ToolState::handle_tool_key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKeyOutcome {
    /// Not a tool key; keydown handling continues.
    NotHandled,
    /// View mode: keydown handling stops here without stopping propagation.
    Ignored,
    /// Escape in view mode switched to the selection tool.
    ViewModeEscape(Result<ToolSwitch, ToolRefusal>),
    /// A tool key (propagation stopped).
    Tool {
        tool: ToolType,
        /// Set `currentItemArrowType` to this (A / 5 with the arrow active).
        next_arrow_type: Option<ArrowType>,
        /// Show the arrow/line shortcut cursor hint.
        hint: Option<KeyHintKind>,
        action: ToolKeyAction,
    },
    /// Q toggled the tool lock (`changed` is false while the tool is
    /// forced). Propagation stopped.
    ToggledLock { changed: bool },
}

/// The editor's tool state: the `AppState` keys this module owns, the host
/// props it reads, and two runtime flags `setActiveTool` consults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolState {
    pub active_tool: ActiveTool,
    pub preferred_selection_tool: PreferredSelectionTool,
    pub pen_mode: bool,
    pub pen_detected: bool,
    /// `currentItemStrokeVariability`, which pen mode switches to variable.
    pub stroke_variability: StrokeVariability,
    pub options: ToolOptions,
    /// Space is held for panning (`pan.isSpaceHeld()`).
    pub space_held: bool,
    /// A drawShape (autoshape) gesture is pending
    /// (`drawShape.hasPendingGesture()`), maintained by the caller.
    pub pending_draw_shape: bool,
}

impl Default for ToolState {
    /// `getDefaultAppState()`'s tool keys and default props.
    fn default() -> ToolState {
        ToolState {
            active_tool: ActiveTool::default(),
            preferred_selection_tool: PreferredSelectionTool::default(),
            pen_mode: false,
            pen_detected: false,
            stroke_variability: StrokeVariability::Constant,
            options: ToolOptions::default(),
            space_held: false,
            pending_draw_shape: false,
        }
    }
}

/// `isSameForcedTool` (`App.tsx:3519-3525`): same type, and the same
/// `customType` for custom tools.
fn same_tool(a: &Tool, b: &Tool) -> bool {
    a == b
}

impl ToolState {
    /// `isInteractionEnabled` (`App.tsx:963-967`).
    pub fn is_interaction_enabled(&self) -> bool {
        self.options.interaction == Interaction::Enabled
    }

    /// `isToolSupported` (`App.tsx:1045-1069`).
    pub fn is_tool_supported(&self, tool: &Tool) -> bool {
        if tool.is(ToolType::Image) && !self.options.image_tool {
            return false;
        }
        match self.options.interaction {
            Interaction::Enabled => true,
            Interaction::Disabled => false,
            Interaction::Restricted { laser, custom } => match tool {
                Tool::Builtin(ToolType::Laser) => laser,
                Tool::Custom(_) => custom,
                Tool::Builtin(_) => false,
            },
        }
    }

    /// `isToolLocked` (`App.tsx:1079-1081`): the padlock, or a forced tool.
    pub fn is_tool_locked(&self) -> bool {
        self.active_tool.locked || self.options.forced_tool.is_some()
    }

    /// `isToolButtonDisabled` (`Tools.tsx:230-231`): a tool is forced and
    /// `type_name` is not its type.
    pub fn is_tool_button_disabled(&self, type_name: &str) -> bool {
        self.options
            .forced_tool
            .as_ref()
            .is_some_and(|f| f.type_name() != type_name)
    }

    /// `setActiveTool` (`App.tsx:6210-6335`).
    pub fn set_active_tool(
        &mut self,
        request: ToolRequest,
        opts: SetActiveToolOptions,
    ) -> Result<ToolSwitch, ToolRefusal> {
        if !self.is_tool_supported(&request.tool) {
            return Err(ToolRefusal::Unsupported);
        }
        if let Some(forced) = &self.options.forced_tool {
            if !same_tool(forced, &request.tool) {
                return Err(ToolRefusal::Forced);
            }
        }
        let finalize_pending_gesture = self.pending_draw_shape;

        let is_toggle = is_toggle_tool(&request.tool);
        let toggle = opts.toggle && is_toggle;
        let current = &self.active_tool;
        let next = if toggle && current.tool == request.tool {
            // toggle back to the tool that was active before this one
            self.revert_from_toggle_tool()
        } else if is_toggle && current.tool != request.tool {
            // record the current tool so Esc and the next toggle return to it
            update_active_tool(current, request.update(Some(Some(current.clone()))))
        } else {
            update_active_tool(current, request.update(None))
        };

        let next_type = next.tool.builtin();
        let cursor = if next_type == Some(ToolType::Hand) {
            CursorEffect::Grab
        } else if !self.space_held {
            CursorEffect::ForTool
        } else {
            CursorEffect::Unchanged
        };
        let clear_selection = match next_type {
            Some(ToolType::Selection) => false,
            Some(ToolType::Lasso) => !opts.keep_selection,
            _ => true,
        };
        let switch = ToolSwitch {
            finalize_pending_gesture,
            cursor,
            clear_suggested_binding: !matches!(
                next_type,
                Some(ToolType::Arrow) | Some(ToolType::Line)
            ),
            open_image_picker: next_type == Some(ToolType::Image),
            schedule_capture: next_type == Some(ToolType::Freedraw),
            clear_selection,
            keep_selected_linear_element: is_selection_like_tool(&next.tool),
        };
        self.active_tool = next;
        Ok(switch)
    }

    /// `{ ...(lastActiveTool || { type: preferred }), lastActiveTool: null }`
    /// through `updateActiveTool`.
    fn revert_from_toggle_tool(&self) -> ActiveTool {
        let update = match self.active_tool.last_active_tool.as_deref() {
            Some(last) => ActiveToolUpdate::spread(last, Some(None)),
            None => ActiveToolUpdate {
                last_active_tool: Some(None),
                ..self.preferred_update()
            },
        };
        update_active_tool(&self.active_tool, update)
    }

    fn preferred_update(&self) -> ActiveToolUpdate {
        ActiveToolUpdate::to(Tool::Builtin(
            self.preferred_selection_tool.tool.tool_type(),
        ))
    }

    /// The tool Esc (`actionDeselect.ts:18-33`) and finalizing an element
    /// (`actionFinalize.tsx:343-355`) switch to: the recorded tool when
    /// leaving a toggle tool, else the preferred selection tool. Whether it
    /// applies (the tool lock) is the caller's call.
    pub fn tool_after_finalize(&self) -> ActiveTool {
        if is_toggle_tool(&self.active_tool.tool) {
            self.revert_from_toggle_tool()
        } else {
            update_active_tool(&self.active_tool, self.preferred_update())
        }
    }

    /// Sets (or clears) the host-forced tool (`props.activeTool`) and
    /// applies it through [`Self::sync_options`].
    pub fn force_tool(&mut self, tool: Option<Tool>) {
        self.options.forced_tool = tool;
        self.sync_options();
    }

    /// The tool half of `componentDidUpdate` (`App.tsx:4326-4327`): run after
    /// any change to [`Self::options`] (interaction, forced tool, image
    /// tool). First [`Self::reset_unsupported_tool`]
    /// (`handleInteractionStateChange`), then [`Self::sync_forced_tool`]
    /// (`handleForcedToolChange`).
    pub fn sync_options(&mut self) {
        self.reset_unsupported_tool();
        self.sync_forced_tool();
    }

    /// The non-interactive invariant of `handleInteractionStateChange`
    /// (`App.tsx:3486-3499`): while interaction is not fully enabled, an
    /// active tool that is not supported (and is not already selection)
    /// resets to selection through `updateActiveTool`, bypassing
    /// [`Self::set_active_tool`], so it applies although selection itself is
    /// unsupported while inert (e.g. a presenter's laser after handing off).
    pub fn reset_unsupported_tool(&mut self) {
        if !self.is_interaction_enabled()
            && !self.is_tool_supported(&self.active_tool.tool)
            && !self.active_tool.tool.is(ToolType::Selection)
        {
            self.active_tool = update_active_tool(
                &self.active_tool,
                ActiveToolUpdate::to(Tool::Builtin(ToolType::Selection)),
            );
        }
    }

    /// `handleForcedToolChange` (`App.tsx:3535-3571`): re-applies the forced
    /// tool after a write that bypassed [`Self::set_active_tool`] or an
    /// option change that made it activatable. The image tool cannot be
    /// forced; an unsupported forced tool leaves the tool as is. After an
    /// option change, call [`Self::sync_options`], which resets a stale
    /// tool first.
    pub fn sync_forced_tool(&mut self) {
        let Some(forced) = self.options.forced_tool.clone() else {
            return;
        };
        if forced.is(ToolType::Image) || same_tool(&forced, &self.active_tool.tool) {
            return;
        }
        // a refusal (unsupported) leaves the tool as is, as upstream
        let _ = self.set_active_tool(ToolRequest::new(forced), SetActiveToolOptions::default());
    }

    /// `toggleLock` (`App.tsx:5220-5245`). Returns whether it changed
    /// anything: a forced tool owns its lock state. Unlocking also returns
    /// to the preferred selection tool.
    pub fn toggle_lock(&mut self) -> bool {
        if self.options.forced_tool.is_some() {
            return false;
        }
        let current = &self.active_tool;
        let mut next = if current.locked {
            update_active_tool(current, self.preferred_update())
        } else {
            // `updateActiveTool(state, activeTool)` spreads the active tool
            // over itself: only the lock changes
            current.clone()
        };
        next.locked = !current.locked;
        self.active_tool = next;
        true
    }

    /// `togglePenMode` (`App.tsx:5271-5281`): flips pen mode (or sets it to
    /// `force`) and marks a pen as detected; the first time, strokes switch
    /// to variable width.
    pub fn toggle_pen_mode(&mut self, force: Option<bool>) {
        self.pen_mode = force.unwrap_or(!self.pen_mode);
        if !self.pen_detected {
            self.stroke_variability = StrokeVariability::Variable;
        }
        self.pen_detected = true;
    }

    /// Pen detection on pointer down (`App.tsx:8830-8837`): the first pen
    /// pointer turns pen mode on, once. Returns whether it did.
    pub fn detect_pen(&mut self, pointer: PointerType) -> bool {
        if self.pen_detected || pointer != PointerType::Pen {
            return false;
        }
        self.pen_mode = true;
        self.pen_detected = true;
        self.stroke_variability = StrokeVariability::Variable;
        true
    }

    /// The pen-mode gate on canvas pointer down (`App.tsx:8963-8969`): in
    /// pen mode touch input only reaches the selection, lasso, text and
    /// image tools (a palm resting on the screen does not draw).
    pub fn allows_pointer_down(&self, pointer: PointerType) -> bool {
        !self.pen_mode
            || pointer != PointerType::Touch
            || matches!(
                self.active_tool.tool.builtin(),
                Some(ToolType::Selection | ToolType::Lasso | ToolType::Text | ToolType::Image)
            )
    }

    /// Two-finger pinch keeps the zoom while drawing freehand in pen mode
    /// (`App.tsx:9341-9343`).
    pub fn pinch_zoom_locked(&self) -> bool {
        self.pen_mode && self.active_tool.tool.is(ToolType::Freedraw)
    }

    /// The tool keys of `onKeyDown` (`App.tsx:5768-5846`), after the action
    /// manager has had the event.
    pub fn handle_tool_key(
        &mut self,
        event: ToolKeyEvent<'_>,
        ctx: &ToolKeyContext,
    ) -> ToolKeyOutcome {
        if ctx.prevent_tool_switching {
            return ToolKeyOutcome::NotHandled;
        }
        if ctx.view_mode_enabled && event.key == "Escape" {
            let result = self.set_active_tool(
                ToolRequest::new(Tool::Builtin(ToolType::Selection)),
                SetActiveToolOptions::default(),
            );
            return ToolKeyOutcome::ViewModeEscape(result);
        }
        if event.ctrl || event.alt || event.meta || ctx.gesture_in_progress {
            return ToolKeyOutcome::NotHandled;
        }
        let shape = find_shape_by_key(event.key, self.preferred_selection_tool.tool, event.shift);
        if ctx.view_mode_enabled && !matches!(shape, Some(ToolType::Laser | ToolType::Hand)) {
            return ToolKeyOutcome::Ignored;
        }
        let Some(shape) = shape else {
            if event.key == "q" {
                return ToolKeyOutcome::ToggledLock {
                    changed: self.toggle_lock(),
                };
            }
            return ToolKeyOutcome::NotHandled;
        };

        let active = self.active_tool.tool.builtin();
        let (next_arrow_type, hint) = if shape == ToolType::Arrow && active == Some(ToolType::Arrow)
        {
            (Some(ctx.arrow_type.next()), None)
        } else if matches!(shape, ToolType::Arrow | ToolType::Line) {
            let digit = event.key.len() == 1 && event.key.as_bytes()[0].is_ascii_digit();
            (
                None,
                Some(if digit {
                    KeyHintKind::Digit
                } else {
                    KeyHintKind::Letter
                }),
            )
        } else {
            (None, None)
        };

        let action = if shape == ToolType::Bucketfill && active == Some(ToolType::Bucketfill) {
            ToolKeyAction::CycleBucketFillColor
        } else if shape == ToolType::Lasso && active == Some(ToolType::Laser) {
            ToolKeyAction::SetTool(self.set_active_tool(
                ToolRequest::new(Tool::Builtin(
                    self.preferred_selection_tool.tool.tool_type(),
                )),
                SetActiveToolOptions::default(),
            ))
        } else {
            ToolKeyAction::SetTool(self.set_active_tool(
                ToolRequest::new(Tool::Builtin(shape)),
                SetActiveToolOptions {
                    toggle: true,
                    ..SetActiveToolOptions::default()
                },
            ))
        };
        ToolKeyOutcome::Tool {
            tool: shape,
            next_arrow_type,
            hint,
            action,
        }
    }
}
