//! AppState (ex-106): defaults, the keys each storage target keeps, and
//! `restoreAppState`'s legacy handling, against upstream's own output.
//!
//! Upstream: `packages/excalidraw/appState.ts` (`getDefaultAppState`,
//! `APP_STATE_STORAGE_CONF`, `cleanAppStateForExport` and the other
//! cleaners) and `packages/excalidraw/data/restore.ts:1175-1372`
//! (`restoreAppState`), at the pinned commit. See
//! `site/content/research/data-model.md` section 3.2.
//!
//! Fixture: `fixtures/app-state.json`, written by
//! `tools/goldens/app-state.mjs`, which runs those upstream functions from
//! the pinned checkout (CI's `goldens` job fails when it is stale). Restore
//! results are stored as diffs against the first `defaults` entry; the
//! tests rebuild the full expected state and compare it written as
//! `JSON.stringify(value, null, 2)` writes it, so key order counts.
//!
//! The named tests below port upstream's own cases
//! (`tests/data/restore.test.ts` `describe("restoreAppState")`,
//! `tests/colorTopPicks.test.ts`, `tests/fontTopPicks.test.ts`).

use excali_core::app_state::{
    clean_app_state_for_export, clear_app_state_for_database, clear_app_state_for_local_storage,
    clear_app_state_for_storage, get_default_app_state, is_allowed_active_tool, restore_app_state,
    storage_conf, AppState, AppStateEnv, StorageType, ALLOWED_ACTIVE_TOOLS, APP_STATE_STORAGE_CONF,
    EXPORTED_KEYS,
};
use excali_core::document::Document;
use excali_core::json::to_string_pretty;
use serde_json::{json, Map, Value};

const FIXTURE: &str = include_str!("fixtures/app-state.json");
const EVERY_TYPE: &str = include_str!("fixtures/every-type.excalidraw");

fn fixture() -> Map<String, Value> {
    match serde_json::from_str(FIXTURE).expect("app-state.json parses") {
        Value::Object(map) => map,
        _ => panic!("app-state.json is not an object"),
    }
}

fn cases(name: &str) -> Vec<Value> {
    fixture()[name]
        .as_array()
        .expect("an array of cases")
        .clone()
}

fn object(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}

fn written(map: &Map<String, Value>) -> String {
    to_string_pretty(&Value::Object(map.clone()))
}

/// The production, device-pixel-ratio-1 defaults every restore diff is
/// taken against.
fn base_defaults() -> Map<String, Value> {
    let defaults = cases("defaults");
    assert_eq!(defaults[0]["devicePixelRatio"], json!(1));
    assert_eq!(defaults[0]["mode"], json!("production"));
    defaults[0]["appState"].as_object().unwrap().clone()
}

/// The full state a golden diff stands for.
fn apply_diff(base: &Map<String, Value>, diff: &Map<String, Value>) -> Map<String, Value> {
    let changed = diff["changed"].as_object().unwrap();
    let removed: Vec<&str> = diff
        .get("removed")
        .and_then(Value::as_array)
        .map(|keys| keys.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let keys: Vec<String> = match diff.get("keys") {
        Some(Value::Array(keys)) => keys
            .iter()
            .map(|k| k.as_str().unwrap().to_owned())
            .collect(),
        _ => base
            .keys()
            .filter(|k| !removed.contains(&k.as_str()))
            .cloned()
            .collect(),
    };
    keys.into_iter()
        .map(|k| {
            let value = changed
                .get(&k)
                .or_else(|| base.get(&k))
                .unwrap_or_else(|| panic!("no value for {k}"))
                .clone();
            (k, value)
        })
        .collect()
}

fn env() -> AppStateEnv {
    AppStateEnv::default()
}

fn restore(app_state: Value, local: Value) -> AppState {
    restore_app_state(object(&app_state), object(&local), &env()).expect("restores")
}

fn defaults_map() -> Map<String, Value> {
    get_default_app_state(&env()).into_map()
}

// ---------------------------------------------------------------------------
// Defaults

#[test]
fn defaults_match_upstream_in_every_environment() {
    for case in cases("defaults") {
        let env = AppStateEnv {
            device_pixel_ratio: case["devicePixelRatio"].as_f64().unwrap(),
            test_env: case["mode"] == json!("test"),
        };
        let expected = case["appState"].as_object().unwrap();
        assert_eq!(
            written(get_default_app_state(&env).as_map()),
            written(expected),
            "getDefaultAppState() at devicePixelRatio {} in {} mode",
            env.device_pixel_ratio,
            case["mode"]
        );
    }
}

#[test]
fn default_values_from_the_data_model_page() {
    // data-model.md 3.2: viewBackgroundColor "#ffffff", gridSize 20,
    // gridStep 5 (constants.ts:293-294).
    let state = AppState::default();
    assert_eq!(state.view_background_color(), Some("#ffffff"));
    assert_eq!(state.grid_size(), Some(20.0));
    assert_eq!(state.grid_step(), Some(5.0));
    assert_eq!(state.grid_mode_enabled(), Some(false));
    assert_eq!(state.locked_multi_selections(), Some(&Map::new()));
    assert_eq!(state.zoom(), Some(1.0));
    assert_eq!(state.open_sidebar(), None);
    assert_eq!(state.get("currentItemRoundness"), Some(&json!("round")));
    assert_eq!(state.get("exportScale"), Some(&json!(1)));
    // offsetTop, offsetLeft, width and height are not defaults
    // (appState.ts:24-27).
    for key in ["offsetTop", "offsetLeft", "width", "height"] {
        assert_eq!(state.get(key), None, "{key}");
    }
}

#[test]
fn export_scale_is_the_device_pixel_ratio_only_when_it_is_an_export_scale() {
    for (dpr, scale) in [(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (1.5, 1.0), (4.0, 1.0)] {
        let env = AppStateEnv {
            device_pixel_ratio: dpr,
            test_env: false,
        };
        let state = get_default_app_state(&env);
        assert_eq!(
            state.get("exportScale").and_then(Value::as_f64),
            Some(scale),
            "dpr {dpr}"
        );
    }
}

// ---------------------------------------------------------------------------
// Storage

#[test]
fn only_the_five_grid_background_and_lock_keys_are_exported() {
    assert_eq!(
        EXPORTED_KEYS,
        [
            "gridSize",
            "gridStep",
            "gridModeEnabled",
            "viewBackgroundColor",
            "lockedMultiSelections",
        ]
    );
    let mut full = defaults_map();
    for (key, value) in [
        ("offsetTop", 1),
        ("offsetLeft", 2),
        ("width", 3),
        ("height", 4),
    ] {
        full.insert(key.into(), json!(value));
    }
    let exported = clean_app_state_for_export(&full);
    assert_eq!(
        exported.keys().collect::<Vec<_>>(),
        EXPORTED_KEYS.iter().collect::<Vec<_>>()
    );
    assert_eq!(
        Value::Object(exported),
        json!({
            "gridSize": 20,
            "gridStep": 5,
            "gridModeEnabled": false,
            "viewBackgroundColor": "#ffffff",
            "lockedMultiSelections": {},
        })
    );
    // The same five are kept for the server (appState.ts:153-291).
    assert_eq!(
        clear_app_state_for_database(&full)
            .keys()
            .collect::<Vec<_>>(),
        EXPORTED_KEYS.iter().collect::<Vec<_>>()
    );
}

#[test]
fn storage_cleaners_keep_what_upstream_keeps() {
    for case in cases("storage") {
        let id = case["id"].as_str().unwrap();
        let input = case["input"].as_object().unwrap();
        let keys = |m: Map<String, Value>| Value::from(m.keys().cloned().collect::<Vec<_>>());
        assert_eq!(
            keys(clear_app_state_for_local_storage(input)),
            case["browser"],
            "{id} browser"
        );
        assert_eq!(
            keys(clean_app_state_for_export(input)),
            case["export"],
            "{id} export"
        );
        assert_eq!(
            keys(clear_app_state_for_database(input)),
            case["server"],
            "{id} server"
        );
        assert_eq!(
            keys(clear_app_state_for_storage(input, StorageType::Export)),
            case["export"],
            "{id}"
        );
        assert_eq!(
            written(&clean_app_state_for_export(input)),
            written(case["exportValue"].as_object().unwrap()),
            "{id} export values"
        );
    }
}

#[test]
fn storage_conf_covers_every_app_state_key() {
    // The every-key storage case holds each key of APP_STATE_STORAGE_CONF.
    let every = cases("storage")
        .into_iter()
        .find(|c| c["id"] == json!("every-key"))
        .unwrap();
    let input = every["input"].as_object().unwrap();
    let conf_keys: Vec<&str> = APP_STATE_STORAGE_CONF.iter().map(|(k, _)| *k).collect();
    let mut sorted_conf = conf_keys.clone();
    sorted_conf.sort_unstable();
    let mut sorted_input: Vec<&str> = input.keys().map(String::as_str).collect();
    sorted_input.sort_unstable();
    assert_eq!(sorted_conf, sorted_input);
    for (flag, storage) in [
        ("browser", StorageType::Browser),
        ("export", StorageType::Export),
        ("server", StorageType::Server),
    ] {
        let kept: Vec<&str> = every[flag]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for key in &conf_keys {
            let conf = storage_conf(key).unwrap();
            assert_eq!(conf.keeps(storage), kept.contains(key), "{key} {flag}");
        }
    }
    assert_eq!(storage_conf("isSidebarDocked"), None);
    assert_eq!(storage_conf("toString"), None);
}

#[test]
fn a_saved_scene_carries_the_exported_app_state() {
    // every-type.excalidraw is serializeAsJSON(elements, getDefaultAppState(),
    // files, "local") from upstream (tools/goldens/scene-fixtures.mjs).
    let doc = Document::from_json(EVERY_TYPE).unwrap();
    let saved = doc.app_state.as_ref().unwrap();
    assert_eq!(written(saved), written(&AppState::default().for_export()));
}

// ---------------------------------------------------------------------------
// Active tools

#[test]
fn allowed_active_tools_match_upstream() {
    let golden = fixture()["allowedActiveTools"].as_object().unwrap().clone();
    let ours: Map<String, Value> = ALLOWED_ACTIVE_TOOLS
        .iter()
        .map(|(k, v)| ((*k).to_owned(), Value::Bool(*v)))
        .collect();
    assert_eq!(written(&ours), written(&golden));
    assert!(is_allowed_active_tool("rectangle"));
    assert!(!is_allowed_active_tool("eraser"));
    assert!(!is_allowed_active_tool("hexagon"));
}

// ---------------------------------------------------------------------------
// restoreAppState

#[test]
fn restore_matches_upstream_for_every_case() {
    let base = base_defaults();
    let mut failures = Vec::new();
    for case in cases("restore") {
        let id = case["id"].as_str().unwrap();
        let result = restore_app_state(
            object(&case["appState"]),
            object(&case["localAppState"]),
            &env(),
        );
        match (result, case.get("result"), case.get("error")) {
            (Ok(state), Some(diff), None) => {
                let expected = apply_diff(&base, diff.as_object().unwrap());
                if written(state.as_map()) != written(&expected) {
                    failures.push(format!(
                        "{id}:\n  got      {}\n  expected {}",
                        serde_json::to_string(&changed_keys(state.as_map(), &base)).unwrap(),
                        serde_json::to_string(diff).unwrap()
                    ));
                }
            }
            (Err(error), None, Some(message)) => {
                if message.as_str() != Some(error.to_string().as_str()) {
                    failures.push(format!("{id}: error {error}, expected {message}"));
                }
            }
            (Ok(state), None, Some(message)) => failures.push(format!(
                "{id}: expected {message}, got {}",
                serde_json::to_string(&changed_keys(state.as_map(), &base)).unwrap()
            )),
            (Err(error), Some(_), None) => failures.push(format!("{id}: unexpected error {error}")),
            _ => panic!("{id}: a case has either result or error"),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of the restore cases differ from upstream:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The keys of `state` whose value differs from `base`, for failure output.
fn changed_keys(state: &Map<String, Value>, base: &Map<String, Value>) -> Map<String, Value> {
    state
        .iter()
        .filter(|(k, v)| base.get(*k).map(to_string_pretty) != Some(to_string_pretty(v)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[test]
fn restore_with_nothing_gives_the_defaults() {
    let restored = restore_app_state(None, None, &env()).unwrap();
    assert_eq!(written(restored.as_map()), written(&base_defaults()));
    assert_eq!(restored, AppState::default());
}

#[test]
fn numeric_zoom_is_migrated() {
    // restore.test.ts "when imported data state has zoom as a number"
    let mut imported = defaults_map();
    imported.insert("zoom".into(), json!(10));
    let restored = restore(Value::Object(imported), Value::Object(defaults_map()));
    assert_eq!(restored.get("zoom"), Some(&json!({"value": 10})));
    assert_eq!(restored.zoom(), Some(10.0));
    // "when the zoom of imported data state is not a number"
    let restored = restore(json!({"zoom": {"value": 10}}), Value::Null);
    assert_eq!(restored.zoom(), Some(10.0));
    // "when the zoom of imported data state zoom is null"
    let restored = restore(json!({"zoom": null}), Value::Object(defaults_map()));
    assert_eq!(restored.get("zoom"), Some(&json!({"value": 1})));
    // getNormalizedZoom clamps to [MIN_ZOOM, MAX_ZOOM] and rounds to 6 places.
    assert_eq!(
        restore(json!({"zoom": 0.01}), Value::Null).zoom(),
        Some(0.1)
    );
    assert_eq!(restore(json!({"zoom": 99}), Value::Null).zoom(), Some(30.0));
    assert_eq!(
        restore(json!({"zoom": 1.23456789}), Value::Null).zoom(),
        Some(1.234568)
    );
}

#[test]
fn string_open_sidebar_is_migrated() {
    // restore.test.ts "should handle appState.openSidebar legacy values"
    assert_eq!(
        restore(json!({}), Value::Null).get("openSidebar"),
        Some(&Value::Null)
    );
    for legacy in ["library", "xxx", ""] {
        let restored = restore(json!({ "openSidebar": legacy }), Value::Null);
        assert_eq!(
            restored.get("openSidebar"),
            Some(&json!({"name": "default"})),
            "{legacy:?}"
        );
        assert_eq!(
            restored.open_sidebar(),
            json!({"name": "default"}).as_object()
        );
    }
    assert_eq!(
        restore(json!({"openSidebar": {"name": "library"}}), Value::Null).get("openSidebar"),
        Some(&json!({"name": "library"}))
    );
    assert_eq!(
        restore(
            json!({"openSidebar": {"name": "default", "tab": "ola"}}),
            Value::Null
        )
        .get("openSidebar"),
        Some(&json!({"name": "default", "tab": "ola"}))
    );
}

#[test]
fn local_and_imported_values_are_merged_like_upstream() {
    // "when appState is null it should return the local app state property"
    let mut local = defaults_map();
    local.insert("cursorButton".into(), json!("down"));
    local.insert("name".into(), json!("local app state"));
    let restored = restore(Value::Null, Value::Object(local.clone()));
    assert_eq!(restored.get("cursorButton"), Some(&json!("down")));
    assert_eq!(restored.get("name"), Some(&json!("local app state")));

    // "should return imported data when local app state is null": the
    // imported cursorButton is ignored.
    let mut imported = defaults_map();
    imported.insert("cursorButton".into(), json!("down"));
    imported.insert("name".into(), json!("imported app state"));
    let restored = restore(Value::Object(imported.clone()), Value::Null);
    assert_eq!(restored.get("cursorButton"), Some(&json!("up")));
    assert_eq!(restored.get("name"), Some(&json!("imported app state")));

    // "should restore with imported data"
    let mut local_rect = defaults_map();
    local_rect.insert("activeTool".into(), json!({"type": "rectangle", "customType": null, "locked": false, "fromSelection": false, "lastActiveTool": null}));
    local_rect.insert("name".into(), json!("local app state"));
    let restored = restore(Value::Object(imported), Value::Object(local_rect));
    assert_eq!(restored.get("activeTool"), defaults_map().get("activeTool"));
    assert_eq!(restored.get("cursorButton"), Some(&json!("up")));
    assert_eq!(restored.get("name"), Some(&json!("imported app state")));

    // "should restore with current app state when imported data state is
    // undefined"
    let mut imported = defaults_map();
    imported.remove("cursorButton");
    imported.remove("name");
    let restored = restore(Value::Object(imported), Value::Object(local));
    assert_eq!(restored.get("cursorButton"), Some(&json!("down")));
    assert_eq!(restored.get("name"), Some(&json!("local app state")));
}

#[test]
fn legacy_keys_are_migrated() {
    // "should migrate legacy current item stroke width to stroke width key"
    let mut imported = defaults_map();
    imported.remove("currentItemStrokeWidthKey");
    imported.insert("currentItemStrokeWidth".into(), json!(4));
    let restored = restore(Value::Object(imported), Value::Null);
    assert_eq!(
        restored.get("currentItemStrokeWidthKey"),
        Some(&json!("bold"))
    );
    assert_eq!(restored.get("currentItemStrokeWidth"), None);

    // isSidebarDocked (data/types.ts:30-33) names defaultSidebarDockedPreference,
    // which restore.ts:1265-1275 migrates first: it leads the key order.
    let restored = restore(json!({"isSidebarDocked": true}), Value::Null);
    assert_eq!(
        restored.as_map().keys().next().map(String::as_str),
        Some("defaultSidebarDockedPreference")
    );
    assert_eq!(restored.get("isSidebarDocked"), None);
}

#[test]
fn active_tool_is_limited_to_allowed_tools() {
    // "when imported data state has a not allowed Excalidraw Element Types"
    let mut imported = defaults_map();
    imported.insert(
        "activeTool".into(),
        json!("not allowed Excalidraw Element Types"),
    );
    let restored = restore(Value::Object(imported), Value::Object(defaults_map()));
    assert_eq!(
        restored.get("activeTool").unwrap()["type"],
        json!("selection")
    );
    let restored = restore(json!({"activeTool": {"type": "eraser"}}), Value::Null);
    assert_eq!(
        restored.get("activeTool").unwrap()["type"],
        json!("selection")
    );
    let restored = restore(
        json!({"activeTool": {"type": "custom", "customType": "x"}}),
        Value::Null,
    );
    assert_eq!(
        restored.get("activeTool"),
        Some(
            &json!({"type": "custom", "customType": "x", "locked": false, "fromSelection": false, "lastActiveTool": null})
        )
    );
    // A null activeTool throws in upstream (reading `.type`).
    let error =
        restore_app_state(json!({"activeTool": null}).as_object(), None, &env()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "TypeError: Cannot read properties of null (reading 'type')"
    );
}

#[test]
fn sticky_note_colours_and_top_picks_are_sanitised() {
    // restore.test.ts "should restore the sticky note defaults and top-pick slots"
    let restored = restore(
        json!({
            "currentItemStickynoteBackgroundColor": "transparent",
            "currentItemStickynoteStrokeColor": "transparent",
            "colorTopPicks": {"stickyNoteBackground": ["#fcc2d7", "#b2f2bb"]},
        }),
        Value::Null,
    );
    assert_eq!(
        restored.get("currentItemStickynoteBackgroundColor"),
        Some(&json!("#ffdf6b"))
    );
    assert_eq!(
        restored.get("currentItemStickynoteStrokeColor"),
        Some(&json!("#1e1e1e"))
    );
    let picks = restored.get("colorTopPicks").unwrap();
    assert_eq!(picks["stickyNoteBackground"], json!(["#fcc2d7", "#b2f2bb"]));
    assert_eq!(picks["stickyNoteStroke"], Value::Null);
}

#[test]
fn color_top_picks_dedupe_and_cap() {
    // colorTopPicks.test.ts
    let restored = restore(
        json!({"colorTopPicks": {"elementStroke": null, "elementBackground": [
            "#FFF", "#ffffff", "white", "#a5d8ff", "rgb(165, 216, 255)", "#eebefa", 42,
            "transparent", "rgba(0, 0, 0, 0)"
        ]}}),
        Value::Null,
    );
    let picks = restored.get("colorTopPicks").unwrap();
    assert_eq!(
        picks["elementBackground"],
        json!(["#FFF", "#a5d8ff", "#eebefa", "transparent"])
    );
    assert_eq!(picks["elementStroke"], Value::Null);

    let many: Vec<String> = (0..20).map(|i| format!("#0000{i:02}")).collect();
    let restored = restore(
        json!({"colorTopPicks": {"elementStroke": many, "elementBackground": "junk"}}),
        Value::Null,
    );
    let picks = restored.get("colorTopPicks").unwrap();
    assert_eq!(picks["elementStroke"], json!(many[..5]));
    assert_eq!(picks["elementBackground"], Value::Null);
}

#[test]
fn font_top_picks_dedupe_and_cap() {
    // fontTopPicks.test.ts: Lilita One 7, "7", 7, unused 4, private
    // Assistant 10, fallback Xiaolai 100, deprecated Virgil 1.
    let restored = restore(
        json!({"fontTopPicks": [7, "7", 7, 4, 10, 100, 1]}),
        Value::Null,
    );
    assert_eq!(restored.get("fontTopPicks"), Some(&json!([7, 1])));
    let restored = restore(json!({"fontTopPicks": [6, 5, 7, 8, 1]}), Value::Null);
    assert_eq!(restored.get("fontTopPicks"), Some(&json!([6, 5, 7])));
    for junk in [json!("junk"), json!([4, "5"])] {
        let restored = restore(json!({ "fontTopPicks": junk }), Value::Null);
        assert_eq!(restored.get("fontTopPicks"), Some(&Value::Null));
    }
}

#[test]
fn grid_size_and_step_are_normalised_from_the_file_only() {
    let restored = restore(
        json!({"gridSize": 33.5, "gridStep": 0}),
        json!({"gridSize": 50}),
    );
    assert_eq!(restored.grid_size(), Some(34.0));
    assert_eq!(restored.grid_step(), Some(1.0));
    let restored = restore(json!({"gridSize": "30", "gridStep": 150}), Value::Null);
    assert_eq!(restored.grid_size(), Some(20.0));
    assert_eq!(restored.grid_step(), Some(100.0));
}

#[test]
fn app_state_serde_is_the_json_object() {
    let state = AppState::default();
    let text = serde_json::to_string(&state).unwrap();
    let back: AppState = serde_json::from_str(&text).unwrap();
    assert_eq!(back, state);
    assert_eq!(AppState::from_map(state.clone().into_map()), state);
}
