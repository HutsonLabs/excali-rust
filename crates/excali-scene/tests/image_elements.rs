//! Image elements (ex-404) against upstream's own `renderElement`.
//!
//! `tests/fixtures/image-elements.json` is written by
//! `tools/goldens/image-elements.mjs`: upstream's `renderElement`
//! (`packages/element/src/renderElement.ts:963-1009`) exporting image
//! elements (`isExporting`: translate to the centre, rotate, `scale`,
//! translate back, `:1111-1190`; the image case of `drawElementOnCanvas`,
//! `:517-624`; `drawImagePlaceholder`, `:361-385`) on a context that records
//! its calls, for loaded, pending, missing and uninitialized images, crops,
//! flips, rotations, rounded clips, both themes, SVG and PNG files.
//!
//! The port's side is `excali_scene::render_element::render_element`, the
//! function `render_static_scene` draws every element with, exporting.
//!
//! Each case's calls are played on a model of the canvas state (matrix,
//! `globalAlpha`, `fillStyle`, `filter`, clip stack) into the draws they
//! make, and the port's display list is replayed into the same form. The
//! draws must agree one for one: the placeholder box and colour, the
//! placeholder as the built-in image holding upstream's placeholder SVG
//! (the file records its source), each `drawImage`'s image, source and
//! destination rectangles and filter, the rounded clip, and every matrix
//! and alpha.
//!
//! The raster fixture `image-elements.json` (excali-raster) is the port's
//! display list for all the cases with upstream's recorded calls beside it:
//! Chrome replays upstream's calls, with its own image decoders and its own
//! rendering of the placeholder SVGs, and the tiny-skia backend renders the
//! port's list (`crates/excali-raster/tests/fixtures.rs`).
//! `EXCALI_WRITE_RASTER_FIXTURE=1` rewrites it (then regenerate the Chrome
//! references with `scripts/fixtures/raster-references.sh`).

use std::collections::HashMap;
use std::path::PathBuf;

use excali_core::element::{Element, ElementKind};
use excali_scene::bounds::ElementsMap;
use excali_scene::display::{
    Clip, Color, DisplayItem, DisplayList, FillRule, ImageFilter, ImageItem, PaintState, Painter,
    Path, Rect, Rgba, Stroke, TextRun, Transform,
};
use excali_scene::render_element::{
    builtin_image, builtin_image_by_id, image_placeholder_size, render_element, BuiltinImage,
    BUILTIN_IMAGE_NAMES, ELEMENT_LINK_ID, EXTERNAL_LINK_ID, IMAGE_ERROR_PLACEHOLDER_ID,
    IMAGE_PLACEHOLDER_FILL_DARK, IMAGE_PLACEHOLDER_FILL_LIGHT, IMAGE_PLACEHOLDER_ID,
};
use excali_scene::shape::Theme;
use excali_scene::static_scene::{
    render_static_scene, CachedImage, StaticCanvasAppState, StaticCanvasRenderConfig, StaticScene,
};
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{json, Value};

#[path = "support/vocabulary.rs"]
mod vocabulary;
use vocabulary::{close, item_json};

fn golden() -> Value {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/image-elements.json");
    serde_json::from_str(&std::fs::read_to_string(&file).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn theme(case: &Value) -> Theme {
    match case["theme"].as_str().unwrap() {
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        other => panic!("unknown theme {other}"),
    }
}

fn element(case: &Value) -> Element {
    serde_json::from_value(case["element"].clone()).expect("an image element")
}

/// The image cache the case's export had, as the port holds it: the file's
/// MIME type once its image has loaded. A load still running (or failed)
/// leaves upstream's promise in the cache, which draws the placeholder as
/// no entry does, so the port has no entry for it.
fn cache(g: &Value, case: &Value) -> HashMap<String, CachedImage> {
    let mut cache = HashMap::new();
    let Some(file_id) = case["element"]["fileId"].as_str() else {
        return cache;
    };
    let mime_type = g["files"][file_id]["mimeType"].as_str().unwrap().to_owned();
    match case["cache"].as_str().unwrap() {
        "loaded" => {
            cache.insert(file_id.to_owned(), CachedImage { mime_type });
        }
        "pending" | "none" => {}
        other => panic!("unknown cache state {other}"),
    }
    cache
}

/// `renderElement` exporting `el` with the image cache `images`, in
/// `theme`, at `scroll`: what `render_static_scene` draws for it.
fn render_with(
    el: &Element,
    images: HashMap<String, CachedImage>,
    theme: Theme,
    scroll: (f64, f64),
) -> DisplayItem {
    let config = StaticCanvasRenderConfig {
        image_cache: images,
        is_exporting: true,
        theme,
        ..StaticCanvasRenderConfig::default()
    };
    let app_state = StaticCanvasAppState {
        scroll_x: scroll.0,
        scroll_y: scroll.1,
        theme,
        ..StaticCanvasAppState::default()
    };
    let map = ElementsMap::new([el]);
    render_element(el, &map, &map, &config, &app_state, None).expect("image elements draw")
}

fn render(g: &Value, case: &Value) -> DisplayItem {
    let scroll = (num(&case["scroll"][0]), num(&case["scroll"][1]));
    render_with(&element(case), cache(g, case), theme(case), scroll)
}

// -- draws ------------------------------------------------------------------

/// One draw with the state it is made in.
#[derive(Clone, Debug)]
enum Draw {
    FillRect {
        rect: Rect,
        color: String,
    },
    Fill {
        path: Path,
        color: String,
        rule: FillRule,
    },
    Image {
        id: String,
        source: Rect,
        dest: Rect,
        filter: Option<String>,
    },
}

#[derive(Clone, Debug)]
struct Drawn {
    draw: Draw,
    matrix: Transform,
    alpha: f64,
    clips: Vec<(Path, Transform)>,
}

/// The golden's placeholder kind as the port's built-in image.
fn placeholder_image(kind: &str) -> BuiltinImage {
    match kind {
        "image" => builtin_image("image-placeholder").unwrap(),
        "error" => builtin_image("image-error-placeholder").unwrap(),
        other => panic!("unknown placeholder {other}"),
    }
}

fn svg_view_box(svg: &str) -> f64 {
    let i = svg.find("viewBox=\"").unwrap() + 9;
    let v: Vec<f64> = svg[i..i + svg[i..].find('"').unwrap()]
        .split(' ')
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!((v[0], v[1]), (0.0, 0.0));
    assert_eq!(v[2], v[3], "a square placeholder");
    v[2]
}

#[derive(Clone)]
struct CanvasState {
    matrix: Transform,
    alpha: f64,
    fill_style: String,
    filter: Option<String>,
    clips: Vec<(Path, Transform)>,
}

/// Upstream's recorded calls played on a model of the canvas state.
fn upstream_draws(g: &Value, case: &Value) -> Vec<Drawn> {
    let mut stack = Vec::new();
    let mut s = CanvasState {
        matrix: Transform::IDENTITY,
        alpha: 1.0,
        fill_style: "#000000".to_owned(),
        filter: None,
        clips: Vec::new(),
    };
    let mut path: Option<(Path, Transform)> = None;
    let mut out = Vec::new();
    for call in case["calls"].as_array().unwrap() {
        let call = call.as_array().unwrap();
        let n = |i: usize| num(&call[i]);
        match call[0].as_str().unwrap() {
            "save" => stack.push(s.clone()),
            "restore" => s = stack.pop().expect("balanced save/restore"),
            "set" => match call[1].as_str().unwrap() {
                "globalAlpha" => s.alpha = n(2),
                "fillStyle" => s.fill_style = call[2].as_str().unwrap().to_owned(),
                "filter" => s.filter = Some(call[2].as_str().unwrap().to_owned()),
                other => panic!("unexpected property {other}"),
            },
            "translate" => s.matrix = s.matrix.concat(&Transform::translate(n(1), n(2))),
            "rotate" => s.matrix = s.matrix.concat(&Transform::rotate(n(1))),
            "scale" => s.matrix = s.matrix.concat(&Transform::scale(n(1), n(2))),
            "beginPath" => path = None,
            "roundRect" => {
                path = Some((Path::round_rect(n(1), n(2), n(3), n(4), n(5)), s.matrix));
            }
            "clip" => s.clips.push(path.clone().expect("a path to clip to")),
            "fillRect" => out.push(Drawn {
                draw: Draw::FillRect {
                    rect: Rect::new(n(1), n(2), n(3), n(4)),
                    color: s.fill_style.clone(),
                },
                matrix: s.matrix,
                alpha: s.alpha,
                clips: s.clips.clone(),
            }),
            "drawImage" => {
                let image = &call[1];
                let args: Vec<f64> = call[2..].iter().map(num).collect();
                if let Some(file) = image["file"].as_str() {
                    assert_eq!(
                        args.len(),
                        8,
                        "drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh)"
                    );
                    out.push(Drawn {
                        draw: Draw::Image {
                            id: file.to_owned(),
                            source: Rect::new(args[0], args[1], args[2], args[3]),
                            dest: Rect::new(args[4], args[5], args[6], args[7]),
                            filter: s.filter.clone(),
                        },
                        matrix: s.matrix,
                        alpha: s.alpha,
                        clips: s.clips.clone(),
                    });
                } else {
                    // drawImage(placeholder, dx, dy, dw, dh): the whole SVG,
                    // its viewBox the natural size, onto the rectangle.
                    assert_eq!(args.len(), 4, "drawImage(img, dx, dy, dw, dh)");
                    let kind = image["placeholder"].as_str().unwrap();
                    let builtin = placeholder_image(kind);
                    let vb = svg_view_box(g["placeholders"][kind].as_str().unwrap());
                    out.push(Drawn {
                        draw: Draw::Image {
                            id: builtin.id.to_owned(),
                            source: Rect::new(0.0, 0.0, vb, vb),
                            dest: Rect::new(args[0], args[1], args[2], args[3]),
                            filter: s.filter.clone(),
                        },
                        matrix: s.matrix,
                        alpha: s.alpha,
                        clips: s.clips.clone(),
                    });
                }
            }
            other => panic!("unexpected call {other}"),
        }
    }
    assert!(stack.is_empty(), "{}: unbalanced save/restore", case["id"]);
    out
}

/// The port's display list replayed into draws.
#[derive(Default)]
struct Recorder {
    clips: Vec<(Path, Transform)>,
    draws: Vec<Drawn>,
    naturals: HashMap<String, (f64, f64)>,
}

impl Painter for Recorder {
    fn fill(&mut self, path: &Path, color: &Color, _: Rgba, rule: FillRule, state: &PaintState) {
        self.draws.push(Drawn {
            draw: Draw::Fill {
                path: path.clone(),
                color: color.as_str().to_owned(),
                rule,
            },
            matrix: state.transform,
            alpha: state.alpha,
            clips: self.clips.clone(),
        });
    }
    fn fill_rect(&mut self, rect: &Rect, color: &Color, _: Rgba, state: &PaintState) {
        self.draws.push(Drawn {
            draw: Draw::FillRect {
                rect: *rect,
                color: color.as_str().to_owned(),
            },
            matrix: state.transform,
            alpha: state.alpha,
            clips: self.clips.clone(),
        });
    }
    fn stroke(&mut self, _: &Path, _: &Stroke, _: Rgba, _: &PaintState) {
        panic!("image elements have no strokes");
    }
    fn image(&mut self, image: &ImageItem, state: &PaintState) {
        let (w, h) = self.naturals[&image.id];
        assert!(image.smoothing, "the export draws with smoothing on");
        self.draws.push(Drawn {
            draw: Draw::Image {
                id: image.id.clone(),
                source: image.source.unwrap_or(Rect::new(0.0, 0.0, w, h)),
                dest: image.dest,
                filter: image.filter.map(|f| f.css().to_owned()),
            },
            matrix: state.transform,
            alpha: state.alpha,
            clips: self.clips.clone(),
        });
    }
    fn text(&mut self, _: &TextRun, _: Rgba, _: &PaintState) {
        panic!("image elements have no text");
    }
    fn push_clip(&mut self, clip: &Clip, transform: &Transform) {
        assert_eq!(clip.rule, FillRule::NonZero);
        self.clips.push((clip.path.clone(), *transform));
    }
    fn pop_clip(&mut self) {
        self.clips.pop();
    }
}

/// The natural size of every image the cases draw: the files', and the
/// built-in placeholders' viewBox.
fn naturals(g: &Value) -> HashMap<String, (f64, f64)> {
    let files = g["files"].as_object().unwrap().iter().map(|(id, f)| {
        (
            id.clone(),
            (num(&f["naturalWidth"]), num(&f["naturalHeight"])),
        )
    });
    let builtins = ["image", "error"].into_iter().map(|kind| {
        let side = svg_view_box(placeholder_image(kind).svg);
        (placeholder_image(kind).id.to_owned(), (side, side))
    });
    files.chain(builtins).collect()
}

fn port_draws(g: &Value, case: &Value) -> Vec<Drawn> {
    let list = DisplayList::from_iter([render(g, case)]);
    let mut recorder = Recorder {
        naturals: naturals(g),
        ..Recorder::default()
    };
    list.replay(&mut recorder);
    recorder.draws
}

fn near(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

fn same_matrix(a: &Transform, b: &Transform) -> bool {
    [
        (a.a, b.a),
        (a.b, b.b),
        (a.c, b.c),
        (a.d, b.d),
        (a.e, b.e),
        (a.f, b.f),
    ]
    .iter()
    .all(|&(x, y)| near(x, y))
}

fn same_rect(a: &Rect, b: &Rect) -> bool {
    near(a.x, b.x) && near(a.y, b.y) && near(a.width, b.width) && near(a.height, b.height)
}

fn compare(id: &str, expected: &[Drawn], actual: &[Drawn]) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err(format!(
            "{id}: {} draws upstream, {} in the port\nupstream: {expected:#?}\nport: {actual:#?}",
            expected.len(),
            actual.len()
        ));
    }
    for (i, (e, a)) in expected.iter().zip(actual).enumerate() {
        let at = format!("{id} draw {i}");
        let same_draw = match (&e.draw, &a.draw) {
            (
                Draw::FillRect { rect, color },
                Draw::FillRect {
                    rect: r2,
                    color: c2,
                },
            ) => same_rect(rect, r2) && color == c2,
            (
                Draw::Fill { path, color, rule },
                Draw::Fill {
                    path: p2,
                    color: c2,
                    rule: r2,
                },
            ) => path == p2 && color == c2 && rule == r2,
            (
                Draw::Image {
                    id,
                    source,
                    dest,
                    filter,
                },
                Draw::Image {
                    id: i2,
                    source: s2,
                    dest: d2,
                    filter: f2,
                },
            ) => id == i2 && same_rect(source, s2) && same_rect(dest, d2) && filter == f2,
            _ => false,
        };
        if !same_draw {
            return Err(format!("{at}: {:?} != {:?}", e.draw, a.draw));
        }
        if !same_matrix(&e.matrix, &a.matrix) {
            return Err(format!("{at}: matrix {:?} != {:?}", e.matrix, a.matrix));
        }
        if !near(e.alpha, a.alpha) {
            return Err(format!("{at}: alpha {} != {}", e.alpha, a.alpha));
        }
        if e.clips.len() != a.clips.len()
            || e.clips
                .iter()
                .zip(&a.clips)
                .any(|((p, t), (p2, t2))| p != p2 || !same_matrix(t, t2))
        {
            return Err(format!("{at}: clips {:?} != {:?}", e.clips, a.clips));
        }
    }
    Ok(())
}

#[test]
fn every_case_draws_what_upstream_draws() {
    let g = golden();
    let cases = g["cases"].as_array().unwrap();
    assert!(cases.len() >= 20, "the golden covers the image cases");
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let id = case["id"].as_str().unwrap();
            compare(id, &upstream_draws(&g, case), &port_draws(&g, case)).err()
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn the_placeholders_are_upstreams_svgs() {
    let g = golden();
    for (kind, id) in [
        ("image", IMAGE_PLACEHOLDER_ID),
        ("error", IMAGE_ERROR_PLACEHOLDER_ID),
    ] {
        let svg = g["placeholders"][kind].as_str().unwrap();
        let image = placeholder_image(kind);
        assert_eq!(image.svg, svg);
        assert_eq!(image.id, id);
        assert_eq!(builtin_image_by_id(id), Some(image.clone()));
        // Upstream's src: data:image/svg+xml, then encodeURIComponent(svg).
        let body = image.data_url.strip_prefix("data:image/svg+xml,").unwrap();
        assert!(!body.contains(['<', '>', '"', ' ', '#']));
        assert_eq!(percent_decode(body), svg);
    }
    // one id scheme for every built-in image, and no file id is one
    for name in BUILTIN_IMAGE_NAMES {
        let image = builtin_image(name).unwrap();
        assert_eq!(image.id, format!("excalidraw:{name}"));
        assert_eq!(builtin_image_by_id(image.id), Some(image));
    }
    assert_eq!(builtin_image_by_id("png"), None);
    assert_eq!(builtin_image_by_id("builtin:image-placeholder"), None);
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

/// `drawImagePlaceholder` (`renderElement.ts:361-385`): the icon is
/// `min(min(w, h) * 0.4, 100)`, never above `min(w, h)`.
#[test]
fn the_icon_size_rule() {
    assert_eq!(image_placeholder_size(120.0, 80.0), 32.0);
    assert_eq!(image_placeholder_size(300.0, 260.0), 100.0);
    assert_eq!(image_placeholder_size(1000.0, 1000.0), 100.0);
    assert_eq!(image_placeholder_size(249.0, 400.0), 249.0 * 0.4);
    assert_eq!(image_placeholder_size(12.0, 30.0), 12.0 * 0.4);
    assert_eq!(image_placeholder_size(0.0, 30.0), 0.0);
    // Math.min: NaN wins, and a negative side stays negative.
    assert!(image_placeholder_size(f64::NAN, 30.0).is_nan());
    assert_eq!(image_placeholder_size(-10.0, 30.0), -10.0);
    assert_eq!(IMAGE_PLACEHOLDER_FILL_LIGHT, "#E7E7E7");
    assert_eq!(IMAGE_PLACEHOLDER_FILL_DARK, "#2E2E2E");
}

/// The innermost items of the export's placement groups: the element's own
/// drawing.
fn innermost(item: &DisplayItem) -> &[DisplayItem] {
    match item {
        DisplayItem::Group(g) => match g.items.as_slice() {
            [only @ DisplayItem::Group(_)] => innermost(only),
            items => items,
        },
        _ => panic!("the placement is groups"),
    }
}

/// The element's own drawing sits inside the placement groups.
#[test]
fn the_placement_holds_the_element_drawing() {
    let g = golden();
    let case = g["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "png-rotated-flipped")
        .unwrap();
    assert_eq!(
        innermost(&render(&g, case)),
        [DisplayItem::Image(ImageItem::new(
            "png",
            Rect::new(0.0, 0.0, 80.0, 50.0)
        ))]
    );
}

/// `drawImagePlaceholder`'s box is upstream's `fillRect`, a
/// [`DisplayItem::FillRect`] (the canvas anti-aliases it as a rectangle,
/// not as a filled path), then the built-in placeholder.
#[test]
fn the_placeholder_box_is_a_fill_rect() {
    let g = golden();
    let case = g["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "png")
        .unwrap();
    let el = element(case);
    for (theme, fill) in [
        (Theme::Light, IMAGE_PLACEHOLDER_FILL_LIGHT),
        (Theme::Dark, IMAGE_PLACEHOLDER_FILL_DARK),
    ] {
        let item = render_with(&el, HashMap::new(), theme, (0.0, 0.0));
        match innermost(&item) {
            [DisplayItem::FillRect { rect, color }, DisplayItem::Image(icon)] => {
                assert_eq!(*rect, Rect::new(0.0, 0.0, el.base.width, el.base.height));
                assert_eq!(color.as_str(), fill);
                assert_eq!(icon.id, IMAGE_PLACEHOLDER_ID);
            }
            other => panic!("a fillRect and the placeholder: {other:?}"),
        }
    }
}

/// A dark SVG gets the filter, a dark PNG does not, a light SVG does not.
#[test]
fn only_dark_svg_images_are_filtered() {
    let g = golden();
    let png = g["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "png")
        .unwrap();
    let mut el: Element = element(png);
    let file = match &el.kind {
        ElementKind::Image(f) => f.file_id.clone().unwrap().0,
        _ => panic!("an image"),
    };
    el.base.roundness = None;
    let filter = |mime: &str, theme| {
        let cache = HashMap::from([(
            file.clone(),
            CachedImage {
                mime_type: mime.to_owned(),
            },
        )]);
        match innermost(&render_with(&el, cache, theme, (0.0, 0.0))) {
            [DisplayItem::Image(i)] => i.filter,
            other => panic!("one image: {other:?}"),
        }
    };
    assert_eq!(
        filter("image/svg+xml", Theme::Dark),
        Some(ImageFilter::DarkTheme)
    );
    assert_eq!(filter("image/png", Theme::Dark), None);
    assert_eq!(filter("image/svg+xml", Theme::Light), None);
}

// -- raster fixtures ----------------------------------------------------------

const FIXTURE_WIDTH: u32 = 620;
const FIXTURE_HEIGHT: u32 = 650;

/// The golden's cases as two raster fixtures: the placeholders and bitmap
/// files, and the SVG files (whose curves reach the backend as usvg's
/// cubics where Chrome draws conics, so they carry their own tolerance).
struct Split {
    name: &'static str,
    description: &'static str,
    svg_files: bool,
}

const SPLITS: [Split; 2] = [
    Split {
        name: "image-elements",
        description: "Excalidraw image elements as the port draws them (excali-scene tests/image_elements.rs, from upstream's renderElement in tests/fixtures/image-elements.json): placeholders in both themes with the image and error icons (built-in SVG images) and the icon size rule, a missing file, an uninitialized element, a PNG at natural size, cropped, flipped by scale after rotation, rounded clips of every roundness, opacity and scroll. Chrome replays upstream's recorded canvas calls (canvasCalls) with its own PNG decoder and upstream's placeholder SVGs; the port renders items over the same files.",
        svg_files: false,
    },
    Split {
        name: "image-elements-svg",
        description: "Excalidraw image elements of an SVG file as the port draws them (excali-scene tests/image_elements.rs, from upstream's renderElement in tests/fixtures/image-elements.json): scaled, in the dark theme with DARK_THEME_FILTER, and cropped inside a rounded clip. Chrome replays upstream's recorded canvas calls (canvasCalls) and renders the SVG itself; the port renders items, the SVG through usvg as vector fills and strokes.",
        svg_files: true,
    },
];

impl Split {
    fn file(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../excali-raster/tests/fixtures/display-lists/{}.json",
            self.name
        ))
    }

    fn cases<'a>(&self, g: &'a Value) -> Vec<&'a Value> {
        g["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| {
                let svg = c["element"]["fileId"].as_str().is_some_and(|f| {
                    g["files"][f]["mimeType"] == "image/svg+xml" && c["cache"] == "loaded"
                });
                svg == self.svg_files
            })
            .collect()
    }

    fn items(&self, g: &Value) -> Vec<Value> {
        let (bg, _) = background();
        let mut items = vec![bg];
        for case in self.cases(g) {
            items.push(item_json(&render(g, case)));
        }
        items
    }

    fn calls(&self, g: &Value) -> Vec<Value> {
        let (_, mut calls) = background();
        for case in self.cases(g) {
            calls.extend(case["calls"].as_array().unwrap().iter().cloned());
        }
        calls
    }

    /// The files the cases draw.
    fn images(&self, g: &Value) -> Value {
        let used: Vec<&str> = self
            .cases(g)
            .iter()
            .filter_map(|c| c["element"]["fileId"].as_str())
            .collect();
        Value::Object(
            g["files"]
                .as_object()
                .unwrap()
                .iter()
                .filter(|(id, _)| used.contains(&id.as_str()))
                .map(|(id, f)| (id.clone(), json!({"dataUrl": f["dataURL"]})))
                .collect(),
        )
    }

    /// The file's layout: the header keys one per line, then one call and
    /// one item per line.
    fn write(&self, g: &Value) {
        let existing: Option<Value> = std::fs::read_to_string(self.file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        let tolerance = existing
            .as_ref()
            .and_then(|v| v.get("tolerance").cloned())
            .unwrap_or(json!({"channel": 0, "pixels": 0}));
        let note = existing
            .as_ref()
            .and_then(|v| v.get("toleranceNote").cloned())
            .unwrap_or(json!("to be measured"));
        let header = [
            ("description", json!(self.description)),
            ("width", json!(FIXTURE_WIDTH)),
            ("height", json!(FIXTURE_HEIGHT)),
            ("tolerance", tolerance),
            ("toleranceNote", note),
            ("images", self.images(g)),
            ("placeholders", g["placeholders"].clone()),
        ];
        let mut out = String::from("{\n");
        for (k, v) in header {
            out.push_str(&format!(" {}: {},\n", json!(k), v));
        }
        let lines = |values: Vec<Value>| {
            values
                .iter()
                .map(|v| format!("  {v}"))
                .collect::<Vec<_>>()
                .join(",\n")
        };
        out.push_str(" \"canvasCalls\": [\n");
        out.push_str(&lines(self.calls(g)));
        out.push_str("\n ],\n \"items\": [\n");
        out.push_str(&lines(self.items(g)));
        out.push_str("\n ]\n}\n");
        std::fs::write(self.file(), out).unwrap();
    }
}

/// A white background, as both the port's items and the canvas calls.
fn background() -> (Value, Vec<Value>) {
    let (w, h) = (FIXTURE_WIDTH, FIXTURE_HEIGHT);
    (
        json!({"type": "fillRect", "color": "#ffffff", "rect": [0, 0, w, h]}),
        vec![
            json!(["save"]),
            json!(["set", "fillStyle", "#ffffff"]),
            json!(["fillRect", 0, 0, w, h]),
            json!(["restore"]),
        ],
    )
}

#[test]
fn the_raster_fixtures_are_the_ports_output_beside_upstreams_calls() {
    let g = golden();
    let total: usize = SPLITS.iter().map(|s| s.cases(&g).len()).sum();
    assert_eq!(
        total,
        g["cases"].as_array().unwrap().len(),
        "every case in one fixture"
    );
    for split in &SPLITS {
        assert!(!split.cases(&g).is_empty(), "{}", split.name);
        if std::env::var_os("EXCALI_WRITE_RASTER_FIXTURE").is_some() {
            split.write(&g);
        }
        let text = std::fs::read_to_string(split.file()).unwrap_or_else(|_| {
            panic!(
                "{}.json exists (EXCALI_WRITE_RASTER_FIXTURE=1 writes it)",
                split.name
            )
        });
        let file: Value = serde_json::from_str(&text).unwrap();
        let stale = format!("{}.json is stale: EXCALI_WRITE_RASTER_FIXTURE=1 cargo test -p excali-scene --test image_elements, then scripts/fixtures/raster-references.sh", split.name);
        if let Err(at) = close(&file["items"], &Value::Array(split.items(&g)), "items") {
            panic!("{stale} ({at})");
        }
        assert_eq!(
            file["canvasCalls"],
            Value::Array(split.calls(&g)),
            "{stale}"
        );
        assert_eq!(file["images"], split.images(&g), "{stale}");
        assert_eq!(file["placeholders"], g["placeholders"], "{stale}");
        assert_eq!(file["description"], split.description, "{stale}");
        assert_eq!(
            (file["width"].as_u64(), file["height"].as_u64()),
            (
                Some(u64::from(FIXTURE_WIDTH)),
                Some(u64::from(FIXTURE_HEIGHT))
            )
        );
    }
}

// -- the static scene's images through the raster backend ---------------------

const STATIC_SCENE_WIDTH: u32 = 300;
const STATIC_SCENE_HEIGHT: u32 = 140;
const STATIC_SCENE_FIXTURE: &str = "static-scene-images";
const STATIC_SCENE_DESCRIPTION: &str = "Image placeholders and link icons as render_static_scene draws them in the editor (excali-scene tests/image_elements.rs): bootstrapCanvas's background as fillRect, an image element whose file is not in the image cache (drawImagePlaceholder: a #E7E7E7 fillRect and the built-in excalidraw:image-placeholder), one whose status is error (excalidraw:image-error-placeholder), rotated, and their link icons (excalidraw:external-link, excalidraw:element-link) on a fillRect of the view background. The built-in images are named by id only: the port's backend resolves them itself (builtinImages gives Chrome upstream's data URLs for them).";

/// No text is measured: the scene has no iframes.
struct NoText;

impl TextMetricsProvider for NoText {
    fn get_line_width(&self, _: &str, _: &str) -> f64 {
        panic!("no text in the scene")
    }
}

fn image_element(id: &str, x: f64, y: f64, w: f64, h: f64, extra: Value) -> Element {
    let mut raw = json!({
        "id": id, "type": "image", "x": x, "y": y, "width": w, "height": h,
        "angle": 0, "strokeColor": "transparent", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "index": null, "roundness": null,
        "seed": 1, "version": 1, "versionNonce": 0, "isDeleted": false,
        "boundElements": null, "updated": 1, "created": 1, "link": null, "locked": false,
        "status": "saved", "fileId": "not-loaded", "scale": [1, 1], "crop": null
    });
    for (k, v) in extra.as_object().unwrap() {
        raw[k] = v.clone();
    }
    serde_json::from_value(raw).expect("an image element")
}

/// `renderStaticScene` in the editor over two image elements whose files
/// are not loaded, each with a link.
fn static_scene_images() -> DisplayList {
    let elements = vec![
        image_element(
            "placeholder",
            20.0,
            30.0,
            120.0,
            80.0,
            json!({"link": "https://example.com"}),
        ),
        image_element(
            "error",
            170.0,
            40.0,
            100.0,
            70.0,
            json!({
                "status": "error",
                "angle": 0.3,
                "link": "https://excalidraw.com/?element=placeholder"
            }),
        ),
    ];
    let map = ElementsMap::new(&elements);
    let visible: Vec<&Element> = elements.iter().collect();
    let config = StaticCanvasRenderConfig {
        render_grid: false,
        location_host: "excalidraw.com".to_owned(),
        ..StaticCanvasRenderConfig::default()
    };
    let app_state = StaticCanvasAppState {
        view_background_color: Some("#f8f9fa".to_owned()),
        ..StaticCanvasAppState::default()
    };
    render_static_scene(&StaticScene {
        canvas_width: f64::from(STATIC_SCENE_WIDTH),
        canvas_height: f64::from(STATIC_SCENE_HEIGHT),
        scale: 1.0,
        elements_map: &map,
        all_elements_map: &map,
        visible_elements: &visible,
        app_state: &app_state,
        render_config: &config,
        text_metrics: &NoText,
    })
}

fn image_ids(items: &[DisplayItem], out: &mut Vec<String>) {
    for item in items {
        match item {
            DisplayItem::Image(i) => out.push(i.id.clone()),
            DisplayItem::Group(g) => image_ids(&g.items, out),
            _ => {}
        }
    }
}

fn static_scene_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../excali-raster/tests/fixtures/display-lists/{STATIC_SCENE_FIXTURE}.json"
    ))
}

/// The static scene names upstream's placeholders and link icons by the
/// built-in ids every backend resolves, and the raster fixture
/// `static-scene-images.json` (drawn by Chrome from upstream's data URLs,
/// by excali-raster from its own built-in images) is its output.
#[test]
fn the_static_scene_names_built_in_images_the_backends_resolve() {
    let list = static_scene_images();
    let mut ids = Vec::new();
    image_ids(&list.items, &mut ids);
    assert_eq!(
        ids,
        // each element's link icon right after it (staticScene.ts)
        [
            IMAGE_PLACEHOLDER_ID,
            EXTERNAL_LINK_ID,
            IMAGE_ERROR_PLACEHOLDER_ID,
            ELEMENT_LINK_ID
        ]
    );
    for id in &ids {
        assert!(builtin_image_by_id(id).is_some(), "{id} resolves");
    }
    // the background is bootstrapCanvas's fillRect
    fn first_leaf(items: &[DisplayItem]) -> Option<&DisplayItem> {
        items.iter().find_map(|item| match item {
            DisplayItem::Group(g) => first_leaf(&g.items),
            other => Some(other),
        })
    }
    assert!(matches!(
        first_leaf(&list.items),
        Some(DisplayItem::FillRect { color, .. }) if color.as_str() == "#f8f9fa"
    ));

    let items: Vec<Value> = list.items.iter().map(item_json).collect();
    let builtins: serde_json::Map<String, Value> = BUILTIN_IMAGE_NAMES
        .iter()
        .map(|name| builtin_image(name).unwrap())
        .filter(|b| ids.iter().any(|id| id == b.id))
        .map(|b| (b.id.to_owned(), json!(b.data_url)))
        .collect();
    if std::env::var_os("EXCALI_WRITE_RASTER_FIXTURE").is_some() {
        let existing: Option<Value> = std::fs::read_to_string(static_scene_file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        let keep = |key: &str, default: Value| {
            existing
                .as_ref()
                .and_then(|v| v.get(key).cloned())
                .unwrap_or(default)
        };
        let header = [
            ("description", json!(STATIC_SCENE_DESCRIPTION)),
            ("width", json!(STATIC_SCENE_WIDTH)),
            ("height", json!(STATIC_SCENE_HEIGHT)),
            (
                "tolerance",
                keep("tolerance", json!({"channel": 0, "pixels": 0})),
            ),
            (
                "toleranceNote",
                keep("toleranceNote", json!("to be measured")),
            ),
            ("builtinImages", Value::Object(builtins.clone())),
        ];
        let mut out = String::from("{\n");
        for (k, v) in header {
            out.push_str(&format!(" {}: {},\n", json!(k), v));
        }
        out.push_str(" \"items\": [\n");
        out.push_str(
            &items
                .iter()
                .map(|v| format!("  {v}"))
                .collect::<Vec<_>>()
                .join(",\n"),
        );
        out.push_str("\n ]\n}\n");
        std::fs::write(static_scene_file(), out).unwrap();
    }
    let text = std::fs::read_to_string(static_scene_file()).unwrap_or_else(|_| {
        panic!("{STATIC_SCENE_FIXTURE}.json exists (EXCALI_WRITE_RASTER_FIXTURE=1 writes it)")
    });
    let file: Value = serde_json::from_str(&text).unwrap();
    let stale = format!("{STATIC_SCENE_FIXTURE}.json is stale: EXCALI_WRITE_RASTER_FIXTURE=1 cargo test -p excali-scene --test image_elements, then scripts/fixtures/raster-references.sh");
    if let Err(at) = close(&file["items"], &Value::Array(items), "items") {
        panic!("{stale} ({at})");
    }
    assert_eq!(file["builtinImages"], Value::Object(builtins), "{stale}");
    assert_eq!(file["description"], STATIC_SCENE_DESCRIPTION, "{stale}");
    assert_eq!(
        (file["width"].as_u64(), file["height"].as_u64()),
        (
            Some(u64::from(STATIC_SCENE_WIDTH)),
            Some(u64::from(STATIC_SCENE_HEIGHT))
        )
    );
}
