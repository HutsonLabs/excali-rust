//! Find on canvas behind `<excali-editor>` (ex-708): Ctrl/Cmd+F runs
//! `actionToggleSearchMenu` (`actions/actionToggleSearchMenu.ts`), which
//! opens the default sidebar on its search tab, or asks the host to focus
//! the search field when that tab is already open; the search menu reads
//! the scene, the elements in the viewport (`app.visibleElements`) and the
//! view through [`Editor::with_search_context`], and navigates to a match
//! through [`Editor::navigate_to`], animated over 300 ms as
//! `app.viewport.setViewport` animates it (ex-714); the matches are drawn
//! on the interactive canvas. The menu itself is excali-ui's
//! `search_menu`, held to upstream's by its own tests, and the transition
//! and the drawing are held to upstream's by excali-editor's goldens.

use excali_editor::keyboard::Keystroke;
use excali_editor::viewport::{Fit, Offsets, SetViewportOptions, ViewportAnimation};
use excali_scene::display::{Color, DisplayItem};
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
    ed.app_state()
        .get("openSidebar")
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn ctrl_f_opens_the_search_tab() {
    let mut ed = editor(false);
    let out = ed.key_down(&Keystroke::new("f", "KeyF").ctrl());
    assert!(out.prevent_default, "the browser's find is not opened");
    assert_eq!(
        open_sidebar(&ed),
        json!({ "name": "default", "tab": "search" })
    );
    assert!(!ed.take_search_focus_request());

    // again while the search tab is open: the field is focused
    ed.key_down(&Keystroke::new("f", "KeyF").ctrl());
    assert_eq!(
        open_sidebar(&ed),
        json!({ "name": "default", "tab": "search" })
    );
    assert!(ed.take_search_focus_request());
    assert!(!ed.take_search_focus_request());
}

#[test]
fn cmd_f_opens_it_on_apple_platforms() {
    let mut ed = editor(true);
    ed.key_down(&Keystroke::new("f", "KeyF").meta());
    assert_eq!(
        open_sidebar(&ed),
        json!({ "name": "default", "tab": "search" })
    );
}

#[test]
fn the_library_tab_switches_to_search() {
    let mut ed = editor(false);
    let mut patch = Map::new();
    patch.insert(
        "openSidebar".into(),
        json!({ "name": "default", "tab": "library" }),
    );
    ed.set_app_state(patch);
    ed.key_down(&Keystroke::new("f", "KeyF").ctrl());
    assert_eq!(
        open_sidebar(&ed),
        json!({ "name": "default", "tab": "search" })
    );
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

fn viewport(ed: &Editor<CharCountTextMetrics>) -> (f64, f64, f64) {
    let app = ed.app_state();
    (
        app.get("scrollX").and_then(Value::as_f64).unwrap(),
        app.get("scrollY").and_then(Value::as_f64).unwrap(),
        app.zoom().unwrap(),
    )
}

fn navigate(ed: &mut Editor<CharCountTextMetrics>, target: [f64; 4], fit: Fit, offsets: Offsets) {
    ed.navigate_to(SetViewportOptions {
        fit,
        offsets: Some(offsets),
        animation: ViewportAnimation::Duration(300.0),
        ..SetViewportOptions::new(target)
    });
}

#[test]
fn a_match_is_fitted_into_the_viewport_over_300_ms() {
    let mut ed = editor(false);
    let offsets = Offsets {
        top: 24.0,
        right: 326.0,
        bottom: 24.0,
        left: 24.0,
    };
    let start = viewport(&ed);
    navigate(
        &mut ed,
        [5000.0, 5000.0, 5050.0, 5025.0],
        Fit::ScaleDown,
        offsets,
    );
    // the first frame is where the view was, drawn from zoom-scaled bitmaps
    assert!(ed.is_viewport_animating());
    assert_eq!(viewport(&ed), start);
    assert_eq!(
        ed.app_state().get("shouldCacheIgnoreZoom"),
        Some(&json!(true))
    );
    ed.viewport_frame(1000.0);
    ed.viewport_frame(1150.0);
    let mid = viewport(&ed);
    assert!(ed.is_viewport_animating());
    assert!(mid.0 < start.0 && mid.1 < start.1, "{mid:?}");
    ed.viewport_frame(1300.0);
    assert!(!ed.is_viewport_animating());
    assert_eq!(
        ed.app_state().get("shouldCacheIgnoreZoom"),
        Some(&json!(false))
    );
    let (scroll_x, scroll_y, zoom) = viewport(&ed);
    assert_eq!(zoom, 1.0, "scale-down never zooms in");
    // the target's centre is the centre of the viewport less the offsets
    let cx = (5025.0 + scroll_x) * zoom;
    let cy = (5012.5 + scroll_y) * zoom;
    assert!(
        (cx - (24.0 + (1000.0 - 24.0 - 326.0) / 2.0)).abs() < 1e-9,
        "{cx}"
    );
    assert!((cy - (24.0 + (700.0 - 48.0) / 2.0)).abs() < 1e-9, "{cy}");

    navigate(
        &mut ed,
        [0.0, 0.0, 10.0, 5.0],
        Fit::Contain,
        Offsets::default(),
    );
    ed.viewport_frame(2000.0);
    ed.viewport_frame(2400.0);
    assert_eq!(
        ed.app_state().zoom().unwrap(),
        30.0,
        "contain zooms in up to the maximum"
    );
}

#[test]
fn a_pan_takes_over_from_the_navigation() {
    let mut ed = editor(false);
    navigate(
        &mut ed,
        [5000.0, 5000.0, 5050.0, 5025.0],
        Fit::ScaleDown,
        Offsets::default(),
    );
    ed.viewport_frame(1000.0);
    ed.viewport_frame(1100.0);
    let before = viewport(&ed);
    ed.wheel(&excali_wasm::editor::WheelInput {
        delta_y: 40.0,
        ..Default::default()
    });
    assert!(!ed.is_viewport_animating());
    assert_eq!(
        ed.app_state().get("shouldCacheIgnoreZoom"),
        Some(&json!(false))
    );
    let after = viewport(&ed);
    assert_eq!(after.1, before.1 - 40.0);
    ed.viewport_frame(1200.0);
    assert_eq!(viewport(&ed), after, "the navigation stopped");
}

/// The fills of `color` in a display list.
fn fills(items: &[DisplayItem], color: &str) -> usize {
    items
        .iter()
        .map(|item| match item {
            DisplayItem::FillRect { color: c, .. } => usize::from(*c == Color::new(color)),
            DisplayItem::Group(g) => fills(&g.items, color),
            _ => 0,
        })
        .sum()
}

#[test]
fn the_matches_are_drawn_on_the_interactive_canvas() {
    let mut ed = editor(false);
    let (items, _) = ed.with_search_context(Offsets::default(), |cx| handle_search("label", cx));
    assert_eq!(items.len(), 1);
    let scene =
        |ed: &Editor<CharCountTextMetrics>| ed.interactive_scene(1000.0, 700.0, 1.0, "#6965db");
    assert_eq!(fills(&scene(&ed).items, "rgba(255, 124, 0, 0.4)"), 0);
    let mut patch = Map::new();
    patch.insert(
        "searchMatches".into(),
        json!({
            "focusedId": "label",
            "matches": [{
                "id": "label",
                "focus": true,
                "matchedLines": [{ "offsetX": 0, "offsetY": 0, "width": 50, "height": 25, "showOnCanvas": true }],
            }],
        }),
    );
    ed.set_app_state(patch);
    assert_eq!(fills(&scene(&ed).items, "rgba(255, 124, 0, 0.4)"), 1);
}
