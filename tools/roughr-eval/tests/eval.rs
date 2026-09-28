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
    evaluate, first_difference, recommend, roughr_options, run, to_fixed, Op, Precision, Set,
    DIVERGENCES, FILES,
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
    assert!(sets[0].ops.iter().all(|op| ["move", "lineTo", "bcurveTo"].contains(&op.op.as_str())));
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
        &[("move", &[1.0, 2.0]), ("bcurveTo", &[1.0, 2.0, 3.0, 4.0, 5.0, 6.001])],
    );
    assert_eq!(first_difference(&expected, &[same], Precision::Exact), None);

    let close = set(
        "path",
        &[("move", &[1.0, 2.0]), ("bcurveTo", &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0])],
    );
    assert_eq!(
        first_difference(&expected, std::slice::from_ref(&close), Precision::Decimals(2)),
        None
    );
    let d = first_difference(&expected, &[close], Precision::Exact).expect("differs");
    assert!(d.starts_with("value:"), "{d}");
    assert!(d.contains("set 0 (path) op 1 (bcurveTo) data[5]"), "{d}");
    assert!(d.contains("6.001") && d.contains("6"), "{d}");

    let short = set("path", &[("move", &[1.0, 2.0])]);
    let d = first_difference(&expected, &[short], Precision::Decimals(2)).unwrap();
    assert!(d.starts_with("op count:"), "{d}");
    assert!(d.contains("expected 2") && d.contains("actual 1"), "{d}");

    let kind = set("fillPath", &[]);
    let d = first_difference(&expected, &[kind], Precision::Decimals(2)).unwrap();
    assert!(d.starts_with("set type:"), "{d}");

    let d = first_difference(&expected, &[], Precision::Decimals(2)).unwrap();
    assert!(d.starts_with("set count:"), "{d}");

    let op = set(
        "path",
        &[("move", &[1.0, 2.0]), ("lineTo", &[5.0, 6.0])],
    );
    let d = first_difference(&expected, &[op], Precision::Decimals(2)).unwrap();
    assert!(d.starts_with("op kind:"), "{d}");
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
            assert!(FILES.contains(&name), "{name} is a rough.js golden the spike skips");
            expected += f["cases"].as_u64().unwrap() as usize;
        }
    }
    let report = evaluate(&root().join("goldens"));
    assert_eq!(report.total().cases, expected);
    assert_eq!(expected, 7 + 117 + 228 + 224 + 140);
    // Every case either matches or names its first difference.
    for c in &report.cases {
        assert_eq!(c.svg, c.difference.is_none(), "{} {}", c.file, c.id);
    }
}

#[test]
fn committed_report_is_a_fresh_run() {
    let fresh = evaluate(&root().join("goldens")).to_json();
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
    let report = evaluate(&root().join("goldens"));
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

#[test]
fn adr_003_quotes_the_report() {
    let adr = read("site/content/decisions/adr-003-sketch-renderer.md");
    let report = evaluate(&root().join("goldens"));
    let total = report.total();
    assert!(adr.contains("**Status.** Accepted"), "ADR-003 is decided by ex-206");
    assert!(adr.contains("## Result"), "ADR-003 has the ex-206 result");
    let headline = format!(
        "{} of {} goldens ({})",
        total.svg,
        total.cases,
        report.percent_svg_text()
    );
    assert!(adr.contains(&headline), "ADR-003 must say: {headline}");
    for f in report.files() {
        let row = format!(
            "| `{}` | {} | {} | {} |",
            f.name, f.cases, f.exact, f.svg
        );
        assert!(adr.contains(&row), "ADR-003 file table needs: {row}");
    }
    for d in DIVERGENCES {
        let n = report.cases.iter().filter(|c| c.divergences.contains(&d.id)).count();
        let row = format!("| `{}` | {} |", d.id, n);
        assert!(adr.contains(&row), "ADR-003 divergence list needs: {row}");
    }
    let rec = recommend(total.percent_svg());
    assert!(
        adr.contains(&format!("Recommendation: **{rec}**")),
        "ADR-003 must recommend {rec}"
    );
    assert!(adr.contains("tools/roughr-eval"), "ADR-003 names the evaluation");
}
