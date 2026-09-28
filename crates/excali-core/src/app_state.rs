//! Editor state: [`AppState`], its defaults, which keys each storage target
//! keeps, and `restoreAppState`.
//!
//! Upstream: `packages/excalidraw/appState.ts` (`getDefaultAppState`,
//! `APP_STATE_STORAGE_CONF`, `cleanAppStateForExport`,
//! `clearAppStateForLocalStorage`, `clearAppStateForDatabase`) and
//! `packages/excalidraw/data/restore.ts:220-244, 1175-1372`
//! (`AllowedExcalidrawActiveTools`, `restoreAppState`), at the pinned
//! commit. See `site/content/research/data-model.md` section 3.2.
//!
//! A `.excalidraw` file saves five keys (`gridSize`, `gridStep`,
//! `gridModeEnabled`, `viewBackgroundColor`, `lockedMultiSelections`;
//! [`EXPORTED_KEYS`]); every other key is browser-only or not persisted,
//! dropped on save and defaulted on load.
//!
//! Upstream's `AppState` is a plain object and `restoreAppState` copies
//! imported values without checking their types (a `theme: 123` survives),
//! so [`AppState`] is that object: a JSON map in upstream's key order, with
//! typed accessors for the keys the port reads. Values JSON cannot hold are
//! written as `JSON.stringify` writes them: `collaborators` (a `Map`) is
//! `{}`, a NaN zoom is `null`, an `undefined` property is absent.
//! `restoreAppState` throws for some malformed input (a `null` `activeTool`,
//! an object whose own `toString` key makes it unconvertible);
//! [`restore_app_state`] returns the same `TypeError` as an error.

use excali_math::clamp;
use excali_math::js as math;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Map, Value};

use crate::color::{alpha_of_value, color_to_hex};
use crate::constants::{
    COLOR_TOP_PICKS_SLOTS, DEFAULT_ELEMENT_PROPS, DEFAULT_ELEMENT_STROKE_WIDTH_KEY,
    DEFAULT_FONT_SIZE, DEFAULT_GRID_SIZE, DEFAULT_GRID_STEP, DEFAULT_SIDEBAR_NAME,
    DEFAULT_STICKY_NOTE_BG, DEFAULT_TEXT_ALIGN, DEFAULT_ZOOM, EXPORT_SCALES, FONT_TOP_PICKS_SLOTS,
    MAX_ZOOM, MIN_ZOOM, STATS_PANEL_ELEMENT_PROPERTIES, STATS_PANEL_GENERAL_STATS, STROKE_WIDTH,
};
use crate::element::{FontFamily, StrokeWidthKey};
use crate::js::{self, number_value, truthy};

pub use crate::js::TypeError;

/// The error [`restore_app_state`] returns where upstream's
/// `restoreAppState` throws.
pub type RestoreAppStateError = TypeError;

/// What upstream reads from its environment when building the defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppStateEnv {
    /// `window.devicePixelRatio`: the default `exportScale` when it is one
    /// of [`EXPORT_SCALES`], 1 otherwise (`appState.ts:20-22`).
    pub device_pixel_ratio: f64,
    /// Upstream's test build (`isTestEnv()`), where the default
    /// `currentItemRoundness` is `"sharp"` instead of `"round"`
    /// (`appState.ts:45`). Off in every shipped build.
    pub test_env: bool,
}

impl Default for AppStateEnv {
    fn default() -> AppStateEnv {
        AppStateEnv {
            device_pixel_ratio: 1.0,
            test_env: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Storage configuration

/// Where a key is kept (`APP_STATE_STORAGE_CONF`, `appState.ts:153-291`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageType {
    /// Browser storage (localStorage, IndexedDB).
    Browser,
    /// A file or the database (`.excalidraw`, `.png`/`.svg` payloads).
    Export,
    /// The server (share links, collaboration).
    Server,
}

/// One key's flags in `APP_STATE_STORAGE_CONF`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StorageConf {
    pub browser: bool,
    pub export: bool,
    pub server: bool,
}

impl StorageConf {
    /// The flag for `storage`.
    pub const fn keeps(self, storage: StorageType) -> bool {
        match storage {
            StorageType::Browser => self.browser,
            StorageType::Export => self.export,
            StorageType::Server => self.server,
        }
    }
}

const NONE: StorageConf = StorageConf {
    browser: false,
    export: false,
    server: false,
};
const BROWSER: StorageConf = StorageConf {
    browser: true,
    export: false,
    server: false,
};
const ALL: StorageConf = StorageConf {
    browser: true,
    export: true,
    server: true,
};

/// `APP_STATE_STORAGE_CONF` (`appState.ts:153-291`), in upstream's order:
/// every `AppState` key and where it is kept.
pub const APP_STATE_STORAGE_CONF: &[(&str, StorageConf)] = &[
    ("showWelcomeScreen", BROWSER),
    ("theme", BROWSER),
    ("collaborators", NONE),
    ("currentItemBackgroundColor", BROWSER),
    ("currentItemEndArrowhead", BROWSER),
    ("currentItemFillStyle", BROWSER),
    ("currentItemFontFamily", BROWSER),
    ("currentItemFontSize", BROWSER),
    ("currentItemRoundness", BROWSER),
    ("currentItemArrowType", BROWSER),
    ("currentItemOpacity", BROWSER),
    ("currentItemRoughness", BROWSER),
    ("currentItemStrokeVariability", BROWSER),
    ("currentItemStartArrowhead", BROWSER),
    ("currentItemStrokeColor", BROWSER),
    ("currentItemStickynoteStrokeColor", BROWSER),
    ("currentItemStickynoteBackgroundColor", BROWSER),
    ("currentItemStrokeStyle", BROWSER),
    ("currentItemStrokeWidthKey", BROWSER),
    ("currentItemTextAlign", BROWSER),
    ("currentHoveredFontFamily", NONE),
    ("cursorButton", BROWSER),
    ("activeEmbeddable", NONE),
    ("newElement", NONE),
    ("editingTextElement", NONE),
    ("editingGroupId", BROWSER),
    ("activeTool", BROWSER),
    ("preferredSelectionTool", BROWSER),
    ("penMode", BROWSER),
    ("penDetected", BROWSER),
    ("errorMessage", NONE),
    ("exportBackground", BROWSER),
    ("exportEmbedScene", BROWSER),
    ("exportScale", BROWSER),
    ("exportWithDarkMode", BROWSER),
    ("fileHandle", NONE),
    ("gridSize", ALL),
    ("gridStep", ALL),
    ("gridModeEnabled", ALL),
    ("height", NONE),
    ("isBindingEnabled", BROWSER),
    ("boxSelectionMode", BROWSER),
    ("bindingPreference", BROWSER),
    ("isMidpointSnappingEnabled", BROWSER),
    ("showHints", BROWSER),
    ("inputDevice", BROWSER),
    ("defaultSidebarDockedPreference", BROWSER),
    ("isLoading", NONE),
    ("isResizing", NONE),
    ("isRotating", NONE),
    ("lastPointerDownWith", BROWSER),
    ("multiElement", NONE),
    ("name", BROWSER),
    ("offsetLeft", NONE),
    ("offsetTop", NONE),
    ("contextMenu", NONE),
    ("openMenu", BROWSER),
    ("openPopup", NONE),
    ("openSidebar", BROWSER),
    ("openDialog", NONE),
    ("previousSelectedElementIds", BROWSER),
    ("resizingElement", NONE),
    ("scrolledOutside", BROWSER),
    ("scrollX", BROWSER),
    ("scrollY", BROWSER),
    ("scrollConstraints", NONE),
    ("selectedElementIds", BROWSER),
    ("hoveredElementIds", NONE),
    ("selectedGroupIds", BROWSER),
    ("selectedElementsAreBeingDragged", NONE),
    ("selectionElement", NONE),
    ("shouldCacheIgnoreZoom", BROWSER),
    ("stats", BROWSER),
    ("suggestedBinding", NONE),
    ("textToolHover", NONE),
    ("frameRendering", NONE),
    ("frameToHighlight", NONE),
    ("editingFrame", NONE),
    ("elementsToHighlight", NONE),
    ("toast", NONE),
    ("viewBackgroundColor", ALL),
    ("width", NONE),
    ("zenModeEnabled", BROWSER),
    ("zoom", BROWSER),
    ("viewModeEnabled", NONE),
    ("showHyperlinkPopup", NONE),
    ("selectedLinearElement", BROWSER),
    ("snapLines", NONE),
    ("originSnapOffset", NONE),
    ("objectsSnapModeEnabled", BROWSER),
    ("isCropping", NONE),
    ("croppingElementId", NONE),
    ("searchMatches", NONE),
    ("lockedMultiSelections", ALL),
    ("activeLockedId", NONE),
    ("bindMode", BROWSER),
    ("colorTopPicks", BROWSER),
    ("fontTopPicks", BROWSER),
];

/// The keys a `.excalidraw` file (and the server) keeps, in
/// `APP_STATE_STORAGE_CONF` order (`appState.ts:221-223, 273, 286`).
pub const EXPORTED_KEYS: [&str; 5] = [
    "gridSize",
    "gridStep",
    "gridModeEnabled",
    "viewBackgroundColor",
    "lockedMultiSelections",
];

/// `APP_STATE_STORAGE_CONF[key]`; `None` for a key it does not list.
pub fn storage_conf(key: &str) -> Option<StorageConf> {
    APP_STATE_STORAGE_CONF
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, conf)| *conf)
}

/// `_clearAppStateForStorage(appState, storage)` (`appState.ts:293-315`):
/// the keys of `app_state` kept for `storage`, in `app_state`'s order.
/// Unknown keys are dropped.
pub fn clear_app_state_for_storage(
    app_state: &Map<String, Value>,
    storage: StorageType,
) -> Map<String, Value> {
    app_state
        .iter()
        .filter(|(key, _)| storage_conf(key).is_some_and(|conf| conf.keeps(storage)))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// `clearAppStateForLocalStorage` (`appState.ts:317-319`).
pub fn clear_app_state_for_local_storage(app_state: &Map<String, Value>) -> Map<String, Value> {
    clear_app_state_for_storage(app_state, StorageType::Browser)
}

/// `cleanAppStateForExport` (`appState.ts:321-323`): what `serializeAsJSON`
/// writes as a file's `appState`, i.e. the [`EXPORTED_KEYS`] present.
pub fn clean_app_state_for_export(app_state: &Map<String, Value>) -> Map<String, Value> {
    clear_app_state_for_storage(app_state, StorageType::Export)
}

/// `clearAppStateForDatabase` (`appState.ts:325-327`).
pub fn clear_app_state_for_database(app_state: &Map<String, Value>) -> Map<String, Value> {
    clear_app_state_for_storage(app_state, StorageType::Server)
}

// ---------------------------------------------------------------------------
// Active tools

/// `AllowedExcalidrawActiveTools` (`restore.ts:220-244`): the tools a
/// restored `activeTool` may keep; any other becomes `selection`.
pub const ALLOWED_ACTIVE_TOOLS: &[(&str, bool)] = &[
    ("selection", true),
    ("lasso", true),
    ("text", true),
    ("rectangle", true),
    ("diamond", true),
    ("ellipse", true),
    ("line", true),
    ("image", true),
    ("arrow", true),
    ("freedraw", true),
    ("stickynote", true),
    ("eraser", false),
    ("custom", true),
    ("frame", true),
    ("embeddable", true),
    ("hand", true),
    ("laser", false),
    ("autoshape", false),
    ("magicframe", false),
    ("bucketfill", true),
];

/// Whether `ALLOWED_ACTIVE_TOOLS` allows the tool type `tool`.
pub fn is_allowed_active_tool(tool: &str) -> bool {
    ALLOWED_ACTIVE_TOOLS
        .iter()
        .any(|(name, allowed)| *name == tool && *allowed)
}

/// The properties every object inherits from `Object.prototype` in V8.
/// `AllowedExcalidrawActiveTools[type]` finds them too, and all are truthy
/// (functions, and `__proto__` the prototype itself).
const OBJECT_PROTOTYPE_PROPERTIES: [&str; 12] = [
    "constructor",
    "__defineGetter__",
    "__defineSetter__",
    "hasOwnProperty",
    "__lookupGetter__",
    "__lookupSetter__",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toString",
    "valueOf",
    "__proto__",
    "toLocaleString",
];

/// `!!AllowedExcalidrawActiveTools[key]`, inherited properties included.
fn allowed_tool_lookup(key: &str) -> bool {
    is_allowed_active_tool(key) || OBJECT_PROTOTYPE_PROPERTIES.contains(&key)
}

// ---------------------------------------------------------------------------
// AppState

/// Editor state: the JSON object upstream's `AppState` serialises to, in
/// upstream's key order.
///
/// [`AppState::default`] is `getDefaultAppState()` in a shipped build at a
/// device pixel ratio of 1. The accessors read the keys the port uses and
/// return `None` when the value has another type.
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    map: Map<String, Value>,
}

impl Default for AppState {
    fn default() -> AppState {
        get_default_app_state(&AppStateEnv::default())
    }
}

impl AppState {
    /// An app state holding exactly `map`.
    pub fn from_map(map: Map<String, Value>) -> AppState {
        AppState { map }
    }

    /// The state as a JSON object.
    pub fn as_map(&self) -> &Map<String, Value> {
        &self.map
    }

    /// The state as a mutable JSON object.
    pub fn as_map_mut(&mut self) -> &mut Map<String, Value> {
        &mut self.map
    }

    /// The JSON object.
    pub fn into_map(self) -> Map<String, Value> {
        self.map
    }

    /// The value of `key`.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.map.get(key)
    }

    /// Set `key`, keeping its position if it exists; returns the old value.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        self.map.insert(key.into(), value)
    }

    /// `gridSize`.
    pub fn grid_size(&self) -> Option<f64> {
        self.get("gridSize").and_then(Value::as_f64)
    }

    /// `gridStep`.
    pub fn grid_step(&self) -> Option<f64> {
        self.get("gridStep").and_then(Value::as_f64)
    }

    /// `gridModeEnabled`.
    pub fn grid_mode_enabled(&self) -> Option<bool> {
        self.get("gridModeEnabled").and_then(Value::as_bool)
    }

    /// `viewBackgroundColor`.
    pub fn view_background_color(&self) -> Option<&str> {
        self.get("viewBackgroundColor").and_then(Value::as_str)
    }

    /// `lockedMultiSelections`: group id to `true`.
    pub fn locked_multi_selections(&self) -> Option<&Map<String, Value>> {
        self.get("lockedMultiSelections").and_then(Value::as_object)
    }

    /// `zoom.value`.
    pub fn zoom(&self) -> Option<f64> {
        self.get("zoom")
            .and_then(|zoom| zoom.get("value"))
            .and_then(Value::as_f64)
    }

    /// `openSidebar` when it is an object (`{name, tab?}`); `None` when it
    /// is `null` (closed).
    pub fn open_sidebar(&self) -> Option<&Map<String, Value>> {
        self.get("openSidebar").and_then(Value::as_object)
    }

    /// `cleanAppStateForExport(appState)`: what a `.excalidraw` file saves.
    pub fn for_export(&self) -> Map<String, Value> {
        clean_app_state_for_export(&self.map)
    }

    /// `clearAppStateForLocalStorage(appState)`.
    pub fn for_local_storage(&self) -> Map<String, Value> {
        clear_app_state_for_local_storage(&self.map)
    }

    /// `clearAppStateForDatabase(appState)`.
    pub fn for_database(&self) -> Map<String, Value> {
        clear_app_state_for_database(&self.map)
    }
}

impl Serialize for AppState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.map.serialize(s)
    }
}

impl<'de> Deserialize<'de> for AppState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<AppState, D::Error> {
        Map::deserialize(d).map(AppState::from_map)
    }
}

fn stroke_width_key_str(key: StrokeWidthKey) -> &'static str {
    match key {
        StrokeWidthKey::Thin => "thin",
        StrokeWidthKey::Medium => "medium",
        StrokeWidthKey::Bold => "bold",
    }
}

fn serde_str<T: Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// `getDefaultAppState()` (`appState.ts:24-147`), in its key order.
pub fn get_default_app_state(env: &AppStateEnv) -> AppState {
    let props = DEFAULT_ELEMENT_PROPS;
    let export_scale = if EXPORT_SCALES.contains(&env.device_pixel_ratio) {
        env.device_pixel_ratio
    } else {
        1.0
    };
    let entries: Vec<(&str, Value)> = vec![
        ("showWelcomeScreen", json!(false)),
        ("theme", json!("light")),
        // new Map(), which JSON.stringify writes as {}.
        ("collaborators", json!({})),
        ("currentItemBackgroundColor", json!(props.background_color)),
        ("currentItemEndArrowhead", json!("arrow")),
        ("currentItemFillStyle", serde_str(props.fill_style)),
        ("currentItemFontFamily", json!(FontFamily::DEFAULT.0)),
        ("currentItemFontSize", number_value(DEFAULT_FONT_SIZE)),
        ("currentItemOpacity", number_value(props.opacity)),
        ("currentItemRoughness", number_value(props.roughness)),
        ("currentItemStrokeVariability", json!("constant")),
        ("currentItemStartArrowhead", Value::Null),
        ("currentItemStrokeColor", json!(props.stroke_color)),
        (
            "currentItemStickynoteStrokeColor",
            json!(props.stroke_color),
        ),
        (
            "currentItemStickynoteBackgroundColor",
            json!(DEFAULT_STICKY_NOTE_BG),
        ),
        (
            "currentItemRoundness",
            json!(if env.test_env { "sharp" } else { "round" }),
        ),
        ("currentItemArrowType", json!("round")),
        ("currentItemStrokeStyle", serde_str(props.stroke_style)),
        (
            "currentItemStrokeWidthKey",
            json!(stroke_width_key_str(DEFAULT_ELEMENT_STROKE_WIDTH_KEY)),
        ),
        ("currentItemTextAlign", serde_str(DEFAULT_TEXT_ALIGN)),
        ("currentHoveredFontFamily", Value::Null),
        ("cursorButton", json!("up")),
        ("activeEmbeddable", Value::Null),
        ("newElement", Value::Null),
        ("editingTextElement", Value::Null),
        ("editingGroupId", Value::Null),
        (
            "activeTool",
            json!({
                "type": "selection",
                "customType": null,
                "locked": props.locked,
                "fromSelection": false,
                "lastActiveTool": null,
            }),
        ),
        (
            "preferredSelectionTool",
            json!({"type": "selection", "initialized": false}),
        ),
        ("penMode", json!(false)),
        ("penDetected", json!(false)),
        ("errorMessage", Value::Null),
        ("exportBackground", json!(true)),
        ("exportScale", number_value(export_scale)),
        ("exportEmbedScene", json!(false)),
        ("exportWithDarkMode", json!(false)),
        ("fileHandle", Value::Null),
        ("gridSize", number_value(DEFAULT_GRID_SIZE)),
        ("gridStep", number_value(DEFAULT_GRID_STEP)),
        ("gridModeEnabled", json!(false)),
        ("isBindingEnabled", json!(true)),
        ("bindingPreference", json!("enabled")),
        ("isMidpointSnappingEnabled", json!(true)),
        ("showHints", json!(true)),
        ("inputDevice", json!("auto")),
        ("defaultSidebarDockedPreference", json!(false)),
        ("isLoading", json!(false)),
        ("isResizing", json!(false)),
        ("isRotating", json!(false)),
        ("lastPointerDownWith", json!("mouse")),
        ("multiElement", Value::Null),
        ("name", Value::Null),
        ("contextMenu", Value::Null),
        ("openMenu", Value::Null),
        ("openPopup", Value::Null),
        ("openSidebar", Value::Null),
        ("openDialog", Value::Null),
        ("previousSelectedElementIds", json!({})),
        ("resizingElement", Value::Null),
        ("scrolledOutside", json!(false)),
        ("scrollX", json!(0)),
        ("scrollY", json!(0)),
        ("scrollConstraints", Value::Null),
        ("selectedElementIds", json!({})),
        ("hoveredElementIds", json!({})),
        ("selectedGroupIds", json!({})),
        ("selectedElementsAreBeingDragged", json!(false)),
        ("selectionElement", Value::Null),
        ("shouldCacheIgnoreZoom", json!(false)),
        (
            "stats",
            json!({
                "open": false,
                "panels": STATS_PANEL_GENERAL_STATS | STATS_PANEL_ELEMENT_PROPERTIES,
            }),
        ),
        ("suggestedBinding", Value::Null),
        ("textToolHover", Value::Null),
        (
            "frameRendering",
            json!({"enabled": true, "clip": true, "name": true, "outline": true}),
        ),
        ("frameToHighlight", Value::Null),
        ("editingFrame", Value::Null),
        ("elementsToHighlight", Value::Null),
        ("toast", Value::Null),
        ("viewBackgroundColor", json!(crate::constants::COLOR_WHITE)),
        ("zenModeEnabled", json!(false)),
        ("zoom", json!({ "value": number_value(DEFAULT_ZOOM) })),
        ("viewModeEnabled", json!(false)),
        ("showHyperlinkPopup", json!(false)),
        ("selectedLinearElement", Value::Null),
        ("snapLines", json!([])),
        ("originSnapOffset", json!({"x": 0, "y": 0})),
        ("objectsSnapModeEnabled", json!(false)),
        ("isCropping", json!(false)),
        ("croppingElementId", Value::Null),
        ("searchMatches", Value::Null),
        ("lockedMultiSelections", json!({})),
        ("activeLockedId", Value::Null),
        ("bindMode", json!("orbit")),
        ("boxSelectionMode", json!("contain")),
        (
            "colorTopPicks",
            json!({
                "elementStroke": null,
                "elementBackground": null,
                "bucketFill": null,
                "stickyNoteStroke": null,
                "stickyNoteBackground": null,
            }),
        ),
        ("fontTopPicks", Value::Null),
    ];
    AppState::from_map(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// restoreAppState

/// The `colorTopPicks` lists, in the order `restoreAppState` writes them
/// (`restore.ts:1301-1317`).
const COLOR_TOP_PICK_LISTS: [&str; 5] = [
    "elementStroke",
    "elementBackground",
    "bucketFill",
    "stickyNoteStroke",
    "stickyNoteBackground",
];

/// `restoreAppState(appState, localAppState)` (`restore.ts:1254-1372`):
/// the state a scene loads with.
///
/// Every key of the defaults is taken from `app_state` (the imported file,
/// already reduced by [`clean_app_state_for_export`] on the file-load
/// path), else from `local` (the editor's current state), else from the
/// default, with `null` counting as a value and unknown keys dropped. Then:
///
/// - legacy `isSidebarDocked` (`data/types.ts:30-33`) is migrated to
///   `defaultSidebarDockedPreference` first, which puts that key first;
/// - `boxSelectionMode` is the imported or local value unless both are
///   nullish;
/// - `colorTopPicks` and `fontTopPicks` are sanitised (strings, deduped by
///   hex value, at most five; picker fonts, at most three);
/// - legacy `currentItemStrokeWidth` becomes `currentItemStrokeWidthKey`;
/// - `cursorButton` comes from `local` (else `"up"`) and `penDetected` from
///   `local`, else from the imported `penMode`/`penDetected`;
/// - `activeTool` falls back to `selection` unless its type is allowed
///   ([`ALLOWED_ACTIVE_TOOLS`]);
/// - a numeric `zoom` becomes `{value}`, clamped to 0.1..=30 and rounded
///   to six places (from the imported state only);
/// - a string `openSidebar` becomes `{name: "default"}`;
/// - `gridSize` and `gridStep` are the imported finite numbers rounded and
///   clamped to 1..=100, else 20 and 5;
/// - transparent sticky-note colours are reset, and `editingFrame` is
///   `null`.
///
/// `None` for `app_state` is upstream's falsy `appState` (so is an array:
/// it has none of the keys read). The environment gives the defaults
/// ([`get_default_app_state`]).
pub fn restore_app_state(
    app_state: Option<&Map<String, Value>>,
    local: Option<&Map<String, Value>>,
    env: &AppStateEnv,
) -> Result<AppState, RestoreAppStateError> {
    let empty = Map::new();
    let supplied = app_state.unwrap_or(&empty);
    let from_local = |key: &str| local.and_then(|l| l.get(key));
    let defaults = get_default_app_state(env).into_map();
    let mut next = Map::new();

    // Legacy keys first (restore.ts:1265-1275). The value is replaced by the
    // loop below; only the key's position survives.
    if supplied.contains_key("isSidebarDocked") {
        let value = match supplied.get("isSidebarDocked") {
            Some(v) if !v.is_null() => v.clone(),
            _ => supplied
                .get("defaultSidebarDockedPreference")
                .or_else(|| defaults.get("defaultSidebarDockedPreference"))
                .cloned()
                .unwrap_or(Value::Null),
        };
        next.insert("defaultSidebarDockedPreference".to_owned(), value);
    }

    // Supplied, then local, then default (restore.ts:1277-1292).
    for (key, default) in &defaults {
        let value = supplied
            .get(key)
            .or_else(|| from_local(key))
            .unwrap_or(default);
        next.insert(key.clone(), value.clone());
    }

    // restore.ts:1294-1298
    let box_selection = match supplied.get("boxSelectionMode") {
        Some(v) if !v.is_null() => Some(v),
        _ => from_local("boxSelectionMode"),
    };
    if let Some(mode) = box_selection {
        next.insert("boxSelectionMode".to_owned(), mode.clone());
    }

    // restore.ts:1300-1318
    let color_top_picks: Map<String, Value> = {
        let picks = next.get("colorTopPicks").and_then(Value::as_object);
        COLOR_TOP_PICK_LISTS
            .iter()
            .map(|name| {
                let list = picks.and_then(|p| p.get(*name));
                ((*name).to_owned(), restore_color_top_picks_list(list))
            })
            .collect()
    };
    next.insert("colorTopPicks".to_owned(), Value::Object(color_top_picks));
    let font_top_picks = restore_font_top_picks(next.get("fontTopPicks"));
    next.insert("fontTopPicks".to_owned(), font_top_picks);

    // restore.ts:1320-1325
    if let Some(width) = supplied.get("currentItemStrokeWidth") {
        let key = stroke_width_key(width).unwrap_or(DEFAULT_ELEMENT_STROKE_WIDTH_KEY);
        next.insert(
            "currentItemStrokeWidthKey".to_owned(),
            json!(stroke_width_key_str(key)),
        );
    }

    // The overrides of restore.ts:1327-1371, in their evaluation order.
    let cursor_button = match from_local("cursorButton") {
        v if truthy(v) => v.cloned().unwrap_or(Value::Null),
        _ => json!("up"),
    };
    let pen_detected = match from_local("penDetected") {
        Some(v) if !v.is_null() => v.clone(),
        _ if truthy(supplied.get("penMode")) => match supplied.get("penDetected") {
            Some(v) if !v.is_null() => v.clone(),
            _ => json!(false),
        },
        _ => json!(false),
    };
    let active_tool = restore_active_tool(next.get("activeTool"), &defaults)?;
    let zoom = json!({ "value": number_value(restored_zoom(supplied.get("zoom"))?) });
    let open_sidebar = match supplied.get("openSidebar") {
        Some(Value::String(_)) => json!({ "name": DEFAULT_SIDEBAR_NAME }),
        _ => next.get("openSidebar").cloned().unwrap_or(Value::Null),
    };
    let grid = |key: &str, default: f64| {
        let value = match supplied.get(key) {
            v if js::is_finite_number(v) => js::as_number(v).unwrap_or(default),
            _ => default,
        };
        // getNormalizedGridSize / getNormalizedGridStep (scene/normalize.ts:11-17)
        number_value(clamp(math::round(value), 1.0, 100.0))
    };
    let grid_size = grid("gridSize", DEFAULT_GRID_SIZE);
    let grid_step = grid("gridStep", DEFAULT_GRID_STEP);
    let sticky_stroke = normalize_sticky_note_color(
        next.get("currentItemStickynoteStrokeColor"),
        DEFAULT_ELEMENT_PROPS.stroke_color,
    )?;
    let sticky_background = normalize_sticky_note_color(
        next.get("currentItemStickynoteBackgroundColor"),
        DEFAULT_STICKY_NOTE_BG,
    )?;

    for (key, value) in [
        ("cursorButton", cursor_button),
        ("penDetected", pen_detected),
        ("activeTool", active_tool),
        ("zoom", zoom),
        ("openSidebar", open_sidebar),
        ("gridSize", grid_size),
        ("gridStep", grid_step),
        ("currentItemStickynoteStrokeColor", sticky_stroke),
        ("currentItemStickynoteBackgroundColor", sticky_background),
        ("editingFrame", Value::Null),
    ] {
        next.insert(key.to_owned(), value);
    }
    Ok(AppState::from_map(next))
}

/// `restoreColorTopPicksList(value)` (`restore.ts:1206-1230`): the strings
/// of an array, deduped by `colorToHex` (else lowercase), first notation
/// kept, at most [`COLOR_TOP_PICKS_SLOTS`]; `null` when none is left.
fn restore_color_top_picks_list(value: Option<&Value>) -> Value {
    let Some(Value::Array(items)) = value else {
        return Value::Null;
    };
    let mut colors: Map<String, Value> = Map::new();
    for item in items {
        let Value::String(color) = item else {
            continue;
        };
        let normalized = color_to_hex(color).unwrap_or_else(|| color.to_lowercase());
        if !colors.contains_key(&normalized) {
            colors.insert(normalized, Value::String(color.clone()));
        }
        if colors.len() >= COLOR_TOP_PICKS_SLOTS {
            break;
        }
    }
    if colors.is_empty() {
        Value::Null
    } else {
        Value::Array(colors.into_iter().map(|(_, v)| v).collect())
    }
}

/// Whether `FONT_METADATA[family]` exists and is neither `private` nor a
/// `fallback` (`packages/common/src/font-metadata.ts:35-135`): the families
/// the font picker lists. `family` is the property key, `String(number)`.
fn is_picker_font(family: &str) -> bool {
    [
        FontFamily::EXCALIFONT,
        FontFamily::NUNITO,
        FontFamily::LILITA_ONE,
        FontFamily::COMIC_SHANNS,
        FontFamily::VIRGIL,
        FontFamily::HELVETICA,
        FontFamily::CASCADIA,
    ]
    .iter()
    .any(|f| f.0.to_string() == family)
}

/// `restoreFontTopPicks(value)` (`restore.ts:1232-1252`): the picker font
/// families of an array, deduped, at most [`FONT_TOP_PICKS_SLOTS`]; `null`
/// when none is left.
fn restore_font_top_picks(value: Option<&Value>) -> Value {
    let Some(Value::Array(items)) = value else {
        return Value::Null;
    };
    let mut families: Vec<f64> = Vec::new();
    for item in items {
        let Some(family) = item.as_f64() else {
            continue;
        };
        if !is_picker_font(&js::number_to_string(family)) {
            continue;
        }
        if !families.contains(&family) {
            families.push(family);
        }
        if families.len() >= FONT_TOP_PICKS_SLOTS {
            break;
        }
    }
    if families.is_empty() {
        Value::Null
    } else {
        Value::Array(families.into_iter().map(number_value).collect())
    }
}

/// `getStrokeWidthKey(strokeWidth)` (`restore.ts:267-271`): the preset whose
/// `STROKE_WIDTH` is exactly the value.
fn stroke_width_key(value: &Value) -> Option<StrokeWidthKey> {
    let width = value.as_f64().filter(|w| w.is_finite())?;
    [
        StrokeWidthKey::Thin,
        StrokeWidthKey::Medium,
        StrokeWidthKey::Bold,
    ]
    .into_iter()
    .find(|key| STROKE_WIDTH.get(*key) == width)
}

/// The restored `activeTool` (`restore.ts:1334-1344`, `updateActiveTool` in
/// `packages/common/src/utils.ts:276-307`).
fn restore_active_tool(
    tool: Option<&Value>,
    defaults: &Map<String, Value>,
) -> Result<Value, TypeError> {
    let default_tool = defaults
        .get("activeTool")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    // nextAppState.activeTool.type: reading a property of null throws;
    // any other non-object has no `type`.
    let fields = match tool {
        Some(Value::Null) | None => {
            return Err(TypeError(
                "Cannot read properties of null (reading 'type')".to_owned(),
            ))
        }
        Some(Value::Object(fields)) => Some(fields),
        Some(_) => None,
    };
    let get = |key: &str| fields.and_then(|f| f.get(key));
    let tool_type = get("type");
    let allowed = truthy(tool_type) && allowed_tool_lookup(&js::to_string(tool_type)?);
    let selection = json!({ "type": "selection" });
    let data = if allowed {
        fields.cloned().unwrap_or_default()
    } else {
        selection.as_object().cloned().unwrap_or_default()
    };

    let mut next = default_tool.clone();
    let default_locked = default_tool.get("locked").cloned().unwrap_or(Value::Null);
    let or_default = |value: Option<&Value>, default: &Value| match value {
        Some(v) if !v.is_null() => v.clone(),
        _ => default.clone(),
    };
    if data.get("type") == Some(&json!("custom")) {
        next.insert("type".to_owned(), json!("custom"));
        match data.get("customType") {
            Some(custom) => {
                next.insert("customType".to_owned(), custom.clone());
            }
            // `customType: undefined`, which JSON.stringify omits.
            None => {
                next.shift_remove("customType");
            }
        }
        next.insert(
            "locked".to_owned(),
            or_default(data.get("locked"), &default_locked),
        );
    } else {
        let last = match data.get("lastActiveTool") {
            Some(v) => v.clone(),
            None => default_tool
                .get("lastActiveTool")
                .cloned()
                .unwrap_or(Value::Null),
        };
        next.insert("lastActiveTool".to_owned(), last);
        next.insert(
            "type".to_owned(),
            data.get("type").cloned().unwrap_or(Value::Null),
        );
        next.insert("customType".to_owned(), Value::Null);
        next.insert(
            "locked".to_owned(),
            or_default(data.get("locked"), &default_locked),
        );
        next.insert(
            "fromSelection".to_owned(),
            or_default(data.get("fromSelection"), &json!(false)),
        );
    }
    next.insert("lastActiveTool".to_owned(), Value::Null);
    next.insert(
        "locked".to_owned(),
        or_default(get("locked"), &json!(false)),
    );
    Ok(Value::Object(next))
}

/// The restored `zoom.value` (`restore.ts:1346-1352`): the imported number,
/// or the imported `zoom.value`, or 1, through `getNormalizedZoom`
/// (`scene/normalize.ts:7-9`): `clamp(round(zoom, 6), MIN_ZOOM, MAX_ZOOM)`,
/// where `round` adds `Number.EPSILON` first (`packages/math/src/utils.ts:7-15`)
/// with JS's `+`, so a string concatenates.
fn restored_zoom(zoom: Option<&Value>) -> Result<f64, TypeError> {
    let value = if js::is_finite_number(zoom) {
        zoom.cloned().unwrap_or(Value::Null)
    } else {
        match zoom {
            Some(Value::Object(fields)) => match fields.get("value") {
                Some(v) if !v.is_null() => v.clone(),
                _ => number_value(DEFAULT_ZOOM),
            },
            _ => number_value(DEFAULT_ZOOM),
        }
    };
    let multiplier = 10f64.powf(6.0);
    let sum = js::add_number(&value, f64::EPSILON)?;
    let rounded = math::round(sum * multiplier) / multiplier;
    Ok(clamp(rounded, MIN_ZOOM, MAX_ZOOM))
}

/// `normalizeStickyNoteStrokeColor` / `normalizeStickyNoteBackgroundColor`
/// (`packages/element/src/stickyNote.ts:67-81`): a falsy or transparent
/// colour becomes `default`.
fn normalize_sticky_note_color(color: Option<&Value>, default: &str) -> Result<Value, TypeError> {
    if !truthy(color) || alpha_of_value(color)? == 0.0 {
        Ok(json!(default))
    } else {
        Ok(color.cloned().unwrap_or(Value::Null))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_keys_are_the_all_true_rows() {
        let exported: Vec<&str> = APP_STATE_STORAGE_CONF
            .iter()
            .filter(|(_, conf)| conf.export)
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(exported, EXPORTED_KEYS);
        for (key, conf) in APP_STATE_STORAGE_CONF {
            assert_eq!(conf.export, conf.server, "{key}");
            if conf.export {
                assert!(conf.browser, "{key}");
            }
        }
    }

    #[test]
    fn prototype_names_are_truthy_lookups() {
        assert!(allowed_tool_lookup("toString"));
        assert!(allowed_tool_lookup("rectangle"));
        assert!(!allowed_tool_lookup("laser"));
        assert!(!allowed_tool_lookup("a,b"));
    }
}
