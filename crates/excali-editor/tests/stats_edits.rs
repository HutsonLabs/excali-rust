//! The stats panel's element property edits against upstream
//! (`tests/fixtures/stats_edits.json`, `tools/goldens/stats-edits.mjs`):
//! upstream's Stats over one scene, a value typed and committed or a label
//! dragged, and every element that changed, the scene order and what the
//! callbacks passed to `setAppState`.

mod support;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_core::fractional_index::SceneElementsMap;
use excali_core::restore::RestoreEnv;
use excali_editor::binding::update_bound_elements_in_map;
use excali_editor::resize_elements::{
    sticky_note_layout, StickyNoteLayout, StickyNoteLayoutOpts, TransformEnv,
};
use excali_editor::scene::{MutationEnv, Scene};
use excali_editor::stats::{
    get_step_sized_value, HighlightPatch, StatsChange, StatsDrag, StatsEnv, StatsGesture,
    StatsProperty,
};
use excali_text::text_measurements::{CharWidthCache, TextMetricsProvider};
use serde_json::{json, Value};
use support::TestEnv;

const FIXTURE: &str = include_str!("fixtures/stats_edits.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("stats_edits.json parses")
}

impl RestoreEnv for TestEnv {
    fn now(&mut self) -> f64 {
        1.0
    }

    fn random_id(&mut self) -> String {
        self.ids += 1;
        format!("id-{}", self.ids)
    }

    fn random_integer(&mut self) -> f64 {
        MutationEnv::random_integer(self)
    }
}

impl TransformEnv for TestEnv {
    fn sticky_note_layout(
        &mut self,
        container: &Element,
        text: Option<&Element>,
        opts: &StickyNoteLayoutOpts,
    ) -> StickyNoteLayout {
        self.layouter
            .with_layout(|layout, _| sticky_note_layout(layout, container, text, opts))
    }
}

/// The port's arrow layout for the arrows a label's layout moves, with
/// upstream's test metric.
struct Arrows<'a> {
    stamp: &'a mut dyn excali_core::fractional_index::ChangeStamp,
    char_widths: &'a mut CharWidthCache,
}

impl MutationEnv for Arrows<'_> {
    fn random_integer(&mut self) -> f64 {
        self.stamp.version_nonce()
    }
    fn now(&mut self) -> f64 {
        self.stamp.updated()
    }
}

impl excali_editor::binding::BindingEnv for Arrows<'_> {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (
            &excali_text::text_measurements::CharCountTextMetrics,
            self.char_widths,
        )
    }
}

impl StatsEnv for TestEnv {
    fn redraw_text_bounding_box(
        &mut self,
        scene: &mut Scene,
        text_id: &str,
        container_id: Option<&str>,
    ) {
        let mut map: SceneElementsMap = scene
            .elements()
            .iter()
            .map(|e| (e.base.id.clone(), e.clone()))
            .collect();
        let mut stamp = Stamp(&mut self.nonce);
        let char_widths = &mut self.char_widths;
        self.layouter
            .redraw_text_bounding_box(
                &mut stamp,
                &mut map,
                text_id,
                container_id,
                &mut |stamp, elements, id| {
                    let mut env = Arrows {
                        stamp,
                        char_widths: &mut *char_widths,
                    };
                    update_bound_elements_in_map(elements, id, &SceneElementsMap::new(), &mut env);
                    Ok(())
                },
            )
            .expect("redrawTextBoundingBox");
        *scene = Scene::new(map.into_values().collect());
    }
}

struct Stamp<'a>(&'a mut f64);

impl excali_core::fractional_index::ChangeStamp for Stamp<'_> {
    fn version_nonce(&mut self) -> f64 {
        *self.0 += 1.0;
        1000.0 + *self.0
    }
    fn updated(&mut self) -> f64 {
        1.0
    }
}

fn property(label: &str) -> StatsProperty {
    match label {
        "X" => StatsProperty::X,
        "Y" => StatsProperty::Y,
        "W" => StatsProperty::Width,
        "H" => StatsProperty::Height,
        "A" => StatsProperty::Angle,
        "F" => StatsProperty::FontSize,
        other => panic!("input {other}"),
    }
}

/// `Number(text)` rounded as `Number(parsed.toFixed(2))`.
fn typed(text: &str) -> f64 {
    let parsed: f64 = text.parse().expect("a number");
    excali_scene::display::to_fixed(parsed, 2)
}

/// Upstream's element and the port's, key for key; `versionNonce` and
/// `updated` come from upstream's random generator and clock.
fn same_json(actual: &Value, expected: &Value, path: &str) -> Result<(), String> {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (a.as_f64().expect("f64"), e.as_f64().expect("f64"));
            if a == e {
                Ok(())
            } else {
                Err(format!("{path}: {a} != {e} (expected)"))
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                return Err(format!("{path}: {} items != {} (expected)", a.len(), e.len()));
            }
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                same_json(a, e, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        (Value::Object(a), Value::Object(e)) => {
            for key in e.keys().chain(a.keys()) {
                if key == "versionNonce" || key == "updated" {
                    continue;
                }
                match (a.get(key), e.get(key)) {
                    (Some(av), Some(ev)) => same_json(av, ev, &format!("{path}.{key}"))?,
                    (None, Some(ev)) => return Err(format!("{path}.{key}: missing, expected {ev}")),
                    (Some(av), None) => return Err(format!("{path}.{key}: {av}, expected no key")),
                    (None, None) => unreachable!(),
                }
            }
            Ok(())
        }
        (a, e) if a == e => Ok(()),
        (a, e) => Err(format!("{path}: {a} != {e} (expected)")),
    }
}

struct Outcome {
    scene: Scene,
    patches: Vec<HighlightPatch>,
}

/// The gesture as `DragInput` runs it.
fn run(scene_json: &Value, case: &Value) -> Outcome {
    let elements: Vec<Element> = scene_json
        .as_array()
        .expect("scene")
        .iter()
        .map(|e| Element::from_map(e.as_object().expect("element").clone()).expect("element"))
        .collect();
    let mut scene = Scene::new(elements);
    let mut app_state = AppState::default();
    for (k, v) in case["appState"].as_object().expect("appState") {
        app_state.insert(k.clone(), v.clone());
    }
    let mut env = TestEnv::with_layout();
    let prop = property(case["input"].as_str().expect("input"));
    let name = case["name"].as_str().expect("name");
    let mut patches = Vec::new();
    if let Some(text) = case["gesture"]["typed"].as_str() {
        let gesture = StatsGesture::begin(&scene, &app_state, prop)
            .unwrap_or_else(|| panic!("{name}: the panel shows the input"));
        patches.extend(gesture.apply(&mut scene, &app_state, StatsChange::typed(typed(text)), &mut env));
    } else {
        let steps = case["gesture"]["drag"].as_array().expect("drag");
        let gesture = StatsGesture::begin(&scene, &app_state, prop)
            .unwrap_or_else(|| panic!("{name}: the panel shows the input"));
        let mut drag = StatsDrag::new();
        for step in steps {
            let x = step["x"].as_f64().expect("x");
            if let Some((accumulated_change, instant_change)) = drag.pointer_move(x, 1.0) {
                let change = StatsChange {
                    accumulated_change,
                    instant_change,
                    should_change_by_step_size: step["shift"].as_bool().expect("shift"),
                    next_value: None,
                };
                patches.extend(gesture.apply(&mut scene, &app_state, change, &mut env));
            }
        }
        patches.extend(gesture.finish(&mut scene, &mut env));
    }
    Outcome { scene, patches }
}

fn patches_json(patches: &[HighlightPatch]) -> Value {
    Value::Array(
        patches
            .iter()
            .map(|p| json!({ "elementsToHighlight": p }))
            .collect(),
    )
}

#[test]
fn edits_match_upstream() {
    let fixture = fixture();
    let scene_json = &fixture["scene"];
    let mut failures = Vec::new();
    for case in fixture["cases"].as_array().expect("cases") {
        let name = case["name"].as_str().expect("name").to_owned();
        let result = &case["result"];
        let out = run(scene_json, case);
        let order: Vec<Value> = out
            .scene
            .elements()
            .iter()
            .map(|e| json!(e.base.id))
            .collect();
        if Value::Array(order.clone()) != result["order"] {
            failures.push(format!("{name}: order {order:?} != {}", result["order"]));
            continue;
        }
        let mut expected: Vec<Value> = scene_json.as_array().expect("scene").clone();
        for changed in result["changed"].as_array().expect("changed") {
            let slot = expected
                .iter_mut()
                .find(|e| e["id"] == changed["id"])
                .expect("a changed element is in the scene");
            *slot = changed.clone();
        }
        for actual in out.scene.elements() {
            let exp = expected
                .iter()
                .find(|e| e["id"] == json!(actual.base.id))
                .expect("element in the scene");
            let actual = Value::Object(actual.to_map());
            if let Err(e) = same_json(&actual, exp, &format!("{name} {}", exp["id"])) {
                failures.push(format!("{e}\n  port:     {actual}\n  upstream: {exp}"));
                break;
            }
        }
        let patches = patches_json(&out.patches);
        if patches != result["patches"] {
            failures.push(format!("{name}: patches {patches} != {}", result["patches"]));
        }
    }
    assert!(failures.is_empty(), "{} cases differ:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn every_callback_and_both_gestures_are_covered() {
    let fixture = fixture();
    let cases = fixture["cases"].as_array().expect("cases");
    for input in ["X", "Y", "W", "H", "A", "F"] {
        for multiple in [false, true] {
            let covered = cases.iter().any(|c| {
                c["input"] == input
                    && (c["appState"]["selectedElementIds"]
                        .as_object()
                        .expect("selection")
                        .len()
                        > 1)
                        == multiple
            });
            // Y is covered for groups by MultiPosition's Y cases
            assert!(covered || (input == "Y" && multiple), "{input} multiple={multiple}");
        }
    }
    assert!(cases.iter().any(|c| c["gesture"]["typed"].is_string()));
    assert!(cases.iter().any(|c| c["gesture"]["drag"].is_array()));
    assert!(cases.iter().any(|c| c["appState"]["croppingElementId"].is_string()));
    assert!(cases
        .iter()
        .any(|c| c["gesture"]["drag"].as_array().is_some_and(|d| d.iter().any(|s| s["shift"] == true))));
}

#[test]
fn drag_input_pointer_arithmetic() {
    // DragInput.tsx:255-305: the first move records the pointer; each move
    // of at least `sensitivity` pixels adds whole steps
    let mut drag = StatsDrag::new();
    assert_eq!(drag.pointer_move(100.0, 1.0), None);
    assert_eq!(drag.pointer_move(103.0, 1.0), Some((3.0, 3.0)));
    assert_eq!(drag.pointer_move(103.0, 1.0), None);
    assert_eq!(drag.pointer_move(98.0, 1.0), Some((-2.0, -5.0)));
    let mut grid = StatsDrag::new();
    assert_eq!(grid.pointer_move(0.0, 8.0), None);
    assert_eq!(grid.pointer_move(5.0, 8.0), None);
    // 5 + 12 = 17 px: two steps of 8, the remainder dropped
    assert_eq!(grid.pointer_move(17.0, 8.0), Some((2.0, 2.0)));
    assert_eq!(grid.pointer_move(20.0, 8.0), None);
}

#[test]
fn step_sized_values() {
    assert_eq!(get_step_sized_value(14.0, 10.0), 10.0);
    assert_eq!(get_step_sized_value(15.0, 10.0), 20.0);
    assert_eq!(get_step_sized_value(-14.0, 10.0), -0.0);
    assert_eq!(get_step_sized_value(22.0, 15.0), 15.0);
    assert_eq!(get_step_sized_value(23.0, 15.0), 30.0);
}

#[test]
fn inputs_the_panel_does_not_show_have_no_gesture() {
    let fixture = fixture();
    let elements: Vec<Element> = fixture["scene"]
        .as_array()
        .expect("scene")
        .iter()
        .map(|e| Element::from_map(e.as_object().expect("element").clone()).expect("element"))
        .collect();
    let scene = Scene::new(elements);
    let select = |ids: &[&str]| {
        let mut app_state = AppState::default();
        let ids: serde_json::Map<String, Value> =
            ids.iter().map(|id| (id.to_string(), json!(true))).collect();
        app_state.insert("selectedElementIds", Value::Object(ids));
        app_state
    };
    // nothing selected
    assert!(StatsGesture::begin(&scene, &select(&[]), StatsProperty::X).is_none());
    // a frame's angle (isPropertyEditable)
    assert!(StatsGesture::begin(&scene, &select(&["f"]), StatsProperty::Angle).is_none());
    // a frame with its child
    assert!(StatsGesture::begin(&scene, &select(&["f", "c"]), StatsProperty::X).is_none());
    // no text to size
    assert!(StatsGesture::begin(&scene, &select(&["r"]), StatsProperty::FontSize).is_none());
    assert!(StatsGesture::begin(&scene, &select(&["r", "rot"]), StatsProperty::FontSize).is_none());
    // the grid step is the General section's
    assert!(StatsGesture::begin(&scene, &select(&["r"]), StatsProperty::GridStep).is_none());
    // a container's font size is its label's
    let g = StatsGesture::begin(&scene, &select(&["box"]), StatsProperty::FontSize).expect("label");
    assert_eq!(g.elements[0].base.id, "lbl");
}
