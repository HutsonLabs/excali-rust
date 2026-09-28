//! goldens/js-sort.json: the permutation V8's `Array.prototype.sort`
//! (TimSort, third_party/v8/builtins/array-sort.tq) produces when the
//! comparator answers NaN, and the `convexHull` results that depend on it.
//! Written by tools/goldens/generate.mjs from V8 itself (tools/goldens/jssort.mjs).

use std::path::Path;

use excali_math::*;
use serde_json::Value;

fn load() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../goldens/js-sort.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run node tools/goldens/generate.mjs)",
            path.display()
        )
    });
    let doc: Value = serde_json::from_str(&text).expect("js-sort.json parses");
    doc["cases"].as_array().expect("cases").clone()
}

/// A number, or one of the strings "NaN", "Infinity", "-Infinity".
fn num(v: &Value) -> f64 {
    match v {
        Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            other => panic!("unexpected number string {other}"),
        },
        _ => v
            .as_f64()
            .unwrap_or_else(|| panic!("number expected, got {v}")),
    }
}

fn indices(v: &Value) -> Vec<usize> {
    v.as_array()
        .expect("result")
        .iter()
        .map(|i| usize::try_from(i.as_u64().expect("index")).expect("usize"))
        .collect()
}

#[test]
fn sort_permutations_match_v8() {
    let cases: Vec<Value> = load().into_iter().filter(|c| c["kind"] == "sort").collect();
    assert!(cases.len() >= 100, "{} sort cases", cases.len());
    let mut failures = Vec::new();
    let mut with_nan = 0;
    for c in &cases {
        let values: Vec<f64> = c["values"]
            .as_array()
            .expect("values")
            .iter()
            .map(num)
            .collect();
        let mut order: Vec<usize> = (0..values.len()).collect();
        js::sort(&mut order, |&i, &j| values[i] - values[j]);
        if values.iter().any(|v| !v.is_finite()) {
            with_nan += 1;
        }
        if order != indices(&c["result"]) {
            failures.push(format!(
                "{} ({}, n = {})",
                c["id"].as_str().unwrap_or("?"),
                c["pattern"].as_str().unwrap_or("?"),
                values.len()
            ));
        }
    }
    assert!(
        with_nan >= 100,
        "only {with_nan} cases have an inconsistent comparator"
    );
    assert!(
        failures.is_empty(),
        "{} of {} sort cases differ from V8: {}",
        failures.len(),
        cases.len(),
        failures.join(", ")
    );
}

#[test]
fn convex_hulls_with_non_finite_coordinates_match_upstream() {
    let cases: Vec<Value> = load()
        .into_iter()
        .filter(|c| c["kind"] == "convexHull")
        .collect();
    assert!(cases.len() >= 40, "{} convexHull cases", cases.len());
    let mut failures = Vec::new();
    for c in &cases {
        let points: Vec<GlobalPoint> = c["points"]
            .as_array()
            .expect("points")
            .iter()
            .map(|p| point_from(num(&p[0]), num(&p[1])))
            .collect();
        let expected: Vec<GlobalPoint> = indices(&c["result"]).iter().map(|&i| points[i]).collect();
        let actual = convex_hull(&points);
        // Compare bit patterns: NaN coordinates must sit where upstream put them.
        let bits = |ps: &[GlobalPoint]| -> Vec<(u64, u64)> {
            ps.iter().map(|p| (p.x.to_bits(), p.y.to_bits())).collect()
        };
        if bits(&actual) != bits(&expected) {
            failures.push(c["id"].as_str().unwrap_or("?").to_owned());
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} convexHull cases differ from upstream: {}",
        failures.len(),
        cases.len(),
        failures.join(", ")
    );
}
