//! Parity with rough.js 4.6.4 (the version upstream's `yarn.lock` resolves).
//!
//! The goldens are written by `tools/goldens/generate.mjs`, which calls
//! `new RoughGenerator()[method](...args, options)` from the pinned package
//! under Node and records the `Drawable` without its RNG state:
//!
//! - `random.json`: the first 64 `Random.next()` values for seven seeds;
//! - `rough-primitives.json`: every primitive at seeds 1, 7, 1041657908 and
//!   roughness 0, 1, 2 (the ex-203 acceptance set; every case must run);
//! - `rough-generator.json`: the generator's edge cases (path syntax, short
//!   point lists, `stroke: "none"`, `simplification`, single stroke, seed
//!   wrap-around); every case must run;
//! - `rough-options.json`: the cases without a `fill` (the curve targets and
//!   Excalidraw's dashed/dotted stroke rule) run here too, since their strokes
//!   come from the same generator; fills are ex-204.
//!
//! Numbers are compared as doubles with `==`, op by op, except where rough.js
//! goes through `Math.sin`/`Math.cos`/`Math.tan`/`Math.asin` (ellipses,
//! circles, arcs, SVG `A` commands). Those are not the same function on every
//! platform (upstream's own V8 on arm64 differs from x86_64 V8 and from libm
//! in the last bit; see `tools/goldens/README.md` and
//! `crates/excali-math/tests/goldens.rs`), so they are compared to within
//! [`PLATFORM_TOLERANCE`]. The number of ops, their kinds and every option
//! must still match exactly, so a draw taken out of order fails either way.

use std::path::Path;

use excali_rough::{Drawable, Op, Options, Random, RoughGenerator};
use serde_json::{json, Map, Value};

/// Relative tolerance for trigonometric cases: far below any geometric
/// meaning, far above a few ulps of libm disagreement carried through the
/// arithmetic.
const PLATFORM_TOLERANCE: f64 = 1e-10;

fn load(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../goldens")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run node tools/goldens/generate.mjs)",
            path.display()
        )
    });
    serde_json::from_str(&text).expect("golden parses")
}

fn f(v: &Value) -> f64 {
    v.as_f64()
        .unwrap_or_else(|| panic!("number expected, got {v}"))
}

fn numbers(v: &Value) -> Vec<f64> {
    v.as_array().expect("number list").iter().map(f).collect()
}

fn points(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .expect("point list")
        .iter()
        .map(|p| [f(&p[0]), f(&p[1])])
        .collect()
}

/// rough.js `Object.assign({}, defaultOptions, options)` for the keys the
/// goldens use. `None` when the case sets an option this crate does not
/// generate from yet (`fill`, ex-204).
fn options(v: &Value) -> Option<Options> {
    let mut o = Options::default();
    for (k, v) in v.as_object().expect("options object") {
        match k.as_str() {
            "maxRandomnessOffset" => o.max_randomness_offset = f(v),
            "roughness" => o.roughness = f(v),
            "bowing" => o.bowing = f(v),
            "stroke" => o.stroke = v.as_str().expect("stroke").to_owned(),
            "strokeWidth" => o.stroke_width = f(v),
            "curveFitting" => o.curve_fitting = f(v),
            "curveTightness" => o.curve_tightness = f(v),
            "curveStepCount" => o.curve_step_count = f(v),
            "fillStyle" => o.fill_style = v.as_str().expect("fillStyle").to_owned(),
            "fillWeight" => o.fill_weight = f(v),
            "hachureAngle" => o.hachure_angle = f(v),
            "hachureGap" => o.hachure_gap = f(v),
            "simplification" => o.simplification = Some(f(v)),
            "dashOffset" => o.dash_offset = f(v),
            "dashGap" => o.dash_gap = f(v),
            "zigzagOffset" => o.zigzag_offset = f(v),
            "seed" => o.seed = i32::try_from(v.as_i64().expect("integer seed")).expect("i32 seed"),
            "strokeLineDash" => o.stroke_line_dash = Some(numbers(v)),
            "strokeLineDashOffset" => o.stroke_line_dash_offset = Some(f(v)),
            "fillLineDash" => o.fill_line_dash = Some(numbers(v)),
            "fillLineDashOffset" => o.fill_line_dash_offset = Some(f(v)),
            "disableMultiStroke" => o.disable_multi_stroke = v.as_bool().expect("bool"),
            "disableMultiStrokeFill" => o.disable_multi_stroke_fill = v.as_bool().expect("bool"),
            "preserveVertices" => o.preserve_vertices = v.as_bool().expect("bool"),
            "fixedDecimalPlaceDigits" => o.fixed_decimal_place_digits = Some(f(v)),
            "fillShapeRoughnessGain" => o.fill_shape_roughness_gain = f(v),
            "fill" => return None,
            other => panic!("option {other} is not a rough.js 4.6.4 option"),
        }
    }
    Some(o)
}

/// The resolved options as rough.js holds them (`ResolvedOptions` without the
/// randomizer): the defaults always, the optional keys when set.
fn options_json(o: &Options) -> Value {
    let mut m = Map::new();
    let mut put = |k: &str, v: Value| {
        m.insert(k.to_owned(), v);
    };
    put("maxRandomnessOffset", json!(o.max_randomness_offset));
    put("roughness", json!(o.roughness));
    put("bowing", json!(o.bowing));
    put("stroke", json!(o.stroke));
    put("strokeWidth", json!(o.stroke_width));
    put("curveTightness", json!(o.curve_tightness));
    put("curveFitting", json!(o.curve_fitting));
    put("curveStepCount", json!(o.curve_step_count));
    put("fillStyle", json!(o.fill_style));
    put("fillWeight", json!(o.fill_weight));
    put("hachureAngle", json!(o.hachure_angle));
    put("hachureGap", json!(o.hachure_gap));
    put("dashOffset", json!(o.dash_offset));
    put("dashGap", json!(o.dash_gap));
    put("zigzagOffset", json!(o.zigzag_offset));
    put("seed", json!(o.seed));
    put("disableMultiStroke", json!(o.disable_multi_stroke));
    put("disableMultiStrokeFill", json!(o.disable_multi_stroke_fill));
    put("preserveVertices", json!(o.preserve_vertices));
    put("fillShapeRoughnessGain", json!(o.fill_shape_roughness_gain));
    if let Some(v) = o.simplification {
        put("simplification", json!(v));
    }
    if let Some(v) = &o.stroke_line_dash {
        put("strokeLineDash", json!(v));
    }
    if let Some(v) = o.stroke_line_dash_offset {
        put("strokeLineDashOffset", json!(v));
    }
    if let Some(v) = &o.fill_line_dash {
        put("fillLineDash", json!(v));
    }
    if let Some(v) = o.fill_line_dash_offset {
        put("fillLineDashOffset", json!(v));
    }
    if let Some(v) = o.fixed_decimal_place_digits {
        put("fixedDecimalPlaceDigits", json!(v));
    }
    Value::Object(m)
}

fn op_json(op: &Op) -> Value {
    json!({ "op": op.name(), "data": op.data() })
}

fn drawable_json(d: &Drawable) -> Value {
    json!({
        "shape": d.shape.as_str(),
        "options": options_json(&d.options),
        "sets": d.sets.iter().map(|s| json!({
            "type": s.kind.as_str(),
            "ops": s.ops.iter().map(op_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn call(method: &str, a: &[Value], o: &Options) -> Drawable {
    let g = RoughGenerator::new();
    let n = |i: usize| f(&a[i]);
    match method {
        "line" => g.line(n(0), n(1), n(2), n(3), o),
        "rectangle" => g.rectangle(n(0), n(1), n(2), n(3), o),
        "ellipse" => g.ellipse(n(0), n(1), n(2), n(3), o),
        "circle" => g.circle(n(0), n(1), n(2), o),
        "arc" => g.arc(
            n(0),
            n(1),
            n(2),
            n(3),
            n(4),
            n(5),
            a.get(6).and_then(Value::as_bool).unwrap_or(false),
            o,
        ),
        "linearPath" => g.linear_path(&points(&a[0]), o),
        "polygon" => g.polygon(&points(&a[0]), o),
        "curve" => g.curve(&points(&a[0]), o),
        "path" => g
            .path(a[0].as_str().expect("path data"), o)
            .unwrap_or_else(|e| panic!("path {}: {e}", a[0])),
        other => panic!("unknown method {other}"),
    }
}

/// Whether rough.js computed this case through trigonometric functions.
fn uses_trig(method: &str, args: &[Value]) -> bool {
    match method {
        "ellipse" | "circle" | "arc" => true,
        "path" => args[0]
            .as_str()
            .is_some_and(|d| d.contains('A') || d.contains('a')),
        _ => false,
    }
}

/// Structural equality; numbers exactly or within `tolerance` relative to
/// the larger of 1 and the expected magnitude.
fn same(actual: &Value, expected: &Value, tolerance: f64) -> bool {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().expect("f64"), b.as_f64().expect("f64"));
            a == b || (a - b).abs() <= tolerance * b.abs().max(1.0)
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y, tolerance))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|w| same(v, w, tolerance)))
        }
        _ => actual == expected,
    }
}

/// The first differing path, for a readable failure.
fn first_difference(actual: &Value, expected: &Value, tolerance: f64, at: &str) -> String {
    match (actual, expected) {
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => a
            .iter()
            .zip(b)
            .enumerate()
            .find(|(_, (x, y))| !same(x, y, tolerance))
            .map(|(i, (x, y))| first_difference(x, y, tolerance, &format!("{at}[{i}]")))
            .unwrap_or_default(),
        (Value::Object(a), Value::Object(b)) if a.len() == b.len() => a
            .iter()
            .find(|(k, v)| !b.get(*k).is_some_and(|w| same(v, w, tolerance)))
            .map(|(k, v)| first_difference(v, &b[k], tolerance, &format!("{at}.{k}")))
            .unwrap_or_default(),
        _ => format!("{at}: got {actual}, upstream {expected}"),
    }
}

/// Runs every case of `file` that this crate generates; returns how many ran.
fn check_file(file: &str, all_must_run: bool) -> usize {
    let doc = load(file);
    let cases = doc["cases"].as_array().expect("cases");
    let mut ran = 0;
    let mut failures = Vec::new();
    for c in cases {
        let id = c["id"].as_str().expect("id");
        let Some(o) = options(&c["options"]) else {
            assert!(!all_must_run, "{file} {id}: every case must run");
            continue;
        };
        let method = c["method"].as_str().expect("method");
        let args = c["args"].as_array().expect("args");
        let actual = drawable_json(&call(method, args, &o));
        let tolerance = if uses_trig(method, args) {
            PLATFORM_TOLERANCE
        } else {
            0.0
        };
        if !same(&actual, &c["drawable"], tolerance) {
            failures.push(format!(
                "{id}: {}",
                first_difference(&actual, &c["drawable"], tolerance, "drawable")
            ));
        }
        ran += 1;
    }
    assert!(
        failures.is_empty(),
        "{file}: {} of {ran} cases differ from rough.js 4.6.4:\n{}",
        failures.len(),
        failures.join("\n")
    );
    ran
}

#[test]
fn random_matches_park_miller_sequences() {
    let doc = load("random.json");
    let cases = doc["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 7);
    for c in cases {
        let seed = i32::try_from(c["seed"].as_i64().expect("seed")).expect("i32 seed");
        let mut rng = Random::new(seed);
        for (i, v) in c["values"].as_array().expect("values").iter().enumerate() {
            assert_eq!(rng.next(), f(v), "seed {seed}, draw {i}");
        }
    }
}

#[test]
fn primitives_match_at_every_seed_and_roughness() {
    // 13 primitives x seeds 1, 7, 1041657908 x roughness 0, 1, 2.
    assert_eq!(check_file("rough-primitives.json", true), 117);
}

#[test]
fn generator_edge_cases_match() {
    assert!(check_file("rough-generator.json", true) >= 40);
}

#[test]
fn unfilled_option_cases_match() {
    // The curve targets and Excalidraw's dashed/dotted stroke rule.
    let ran = check_file("rough-options.json", false);
    assert!(ran >= 40, "only {ran} unfilled cases in rough-options.json");
}
