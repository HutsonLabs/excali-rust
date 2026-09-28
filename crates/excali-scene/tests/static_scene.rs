//! The static scene (ex-402): `renderStaticScene`
//! (`packages/excalidraw/renderer/staticScene.ts`) with `bootstrapCanvas`
//! and `snapScrollToDevicePixels` (`renderer/helpers.ts`) and
//! `renderElement` (`packages/element/src/renderElement.ts`), against what
//! upstream draws.
//!
//! Fixture: `tests/fixtures/static-scene.json`, upstream's own
//! `renderStaticScene` at the pinned commit on a recording 2D context
//! (`tools/goldens/static-scene.mjs`): every fill, stroke, clip, text and
//! image in order, with its path, matrix, alpha and styles. The port's
//! display list is replayed through a [`Painter`] that records the same
//! things, and the two lists must agree draw for draw: the order of work
//! (background, grid, elements with their bound text, iframes and
//! embeddables last, pending flowchart nodes), the grid's colours, widths,
//! dashes and zoom cutoff, and each element's drawing.

use std::collections::{HashMap, HashSet};

use excali_core::color::apply_dark_mode_filter;
use excali_core::element::Element;
use excali_scene::bounds::ElementsMap;
use excali_scene::display::DisplayList;
use excali_scene::export::FrameRendering;
use excali_scene::render_element::{is_rtl, ElementRenderOverride};
use excali_scene::shape::Theme;
use excali_scene::static_scene::{
    grid_line_colors, render_static_scene, snap_scroll_to_device_pixels, stroke_grid, CachedImage,
    GridConfig, StaticCanvasAppState, StaticCanvasRenderConfig, StaticScene, GRID_LINE_COLOR_BOLD,
    GRID_LINE_COLOR_REGULAR,
};
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{Map, Value};

#[path = "support/draws.rs"]
mod draws;

use draws::{compare, Draw, Recorder};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/static-scene.json")).unwrap()
}

/// Upstream's test text metrics: 10 px per UTF-16 code unit.
struct TenPxPerCodeUnit;

impl TextMetricsProvider for TenPxPerCodeUnit {
    fn get_line_width(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

// ---------------------------------------------------------------------------
// Reading a scene

fn elements(value: &Value) -> Vec<Element> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
                .collect()
        })
        .unwrap_or_default()
}

fn ids(value: &Value) -> HashSet<String> {
    match value {
        Value::Object(map) => map
            .iter()
            .filter(|(_, v)| v.as_bool() == Some(true))
            .map(|(k, _)| k.clone())
            .collect(),
        Value::Array(list) => list
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect(),
        _ => HashSet::new(),
    }
}

fn theme(value: &Value) -> Theme {
    match value.as_str() {
        Some("dark") => Theme::Dark,
        _ => Theme::Light,
    }
}

fn app_state(value: &Value) -> StaticCanvasAppState {
    let fr = &value["frameRendering"];
    StaticCanvasAppState {
        zoom: value["zoom"]["value"].as_f64().unwrap(),
        scroll_x: value["scrollX"].as_f64().unwrap(),
        scroll_y: value["scrollY"].as_f64().unwrap(),
        theme: theme(&value["theme"]),
        view_background_color: value["viewBackgroundColor"].as_str().map(str::to_owned),
        grid_size: value["gridSize"].as_f64().unwrap(),
        grid_step: value["gridStep"].as_f64().unwrap(),
        frame_rendering: FrameRendering {
            enabled: fr["enabled"].as_bool().unwrap(),
            clip: fr["clip"].as_bool().unwrap(),
            name: fr["name"].as_bool().unwrap(),
            outline: fr["outline"].as_bool().unwrap(),
        },
        selected_element_ids: ids(&value["selectedElementIds"]),
        hovered_element_ids: ids(&value["hoveredElementIds"]),
        open_dialog: value["openDialog"]["name"].as_str().map(str::to_owned),
        frame_to_highlight: match &value["frameToHighlight"] {
            Value::Null => None,
            frame => Some(Element::from_map(frame.as_object().unwrap().clone()).unwrap()),
        },
        selected_elements_are_being_dragged: value["selectedElementsAreBeingDragged"]
            .as_bool()
            .unwrap(),
        editing_group_id: value["editingGroupId"].as_str().map(str::to_owned),
    }
}

fn overrides(value: &Value) -> HashMap<String, ElementRenderOverride> {
    value
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(id, o)| {
                    let offset = o
                        .get("offset")
                        .map(|off| [off["x"].as_f64().unwrap(), off["y"].as_f64().unwrap()]);
                    (
                        id.clone(),
                        ElementRenderOverride {
                            opacity: o.get("opacity").and_then(Value::as_f64),
                            offset,
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn images(value: &Value) -> HashMap<String, CachedImage> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, image)| {
            (
                id.clone(),
                CachedImage {
                    mime_type: image["mimeType"].as_str().unwrap().to_owned(),
                },
            )
        })
        .collect()
}

fn embeds(value: &Value) -> HashMap<String, bool> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, v)| (id.clone(), v.as_bool().unwrap()))
        .collect()
}

/// Draws the scene as the port does.
fn render(scene: &Value, origin_host: &str) -> DisplayList {
    let all = elements(&scene["elements"]);
    let rc = &scene["renderConfig"];
    let pending = elements(&rc["pendingFlowchartNodes"]);
    let map = ElementsMap::new(&all);
    let visible: Vec<&Element> = all.iter().collect();
    let embeds = embeds(&rc["embedsValidationStatus"]);
    let config = StaticCanvasRenderConfig {
        canvas_background_color: rc["canvasBackgroundColor"].as_str().unwrap().to_owned(),
        image_cache: images(&scene["images"]),
        render_grid: rc["renderGrid"].as_bool().unwrap(),
        render_links: rc["renderLinks"].as_bool().unwrap_or(true),
        is_exporting: rc["isExporting"].as_bool().unwrap(),
        embeds_validation_status: embeds,
        elements_pending_erasure: ids(&rc["elementsPendingErasure"]),
        pending_flowchart_nodes: pending,
        theme: theme(&rc["theme"]),
        element_render_overrides: overrides(&rc["elementRenderOverrides"]),
        location_host: origin_host.to_owned(),
    };
    let state = app_state(&scene["appState"]);
    render_static_scene(&StaticScene {
        canvas_width: scene["width"].as_f64().unwrap(),
        canvas_height: scene["height"].as_f64().unwrap(),
        scale: scene["scale"].as_f64().unwrap(),
        elements_map: &map,
        all_elements_map: &map,
        visible_elements: &visible,
        app_state: &state,
        render_config: &config,
        text_metrics: &TenPxPerCodeUnit,
    })
}

fn scenes() -> Vec<Value> {
    fixture()["scenes"].as_array().unwrap().clone()
}

#[test]
fn every_scene_draws_what_upstream_draws() {
    let doc = fixture();
    let host = doc["origin"]
        .as_str()
        .unwrap()
        .trim_start_matches("https://")
        .to_owned();
    let mut failures = Vec::new();
    let mut compared = 0;
    for scene in scenes() {
        let name = scene["name"].as_str().unwrap();
        let list = render(&scene, &host);
        match compare(&list, &scene["events"], &scene["images"]) {
            Ok(n) => compared += n,
            Err(why) => failures.push(format!("{name} {why}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(compared > 1700, "only {compared} draws compared");
}

#[test]
fn the_fixture_covers_the_order_of_work() {
    // the scenes the acceptance names: zoom below and at the 10 px cutoff,
    // dark grid, every element kind, exporting, links, pending nodes
    let names: Vec<String> = scenes()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_owned())
        .collect();
    for name in [
        "grid",
        "grid-zoom-0.5",
        "grid-zoom-0.45",
        "grid-dark",
        "elements",
        "elements-exporting",
        "links",
        "pending-flowchart",
        "opacity",
        "frame-clip",
        "frame-clip-zoomed-dpr-2",
        "frame-clip-exporting",
        "frame-clip-off",
        "frame-clip-disabled",
        "frame-clip-offsets",
        "frame-drag",
        "frame-selected",
    ] {
        assert!(names.iter().any(|n| n == name), "no scene {name}");
    }
}

/// The clips of a scene's draws: `(clip path, matrix)` in order.
fn clips(scene: &Value) -> Vec<(Path, Transform)> {
    scene["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["op"] == "clip")
        .map(|e| (path(&e["path"]), matrix(&e["m"])))
        .collect()
}

#[test]
fn frame_children_are_clipped_to_a_round_rect_of_radius_8_over_zoom() {
    // staticScene.ts:165-189: roundRect(0, 0, w, h, FRAME_STYLE.radius /
    // zoom) at the frame's corner plus the scroll, under the zoom and the
    // device pixel ratio
    let all = scenes();
    let scene = |name: &str| all.iter().find(|s| s["name"] == name).unwrap().clone();
    let frame_clips = |name: &str, radius: f64| {
        clips(&scene(name))
            .into_iter()
            .filter(|(p, _)| *p == Path::round_rect(0.0, 0.0, 240.0, 160.0, radius))
            .count()
    };
    assert!(frame_clips("frame-clip", 8.0) > 15);
    assert!(frame_clips("frame-clip-zoomed-dpr-2", 8.0 / 1.5) > 15);
    assert!(frame_clips("frame-clip-zoom-0.5", 16.0) > 15);
    // clipping off, or frames off: no frame clips at all
    assert_eq!(frame_clips("frame-clip-off", 8.0), 0);
    assert_eq!(frame_clips("frame-clip-disabled", 8.0), 0);
    let (_, m) = clips(&scene("frame-clip-zoomed-dpr-2"))
        .into_iter()
        .find(|(p, _)| *p == Path::round_rect(0.0, 0.0, 240.0, 160.0, 8.0 / 1.5))
        .unwrap();
    // scale(2) · scale(1.5) · translate(50 + scrollX, 40 + scrollY), the
    // scroll (7.3, -3.6) snapped to device pixels: round(7.3 × 3) / 3 and
    // round(-3.6 × 3) / 3
    let (sx, sy) = snap_scroll_to_device_pixels(7.3, -3.6, 1.5, 2.0);
    assert_eq!((sx, sy), (22.0 / 3.0, -11.0 / 3.0));
    assert!(same_matrix(
        &m,
        &Transform::new(3.0, 0.0, 0.0, 3.0, 3.0 * (50.0 + sx), 3.0 * (40.0 + sy))
    ));
    // the port draws the same clips: every_scene_draws_what_upstream_draws
    let doc = fixture();
    let host = doc["origin"]
        .as_str()
        .unwrap()
        .trim_start_matches("https://");
    for name in ["frame-clip", "frame-drag", "frame-clip-offsets"] {
        let s = scene(name);
        let mut recorder = Recorder::default();
        render(&s, host).replay(&mut recorder);
        let ported = recorder
            .0
            .iter()
            .filter(|d| matches!(d, Draw::Clip { .. }))
            .count();
        assert_eq!(ported, clips(&s).len(), "{name}: clips");
    }
}

// ---------------------------------------------------------------------------
// The grid (staticScene.ts:57-163)

#[test]
fn grid_colours_are_upstreams() {
    assert_eq!(GRID_LINE_COLOR_BOLD, "#dddddd");
    assert_eq!(GRID_LINE_COLOR_REGULAR, "#e5e5e5");
    assert_eq!(
        grid_line_colors(Theme::Light),
        ("#dddddd".to_owned(), "#e5e5e5".to_owned())
    );
    assert_eq!(
        grid_line_colors(Theme::Dark),
        (
            apply_dark_mode_filter("#dddddd", true),
            apply_dark_mode_filter("#e5e5e5", true)
        )
    );
}

/// The grid's strokes: (colour, width, dash).
fn grid_strokes(config: &GridConfig) -> Vec<(String, f64, Option<Vec<f64>>)> {
    let list = DisplayList::from_iter([stroke_grid(config)]);
    let mut recorder = Recorder::default();
    list.replay(&mut recorder);
    recorder
        .0
        .into_iter()
        .map(|d| match d {
            Draw::Stroke { stroke, .. } => (
                stroke.color.as_str().to_owned(),
                stroke.width,
                stroke.dash.map(|d| d.segments().to_vec()),
            ),
            other => panic!("the grid drew {other:?}"),
        })
        .collect()
}

fn grid(zoom: f64, grid_size: f64, grid_step: f64, scale: f64) -> GridConfig {
    GridConfig {
        grid_size,
        grid_step,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom,
        theme: Theme::Light,
        width: 400.0 / zoom,
        height: 300.0 / zoom,
        scale,
    }
}

#[test]
fn regular_lines_are_left_out_below_10_px() {
    // gridSize × zoom = 10: drawn; 9.8: only the bold lines
    let at = grid_strokes(&grid(0.5, 20.0, 5.0, 1.0));
    assert!(at.iter().any(|(c, _, _)| c == GRID_LINE_COLOR_REGULAR));
    let below = grid_strokes(&grid(0.49, 20.0, 5.0, 1.0));
    assert!(!below.is_empty());
    assert!(below
        .iter()
        .all(|(c, _, dash)| c == GRID_LINE_COLOR_BOLD && dash.is_none()));
    // with bold lines off (gridStep 1) nothing is drawn below the cutoff
    assert!(grid_strokes(&grid(0.49, 20.0, 1.0, 1.0)).is_empty());
}

#[test]
fn regular_lines_are_dashed_bold_lines_solid() {
    // zoom 1, dpr 1: regular lines 1 px wide, dash [3, 1 + (1 + 1)]; bold
    // lines min(dpr, 4 × zoom × dpr) = 1 device pixel, solid
    let strokes = grid_strokes(&grid(1.0, 20.0, 5.0, 1.0));
    let regular: Vec<_> = strokes
        .iter()
        .filter(|(c, ..)| c == GRID_LINE_COLOR_REGULAR)
        .collect();
    let bold: Vec<_> = strokes
        .iter()
        .filter(|(c, ..)| c == GRID_LINE_COLOR_BOLD)
        .collect();
    assert!(!regular.is_empty() && !bold.is_empty());
    for (_, width, dash) in regular {
        assert_eq!(*width, 1.0);
        assert_eq!(dash.as_deref(), Some(&[3.0, 3.0][..]));
    }
    for (_, width, dash) in bold {
        assert_eq!(*width, 1.0);
        assert!(dash.is_none());
    }
    // zoom 0.25 at dpr 2: space = 4; a regular line is 1/zoom wide in
    // scene units, capped at dpr device pixels: 2 / 0.5 = 4 scene units
    let strokes = grid_strokes(&grid(2.0, 20.0, 5.0, 2.0));
    let (_, width, dash) = strokes
        .iter()
        .find(|(c, ..)| c == GRID_LINE_COLOR_REGULAR)
        .unwrap();
    assert_eq!(*width, 0.5);
    assert_eq!(dash.as_deref(), Some(&[1.5, 0.5 + (0.5 + 0.5)][..]));
}

#[test]
fn scroll_snaps_to_device_pixels() {
    // round(scroll × zoom × dpr) / (zoom × dpr)
    assert_eq!(
        snap_scroll_to_device_pixels(10.3, -4.21, 1.37, 2.0),
        (
            (10.3f64 * 2.74).round() / 2.74,
            (-4.21f64 * 2.74).round() / 2.74
        )
    );
    // 0.6 and 1.35 device pixels: both round to 1
    assert_eq!(
        snap_scroll_to_device_pixels(0.4, 0.9, 1.0, 1.5),
        (1.0 / 1.5, 1.0 / 1.5)
    );
    // halves round up, as Math.round does
    assert_eq!(
        snap_scroll_to_device_pixels(-0.5, 2.5, 1.0, 1.0),
        (0.0, 3.0)
    );
    // no device pixels: unchanged
    assert_eq!(snap_scroll_to_device_pixels(0.4, 0.9, 0.0, 1.0), (0.4, 0.9));
    assert_eq!(
        snap_scroll_to_device_pixels(0.4, 0.9, f64::NAN, 1.0),
        (0.4, 0.9)
    );
}

// ---------------------------------------------------------------------------
// Order of work

#[test]
fn iframes_come_last_and_bound_text_follows_its_container() {
    // the box's label is listed before the box and drawn right after it;
    // the iframe and the embeddable are listed early and drawn last, with
    // their Helvetica placeholder labels
    let scene = scenes()
        .into_iter()
        .find(|s| s["name"] == "elements-exporting")
        .unwrap();
    let list = render(&scene, "excalidraw.com");
    let mut recorder = Recorder::default();
    list.replay(&mut recorder);
    let first_text = recorder.0.iter().find_map(|d| match d {
        Draw::Text { run, .. } => Some(run.text.as_str()),
        _ => None,
    });
    assert_eq!(first_text, Some("in a box"));
    let first_label = recorder
        .0
        .iter()
        .position(
            |d| matches!(d, Draw::Text { run, .. } if run.font.family.starts_with("Helvetica")),
        )
        .unwrap();
    assert!(recorder.0[first_label..].iter().all(|d| match d {
        Draw::Text { run, .. } => run.font.family.starts_with("Helvetica"),
        Draw::Image { .. } => false,
        _ => true,
    }));
}

#[test]
fn rtl_is_the_first_strong_character() {
    // RE_RTL_CHECK (common/src/utils.ts:363-374)
    assert!(is_rtl("שלום"));
    assert!(is_rtl("123 שלום"));
    assert!(is_rtl("  مرحبا abc"));
    assert!(!is_rtl("abc שלום"));
    assert!(!is_rtl("123"));
    assert!(!is_rtl(""));
    // astral characters are surrogates, in the LTR range
    assert!(!is_rtl("😀 שלום"));
    // U+0590 is LTR, U+0591 RTL, U+FB1D RTL, U+FDFE LTR, U+FE70 RTL
    assert!(!is_rtl("\u{0590}"));
    assert!(is_rtl("\u{0591}"));
    assert!(is_rtl("\u{FB1D}"));
    assert!(!is_rtl("\u{FDFE}"));
    assert!(is_rtl("\u{FE70}"));
    assert!(!is_rtl("\u{FEFD}"));
}

const RECTANGLE: &str = r##"{"id":"a","type":"rectangle","x":0,"y":0,"width":10,"height":10,
    "angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid",
    "strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,
    "index":"a0","roundness":null,"seed":1,"version":1,"versionNonce":0,"isDeleted":false,
    "boundElements":null,"updated":1,"link":null,"locked":false}"##;

const STICKY_NOTE: &str = r##"{"id":"sticky","type":"stickynote","x":-120,"y":-90,"width":200,
    "height":200,"angle":0.3,"strokeColor":"#1e1e1e","backgroundColor":"#ffdf6b","fillStyle":"solid",
    "strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,
    "index":null,"roundness":null,"seed":5,"version":2,"versionNonce":428152832,"isDeleted":false,
    "boundElements":null,"updated":1,"created":1,"link":null,"locked":false,"baseHeight":200}"##;

fn element(json: &str, id: &str, x: f64) -> Element {
    let mut raw: Map<String, Value> = serde_json::from_str(json).unwrap();
    raw.insert("id".into(), id.into());
    raw.insert("x".into(), x.into());
    Element::from_map(raw).unwrap()
}

fn draws(all: &[Element]) -> Vec<Draw> {
    let map = ElementsMap::new(all);
    let visible: Vec<&Element> = all.iter().collect();
    let state = StaticCanvasAppState::default();
    let config = StaticCanvasRenderConfig {
        render_grid: false,
        ..StaticCanvasRenderConfig::default()
    };
    let list = render_static_scene(&StaticScene {
        canvas_width: 100.0,
        canvas_height: 100.0,
        scale: 1.0,
        elements_map: &map,
        all_elements_map: &map,
        visible_elements: &visible,
        app_state: &state,
        render_config: &config,
        text_metrics: &TenPxPerCodeUnit,
    });
    let mut recorder = Recorder::default();
    list.replay(&mut recorder);
    recorder.0
}

#[test]
fn sticky_notes_are_not_drawn_here() {
    // sticky notes are drawn by ex-703 (renderElement.ts:438-472); until
    // then the static scene leaves them out and draws the rest as before
    let with = draws(&[
        element(RECTANGLE, "a", 0.0),
        element(STICKY_NOTE, "sticky", 20.0),
        element(RECTANGLE, "b", 50.0),
    ]);
    let without = draws(&[element(RECTANGLE, "a", 0.0), element(RECTANGLE, "b", 50.0)]);
    assert_eq!(format!("{with:?}"), format!("{without:?}"));
    // the background and two rough rectangles, one stroke each
    assert!(matches!(with[0], Draw::FillRect { .. }));
    assert_eq!(with.len(), 3);
}

fn linked_rectangle(bound: Option<&str>) -> Element {
    let mut raw: Map<String, Value> = serde_json::from_str(RECTANGLE).unwrap();
    raw.insert("link".into(), "https://example.com".into());
    if let Some(id) = bound {
        raw.insert(
            "boundElements".into(),
            serde_json::json!([{ "type": "text", "id": id }]),
        );
    }
    Element::from_map(raw).unwrap()
}

#[test]
fn a_bound_text_that_cannot_draw_leaves_its_container_and_drops_its_icon() {
    // getBoundTextElement returns whatever element the container's text
    // entry names; a sticky note there cannot be drawn yet, so, as when
    // upstream's renderElement throws for the label, the container stays
    // drawn and its link icon is skipped
    let with = draws(&[
        linked_rectangle(Some("sticky")),
        element(STICKY_NOTE, "sticky", 20.0),
    ]);
    let plain = draws(&[element(RECTANGLE, "a", 0.0)]);
    assert_eq!(format!("{with:?}"), format!("{plain:?}"));
    // without the failing label the icon is drawn: a clip, the icon
    // canvas's background, the icon, the end of the clip
    let icon = draws(&[linked_rectangle(None)]);
    assert_eq!(icon.len(), plain.len() + 4);
    assert!(matches!(icon[plain.len()], Draw::Clip { .. }));
    assert!(matches!(icon[plain.len() + 2], Draw::Image { .. }));
}
