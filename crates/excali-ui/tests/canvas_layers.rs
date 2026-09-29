//! The layered canvases (ex-503): the static, new-element and interactive
//! canvases at device-pixel scale (`components/canvases/StaticCanvas.tsx`,
//! `NewElementCanvas.tsx`, `InteractiveCanvas.tsx`), the helpers every
//! layer renders through (`renderer/helpers.ts`: `snapScrollToDevicePixels`,
//! `getNormalizedCanvasDimensions`, `bootstrapCanvas`) and the new-element
//! scene (`renderer/renderNewElementScene.ts`), against upstream.
//!
//! Fixture: `tests/fixtures/canvas-layers.json`, upstream's own helpers and
//! `renderNewElementScene` at the pinned commit on the recording 2D context
//! (`tools/goldens/canvas-layers.mjs`): snapped scrolls, each layer's
//! bootstrap draws on a canvas sized as its component sizes it, and every
//! draw of the new-element canvas per scene.
//!
//! The browser half (`tests/web/layers`, `scripts/web/canvas-layers.sh`)
//! mounts the layers in Chromium at several device pixel ratios and holds
//! their backing sizes, order and pixels to these rules.

use std::collections::HashSet;

use excali_canvas2d::Context2d;
use excali_core::element::Element;
use excali_scene::bounds::ElementsMap;
use excali_scene::display::{DisplayList, Rect, Transform};
use excali_scene::export::FrameRendering;
use excali_scene::new_element_scene::{
    is_invisibly_small_element, render_new_element_scene, NewElementScene,
};
use excali_scene::shape::Theme;
use excali_scene::static_scene::{
    render_static_scene, StaticCanvasAppState, StaticCanvasRenderConfig, StaticScene,
};
use excali_text::text_measurements::TextMetricsProvider;
use excali_ui::layers::{
    bootstrap_canvas, canvas_dimension_from_attribute, canvas_dimension_from_property,
    is_opaque_hex_color, normalized_canvas_dimensions, paint_interactive_layer,
    paint_new_element_layer, paint_static_layer, snap_scroll_to_device_pixels, BackingSize,
    Layer, LayerContext, SceneViewport, CANVAS_LAYER_CSS, DEFAULT_CANVAS_HEIGHT,
    DEFAULT_CANVAS_WIDTH,
};
use serde_json::Value;

#[path = "../../excali-scene/tests/support/draws.rs"]
mod draws;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/canvas-layers.json")).unwrap()
}

fn cases(key: &str) -> Vec<Value> {
    fixture()[key].as_array().unwrap().clone()
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn same(a: f64, b: f64) -> bool {
    // JSON writes -0 as 0
    a == b || (a.is_nan() && b.is_nan())
}

// ---------------------------------------------------------------------------
// A 2D context that keeps the matrix and records clears and draws

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Clear { m: Transform, rect: Rect },
    Draw(&'static str),
}

#[derive(Default)]
struct Recording {
    m: Vec<Transform>,
    calls: Vec<Call>,
}

impl Recording {
    fn matrix(&self) -> Transform {
        self.m.last().copied().unwrap_or(Transform::IDENTITY)
    }

    fn clears(&self) -> Vec<(Transform, Rect)> {
        self.calls
            .iter()
            .filter_map(|c| match c {
                Call::Clear { m, rect } => Some((*m, *rect)),
                Call::Draw(_) => None,
            })
            .collect()
    }

    fn draws(&self) -> usize {
        self.calls
            .iter()
            .filter(|c| matches!(c, Call::Draw(_)))
            .count()
    }
}

impl Context2d for Recording {
    fn save(&mut self) {
        let m = self.matrix();
        self.m.push(m);
    }
    fn restore(&mut self) {
        // the base matrix is never popped
        if self.m.len() > 1 {
            self.m.pop();
        }
    }
    fn set_transform(&mut self, t: &Transform) {
        if self.m.is_empty() {
            self.m.push(*t);
        } else {
            *self.m.last_mut().unwrap() = *t;
        }
    }
    fn set_global_alpha(&mut self, _: f64) {}
    fn set_fill_style(&mut self, _: &str) {}
    fn set_stroke_style(&mut self, _: &str) {}
    fn set_line_width(&mut self, _: f64) {}
    fn set_line_cap(&mut self, _: &str) {}
    fn set_line_join(&mut self, _: &str) {}
    fn set_miter_limit(&mut self, _: f64) {}
    fn set_line_dash(&mut self, _: &[f64]) {}
    fn set_line_dash_offset(&mut self, _: f64) {}
    fn begin_path(&mut self) {}
    fn move_to(&mut self, _: f64, _: f64) {}
    fn line_to(&mut self, _: f64, _: f64) {}
    fn quadratic_curve_to(&mut self, _: f64, _: f64, _: f64, _: f64) {}
    fn bezier_curve_to(&mut self, _: f64, _: f64, _: f64, _: f64, _: f64, _: f64) {}
    fn arc(&mut self, _: f64, _: f64, _: f64, _: f64, _: f64, _: bool) {}
    fn close_path(&mut self) {}
    fn fill(&mut self, _: &str) {
        self.calls.push(Call::Draw("fill"));
    }
    fn fill_rect(&mut self, _: &Rect) {
        self.calls.push(Call::Draw("fillRect"));
    }
    fn stroke(&mut self) {
        self.calls.push(Call::Draw("stroke"));
    }
    fn clip(&mut self, _: &str) {}
    fn set_font(&mut self, _: &str) {}
    fn set_text_align(&mut self, _: &str) {}
    fn set_direction(&mut self, _: &str) {}
    fn fill_text(&mut self, _: &str, _: f64, _: f64) {
        self.calls.push(Call::Draw("text"));
    }
    fn set_image_smoothing_enabled(&mut self, _: bool) {}
    fn set_filter(&mut self, _: &str) {}
    fn image_size(&self, _: &str) -> Option<(f64, f64)> {
        None
    }
    fn draw_image(&mut self, _: &str, _: &Rect, _: &Rect) {
        self.calls.push(Call::Draw("image"));
    }
}

impl LayerContext for Recording {
    fn clear_rect(&mut self, rect: &Rect) {
        let m = self.matrix();
        self.calls.push(Call::Clear { m, rect: *rect });
    }
}

/// The `clear` events of upstream's draws, as `(matrix, rect)`.
fn upstream_clears(events: &Value) -> Vec<(Transform, Rect)> {
    events
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["op"] == "clear")
        .map(|e| {
            let r: Vec<f64> = e["rect"].as_array().unwrap().iter().map(num).collect();
            (draws::matrix(&e["m"]), Rect::new(r[0], r[1], r[2], r[3]))
        })
        .collect()
}

fn same_clears(ours: &[(Transform, Rect)], theirs: &[(Transform, Rect)]) -> Result<(), String> {
    if ours.len() != theirs.len() {
        return Err(format!("{} clears, upstream {}", ours.len(), theirs.len()));
    }
    for (i, ((m, r), (um, ur))) in ours.iter().zip(theirs).enumerate() {
        if !draws::same_matrix(m, um) {
            return Err(format!("clear #{i}: matrix {m:?}, upstream {um:?}"));
        }
        let (a, b) = (
            [r.x, r.y, r.width, r.height],
            [ur.x, ur.y, ur.width, ur.height],
        );
        if !a.iter().zip(&b).all(|(x, y)| same(*x, *y)) {
            return Err(format!("clear #{i}: rect {a:?}, upstream {b:?}"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// snapScrollToDevicePixels (helpers.ts:47-63)

#[test]
fn scroll_snaps_as_upstream_on_every_case() {
    let all = cases("snapScroll");
    assert!(all.len() >= 20);
    for case in &all {
        let (sx, sy, zoom, scale) = (
            num(&case["scrollX"]),
            num(&case["scrollY"]),
            num(&case["zoom"]),
            num(&case["scale"]),
        );
        let got = snap_scroll_to_device_pixels(sx, sy, zoom, scale);
        let want = (
            num(&case["result"]["scrollX"]),
            num(&case["result"]["scrollY"]),
        );
        assert!(
            same(got.0, want.0) && same(got.1, want.1),
            "snap({sx}, {sy}, zoom {zoom}, dpr {scale}) = {got:?}, upstream {want:?}"
        );
        // identity: upstream returns its argument when nothing moves
        let viewport = SceneViewport::new(sx, sy, zoom, scale);
        assert_eq!(
            viewport.is_snapped(),
            case["identity"].as_bool().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn snapped_scroll_is_round_scroll_times_zoom_times_dpr_over_zoom_times_dpr() {
    // pixelSnap.test.tsx "rounds the scroll to whole device pixels":
    // 3.3 × 1.5 × 2 = 9.9 → 10; -7.77 × 3 = -23.31 → -23
    let v = SceneViewport::new(3.3, -7.77, 1.5, 2.0).snapped();
    assert!((v.scroll_x * 3.0 - 10.0).abs() < 1e-9);
    assert!((v.scroll_y * 3.0 - -23.0).abs() < 1e-9);
    // … "and is identity when it already is"
    let whole = SceneViewport::new(4.0, -6.0, 1.5, 2.0);
    assert!(whole.is_snapped());
    assert_eq!(whole.snapped(), whole);
    // pixelSnap.test.tsx "a tenth of a device pixel further lands on the
    // same pixels"
    for zoom in [1.0, 1.5] {
        let at = SceneViewport::new(3.3, -7.77, zoom, 1.0).snapped();
        let further = SceneViewport::new(3.3 + 0.1 / zoom, -7.77, zoom, 1.0).snapped();
        assert_eq!(at, further);
    }
    // the scene transform is dpr × zoom, and the snapped scroll puts the
    // scene origin on a whole device pixel
    for (sx, sy, zoom, dpr) in [(3.3, -7.77, 1.5, 2.0), (0.3, 0.7, 0.73, 1.25), (-37.25, 113.5, 1.25, 1.75)] {
        let v = SceneViewport::new(sx, sy, zoom, dpr).snapped();
        let t = v.transform();
        assert_eq!(t, Transform::scale(dpr, dpr).concat(&Transform::scale(zoom, zoom)));
        for s in [v.scroll_x, v.scroll_y] {
            let device = s * zoom * dpr;
            assert!((device - device.round()).abs() < 1e-9, "{device}");
        }
        // under half a device pixel from the real scroll
        assert!(((v.scroll_x - sx) * zoom * dpr).abs() <= 0.5 + 1e-9);
        assert!(((v.scroll_y - sy) * zoom * dpr).abs() <= 0.5 + 1e-9);
    }
    // no device pixels: unchanged
    for (zoom, dpr) in [(0.0, 1.0), (1.0, 0.0), (-1.0, 1.0), (f64::NAN, 1.0), (1.0, f64::NAN)] {
        let v = SceneViewport::new(3.3, -7.77, zoom, dpr);
        assert!(v.is_snapped());
        assert_eq!((v.snapped().scroll_x, v.snapped().scroll_y), (3.3, -7.77));
    }
}

// ---------------------------------------------------------------------------
// Backing size = CSS size × devicePixelRatio

#[test]
fn layers_mount_in_upstreams_order_with_its_classes() {
    // App.tsx:2654, 2693, 2724: StaticCanvas, NewElementCanvas,
    // InteractiveCanvas
    assert_eq!(
        Layer::ALL,
        [Layer::Static, Layer::NewElement, Layer::Interactive]
    );
    assert_eq!(Layer::Static.class_name(), "excalidraw__canvas static");
    assert_eq!(Layer::NewElement.class_name(), "excalidraw__canvas");
    assert_eq!(
        Layer::Interactive.class_name(),
        "excalidraw__canvas interactive"
    );
}

#[test]
fn backing_size_is_css_size_times_device_pixel_ratio() {
    for layer in Layer::ALL {
        for (w, h, dpr, bw, bh) in [
            (800.0, 600.0, 1.0, 800, 600),
            (800.0, 600.0, 2.0, 1600, 1200),
            (800.0, 600.0, 3.0, 2400, 1800),
            (800.0, 600.0, 1.5, 1200, 900),
            // the canvas keeps whole pixels: 1001.25 → 1001
            (801.0, 601.0, 1.25, 1001, 751),
            (333.3, 222.2, 1.5, 499, 333),
            (1280.0, 720.0, 2.625, 3360, 1890),
            (1.0, 1.0, 0.5, 0, 0),
        ] {
            assert_eq!(
                layer.backing_size(w, h, dpr),
                BackingSize {
                    width: bw,
                    height: bh
                },
                "{layer:?} {w}x{h} at {dpr}"
            );
        }
    }
}

#[test]
fn backing_size_matches_upstreams_canvases() {
    // the fixture's canvases were sized by upstream's components' own
    // assignments on jsdom's HTMLCanvasElement
    for case in cases("bootstrap") {
        let layer = match case["layer"].as_str().unwrap() {
            "static" => Layer::Static,
            "new-element" => Layer::NewElement,
            _ => Layer::Interactive,
        };
        let size = layer.backing_size(num(&case["width"]), num(&case["height"]), num(&case["scale"]));
        assert_eq!(
            (size.width as f64, size.height as f64),
            (num(&case["canvasWidth"]), num(&case["canvasHeight"])),
            "{}",
            case["name"]
        );
        let n = normalized_canvas_dimensions(size, num(&case["scale"]));
        let want = case["normalized"].as_array().unwrap();
        assert_eq!((n.0, n.1), (num(&want[0]), num(&want[1])), "{}", case["name"]);
    }
    for scene in cases("newElementScenes") {
        let size = Layer::NewElement.backing_size(
            num(&scene["width"]),
            num(&scene["height"]),
            num(&scene["scale"]),
        );
        assert_eq!(
            (size.width as f64, size.height as f64),
            (num(&scene["canvasWidth"]), num(&scene["canvasHeight"])),
            "{}",
            scene["name"]
        );
    }
}

#[test]
fn canvas_width_conversions_follow_the_html_canvas() {
    // `canvas.width = x` (StaticCanvas.tsx:40): WebIDL unsigned long
    // (ToNumber, truncate, modulo 2^32), then the reflected attribute's
    // range 0..=2^31-1, else the default
    assert_eq!(DEFAULT_CANVAS_WIDTH, 300);
    assert_eq!(DEFAULT_CANVAS_HEIGHT, 150);
    let p = |x| canvas_dimension_from_property(x, DEFAULT_CANVAS_WIDTH);
    assert_eq!(p(800.0), 800);
    assert_eq!(p(1001.25), 1001);
    assert_eq!(p(1001.999), 1001);
    assert_eq!(p(0.4), 0);
    assert_eq!(p(1e-7), 0);
    assert_eq!(p(-0.5), 0);
    assert_eq!(p(-1.0), 300);
    assert_eq!(p(f64::NAN), 0);
    assert_eq!(p(f64::INFINITY), 0);
    assert_eq!(p(2147483647.0), 2147483647);
    assert_eq!(p(2147483648.0), 300);
    assert_eq!(p(4294967296.0 + 5.0), 5);
    // `width={x}` (NewElementCanvas.tsx:56, InteractiveCanvas.tsx:208):
    // React writes String(x), which the canvas reads with the rules for
    // parsing non-negative integers, the default when that fails
    let a = |x| canvas_dimension_from_attribute(x, DEFAULT_CANVAS_HEIGHT);
    assert_eq!(a(800.0), 800);
    assert_eq!(a(1001.25), 1001);
    assert_eq!(a(0.4), 0);
    assert_eq!(a(-0.0), 0);
    assert_eq!(a(-1.0), 150);
    assert_eq!(a(-0.5), 150);
    assert_eq!(a(f64::NAN), 150);
    assert_eq!(a(f64::INFINITY), 150);
    // String(1e-7) is "1e-7", String(1e21) "1e+21": the digits before the e
    assert_eq!(a(1e-7), 1);
    assert_eq!(a(1e21), 1);
    assert_eq!(a(2147483647.0), 2147483647);
    assert_eq!(a(2147483648.0), 150);
}

// ---------------------------------------------------------------------------
// bootstrapCanvas (helpers.ts:73-127) per layer

#[test]
fn opaque_hex_colours_skip_the_clear() {
    for c in ["#fff", "#FFF", "#abcdef", "#ABCDEF", "#123456"] {
        assert!(is_opaque_hex_color(c), "{c}");
    }
    for c in [
        "#ffff", "#ffffff80", "#ffffffff", "fff", "#ggg", "white", "", "transparent",
        "#ffffff ", " #ffffff", "rgba(0,0,0,1)",
    ] {
        assert!(!is_opaque_hex_color(c), "{c}");
    }
}

fn static_state(case: &Value) -> StaticCanvasAppState {
    let state = &case["appState"];
    StaticCanvasAppState {
        theme: if state["theme"] == "dark" {
            Theme::Dark
        } else {
            Theme::Light
        },
        view_background_color: state["viewBackgroundColor"].as_str().map(str::to_owned),
        ..StaticCanvasAppState::default()
    }
}

struct TenPxPerCodeUnit;

impl TextMetricsProvider for TenPxPerCodeUnit {
    fn get_line_width(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

#[test]
fn every_layer_bootstraps_as_upstream() {
    let all = cases("bootstrap");
    let mut layers = HashSet::new();
    for case in &all {
        let name = case["name"].as_str().unwrap();
        let scale = num(&case["scale"]);
        let (w, h) = (num(&case["width"]), num(&case["height"]));
        let mut ctx = Recording::default();
        let layer = case["layer"].as_str().unwrap();
        layers.insert(layer.to_owned());
        match layer {
            "static" => {
                let size = Layer::Static.backing_size(w, h, scale);
                let state = static_state(case);
                let config = StaticCanvasRenderConfig {
                    render_grid: false,
                    ..StaticCanvasRenderConfig::default()
                };
                let empty = ElementsMap::new([]);
                let list = render_static_scene(&StaticScene {
                    canvas_width: size.width as f64,
                    canvas_height: size.height as f64,
                    scale,
                    elements_map: &empty,
                    all_elements_map: &empty,
                    visible_elements: &[],
                    app_state: &state,
                    render_config: &config,
                    text_metrics: &TenPxPerCodeUnit,
                });
                paint_static_layer(
                    &mut ctx,
                    size,
                    scale,
                    state.view_background_color.as_deref(),
                    &list,
                );
                // the background fill is the scene's first draw
                draws::compare(&list, &case["events"], &Value::Null)
                    .unwrap_or_else(|why| panic!("{name}: {why}"));
            }
            "new-element" => {
                let size = Layer::NewElement.backing_size(w, h, scale);
                let n = normalized_canvas_dimensions(size, scale);
                bootstrap_canvas(&mut ctx, scale, n, None);
            }
            _ => {
                let size = Layer::Interactive.backing_size(w, h, scale);
                paint_interactive_layer(&mut ctx, size, scale, &DisplayList::default());
            }
        }
        same_clears(&ctx.clears(), &upstream_clears(&case["events"]))
            .unwrap_or_else(|why| panic!("{name}: {why}"));
        // clears come before anything is drawn
        let first_draw = ctx.calls.iter().position(|c| matches!(c, Call::Draw(_)));
        let last_clear = ctx.calls.iter().rposition(|c| matches!(c, Call::Clear { .. }));
        if let (Some(d), Some(c)) = (first_draw, last_clear) {
            assert!(c < d, "{name}: a clear after a draw");
        }
    }
    assert_eq!(layers.len(), 3);
}

// ---------------------------------------------------------------------------
// renderNewElementScene (renderNewElementScene.ts)

fn element(v: &Value) -> Element {
    Element::from_map(v.as_object().unwrap().clone()).unwrap()
}

fn ids(value: &Value) -> HashSet<String> {
    value
        .as_object()
        .map(|m| {
            m.iter()
                .filter(|(_, v)| v.as_bool() == Some(true))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn app_state(value: &Value) -> StaticCanvasAppState {
    let fr = &value["frameRendering"];
    StaticCanvasAppState {
        zoom: num(&value["zoom"]["value"]),
        scroll_x: num(&value["scrollX"]),
        scroll_y: num(&value["scrollY"]),
        theme: if value["theme"] == "dark" {
            Theme::Dark
        } else {
            Theme::Light
        },
        view_background_color: value["viewBackgroundColor"].as_str().map(str::to_owned),
        grid_size: num(&value["gridSize"]),
        grid_step: num(&value["gridStep"]),
        frame_rendering: FrameRendering {
            enabled: fr["enabled"].as_bool().unwrap(),
            clip: fr["clip"].as_bool().unwrap(),
            name: fr["name"].as_bool().unwrap(),
            outline: fr["outline"].as_bool().unwrap(),
        },
        selected_element_ids: ids(&value["selectedElementIds"]),
        hovered_element_ids: ids(&value["hoveredElementIds"]),
        open_dialog: None,
        frame_to_highlight: match &value["frameToHighlight"] {
            Value::Null => None,
            frame => Some(element(frame)),
        },
        selected_elements_are_being_dragged: value["selectedElementsAreBeingDragged"]
            .as_bool()
            .unwrap(),
        editing_group_id: value["editingGroupId"].as_str().map(str::to_owned),
    }
}

fn render_config(value: &Value) -> StaticCanvasRenderConfig {
    StaticCanvasRenderConfig {
        canvas_background_color: value["canvasBackgroundColor"].as_str().unwrap().to_owned(),
        render_grid: value["renderGrid"].as_bool().unwrap(),
        is_exporting: value["isExporting"].as_bool().unwrap(),
        theme: if value["theme"] == "dark" {
            Theme::Dark
        } else {
            Theme::Light
        },
        location_host: "excalidraw.com".to_owned(),
        ..StaticCanvasRenderConfig::default()
    }
}

#[test]
fn every_new_element_scene_draws_what_upstream_draws() {
    let mut failures = Vec::new();
    let mut compared = 0;
    let all = cases("newElementScenes");
    assert!(all.len() >= 25);
    for scene in &all {
        let name = scene["name"].as_str().unwrap();
        let elements: Vec<Element> = scene["elements"].as_array().unwrap().iter().map(element).collect();
        let new_element = match &scene["newElement"] {
            Value::Null => None,
            v => Some(element(v)),
        };
        let map = ElementsMap::new(&elements);
        let state = app_state(&scene["appState"]);
        let config = render_config(&scene["renderConfig"]);
        let scale = num(&scene["scale"]);
        let size = Layer::NewElement.backing_size(num(&scene["width"]), num(&scene["height"]), scale);
        let drawing = render_new_element_scene(&NewElementScene {
            canvas_width: size.width as f64,
            canvas_height: size.height as f64,
            scale,
            new_element: new_element.as_ref(),
            elements_map: &map,
            all_elements_map: &map,
            app_state: &state,
            render_config: &config,
        });
        let mut ctx = Recording::default();
        paint_new_element_layer(&mut ctx, size, scale, state.zoom, drawing.as_ref());
        if let Err(why) = same_clears(&ctx.clears(), &upstream_clears(&scene["events"])) {
            failures.push(format!("{name}: {why}"));
            continue;
        }
        let list = drawing.unwrap_or_default();
        match draws::compare(&list, &scene["events"], &scene["images"]) {
            Ok(n) => {
                compared += n;
                assert_eq!(ctx.draws(), n, "{name}: the layer paints the scene's draws");
            }
            Err(why) => failures.push(format!("{name}: {why}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(compared >= 40, "only {compared} draws compared");
}

#[test]
fn nothing_to_draw_clears_the_canvas_again_under_the_zoom() {
    // renderNewElementScene.ts:87-89: the else branch's clearRect runs
    // after context.scale(zoom)
    let scene = cases("newElementScenes")
        .into_iter()
        .find(|s| s["name"] == "none")
        .unwrap();
    let clears = upstream_clears(&scene["events"]);
    assert_eq!(clears.len(), 2);
    assert!(draws::same_matrix(&clears[1].0, &Transform::scale(1.5, 1.5)));
    let mut ctx = Recording::default();
    paint_new_element_layer(&mut ctx, BackingSize { width: 400, height: 300 }, 1.0, 1.5, None);
    same_clears(&ctx.clears(), &clears).unwrap();
}

#[test]
fn invisibly_small_elements_are_upstreams() {
    // sizeHelpers.ts:61-78, over the fixture's new elements
    let small: HashSet<&str> = [
        "arrow-below-drag-threshold",
        "arrow-one-point",
        "freedraw-one-point",
        "rectangle-zero-size",
    ]
    .into_iter()
    .collect();
    for scene in cases("newElementScenes") {
        let name = scene["name"].as_str().unwrap();
        if scene["newElement"].is_null() || scene["newElement"]["type"] == "selection" {
            continue;
        }
        let el = element(&scene["newElement"]);
        assert_eq!(is_invisibly_small_element(&el), small.contains(name), "{name}");
        // what upstream drew: only the bootstrap clear when invisibly small
        let events = scene["events"].as_array().unwrap();
        assert_eq!(events.len() == 1, small.contains(name), "{name}");
    }
}

// ---------------------------------------------------------------------------
// The canvases' stylesheet rules

#[test]
fn canvas_css_is_upstreams_canvas_rules() {
    // css/styles.scss:5-6, 105-132
    for rule in [
        "--zIndex-canvas: 1;",
        "--zIndex-interactiveCanvas: 2;",
        "touch-action: none;",
        "image-rendering: pixelated;",
        "image-rendering: -moz-crisp-edges;",
        "z-index: var(--zIndex-canvas);",
        ".excalidraw canvas.interactive",
        "z-index: var(--zIndex-interactiveCanvas);",
        ".excalidraw .excalidraw__canvas-wrapper,\n.excalidraw .excalidraw__canvas.static",
        "pointer-events: none;",
        ".excalidraw .excalidraw__canvas",
        "position: absolute;",
    ] {
        assert!(CANVAS_LAYER_CSS.contains(rule), "missing {rule:?}");
    }
    // -moz-crisp-edges after pixelated, as upstream orders them
    assert!(
        CANVAS_LAYER_CSS.find("pixelated").unwrap()
            < CANVAS_LAYER_CSS.find("-moz-crisp-edges").unwrap()
    );
}
