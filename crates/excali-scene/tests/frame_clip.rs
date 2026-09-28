//! Frame clipping (ex-403): the frame geometry the static scene's
//! `clipElementToFrame` (`packages/excalidraw/renderer/staticScene.ts:
//! 358-393`) decides with, against upstream's own results.
//!
//! - `getElementLineSegments` (`packages/element/src/bounds.ts:299-420`):
//!   every element kind's outline as segments (rectanguloids and diamonds
//!   by their sides and flattened corners, ellipses by 90 chords, lines and
//!   arrows by their flattened rough.js curves or closed polygon, freedraw
//!   by its points, text by its box).
//! - `isElementIntersectingFrame`, `isElementContainingFrame`,
//!   `elementsAreInFrameBounds`, `elementOverlapsWithFrame`,
//!   `getTargetFrame`, `isElementInFrame` and `shouldApplyFrameClip`
//!   (`packages/element/src/frame.ts`).
//! - `frameClip` (`staticScene.ts:165-189`): a `roundRect` of radius
//!   `FRAME_STYLE.radius / zoom` at the frame's corner.
//!
//! Fixture: `tests/fixtures/frame-clip.json`, upstream's output at the
//! pinned commit (`tools/goldens/frame-clip.mjs`) on the frame scenes of
//! the static scene fixture. What those decisions draw, clip by clip, is
//! held by `tests/static_scene.rs` against `static-scene.json`.

use std::collections::{HashMap, HashSet};

use excali_core::element::Element;
use excali_scene::bounds::{get_element_bounds, get_element_line_segments, ElementsMap};
use excali_scene::display::{FillRule, Path, Transform};
use excali_scene::export::frame_style;
use excali_scene::frame::{
    element_overlaps_with_frame, elements_are_in_frame_bounds, frame_clip, get_target_frame,
    is_element_containing_frame, is_element_in_frame, is_element_intersecting_frame,
    should_apply_frame_clip,
};
use excali_scene::static_scene::StaticCanvasAppState;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/frame-clip.json")).unwrap()
}

fn scenes() -> Vec<Value> {
    fixture()["scenes"].as_array().unwrap().clone()
}

fn element(value: &Value) -> Element {
    Element::from_map(value.as_object().unwrap().clone()).unwrap()
}

fn elements(scene: &Value) -> Vec<Element> {
    scene["elements"].as_array().unwrap().iter().map(element).collect()
}

fn app_state(value: &Value) -> StaticCanvasAppState {
    let fr = &value["frameRendering"];
    let mut state = StaticCanvasAppState::default();
    state.frame_rendering.enabled = fr["enabled"].as_bool().unwrap();
    state.frame_rendering.clip = fr["clip"].as_bool().unwrap();
    state.frame_rendering.name = fr["name"].as_bool().unwrap();
    state.frame_rendering.outline = fr["outline"].as_bool().unwrap();
    state.selected_element_ids = value["selectedElementIds"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, v)| v.as_bool() == Some(true))
        .map(|(k, _)| k.clone())
        .collect::<HashSet<_>>();
    state.selected_elements_are_being_dragged =
        value["selectedElementsAreBeingDragged"].as_bool().unwrap();
    state.editing_group_id = value["editingGroupId"].as_str().map(str::to_owned);
    state.frame_to_highlight = match &value["frameToHighlight"] {
        Value::Null => None,
        frame => Some(element(frame)),
    };
    state
}

fn close(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

#[test]
fn outlines_are_upstreams_segments() {
    let mut failures = Vec::new();
    let mut compared = 0;
    for scene in scenes() {
        let name = scene["name"].as_str().unwrap();
        let all = elements(&scene);
        let map = ElementsMap::new(&all);
        for (el, expected) in all.iter().zip(scene["results"].as_array().unwrap()) {
            let id = &el.base.id;
            let segments = get_element_line_segments(el, &map).unwrap();
            let want = expected["segments"].as_array().unwrap();
            if segments.len() != want.len() {
                failures.push(format!(
                    "{name} {id}: {} segments, upstream {}",
                    segments.len(),
                    want.len()
                ));
                continue;
            }
            for (i, (s, w)) in segments.iter().zip(want).enumerate() {
                let w: Vec<f64> = w
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|p| p.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()))
                    .collect();
                let got = [s.0.x, s.0.y, s.1.x, s.1.y];
                if !got.iter().zip(&w).all(|(a, b)| close(*a, *b)) {
                    failures.push(format!("{name} {id} #{i}: {got:?}, upstream {w:?}"));
                    break;
                }
                compared += 1;
            }
            let bounds: Vec<f64> = expected["bounds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            let got = get_element_bounds(el, &map);
            if !got.iter().zip(&bounds).all(|(a, b)| close(*a, *b)) {
                failures.push(format!("{name} {id}: bounds {got:?}, upstream {bounds:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(compared > 1000, "only {compared} segments compared");
}

#[test]
fn frame_decisions_are_upstreams() {
    let mut failures = Vec::new();
    let mut clipped = 0;
    let mut not_clipped = 0;
    for scene in scenes() {
        let name = scene["name"].as_str().unwrap();
        let all = elements(&scene);
        let map = ElementsMap::new(&all);
        let state = app_state(&scene["appState"]);
        for (el, expected) in all.iter().zip(scene["results"].as_array().unwrap()) {
            let id = &el.base.id;
            let target = get_target_frame(el, &map, &state).map(|f| f.base.id.as_str());
            if target != expected["targetFrame"].as_str() {
                failures.push(format!(
                    "{name} {id}: target frame {target:?}, upstream {}",
                    expected["targetFrame"]
                ));
            }
            for (frame_id, want) in expected["frames"].as_object().unwrap() {
                let frame = map.get(frame_id).unwrap();
                let got = [
                    (
                        "intersecting",
                        is_element_intersecting_frame(el, frame, &map).unwrap(),
                    ),
                    (
                        "containing",
                        is_element_containing_frame(el, frame, &map),
                    ),
                    (
                        "inBounds",
                        elements_are_in_frame_bounds(&[el], frame, &map),
                    ),
                    (
                        "overlaps",
                        element_overlaps_with_frame(el, frame, &map).unwrap(),
                    ),
                    (
                        "inFrame",
                        is_element_in_frame(el, &map, &state, Some(frame), Some(&mut HashMap::new()))
                            .unwrap(),
                    ),
                    (
                        "shouldClip",
                        should_apply_frame_clip(el, frame, &state, &map, Some(&mut HashMap::new()))
                            .unwrap(),
                    ),
                ];
                for (key, value) in got {
                    if want[key].as_bool() != Some(value) {
                        failures.push(format!(
                            "{name} {id} in {frame_id}: {key} {value}, upstream {}",
                            want[key]
                        ));
                    }
                }
                if want["shouldClip"] == true {
                    clipped += 1;
                } else {
                    not_clipped += 1;
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(clipped > 50 && not_clipped > 50, "{clipped} clipped, {not_clipped} not");
}

#[test]
fn the_fixture_covers_every_branch() {
    // every outline kind and every clip decision the static scene takes
    let doc = fixture();
    let scene = &doc["scenes"][0];
    let results = scene["results"].as_array().unwrap();
    let result = |id: &str| results.iter().find(|r| r["id"] == id).unwrap();
    let decision = |id: &str| result(id)["frames"]["clip-frame"].clone();
    // crossing, containing, inside, grouped outside
    assert_eq!(decision("fc-crossing")["intersecting"], true);
    assert_eq!(decision("fc-backdrop")["containing"], true);
    assert_eq!(decision("fc-inside")["shouldClip"], false);
    assert_eq!(decision("fc-grouped-out")["intersecting"], false);
    assert_eq!(decision("fc-grouped-out")["shouldClip"], true);
    // ellipses are 90 chords, frames 4 sides and 4 corners
    assert_eq!(result("fc-ellipse")["segments"].as_array().unwrap().len(), 90);
    assert_eq!(result("clip-frame")["segments"].as_array().unwrap().len(), 8);
    let names: Vec<&str> = doc["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    for name in ["frame-clip", "frame-clip-off", "frame-drag", "frame-drag-editing-group", "frame-selected"] {
        assert!(names.contains(&name), "no scene {name}");
    }
}

#[test]
fn frame_clip_is_a_round_rect_of_radius_8_over_zoom_at_the_frame() {
    assert_eq!(frame_style::RADIUS, 8.0);
    assert_eq!(frame_style::STROKE_COLOR, "#bbb");
    assert_eq!(frame_style::STROKE_WIDTH, 2.0);
    let frame = element(&serde_json::json!({
        "id": "f", "type": "frame", "x": 10, "y": 20, "width": 100, "height": 50,
        "angle": 0, "strokeColor": "#bbb", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 0,
        "opacity": 100, "groupIds": [], "frameId": null, "roundness": null, "seed": 1,
        "version": 1, "versionNonce": 1, "isDeleted": false, "boundElements": null,
        "updated": 1, "link": null, "locked": false, "name": null
    }));
    let state = StaticCanvasAppState {
        zoom: 2.0,
        scroll_x: 5.0,
        scroll_y: -3.0,
        ..StaticCanvasAppState::default()
    };
    let (translate, clip, back) = frame_clip(&frame, &state);
    assert_eq!(translate, Transform::translate(15.0, 17.0));
    assert_eq!(back, Transform::translate(-15.0, -17.0));
    assert_eq!(clip.rule, FillRule::NonZero);
    assert_eq!(clip.path, Path::round_rect(0.0, 0.0, 100.0, 50.0, 4.0));
}
