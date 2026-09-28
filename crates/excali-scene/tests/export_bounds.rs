//! Element bounds and export sizing (ex-406): `getElementAbsoluteCoords`,
//! `getElementBounds` and `getCommonBounds`
//! (`packages/element/src/bounds.ts:84-297, 997-1029`, with
//! `LinearElementEditor.getElementAbsoluteCoords`,
//! `getBoundTextElementPosition` and `getMinMaxXYWithBoundText`,
//! `linearElementEditor.ts:2068-2270`), and what `exportToSvg` computes
//! before it builds the document (`packages/excalidraw/scene/export.ts`):
//! the frame rendering config, the frame name labels and the canvas size.
//!
//! Fixture: `tests/fixtures/export-bounds.json`, upstream's own output at the
//! pinned commit (`tools/goldens/svg-export.mjs`), with text measured at
//! 10 px per UTF-16 code unit.

use excali_core::element::Element;
use excali_scene::bounds::{
    get_common_bounds, get_element_absolute_coords, get_element_bounds, ElementsMap,
};
use excali_scene::export::{
    canvas_size, frame_labels, get_frame_rendering_config, get_root_elements, FrameRendering,
    TextMetrics, DEFAULT_EXPORT_PADDING,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/export-bounds.json")).unwrap()
}

/// Upstream's test text metrics: 10 px per UTF-16 code unit.
struct TenPxPerCodeUnit;

impl TextMetrics for TenPxPerCodeUnit {
    fn measure(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

fn elements(scene: &Value) -> Vec<Element> {
    scene["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
        .collect()
}

/// The fixture's numbers; a non-finite one is its `String()`.
fn numbers(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| match v.as_str() {
            Some("Infinity") => f64::INFINITY,
            Some("-Infinity") => f64::NEG_INFINITY,
            Some("NaN") => f64::NAN,
            _ => v.as_f64().unwrap(),
        })
        .collect()
}

/// Equal as `Object.is` compares numbers: NaN equals NaN.
fn same(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x == y || (x.is_nan() && y.is_nan()))
}

fn frame_rendering(value: &Value) -> Option<FrameRendering> {
    let o = value.as_object()?;
    Some(FrameRendering {
        enabled: o["enabled"].as_bool().unwrap(),
        clip: o["clip"].as_bool().unwrap(),
        name: o["name"].as_bool().unwrap(),
        outline: o["outline"].as_bool().unwrap(),
    })
}

fn scenes() -> Vec<Value> {
    fixture()["scenes"].as_array().unwrap().clone()
}

#[test]
fn the_fixture_has_every_scene() {
    let names: Vec<String> = scenes()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_owned())
        .collect();
    for name in [
        "fixture",
        "shapes",
        "linear",
        "arrow-labels",
        "frames",
        "exporting-frame",
        "frame-rotated-label",
        "empty",
    ] {
        assert!(names.iter().any(|n| n == name), "{name}");
    }
}

#[test]
fn element_absolute_coords_match_upstream() {
    let mut failures = Vec::new();
    let mut count = 0;
    for scene in scenes() {
        let elements = elements(&scene);
        let map = ElementsMap::new(&elements);
        for (element, expected) in elements.iter().zip(scene["coords"].as_array().unwrap()) {
            count += 1;
            let got = get_element_absolute_coords(element, &map, false);
            if !same(&got, &numbers(expected)) {
                failures.push(format!(
                    "{} {}: {got:?} != {expected}",
                    scene["name"], element.base.id
                ));
            }
        }
    }
    assert!(count > 60, "{count} elements");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn element_bounds_match_upstream() {
    let mut failures = Vec::new();
    for scene in scenes() {
        let elements = elements(&scene);
        let map = ElementsMap::new(&elements);
        for (element, expected) in elements.iter().zip(scene["bounds"].as_array().unwrap()) {
            let got = get_element_bounds(element, &map);
            if !same(&got, &numbers(expected)) {
                failures.push(format!(
                    "{} {}: {got:?} != {expected}",
                    scene["name"], element.base.id
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn common_bounds_match_upstream() {
    for scene in scenes() {
        let elements = elements(&scene);
        let refs: Vec<&Element> = elements.iter().collect();
        assert_eq!(
            get_common_bounds(&refs).to_vec(),
            numbers(&scene["commonBounds"]),
            "{}",
            scene["name"]
        );
    }
}

#[test]
fn frame_rendering_config_matches_upstream() {
    for scene in scenes() {
        let elements = elements(&scene);
        let exporting_frame = scene["exportingFrame"]
            .as_str()
            .map(|id| elements.iter().find(|e| e.base.id == id).unwrap());
        let got = get_frame_rendering_config(
            exporting_frame,
            frame_rendering(&scene["appState"]["frameRendering"]),
        );
        assert_eq!(
            Some(got),
            frame_rendering(&scene["frameRendering"]),
            "{}",
            scene["name"]
        );
    }
}

#[test]
fn frame_labels_match_upstream() {
    let mut seen = 0;
    for scene in scenes() {
        let elements = elements(&scene);
        let app_state = &scene["appState"];
        let exporting_frame = scene["exportingFrame"].as_str();
        let config = frame_rendering(&scene["frameRendering"]).unwrap();
        // prepareElementsForRender adds labels only when not exporting a
        // frame and names are rendered (export.ts:156-183)
        let labels = if exporting_frame.is_none() && config.enabled && config.name {
            frame_labels(
                &elements,
                app_state["exportWithDarkMode"].as_bool().unwrap_or(false),
                &TenPxPerCodeUnit,
            )
        } else {
            Vec::new()
        };
        let expected = scene["labels"].as_array().unwrap();
        assert_eq!(labels.len(), expected.len(), "{}", scene["name"]);
        for (label, e) in labels.iter().zip(expected) {
            seen += 1;
            assert_eq!(label.text, e["text"].as_str().unwrap(), "{}", scene["name"]);
            assert_eq!(
                [label.x, label.y, label.width, label.height],
                [
                    e["x"].as_f64().unwrap(),
                    e["y"].as_f64().unwrap(),
                    e["width"].as_f64().unwrap(),
                    e["height"].as_f64().unwrap()
                ],
                "{} {}",
                scene["name"],
                label.text
            );
            assert_eq!(label.font_size, e["fontSize"].as_f64().unwrap());
            assert_eq!(
                f64::from(label.font_family.0),
                e["fontFamily"].as_f64().unwrap()
            );
            assert_eq!(label.line_height, e["lineHeight"].as_f64().unwrap());
            assert_eq!(label.stroke_color, e["strokeColor"].as_str().unwrap());
        }
    }
    assert!(seen >= 20, "{seen} labels");
}

#[test]
fn root_elements_and_canvas_size_match_upstream() {
    for scene in scenes() {
        let name = scene["name"].as_str().unwrap();
        let elements = elements(&scene);
        let app_state = &scene["appState"];
        let exporting_frame = scene["exportingFrame"]
            .as_str()
            .map(|id| elements.iter().find(|e| e.base.id == id).unwrap());
        let config = frame_rendering(&scene["frameRendering"]).unwrap();
        let labels = if exporting_frame.is_none() && config.enabled && config.name {
            frame_labels(
                &elements,
                app_state["exportWithDarkMode"].as_bool().unwrap_or(false),
                &TenPxPerCodeUnit,
            )
        } else {
            Vec::new()
        };
        let roots: Vec<&Element> = match exporting_frame {
            Some(frame) => vec![frame],
            None => get_root_elements(&elements),
        };
        let expected_roots: Vec<&str> = scene["rootElements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            roots.iter().map(|e| e.base.id.as_str()).collect::<Vec<_>>(),
            expected_roots,
            "{name}"
        );
        let padding = if exporting_frame.is_some() {
            0.0
        } else {
            app_state["exportPadding"]
                .as_f64()
                .unwrap_or(DEFAULT_EXPORT_PADDING)
        };
        assert_eq!(
            canvas_size(&roots, &labels, padding).to_vec(),
            numbers(&scene["canvas"]),
            "{name}"
        );
    }
}
