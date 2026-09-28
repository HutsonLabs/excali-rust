//! The raster backend's `sketch` fixture is this crate's own output
//! (ex-401): four Excalidraw box shapes (hachure, solid, cross-hatch and
//! zigzag fills; solid, dashed and dotted strokes; roughness 1 and 2; a
//! rounded, rotated, translucent rectangle) built as upstream draws them,
//! `ShapeCache.generateElementShape` (`generate_element_shape`) then
//! roughjs's `RoughCanvas.draw` with round caps and joins
//! (`rough_canvas::draw`, `renderElement.ts:473-495`), each element in the
//! translate / rotate / translate groups and opacity `renderElement.ts`
//! applies (`:1111-1190`).
//!
//! `crates/excali-raster/tests/fixtures/display-lists/sketch.json` holds
//! these items in the fixture vocabulary, so Chrome draws exactly what the
//! port draws. This test fails when the file and the port disagree;
//! `EXCALI_WRITE_RASTER_FIXTURE=1` rewrites the items (then regenerate the
//! Chrome references with `scripts/fixtures/raster-references.sh`).

use std::path::PathBuf;

use excali_core::element::{
    Element, ElementBase, ElementKind, FillStyle, Radians, Roundness, RoundnessType, StrokeStyle,
};
use excali_rough::RoughGenerator;
use excali_scene::display::{DisplayItem, Group, LineCap, LineJoin, Transform};
use excali_scene::rough_canvas::draw;
use excali_scene::shape::{generate_element_shape, RenderConfig};
use serde_json::{json, Value};

#[path = "support/vocabulary.rs"]
mod vocabulary;
use vocabulary::{close, item_json};

const SEED: f64 = 1041657908.0;

#[allow(clippy::too_many_arguments)]
fn element(
    kind: ElementKind,
    (x, y, w, h): (f64, f64, f64, f64),
    stroke: &str,
    background: &str,
    fill: FillStyle,
    stroke_style: StrokeStyle,
    stroke_width: f64,
    roughness: f64,
) -> Element {
    let mut base = ElementBase::new("el", x, y, SEED, 1.0);
    base.width = w;
    base.height = h;
    base.stroke_color = stroke.to_owned();
    base.background_color = background.to_owned();
    base.fill_style = fill;
    base.stroke_style = stroke_style;
    base.stroke_width = stroke_width;
    base.roughness = roughness;
    Element::new(base, kind)
}

fn elements() -> Vec<Element> {
    let mut rounded = element(
        ElementKind::Rectangle,
        (150.0, 104.0, 96.0, 64.0),
        "#f08c00",
        "#ffec99",
        FillStyle::Zigzag,
        StrokeStyle::Solid,
        4.0,
        1.0,
    );
    rounded.base.roundness = Some(Roundness {
        kind: RoundnessType::AdaptiveRadius,
        value: None,
    });
    rounded.base.angle = Radians(0.3);
    rounded.base.opacity = 60.0;
    vec![
        element(
            ElementKind::Rectangle,
            (12.0, 12.0, 120.0, 72.0),
            "#1971c2",
            "#a5d8ff",
            FillStyle::Hachure,
            StrokeStyle::Solid,
            2.0,
            1.0,
        ),
        element(
            ElementKind::Ellipse,
            (150.0, 12.0, 100.0, 72.0),
            "#e03131",
            "#ffc9c9",
            FillStyle::Solid,
            StrokeStyle::Dashed,
            2.0,
            1.0,
        ),
        element(
            ElementKind::Diamond,
            (12.0, 100.0, 116.0, 80.0),
            "#2f9e44",
            "#b2f2bb",
            FillStyle::CrossHatch,
            StrokeStyle::Dotted,
            1.0,
            2.0,
        ),
        rounded,
    ]
}

/// The element's items in the groups `renderElement` draws them in:
/// translate to the centre, rotate, translate back by half the size, and
/// the element's opacity.
fn element_items(el: &Element) -> DisplayItem {
    let drawable =
        generate_element_shape(el, &RoughGenerator::new(), &RenderConfig::default()).unwrap();
    let items = draw(&drawable, LineCap::Round, LineJoin::Round).unwrap();
    let b = &el.base;
    let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    let inner = DisplayItem::Group(Group {
        transform: Transform::translate(-b.width / 2.0, -b.height / 2.0),
        opacity: 1.0,
        clip: None,
        items,
    });
    let rotated = DisplayItem::Group(Group {
        transform: Transform::rotate(b.angle.0),
        opacity: 1.0,
        clip: None,
        items: vec![inner],
    });
    DisplayItem::Group(Group {
        transform: Transform::translate(cx, cy),
        opacity: b.opacity / 100.0,
        clip: None,
        items: vec![rotated],
    })
}

fn sketch_items() -> Vec<Value> {
    let mut items = vec![json!({
        "type": "fill",
        "color": "#ffffff",
        "path": [["rect", 0, 0, 262, 192]],
    })];
    items.extend(elements().iter().map(|e| item_json(&element_items(e))));
    items
}

fn fixture_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../excali-raster/tests/fixtures/display-lists/sketch.json")
}

/// The file's layout: the header keys one per line, then one item per
/// line, so a regenerated fixture diffs item by item.
fn write_fixture(items: &[Value]) {
    let header = [
        ("description", json!("Excalidraw box shapes as the port draws them (excali-scene tests/raster_fixture.rs): rough.js hachure, solid, cross-hatch and zigzag fills, solid, dashed and dotted strokes with round caps and joins, roughness 1 and 2, and a rounded rectangle rotated 0.3 rad at 60% opacity, over a white background")),
        ("width", json!(262)),
        ("height", json!(192)),
    ];
    let existing: Option<Value> = std::fs::read_to_string(fixture_file())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let tolerance = existing
        .as_ref()
        .and_then(|v| v.get("tolerance").cloned())
        .unwrap_or(json!({"channel": 0, "pixels": 0}));
    let mut out = String::from("{\n");
    for (k, v) in header {
        out.push_str(&format!(" {}: {},\n", json!(k), v));
    }
    out.push_str(&format!(" \"tolerance\": {},\n", tolerance));
    if let Some(note) = existing.as_ref().and_then(|v| v.get("toleranceNote")) {
        out.push_str(&format!(" \"toleranceNote\": {},\n", note));
    }
    out.push_str(" \"items\": [\n");
    let lines: Vec<String> = items.iter().map(|i| format!("  {i}")).collect();
    out.push_str(&lines.join(",\n"));
    out.push_str("\n ]\n}\n");
    std::fs::write(fixture_file(), out).unwrap();
}

#[test]
fn the_sketch_fixture_is_the_ports_output() {
    let items = sketch_items();
    if std::env::var_os("EXCALI_WRITE_RASTER_FIXTURE").is_some() {
        write_fixture(&items);
    }
    let text = std::fs::read_to_string(fixture_file())
        .expect("sketch.json exists (EXCALI_WRITE_RASTER_FIXTURE=1 writes it)");
    let file: Value = serde_json::from_str(&text).unwrap();
    if let Err(at) = close(&file["items"], &Value::Array(items), "items") {
        panic!(
            "sketch.json is stale at {at}: EXCALI_WRITE_RASTER_FIXTURE=1 cargo test -p excali-scene --test raster_fixture, then scripts/fixtures/raster-references.sh"
        );
    }
    // Every stroke is round-capped and round-joined, as upstream sets the
    // context before rc.draw.
    let strokes = text.matches("\"type\":\"stroke\"").count();
    assert!(strokes >= 4, "{strokes} strokes: at least one per element");
    assert_eq!(strokes, text.matches("\"cap\":\"round\"").count());
    assert_eq!(strokes, text.matches("\"join\":\"round\"").count());
}

#[test]
fn the_comparison_allows_last_bits_not_changes() {
    let a = json!({"p": [["M", 1.0, 2.5]], "c": "#fff"});
    let ulp = f64::from_bits(2.5f64.to_bits() + 3);
    assert!(close(&a, &json!({"p": [["M", 1.0, ulp]], "c": "#fff"}), "").is_ok());
    assert!(close(&a, &json!({"p": [["M", 1.0, 2.5001]], "c": "#fff"}), "").is_err());
    assert!(close(&a, &json!({"p": [["L", 1.0, 2.5]], "c": "#fff"}), "").is_err());
    assert!(close(&a, &json!({"p": [["M", 1.0]], "c": "#fff"}), "").is_err());
}
