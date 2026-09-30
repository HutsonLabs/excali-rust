//! The interactive scene (ex-713): `renderInteractiveScene`
//! (`packages/excalidraw/renderer/interactiveScene.ts`) with `renderSnaps`
//! (`renderer/renderSnaps.ts`) and the helpers of `renderer/helpers.ts`,
//! against what upstream draws.
//!
//! Fixture: `tests/fixtures/interactive-scene.json`, upstream's own
//! `renderInteractiveScene` at the pinned commit on the recording 2D
//! context the static scene goldens use (`tools/goldens/
//! interactive-scene.mjs`): every fill, stroke and clip in order, with its
//! path, matrix, alpha and styles. The port's display list is replayed
//! through the static scene's recording [`Painter`] and the two must agree
//! draw for draw, scene by scene: selection borders (single, rotated,
//! locked, groups, the group being edited, image and active embeddable
//! variants), transform handles (single element, multi-selection box,
//! frames, phones), the selection box, crop handles, linear element point
//! handles, midpoints and hover highlights, the focus point indicator, the
//! binding highlight with its midpoints and frame clip, the frame and
//! element highlights, snap lines, the text box and its auto-resize
//! handle, the search matches (texts, bound text, frames, the focused
//! match, frame names hidden on the canvas, missing elements, ex-714),
//! under zoom, scroll, device pixel ratios and the dark theme.
//!
//! [`Painter`]: excali_scene::display::Painter

use excali_core::element::Element;
use excali_editor::interactive_scene::{
    render_interactive_scene, InteractiveCanvasAppState, InteractiveScene, DEFAULT_SELECTION_COLOR,
};
use excali_editor::transform_handles::{EditorInterface, FormFactor};
use excali_scene::bounds::ElementsMap;
use excali_scene::display::{DisplayList, Transform};
use excali_scene::shape::Theme;
use serde_json::Value;

#[path = "../../excali-scene/tests/support/draws.rs"]
mod draws;

use draws::{compare, Draw, Recorder};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/interactive-scene.json")).unwrap()
}

fn scenes() -> Vec<Value> {
    fixture()["scenes"].as_array().unwrap().clone()
}

fn scene(name: &str) -> Value {
    scenes()
        .into_iter()
        .find(|s| s["name"] == name)
        .unwrap_or_else(|| panic!("no scene {name}"))
}

fn elements(value: &Value) -> Vec<Element> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
        .collect()
}

fn editor_interface(value: &Value) -> EditorInterface {
    let form_factor = match value["formFactor"].as_str().unwrap() {
        "phone" => FormFactor::Phone,
        "tablet" => FormFactor::Tablet,
        _ => FormFactor::Desktop,
    };
    EditorInterface::new(
        form_factor,
        value["userAgent"]["isMobileDevice"].as_bool().unwrap(),
    )
}

/// Draws the scene as the port does: the app state read from upstream's
/// JSON by [`InteractiveCanvasAppState::from_app_state`], the selected
/// elements in scene order (`scene.getSelectedElements`).
fn render(scene: &Value) -> DisplayList {
    let all = elements(&scene["elements"]);
    let refs: Vec<&Element> = all.iter().collect();
    let map = ElementsMap::new(refs.iter().copied());
    let state = InteractiveCanvasAppState::from_app_state(scene["appState"].as_object().unwrap());
    let selected: Vec<&Element> = refs
        .iter()
        .copied()
        .filter(|e| state.selected_element_ids.contains(&e.base.id))
        .collect();
    let pointer = scene["pointer"]
        .as_array()
        .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]);
    render_interactive_scene(&InteractiveScene {
        canvas_width: scene["width"].as_f64().unwrap(),
        canvas_height: scene["height"].as_f64().unwrap(),
        scale: scene["scale"].as_f64().unwrap(),
        elements_map: &map,
        elements: &refs,
        all_elements_map: &map,
        all_elements: &refs,
        visible_elements: &refs,
        selected_elements: &selected,
        app_state: &state,
        selection_color: scene["selectionColor"].as_str().unwrap(),
        editor_interface: editor_interface(&scene["editorInterface"]),
        pointer,
        angle_locked: scene["angleLocked"].as_bool().unwrap(),
    })
}

fn draws(list: &DisplayList) -> Vec<Draw> {
    let mut recorder = Recorder::default();
    list.replay(&mut recorder);
    recorder.0
}

#[test]
fn every_scene_draws_what_upstream_draws() {
    let mut failures = Vec::new();
    let mut compared = 0;
    let all = scenes();
    for scene in &all {
        let name = scene["name"].as_str().unwrap();
        match compare(&render(scene), &scene["events"], &Value::Null) {
            Ok(n) => compared += n,
            Err(why) => failures.push(format!("{name}: {why}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(compared > 450, "only {compared} draws compared");
}

#[test]
fn the_fixture_covers_the_scope() {
    let names: Vec<String> = scenes()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_owned())
        .collect();
    for name in [
        "rectangle-selected",
        "rectangle-rotated",
        "multi-selection",
        "group-selected",
        "editing-group",
        "locked-selected",
        "selection-box",
        "zoom-2-dpr-2",
        "dark",
        "line-selected",
        "line-editing",
        "polygon-editing",
        "elbow-arrow-selected",
        "focus-point",
        "suggested-binding",
        "suggested-binding-elbow-in-frame",
        "frame-to-highlight",
        "elements-to-highlight",
        "active-locked-group",
        "snap-lines",
        "text-editing",
        "image-cropping",
        "search-matches-texts",
        "search-matches-rotated-dark-zoomed",
        "search-matches-frames",
        "search-matches-missing-and-empty",
    ] {
        assert!(names.iter().any(|n| n == name), "no scene {name}");
    }
    // every scene draws something but the ones that must not
    for s in scenes() {
        let draws = s["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["op"] != "clear")
            .count();
        let empty = [
            "multi-element",
            "selection-box-cropping",
            "suggested-binding-disabled",
        ];
        assert_eq!(
            draws == 0,
            empty.contains(&s["name"].as_str().unwrap()),
            "{}",
            s["name"]
        );
    }
}

#[test]
fn the_list_is_scaled_by_the_device_pixel_ratio_then_the_zoom() {
    // bootstrapCanvas: scale(dpr); then "Apply zoom": scale(zoom); the
    // overlays translate by the scroll snapped to device pixels
    let list = render(&scene("zoom-2-dpr-2"));
    let first = draws(&list).into_iter().next().unwrap();
    let Draw::Stroke { m, .. } = first else {
        panic!("{first:?}")
    };
    // round(13.3 × 4) / 4 and round(-7.26 × 4) / 4
    let (sx, sy) = (53.0 / 4.0, -29.0 / 4.0);
    let expected = Transform::scale(2.0, 2.0)
        .concat(&Transform::scale(2.0, 2.0))
        .concat(&Transform::translate(sx, sy));
    // the selection border's own translate(cx, cy) and rotate(0) follow
    assert_eq!((m.a, m.d), (expected.a, expected.d));
    assert!(m.e > expected.e && m.f > expected.f);
}

#[test]
fn from_app_state_reads_upstream_json() {
    let s = scene("line-editing");
    let state = InteractiveCanvasAppState::from_app_state(s["appState"].as_object().unwrap());
    let linear = state.selected_linear_element.as_ref().unwrap();
    assert_eq!(linear.element_id, "l");
    assert!(linear.is_editing && !linear.is_dragging);
    assert_eq!(linear.selected_points_indices.as_deref(), Some(&[1][..]));
    assert_eq!(linear.hover_point_index, 2);
    assert_eq!(state.theme, Theme::Light);

    let s = scene("suggested-binding-midpoint");
    let state = InteractiveCanvasAppState::from_app_state(s["appState"].as_object().unwrap());
    let binding = state.suggested_binding.as_ref().unwrap();
    assert_eq!(binding.element_id, "target");
    assert_eq!(binding.mid_point, Some([250.0, 50.0]));
    assert_eq!(state.new_element.as_deref(), Some("drawing"));
    assert_eq!(state.zoom, 1.25);

    let s = scene("snap-lines");
    let state = InteractiveCanvasAppState::from_app_state(s["appState"].as_object().unwrap());
    assert_eq!(state.snap_lines.len(), 5);

    let s = scene("search-matches-frames");
    let state = InteractiveCanvasAppState::from_app_state(s["appState"].as_object().unwrap());
    assert_eq!(state.search_matches.len(), 3);
    let focused = &state.search_matches[2];
    assert_eq!(focused.id, "f3");
    assert!(focused.focus);
    let line = &focused.matched_lines[0];
    assert_eq!(
        (line.offset_x, line.offset_y, line.width, line.height),
        (4.0, -18.0, 28.0, 14.0)
    );
    assert!(!line.show_on_canvas);

    // the defaults of an empty app state
    let empty = InteractiveCanvasAppState::from_app_state(&serde_json::Map::new());
    assert_eq!(empty, InteractiveCanvasAppState::default());
    assert_eq!(empty.zoom, 1.0);
    assert!(empty.is_binding_enabled && empty.is_midpoint_snapping_enabled);
    assert_eq!(DEFAULT_SELECTION_COLOR, "#6965db");
}
