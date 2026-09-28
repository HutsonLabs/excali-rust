//! ex-206: the roughr evaluation and the ADR-003 result it backs.
//!
//! The comparison helpers are pinned to rough.js and upstream semantics
//! (`Number.prototype.toFixed`, rough.js 4.6.4's resolved options), the
//! committed `report.json` must be what a fresh run over `goldens/` gives,
//! and ADR-003 must quote that report: the match rate, every file's count,
//! every divergence and the recommendation the ADR's own rule gives.

use std::path::{Path, PathBuf};

use roughr::core::FillStyle;
use roughr_eval::{
    classify, evaluate, first_difference, recommend, report_name, roughr_is_park_miller,
    roughr_options, run, to_fixed, Op, Precision, Set, DIVERGENCES, FILES,
};
use serde_json::{json, Value};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

// ---------------------------------------------------------------------------
// Two decimals, as upstream's SVG export writes them

#[test]
fn to_fixed_is_number_to_fixed() {
    // roughjs 4.6.4 opsToPath writes `+d.toFixed(fixedDecimals)`, and
    // upstream's SVG export passes fixedDecimalPlaceDigits 2
    // (packages/excalidraw/renderer/staticSvgScene.ts:71).
    // toFixed picks the larger n on an exact tie, on the magnitude.
    assert_eq!(to_fixed(0.125, 2), 0.13);
    assert_eq!(to_fixed(-0.125, 2), -0.13);
    assert_eq!(to_fixed(0.375, 2), 0.38);
    // Not ties: the doubles are below the decimal midpoint.
    assert_eq!(to_fixed(1.005, 2), 1.0);
    assert_eq!(to_fixed(2.675, 2), 2.67);
    assert_eq!(to_fixed(1.2345, 2), 1.23);
    assert_eq!(to_fixed(123.456, 2), 123.46);
    assert_eq!(to_fixed(-7.999, 2), -8.0);
    assert_eq!(to_fixed(1e-9, 2), 0.0);
    assert_eq!(to_fixed(0.5, 2), 0.5);
}

// ---------------------------------------------------------------------------
// Options: what an adapter from rough.js options to roughr would pass

fn resolved(extra: Value) -> Value {
    // A drawable's `options` as the goldens record them: rough.js 4.6.4's
    // defaultOptions merged with the call's options.
    let mut o = json!({
        "maxRandomnessOffset": 2, "roughness": 1, "bowing": 1, "stroke": "#000",
        "strokeWidth": 1, "curveTightness": 0, "curveFitting": 0.95,
        "curveStepCount": 9, "fillStyle": "hachure", "fillWeight": -1,
        "hachureAngle": -41, "hachureGap": -1, "dashOffset": -1, "dashGap": -1,
        "zigzagOffset": -1, "seed": 0, "disableMultiStroke": false,
        "disableMultiStrokeFill": false, "preserveVertices": false,
        "fillShapeRoughnessGain": 0.8
    });
    for (k, v) in extra.as_object().expect("object") {
        o[k] = v.clone();
    }
    o
}

#[test]
fn options_carry_rough_js_resolved_values() {
    let o = roughr_options(&resolved(json!({
        "seed": 7, "roughness": 2, "strokeWidth": 4, "fill": "#a5d8ff",
        "fillStyle": "cross-hatch", "fillWeight": 2, "hachureGap": 16,
        "hachureAngle": 60, "simplification": 0.5, "preserveVertices": true,
        "disableMultiStroke": true, "disableMultiStrokeFill": true,
    })))
    .expect("maps");
    // roughr's own default bowing is 2; rough.js's is 1 and must be passed.
    assert_eq!(o.bowing, Some(1.0));
    assert_eq!(o.max_randomness_offset, Some(2.0));
    assert_eq!(o.roughness, Some(2.0));
    assert_eq!(o.seed, Some(7));
    assert_eq!(o.stroke_width, Some(4.0));
    assert_eq!(o.curve_fitting, Some(0.95));
    assert_eq!(o.curve_step_count, Some(9.0));
    assert_eq!(o.curve_tightness, Some(0.0));
    assert_eq!(o.fill_weight, Some(2.0));
    assert_eq!(o.hachure_gap, Some(16.0));
    assert_eq!(o.hachure_angle, Some(60.0));
    assert_eq!(o.simplification, Some(0.5));
    assert_eq!(o.dash_offset, Some(-1.0));
    assert_eq!(o.dash_gap, Some(-1.0));
    assert_eq!(o.zigzag_offset, Some(-1.0));
    assert_eq!(o.preserve_vertices, Some(true));
    assert_eq!(o.disable_multi_stroke, Some(true));
    assert_eq!(o.disable_multi_stroke_fill, Some(true));
    assert!(o.fill.is_some());
    assert!(o.stroke.is_some());
    assert_eq!(o.fill_style, Some(FillStyle::CrossHatch));
}

#[test]
fn options_map_every_fill_style_and_the_none_sentinels() {
    for (name, style) in [
        ("hachure", FillStyle::Hachure),
        ("solid", FillStyle::Solid),
        ("zigzag", FillStyle::ZigZag),
        ("cross-hatch", FillStyle::CrossHatch),
        ("dots", FillStyle::Dots),
        ("dashed", FillStyle::Dashed),
        ("zigzag-line", FillStyle::ZigZagLine),
        // rough.js's getFiller falls back to hachure for unknown names.
        ("sketchy", FillStyle::Hachure),
    ] {
        let o = roughr_options(&resolved(json!({"fill": "red", "fillStyle": name}))).unwrap();
        assert_eq!(o.fill_style, Some(style), "{name}");
    }
    // roughr has no "none" sentinel; no colour is None.
    let o = roughr_options(&resolved(json!({"stroke": "none"}))).unwrap();
    assert!(o.stroke.is_none());
    for fill in ["none", "transparent"] {
        let o = roughr_options(&resolved(json!({"fill": fill}))).unwrap();
        assert!(o.fill.is_none(), "{fill}");
    }
    let o = roughr_options(&resolved(json!({}))).unwrap();
    assert!(o.fill.is_none());
    assert!(o.simplification.is_none() || o.simplification == Some(1.0));
    // An option rough.js 4.6.4 does not have is an error, not silently lost.
    assert!(roughr_options(&resolved(json!({"hachureSpacing": 3}))).is_err());
}

// ---------------------------------------------------------------------------
// Running roughr and comparing op by op

fn set(kind: &str, ops: &[(&str, &[f64])]) -> Set {
    Set {
        kind: kind.to_owned(),
        ops: ops
            .iter()
            .map(|(op, data)| Op {
                op: (*op).to_owned(),
                data: data.to_vec(),
            })
            .collect(),
    }
}

#[test]
fn run_returns_roughr_sets_in_golden_vocabulary() {
    let o = resolved(json!({"seed": 1, "roughness": 0}));
    let sets = run("line", &[json!(0), json!(0), json!(100), json!(0)], &o).expect("runs");
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].kind, "path");
    assert!(sets[0]
        .ops
        .iter()
        .all(|op| ["move", "lineTo", "bcurveTo"].contains(&op.op.as_str())));
    assert_eq!(sets[0].ops[0].op, "move");
    let o = resolved(json!({"seed": 1, "fill": "#a5d8ff", "fillStyle": "solid"}));
    let sets = run(
        "rectangle",
        &[json!(10), json!(10), json!(100), json!(60)],
        &o,
    )
    .expect("runs");
    let kinds: Vec<_> = sets.iter().map(|s| s.kind.as_str()).collect();
    assert_eq!(kinds, ["fillPath", "path"]);
    assert!(run("hexagon", &[], &o).is_err());
}

#[test]
fn first_difference_names_where_and_what() {
    let expected = json!([{"type": "path", "ops": [
        {"op": "move", "data": [1.0, 2.0]},
        {"op": "bcurveTo", "data": [1.0, 2.0, 3.0, 4.0, 5.0, 6.001]}
    ]}]);
    let same = set(
        "path",
        &[
            ("move", &[1.0, 2.0]),
            ("bcurveTo", &[1.0, 2.0, 3.0, 4.0, 5.0, 6.001]),
        ],
    );
    assert_eq!(first_difference(&expected, &[same], Precision::Exact), None);

    let close = set(
        "path",
        &[
            ("move", &[1.0, 2.0]),
            ("bcurveTo", &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        ],
    );
    assert_eq!(
        first_difference(
            &expected,
            std::slice::from_ref(&close),
            Precision::Decimals(2)
        ),
        None
    );
    let d = first_difference(&expected, &[close], Precision::Exact).expect("differs");
    assert!(d.starts_with("value:"), "{d}");
    assert!(d.contains("set 0 (path) op 1 (bcurveTo) data[5]"), "{d}");
    assert!(d.contains("6.001") && d.contains("6"), "{d}");

    // roughr's value goes through the platform's libm (sin, cos, powf), which
    // can differ in the last bit between macOS and Linux; the report quotes it
    // at ten decimals so report.json is the same on both.
    let libm = set(
        "path",
        &[
            ("move", &[1.0, 2.0]),
            ("bcurveTo", &[1.0, 2.0, 3.0, 4.0, 5.0, 165.74680675874848]),
        ],
    );
    let d = first_difference(&expected, &[libm], Precision::Decimals(2)).unwrap();
    assert!(d.ends_with("expected 6.001 actual 165.7468067587"), "{d}");

    let short = set("path", &[("move", &[1.0, 2.0])]);
    let d = first_difference(&expected, &[short], Precision::Decimals(2)).unwrap();
    assert!(d.starts_with("op count:"), "{d}");
    assert!(d.contains("expected 2") && d.contains("actual 1"), "{d}");

    let kind = set("fillPath", &[]);
    let d = first_difference(&expected, &[kind], Precision::Decimals(2)).unwrap();
    assert!(d.starts_with("set type:"), "{d}");

    let d = first_difference(&expected, &[], Precision::Decimals(2)).unwrap();
    assert!(d.starts_with("set count:"), "{d}");

    let op = set("path", &[("move", &[1.0, 2.0]), ("lineTo", &[5.0, 6.0])]);
    let d = first_difference(&expected, &[op], Precision::Decimals(2)).unwrap();
    assert!(d.starts_with("op kind:"), "{d}");
}

fn case(method: &str, args: Value, extra: Value) -> Value {
    json!({"id": "t", "method": method, "args": args, "options": {},
           "drawable": {"shape": method, "options": resolved(extra), "sets": []}})
}

#[test]
fn classify_names_the_divergence_behind_a_difference() {
    let rect = json!([0, 0, 10, 10]);
    assert_eq!(
        classify(
            &case("rectangle", rect.clone(), json!({"seed": -5})),
            "error: seed: non-negative integer expected, got -5"
        ),
        ["seed-range"]
    );
    assert_eq!(
        classify(
            &case("rectangle", rect.clone(), json!({"fill": "none"})),
            "set count: expected 2 actual 1"
        ),
        ["fill-sentinel"]
    );
    assert_eq!(
        classify(
            &case("rectangle", rect.clone(), json!({"fill": "red"})),
            "value: set 0 (fillSketch) op 0 (move) data[0] expected 9.99 actual 10"
        ),
        ["pattern-fill"]
    );
    // A difference within 1e-4 is f32 arithmetic, whatever the set.
    assert_eq!(
        classify(
            &case("ellipse", rect.clone(), json!({})),
            "value: set 0 (path) op 36 (bcurveTo) data[0] expected 696.915006671141 actual 696.914992608567"
        ),
        ["f32"]
    );
    assert_eq!(
        classify(
            &case("ellipse", rect.clone(), json!({})),
            "op count: set 0 (path) expected 38 actual 37"
        ),
        ["f32"]
    );
    assert_eq!(
        classify(
            &case("curve", json!([[[0, 0], [30, 30]]]), json!({})),
            "value: set 0 (path) op 2 (move) data[0] expected -1.35 actual 0.63"
        ),
        ["curve-reseed"]
    );
    let d = "op count: set 0 (path) expected 8 actual 9";
    assert_eq!(
        classify(&case("path", json!(["M 0 0 L 9 9"]), json!({})), d),
        ["svg-path"]
    );
    for s in [0.0, 0.5] {
        assert_eq!(
            classify(
                &case("path", json!(["M 0 0 L 9 9"]), json!({"simplification": s})),
                d
            ),
            ["path-simplification"],
            "simplification {s}"
        );
    }
    let fill = "value: set 0 (fillPath) op 0 (move) data[0] expected 0.1 actual -0.7";
    let solid = json!({"fill": "red", "fillStyle": "solid"});
    assert_eq!(
        classify(&case("path", json!(["M 0 0 L 9 9 Z"]), solid.clone()), fill),
        ["solid-fill-shape"]
    );
    assert_eq!(
        classify(
            &case("curve", json!([[[0, 0], [9, 9], [0, 9]]]), solid.clone()),
            fill
        ),
        ["solid-fill-shape"]
    );
    assert_eq!(
        classify(
            &case("path", json!(["M 0 0 L 9 9 Z M 20 20 L 30 30 Z"]), solid),
            fill
        ),
        ["path-draw-order"]
    );
    // Nothing a divergence explains: no attribution, which the report
    // tests reject.
    assert!(classify(
        &case("line", rect, json!({})),
        "set type: set 0 expected path actual fillPath"
    )
    .is_empty());
}

#[test]
fn the_tests_build_roughr_as_published() {
    // fork.py builds src/ only against park-miller.patch; these tests always
    // see roughr 0.14.0 from crates.io.
    assert!(!roughr_is_park_miller());
    assert_eq!(report_name(), "report.json");
}

#[test]
fn recommendation_follows_the_adr_rule() {
    // ADR-003: adopt at 100 %, fork at a small, fixable divergence, port
    // otherwise.
    assert_eq!(recommend(100.0), "adopt");
    assert_eq!(recommend(99.0), "fork");
    assert_eq!(recommend(90.0), "fork");
    assert_eq!(recommend(89.9), "port");
    assert_eq!(recommend(0.0), "port");
}

// ---------------------------------------------------------------------------
// The report and the ADR

#[test]
fn evaluation_covers_every_rough_js_golden() {
    let manifest: Value = serde_json::from_str(&read("goldens/manifest.json")).unwrap();
    let mut expected = 0;
    for f in manifest["files"].as_array().unwrap() {
        let name = f["name"].as_str().unwrap();
        if name == "random.json" || name.starts_with("rough-") {
            assert!(
                FILES.contains(&name),
                "{name} is a rough.js golden the spike skips"
            );
            expected += f["cases"].as_u64().unwrap() as usize;
        }
    }
    let report = evaluate(&root());
    assert_eq!(report.total().cases, expected);
    // random, primitives, generator, fills, options, strokes (ex-205).
    assert_eq!(expected, 7 + 117 + 228 + 224 + 140 + 630);
    // Every case either matches or names its first difference.
    for c in &report.cases {
        assert_eq!(c.svg, c.difference.is_none(), "{} {}", c.file, c.id);
    }
}

#[test]
fn evaluation_replays_excalidraws_stroke_styles() {
    // rough-strokes.json (ex-205): Excalidraw's solid, dashed and dotted
    // strokes, with disableMultiStroke and preserveVertices as upstream's
    // generateRoughOptions sets them (shape.ts:195-260).
    let report = evaluate(&root());
    let strokes: Vec<_> = report
        .cases
        .iter()
        .filter(|c| c.file == "rough-strokes.json")
        .collect();
    assert_eq!(strokes.len(), 630);
    for style in ["solid/", "dashed/", "dotted/"] {
        assert_eq!(
            strokes.iter().filter(|c| c.id.starts_with(style)).count(),
            210,
            "{style}"
        );
    }
    // Every case ran: the resolved options map to roughr (strokeLineDash,
    // disableMultiStroke, preserveVertices included), none is an error.
    for c in &strokes {
        assert!(
            !c.difference.as_deref().unwrap_or("").starts_with("error:"),
            "{} {:?}",
            c.id,
            c.difference
        );
    }
    let o = roughr_options(&json!({
        "strokeLineDash": [8, 9], "disableMultiStroke": true, "preserveVertices": true,
    }))
    .unwrap();
    assert_eq!(o.stroke_line_dash, Some(vec![8.0, 9.0]));
    assert_eq!(o.disable_multi_stroke, Some(true));
    assert_eq!(o.preserve_vertices, Some(true));
}

#[test]
fn committed_report_is_a_fresh_run() {
    let fresh = evaluate(&root()).to_json();
    let committed: Value =
        serde_json::from_str(&read("tools/roughr-eval/report.json")).expect("report.json parses");
    assert!(
        fresh == committed,
        "tools/roughr-eval/report.json is stale: run \
         cargo run --manifest-path tools/roughr-eval/Cargo.toml -- --write"
    );
}

#[test]
fn every_mismatch_is_attributed_to_a_listed_divergence() {
    let report = evaluate(&root());
    for c in report.cases.iter().filter(|c| !c.svg) {
        assert!(
            !c.divergences.is_empty(),
            "{} {}: no divergence explains it ({:?})",
            c.file,
            c.id,
            c.difference
        );
        for d in &c.divergences {
            assert!(DIVERGENCES.iter().any(|x| x.id == *d), "{d}");
        }
    }
}

fn fork_report() -> Value {
    serde_json::from_str(&read("tools/roughr-eval/report-fork.json"))
        .expect("report-fork.json parses (python3 tools/roughr-eval/fork.py --write)")
}

#[test]
fn fork_report_is_roughr_with_rough_js_random() {
    // report-fork.json is checked for freshness by `fork.py --check` in CI
    // (it needs the patched build); here: it is the patched roughr, and
    // the attribution it drives is consistent.
    let fork = fork_report();
    assert_eq!(fork["roughr"], "0.14.0 + park-miller.patch");
    let files = fork["files"].as_array().unwrap();
    assert_eq!(files[0]["name"], "random.json");
    assert_eq!(files[0]["svg"], 7, "the patch reproduces Random.next()");
    assert_eq!(files[0]["exact"], 7);
    let fork_total = fork["total"]["cases"].as_u64().unwrap() as usize;
    let fork_svg = fork["total"]["svg"].as_u64().unwrap() as usize;
    let fork_mismatches: Vec<(String, String)> = fork["mismatches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            assert!(
                !m["divergences"].as_array().unwrap().is_empty(),
                "fork mismatch {m} has no divergence"
            );
            assert!(!m["divergences"].as_array().unwrap().contains(&json!("rng")));
            (
                m["file"].as_str().unwrap().to_owned(),
                m["id"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(fork_total - fork_mismatches.len(), fork_svg);

    let report = evaluate(&root());
    assert_eq!(report.total().cases, fork_total);
    // Swapping the generator never loses a match ...
    for c in report.cases.iter().filter(|c| c.svg) {
        assert!(
            !fork_mismatches.contains(&(c.file.clone(), c.id.clone())),
            "{} {} matches as published but not with the patch",
            c.file,
            c.id
        );
    }
    // ... and `rng` is exactly what it gains.
    let rng = report
        .cases
        .iter()
        .filter(|c| c.divergences == ["rng"])
        .count();
    assert_eq!(rng, fork_svg - report.total().svg);
    // The other divergences are the fork's remaining mismatches, case by case.
    for c in report
        .cases
        .iter()
        .filter(|c| !c.svg && c.divergences != ["rng"])
    {
        let m = fork["mismatches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["file"] == c.file.as_str() && m["id"] == c.id.as_str())
            .expect("a non-rng mismatch is a fork mismatch");
        assert_eq!(
            m["divergences"],
            json!(c.divergences),
            "{} {}",
            c.file,
            c.id
        );
    }
}

#[test]
fn adr_003_quotes_the_report() {
    let adr = read("site/content/decisions/adr-003-sketch-renderer.md");
    let report = evaluate(&root());
    let fork = fork_report();
    let total = report.total();
    assert!(
        adr.contains("**Status.** Accepted"),
        "ADR-003 is decided by ex-206"
    );
    assert!(adr.contains("## Result"), "ADR-003 has the ex-206 result");
    let headline = format!(
        "{} of {} goldens ({})",
        total.svg,
        total.cases,
        report.percent_svg_text()
    );
    assert!(adr.contains(&headline), "ADR-003 must say: {headline}");
    let mismatches = format!("Every one of the {} mismatches", total.cases - total.svg);
    assert!(adr.contains(&mismatches), "ADR-003 must say: {mismatches}");
    let empty = format!("{} of them draw nothing", report.matched_without_ops());
    assert!(adr.contains(&empty), "ADR-003 must say: {empty}");
    let fork_headline = format!(
        "{} of {} goldens ({})",
        fork["total"]["svg"],
        fork["total"]["cases"],
        fork["total"]["percent_svg"].as_str().unwrap()
    );
    assert!(
        adr.contains(&fork_headline),
        "ADR-003 must say: {fork_headline}"
    );
    for (f, ff) in report.files().iter().zip(fork["files"].as_array().unwrap()) {
        assert_eq!(ff["name"], f.name.as_str());
        let row = format!(
            "| `{}` | {} | {} | {} | {} |",
            f.name, f.cases, f.exact, f.svg, ff["svg"]
        );
        assert!(adr.contains(&row), "ADR-003 file table needs: {row}");
    }
    let row = format!(
        "| total | {} | {} | {} | {} |",
        total.cases, total.exact, total.svg, fork["total"]["svg"]
    );
    assert!(adr.contains(&row), "ADR-003 file table needs: {row}");
    for d in DIVERGENCES {
        let n = report
            .cases
            .iter()
            .filter(|c| c.divergences.contains(&d.id))
            .count();
        let row = format!("| `{}` | {} |", d.id, n);
        assert!(adr.contains(&row), "ADR-003 divergence list needs: {row}");
    }
    let rec = recommend(total.percent_svg());
    assert!(
        adr.contains(&format!("Recommendation: **{rec}**")),
        "ADR-003 must recommend {rec}"
    );
    assert!(
        adr.contains("tools/roughr-eval"),
        "ADR-003 names the evaluation"
    );
}
