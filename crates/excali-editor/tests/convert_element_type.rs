//! Element type conversion against upstream's own
//! `packages/excalidraw/components/ConvertElementTypePopup.tsx`
//! (`convertElementTypes`, `getConversionTypeFromElements`,
//! `convertLineToElbow` and the Panel's cache priming), recorded by
//! `tools/goldens/convert-element-type-fixtures.mjs` in
//! `fixtures/convert-element-type.json`.

mod support;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_editor::convert_element_type::{
    convert_line_to_elbow, ConvertElementTypePopup, ConvertibleType,
};
use excali_editor::keyboard::{get_conversion_type, ConversionType, ConvertDirection};
use excali_editor::scene::Scene;
use excali_editor::tools::{ActiveTool, Tool, ToolType};
use serde_json::{json, Map, Value};
use support::TestEnv;

const FIXTURE: &str = include_str!("fixtures/convert-element-type.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("convert-element-type.json parses")
}

fn element(value: &Value) -> Element {
    Element::from_map(value.as_object().expect("an element").clone())
        .expect("an element upstream built")
}

fn elements(value: &Value) -> Vec<Element> {
    value
        .as_array()
        .expect("elements")
        .iter()
        .map(element)
        .collect()
}

fn conversion(value: &Value) -> Option<ConversionType> {
    match value.as_str() {
        Some("generic") => Some(ConversionType::Generic),
        Some("linear") => Some(ConversionType::Linear),
        None => None,
        Some(other) => panic!("conversion type {other}"),
    }
}

fn points(value: &Value) -> Vec<[f64; 2]> {
    value
        .as_array()
        .expect("points")
        .iter()
        .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
        .collect()
}

/// The element's JSON with the drawn `versionNonce` left out (the port's
/// test environment draws its own).
fn comparable(e: &Element) -> Map<String, Value> {
    let mut map = e.to_map();
    map.remove("versionNonce");
    map
}

#[test]
fn conversion_type_matches_upstream() {
    let f = fixture();
    let cases = f["conversionType"].as_array().unwrap();
    assert!(cases.len() > 20);
    for case in cases {
        let els = elements(&case["elements"]);
        let refs: Vec<&Element> = els.iter().collect();
        assert_eq!(
            get_conversion_type(&refs),
            conversion(&case["result"]),
            "{}",
            case["id"]
        );
    }
}

#[test]
fn line_to_elbow_matches_upstream() {
    let f = fixture();
    let cases = f["lineToElbow"].as_array().unwrap();
    assert!(cases.len() > 60);
    for case in cases {
        assert_eq!(
            convert_line_to_elbow(&points(&case["points"])),
            points(&case["result"]),
            "{}",
            case["id"]
        );
    }
}

fn app_state_of(case: &Value) -> AppState {
    let a = &case["appState"];
    let mut state = AppState::default();
    state.insert(
        "currentItemStartArrowhead",
        a["currentItemStartArrowhead"].clone(),
    );
    state.insert(
        "currentItemEndArrowhead",
        a["currentItemEndArrowhead"].clone(),
    );
    state.insert("zoom", json!({ "value": a["zoom"] }));
    let ids: Map<String, Value> = case["selected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| (id.as_str().unwrap().to_owned(), Value::Bool(true)))
        .collect();
    state.insert("selectedElementIds", Value::Object(ids));
    state
}

fn tool_of(case: &Value) -> ActiveTool {
    let name = case["appState"]["activeTool"].as_str().unwrap();
    ActiveTool {
        tool: Tool::Builtin(ToolType::from_name(name).unwrap()),
        ..ActiveTool::default()
    }
}

fn next_type(value: &Value) -> Option<ConvertibleType> {
    value
        .as_str()
        .map(|s| ConvertibleType::from_name(s).unwrap_or_else(|| panic!("type {s}")))
}

fn direction(value: &Value) -> ConvertDirection {
    match value.as_str() {
        Some("left") => ConvertDirection::Left,
        _ => ConvertDirection::Right,
    }
}

#[test]
fn sessions_match_upstream() {
    let f = fixture();
    let sessions = f["sessions"].as_array().unwrap();
    assert!(sessions.len() > 40);
    let mut steps_run = 0;
    for case in sessions {
        let id = case["id"].as_str().unwrap();
        let mut scene = Scene::new(elements(&case["elements"]));
        let mut app_state = app_state_of(case);
        let mut tool = tool_of(case);
        let mut popup = ConvertElementTypePopup::default();
        let mut env = TestEnv::with_layout();
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            popup.prime(&scene, &app_state);
            let conv = conversion(&step["conversionType"]);
            let converted = popup.convert(
                &mut scene,
                &mut app_state,
                &mut tool,
                &mut env,
                conv,
                next_type(&step["nextType"]),
                direction(&step["direction"]),
            );
            let at = format!("{id} step {i}");
            assert_eq!(converted, step["result"].as_bool().unwrap(), "{at}");
            let expected = elements(&step["elements"]);
            assert_eq!(scene.elements().len(), expected.len(), "{at}");
            for (got, want) in scene.elements().iter().zip(&expected) {
                assert_eq!(
                    comparable(got),
                    comparable(want),
                    "{at}: element {}",
                    want.base.id
                );
            }
            if converted {
                assert_eq!(
                    app_state.get("selectedElementIds"),
                    Some(&step["selectedElementIds"]),
                    "{at}"
                );
                let linear = app_state
                    .get("selectedLinearElement")
                    .and_then(|l| l.get("elementId"))
                    .cloned()
                    .unwrap_or(Value::Null);
                assert_eq!(linear, step["selectedLinearElement"], "{at}");
                assert_eq!(tool.to_json(), step["activeTool"], "{at}");
                assert_eq!(
                    app_state.get("activeTool"),
                    Some(&step["activeTool"]),
                    "{at}"
                );
            }
            steps_run += 1;
        }
    }
    assert!(steps_run > 150);
}

// -- the popup ------------------------------------------------------------------

fn rect(id: &str, ty: &str) -> Element {
    element(&Value::Object(base(id, ty)))
}

fn base(id: &str, ty: &str) -> Map<String, Value> {
    let Value::Object(map) = json!({
        "id": id, "type": ty, "x": 0, "y": 0, "width": 100, "height": 50,
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid",
        "roughness": 1, "opacity": 100, "groupIds": [], "frameId": null,
        "index": "a0", "roundness": null, "seed": 1, "version": 1,
        "versionNonce": 0, "isDeleted": false, "boundElements": null,
        "updated": 1, "link": null, "locked": false
    }) else {
        unreachable!()
    };
    map
}

fn line(id: &str) -> Element {
    let mut map = base(id, "line");
    map.insert("points".into(), json!([[0, 0], [100, 50]]));
    map.insert("startBinding".into(), Value::Null);
    map.insert("endBinding".into(), Value::Null);
    map.insert("startArrowhead".into(), Value::Null);
    map.insert("endArrowhead".into(), Value::Null);
    map.insert("polygon".into(), json!(false));
    Element::from_map(map).unwrap()
}

fn text(id: &str) -> Element {
    let mut map = base(id, "text");
    for (k, v) in [
        ("text", json!("t")),
        ("fontSize", json!(20)),
        ("fontFamily", json!(5)),
        ("textAlign", json!("left")),
        ("verticalAlign", json!("top")),
        ("containerId", Value::Null),
        ("originalText", json!("t")),
        ("autoResize", json!(true)),
        ("lineHeight", json!(1.25)),
    ] {
        map.insert(k.into(), v);
    }
    Element::from_map(map).unwrap()
}

fn select(app_state: &mut AppState, ids: &[&str]) {
    let ids: Map<String, Value> = ids
        .iter()
        .map(|id| ((*id).to_owned(), Value::Bool(true)))
        .collect();
    app_state.insert("selectedElementIds", Value::Object(ids));
}

/// `ConvertElementTypePopup`'s effect (`ConvertElementTypePopup.tsx:
/// 157-176`): the popup closes on an empty selection and when the
/// selection's conversion type changes from the one it opened with.
#[test]
fn popup_closes_when_the_selection_changes_kind() {
    let scene = Scene::new(vec![rect("r", "rectangle"), line("l"), text("t")]);
    let mut app_state = AppState::default();
    let mut popup = ConvertElementTypePopup::default();

    select(&mut app_state, &["r"]);
    let mut open = true;
    popup.sync(&mut open, &scene, &app_state);
    assert!(open);
    assert_eq!(popup.category(), Some(ConversionType::Generic));

    // still generic: stays open
    select(&mut app_state, &["r", "l"]);
    popup.sync(&mut open, &scene, &app_state);
    assert!(open);

    // linear now: closes
    select(&mut app_state, &["l"]);
    popup.sync(&mut open, &scene, &app_state);
    assert!(!open);
    assert_eq!(popup.category(), None);

    // nothing convertible: closes
    let mut open = true;
    popup.sync(&mut open, &scene, &app_state);
    assert!(open);
    select(&mut app_state, &["t"]);
    popup.sync(&mut open, &scene, &app_state);
    assert!(!open);

    // empty selection: closes
    let mut open = true;
    select(&mut app_state, &[]);
    popup.sync(&mut open, &scene, &app_state);
    assert!(!open);
}

/// The Panel (`ConvertElementTypePopup.tsx:257-331`): the shapes of the
/// selection's kind, the one they all are checked, placed below the
/// bottom-left corner of the selection.
#[test]
fn panel_lists_the_shapes_of_the_kind() {
    let mut r = rect("r", "diamond");
    r.base.x = 40.0;
    r.base.y = 30.0;
    let mut e = rect("e", "ellipse");
    e.base.x = 10.0;
    e.base.y = 100.0;
    let scene = Scene::new(vec![r, e, line("l")]);
    let mut app_state = AppState::default();
    app_state.insert("scrollX", json!(5));
    app_state.insert("scrollY", json!(-10));
    app_state.insert("zoom", json!({ "value": 2 }));
    app_state.insert("offsetLeft", json!(7));
    app_state.insert("offsetTop", json!(3));

    select(&mut app_state, &["r"]);
    let panel = ConvertElementTypePopup::panel(&scene, &app_state).unwrap();
    let names: Vec<&str> = panel.shapes.iter().map(|s| s.kind.name()).collect();
    assert_eq!(names, ["rectangle", "diamond", "ellipse"]);
    let checked: Vec<bool> = panel.shapes.iter().map(|s| s.checked).collect();
    assert_eq!(checked, [false, true, false]);
    // bottom-left (40, 80) in the viewport: ((40 + 5) * 2 + 7, (80 - 10) * 2 + 3)
    // then top = y + (10 + 8) * zoom - offsetTop, left = x - offsetLeft - 8
    assert_eq!(panel.left, 97.0 - 7.0 - 8.0);
    assert_eq!(panel.top, 143.0 + 36.0 - 3.0);

    // mixed types: nothing checked; the common box's bottom-left
    select(&mut app_state, &["r", "e"]);
    let panel = ConvertElementTypePopup::panel(&scene, &app_state).unwrap();
    assert!(panel.shapes.iter().all(|s| !s.checked));
    assert_eq!(panel.left, (10.0 + 5.0) * 2.0 + 7.0 - 7.0 - 8.0);
    assert_eq!(panel.top, (150.0 - 10.0) * 2.0 + 3.0 + 36.0 - 3.0);

    select(&mut app_state, &["l"]);
    let panel = ConvertElementTypePopup::panel(&scene, &app_state).unwrap();
    let names: Vec<&str> = panel.shapes.iter().map(|s| s.kind.name()).collect();
    assert_eq!(names, ["line", "sharpArrow", "curvedArrow", "elbowArrow"]);
    assert!(panel.shapes[0].checked);

    select(&mut app_state, &[]);
    assert!(ConvertElementTypePopup::panel(&scene, &app_state).is_none());
}

/// Clicking a shape in the panel converts to it; the checked one is a
/// no-op (`onSelect`, `ConvertElementTypePopup.tsx:307-324`).
#[test]
fn selecting_the_checked_shape_is_a_no_op() {
    let mut scene = Scene::new(vec![rect("r", "rectangle")]);
    let mut app_state = AppState::default();
    select(&mut app_state, &["r"]);
    let mut tool = ActiveTool::default();
    let mut popup = ConvertElementTypePopup::default();
    let mut env = TestEnv::with_layout();
    assert!(!popup.select(
        &mut scene,
        &mut app_state,
        &mut tool,
        &mut env,
        ConvertibleType::Rectangle
    ));
    assert_eq!(scene.elements()[0].base.version, 1.0);
    assert!(popup.select(
        &mut scene,
        &mut app_state,
        &mut tool,
        &mut env,
        ConvertibleType::Ellipse
    ));
    assert_eq!(scene.elements()[0].element_type().as_str(), "ellipse");
}
