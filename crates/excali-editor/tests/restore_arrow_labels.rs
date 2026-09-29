//! Arrow labels refitted on restore (ex-511, the ADR-008 exception moved
//! here from ex-304).
//!
//! `restoreElements` with `refreshDimensions` asks its environment to
//! refit every text (`restore.ts:1032-1045`); an arrow label is placed by
//! `LinearElementEditor.getBoundTextElementPosition`
//! (`linearElementEditor.ts:2068-2131`), which excali-editor gives
//! `excali_text::restore_env::TextEnv` as [`SceneArrowGeometry`]. The
//! `refresh-arrow-labels` case of excali-core's `restore-elements.json`
//! (generated from upstream by `tools/goldens/restore-elements-fixtures.mjs`,
//! text 10 px per character) records upstream's `refreshTextDimensions`
//! hook for labels on straight, bent, round, elbow and rotated arrows and
//! one with a `labelPosition`; each is reproduced from its arguments, and
//! so is the restored scene.

use excali_core::json;
use excali_core::restore::{
    restore_elements, RestoreElementsOptions, RestoreEnv, TestEnv, TextDimensionsRequest,
};
use excali_editor::text_layout::SceneArrowGeometry;
use excali_text::restore_env::TextEnv;
use excali_text::text_measurements::CharCountTextMetrics;
use serde_json::{Map, Value};

const RESTORE: &str = include_str!("../../excali-core/tests/fixtures/restore-elements.json");

fn arrow_case() -> Value {
    let fixture: Value = serde_json::from_str(RESTORE).expect("restore fixture parses");
    fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["id"] == "refresh-arrow-labels")
        .expect("the arrow label case")
        .clone()
}

fn object(value: &Value) -> Map<String, Value> {
    value.as_object().expect("an object").clone()
}

/// JSON equality with numbers compared as the doubles they denote and
/// objects compared by key, whatever their order.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

fn env() -> TextEnv<TestEnv, CharCountTextMetrics, SceneArrowGeometry> {
    TextEnv::with_geometry(TestEnv::default(), CharCountTextMetrics, SceneArrowGeometry)
}

/// Every recorded hook, answered by `TextEnv` with the scene's geometry
/// from the text as restore passed it, its arrow and the restored scene.
#[test]
fn arrow_label_hooks_are_reproduced() {
    let case = arrow_case();
    let elements: Vec<Map<String, Value>> = case["result"]
        .as_array()
        .expect("result")
        .iter()
        .map(object)
        .collect();
    let mut arrows = 0;
    for hook in case["hooks"].as_array().expect("hooks") {
        assert_eq!(hook["hook"], "refreshTextDimensions");
        let text = object(&hook["element"]);
        let container = elements
            .iter()
            .find(|e| e["id"] == hook["other"])
            .expect("the label's arrow");
        assert_eq!(container["type"], "arrow");
        arrows += 1;
        let answer = env().refresh_text_dimensions(TextDimensionsRequest {
            text: &text,
            container: Some(container),
            elements: &elements,
        });
        let got = answer.map_or(Value::Null, Value::Object);
        assert!(
            same(&got, &hook["result"]),
            "{}: got {got}, upstream {}",
            text["id"],
            hook["result"]
        );
    }
    assert_eq!(arrows, 7, "labelled arrows");
}

/// `restoreElements` with that environment writes upstream's scene.
#[test]
fn restore_elements_refits_arrow_labels() {
    let case = arrow_case();
    let opts = &case["opts"];
    let options = RestoreElementsOptions {
        refresh_dimensions: opts["refreshDimensions"] == true,
        repair_bindings: opts["repairBindings"] == true,
        delete_invisible_elements: opts["deleteInvisibleElements"] == true,
    };
    let elements = case["elements"].as_array().expect("elements").clone();
    let restored = restore_elements(&elements, None, options, &mut env()).expect("restores");
    let got = json::to_string_compact(&Value::Array(
        restored.into_iter().map(Value::Object).collect(),
    ));
    assert_eq!(got, json::to_string_compact(&case["result"]));
}
