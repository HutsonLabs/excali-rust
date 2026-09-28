//! Freedraw outline to SVG path (ex-215): upstream's `getSvgPathFromStroke`
//! and `TO_FIXED_PRECISION` (`packages/element/src/shape.ts:1314-1344`),
//! checked byte for byte against the `svgPath` shapes of
//! `goldens/elements-freedraw.json` (upstream's own
//! `ShapeCache.generateElementShape`, written by
//! `tools/goldens/generate.mjs`).
//!
//! - Every case's recorded outline (`getFreedrawOutlinePoints`, variable and
//!   constant width) must give the recorded path string.
//! - Every variable-width case must give it from the element too, through
//!   excali-freehand's outline.
//! - The element entry points `getFreedrawOutlinePoints` and
//!   `getFreeDrawSvgPath` (`shape.ts:1187-1191, 1270-1277`) switch on
//!   `strokeOptions.variability`: variable-width cases give the recorded
//!   path, constant-width ones fail with
//!   `FreedrawOutlineError::ConstantWidthNotPorted` until ex-214 ports the
//!   laser pointer (no silent fallback to the variable outline).
//! - Edge cases the fixtures do not reach (exponent notation, `-0`,
//!   non-finite numbers, integers) are pinned to the strings upstream's code
//!   writes in Node 26 (the vectors below).

use std::path::Path;

use excali_core::element::{Element, ElementKind, StrokeVariability};
use excali_core::json::number_to_string;
use excali_scene::freedraw::{
    get_free_draw_svg_path, get_freedraw_outline_points, get_svg_path_from_stroke,
    get_variable_width_freedraw_outline, trim_to_fixed_precision, FreedrawOutlineError,
};
use serde_json::Value;

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

fn outline(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .expect("outline")
        .iter()
        .map(|p| {
            let p = p.as_array().expect("point");
            [p[0].as_f64().expect("x"), p[1].as_f64().expect("y")]
        })
        .collect()
}

/// The `d` of the case's single `svgPath` shape.
fn expected_path(c: &Value) -> &str {
    let paths: Vec<&str> = c["shapes"]
        .as_array()
        .expect("shapes")
        .iter()
        .filter(|s| s["type"] == "svgPath")
        .map(|s| s["d"].as_str().expect("d"))
        .collect();
    assert_eq!(paths.len(), 1, "{}: one svgPath shape", c["id"]);
    paths[0]
}

#[test]
fn recorded_outlines_give_upstream_paths() {
    let cases = load("elements-freedraw.json");
    for c in &cases {
        let id = c["id"].as_str().expect("id");
        let d = get_svg_path_from_stroke(&outline(&c["outline"]));
        assert_eq!(d, expected_path(c), "{id}");
    }
    assert_eq!(cases.len(), 48);
}

/// The case's element in the port's model. A legacy element without
/// strokeOptions: upstream reads `strokeOptions?.variability` (variable) and
/// `strokeOptions?.streamline ?? DEFAULT_STROKE_STREAMLINE`
/// (`shape.ts:1217-1218, 1270-1277`), which is what restore writes
/// (`restoreFreedrawStrokeOptions`, `data/restore.ts:273-287`) and the
/// port's model requires.
fn element(c: &Value) -> Element {
    let id = c["id"].as_str().expect("id");
    let mut raw = c["element"].as_object().expect("element").clone();
    raw.entry("strokeOptions")
        .or_insert_with(|| serde_json::json!({"variability": "variable", "streamline": 0.5}));
    Element::from_map(raw).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn is_constant(el: &Element) -> bool {
    let ElementKind::Freedraw(fields) = &el.kind else {
        panic!("{}: not freedraw", el.base.id);
    };
    fields.stroke_options.variability == StrokeVariability::Constant
}

#[test]
fn variable_width_elements_give_upstream_paths() {
    let mut ran = 0;
    for c in load("elements-freedraw.json") {
        let id = c["id"].as_str().expect("id");
        let el = element(&c);
        if is_constant(&el) {
            continue; // laser-pointer geometry, ex-214
        }
        let points = get_variable_width_freedraw_outline(&el).expect("freedraw");
        let d = get_svg_path_from_stroke(&points);
        assert_eq!(d, expected_path(&c), "{id}");
        ran += 1;
    }
    assert_eq!(ran, 32);
}

/// `getFreeDrawSvgPath` / `getFreedrawOutlinePoints` (`shape.ts:1187-1191,
/// 1270-1277`): the variability switch. Variable width gives upstream's
/// path; constant width is ex-214's laser pointer and must fail loudly
/// rather than fall back to the variable outline.
#[test]
fn element_entry_points_switch_on_variability() {
    let (mut variable, mut constant) = (0, 0);
    for c in load("elements-freedraw.json") {
        let id = c["id"].as_str().expect("id");
        let el = element(&c);
        if is_constant(&el) {
            assert_eq!(
                get_freedraw_outline_points(&el),
                Err(FreedrawOutlineError::ConstantWidthNotPorted),
                "{id}"
            );
            assert_eq!(
                get_free_draw_svg_path(&el),
                Err(FreedrawOutlineError::ConstantWidthNotPorted),
                "{id}"
            );
            constant += 1;
        } else {
            assert_eq!(
                get_freedraw_outline_points(&el).as_deref(),
                Ok(get_variable_width_freedraw_outline(&el)
                    .expect("freedraw")
                    .as_slice()),
                "{id}"
            );
            assert_eq!(
                get_free_draw_svg_path(&el).as_deref(),
                Ok(expected_path(&c)),
                "{id}"
            );
            variable += 1;
        }
    }
    assert_eq!((variable, constant), (32, 16));
}

#[test]
fn element_entry_points_reject_other_types() {
    // A fixture freedraw retyped as a rectangle.
    let c = &load("elements-freedraw.json")[0];
    let mut raw = c["element"].as_object().expect("element").clone();
    raw.insert("type".to_owned(), "rectangle".into());
    for key in [
        "points",
        "pressures",
        "simulatePressure",
        "strokeOptions",
        "lastCommittedPoint",
    ] {
        raw.remove(key);
    }
    let el = Element::from_map(raw).expect("rectangle");
    assert!(matches!(el.kind, ElementKind::Rectangle));
    assert_eq!(
        get_freedraw_outline_points(&el),
        Err(FreedrawOutlineError::NotFreedraw)
    );
    assert_eq!(
        get_free_draw_svg_path(&el),
        Err(FreedrawOutlineError::NotFreedraw)
    );
    assert!(FreedrawOutlineError::ConstantWidthNotPorted
        .to_string()
        .contains("ex-214"));
}

/// Outline coordinates go through `Math.sin`/`Math.cos`, which may differ
/// from libm in the last bit on some platforms (see
/// `crates/excali-freehand/tests/goldens.rs`). The end-to-end byte equality
/// above holds on every platform only where a one-ulp change cannot move
/// the trimmed text (a value at a 0.01 boundary, or a short decimal such as
/// `0.5` that would grow digits). This finds every such coordinate and
/// midpoint of the variable-width fixtures and pins the list.
///
/// There is one: the first point of the `[0, 0, 0.5]` dot,
/// `1 + (8.5 * sin(pi / 4)) / sqrt(2)` = 5.249999999999999 (perfect-freehand's
/// start cap at angle 0, where `cos` is exactly 1 and `sin` exactly 0). It
/// depends on `Math.sin(Math.PI / 4)`, the easeOutSine of pressure 0.5,
/// which V8, fdlibm, glibc and Apple's libm all round to
/// 0.7071067811865475 (one ulp below `Math.SQRT1_2`); the assertion below
/// pins that for the platform running the test.
#[test]
fn variable_width_paths_depend_on_the_last_bit_only_where_pinned() {
    assert_eq!(
        (std::f64::consts::PI / 4.0).sin(),
        0.7071067811865475,
        "sin(pi / 4) on this platform"
    );
    let trim = |x: f64| trim_to_fixed_precision(&number_to_string(x));
    let mut checked = 0;
    let mut sensitive = Vec::new();
    for c in load("elements-freedraw.json") {
        let id = c["id"].as_str().expect("id");
        if c["element"]["strokeOptions"]["variability"] == "constant" {
            continue;
        }
        let pts = outline(&c["outline"]);
        let mut values = Vec::new();
        for (i, p) in pts.iter().enumerate() {
            let q = pts[(i + 1) % pts.len()];
            values.extend([p[0], p[1], (p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0]);
        }
        for v in values {
            if [v.next_up(), v.next_down()]
                .iter()
                .any(|&nudged| trim(nudged) != trim(v))
            {
                sensitive.push(format!("{id} {}", number_to_string(v)));
            }
            checked += 1;
        }
    }
    assert!(checked > 0);
    sensitive.sort();
    sensitive.dedup();
    assert_eq!(
        sensitive,
        ["freedraw/pressure-empty-points 5.249999999999999"]
    );
}

#[test]
fn empty_outline_is_an_empty_path() {
    assert_eq!(get_svg_path_from_stroke(&[]), "");
}

/// `[points, getSvgPathFromStroke(points)]` from upstream's code in Node
/// 26.10.0 (the numbers as `String(x)` writes them).
#[test]
fn edge_cases_match_node() {
    let cases: &[(&[[f64; 2]], &str)] = &[
        (&[[1.0, 2.0]], "M 1,2 Q 1,2 1,2 L 1,2 Z"),
        (
            &[[0.0, 0.0], [10.0, 0.0]],
            "M 0,0 Q 0,0 5,0 10,0 5,0 L 0,0 Z",
        ),
        (
            &[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
            "M 0,0 Q 0,0 5,0 10,0 10,5 10,10 5,5 L 0,0 Z",
        ),
        (
            &[[-0.0, 0.5], [3.0, -0.25]],
            "M 0,0.5 Q 0,0.5 1.5,0.12 3,-0.25 1.5,0.12 L 0,0.5 Z",
        ),
        (
            &[[1.23456, -7.891011], [2.5, 3.999999]],
            "M 1.23,-7.89 Q 1.23,-7.89 1.86,-1.94 2.5,3.99 1.86,-1.94 L 1.23,-7.89 Z",
        ),
        (
            &[[1.5e-7, 2e-7], [-3.25e-9, 1e21]],
            "M 1.5,2e-7 Q 1.5,2e-7 7.33,500000000000000000000 -3.25,1e+21 \
             7.33,500000000000000000000 L 1.5,2e-7 Z",
        ),
        (
            &[[1.2345e21, -5e-324], [123_456_789.987_654_33, 0.1 + 0.2]],
            "M 1.23+21,-5e-324 Q 1.23+21,-5e-324 617250000000061700000,0.15 \
             123456789.98,0.30 617250000000061700000,0.15 L 1.23+21,-5e-324 Z",
        ),
        (
            &[[f64::NAN, f64::INFINITY], [f64::NEG_INFINITY, 1.005]],
            "M NaN,Infinity Q NaN,Infinity NaN,Infinity -Infinity,1.00 NaN,Infinity \
             L NaN,Infinity Z",
        ),
        (
            &[[1e-7, 0.000001], [0.0000015, -0.0000099]],
            "M 1e-7,0.00 Q 1e-7,0.00 8e-7,-0.00 0.00,-0.00 8e-7,-0.00 L 1e-7,0.00 Z",
        ),
        (
            &[[-1e-7, 3.0], [2.5e25, -1.25e-20]],
            "M -1e-7,3 Q -1e-7,3 1.25+25,1.5 2.5+25,-1.25 1.25+25,1.5 L -1e-7,3 Z",
        ),
    ];
    for (points, expected) in cases {
        assert_eq!(get_svg_path_from_stroke(points), *expected, "{points:?}");
    }
}

/// The regex on its own: `$1` keeps an optional space, capital letter,
/// comma and minus, the integer digits, the dot and up to two decimals; the
/// digits, `e` and `-` after them are dropped. Numbers without a dot are
/// left alone. Expected strings from `s.replace(TO_FIXED_PRECISION, "$1")`
/// in Node 26.10.0.
#[test]
fn trimming_follows_the_regex() {
    let cases = [
        ("", ""),
        ("M 1,2", "M 1,2"),
        ("1.23456", "1.23"),
        ("1.", "1."),
        (".5678", ".56"),
        ("-.5678", "-.56"),
        ("M1.2345,-6.789", "M1.23,-6.78"),
        ("1.5e-7", "1.5"),
        ("1.5e+21", "1.5+21"),
        ("1.2345-6.789", "1.23.78"),
        ("1e-7,2.345", "1e-7,2.34"),
        ("a1.999b", "a1.99b"),
        ("\t1.234", "\t1.23"),
        ("L 0.001 Z", "L 0.00 Z"),
        ("1.2e5.678", "1.2.67"),
        ("Q.1234", "Q.12"),
        ("--1.234", "--1.23"),
        (" ,-12.3456e-5", " ,-12.34"),
    ];
    for (input, expected) in cases {
        assert_eq!(trim_to_fixed_precision(input), expected, "{input:?}");
    }
}
