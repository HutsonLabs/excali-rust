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
use excali_editor::tools::{SetActiveToolOptions, Tool, ToolRequest, ToolType};
use excali_editor::viewport::wheel_zoom_value;
use excali_text::text_measurements::CharCountTextMetrics;
use excali_ui::text_editor::{TextareaEvent, TextareaKey};
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

// -- Tools --------------------------------------------------------------------

/// Draws with the tool of `k` from `from` to `to`; the one new element.
fn draw(ed: &mut Ed, k: &str, from: [f64; 2], to: [f64; 2]) -> Element {
    let before: Vec<String> = live(ed).iter().map(|e| e.base.id.clone()).collect();
    letter(ed, k);
    drag(ed, from, to);
    let created: Vec<Element> = live(ed)
        .into_iter()
        .filter(|e| !before.contains(&e.base.id))
        .cloned()
        .collect();
    assert_eq!(created.len(), 1, "{k} created {created:?}");
    created.into_iter().next().unwrap()
}

fn json_of(e: &Element) -> Value {
    Value::Object(e.to_map())
}

#[test]
fn tool_rectangle() {
    let mut ed = editor_with(vec![]);
    let e = draw(&mut ed, "r", [100.0, 100.0], [250.0, 200.0]);
    let j = json_of(&e);
    for (k, v) in [
        ("type", json!("rectangle")),
        ("x", json!(100.0)),
        ("y", json!(100.0)),
        ("width", json!(150.0)),
        ("height", json!(100.0)),
        ("angle", json!(0.0)),
        ("strokeColor", json!("#1e1e1e")),
        ("backgroundColor", json!("transparent")),
        ("fillStyle", json!("solid")),
        ("strokeWidth", json!(2.0)),
        ("strokeStyle", json!("solid")),
        ("roughness", json!(1.0)),
        ("opacity", json!(100.0)),
        ("roundness", json!({ "type": 3 })),
        ("groupIds", json!([])),
        ("frameId", Value::Null),
        ("locked", json!(false)),
    ] {
        assert_eq!(j[k], v, "{k}");
    }
    assert_eq!(state(&ed, "activeTool"), "selection");
    assert_eq!(selected(&ed), std::slice::from_ref(&e.base.id));
    // one undo step removes it
    key(&mut ed, Keystroke::new("z", "KeyZ").ctrl());
    assert!(live(&ed).is_empty());
}

#[test]
fn tool_diamond_and_ellipse() {
    let mut ed = editor_with(vec![]);
    let d = draw(&mut ed, "d", [100.0, 100.0], [250.0, 200.0]);
    assert_eq!(json_of(&d)["roundness"], json!({ "type": 2 }));
    assert_eq!((d.base.width, d.base.height), (150.0, 100.0));
    let o = draw(&mut ed, "o", [300.0, 100.0], [450.0, 200.0]);
    // getCurrentItemRoundness makes no exception for the ellipse
    assert_eq!(json_of(&o)["roundness"], json!({ "type": 2 }));
    assert_eq!((o.base.x, o.base.y, o.base.width), (300.0, 100.0, 150.0));
}

#[test]
fn tool_arrow_and_line() {
    let mut ed = editor_with(vec![]);
    let a = json_of(&draw(&mut ed, "a", [100.0, 100.0], [250.0, 200.0]));
    assert_eq!(a["type"], "arrow");
    assert_eq!(
        (a["x"].clone(), a["y"].clone()),
        (json!(100.0), json!(100.0))
    );
    assert_eq!(a["points"], json!([[0.0, 0.0], [150.0, 100.0]]));
    assert_eq!(a["startArrowhead"], Value::Null);
    assert_eq!(a["endArrowhead"], "arrow");
    let l = json_of(&draw(&mut ed, "l", [100.0, 300.0], [250.0, 400.0]));
    assert_eq!(l["type"], "line");
    assert_eq!(l["points"], json!([[0.0, 0.0], [150.0, 100.0]]));
    assert_eq!(l["endArrowhead"], Value::Null);
}

#[test]
fn tool_freedraw() {
    let mut ed = editor_with(vec![]);
    let f = json_of(&draw(&mut ed, "p", [100.0, 100.0], [250.0, 200.0]));
    assert_eq!(f["type"], "freedraw");
    assert_eq!(
        (f["x"].clone(), f["y"].clone()),
        (json!(100.0), json!(100.0))
    );
    let points = f["points"].as_array().unwrap();
    assert!(points.len() > 2);
    assert_eq!(points.last().unwrap(), &json!([150.0, 100.0]));
    // the freedraw tool stays
    assert_eq!(state(&ed, "activeTool"), "freedraw");
}

#[test]
fn tool_frame() {
    let mut ed = editor_with(vec![]);
    let f = json_of(&draw(&mut ed, "f", [100.0, 100.0], [400.0, 300.0]));
    assert_eq!(f["type"], "frame");
    assert_eq!(
        [&f["x"], &f["y"], &f["width"], &f["height"]],
        [&json!(100.0), &json!(100.0), &json!(300.0), &json!(200.0)]
    );
    assert_eq!(f["name"], Value::Null);
}

#[test]
fn tool_lock() {
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "q");
    draw(&mut ed, "r", [100.0, 100.0], [200.0, 200.0]);
    assert_eq!(state(&ed, "activeTool"), "rectangle");
    drag(&mut ed, [300.0, 100.0], [400.0, 200.0]);
    let rects = live(&ed)
        .iter()
        .filter(|e| json_of(e)["type"] == "rectangle")
        .count();
    assert_eq!(rects, 2);
}

#[test]
fn a_click_with_a_shape_tool_draws_nothing() {
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "r");
    click(&mut ed, [100.0, 100.0]);
    assert!(live(&ed).is_empty());
}

#[test]
fn tool_eraser() {
    let mut ed = editor_with(vec![rect("r", 100.0, 100.0), rect("keep", 400.0, 100.0)]);
    letter(&mut ed, "e");
    drag(&mut ed, [80.0, 150.0], [220.0, 150.0]);
    let ids: Vec<String> = live(&ed).iter().map(|e| e.base.id.clone()).collect();
    assert_eq!(ids, ["keep"]);
    assert_eq!(state(&ed, "activeTool"), "eraser");
    key(&mut ed, Keystroke::new("z", "KeyZ").ctrl());
    assert_eq!(live(&ed).len(), 2);
}

// -- Lasso ----------------------------------------------------------------------

fn lasso_tool(ed: &mut Ed) {
    ed.tools_mut()
        .set_active_tool(
            ToolRequest::new(Tool::Builtin(ToolType::Lasso)),
            SetActiveToolOptions::default(),
        )
        .expect("the lasso");
}

/// A lasso drawn through `points` (client coordinates), then released.
fn lasso(ed: &mut Ed, points: &[[f64; 2]], input: PointerInput) {
    let at = |[x, y]: [f64; 2]| PointerInput {
        client_x: x,
        client_y: y,
        ..input
    };
    ed.pointer_down(at(points[0]));
    for &p in &points[1..] {
        ed.pointer_move(at(p));
    }
    ed.pointer_up(at(points[points.len() - 1]));
}

/// lasso.test.tsx:1875-1926: a lasso enclosing one rectangle and cutting
/// through another selects the enclosed one in the default `contain`
/// mode, and both in `overlap` mode.
#[test]
fn tool_lasso_contain_and_overlap() {
    let path = [
        [80.0, 80.0],
        [320.0, 80.0],
        [320.0, 150.0],
        [420.0, 150.0],
        [420.0, 220.0],
        [80.0, 220.0],
        [80.0, 80.0],
    ];
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0), rect("b", 350.0, 100.0)]);
    lasso_tool(&mut ed);
    lasso(&mut ed, &path, PointerInput::at(0.0, 0.0));
    assert_eq!(selected(&ed), ["a"]);
    assert_eq!(state(&ed, "activeTool"), "lasso");

    let mut ed = editor_with(vec![rect("a", 100.0, 100.0), rect("b", 350.0, 100.0)]);
    ed.set_app_state(
        json!({ "boxSelectionMode": "overlap" })
            .as_object()
            .unwrap()
            .clone(),
    );
    lasso_tool(&mut ed);
    lasso(&mut ed, &path, PointerInput::at(0.0, 0.0));
    assert_eq!(selected(&ed), ["a", "b"]);
}

/// A new lasso replaces the selection; with Shift it adds to it
/// (`startPath(x, y, event.shiftKey)`, App.tsx:8990-8996).
#[test]
fn tool_lasso_shift_keeps_the_selection() {
    let around = |x: f64| {
        [
            [x - 20.0, 80.0],
            [x + 120.0, 80.0],
            [x + 120.0, 220.0],
            [x - 20.0, 220.0],
            [x - 20.0, 80.0],
        ]
    };
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0), rect("b", 400.0, 100.0)]);
    lasso_tool(&mut ed);
    lasso(&mut ed, &around(100.0), PointerInput::at(0.0, 0.0));
    assert_eq!(selected(&ed), ["a"]);
    lasso(&mut ed, &around(400.0), PointerInput::at(0.0, 0.0));
    assert_eq!(selected(&ed), ["b"]);
    lasso(&mut ed, &around(100.0), PointerInput::at(0.0, 0.0).shift());
    assert_eq!(selected(&ed), ["a", "b"]);
    // a lasso around nothing clears the selection
    lasso(&mut ed, &around(700.0), PointerInput::at(0.0, 0.0));
    assert!(selected(&ed).is_empty());
}

/// A press on a selected element drags the selection instead of starting
/// a lasso (App.tsx:8975-8990).
#[test]
fn tool_lasso_drags_the_selection() {
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0)]);
    lasso_tool(&mut ed);
    lasso(
        &mut ed,
        &[
            [80.0, 80.0],
            [220.0, 80.0],
            [220.0, 220.0],
            [80.0, 220.0],
            [80.0, 80.0],
        ],
        PointerInput::at(0.0, 0.0),
    );
    assert_eq!(selected(&ed), ["a"]);
    drag(&mut ed, [150.0, 150.0], [250.0, 170.0]);
    assert_eq!(get(&ed, "a").base.x, 200.0);
    assert_eq!(get(&ed, "a").base.y, 120.0);
    assert_eq!(selected(&ed), ["a"]);
}

// -- Bound text and arrows ----------------------------------------------------

#[test]
fn bound_arrow_create() {
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0), rect("b", 400.0, 100.0)]);
    let arrow = json_of(&draw(&mut ed, "a", [150.0, 150.0], [450.0, 150.0]));
    assert_eq!(arrow["startBinding"]["elementId"], "a");
    assert_eq!(arrow["endBinding"]["elementId"], "b");
    // the shapes list the arrow
    let bound = |id: &str| json_of(get(&ed, id))["boundElements"].clone();
    let arrow_id = arrow["id"].clone();
    assert_eq!(bound("a"), json!([{ "id": arrow_id, "type": "arrow" }]));
    assert_eq!(bound("b"), json!([{ "id": arrow_id, "type": "arrow" }]));
}

// -- Text ---------------------------------------------------------------------

fn text_el(id: &str, x: f64, y: f64, value: &str) -> Value {
    json!({
        "id": id, "type": "text", "x": x, "y": y, "width": 50, "height": 25,
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "roundness": null, "seed": 1,
        "version": 1, "versionNonce": 1, "isDeleted": false, "boundElements": null,
        "updated": 1, "link": null, "locked": false, "text": value, "originalText": value,
        "fontSize": 20, "fontFamily": 5, "textAlign": "left", "verticalAlign": "top",
        "containerId": null, "autoResize": true, "lineHeight": 1.25,
    })
}

fn escape() -> TextareaKey {
    TextareaKey {
        key: "Escape".into(),
        code: "Escape".into(),
        shift_key: false,
        alt_key: false,
        ctrl_or_cmd: false,
        is_composing: false,
        key_code: 27,
    }
}

fn double_click(ed: &mut Ed, at: [f64; 2]) {
    click(ed, at);
    click(ed, at);
    ed.double_click(PointerInput::at(at[0], at[1]));
}

#[test]
fn tool_text() {
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "t");
    click(&mut ed, [300.0, 300.0]);
    let area = ed.textarea().expect("the text editor opens");
    assert!(area.open);
    // the tool reverts at once (AppTextTool.finish)
    assert_eq!(state(&ed, "activeTool"), "selection");
    ed.textarea_event(TextareaEvent::Input {
        value: "hi".into(),
        selection: (2, 2),
    });
    ed.textarea_event(TextareaEvent::KeyDown {
        key: escape(),
        selection: (2, 2),
    });
    assert!(ed.textarea().is_none());
    let texts: Vec<Value> = live(&ed)
        .iter()
        .map(|e| json_of(e))
        .filter(|e| e["type"] == "text")
        .collect();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0]["text"], "hi");
    assert_eq!(texts[0]["fontSize"], json!(20.0));
    assert_eq!(texts[0]["fontFamily"], json!(5));
    // typed and submitted: one undo step removes it
    key(&mut ed, Keystroke::new("z", "KeyZ").ctrl());
    assert!(live(&ed).is_empty());
}

#[test]
fn text_tool_empty_submit_leaves_nothing() {
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "t");
    click(&mut ed, [300.0, 300.0]);
    ed.textarea_event(TextareaEvent::Submit);
    assert!(ed.textarea().is_none());
    assert!(live(&ed).is_empty());
}

#[test]
fn text_dblclick_edit() {
    let mut ed = editor_with(vec![text_el("t", 100.0, 100.0, "hello")]);
    double_click(&mut ed, [110.0, 110.0]);
    let area = ed.textarea().expect("the text editor opens");
    assert_eq!(area.value, "hello");
    let attributes = ed.textarea_attributes().expect("an open editor");
    assert_eq!((attributes.dir, attributes.wrap), ("auto", "off"));
    ed.textarea_event(TextareaEvent::Input {
        value: "hello world".into(),
        selection: (11, 11),
    });
    ed.textarea_event(TextareaEvent::Submit);
    assert_eq!(json_of(get(&ed, "t"))["text"], "hello world");
}

#[test]
fn text_dblclick_label() {
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0)]);
    double_click(&mut ed, [150.0, 150.0]);
    assert!(ed.textarea().is_some_and(|a| a.open));
    ed.textarea_event(TextareaEvent::Input {
        value: "label".into(),
        selection: (5, 5),
    });
    ed.textarea_event(TextareaEvent::KeyDown {
        key: escape(),
        selection: (5, 5),
    });
    let label = live(&ed)
        .into_iter()
        .find(|e| json_of(e)["type"] == "text")
        .map(json_of)
        .expect("a label");
    assert_eq!(label["containerId"], "a");
    assert_eq!(label["text"], "label");
    assert_eq!(
        json_of(get(&ed, "a"))["boundElements"],
        json!([{ "id": label["id"], "type": "text" }])
    );
    // the keyboard submit selects the container
    assert_eq!(selected(&ed), ["a"]);
}

// -- Editing ------------------------------------------------------------------

fn ids(ed: &Ed) -> Vec<String> {
    live(ed).iter().map(|e| e.base.id.clone()).collect()
}

#[test]
fn edit_delete() {
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0), rect("b", 300.0, 100.0)]);
    click(&mut ed, [150.0, 150.0]);
    key(&mut ed, Keystroke::new("Delete", "Delete"));
    assert_eq!(ids(&ed), ["b"]);
    assert!(selected(&ed).is_empty());
    key(&mut ed, Keystroke::new("z", "KeyZ").ctrl());
    assert_eq!(ids(&ed), ["a", "b"]);
    // Backspace deletes too
    click(&mut ed, [350.0, 150.0]);
    key(&mut ed, Keystroke::new("Backspace", "Backspace"));
    assert_eq!(ids(&ed), ["a"]);
}

#[test]
fn edit_duplicate() {
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0)]);
    click(&mut ed, [150.0, 150.0]);
    key(&mut ed, Keystroke::new("d", "KeyD").ctrl());
    let all = live(&ed);
    assert_eq!(all.len(), 2);
    let copy = all.iter().find(|e| e.base.id != "a").unwrap();
    let j = json_of(copy);
    assert_eq!(j["type"], "rectangle");
    assert_eq!(
        [&j["x"], &j["y"], &j["width"], &j["height"]],
        [&json!(110.0), &json!(110.0), &json!(100.0), &json!(100.0)]
    );
    // the copy is selected, above the original
    assert_eq!(selected(&ed), std::slice::from_ref(&copy.base.id));
    assert_eq!(ids(&ed)[1], copy.base.id);
}

#[test]
fn edit_group() {
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0), rect("b", 300.0, 100.0)]);
    key(&mut ed, Keystroke::new("a", "KeyA").ctrl());
    key(&mut ed, Keystroke::new("g", "KeyG").ctrl());
    let (a, b) = (get(&ed, "a"), get(&ed, "b"));
    assert_eq!(a.base.group_ids.len(), 1);
    assert_eq!(a.base.group_ids, b.base.group_ids);
    // Ctrl+Shift+G ungroups
    key(&mut ed, Keystroke::new("G", "KeyG").ctrl().shift());
    assert!(get(&ed, "a").base.group_ids.is_empty());
}

#[test]
fn edit_zorder() {
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0), rect("b", 300.0, 100.0)]);
    click(&mut ed, [150.0, 150.0]);
    key(&mut ed, Keystroke::new("}", "BracketRight").ctrl().shift());
    assert_eq!(ids(&ed), ["b", "a"]);
    // the indices follow the order
    let (a, b) = (get(&ed, "a"), get(&ed, "b"));
    assert!(b.base.index < a.base.index);
    key(&mut ed, Keystroke::new("{", "BracketLeft").ctrl().shift());
    assert_eq!(ids(&ed), ["a", "b"]);
}

#[test]
fn edit_zorder_on_a_mac() {
    // Cmd+Alt+] passes actionBringToFront's and actionBringForward's key
    // tests: the ActionManager cancels an ambiguous key (manager.tsx:114-119)
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", true);
    ed.set_viewport(1000.0, 700.0, 0.0, 0.0);
    let scene = json!({
        "type": "excalidraw", "version": 2, "source": "", "files": {}, "appState": {},
        "elements": [rect("a", 100.0, 100.0), rect("b", 300.0, 100.0)],
    });
    ed.load(&scene.to_string()).unwrap();
    click(&mut ed, [150.0, 150.0]);
    key(&mut ed, Keystroke::new("]", "BracketRight").meta().alt());
    assert_eq!(ids(&ed), ["a", "b"]);
    // Cmd+] brings it forward, past b
    key(&mut ed, Keystroke::new("]", "BracketRight").meta());
    assert_eq!(ids(&ed), ["b", "a"]);
}

#[test]
fn edit_copy_paste() {
    let mut ed = editor_with(vec![rect("a", 100.0, 100.0)]);
    click(&mut ed, [150.0, 150.0]);
    let text = ed.copy().expect("the selection is copied");
    let data: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(data["type"], "excalidraw/clipboard");
    assert_eq!(data["elements"][0]["id"], "a");
    ed.pointer_move(PointerInput::at(500.0, 400.0));
    ed.paste(&text, false);
    let all = live(&ed);
    assert_eq!(all.len(), 2);
    let copy = (*all.iter().find(|e| e.base.id != "a").unwrap()).clone();
    // centred on the pointer
    assert_eq!((copy.base.x + 50.0, copy.base.y + 50.0), (500.0, 400.0));
    assert_eq!(selected(&ed), std::slice::from_ref(&copy.base.id));
    // cut: copied, then deleted
    let cut = ed.cut().expect("the selection is cut");
    assert!(cut.contains(&copy.base.id));
    assert_eq!(ids(&ed), ["a"]);
}
