//! The golden harness itself (ex-217): `excali_rough::goldens`, compiled
//! with the `goldens` feature.
//!
//! Acceptance: a failing golden prints a numeric diff with the element id,
//! the op index and the expected and actual values. These tests build a
//! golden case from a real drawable, break one thing in it (a number, an op
//! kind, an op count, an option, a shape, an SVG path token) and check the
//! report the harness prints, and that a clean case passes.

use excali_rough::goldens::{
    drawable_json, ulps, ActualShape, Difference, Manifest, Report, Tolerance,
};
use excali_rough::{Drawable, Options, RoughGenerator};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn options() -> Options {
    Options {
        seed: 7,
        ..Options::default()
    }
}

fn rectangle() -> Drawable {
    RoughGenerator::new().rectangle(10.0, 20.0, 200.0, 100.0, &options())
}

/// An element case (`{ id, element, renderConfig, shapes }`) whose one shape
/// is `drawable`.
fn element_case(drawable: Value) -> Value {
    json!({
        "id": "rectangle/seed7-r1",
        "element": { "id": "rectangle_seed7-r1", "type": "rectangle" },
        "renderConfig": {},
        "shapes": [{ "type": "rough", "drawable": drawable }],
    })
}

fn op_data(d: &mut Value, set: usize, op: usize) -> &mut Vec<Value> {
    d["sets"][set]["ops"][op]["data"]
        .as_array_mut()
        .expect("op data")
}

fn number(v: &Value) -> f64 {
    v.as_f64().expect("number")
}

#[test]
fn drawable_json_is_the_golden_shape_of_a_drawable() {
    let d = rectangle();
    let j = drawable_json(&d);
    assert_eq!(j["shape"], "rectangle");
    assert_eq!(j["options"]["seed"], 7);
    assert_eq!(j["options"]["roughness"], 1.0);
    // no optional key unless set
    assert!(j["options"].get("simplification").is_none());
    let sets = j["sets"].as_array().expect("sets");
    assert_eq!(sets.len(), d.sets.len());
    assert_eq!(sets[0]["type"], "path");
    let ops = sets[0]["ops"].as_array().expect("ops");
    assert_eq!(ops.len(), d.sets[0].ops.len());
    assert_eq!(ops[0]["op"], "move");
    assert_eq!(ops[1]["op"], "bcurveTo");
    assert_eq!(ops[1]["data"].as_array().expect("data").len(), 6);
    assert_eq!(number(&ops[1]["data"][4]), d.sets[0].ops[1].data()[4]);
}

#[test]
fn a_matching_case_passes() {
    let d = rectangle();
    let case = element_case(drawable_json(&d));
    let mut report = Report::new("elements-rectangle.json");
    let found = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Exact);
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(report.ran(), 1);
    assert_eq!(report.failed(), 0);
    assert_eq!(report.assert_ok(), 1);
}

#[test]
fn a_failing_golden_prints_element_id_op_index_and_expected_actual() {
    let d = rectangle();
    let mut golden = drawable_json(&d);
    let actual = d.sets[0].ops[1].data()[4];
    let expected = actual + 0.5;
    op_data(&mut golden, 0, 1)[4] = json!(expected);
    let case = element_case(golden);

    let mut report = Report::new("elements-rectangle.json");
    let found = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Exact);
    assert_eq!(found.len(), 1, "{found:?}");
    let m = &found[0];
    assert_eq!(m.location.case, "rectangle/seed7-r1");
    assert_eq!(m.location.element.as_deref(), Some("rectangle_seed7-r1"));
    assert_eq!(m.location.shape, Some(0));
    assert_eq!(m.location.set, Some((0, "path".to_owned())));
    assert_eq!(m.location.op, Some((1, "bcurveTo".to_owned())));
    assert_eq!(m.location.data, Some(4));
    assert_eq!(
        m.difference,
        Difference::Number {
            expected,
            actual,
            tolerance: Tolerance::Exact
        }
    );

    let text = report.render();
    assert!(
        text.contains("elements-rectangle.json: 1 of 1 cases differ from upstream"),
        "{text}"
    );
    assert!(text.contains("case rectangle/seed7-r1"), "{text}");
    assert!(text.contains("element rectangle_seed7-r1"), "{text}");
    assert!(text.contains("shape 0"), "{text}");
    assert!(text.contains("set 0 (path)"), "{text}");
    assert!(text.contains("op 1 (bcurveTo)"), "{text}");
    assert!(text.contains("data[4]"), "{text}");
    assert!(text.contains(&format!("expected {expected}")), "{text}");
    assert!(text.contains(&format!("actual   {actual}")), "{text}");
    assert!(text.contains("diff     -0.5"), "{text}");
    assert!(text.contains("tolerance exact"), "{text}");
    // the whole ops, so the reader sees the neighbouring numbers too
    assert!(text.contains("expected op bcurveTo ["), "{text}");
    assert!(text.contains("actual op   bcurveTo ["), "{text}");

    // and a failing report panics with that text
    let panic = std::panic::catch_unwind(move || report.assert_ok()).expect_err("panics");
    let message = panic
        .downcast_ref::<String>()
        .cloned()
        .expect("string panic");
    assert_eq!(message, text);
}

#[test]
fn every_differing_number_of_a_case_is_listed_up_to_a_cap() {
    let d = rectangle();
    let mut golden = drawable_json(&d);
    let n = d.sets[0].ops.len();
    assert!(n > 12);
    for i in 0..n {
        let data = op_data(&mut golden, 0, i);
        let x = number(&data[0]);
        data[0] = json!(x + 1.0);
    }
    let case = element_case(golden);
    let mut report = Report::new("elements-rectangle.json");
    let found = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Exact);
    assert_eq!(found.len(), n);
    let text = report.render();
    assert!(text.contains("op 0 (move)"), "{text}");
    assert!(text.contains("op 7 ("), "{text}");
    assert!(!text.contains("op 8 ("), "{text}");
    assert!(
        text.contains(&format!("... and {} more differences in this case", n - 8)),
        "{text}"
    );
}

#[test]
fn tolerance_decides_and_is_printed_with_ulps() {
    let d = rectangle();
    let mut golden = drawable_json(&d);
    let actual = d.sets[0].ops[1].data()[0];
    let expected = f64::from_bits(actual.to_bits() + 3);
    op_data(&mut golden, 0, 1)[0] = json!(expected);
    let case = element_case(golden);

    let mut report = Report::new("elements-rectangle.json");
    let relative = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Relative(1e-10));
    assert!(relative.is_empty(), "{relative:?}");

    let exact = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Exact);
    assert_eq!(exact.len(), 1);
    let text = exact[0].to_string();
    assert!(text.contains("(3 ulp"), "{text}");
    assert!(text.contains("tolerance exact"), "{text}");

    // beyond the relative tolerance: the tolerance is printed
    let mut golden = drawable_json(&d);
    op_data(&mut golden, 0, 1)[0] = json!(actual * (1.0 + 1e-8));
    let case = element_case(golden);
    let far = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Relative(1e-10));
    assert_eq!(far.len(), 1);
    assert!(
        far[0].to_string().contains("tolerance relative 1e-10"),
        "{}",
        far[0]
    );
    assert_eq!(report.ran(), 3);
    assert_eq!(report.failed(), 2);
}

#[test]
fn tolerance_is_relative_to_the_larger_of_one_and_the_expected_magnitude() {
    let t = Tolerance::Relative(1e-10);
    assert!(t.accepts(1e6, 1e6 + 1e-5));
    assert!(!t.accepts(1e6, 1e6 + 1e-3));
    assert!(t.accepts(0.0, 5e-11));
    assert!(!t.accepts(0.0, 5e-10));
    assert!(Tolerance::Exact.accepts(0.5, 0.5));
    assert!(Tolerance::Exact.accepts(0.0, -0.0));
    assert!(!Tolerance::Exact.accepts(0.5, 0.5000000000000001));
    assert!(!t.accepts(f64::NAN, f64::NAN));
}

#[test]
fn ulps_counts_representable_doubles_between() {
    assert_eq!(ulps(1.0, 1.0), 0);
    assert_eq!(ulps(1.0, 1.0 + f64::EPSILON), 1);
    assert_eq!(ulps(1.0 + f64::EPSILON, 1.0), 1);
    assert_eq!(ulps(0.0, -0.0), 0);
    let tiny = f64::from_bits(1);
    assert_eq!(ulps(-tiny, tiny), 2);
    assert_eq!(ulps(2.0, f64::from_bits(2.0f64.to_bits() + 5)), 5);
}

#[test]
fn a_different_op_count_is_reported_with_both_counts() {
    let d = rectangle();
    let mut golden = drawable_json(&d);
    let n = d.sets[0].ops.len();
    golden["sets"][0]["ops"].as_array_mut().expect("ops").pop();
    let case = element_case(golden);
    let mut report = Report::new("elements-rectangle.json");
    let found = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Exact);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        found[0].difference,
        Difference::Count {
            what: "ops",
            expected: n - 1,
            actual: n
        }
    );
    let text = report.render();
    assert!(text.contains("set 0 (path)"), "{text}");
    assert!(
        text.contains(&format!("ops: expected {}, actual {n}", n - 1)),
        "{text}"
    );
}

#[test]
fn a_different_op_kind_is_reported_at_its_index() {
    let d = rectangle();
    let mut golden = drawable_json(&d);
    golden["sets"][0]["ops"][2]["op"] = json!("lineTo");
    golden["sets"][0]["ops"][2]["data"] = json!([1, 2]);
    let case = element_case(golden);
    let mut report = Report::new("elements-rectangle.json");
    let found = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Exact);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].location.op, Some((2, "lineTo".to_owned())));
    let text = found[0].to_string();
    assert!(text.contains("op 2 (lineTo)"), "{text}");
    assert!(text.contains("expected \"lineTo\""), "{text}");
    assert!(
        text.contains(&format!("actual   \"{}\"", d.sets[0].ops[2].name())),
        "{text}"
    );
}

#[test]
fn a_different_set_type_and_set_count_are_reported() {
    let d = RoughGenerator::new().rectangle(
        0.0,
        0.0,
        50.0,
        40.0,
        &Options {
            seed: 3,
            fill: Some("#ff0000".to_owned()),
            fill_style: "solid".to_owned(),
            ..Options::default()
        },
    );
    assert_eq!(d.sets.len(), 2);
    let mut golden = drawable_json(&d);
    golden["sets"][0]["type"] = json!("fillSketch");
    let case = element_case(golden.clone());
    let mut report = Report::new("elements-rectangle.json");
    let found = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Exact);
    assert_eq!(found.len(), 1, "{found:?}");
    let text = found[0].to_string();
    assert!(text.contains("set 0 (fillSketch)"), "{text}");
    assert!(text.contains("expected \"fillSketch\""), "{text}");
    assert!(text.contains("actual   \"fillPath\""), "{text}");

    golden = drawable_json(&d);
    golden["sets"].as_array_mut().expect("sets").pop();
    let found = report.element(
        &element_case(golden),
        &[ActualShape::Rough(&d)],
        Tolerance::Exact,
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].to_string().contains("sets: expected 1, actual 2"),
        "{}",
        found[0]
    );
}

#[test]
fn a_different_option_is_reported_by_key() {
    let d = rectangle();
    let mut golden = drawable_json(&d);
    golden["options"]["roughness"] = json!(2);
    golden["options"]["stroke"] = json!("#1e1e1e");
    golden["options"]["simplification"] = json!(0.5);
    let case = element_case(golden);
    let mut report = Report::new("elements-rectangle.json");
    let found = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Exact);
    assert_eq!(found.len(), 3, "{found:?}");
    let text = report.render();
    assert!(text.contains("options.roughness"), "{text}");
    assert!(text.contains("expected 2"), "{text}");
    assert!(text.contains("actual   1"), "{text}");
    assert!(text.contains("options.stroke"), "{text}");
    assert!(text.contains("expected \"#1e1e1e\""), "{text}");
    assert!(text.contains("actual   \"#000\""), "{text}");
    assert!(text.contains("options.simplification"), "{text}");
    assert!(text.contains("actual   (absent)"), "{text}");
}

#[test]
fn a_different_shape_count_or_kind_is_reported() {
    let d = rectangle();
    let case = json!({
        "id": "line/two",
        "element": { "id": "line_two" },
        "shapes": [
            { "type": "rough", "drawable": drawable_json(&d) },
            { "type": "rough", "drawable": drawable_json(&d) },
        ],
    });
    let mut report = Report::new("elements-line.json");
    let found = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Exact);
    assert_eq!(found.len(), 1, "{found:?}");
    let text = found[0].to_string();
    assert!(text.contains("element line_two"), "{text}");
    assert!(text.contains("shapes: expected 2, actual 1"), "{text}");

    let case = json!({
        "id": "freedraw/one",
        "element": { "id": "freedraw_one" },
        "shapes": [{ "type": "svgPath", "d": "M 1.00,2.00 L 3.00,4.00" }],
    });
    let found = report.element(&case, &[ActualShape::Rough(&d)], Tolerance::Exact);
    assert_eq!(found.len(), 1, "{found:?}");
    let text = found[0].to_string();
    assert!(text.contains("shape 0"), "{text}");
    assert!(text.contains("expected \"svgPath\""), "{text}");
    assert!(text.contains("actual   \"rough\""), "{text}");
}

#[test]
fn an_empty_shape_list_matches_nothing_drawn() {
    // upstream returns null for text: `shapes` is []
    let case = json!({ "id": "text/plain", "element": { "id": "text_plain" }, "shapes": [] });
    let mut report = Report::new("elements-upstream-fixtures.json");
    assert!(report.element(&case, &[], Tolerance::Exact).is_empty());
    assert_eq!(report.assert_ok(), 1);
}

#[test]
fn svg_paths_are_compared_number_by_number() {
    let case = json!({
        "id": "freedraw/wave",
        "element": { "id": "freedraw_wave" },
        "shapes": [{ "type": "svgPath", "d": "M 1.00,-1.19 Q 1.00,-1.19 3.73,1.15 Z" }],
    });
    let mut report = Report::new("elements-freedraw.json");
    assert!(report
        .element(
            &case,
            &[ActualShape::SvgPath(
                "M 1.00,-1.19 Q 1.00,-1.19 3.73,1.15 Z"
            )],
            Tolerance::Exact
        )
        .is_empty());

    let found = report.element(
        &case,
        &[ActualShape::SvgPath(
            "M 1.00,-1.19 Q 1.00,-1.19 3.74,1.15 Z",
        )],
        Tolerance::Exact,
    );
    assert_eq!(found.len(), 1, "{found:?}");
    let text = found[0].to_string();
    assert!(text.contains("element freedraw_wave"), "{text}");
    assert!(text.contains("d number 4"), "{text}");
    assert!(text.contains("expected 3.73"), "{text}");
    assert!(text.contains("actual   3.74"), "{text}");

    // a command letter that differs is a structural difference
    let found = report.element(
        &case,
        &[ActualShape::SvgPath(
            "M 1.00,-1.19 L 1.00,-1.19 3.73,1.15 Z",
        )],
        Tolerance::Exact,
    );
    assert_eq!(found.len(), 1, "{found:?}");
    let text = found[0].to_string();
    assert!(text.contains("d token 3"), "{text}");
    assert!(text.contains("expected \"Q\""), "{text}");
    assert!(text.contains("actual   \"L\""), "{text}");
}

#[test]
fn rough_cases_without_an_element_print_the_case_id() {
    let d = rectangle();
    let mut golden = drawable_json(&d);
    op_data(&mut golden, 0, 0)[1] = json!(99);
    let case = json!({ "id": "rectangle/seed7/r1", "method": "rectangle", "drawable": golden });
    let mut report = Report::new("rough-primitives.json");
    let found = report.drawable(&case, &case["drawable"], &d, Tolerance::Exact);
    assert_eq!(found.len(), 1, "{found:?}");
    let text = report.render();
    assert!(text.contains("case rectangle/seed7/r1"), "{text}");
    assert!(!text.contains("element"), "{text}");
    assert!(text.contains("op 0 (move) data[1]"), "{text}");
    assert!(text.contains("expected 99"), "{text}");
}

fn manifest_text(name: &str, cases: usize, bytes: &[u8]) -> String {
    let sha: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!(
        r#"{{"generator":"tools/goldens/generate.mjs","upstream":{{"repo":"https://github.com/excalidraw/excalidraw","commit":"abc"}},"packages":{{"roughjs":"4.6.4"}},"files":[{{"name":"{name}","cases":{cases},"sha256":"{sha}"}}]}}"#
    )
}

#[test]
fn the_manifest_pins_every_file_by_hash_and_case_count() {
    let golden = br#"{"description":"x","cases":[{"id":"a"},{"id":"b"}]}"#;
    let manifest = Manifest::parse(&manifest_text("a.json", 2, golden)).expect("parses");
    assert_eq!(manifest.upstream_commit, "abc");
    assert_eq!(manifest.packages["roughjs"], "4.6.4");
    assert_eq!(manifest.files.len(), 1);
    assert_eq!(manifest.files[0].name, "a.json");
    assert_eq!(manifest.files[0].cases, 2);
    manifest.verify("a.json", golden).expect("verifies");

    let edited = br#"{"description":"x","cases":[{"id":"a"},{"id":"c"}]}"#;
    let e = manifest.verify("a.json", edited).expect_err("hash differs");
    assert!(e.contains("a.json: sha256"), "{e}");
    assert!(e.contains("node tools/goldens/generate.mjs"), "{e}");

    let e = manifest.verify("b.json", golden).expect_err("not listed");
    assert!(e.contains("b.json is not in goldens/manifest.json"), "{e}");

    let three = br#"{"description":"x","cases":[{"id":"a"},{"id":"b"},{"id":"c"}]}"#;
    let manifest = Manifest::parse(&manifest_text("a.json", 2, three)).expect("parses");
    let e = manifest.verify("a.json", three).expect_err("count differs");
    assert!(e.contains("a.json: 3 cases, manifest says 2"), "{e}");

    assert!(Manifest::parse("{}").is_err());
    assert!(Manifest::parse("not json").is_err());
}
