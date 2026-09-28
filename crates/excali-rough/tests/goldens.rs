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
//! - `rough-fills.json`: every fill style (hachure, cross-hatch, zigzag,
//!   solid, dashed, zigzag-line) on rectangle, polygon, ellipse and path at
//!   Excalidraw's `fillWeight = strokeWidth / 2` and `hachureGap =
//!   strokeWidth * 4` for stroke widths 1, 2, 4, the seeds x roughness grid,
//!   and the fill edge cases (curve and arc fills, subpaths, concave
//!   polygons, gaps and angles, single fill stroke, fills that are skipped);
//!   every case must run (the ex-204 acceptance set);
//! - `rough-options.json`: option variations with and without fills; every
//!   case must run.
//! - `rough-strokes.json`: Excalidraw's solid, dashed and dotted strokes
//!   (the ex-205 acceptance set): the options are upstream's own
//!   `generateRoughOptions` output for an element, so dashed strokes carry
//!   `[8, 8 + sw]`, dotted `[1.5, 6 + sw]`, and both are single-stroke at
//!   `sw + 0.5`, with `preserveVertices` and `curveFitting` as upstream sets
//!   them; each is drawn with the generator method that element type uses,
//!   at stroke widths 1, 2, 4, roughness 0, 1, 2 and three seeds.
//!
//! Numbers are compared as doubles with `==`, op by op, except where rough.js
//! goes through `Math.sin`/`Math.cos`/`Math.tan`/`Math.asin`/`Math.atan` on
//! arguments that land near a rounding boundary: ellipses, circles, arcs, SVG
//! `A` commands, and the `dashed` and `zigzag-line` fillers, which walk each
//! line along its `atan` slope. Those are not the same function on every
//! platform (upstream's own V8 on arm64 differs from x86_64 V8 and from libm
//! in the last bit; see `tools/goldens/README.md` and
//! `crates/excali-math/tests/goldens.rs`), so they are compared to within
//! `PLATFORM_TOLERANCE`. The number of ops, their kinds and every option
//! must still match exactly, so a draw taken out of order fails either way.
//!
//! Every other fill (solid, hachure, cross-hatch, zigzag) is compared exactly.
//! hachure-fill rotates the polygon by the hachure angle in place and back
//! (`rotatePoints`), which moves shared vertices in the last bits; the
//! goldens record that drift (for example `4.999999999999999` where the
//! unrotated point was `5`), so a port that restored the points, or rotated a
//! repeated vertex once, fails here. The rotation's `cos`/`sin` are those
//! of the hachure angle plus 90 (plus 180 for cross-hatch): 49, 90, 139 and
//! 150 degrees in the exactly compared cases. V8 returns them correctly
//! rounded, and every true value lies at least 0.07 ulp from a rounding
//! midpoint, so any libm within glibc's documented 0.548 ulp bound returns
//! the same doubles (checked on macOS arm64 and Linux arm64 glibc 2.36). Only 42 fill cases (all
//! `dashed` or `zigzag-line`) in `rough-fills.json` and 5 in
//! `rough-options.json` differ from V8 in the last bits.

use std::path::Path;

use excali_rough::goldens::{Report, Tolerance, PLATFORM_TOLERANCE};
use excali_rough::{Drawable, Options, Random, RoughGenerator};
use serde_json::Value;

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
/// goldens use.
fn options(v: &Value) -> Options {
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
            "fill" => o.fill = Some(v.as_str().expect("fill").to_owned()),
            other => panic!("option {other} is not a rough.js 4.6.4 option"),
        }
    }
    o
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
        "curve" => g
            .curve(&points(&a[0]), o)
            .unwrap_or_else(|e| panic!("curve {}: {e}", a[0])),
        "path" => g
            .path(a[0].as_str().expect("path data"), o)
            .unwrap_or_else(|e| panic!("path {}: {e}", a[0])),
        other => panic!("unknown method {other}"),
    }
}

/// Whether rough.js computed this case through trigonometric functions
/// whose last bit is platform-dependent (see the module comment). The
/// hachure-fill rotation is not among them: it is checked exactly.
fn uses_trig(method: &str, args: &[Value], o: &Options) -> bool {
    if o.fill.is_some() && matches!(o.fill_style.as_str(), "dashed" | "zigzag-line") {
        return true;
    }
    match method {
        "ellipse" | "circle" | "arc" => true,
        "path" => args[0]
            .as_str()
            .is_some_and(|d| d.contains('A') || d.contains('a')),
        _ => false,
    }
}

/// Runs every case of `file` through the golden harness; returns how many
/// ran. A difference panics with the case id, the set and op index and the
/// expected and actual numbers (`excali_rough::goldens`).
fn check_file(file: &str) -> usize {
    let doc = load(file);
    let cases = doc["cases"].as_array().expect("cases");
    let mut report = Report::new(file);
    for c in cases {
        let o = options(&c["options"]);
        let method = c["method"].as_str().expect("method");
        let args = c["args"].as_array().expect("args");
        let tolerance = if uses_trig(method, args, &o) {
            Tolerance::Relative(PLATFORM_TOLERANCE)
        } else {
            Tolerance::Exact
        };
        report.drawable(c, &c["drawable"], &call(method, args, &o), tolerance);
    }
    report.assert_ok()
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
    assert_eq!(check_file("rough-primitives.json"), 117);
}

#[test]
fn generator_edge_cases_match() {
    // 76 GENERATOR fixtures (tools/goldens/fixtures.mjs) x roughness 0, 1, 2.
    assert_eq!(check_file("rough-generator.json"), 228);
}

#[test]
fn fills_match_for_every_style_at_excalidraw_weights() {
    // 6 styles x (3 stroke widths x 4 shapes + 3 seeds x 3 roughnesses),
    // plus 98 edge cases (tools/goldens/fixtures.mjs `edge`).
    assert_eq!(check_file("rough-fills.json"), 126 + 98);
}

#[test]
fn option_cases_match() {
    // Option variations on filled rectangles, ellipses and paths and on
    // unfilled curves, and Excalidraw's dashed/dotted stroke rule.
    assert_eq!(check_file("rough-options.json"), 140);
}

#[test]
fn stroke_style_cases_match() {
    // ex-205: solid, dashed and dotted x (sw 1, 2, 4 x roughness 0, 1, 2 at
    // the fixture seed + seeds 1 and 7 at sw 2) x 14 element-shaped calls.
    assert_eq!(check_file("rough-strokes.json"), 3 * (9 + 6) * 14);
}

/// The options of every `rough-strokes.json` case are upstream's
/// `generateRoughOptions`; this restates the rule they must follow
/// (`packages/element/src/shape.ts:168-170, 202-225, 238-240`), so the
/// goldens the port matches are the ones the acceptance names.
#[test]
fn stroke_style_goldens_follow_the_dash_and_width_rule() {
    let doc = load("rough-strokes.json");
    let mut styles = [0usize; 3];
    for c in doc["cases"].as_array().expect("cases") {
        let id = c["id"].as_str().expect("id");
        let el = &c["element"];
        let sw = f(&el["strokeWidth"]);
        let roughness = f(&el["roughness"]);
        let continuous = c["continuousPath"].as_bool().expect("continuousPath");
        let o = options(&c["options"]);
        let (dash, index) = match el["strokeStyle"].as_str().expect("strokeStyle") {
            "solid" => (None, 0),
            "dashed" => (Some(vec![8.0, 8.0 + sw]), 1),
            "dotted" => (Some(vec![1.5, 6.0 + sw]), 2),
            other => panic!("{id}: stroke style {other}"),
        };
        styles[index] += 1;
        let solid = dash.is_none();
        assert_eq!(o.stroke_line_dash, dash, "{id}: strokeLineDash");
        assert_eq!(o.disable_multi_stroke, !solid, "{id}: disableMultiStroke");
        let width = if solid { sw } else { sw + 0.5 };
        assert_eq!(o.stroke_width, width, "{id}: strokeWidth");
        assert_eq!(o.fill_weight, sw / 2.0, "{id}: fillWeight");
        assert_eq!(o.hachure_gap, sw * 4.0, "{id}: hachureGap");
        assert_eq!(o.roughness, roughness, "{id}: roughness (sizes keep it)");
        assert_eq!(
            o.preserve_vertices,
            continuous || roughness < 2.0,
            "{id}: preserveVertices"
        );
        let fitting = if el["type"] == "ellipse" { 1.0 } else { 0.95 };
        assert_eq!(o.curve_fitting, fitting, "{id}: curveFitting");
    }
    assert_eq!(styles, [210, 210, 210]);
}
