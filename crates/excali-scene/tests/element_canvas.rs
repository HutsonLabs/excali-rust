//! The per-element bitmap cache (ex-504): `renderElement`
//! (`packages/element/src/renderElement.ts`) on its editor path, which
//! rasterises each element once into a canvas of its own
//! (`generateElementWithCanvas`, `generateElementCanvas`,
//! `cappedElementCanvasSize`, `getCanvasPadding`) and blits it
//! (`drawElementFromCanvas`, `canSnapElement`, `SNAP_TIE_BIAS`), against
//! what upstream does.
//!
//! Fixture: `tests/fixtures/element-canvas.json`, upstream's own
//! `renderElement` at the pinned commit on a recording 2D context
//! (`tools/goldens/element-canvas.mjs`). Per case the bitmap upstream
//! caches (its size, scale, offsets, cache key and every draw into it) and
//! the blit on the main context (clip, matrix, destination, alpha,
//! smoothing), which must agree exactly: the snapped matrix's origin is a
//! whole device pixel. Per sequence, whether each change of zoom, zoom
//! gesture, theme, device pixel ratio, frame opacity, image crop or the
//! element itself made a new bitmap.

use std::collections::HashSet;

use excali_core::element::{Element, ElementKind};
use excali_math::Radians;
use excali_scene::bounds::ElementsMap;
use excali_scene::display::{Blit, FillRule, Transform};
use excali_scene::element_canvas::{
    can_snap_element, capped_element_canvas_size, get_canvas_padding, render_element_cached,
    ElementCanvas, ElementCanvasCache, ElementDraw, AREA_LIMIT, SNAP_TIE_BIAS,
    WIDTH_HEIGHT_LIMIT,
};
use excali_scene::render_element::ElementRenderOverride;
use excali_scene::shape::Theme;
use excali_scene::static_scene::{CachedImage, StaticCanvasAppState, StaticCanvasRenderConfig};
use serde_json::Value;

#[path = "support/draws.rs"]
mod draws;

use draws::{compare, matrix, path};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/element-canvas.json")).unwrap()
}

// ---------------------------------------------------------------------------
// Reading a case

fn elements(value: &Value) -> Vec<Element> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
        .collect()
}

fn theme(value: &Value) -> Theme {
    match value.as_str() {
        Some("dark") => Theme::Dark,
        _ => Theme::Light,
    }
}

fn app_state(value: &Value) -> StaticCanvasAppState {
    StaticCanvasAppState {
        zoom: value["zoom"]["value"].as_f64().unwrap(),
        scroll_x: value["scrollX"].as_f64().unwrap(),
        scroll_y: value["scrollY"].as_f64().unwrap(),
        theme: theme(&value["theme"]),
        should_cache_ignore_zoom: value["shouldCacheIgnoreZoom"].as_bool().unwrap(),
        ..StaticCanvasAppState::default()
    }
}

fn render_config(value: &Value, images: &Value) -> StaticCanvasRenderConfig {
    let overrides = value["elementRenderOverrides"]
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(id, o)| {
                    let offset = o
                        .get("offset")
                        .map(|off| [off["x"].as_f64().unwrap(), off["y"].as_f64().unwrap()]);
                    let opacity = o.get("opacity").and_then(Value::as_f64);
                    (id.clone(), ElementRenderOverride { opacity, offset })
                })
                .collect()
        })
        .unwrap_or_default();
    StaticCanvasRenderConfig {
        canvas_background_color: value["canvasBackgroundColor"].as_str().unwrap().to_owned(),
        image_cache: images
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, image)| {
                let mime_type = image["mimeType"].as_str().unwrap().to_owned();
                (id.clone(), CachedImage { mime_type })
            })
            .collect(),
        render_grid: false,
        is_exporting: value["isExporting"].as_bool().unwrap(),
        elements_pending_erasure: value["elementsPendingErasure"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<HashSet<_>>(),
        theme: theme(&value["theme"]),
        element_render_overrides: overrides,
        ..StaticCanvasRenderConfig::default()
    }
}

/// Upstream's numbers and the port's, the same double (JSON writes -0 as
/// 0).
fn same(expected: &Value, actual: f64) -> bool {
    expected.as_f64() == Some(actual) || (expected.as_f64() == Some(0.0) && actual == 0.0)
}

fn same_transform(expected: &Value, actual: &Transform) -> bool {
    let m = expected.as_array().unwrap();
    [actual.a, actual.b, actual.c, actual.d, actual.e, actual.f]
        .iter()
        .zip(m)
        .all(|(a, e)| same(e, *a))
}

/// The bitmap a cache entry was made from, as the test's surface.
#[derive(Clone, Debug)]
struct Surface {
    canvas: ElementCanvas,
    serial: usize,
}

struct Run {
    cache: ElementCanvasCache<Surface>,
    made: usize,
}

impl Run {
    fn new() -> Self {
        Run {
            cache: ElementCanvasCache::new(),
            made: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        all: &[Element],
        id: &str,
        state: &StaticCanvasAppState,
        config: &StaticCanvasRenderConfig,
        scale: f64,
        base: Transform,
    ) -> Option<ElementDraw> {
        let map = ElementsMap::new(all);
        let element = all.iter().find(|e| e.base.id == id).unwrap();
        let made = &mut self.made;
        render_element_cached(
            element,
            &map,
            &map,
            config,
            state,
            scale,
            base,
            None,
            &mut self.cache,
            |canvas: &ElementCanvas| {
                *made += 1;
                Surface {
                    canvas: canvas.clone(),
                    serial: *made,
                }
            },
        )
        .unwrap()
    }
}

fn check_cached(case: &Value, run: &Run, images: &Value) -> Result<(), String> {
    let id = case["draw"].as_str().unwrap();
    let expected = &case["cached"];
    let entry = run.cache.get(id);
    let (expected, entry) = match (expected, entry) {
        (Value::Null, None) => return Ok(()),
        (Value::Null, Some(_)) => return Err("cached a bitmap upstream did not make".into()),
        (_, None) => return Err("no bitmap cached".into()),
        (e, Some(entry)) => (e, entry),
    };
    let canvas = &entry.surface.canvas;
    let fields = [
        ("width", canvas.width),
        ("height", canvas.height),
        ("scale", canvas.scale),
        ("canvasOffsetX", canvas.canvas_offset_x),
        ("canvasOffsetY", canvas.canvas_offset_y),
        ("zoomValue", canvas.key.zoom_value),
        (
            "containingFrameOpacity",
            canvas.key.containing_frame_opacity,
        ),
    ];
    for (name, actual) in fields {
        if !same(&expected[name], actual) {
            return Err(format!("{name} {actual}, expected {}", expected[name]));
        }
    }
    if theme(&expected["theme"]) != canvas.key.theme {
        return Err(format!("theme {:?}", canvas.key.theme));
    }
    let crop = canvas.key.image_crop.map(|c| {
        serde_json::json!({
            "x": c.x, "y": c.y, "width": c.width, "height": c.height,
            "naturalWidth": c.natural_width, "naturalHeight": c.natural_height,
        })
    });
    if expected["imageCrop"] != crop.unwrap_or(Value::Null) {
        return Err(format!("imageCrop {:?}", canvas.key.image_crop));
    }
    compare(&canvas.content, &expected["events"], images)
        .map(|_| ())
        .map_err(|why| format!("bitmap draws: {why}"))
}

fn check_blit(case: &Value, draw: Option<&ElementDraw>) -> Result<(), String> {
    let events = case["events"].as_array().unwrap();
    let blit: Option<&Blit> = match draw {
        Some(ElementDraw::Blit(blit)) => Some(blit),
        Some(ElementDraw::Vector(_)) => return Err("drawn as vectors".into()),
        None => None,
    };
    let Some(blit) = blit else {
        return if events.is_empty() {
            Ok(())
        } else {
            Err(format!("nothing drawn, upstream drew {}", events.len()))
        };
    };
    let expected_ops: Vec<&str> = events.iter().map(|e| e["op"].as_str().unwrap()).collect();
    let ops: &[&str] = if blit.clip.is_some() {
        &["clip", "blit", "unclip"]
    } else {
        &["blit"]
    };
    if expected_ops != ops {
        return Err(format!("ops {ops:?}, expected {expected_ops:?}"));
    }
    if let Some((clip, transform)) = &blit.clip {
        let e = &events[0];
        if !same_transform(&e["m"], transform) {
            return Err(format!("clip matrix {transform:?}, expected {}", e["m"]));
        }
        if clip.rule != FillRule::EvenOdd || e["rule"] != "evenodd" {
            return Err(format!("clip rule {:?}, expected {}", clip.rule, e["rule"]));
        }
        if path(&e["path"]) != clip.path {
            return Err(format!("clip path {:?}, expected {}", clip.path, e["path"]));
        }
    }
    let e = events.iter().find(|e| e["op"] == "blit").unwrap();
    if !same_transform(&e["m"], &blit.transform) {
        return Err(format!("blit matrix {:?}, expected {}", blit.transform, e["m"]));
    }
    let args = e["args"].as_array().unwrap();
    let dest = [blit.dest.x, blit.dest.y, blit.dest.width, blit.dest.height];
    if !args.iter().zip(dest).all(|(e, a)| same(e, a)) {
        return Err(format!("blit rect {dest:?}, expected {args:?}"));
    }
    if !same(&e["alpha"], blit.alpha) {
        return Err(format!("alpha {}, expected {}", blit.alpha, e["alpha"]));
    }
    // the context's smoothing is on unless the blit turns it off
    if e["smoothing"].as_bool() != Some(blit.smoothing.unwrap_or(true)) {
        return Err(format!("smoothing {:?}, expected {}", blit.smoothing, e["smoothing"]));
    }
    Ok(())
}

#[test]
fn every_case_caches_and_blits_what_upstream_does() {
    let doc = fixture();
    let images = &doc["images"];
    let mut failures = Vec::new();
    let cases = doc["cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let all = elements(&case["elements"]);
        let state = app_state(&case["appState"]);
        let config = render_config(&case["renderConfig"], images);
        let mut run = Run::new();
        let draw = run.draw(
            &all,
            case["draw"].as_str().unwrap(),
            &state,
            &config,
            case["scale"].as_f64().unwrap(),
            matrix(&case["base"]),
        );
        if let Err(why) = check_cached(case, &run, images) {
            failures.push(format!("{name}: {why}"));
        }
        if let Err(why) = check_blit(case, draw.as_ref()) {
            failures.push(format!("{name}: {why}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(cases.len() >= 35);
}

#[test]
fn every_sequence_regenerates_when_upstream_does() {
    let doc = fixture();
    let images = &doc["images"];
    let mut failures = Vec::new();
    let mut regenerations = 0;
    for sequence in doc["sequences"].as_array().unwrap() {
        let name = sequence["name"].as_str().unwrap();
        let id = sequence["draw"].as_str().unwrap();
        let config = render_config(&sequence["renderConfig"], images);
        let mut run = Run::new();
        let mut before = None;
        for step in sequence["steps"].as_array().unwrap() {
            let label = step["label"].as_str().unwrap();
            let all = elements(&step["elements"]);
            let state = app_state(&step["appState"]);
            let scale = step["scale"].as_f64().unwrap();
            let base = Transform::scale(scale, scale).concat(&Transform::scale(state.zoom, state.zoom));
            run.draw(&all, id, &state, &config, scale, base);
            let now = run.cache.get(id).map(|e| e.surface.serial);
            let regenerated = now != before;
            if Some(regenerated) != step["regenerated"].as_bool() {
                failures.push(format!(
                    "{name} / {label}: regenerated {regenerated}, expected {}",
                    step["regenerated"]
                ));
            }
            regenerations += usize::from(regenerated);
            let cached = &step["cached"];
            if let Some(entry) = run.cache.get(id) {
                let key = &entry.surface.canvas.key;
                if !same(&cached["zoomValue"], key.zoom_value)
                    || !same(&cached["containingFrameOpacity"], key.containing_frame_opacity)
                    || theme(&cached["theme"]) != key.theme
                {
                    failures.push(format!("{name} / {label}: key {key:?}, expected {cached}"));
                }
            }
            before = now;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(regenerations >= 12, "only {regenerations} regenerations");
}

#[test]
fn padding_by_type() {
    let doc = fixture();
    let padding = |case: &str| {
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == case)
            .unwrap();
        let all = elements(&case["elements"]);
        let element = all.iter().find(|e| e.base.id == case["draw"].as_str().unwrap()).unwrap();
        (get_canvas_padding(element), element.clone())
    };
    // freedraw: strokeWidth × 12
    let (p, e) = padding("freedraw");
    assert_eq!(e.base.stroke_width, 4.0);
    assert_eq!(p, 48.0);
    assert_eq!(padding("freedraw-thin").0, 12.0);
    // text: fontSize / 2
    let (p, e) = padding("text");
    let ElementKind::Text(text) = &e.kind else { panic!() };
    assert_eq!(p, text.font_size / 2.0);
    assert_eq!(padding("text-rtl").0, 18.0);
    // arrows: 40 with an end arrowhead, else 20; upstream tests
    // `endArrowhead || endArrowhead`, so a start arrowhead alone is 20
    assert_eq!(padding("arrow-end-arrowhead").0, 40.0);
    assert_eq!(padding("arrow-no-arrowhead").0, 20.0);
    assert_eq!(padding("arrow-start-arrowhead-only").0, 20.0);
    // everything else: 20
    for case in ["rectangle", "ellipse", "diamond", "line-left-of-origin", "image"] {
        assert_eq!(padding(case).0, 20.0, "{case}");
    }
}

#[test]
fn canvas_size_is_capped_at_the_side_and_area_limits() {
    assert_eq!(AREA_LIMIT, 16_777_216.0);
    assert_eq!(WIDTH_HEIGHT_LIMIT, 32_767.0);
    let doc = fixture();
    let mut capped = 0;
    for case in doc["cases"].as_array().unwrap() {
        let all = elements(&case["elements"]);
        let map = ElementsMap::new(&all);
        let element = all.iter().find(|e| e.base.id == case["draw"].as_str().unwrap()).unwrap();
        let zoom = case["appState"]["zoom"]["value"].as_f64().unwrap();
        let size = capped_element_canvas_size(element, &map, zoom, case["scale"].as_f64().unwrap());
        assert!(size.width <= WIDTH_HEIGHT_LIMIT && size.height <= WIDTH_HEIGHT_LIMIT);
        assert!(size.width * size.height <= AREA_LIMIT);
        let cached = &case["cached"];
        if cached.is_null() {
            assert!(size.width == 0.0 || size.height == 0.0, "{}", case["name"]);
            continue;
        }
        assert!(same(&cached["width"], size.width), "{}", case["name"]);
        assert!(same(&cached["height"], size.height), "{}", case["name"]);
        assert!(same(&cached["scale"], size.scale), "{}", case["name"]);
        if size.scale != zoom {
            capped += 1;
        }
    }
    // side-cap, side-cap-zoom, area-cap, area-cap-zoom, both-caps
    assert_eq!(capped, 5);
}

#[test]
fn snapping_needs_a_right_angle_and_no_zoom_gesture() {
    use std::f64::consts::PI;
    assert_eq!(SNAP_TIE_BIAS, 1e-6);
    for angle in [0.0, PI / 2.0, PI, 3.0 * PI / 2.0, 2.0 * PI, -PI / 2.0] {
        assert!(can_snap_element(Radians(angle), false), "{angle}");
        assert!(!can_snap_element(Radians(angle), true), "{angle}");
    }
    for angle in [0.4, PI / 4.0, 1.0, PI / 2.0 + 1e-3] {
        assert!(!can_snap_element(Radians(angle), false), "{angle}");
    }
}

#[test]
fn exporting_draws_vectors_and_caches_nothing() {
    let doc = fixture();
    let case = &doc["cases"][0];
    let all = elements(&case["elements"]);
    let state = app_state(&case["appState"]);
    let mut config = render_config(&case["renderConfig"], &doc["images"]);
    config.is_exporting = true;
    let mut run = Run::new();
    let draw = run.draw(&all, "r", &state, &config, 1.0, Transform::IDENTITY);
    assert!(matches!(draw, Some(ElementDraw::Vector(_))));
    assert_eq!(run.made, 0);
}

#[test]
fn frames_draw_vectors_and_deleting_an_entry_regenerates() {
    let doc = fixture();
    let case = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "frame-opacity")
        .unwrap();
    let all = elements(&case["elements"]);
    let state = app_state(&case["appState"]);
    let config = render_config(&case["renderConfig"], &doc["images"]);
    let mut run = Run::new();
    let draw = run.draw(&all, "frame", &state, &config, 1.0, Transform::IDENTITY);
    assert!(matches!(draw, Some(ElementDraw::Vector(_))));
    assert_eq!(run.made, 0);
    run.draw(&all, "r", &state, &config, 1.0, Transform::IDENTITY);
    run.draw(&all, "r", &state, &config, 1.0, Transform::IDENTITY);
    assert_eq!(run.made, 1);
    // ShapeCache.delete(element) also drops its bitmap
    run.cache.delete("r");
    run.draw(&all, "r", &state, &config, 1.0, Transform::IDENTITY);
    assert_eq!(run.made, 2);
    // an element no longer in the scene lets its bitmap go (the WeakMap)
    run.cache.retain(|id| id != "r");
    assert!(run.cache.get("r").is_none());
}
