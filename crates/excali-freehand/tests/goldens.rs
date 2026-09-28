//! Parity with perfect-freehand 1.2.0 (the version upstream's `yarn.lock`
//! resolves) and with upstream's `getVariableWidthFreedrawOutline`
//! (`packages/element/src/shape.ts:1221-1245` at the pinned commit).
//!
//! The goldens are written by `tools/goldens/generate.mjs` from the pinned
//! package under Node:
//!
//! - `freehand.json`: `getStrokePoints(points, options)` and
//!   `getStroke(points, options)` for Excalidraw's options (size `sw * 4.25`,
//!   thinning 0.6, smoothing 0.5, streamline from `strokeOptions`, easing
//!   `sin(t * pi / 2)`, `last: true`, simulated and real pressure), the
//!   library defaults, and `edge/` cases for every other branch. Every case
//!   must run.
//! - `elements-freedraw.json`: `getFreedrawOutlinePoints(element)` for every
//!   element: perfect-freehand for variable width, the laser pointer
//!   (`getConstantWidthFreedrawOutline`, `shape.ts:1247-1268`) for constant
//!   width.
//!
//! Stroke points involve no trigonometry (lerps, V8's `Math.hypot`, which
//! `excali_math::js::hypot` reproduces, and divisions), so they are compared
//! with `==`. Outlines go through `Math.sin`/`Math.cos` (cap and corner
//! rotations, the easeOutSine easing), which differ in the last bit between
//! V8 on arm64 and libm (see `crates/excali-math/tests/goldens.rs`), so their
//! coordinates are compared to within [`PLATFORM_TOLERANCE`]; the number of
//! outline points must still match exactly.

use std::path::Path;

use excali_freehand::{
    constant_width_outline, ease_out_sine, get_stroke, get_stroke_outline_points,
    get_stroke_points, variable_width_options, variable_width_outline, CapOptions, InputPoint,
    StrokeOptions, StrokePoint, Taper, DEFAULT_STROKE_STREAMLINE,
};
use serde_json::Value;

/// Relative tolerance (absolute below 1) for outline coordinates.
const PLATFORM_TOLERANCE: f64 = 1e-10;

fn load(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../goldens")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run node tools/goldens/generate.mjs)",
            path.display()
        )
    });
    let doc: Value = serde_json::from_str(&text).expect("golden parses");
    doc["cases"].as_array().expect("cases").clone()
}

fn f(v: &Value) -> f64 {
    v.as_f64()
        .unwrap_or_else(|| panic!("number expected, got {v}"))
}

fn pair(v: &Value) -> [f64; 2] {
    [f(&v[0]), f(&v[1])]
}

fn easing(name: &str) -> fn(f64) -> f64 {
    match name {
        "easeOutSine" => ease_out_sine,
        "linear" => |t| t,
        other => panic!("unknown easing {other}"),
    }
}

/// A golden input point: `[x, y]`, `[x, y, pressure]` or `{x, y, pressure?}`.
fn input_point(v: &Value) -> InputPoint {
    if let Some(o) = v.as_object() {
        InputPoint::from_object(f(&o["x"]), f(&o["y"]), o.get("pressure").map(f))
    } else {
        let a = v.as_array().expect("point");
        match a.len() {
            2 => InputPoint::new(f(&a[0]), f(&a[1])),
            3 => InputPoint::with_pressure(f(&a[0]), f(&a[1]), f(&a[2])),
            n => panic!("point of length {n}"),
        }
    }
}

fn taper(v: &Value) -> Taper {
    match v {
        Value::Bool(true) => Taper::Full,
        Value::Bool(false) => Taper::Off,
        n => Taper::Distance(f(n)),
    }
}

fn cap(v: Option<&Value>, mut c: CapOptions) -> CapOptions {
    let Some(v) = v else { return c };
    for (k, v) in v.as_object().expect("cap options") {
        match k.as_str() {
            "cap" => c.cap = v.as_bool().expect("cap"),
            "taper" => c.taper = Some(taper(v)),
            "easing" => c.easing = easing(v.as_str().expect("easing name")),
            other => panic!("unknown cap option {other}"),
        }
    }
    c
}

/// perfect-freehand's destructuring defaults, then the case's options.
fn options(v: &Value) -> StrokeOptions {
    let mut o = StrokeOptions::default();
    let map = v.as_object().expect("options");
    for (k, v) in map {
        match k.as_str() {
            "size" => o.size = f(v),
            "thinning" => o.thinning = f(v),
            "smoothing" => o.smoothing = f(v),
            "streamline" => o.streamline = f(v),
            "easing" => o.easing = easing(v.as_str().expect("easing name")),
            "simulatePressure" => o.simulate_pressure = v.as_bool().expect("bool"),
            "last" => o.last = v.as_bool().expect("bool"),
            "start" | "end" => {}
            other => panic!("unknown option {other}"),
        }
    }
    o.start = cap(map.get("start"), CapOptions::start());
    o.end = cap(map.get("end"), CapOptions::end());
    o
}

fn close(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= PLATFORM_TOLERANCE * b.abs().max(1.0)
}

fn check_outline(id: &str, got: &[[f64; 2]], want: &Value) {
    let want: Vec<[f64; 2]> = want.as_array().expect("outline").iter().map(pair).collect();
    assert_eq!(got.len(), want.len(), "{id}: outline length");
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert!(
            close(g[0], w[0]) && close(g[1], w[1]),
            "{id}: outline[{i}] = {g:?}, upstream {w:?}"
        );
    }
}

fn check_stroke_points(id: &str, got: &[StrokePoint], want: &Value) {
    let want = want.as_array().expect("strokePoints");
    assert_eq!(got.len(), want.len(), "{id}: stroke point count");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert_eq!(g.point, pair(&w["point"]), "{id}: [{i}].point");
        assert_eq!(g.pressure, f(&w["pressure"]), "{id}: [{i}].pressure");
        assert_eq!(g.vector, pair(&w["vector"]), "{id}: [{i}].vector");
        assert_eq!(g.distance, f(&w["distance"]), "{id}: [{i}].distance");
        assert_eq!(
            g.running_length,
            f(&w["runningLength"]),
            "{id}: [{i}].runningLength"
        );
    }
}

#[test]
fn freehand_goldens() {
    let cases = load("freehand.json");
    assert!(cases.len() >= 50, "freehand.json has {} cases", cases.len());
    for c in &cases {
        let id = c["id"].as_str().expect("id");
        let points: Vec<InputPoint> = c["points"]
            .as_array()
            .expect("points")
            .iter()
            .map(input_point)
            .collect();
        let o = options(&c["options"]);
        let stroke_points = get_stroke_points(&points, &o);
        check_stroke_points(id, &stroke_points, &c["strokePoints"]);
        // getStroke is getStrokeOutlinePoints(getStrokePoints(...)).
        check_outline(
            id,
            &get_stroke_outline_points(&stroke_points, &o),
            &c["outline"],
        );
        check_outline(id, &get_stroke(&points, &o), &c["outline"]);
    }
}

/// The Excalidraw cases in freehand.json were generated with exactly the
/// options `variable_width_options` builds.
#[test]
fn excalidraw_cases_use_variable_width_options() {
    let mut seen = 0;
    for c in load("freehand.json") {
        let id = c["id"].as_str().expect("id");
        if !id.starts_with("excalidraw/") {
            continue;
        }
        let o = &c["options"];
        let size = f(&o["size"]);
        let built = variable_width_options(
            size / 4.25,
            Some(f(&o["streamline"])),
            o["simulatePressure"].as_bool().expect("bool"),
        );
        let golden = options(o);
        assert_eq!(built.size, golden.size, "{id}");
        assert_eq!(built.thinning, golden.thinning, "{id}");
        assert_eq!(built.smoothing, golden.smoothing, "{id}");
        assert_eq!(built.streamline, golden.streamline, "{id}");
        assert_eq!(built.simulate_pressure, golden.simulate_pressure, "{id}");
        assert_eq!(built.last, golden.last, "{id}");
        assert!(built.last);
        for t in [0.0, 0.1, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!((built.easing)(t), (golden.easing)(t), "{id}: easing({t})");
        }
        seen += 1;
    }
    assert!(seen >= 25);
}

#[test]
fn variable_width_element_outlines() {
    let mut seen = 0;
    for c in load("elements-freedraw.json") {
        let id = c["id"].as_str().expect("id");
        let e = &c["element"];
        let stroke_options = &e["strokeOptions"];
        if stroke_options["variability"].as_str() == Some("constant") {
            continue; // constant_width_element_outlines
        }
        let points: Vec<[f64; 2]> = e["points"]
            .as_array()
            .expect("points")
            .iter()
            .map(pair)
            .collect();
        let pressures: Vec<f64> = e["pressures"]
            .as_array()
            .expect("pressures")
            .iter()
            .map(f)
            .collect();
        let streamline = stroke_options.get("streamline").map(f);
        let outline = variable_width_outline(
            &points,
            &pressures,
            f(&e["strokeWidth"]),
            streamline,
            e["simulatePressure"].as_bool().expect("simulatePressure"),
        );
        check_outline(id, &outline, &c["outline"]);
        seen += 1;
    }
    assert!(seen >= 30, "{seen} variable-width freedraw cases");
}

/// `getConstantWidthFreedrawOutline` (`shape.ts:1247-1268`): the
/// laser-pointer outline of every constant-width element in
/// elements-freedraw.json (size `strokeWidth * 1.4`, simplify 0, pressure 1).
#[test]
fn constant_width_element_outlines() {
    let mut seen = 0;
    for c in load("elements-freedraw.json") {
        let id = c["id"].as_str().expect("id");
        let e = &c["element"];
        let stroke_options = &e["strokeOptions"];
        if stroke_options["variability"].as_str() != Some("constant") {
            continue;
        }
        let points: Vec<[f64; 2]> = e["points"]
            .as_array()
            .expect("points")
            .iter()
            .map(pair)
            .collect();
        let outline = constant_width_outline(
            &points,
            f(&e["strokeWidth"]),
            stroke_options.get("streamline").map(f),
        );
        check_outline(id, &outline, &c["outline"]);
        seen += 1;
    }
    assert_eq!(seen, 16, "constant-width freedraw cases");
}

#[test]
fn default_streamline_is_upstreams() {
    // common/src/constants.ts:622
    assert_eq!(DEFAULT_STROKE_STREAMLINE, 0.5);
    assert_eq!(variable_width_options(2.0, None, true).streamline, 0.5);
    assert_eq!(variable_width_options(2.0, Some(0.2), true).streamline, 0.2);
}
