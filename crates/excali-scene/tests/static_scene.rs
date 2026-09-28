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
use excali_scene::display::{
    Clip, Color, DisplayList, FillRule, ImageItem, PaintState, Painter, Path, PathCommand, Rgba,
    Stroke, TextRun, Transform,
};
use excali_scene::export::FrameRendering;
use excali_scene::render_element::{builtin_image, is_rtl, ElementRenderOverride};
use excali_scene::shape::Theme;
use excali_scene::static_scene::{
    grid_line_colors, render_static_scene, snap_scroll_to_device_pixels, stroke_grid, CachedImage,
    GridConfig, StaticCanvasAppState, StaticCanvasRenderConfig, StaticScene, GRID_LINE_COLOR_BOLD,
    GRID_LINE_COLOR_REGULAR,
};
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{Map, Value};

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

// ---------------------------------------------------------------------------
// Recording the port's draws

#[derive(Debug)]
enum Draw {
    Fill {
        m: Transform,
        alpha: f64,
        color: Color,
        rgba: Rgba,
        rule: FillRule,
        path: Path,
    },
    Stroke {
        m: Transform,
        alpha: f64,
        stroke: Stroke,
        rgba: Rgba,
        path: Path,
    },
    Image {
        m: Transform,
        alpha: f64,
        image: ImageItem,
    },
    Text {
        m: Transform,
        alpha: f64,
        run: TextRun,
        rgba: Rgba,
    },
    Clip {
        m: Transform,
        clip: Clip,
    },
    Unclip,
}

#[derive(Default)]
struct Recorder(Vec<Draw>);

impl Painter for Recorder {
    fn fill(&mut self, path: &Path, color: &Color, rgba: Rgba, rule: FillRule, s: &PaintState) {
        self.0.push(Draw::Fill {
            m: s.transform,
            alpha: s.alpha,
            color: color.clone(),
            rgba,
            rule,
            path: path.clone(),
        });
    }
    fn stroke(&mut self, path: &Path, stroke: &Stroke, rgba: Rgba, s: &PaintState) {
        self.0.push(Draw::Stroke {
            m: s.transform,
            alpha: s.alpha,
            stroke: stroke.clone(),
            rgba,
            path: path.clone(),
        });
    }
    fn image(&mut self, image: &ImageItem, s: &PaintState) {
        self.0.push(Draw::Image {
            m: s.transform,
            alpha: s.alpha,
            image: image.clone(),
        });
    }
    fn text(&mut self, run: &TextRun, rgba: Rgba, s: &PaintState) {
        self.0.push(Draw::Text {
            m: s.transform,
            alpha: s.alpha,
            run: run.clone(),
            rgba,
        });
    }
    fn push_clip(&mut self, clip: &Clip, transform: &Transform) {
        self.0.push(Draw::Clip {
            m: *transform,
            clip: clip.clone(),
        });
    }
    fn pop_clip(&mut self) {
        self.0.push(Draw::Unclip);
    }
}

// ---------------------------------------------------------------------------
// Comparing

fn close(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn matrix(value: &Value) -> Transform {
    let v: Vec<f64> = value.as_array().unwrap().iter().map(num).collect();
    Transform::new(v[0], v[1], v[2], v[3], v[4], v[5])
}

fn same_matrix(a: &Transform, b: &Transform) -> bool {
    close(a.a, b.a)
        && close(a.b, b.b)
        && close(a.c, b.c)
        && close(a.d, b.d)
        && close(a.e, b.e)
        && close(a.f, b.f)
}

/// The recorded calls since `beginPath()` as a [`Path`]: `rect` and
/// `roundRect` add what those canvas methods add, a Path2D's SVG data is
/// read as the Path2D constructor reads it.
fn path(value: &Value) -> Path {
    let mut p = Path::new();
    for command in value.as_array().unwrap() {
        let c = command.as_array().unwrap();
        let a = |i: usize| num(&c[i]);
        match c[0].as_str().unwrap() {
            "moveTo" => {
                p.move_to(a(1), a(2));
            }
            "lineTo" => {
                p.line_to(a(1), a(2));
            }
            "bezierCurveTo" => {
                p.cubic_to(a(1), a(2), a(3), a(4), a(5), a(6));
            }
            "quadraticCurveTo" => {
                p.quad_to(a(1), a(2), a(3), a(4));
            }
            "arc" => {
                p.arc(a(1), a(2), a(3), a(4), a(5), c[6].as_bool().unwrap());
            }
            "closePath" => {
                p.close();
            }
            "rect" => p
                .commands
                .extend(Path::rect(a(1), a(2), a(3), a(4)).commands),
            "roundRect" => p
                .commands
                .extend(Path::round_rect(a(1), a(2), a(3), a(4), a(5)).commands),
            "svg" => p
                .commands
                .extend(Path::from_svg_path_data(c[1].as_str().unwrap()).commands),
            other => panic!("unknown path call {other}"),
        }
    }
    p
}

fn command_values(c: &PathCommand) -> (u8, Vec<f64>) {
    match *c {
        PathCommand::MoveTo(x, y) => (0, vec![x, y]),
        PathCommand::LineTo(x, y) => (1, vec![x, y]),
        PathCommand::QuadTo(a, b, c, d) => (2, vec![a, b, c, d]),
        PathCommand::CubicTo(a, b, c, d, e, f) => (3, vec![a, b, c, d, e, f]),
        PathCommand::Arc {
            cx,
            cy,
            radius,
            start,
            end,
            anticlockwise,
        } => (
            4,
            vec![
                cx,
                cy,
                radius,
                start,
                end,
                f64::from(u8::from(anticlockwise)),
            ],
        ),
        PathCommand::Close => (5, vec![]),
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.commands.len() == b.commands.len()
        && a.commands.iter().zip(&b.commands).all(|(x, y)| {
            let (kx, vx) = command_values(x);
            let (ky, vy) = command_values(y);
            kx == ky && vx.iter().zip(&vy).all(|(p, q)| close(*p, *q))
        })
}

fn rule(value: &Value) -> FillRule {
    match value.as_str().unwrap() {
        "evenodd" => FillRule::EvenOdd,
        _ => FillRule::NonZero,
    }
}

/// The style a list of assignments leaves: the last one the canvas can
/// parse, or none (the context's black).
fn style(value: &Value) -> Option<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .map(|v| v.as_str().unwrap())
        .find(|s| Color::new(*s).rgba().is_some())
        .map(str::to_owned)
}

fn check_color(expected: &Value, color: &Color, rgba: Rgba) -> Result<(), String> {
    match style(expected) {
        Some(s) if s == color.as_str() => Ok(()),
        Some(s) => Err(format!("colour {:?}, expected {s:?}", color.as_str())),
        None if rgba == Rgba::BLACK => Ok(()),
        None => Err(format!(
            "colour {:?} ({rgba:?}), expected the context's black",
            color.as_str()
        )),
    }
}

fn check_alpha(expected: &Value, alpha: f64) -> Result<(), String> {
    if close(num(expected), alpha) {
        Ok(())
    } else {
        Err(format!("alpha {alpha}, expected {expected}"))
    }
}

fn check_matrix(expected: &Value, m: &Transform) -> Result<(), String> {
    if same_matrix(&matrix(expected), m) {
        Ok(())
    } else {
        Err(format!("matrix {m:?}, expected {expected}"))
    }
}

fn check_path(expected: &Value, actual: &Path) -> Result<(), String> {
    let e = path(expected);
    if same_path(&e, actual) {
        Ok(())
    } else {
        Err(format!(
            "path {:?}\nexpected {:?}",
            actual.commands, e.commands
        ))
    }
}

fn check(e: &Value, draw: &Draw, images: &Value) -> Result<(), String> {
    let op = e["op"].as_str().unwrap();
    match (op, draw) {
        (
            "fill",
            Draw::Fill {
                m,
                alpha,
                color,
                rgba,
                rule: r,
                path: p,
            },
        ) => {
            check_matrix(&e["m"], m)?;
            check_alpha(&e["alpha"], *alpha)?;
            check_color(&e["fillStyle"], color, *rgba)?;
            if rule(&e["rule"]) != *r {
                return Err(format!("rule {r:?}, expected {}", e["rule"]));
            }
            check_path(&e["path"], p)
        }
        (
            "stroke",
            Draw::Stroke {
                m,
                alpha,
                stroke,
                rgba,
                path: p,
            },
        ) => {
            check_matrix(&e["m"], m)?;
            check_alpha(&e["alpha"], *alpha)?;
            check_color(&e["strokeStyle"], &stroke.color, *rgba)?;
            if !close(num(&e["lineWidth"]), stroke.effective_width()) {
                return Err(format!(
                    "lineWidth {}, expected {}",
                    stroke.width, e["lineWidth"]
                ));
            }
            if e["lineCap"] != stroke.cap.as_css() || e["lineJoin"] != stroke.join.as_css() {
                return Err(format!(
                    "cap/join {:?} {:?}, expected {} {}",
                    stroke.cap, stroke.join, e["lineCap"], e["lineJoin"]
                ));
            }
            if !close(num(&e["miterLimit"]), stroke.effective_miter_limit()) {
                return Err(format!("miterLimit {}", stroke.miter_limit));
            }
            let dash: Vec<f64> = e["dash"].as_array().unwrap().iter().map(num).collect();
            let solid = dash.iter().sum::<f64>() <= 0.0;
            match (&stroke.dash, solid) {
                (None, true) => {}
                (Some(d), false) => {
                    let same = d.segments().len() == dash.len()
                        && d.segments().iter().zip(&dash).all(|(a, b)| close(*a, *b))
                        && close(d.offset(), num(&e["dashOffset"]));
                    if !same {
                        return Err(format!(
                            "dash {d:?}, expected {dash:?} at {}",
                            e["dashOffset"]
                        ));
                    }
                }
                (d, _) => return Err(format!("dash {d:?}, expected {dash:?}")),
            }
            check_path(&e["path"], p)
        }
        ("clip", Draw::Clip { m, clip }) => {
            check_matrix(&e["m"], m)?;
            if rule(&e["rule"]) != clip.rule {
                return Err(format!("clip rule {:?}, expected {}", clip.rule, e["rule"]));
            }
            check_path(&e["path"], &clip.path)
        }
        ("unclip", Draw::Unclip) => Ok(()),
        (
            "text",
            Draw::Text {
                m,
                alpha,
                run,
                rgba,
            },
        ) => {
            check_matrix(&e["m"], m)?;
            check_alpha(&e["alpha"], *alpha)?;
            check_color(&e["fillStyle"], &run.color, *rgba)?;
            let checks = [
                (e["text"] == run.text.as_str(), "text"),
                (
                    close(num(&e["x"]), run.x) && close(num(&e["y"]), run.y),
                    "position",
                ),
                (e["font"] == run.font.css().as_str(), "font"),
                (e["textAlign"] == run.align.as_css(), "textAlign"),
                (e["direction"] == run.direction.as_css(), "direction"),
            ];
            match checks.iter().find(|(ok, _)| !ok) {
                Some((_, what)) => Err(format!("text {what}: {run:?}")),
                None => Ok(()),
            }
        }
        ("image", Draw::Image { m, alpha, image }) => {
            check_matrix(&e["m"], m)?;
            check_alpha(&e["alpha"], *alpha)?;
            let name = &e["image"];
            let natural = if let Some(file) = name["file"].as_str() {
                if image.id != file {
                    return Err(format!("image {}, expected file {file}", image.id));
                }
                let n = &images[file];
                Some((num(&n["naturalWidth"]), num(&n["naturalHeight"])))
            } else {
                let builtin = name["builtin"].as_str().unwrap();
                let Some(expected) = builtin_image(builtin) else {
                    return Err(format!("no built-in image {builtin}"));
                };
                if image.id != expected.id {
                    return Err(format!("image {}, expected {}", image.id, expected.id));
                }
                if name["src"] != expected.data_url.as_str() {
                    return Err(format!(
                        "built-in {builtin}'s data URL differs from upstream's"
                    ));
                }
                None
            };
            let args: Vec<f64> = e["args"].as_array().unwrap().iter().map(num).collect();
            let (source, dest) = match args.len() {
                4 => (None, &args[..]),
                8 => (Some(&args[..4]), &args[4..]),
                n => return Err(format!("drawImage with {n} arguments")),
            };
            let d = image.dest;
            if ![d.x, d.y, d.width, d.height]
                .iter()
                .zip(dest)
                .all(|(a, b)| close(*a, *b))
            {
                return Err(format!("dest {d:?}, expected {dest:?}"));
            }
            let source_ok = match (source, image.source) {
                (None, None) => true,
                (Some(s), Some(r)) => [r.x, r.y, r.width, r.height]
                    .iter()
                    .zip(s)
                    .all(|(a, b)| close(*a, *b)),
                // the whole bitmap at its natural size
                (Some(s), None) => natural.is_some_and(|(w, h)| s == [0.0, 0.0, w, h]),
                (None, Some(_)) => false,
            };
            if !source_ok {
                return Err(format!("source {:?}, expected {source:?}", image.source));
            }
            let filter = match image.filter {
                None => "none",
                Some(f) => f.css(),
            };
            if e["filter"] != filter || e["smoothing"] != image.smoothing {
                return Err(format!(
                    "filter/smoothing {:?} {}",
                    image.filter, image.smoothing
                ));
            }
            Ok(())
        }
        _ => Err(format!("{draw:?}, expected {op}")),
    }
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
        let mut recorder = Recorder::default();
        list.replay(&mut recorder);
        let expected: Vec<&Value> = scene["events"].as_array().unwrap().iter().collect();
        // clearRect clears a canvas the display list starts without: it
        // may only come before the first draw
        let first_draw = expected.iter().position(|e| e["op"] != "clear");
        let expected: Vec<&Value> = expected
            .into_iter()
            .enumerate()
            .filter(|(i, e)| {
                if e["op"] == "clear" {
                    assert!(
                        first_draw.is_none_or(|f| *i < f),
                        "{name}: clearRect after a draw"
                    );
                    false
                } else {
                    true
                }
            })
            .map(|(_, e)| e)
            .collect();
        for (i, (e, draw)) in expected.iter().zip(&recorder.0).enumerate() {
            if let Err(why) = check(e, draw, &scene["images"]) {
                failures.push(format!("{name} #{i}: {why}"));
                break;
            }
            compared += 1;
        }
        if expected.len() != recorder.0.len() {
            failures.push(format!(
                "{name}: {} draws, upstream made {}",
                recorder.0.len(),
                expected.len()
            ));
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
    ] {
        assert!(names.iter().any(|n| n == name), "no scene {name}");
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
    assert!(matches!(with[0], Draw::Fill { .. }));
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
