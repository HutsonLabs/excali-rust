//! The styles panel's actions in the editor (ex-540): `perform_style_action`
//! writes an action's result back as one history entry, and the aligns,
//! distributes, font size steps and polygon toggle run from
//! `perform_action` and their keys.

use excali_core::element::Element;
use excali_editor::actions::ActionName;
use excali_editor::keyboard::Keystroke;
use excali_text::text_measurements::CharCountTextMetrics;
use excali_wasm::editor::Editor;
use excali_wasm::env::EditorEnv;
use serde_json::{json, Map, Value};

fn rectangle(id: &str, x: f64, y: f64) -> Value {
    json!({
        "id": id, "type": "rectangle", "x": x, "y": y, "width": 100, "height": 100,
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "index": null, "roundness": null,
        "seed": 1, "version": 1, "versionNonce": 0, "isDeleted": false,
        "boundElements": null, "updated": 1, "link": null, "locked": false
    })
}

fn editor(elements: Vec<Value>) -> Editor<CharCountTextMetrics> {
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", false);
    ed.set_viewport(1000.0, 700.0, 0.0, 0.0);
    let scene = json!({
        "type": "excalidraw", "version": 2, "source": "https://excalidraw.com",
        "elements": elements, "appState": {}, "files": {}
    });
    ed.load(&scene.to_string()).expect("the scene loads");
    ed.take_events();
    ed
}

fn select(ed: &mut Editor<CharCountTextMetrics>, ids: &[&str]) {
    let selected: Map<String, Value> = ids
        .iter()
        .map(|id| ((*id).to_owned(), json!(true)))
        .collect();
    let mut patch = Map::new();
    patch.insert("selectedElementIds".into(), Value::Object(selected));
    ed.set_app_state(patch);
}

fn get<'a>(ed: &'a Editor<CharCountTextMetrics>, id: &str) -> &'a Element {
    ed.elements()
        .iter()
        .find(|e| e.base.id == id)
        .unwrap_or_else(|| panic!("no element {id}"))
}

#[test]
fn a_stroke_colour_pick_is_one_undo_entry() {
    let mut ed = editor(vec![
        rectangle("a", 100.0, 100.0),
        rectangle("b", 300.0, 100.0),
    ]);
    select(&mut ed, &["a"]);
    assert!(!ed.can_undo());
    ed.perform_style_action(
        ActionName::ChangeStrokeColor,
        &json!({ "color": "#e03131" }),
    );
    assert_eq!(get(&ed, "a").base.stroke_color, "#e03131");
    assert_eq!(get(&ed, "b").base.stroke_color, "#1e1e1e");
    assert_eq!(
        ed.app_state().get("currentItemStrokeColor"),
        Some(&json!("#e03131"))
    );
    assert!(ed.can_undo());
    ed.undo();
    assert_eq!(get(&ed, "a").base.stroke_color, "#1e1e1e");
    assert!(!ed.can_undo(), "one entry");
    ed.redo();
    assert_eq!(get(&ed, "a").base.stroke_color, "#e03131");
}

#[test]
fn a_colour_without_a_pick_sets_the_current_item_only() {
    let mut ed = editor(vec![rectangle("a", 100.0, 100.0)]);
    select(&mut ed, &["a"]);
    ed.perform_style_action(
        ActionName::ChangeStrokeColor,
        &json!({ "currentItemStrokeColor": "#2f9e44" }),
    );
    assert_eq!(get(&ed, "a").base.stroke_color, "#1e1e1e");
    assert_eq!(
        ed.app_state().get("currentItemStrokeColor"),
        Some(&json!("#2f9e44"))
    );
    assert!(!ed.can_undo(), "not captured");
}

#[test]
fn align_left_from_perform_action_and_its_key() {
    let mut ed = editor(vec![
        rectangle("a", 100.0, 100.0),
        rectangle("b", 300.0, 250.0),
    ]);
    select(&mut ed, &["a", "b"]);
    ed.perform_action(ActionName::AlignLeft);
    assert_eq!(get(&ed, "b").base.x, 100.0);
    assert!(ed.can_undo());
    ed.undo();
    assert_eq!(get(&ed, "b").base.x, 300.0);
    // CtrlOrCmd+Shift+Up aligns the tops (of a selection the undo left)
    select(&mut ed, &["a", "b"]);
    ed.key_down(&Keystroke::new("ArrowUp", "ArrowUp").ctrl().shift());
    assert_eq!(get(&ed, "b").base.y, 100.0);
}

#[test]
fn distribute_horizontally_from_perform_action() {
    let mut ed = editor(vec![
        rectangle("a", 0.0, 0.0),
        rectangle("b", 150.0, 0.0),
        rectangle("c", 500.0, 0.0),
    ]);
    select(&mut ed, &["a", "b", "c"]);
    ed.perform_action(ActionName::DistributeHorizontally);
    assert_eq!(get(&ed, "b").base.x, 250.0);
    ed.undo();
    assert_eq!(get(&ed, "b").base.x, 150.0);
}

#[test]
fn ctrl_shift_period_increases_the_font_size() {
    let mut text = rectangle("t", 100.0, 100.0);
    let fields = json!({
        "type": "text", "width": 50, "height": 25, "text": "hello", "originalText": "hello",
        "fontSize": 20, "fontFamily": 5, "textAlign": "left", "verticalAlign": "top",
        "containerId": null, "autoResize": true, "lineHeight": 1.25
    });
    for (k, v) in fields.as_object().unwrap() {
        text[k] = v.clone();
    }
    let mut ed = editor(vec![text]);
    select(&mut ed, &["t"]);
    ed.key_down(&Keystroke::new(".", "Period").ctrl().shift());
    let size = match &get(&ed, "t").kind {
        excali_core::element::ElementKind::Text(t) => t.font_size,
        _ => panic!("a text"),
    };
    assert_eq!(size, 22.0);
    assert_eq!(
        ed.app_state()
            .get("currentItemFontSize")
            .and_then(Value::as_f64),
        Some(22.0)
    );
    assert!(ed.can_undo());
}

/// The eye dropper previews on the selection while held (LayerUI's
/// `onChange`, `LayerUI.tsx:522-567`, `mutateElement` without a capture)
/// and the release picks through the colour action: one entry, whose undo
/// goes back to the colour before the preview.
#[test]
fn an_eye_dropper_preview_and_pick_are_one_undo_entry() {
    let mut ed = editor(vec![rectangle("a", 100.0, 100.0)]);
    select(&mut ed, &["a"]);
    assert!(!ed.can_undo());
    let version = get(&ed, "a").base.version;
    ed.preview_color(
        excali_editor::actions::ColorProperty::StrokeColor,
        "#ffc9c9",
    );
    assert_eq!(get(&ed, "a").base.stroke_color, "#ffc9c9");
    assert_eq!(get(&ed, "a").base.version, version + 1.0);
    assert!(!ed.can_undo(), "a preview is not captured");
    ed.preview_color(
        excali_editor::actions::ColorProperty::StrokeColor,
        "#b2f2bb",
    );
    ed.perform_style_action(
        ActionName::ChangeStrokeColor,
        &json!({ "color": "#b2f2bb" }),
    );
    assert_eq!(get(&ed, "a").base.stroke_color, "#b2f2bb");
    assert!(ed.can_undo());
    ed.undo();
    assert_eq!(get(&ed, "a").base.stroke_color, "#1e1e1e");
    assert!(!ed.can_undo(), "one entry");
}

/// With nothing selected the preview writes the colour target's default
/// (`getColorTargetAppStateUpdates`).
#[test]
fn an_eye_dropper_preview_without_a_selection_sets_the_default() {
    let mut ed = editor(vec![rectangle("a", 100.0, 100.0)]);
    ed.preview_color(
        excali_editor::actions::ColorProperty::BackgroundColor,
        "#ffec99",
    );
    assert_eq!(
        ed.app_state().get("currentItemBackgroundColor"),
        Some(&json!("#ffec99"))
    );
    assert_eq!(get(&ed, "a").base.background_color, "transparent");
}

/// A press on the canvas closes the open popup
/// (`App.handleCanvasPointerDown`, `App.tsx:8772-8774`).
#[test]
fn a_canvas_press_closes_the_open_popup() {
    let mut ed = editor(vec![rectangle("a", 100.0, 100.0)]);
    let mut patch = Map::new();
    patch.insert("openPopup".into(), json!("elementStroke"));
    ed.set_app_state(patch);
    ed.pointer_down(excali_wasm::editor::PointerInput {
        client_x: 800.0,
        client_y: 600.0,
        ..Default::default()
    });
    assert_eq!(ed.app_state().get("openPopup"), Some(&Value::Null));
}
