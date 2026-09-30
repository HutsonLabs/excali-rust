//! Find on canvas behind `<excali-editor>` (ex-708): Ctrl/Cmd+F runs
//! `actionToggleSearchMenu` (`actions/actionToggleSearchMenu.ts`), which
//! opens the default sidebar on its search tab, or asks the host to focus
//! the search field when that tab is already open; the search menu reads
//! the scene, the elements in the viewport (`app.visibleElements`) and the
//! view through [`Editor::with_search_context`], and navigates to a match
//! through [`Editor::fit_bounds`]. The menu itself is excali-ui's
//! `search_menu`, held to upstream's by its own tests.

use excali_editor::keyboard::Keystroke;
use excali_editor::viewport::{Fit, Offsets};
use excali_text::text_measurements::CharCountTextMetrics;
use excali_ui::search_menu::{handle_search, MatchKind};
use excali_wasm::editor::Editor;
use excali_wasm::env::EditorEnv;
use serde_json::{json, Map, Value};

const SCENE: &str = include_str!("fixtures/bound.excalidraw");

fn editor(darwin: bool) -> Editor<CharCountTextMetrics> {
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", darwin);
    ed.set_viewport(1000.0, 700.0, 0.0, 0.0);
    ed.load(SCENE).expect("the fixture loads");
    ed.take_events();
    ed
}

fn open_sidebar(ed: &Editor<CharCountTextMetrics>) -> Value {
    ed.app_state().get("openSidebar").cloned().unwrap_or(Value::Null)
}

#[test]
fn ctrl_f_opens_the_search_tab() {
    let mut ed = editor(false);
    let out = ed.key_down(&Keystroke::new("f", "KeyF").ctrl());
    assert!(out.prevent_default, "the browser's find is not opened");
    assert_eq!(open_sidebar(&ed), json!({ "name": "default", "tab": "search" }));
    assert!(!ed.take_search_focus_request());

    // again while the search tab is open: the field is focused
    ed.key_down(&Keystroke::new("f", "KeyF").ctrl());
    assert_eq!(open_sidebar(&ed), json!({ "name": "default", "tab": "search" }));
    assert!(ed.take_search_focus_request());
    assert!(!ed.take_search_focus_request());
}

#[test]
fn cmd_f_opens_it_on_apple_platforms() {
    let mut ed = editor(true);
    ed.key_down(&Keystroke::new("f", "KeyF").meta());
    assert_eq!(open_sidebar(&ed), json!({ "name": "default", "tab": "search" }));
}

#[test]
fn the_library_tab_switches_to_search() {
    let mut ed = editor(false);
    let mut patch = Map::new();
    patch.insert("openSidebar".into(), json!({ "name": "default", "tab": "library" }));
    ed.set_app_state(patch);
    ed.key_down(&Keystroke::new("f", "KeyF").ctrl());
    assert_eq!(open_sidebar(&ed), json!({ "name": "default", "tab": "search" }));
}

#[test]
fn an_open_dialog_keeps_the_sidebar_closed() {
    let mut ed = editor(false);
    let mut patch = Map::new();
    patch.insert("openDialog".into(), json!({ "name": "help" }));
    ed.set_app_state(patch);
    ed.key_down(&Keystroke::new("f", "KeyF").ctrl());
    assert_eq!(open_sidebar(&ed), Value::Null);
    assert!(!ed.take_search_focus_request());
}

#[test]
fn the_search_reads_the_scene_and_the_visible_elements() {
    let mut ed = editor(false);
    let (items, focus) = ed.with_search_context(Offsets::default(), |cx| handle_search("LAB", cx));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].element_id, "label");
    assert_eq!(items[0].kind, MatchKind::Text);
    assert_eq!(focus, Some(0), "the label is in the viewport");

    // scrolled far away: nothing is visible
    let mut patch = Map::new();
    patch.insert("scrollX".into(), json!(-10000.0));
    ed.set_app_state(patch);
    let (_, focus) = ed.with_search_context(Offsets::default(), |cx| handle_search("label", cx));
    assert_eq!(focus, Some(-1));
}

#[test]
fn a_match_is_fitted_into_the_viewport() {
    let mut ed = editor(false);
    let offsets = Offsets {
        top: 24.0,
        right: 326.0,
        bottom: 24.0,
        left: 24.0,
    };
    ed.fit_bounds([5000.0, 5000.0, 5050.0, 5025.0], Fit::ScaleDown, offsets);
    let app = ed.app_state();
    let zoom = app.zoom().unwrap();
    let scroll_x = app.get("scrollX").and_then(Value::as_f64).unwrap();
    let scroll_y = app.get("scrollY").and_then(Value::as_f64).unwrap();
    assert_eq!(zoom, 1.0, "scale-down never zooms in");
    // the target's centre is the centre of the viewport less the offsets
    let cx = (5025.0 + scroll_x) * zoom;
    let cy = (5012.5 + scroll_y) * zoom;
    assert!((cx - (24.0 + (1000.0 - 24.0 - 326.0) / 2.0)).abs() < 1e-9, "{cx}");
    assert!((cy - (24.0 + (700.0 - 48.0) / 2.0)).abs() < 1e-9, "{cy}");

    ed.fit_bounds([0.0, 0.0, 10.0, 5.0], Fit::Contain, Offsets::default());
    assert_eq!(ed.app_state().zoom().unwrap(), 30.0, "contain zooms in up to the maximum");
}
