//! The fixture display lists against Chrome (ex-401).
//!
//! Every `tests/fixtures/display-lists/<name>.json` is a display list in
//! the canvas's own vocabulary (see `tests/fixtures/README.md`).
//! `scripts/fixtures/raster-references.sh` replays each one on a real
//! `CanvasRenderingContext2D` in headless Chrome, the renderer upstream
//! draws with, and keeps the pixels as `tests/fixtures/chrome/<name>.png`.
//! This test builds the same list as an `excali_scene::display::DisplayList`,
//! renders it with the tiny-skia backend and compares the two with
//! `excali_raster::diff` under the fixture's own tolerance.
//!
//! `EXCALI_RASTER_REFERENCES=<dir>` reads the references from another
//! directory (the generator's `--check` renders them with the Chrome on the
//! machine and points here). On a failure the rendered pixmap and a diff
//! image are written to `target/raster-diff/`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path as FsPath, PathBuf};

use excali_raster::diff::{compare, diff_image, Tolerance};
use excali_raster::tiny_skia::{self, ColorU8, IntSize, Mask, Pixmap};
use excali_raster::{render_scaled, TextRasterizer};
use excali_scene::display::{
    Clip, Color, Dash, DisplayItem, DisplayList, FillRule, Group, ImageFilter, ImageItem, LineCap,
    LineJoin, Path, Rect, Stroke, TextRun, Transform,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn lists_dir() -> PathBuf {
    crate_dir().join("tests/fixtures/display-lists")
}

fn references_dir() -> PathBuf {
    match std::env::var_os("EXCALI_RASTER_REFERENCES") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => crate_dir().join("tests/fixtures/chrome"),
    }
}

/// Fixture names in file-name order.
fn fixture_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(lists_dir())
        .expect("tests/fixtures/display-lists exists")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A fixture read into the port's types.
struct Fixture {
    width: u32,
    height: u32,
    scale: f64,
    tolerance: Tolerance,
    images: HashMap<String, Pixmap>,
    list: DisplayList,
}

/// A number: JSON numbers, or `"NaN"`, `"Infinity"` and `"-Infinity"`,
/// which JSON cannot write and the canvas rules are about.
fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            other => panic!("not a number: {other:?}"),
        },
        other => panic!("not a number: {other}"),
    }
}

fn nums(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap_or_else(|| panic!("not an array: {v}"))
        .iter()
        .map(num)
        .collect()
}

fn rect(v: &Value) -> Rect {
    let n = nums(v);
    assert_eq!(n.len(), 4, "a rectangle is [x, y, width, height]: {v}");
    Rect::new(n[0], n[1], n[2], n[3])
}

fn rule(v: Option<&Value>) -> FillRule {
    match v.and_then(Value::as_str) {
        None | Some("nonzero") => FillRule::NonZero,
        Some("evenodd") => FillRule::EvenOdd,
        Some(other) => panic!("unknown fill rule {other:?}"),
    }
}

/// A path: canvas calls `["M", x, y]`, `["L", x, y]`, `["Q", cpx, cpy, x,
/// y]`, `["C", ...6]`, `["A", cx, cy, r, start, end, anticlockwise]`,
/// `["Z"]`, and the rectangle methods `["rect", x, y, w, h]` and
/// `["roundRect", x, y, w, h, r]` through the port's `Path::rect` and
/// `Path::round_rect`.
fn path(v: &Value) -> Path {
    let mut p = Path::new();
    for call in v.as_array().expect("a path is an array of calls") {
        let call = call.as_array().expect("a path call is an array");
        let op = call[0].as_str().expect("a path call starts with its name");
        let args = &call[1..];
        let n = |i: usize| num(&args[i]);
        match op {
            "M" => {
                p.move_to(n(0), n(1));
            }
            "L" => {
                p.line_to(n(0), n(1));
            }
            "Q" => {
                p.quad_to(n(0), n(1), n(2), n(3));
            }
            "C" => {
                p.cubic_to(n(0), n(1), n(2), n(3), n(4), n(5));
            }
            "A" => {
                let anticlockwise = args.get(5).and_then(Value::as_bool).unwrap_or(false);
                p.arc(n(0), n(1), n(2), n(3), n(4), anticlockwise);
            }
            "Z" => {
                p.close();
            }
            "rect" => p
                .commands
                .extend(Path::rect(n(0), n(1), n(2), n(3)).commands),
            "roundRect" => p
                .commands
                .extend(Path::round_rect(n(0), n(1), n(2), n(3), n(4)).commands),
            other => panic!("unknown path call {other:?}"),
        }
    }
    p
}

fn item(v: &Value) -> DisplayItem {
    let kind = v["type"].as_str().expect("an item has a type");
    match kind {
        "fill" => DisplayItem::Fill {
            path: path(&v["path"]),
            color: Color::new(v["color"].as_str().expect("a fill has a colour")),
            rule: rule(v.get("rule")),
        },
        "stroke" => {
            let mut stroke = Stroke::new(
                Color::new(v["color"].as_str().expect("a stroke has a colour")),
                v.get("width").map_or(1.0, num),
            );
            stroke.cap = match v.get("cap").and_then(Value::as_str) {
                None | Some("butt") => LineCap::Butt,
                Some("round") => LineCap::Round,
                Some("square") => LineCap::Square,
                Some(other) => panic!("unknown cap {other:?}"),
            };
            stroke.join = match v.get("join").and_then(Value::as_str) {
                None | Some("miter") => LineJoin::Miter,
                Some("round") => LineJoin::Round,
                Some("bevel") => LineJoin::Bevel,
                Some(other) => panic!("unknown join {other:?}"),
            };
            if let Some(limit) = v.get("miterLimit") {
                stroke.miter_limit = num(limit);
            }
            if let Some(dash) = v.get("dash") {
                stroke.dash = Dash::new(&nums(dash), v.get("dashOffset").map_or(0.0, num));
            }
            DisplayItem::Stroke {
                path: path(&v["path"]),
                stroke,
            }
        }
        "image" => DisplayItem::Image(ImageItem {
            id: v["id"].as_str().expect("an image has an id").to_owned(),
            source: v.get("source").map(rect),
            dest: rect(&v["dest"]),
            smoothing: v.get("smoothing").and_then(Value::as_bool).unwrap_or(true),
            filter: match v.get("filter").and_then(Value::as_str) {
                None => None,
                Some("dark") => Some(ImageFilter::DarkTheme),
                Some(other) => panic!("unknown image filter {other:?}"),
            },
        }),
        "group" => {
            let transform = match v.get("transform") {
                Some(t) => {
                    let m = nums(t);
                    assert_eq!(m.len(), 6, "a transform is [a, b, c, d, e, f]: {t}");
                    Transform::new(m[0], m[1], m[2], m[3], m[4], m[5])
                }
                None => Transform::IDENTITY,
            };
            DisplayItem::Group(Group {
                transform,
                opacity: v.get("opacity").map_or(1.0, num),
                clip: v.get("clip").map(|c| Clip {
                    path: path(&c["path"]),
                    rule: rule(c.get("rule")),
                }),
                items: items(&v["items"]),
            })
        }
        other => panic!("unknown item type {other:?}"),
    }
}

fn items(v: &Value) -> Vec<DisplayItem> {
    v.as_array()
        .expect("items is an array")
        .iter()
        .map(item)
        .collect()
}

/// An image given as unpremultiplied RGBA rows, as `ImageData` holds it.
fn image(v: &Value) -> Pixmap {
    let w = v["width"].as_u64().unwrap() as u32;
    let h = v["height"].as_u64().unwrap() as u32;
    let rgba: Vec<u8> = v["rgba"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| u8::try_from(c.as_u64().unwrap()).unwrap())
        .collect();
    assert_eq!(rgba.len() as u32, w * h * 4, "rgba has width * height pixels");
    let mut data = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let c = ColorU8::from_rgba(px[0], px[1], px[2], px[3]).premultiply();
        data.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Pixmap::from_vec(data, IntSize::from_wh(w, h).unwrap()).unwrap()
}

fn load(name: &str) -> Fixture {
    let file = lists_dir().join(format!("{name}.json"));
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    assert!(
        v["description"].as_str().is_some_and(|d| !d.is_empty()),
        "{name}: every fixture says what it pins"
    );
    let tolerance = &v["tolerance"];
    let tolerance = Tolerance {
        channel: u8::try_from(tolerance["channel"].as_u64().expect("tolerance.channel")).unwrap(),
        pixels: tolerance["pixels"].as_u64().expect("tolerance.pixels") as usize,
    };
    let images = v
        .get("images")
        .and_then(Value::as_object)
        .map(|m| m.iter().map(|(id, img)| (id.clone(), image(img))).collect())
        .unwrap_or_default();
    Fixture {
        width: v["width"].as_u64().unwrap() as u32,
        height: v["height"].as_u64().unwrap() as u32,
        scale: v.get("scale").map_or(1.0, num),
        tolerance,
        images,
        list: items(&v["items"]).into_iter().collect(),
    }
}

/// The fixtures have no text; a text run reaching the rasterizer is a
/// fixture error.
struct NoText;

impl TextRasterizer for NoText {
    fn fill_text(
        &mut self,
        _: &mut Pixmap,
        run: &TextRun,
        _: tiny_skia::Color,
        _: tiny_skia::Transform,
        _: Option<&Mask>,
    ) {
        panic!("fixture display lists have no text, got {:?}", run.text);
    }
}

fn rendered(f: &Fixture) -> Pixmap {
    let mut pixmap = Pixmap::new(f.width, f.height).unwrap();
    render_scaled(&f.list, &mut pixmap, f.scale, &f.images, &mut NoText);
    pixmap
}

fn out_dir() -> PathBuf {
    // tests run in the crate directory; target/ is the workspace's.
    let dir = crate_dir().join("../../target/raster-diff");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sha256(file: &FsPath) -> String {
    let bytes = std::fs::read(file).unwrap();
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn every_fixture_matches_chrome_within_its_tolerance() {
    let names = fixture_names();
    assert!(!names.is_empty(), "no fixture display lists");
    let mut failures = Vec::new();
    let mut summary = Vec::new();
    for name in &names {
        let f = load(name);
        let actual = rendered(&f);
        let reference = references_dir().join(format!("{name}.png"));
        let expected = Pixmap::load_png(&reference)
            .unwrap_or_else(|e| panic!("{}: {e}", reference.display()));
        let diff = compare(actual.as_ref(), expected.as_ref())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        summary.push(format!(
            "{name}: max {} over {} {}/{} (tolerance {} / {})",
            diff.max_channel,
            f.tolerance.channel,
            diff.pixels_over(f.tolerance.channel),
            diff.width * diff.height,
            f.tolerance.channel,
            f.tolerance.pixels,
        ));
        if !diff.within(&f.tolerance) {
            let out = out_dir();
            actual.save_png(out.join(format!("{name}.png"))).unwrap();
            diff_image(actual.as_ref(), expected.as_ref(), &f.tolerance)
                .unwrap()
                .save_png(out.join(format!("{name}-diff.png")))
                .unwrap();
            failures.push(format!("{name}: {}", diff.report(&f.tolerance)));
        }
    }
    eprintln!("{}", summary.join("\n"));
    assert!(
        failures.is_empty(),
        "{} of {} fixtures differ from Chrome (rendered and diff images in target/raster-diff/):\n{}",
        failures.len(),
        names.len(),
        failures.join("\n")
    );
}

/// The references are Chrome's pixels for these exact fixture files: the
/// manifest records each fixture's SHA-256, so editing a fixture without
/// regenerating its reference fails here, and so does a reference without a
/// fixture.
#[test]
fn references_are_current() {
    let dir = references_dir();
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
            .unwrap();
    assert!(
        manifest["userAgent"]
            .as_str()
            .is_some_and(|ua| ua.contains("Chrome/")),
        "the manifest names the Chrome that drew the references"
    );
    let recorded: BTreeMap<String, String> = manifest["fixtures"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect();
    let current: BTreeMap<String, String> = fixture_names()
        .into_iter()
        .map(|n| {
            let digest = sha256(&lists_dir().join(format!("{n}.json")));
            (n, digest)
        })
        .collect();
    assert_eq!(
        recorded, current,
        "references are stale: run scripts/fixtures/raster-references.sh"
    );
    let mut pngs: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    pngs.sort();
    assert_eq!(pngs, fixture_names(), "one reference per fixture");
}

/// Each tolerance is earned: a fixture's list must differ from an empty
/// canvas by more than its tolerance, so no fixture passes by drawing
/// nothing.
#[test]
fn every_fixture_draws_more_than_its_tolerance() {
    for name in fixture_names() {
        let f = load(&name);
        let blank = Pixmap::new(f.width, f.height).unwrap();
        let expected = Pixmap::load_png(references_dir().join(format!("{name}.png"))).unwrap();
        let diff = compare(blank.as_ref(), expected.as_ref()).unwrap();
        assert!(
            !diff.within(&f.tolerance),
            "{name}: an empty canvas passes the tolerance {:?}",
            f.tolerance
        );
    }
}

/// The loader reads what the generator page draws: the call names, the
/// non-finite spellings and the defaults.
#[test]
fn the_fixture_vocabulary() {
    let v: Value = serde_json::json!([
        ["M", 1, 2], ["L", "NaN", 3], ["Q", 1, 2, 3, 4], ["C", 1, 2, 3, 4, 5, 6],
        ["A", 5, 5, 2, 0, "Infinity", true], ["Z"], ["rect", 0, 0, 2, 3],
        ["roundRect", 0, 0, 4, 4, 1]
    ]);
    let p = path(&v);
    let mut expected = Path::new();
    expected
        .move_to(1.0, 2.0)
        .line_to(f64::NAN, 3.0)
        .quad_to(1.0, 2.0, 3.0, 4.0)
        .cubic_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)
        .arc(5.0, 5.0, 2.0, 0.0, f64::INFINITY, true)
        .close();
    expected.commands.extend(Path::rect(0.0, 0.0, 2.0, 3.0).commands);
    expected
        .commands
        .extend(Path::round_rect(0.0, 0.0, 4.0, 4.0, 1.0).commands);
    // NaN != NaN: compare the debug text.
    assert_eq!(format!("{p:?}"), format!("{expected:?}"));

    let s = item(&serde_json::json!({"type": "stroke", "path": [], "color": "red"}));
    let DisplayItem::Stroke { stroke, .. } = s else {
        panic!("a stroke")
    };
    assert_eq!(stroke, Stroke::new(Color::new("red"), 1.0));

    let g = item(&serde_json::json!({"type": "group", "items": []}));
    assert_eq!(g, DisplayItem::Group(Group::new(vec![])));

    let img = image(&serde_json::json!({"width": 1, "height": 1, "rgba": [255, 0, 0, 128]}));
    let c = img.pixel(0, 0).unwrap();
    assert_eq!((c.red(), c.green(), c.blue(), c.alpha()), (128, 0, 0, 128));
}
