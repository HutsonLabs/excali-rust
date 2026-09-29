//! The static scene in the editor (ex-710): `renderStaticScene`
//! (`packages/excalidraw/renderer/staticScene.ts:274-545`) calls
//! `renderElement` for every visible element, bound text, embeddable and
//! pending flowchart node, and outside export `renderElement` draws each
//! element but a frame from a bitmap of its own
//! (`packages/element/src/renderElement.ts:963-1009`, `:682-728`) made once
//! per element version. [`render_static_scene_cached`] is that path: each
//! element's draw is exactly the blit [`render_element_cached`] gives on the
//! scene's matrix (the device pixel ratio times the zoom), the bitmaps come
//! from one [`ElementCanvasCache`] that a pan does not invalidate, and an
//! export stays the vector scene of [`render_static_scene`].
//!
//! The bitmaps and blits themselves are held to upstream by
//! `tests/element_canvas.rs` (fixture `element-canvas.json`).

use std::collections::HashMap;

use excali_core::element::Element;
use excali_scene::bounds::ElementsMap;
use excali_scene::display::{
    bitmap_id, Blit, Clip, DisplayItem, DisplayList, FillRule, ImageItem, PaintState, Painter,
    Path, Rect, Rgba, Stroke, TextRun, Transform,
};
use excali_scene::element_canvas::{
    render_element_cached, ElementCanvas, ElementCanvasCache, ElementDraw,
};
use excali_scene::static_scene::{
    render_static_scene, render_static_scene_cached, StaticCanvasAppState,
    StaticCanvasRenderConfig, StaticScene,
};
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{json, Value};

struct TenPxPerCodeUnit;

impl TextMetricsProvider for TenPxPerCodeUnit {
    fn get_line_width(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

fn element(fields: Value) -> Element {
    let mut base = json!({
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "#a5d8ff",
        "fillStyle": "hachure", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "roundness": null, "seed": 7,
        "version": 1, "versionNonce": 1, "isDeleted": false, "boundElements": null,
        "updated": 1, "link": null, "locked": false,
    });
    let map = base.as_object_mut().unwrap();
    for (k, v) in fields.as_object().unwrap() {
        map.insert(k.clone(), v.clone());
    }
    Element::from_map(map.clone()).unwrap()
}

/// A rectangle, an ellipse, a labelled rectangle, free text, an arrow, a
/// freedraw stroke and a frame holding the ellipse.
fn scene() -> Vec<Element> {
    vec![
        element(
            json!({"id": "frame", "type": "frame", "x": 200, "y": 0, "width": 150,
            "height": 120, "index": "a0", "name": "F", "backgroundColor": "transparent"}),
        ),
        element(
            json!({"id": "rect", "type": "rectangle", "x": 10.25, "y": 20.5, "width": 80,
            "height": 40, "index": "a1", "roundness": {"type": 3}}),
        ),
        element(
            json!({"id": "ell", "type": "ellipse", "x": 220, "y": 30, "width": 60,
            "height": 50, "index": "a2", "frameId": "frame", "fillStyle": "cross-hatch"}),
        ),
        element(
            json!({"id": "box", "type": "rectangle", "x": 10, "y": 100, "width": 100,
            "height": 50, "index": "a3", "boundElements": [{"id": "label", "type": "text"}]}),
        ),
        element(
            json!({"id": "label", "type": "text", "x": 35, "y": 112.5, "width": 50,
            "height": 25, "index": "a4", "text": "label", "originalText": "label",
            "fontSize": 20, "fontFamily": 5, "textAlign": "center", "verticalAlign": "middle",
            "containerId": "box", "autoResize": true, "lineHeight": 1.25,
            "backgroundColor": "transparent"}),
        ),
        element(
            json!({"id": "free", "type": "text", "x": 400, "y": 10, "width": 40,
            "height": 25, "index": "a5", "text": "hi", "originalText": "hi", "fontSize": 20,
            "fontFamily": 5, "textAlign": "left", "verticalAlign": "top", "containerId": null,
            "autoResize": true, "lineHeight": 1.25, "backgroundColor": "transparent"}),
        ),
        element(
            json!({"id": "arrow", "type": "arrow", "x": 120, "y": 200, "width": 100,
            "height": 30, "index": "a6", "points": [[0, 0], [50, 30], [100, 0]],
            "lastCommittedPoint": null, "startBinding": null, "endBinding": null,
            "startArrowhead": null, "endArrowhead": "arrow", "elbowed": false,
            "roundness": {"type": 2}}),
        ),
        element(
            json!({"id": "pen", "type": "freedraw", "x": 300, "y": 200, "width": 40,
            "height": 20, "index": "a7", "points": [[0, 0], [10, 5], [20, 20], [40, 10]],
            "pressures": [], "simulatePressure": true, "lastCommittedPoint": null,
            "strokeOptions": {"variability": "variable", "streamline": 0.5}}),
        ),
    ]
}

const SCALE: f64 = 2.0;

fn app_state(scroll_x: f64, scroll_y: f64) -> StaticCanvasAppState {
    StaticCanvasAppState {
        zoom: 1.5,
        scroll_x,
        scroll_y,
        view_background_color: Some("#ffffff".into()),
        ..StaticCanvasAppState::default()
    }
}

struct Frame {
    list: DisplayList,
    /// The element ids a bitmap was made for, in order.
    made: Vec<String>,
}

fn render(
    elements: &[Element],
    state: &StaticCanvasAppState,
    config: &StaticCanvasRenderConfig,
    cache: &mut ElementCanvasCache<String>,
) -> Frame {
    let visible: Vec<&Element> = elements.iter().collect();
    let map = ElementsMap::new(visible.iter().copied());
    let mut made = Vec::new();
    let list = render_static_scene_cached(
        &StaticScene {
            canvas_width: 1000.0 * SCALE,
            canvas_height: 700.0 * SCALE,
            scale: SCALE,
            elements_map: &map,
            all_elements_map: &map,
            visible_elements: &visible,
            app_state: state,
            render_config: config,
            text_metrics: &TenPxPerCodeUnit,
        },
        cache,
        &mut |element: &Element, _: ElementCanvas| {
            made.push(element.base.id.clone());
            element.base.id.clone()
        },
    );
    Frame { list, made }
}

fn vector(
    elements: &[Element],
    state: &StaticCanvasAppState,
    config: &StaticCanvasRenderConfig,
) -> DisplayList {
    let visible: Vec<&Element> = elements.iter().collect();
    let map = ElementsMap::new(visible.iter().copied());
    render_static_scene(&StaticScene {
        canvas_width: 1000.0 * SCALE,
        canvas_height: 700.0 * SCALE,
        scale: SCALE,
        elements_map: &map,
        all_elements_map: &map,
        visible_elements: &visible,
        app_state: state,
        render_config: config,
        text_metrics: &TenPxPerCodeUnit,
    })
}

fn editor_config() -> StaticCanvasRenderConfig {
    StaticCanvasRenderConfig {
        render_grid: false,
        ..StaticCanvasRenderConfig::default()
    }
}

fn blits(items: &[DisplayItem], out: &mut Vec<Blit>) {
    for item in items {
        match item {
            DisplayItem::Blit(blit) => out.push(blit.clone()),
            DisplayItem::Group(group) => blits(&group.items, out),
            _ => {}
        }
    }
}

fn all_blits(list: &DisplayList) -> Vec<Blit> {
    let mut out = Vec::new();
    blits(&list.items, &mut out);
    out
}

#[test]
fn every_element_but_a_frame_is_drawn_from_its_bitmap() {
    let elements = scene();
    let mut cache = ElementCanvasCache::new();
    let frame = render(
        &elements,
        &app_state(12.3, -4.7),
        &editor_config(),
        &mut cache,
    );
    let ids: Vec<String> = all_blits(&frame.list).into_iter().map(|b| b.id).collect();
    // the label right after its container; the frame is drawn as vectors
    let want = ["rect", "ell", "box", "label", "free", "arrow", "pen"];
    assert_eq!(ids, want.iter().map(|id| bitmap_id(id)).collect::<Vec<_>>());
    assert_eq!(frame.made, want);
    assert_eq!(cache.len(), want.len());
}

#[test]
fn each_blit_is_render_element_cached_on_the_scene_matrix() {
    let elements = scene();
    let state = app_state(12.3, -4.7);
    let config = editor_config();
    let mut cache = ElementCanvasCache::new();
    let frame = render(&elements, &state, &config, &mut cache);
    let got: HashMap<String, Blit> = all_blits(&frame.list)
        .into_iter()
        .map(|b| (b.id.clone(), b))
        .collect();

    let visible: Vec<&Element> = elements.iter().collect();
    let map = ElementsMap::new(visible.iter().copied());
    // the scroll the scene snaps to device pixels before drawing
    let (sx, sy) = excali_scene::static_scene::snap_scroll_to_device_pixels(
        state.scroll_x,
        state.scroll_y,
        state.zoom,
        SCALE,
    );
    let snapped = StaticCanvasAppState {
        scroll_x: sx,
        scroll_y: sy,
        ..state.clone()
    };
    let base = Transform::scale(SCALE, SCALE).concat(&Transform::scale(state.zoom, state.zoom));
    let mut fresh = ElementCanvasCache::new();
    for e in elements.iter().filter(|e| e.base.id != "frame") {
        let draw = render_element_cached(
            e,
            &map,
            &map,
            &config,
            &snapped,
            SCALE,
            base,
            None,
            &mut fresh,
            |_| (),
        )
        .unwrap()
        .unwrap();
        let ElementDraw::Blit(want) = draw else {
            panic!("{} drawn as vectors", e.base.id);
        };
        assert_eq!(got[&bitmap_id(&e.base.id)], want, "{}", e.base.id);
    }
}

#[test]
fn a_pan_reuses_every_bitmap() {
    let elements = scene();
    let config = editor_config();
    let mut cache = ElementCanvasCache::new();
    let first = render(&elements, &app_state(0.0, 0.0), &config, &mut cache);
    assert_eq!(first.made.len(), 7);
    for (i, (x, y)) in [(3.0, 2.0), (-40.5, 17.25), (1000.0, -800.0)]
        .into_iter()
        .enumerate()
    {
        let panned = render(&elements, &app_state(x, y), &config, &mut cache);
        assert!(panned.made.is_empty(), "pan {i} made {:?}", panned.made);
        assert_eq!(all_blits(&panned.list).len(), 7);
    }
}

#[test]
fn a_new_version_remakes_only_that_bitmap() {
    let mut elements = scene();
    let config = editor_config();
    let mut cache = ElementCanvasCache::new();
    render(&elements, &app_state(0.0, 0.0), &config, &mut cache);
    let rect = elements.iter_mut().find(|e| e.base.id == "rect").unwrap();
    rect.base.version += 1.0;
    rect.base.version_nonce += 1.0;
    rect.base.width = 120.0;
    let next = render(&elements, &app_state(0.0, 0.0), &config, &mut cache);
    assert_eq!(next.made, ["rect"]);
}

#[test]
fn a_zoom_or_theme_change_remakes_every_bitmap() {
    let elements = scene();
    let config = editor_config();
    let mut cache = ElementCanvasCache::new();
    render(&elements, &app_state(0.0, 0.0), &config, &mut cache);
    let zoomed = StaticCanvasAppState {
        zoom: 2.0,
        ..app_state(0.0, 0.0)
    };
    assert_eq!(
        render(&elements, &zoomed, &config, &mut cache).made.len(),
        7
    );
    let dark = StaticCanvasAppState {
        theme: excali_scene::shape::Theme::Dark,
        ..zoomed
    };
    assert_eq!(render(&elements, &dark, &config, &mut cache).made.len(), 7);
}

#[test]
fn an_export_is_the_vector_scene() {
    let elements = scene();
    let state = app_state(12.3, -4.7);
    let config = StaticCanvasRenderConfig {
        is_exporting: true,
        ..editor_config()
    };
    let mut cache = ElementCanvasCache::new();
    let frame = render(&elements, &state, &config, &mut cache);
    assert!(frame.made.is_empty());
    assert!(cache.is_empty());
    assert_eq!(frame.list, vector(&elements, &state, &config));
}

#[test]
fn without_elements_the_scene_is_the_vector_scene() {
    // background and grid only: nothing to cache, the same list
    let config = StaticCanvasRenderConfig {
        render_grid: true,
        ..StaticCanvasRenderConfig::default()
    };
    let state = app_state(5.0, 5.0);
    let mut cache = ElementCanvasCache::new();
    assert_eq!(
        render(&[], &state, &config, &mut cache).list,
        vector(&[], &state, &config)
    );
}

// ---------------------------------------------------------------------------
// Replaying a blit

#[derive(Default)]
struct Recorder(Vec<String>);

impl Painter for Recorder {
    fn fill(
        &mut self,
        _: &Path,
        _: &excali_scene::display::Color,
        _: Rgba,
        _: FillRule,
        _: &PaintState,
    ) {
        self.0.push("fill".into());
    }
    fn stroke(&mut self, _: &Path, _: &Stroke, _: Rgba, _: &PaintState) {
        self.0.push("stroke".into());
    }
    fn image(&mut self, image: &ImageItem, state: &PaintState) {
        self.0.push(format!(
            "image {} {:?} {:?} smoothing {} at {:?} alpha {}",
            image.id, image.source, image.dest, image.smoothing, state.transform, state.alpha
        ));
    }
    fn text(&mut self, _: &TextRun, _: Rgba, _: &PaintState) {
        self.0.push("text".into());
    }
    fn push_clip(&mut self, clip: &Clip, transform: &Transform) {
        self.0.push(format!("clip {:?} {:?}", clip.rule, transform));
    }
    fn pop_clip(&mut self) {
        self.0.push("unclip".into());
    }
}

#[test]
fn a_blit_replays_as_one_image_under_its_absolute_matrix() {
    // drawElementFromCanvas: save, globalAlpha, the clip under its own
    // matrix, setTransform to the blit's matrix, drawImage, restore; the
    // enclosing groups' matrices and alpha do not apply
    let clip_matrix = Transform::scale(2.0, 2.0);
    let at = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 31.0,
        f: 57.0,
    };
    let blit = Blit {
        id: bitmap_id("rect"),
        alpha: 0.5,
        smoothing: Some(false),
        clip: Some((
            Clip {
                path: Path::rect(0.0, 0.0, 10.0, 10.0),
                rule: FillRule::EvenOdd,
            },
            clip_matrix,
        )),
        transform: at,
        dest: Rect::new(0.0, 0.0, 44.0, 33.0),
    };
    let list = DisplayList::from_iter([DisplayItem::Group(excali_scene::display::Group {
        transform: Transform::scale(3.0, 3.0),
        opacity: 0.25,
        ..excali_scene::display::Group::new(vec![
            DisplayItem::Blit(blit.clone()),
            DisplayItem::Blit(Blit {
                smoothing: None,
                clip: None,
                ..blit
            }),
        ])
    })]);
    let mut rec = Recorder::default();
    list.replay(&mut rec);
    let image = |smoothing: bool| {
        format!(
            "image bitmap:rect None {:?} smoothing {smoothing} at {at:?} alpha 0.5",
            Rect::new(0.0, 0.0, 44.0, 33.0)
        )
    };
    assert_eq!(
        rec.0,
        [
            format!("clip EvenOdd {clip_matrix:?}"),
            image(false),
            "unclip".to_owned(),
            // no smoothing set: the context's default, on
            image(true),
        ]
    );
}
