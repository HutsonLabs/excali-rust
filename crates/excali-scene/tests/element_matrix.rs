//! The M2 golden matrix (ex-g202): every element type x fill style x
//! roughness 0, 1, 2 x seed 1, 7, 1041657908, against upstream.
//!
//! `goldens/elements-matrix.json` is written by `tools/goldens/generate.mjs`
//! from upstream's own `ShapeCache.generateElementShape`
//! (`packages/element/src/shape.ts:120-165`, `_generateElementShape` at
//! `shape.ts:770-1015` at the pinned commit). Each variant below carries a
//! background colour and is drawn at every `fillStyle` (hachure,
//! cross-hatch, zigzag, solid), roughness and seed:
//!
//! - rectangle (sharp, and roundness 3), diamond (sharp, and roundness 2),
//!   ellipse, iframe, embeddable;
//! - line (a closed loop, the loop curved, a polygon line);
//! - arrow (sharp and curved, with a filled triangle end head);
//! - freedraw (a closed loop, variable and constant width);
//! - text, image, frame, magicframe and stickynote, which upstream gives no
//!   rough shape (`shape.ts:996-1006`).
//!
//! Every shape of every case is compared with the port's exactly: box
//! shapes, lines, arrows and freedraw fills, ellipses and arrowheads too
//! (they go through `Math.cos` and `Math.sin`, which `excali_math::js`
//! computes as V8 does on every platform, ex-009); the freedraw stroke's
//! `svgPath` byte for byte. The shapeless types must
//! have no shape in the golden and none from any of the port's builders.
//! A matrix cell with no case fails the coverage test.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use excali_core::element::Element;
use excali_rough::goldens::{ActualShape, Report, Tolerance};
use excali_rough::RoughGenerator;
use excali_scene::shape::{
    generate_element_shape, generate_freedraw_shapes, generate_linear_element_shapes,
    FreedrawShape, RenderConfig, ShapeError, Theme,
};
use serde_json::Value;

const FILE: &str = "elements-matrix.json";

/// Every element variant of the matrix, as the case ids name them.
const VARIANTS: [&str; 19] = [
    "rectangle",
    "rectangle-round",
    "diamond",
    "diamond-round",
    "ellipse",
    "iframe",
    "embeddable",
    "line-loop",
    "line-loop-curve",
    "line-polygon",
    "arrow",
    "arrow-curve",
    "freedraw-variable",
    "freedraw-constant",
    "text",
    "image",
    "frame",
    "magicframe",
    "stickynote",
];

/// The element types `_generateElementShape` gives no rough shape.
const SHAPELESS: [&str; 5] = ["text", "image", "frame", "magicframe", "stickynote"];

/// Excalidraw's fill styles (`element/src/types.ts:19`).
const FILL_STYLES: [&str; 4] = ["hachure", "cross-hatch", "zigzag", "solid"];
/// Architect, artist, cartoonist.
const ROUGHNESSES: [u64; 3] = [0, 1, 2];
const SEEDS: [u64; 3] = [1, 7, 1041657908];

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

fn cases(doc: &Value) -> &[Value] {
    doc["cases"].as_array().expect("cases")
}

/// The variant a case's element is, read from the element itself (type,
/// roundness, polygon, stroke variability), not from its id.
fn variant_of(el: &Value) -> String {
    let ty = el["type"].as_str().expect("type");
    let round = !el["roundness"].is_null();
    match ty {
        "rectangle" | "diamond" if round => format!("{ty}-round"),
        "line" if el["polygon"] == true => "line-polygon".to_owned(),
        "line" if round => "line-loop-curve".to_owned(),
        "line" => "line-loop".to_owned(),
        "arrow" if round => "arrow-curve".to_owned(),
        "freedraw" => format!(
            "freedraw-{}",
            el["strokeOptions"]["variability"]
                .as_str()
                .expect("variability")
        ),
        _ => ty.to_owned(),
    }
}

/// `(variant, fillStyle, roughness, seed)` of a case, from its element.
fn cell(c: &Value) -> (String, String, u64, u64) {
    let el = &c["element"];
    (
        variant_of(el),
        el["fillStyle"].as_str().expect("fillStyle").to_owned(),
        el["roughness"].as_u64().expect("roughness"),
        el["seed"].as_u64().expect("seed"),
    )
}

fn render_config(v: &Value) -> (bool, String, HashMap<String, bool>, Theme) {
    let embeds = v["validatedEmbeds"]
        .as_array()
        .expect("validatedEmbeds")
        .iter()
        .map(|id| (id.as_str().expect("id").to_owned(), true))
        .collect();
    let theme = match v["theme"].as_str().expect("theme") {
        "dark" => Theme::Dark,
        "light" => Theme::Light,
        other => panic!("theme {other}"),
    };
    (
        v["isExporting"].as_bool().expect("isExporting"),
        v["canvasBackgroundColor"]
            .as_str()
            .expect("canvasBackgroundColor")
            .to_owned(),
        embeds,
        theme,
    )
}

#[test]
fn the_matrix_has_a_case_for_every_variant_fill_roughness_and_seed() {
    let doc = load(FILE);
    let mut seen = BTreeMap::<(String, String, u64, u64), usize>::new();
    for c in cases(&doc) {
        let id = c["id"].as_str().expect("id");
        let key = cell(c);
        // the id names the cell, and the element agrees with it
        let (variant, fill, roughness, seed) = &key;
        assert_eq!(
            id,
            format!("matrix/{variant}/{fill}-r{roughness}-seed{seed}"),
            "{id}: the id does not name the element's cell"
        );
        assert!(VARIANTS.contains(&variant.as_str()), "{id}: variant");
        assert!(FILL_STYLES.contains(&fill.as_str()), "{id}: fillStyle");
        assert!(ROUGHNESSES.contains(roughness), "{id}: roughness");
        assert!(SEEDS.contains(seed), "{id}: seed");
        let background = c["element"]["backgroundColor"]
            .as_str()
            .expect("backgroundColor");
        assert_ne!(background, "transparent", "{id}: no background colour");
        *seen.entry(key).or_default() += 1;
    }
    let mut missing = Vec::new();
    for variant in VARIANTS {
        for fill in FILL_STYLES {
            for roughness in ROUGHNESSES {
                for seed in SEEDS {
                    let key = (variant.to_owned(), fill.to_owned(), roughness, seed);
                    match seen.get(&key) {
                        None => missing.push(format!("{variant} {fill} r{roughness} seed {seed}")),
                        Some(&n) => assert_eq!(n, 1, "{key:?}: {n} cases"),
                    }
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "{} matrix cells have no case:\n{}",
        missing.len(),
        missing.join("\n")
    );
    assert_eq!(seen.len(), VARIANTS.len() * 4 * 3 * 3);
}

#[test]
fn every_matrix_case_matches_upstream() {
    let doc = load(FILE);
    let generator = RoughGenerator::new();
    let mut report = Report::new(FILE);
    let mut per_type = BTreeMap::<String, usize>::new();
    let mut shapes_compared = 0;
    for c in cases(&doc) {
        let id = c["id"].as_str().expect("id");
        let raw = c["element"].as_object().expect("element").clone();
        let ty = raw["type"].as_str().expect("type").to_owned();
        let el = Element::from_map(raw).unwrap_or_else(|e| panic!("{id}: {e}"));
        let (is_exporting, background, embeds, theme) = render_config(&c["renderConfig"]);
        let config = RenderConfig {
            is_exporting,
            canvas_background_color: &background,
            canvas_background_unfiltered: false,
            embeds_validation_status: Some(&embeds),
            theme,
        };
        let expected = c["shapes"].as_array().expect("shapes");
        match ty.as_str() {
            "rectangle" | "iframe" | "embeddable" | "diamond" | "ellipse" => {
                let drawable = generate_element_shape(&el, &generator, &config)
                    .unwrap_or_else(|e| panic!("{id}: {e}"));
                report.element(c, &[ActualShape::Rough(&drawable)], Tolerance::Exact);
                shapes_compared += 1;
            }
            "line" | "arrow" => {
                let actual = generate_linear_element_shapes(&el, &generator, &config)
                    .unwrap_or_else(|e| panic!("{id}: {e}"));
                assert_eq!(
                    expected.len(),
                    actual.len(),
                    "{id}: {} shapes upstream, {} in the port",
                    expected.len(),
                    actual.len()
                );
                if ty == "line" {
                    assert_eq!(actual.len(), 1, "{id}: a line is its body alone");
                } else {
                    assert!(actual.len() >= 2, "{id}: the body and its end head");
                }
                // the body and the heads after it, exactly
                for (i, (e, a)) in expected.iter().zip(&actual).enumerate() {
                    assert_eq!(e["type"], "rough", "{id}: shape {i}");
                    report.drawable(c, &e["drawable"], a, Tolerance::Exact);
                    shapes_compared += 1;
                }
            }
            "freedraw" => {
                let shapes = generate_freedraw_shapes(&el, &generator, &config)
                    .unwrap_or_else(|e| panic!("{id}: {e}"));
                // a closed loop: the rough fill, then the stroke
                assert_eq!(shapes.len(), 2, "{id}: fill and stroke");
                assert!(matches!(shapes[0], FreedrawShape::Rough(_)), "{id}");
                assert!(matches!(shapes[1], FreedrawShape::SvgPath(_)), "{id}");
                let actual: Vec<ActualShape<'_>> = shapes
                    .iter()
                    .map(|s| match s {
                        FreedrawShape::Rough(d) => ActualShape::Rough(d),
                        FreedrawShape::SvgPath(d) => ActualShape::SvgPath(d),
                    })
                    .collect();
                report.element(c, &actual, Tolerance::Exact);
                shapes_compared += actual.len();
            }
            shapeless if SHAPELESS.contains(&shapeless) => {
                assert!(expected.is_empty(), "{id}: upstream gives {ty} no shape");
                let t = el.element_type();
                assert_eq!(
                    generate_element_shape(&el, &generator, &config),
                    Err(ShapeError::NotABoxShape(t)),
                    "{id}"
                );
                assert_eq!(
                    generate_linear_element_shapes(&el, &generator, &config),
                    Err(ShapeError::NotALinearShape(t)),
                    "{id}"
                );
                assert_eq!(
                    generate_freedraw_shapes(&el, &generator, &config),
                    Err(ShapeError::NotAFreedraw(t)),
                    "{id}"
                );
                report.element(c, &[], Tolerance::Exact);
            }
            other => panic!("{id}: element type {other} is not in the matrix"),
        }
        *per_type.entry(ty).or_default() += 1;
    }
    report.assert_ok();
    // 36 cells per variant; the types with two variants have 72 cases
    let expected_per_type: BTreeMap<String, usize> = [
        ("rectangle", 72),
        ("diamond", 72),
        ("ellipse", 36),
        ("iframe", 36),
        ("embeddable", 36),
        ("line", 108),
        ("arrow", 72),
        ("freedraw", 72),
        ("text", 36),
        ("image", 36),
        ("frame", 36),
        ("magicframe", 36),
        ("stickynote", 36),
    ]
    .into_iter()
    .map(|(t, n)| (t.to_owned(), n))
    .collect();
    assert_eq!(per_type, expected_per_type);
    assert!(
        shapes_compared > 14 * 36,
        "{shapes_compared} shapes compared"
    );
}

/// The matrix is not vacuous: the fill styles reach rough.js as upstream
/// set them (each fillable variant's fill set is the style's filler, solid
/// is a `fillPath`, the patterns `fillSketch`), and the arrows' filled
/// heads carry the stroke colour as their fill.
#[test]
fn the_matrix_fills_are_the_fill_styles_they_name() {
    let doc = load(FILE);
    let mut checked = BTreeSet::new();
    for c in cases(&doc) {
        let id = c["id"].as_str().expect("id");
        let (variant, fill, _, _) = cell(c);
        let shapes = c["shapes"].as_array().expect("shapes");
        let fills: &[&str] = match variant.as_str() {
            "arrow" | "arrow-curve" => {
                // arrow bodies never fill (shape.ts:254-255); the triangle
                // head is filled with the stroke colour, solid
                assert!(shapes[0]["drawable"]["options"]["fill"].is_null(), "{id}");
                let head = &shapes[shapes.len() - 1]["drawable"];
                assert_eq!(head["options"]["fill"], c["element"]["strokeColor"], "{id}");
                checked.insert(variant);
                continue;
            }
            v if SHAPELESS.contains(&v) => continue,
            _ => &["fillSketch", "fillPath"],
        };
        let drawable = &shapes[0]["drawable"];
        assert_eq!(drawable["options"]["fillStyle"], fill.as_str(), "{id}");
        let sets: Vec<&str> = drawable["sets"]
            .as_array()
            .expect("sets")
            .iter()
            .map(|s| s["type"].as_str().expect("set type"))
            .collect();
        let want = if fill == "solid" { fills[1] } else { fills[0] };
        assert!(sets.contains(&want), "{id}: {want} in {sets:?}");
        checked.insert(variant);
    }
    let shaped = VARIANTS.iter().filter(|v| !SHAPELESS.contains(v)).count();
    assert_eq!(checked.len(), shaped);
}
