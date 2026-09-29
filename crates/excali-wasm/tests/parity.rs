//! The parity checklist's rows natively (ex-712): each behaviour of
//! `site/content/plan/parity.md` that the element wires, driven through
//! [`Editor`] with the pointer, wheel and keys, and checked against what
//! upstream does at the pin. The browser suite
//! (`tests/web/specs/parity.spec.mjs`) runs the same gestures in Chromium
//! on `<excali-editor>`.
//!
//! The viewport is 1000 × 700 at the page's origin, so client coordinates
//! are scene coordinates at scroll 0 and zoom 1. Text is measured with
//! upstream's test metric (10 px per code unit).

use excali_core::element::Element;
use excali_editor::keyboard::Keystroke;
use excali_editor::viewport::wheel_zoom_value;
use excali_text::text_measurements::CharCountTextMetrics;
use excali_wasm::editor::{Editor, PointerInput, WheelInput};
use excali_wasm::env::EditorEnv;
use serde_json::{json, Value};

type Ed = Editor<CharCountTextMetrics>;

/// A filled 100 × 100 rectangle (upstream hits a transparent one on its
/// outline only).
fn rect(id: &str, x: f64, y: f64) -> Value {
    json!({
        "id": id, "type": "rectangle", "x": x, "y": y, "width": 100, "height": 100,
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "#a5d8ff",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "roundness": null, "seed": 1,
        "version": 1, "versionNonce": 1, "isDeleted": false, "boundElements": null,
        "updated": 1, "link": null, "locked": false,
    })
}

fn editor_with(elements: Vec<Value>) -> Ed {
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", false);
    ed.set_viewport(1000.0, 700.0, 0.0, 0.0);
    let scene = json!({
        "type": "excalidraw", "version": 2, "source": "https://excalidraw.com",
        "elements": elements,
        "appState": { "gridSize": 20, "viewBackgroundColor": "#ffffff" },
        "files": {},
    });
    ed.load(&scene.to_string()).expect("the scene loads");
    ed.take_events();
    ed
}

fn live(ed: &Ed) -> Vec<&Element> {
    ed.elements()
        .iter()
        .filter(|e| !e.base.is_deleted)
        .collect()
}

fn get<'a>(ed: &'a Ed, id: &str) -> &'a Element {
    live(ed)
        .into_iter()
        .find(|e| e.base.id == id)
        .unwrap_or_else(|| panic!("no element {id}"))
}

fn state(ed: &Ed, key: &str) -> Value {
    ed.state()[key].clone()
}

fn zoom(ed: &Ed) -> f64 {
    state(ed, "zoom").as_f64().unwrap()
}

fn key(ed: &mut Ed, stroke: Keystroke) {
    ed.key_down(&stroke);
    ed.key_up(&stroke);
}

fn letter(ed: &mut Ed, k: &str) {
    let code = format!("Key{}", k.to_uppercase());
    key(ed, Keystroke::new(k, &code));
}

fn click(ed: &mut Ed, at: [f64; 2]) {
    ed.pointer_down(PointerInput::at(at[0], at[1]));
    ed.pointer_up(PointerInput::at(at[0], at[1]));
}

fn drag_with(ed: &mut Ed, from: [f64; 2], to: [f64; 2], input: PointerInput) {
    let at = |x: f64, y: f64| PointerInput {
        client_x: x,
        client_y: y,
        ..input
    };
    ed.pointer_down(at(from[0], from[1]));
    for i in 1..=4 {
        let t = f64::from(i) / 4.0;
        ed.pointer_move(at(
            from[0] + (to[0] - from[0]) * t,
            from[1] + (to[1] - from[1]) * t,
        ));
    }
    ed.pointer_up(at(to[0], to[1]));
}

fn drag(ed: &mut Ed, from: [f64; 2], to: [f64; 2]) {
    drag_with(ed, from, to, PointerInput::at(0.0, 0.0));
}

/// Whether a click at `at` selects something; the selection is cleared
/// again.
fn hits(ed: &mut Ed, at: [f64; 2]) -> bool {
    click(ed, at);
    let hit = state(ed, "selectionCount") != json!(0);
    click(ed, [850.0, 300.0]);
    hit
}

// -- Canvas and view ----------------------------------------------------------

#[test]
fn view_zoom_keys() {
    // actionZoomIn: + ZOOM_STEP (0.1) around the centre; actionResetZoom: 1
    let mut ed = editor_with(vec![rect("r", 100.0, 100.0)]);
    key(&mut ed, Keystroke::new("=", "Equal").ctrl());
    assert!((zoom(&ed) - 1.1).abs() < 1e-9, "{}", zoom(&ed));
    key(&mut ed, Keystroke::new("0", "Digit0").ctrl());
    assert_eq!(zoom(&ed), 1.0);
}

#[test]
fn view_wheel_zoom() {
    // AppWheel: ctrl+wheel zooms by zoomBy around the last pointer position
    let mut ed = editor_with(vec![rect("r", 100.0, 100.0)]);
    ed.pointer_move(PointerInput::at(500.0, 350.0));
    ed.wheel(&WheelInput {
        delta_y: -100.0,
        ctrl_key: true,
        ..WheelInput::default()
    });
    assert_eq!(zoom(&ed), wheel_zoom_value(1.0, -100.0, 0.1));
    assert!(zoom(&ed) > 1.0);
    // the point under the pointer stays put
    let app = ed.app_state();
    let (sx, z) = (
        app.get("scrollX").and_then(Value::as_f64).unwrap(),
        zoom(&ed),
    );
    assert!(((500.0 / z - sx) - 500.0).abs() < 1e-9);
}

#[test]
fn view_wheel_scroll() {
    // a plain wheel from a trackpad or an undetected device scrolls
    let mut ed = editor_with(vec![rect("r", 100.0, 100.0)]);
    assert!(!hits(&mut ed, [150.0, 50.0]));
    ed.pointer_move(PointerInput::at(500.0, 350.0));
    ed.wheel(&WheelInput {
        delta_y: 100.0,
        ..WheelInput::default()
    });
    assert_eq!(ed.app_state().get("scrollY"), Some(&json!(-100.0)));
    assert!(hits(&mut ed, [150.0, 50.0]));
}

#[test]
fn view_hand_pan() {
    let mut ed = editor_with(vec![rect("r", 100.0, 100.0)]);
    letter(&mut ed, "h");
    assert_eq!(state(&ed, "activeTool"), "hand");
    drag(&mut ed, [500.0, 500.0], [500.0, 400.0]);
    assert_eq!(ed.app_state().get("scrollY"), Some(&json!(-100.0)));
    letter(&mut ed, "v");
    assert!(hits(&mut ed, [150.0, 50.0]));
}

#[test]
fn view_space_pan() {
    let mut ed = editor_with(vec![rect("r", 100.0, 100.0)]);
    let space = Keystroke::new(" ", "Space");
    ed.key_down(&space);
    drag(&mut ed, [500.0, 500.0], [500.0, 400.0]);
    ed.key_up(&space);
    assert_eq!(ed.app_state().get("scrollY"), Some(&json!(-100.0)));
    // the pan selected nothing and the selection tool is back
    assert_eq!(state(&ed, "activeTool"), "selection");
    assert!(hits(&mut ed, [150.0, 50.0]));
}

#[test]
fn secondary_button_press_does_not_select() {
    let mut ed = editor_with(vec![rect("r", 100.0, 100.0)]);
    let right = PointerInput {
        button: 2,
        ..PointerInput::at(150.0, 150.0)
    };
    ed.pointer_down(right);
    ed.pointer_up(right);
    assert_eq!(state(&ed, "selectionCount"), 0);
    assert_eq!(get(&ed, "r").base.x, 100.0);
}

// -- Selection and transforms -------------------------------------------------

fn three() -> Ed {
    editor_with(vec![
        rect("a", 100.0, 100.0),
        rect("b", 300.0, 100.0),
        rect("c", 600.0, 400.0),
    ])
}

fn selected(ed: &Ed) -> Vec<String> {
    let mut ids: Vec<String> = ed
        .app_state()
        .get("selectedElementIds")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter(|(_, v)| v.as_bool() == Some(true))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

#[test]
fn select_box() {
    // a drag from empty canvas: getElementsWithinSelection in "contain" mode
    let mut ed = three();
    drag(&mut ed, [50.0, 50.0], [450.0, 250.0]);
    assert_eq!(selected(&ed), ["a", "b"]);
    // the box itself is gone and nothing moved
    assert!(ed
        .app_state()
        .get("selectionElement")
        .is_none_or(Value::is_null));
    assert_eq!(get(&ed, "a").base.x, 100.0);
    // a new box without Shift replaces the selection; with Shift adds to it
    drag(&mut ed, [550.0, 350.0], [750.0, 550.0]);
    assert_eq!(selected(&ed), ["c"]);
    drag_with(
        &mut ed,
        [50.0, 50.0],
        [250.0, 250.0],
        PointerInput::at(0.0, 0.0).shift(),
    );
    assert_eq!(selected(&ed), ["a", "c"]);
}

#[test]
fn select_box_takes_whole_groups() {
    let mut g1 = rect("g1", 100.0, 100.0);
    let mut g2 = rect("g2", 300.0, 100.0);
    g1["groupIds"] = json!(["grp"]);
    g2["groupIds"] = json!(["grp"]);
    let mut ed = editor_with(vec![g1, g2]);
    // a box around one member of a group selects nothing in "contain" mode
    drag(&mut ed, [50.0, 50.0], [250.0, 250.0]);
    assert!(selected(&ed).is_empty());
    drag(&mut ed, [50.0, 50.0], [450.0, 250.0]);
    assert_eq!(selected(&ed), ["g1", "g2"]);
    assert_eq!(
        ed.app_state().get("selectedGroupIds"),
        Some(&json!({ "grp": true }))
    );
}

#[test]
fn select_all() {
    // actionSelectAll: every element but bound text and deleted ones
    let mut ed = three();
    key(&mut ed, Keystroke::new("a", "KeyA").ctrl());
    assert_eq!(selected(&ed), ["a", "b", "c"]);
    assert_eq!(state(&ed, "selectionCount"), 3);
}

#[test]
fn resize_handle() {
    // getTransformHandlesFromCoords at zoom 1 for the mouse: the south-east
    // handle of (100, 100)–(200, 200) spans (202, 202)–(210, 210)
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0)]);
    click(&mut ed, [150.0, 150.0]);
    drag(&mut ed, [206.0, 206.0], [256.0, 236.0]);
    let a = get(&ed, "a");
    assert_eq!(
        (a.base.x, a.base.y, a.base.width, a.base.height),
        (100.0, 100.0, 150.0, 130.0)
    );
    // one undo step
    key(&mut ed, Keystroke::new("z", "KeyZ").ctrl());
    assert_eq!(get(&ed, "a").base.width, 100.0);
}

#[test]
fn rotate_handle() {
    // the rotation handle spans (146, 74)–(154, 82)
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0)]);
    click(&mut ed, [150.0, 150.0]);
    drag(&mut ed, [150.0, 78.0], [300.0, 150.0]);
    let a = get(&ed, "a");
    // rotateSingleElement: the angle from the centre plus 90 degrees, as
    // upstream computes it for this gesture (excali-editor's transform.json,
    // case parity-rotate-handle)
    assert_eq!(a.base.angle.0, std::f64::consts::FRAC_PI_2);
    assert_eq!((a.base.width, a.base.height), (100.0, 100.0));
}
