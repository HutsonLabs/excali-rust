//! PNG export, the scene side (ex-405): `exportToCanvas`
//! (`packages/excalidraw/scene/export.ts:180-285`), the utils wrapper's
//! canvas sizing (`packages/utils/src/export.ts:42-105`) and the scene text
//! a PNG export embeds (`data/index.ts:173-186`, `data/image.ts:25-47`),
//! against what upstream does.
//!
//! Fixture: `tests/fixtures/canvas-export.json`, upstream's own
//! `exportToCanvas` and utils `exportToCanvas` at the pinned commit on a
//! recording 2D context (`tools/goldens/png-export.mjs`). Per scene the
//! port must make a canvas of the same width and height (the sizing
//! formula: common bounds plus twice the padding, times the export scale,
//! as the canvas's `width` attribute holds it) and draw on it what upstream
//! drew, draw for draw: the background (or none), the scroll that puts the
//! content's top left corner at the padding, the scale, frame labels,
//! the exporting frame's elements, images and placeholders.

use std::collections::HashSet;

use excali_core::element::Element;
use excali_core::png::{decode_png_metadata, decode_text_chunk, encode_chunks, Chunk};
use excali_scene::canvas_export::{
    canvas_height, canvas_width, export_canvas_png, export_to_canvas, png_payload,
    CanvasExportOptions, CanvasSizing, Dimensions, ExportCanvasError,
};
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{Map, Value};

#[path = "support/draws.rs"]
mod draws;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/canvas-export.json")).unwrap()
}

/// Upstream's golden text metrics: 10 px per UTF-16 code unit.
struct TenPxPerCodeUnit;

impl TextMetricsProvider for TenPxPerCodeUnit {
    fn get_line_width(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

fn elements(value: &Value) -> Vec<Element> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
        .collect()
}

fn object(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The files whose data URL loads as an image in the generator (its
/// `LOADABLE` table): the one PNG.
fn loads(file_id: &str) -> bool {
    file_id == "file-png"
}

/// `(w, h) => ({ width: w * factor, height: h * factor, scale })`.
fn get_dimensions(spec: &Value) -> impl Fn(f64, f64) -> Dimensions + '_ {
    move |width, height| {
        let factor = spec["factor"].as_f64().unwrap();
        Dimensions {
            width: width * factor,
            height: height * factor,
            scale: spec.get("scale").and_then(Value::as_f64),
        }
    }
}

/// The images the draws name: the loaded file and its natural size.
fn images() -> Value {
    serde_json::json!({ "file-png": { "naturalWidth": 64, "naturalHeight": 48 } })
}

struct Exported {
    width: u32,
    height: u32,
    list: excali_scene::display::DisplayList,
}

/// Export the scene as the port does.
fn export(scene: &Value) -> Exported {
    let all = elements(&scene["elements"]);
    let app_state = object(&scene["appState"]);
    let files = object(&scene["files"]);
    let opts = &scene["opts"];
    let frame = opts["exportingFrame"]
        .as_str()
        .map(|id| all.iter().find(|e| e.base.id == id).unwrap());
    let sizing = &scene["sizing"];
    let dims_spec = &sizing["getDimensions"];
    let dims = get_dimensions(dims_spec);
    let sizing = match sizing["kind"].as_str().unwrap() {
        "exportScale" => CanvasSizing::ExportScale,
        "utils" => CanvasSizing::Utils {
            max_width_or_height: sizing["maxWidthOrHeight"].as_f64(),
            export_scale: sizing["exportScale"].as_f64(),
            get_dimensions: if dims_spec.is_null() {
                None
            } else {
                Some(&dims)
            },
        },
        other => panic!("sizing {other}"),
    };
    let options = CanvasExportOptions {
        export_background: opts["exportBackground"].as_bool().unwrap(),
        export_padding: opts["exportPadding"].as_f64(),
        view_background_color: opts["viewBackgroundColor"].as_str().unwrap().to_owned(),
        exporting_frame: frame,
        sizing,
        text_metrics: &TenPxPerCodeUnit,
        image_loads: &loads,
    };
    let doc = export_to_canvas(&all, &app_state, &files, &options);
    assert!(doc.payload.is_none(), "exportToCanvas embeds nothing");
    Exported {
        width: doc.width,
        height: doc.height,
        list: doc.list,
    }
}

fn scenes() -> Vec<Value> {
    fixture()["scenes"].as_array().unwrap().clone()
}

#[test]
fn every_scene_matches_upstream() {
    let mut failures = Vec::new();
    let mut compared = 0;
    for scene in scenes() {
        let name = scene["name"].as_str().unwrap();
        let out = export(&scene);
        let size = (
            scene["width"].as_u64().unwrap(),
            scene["height"].as_u64().unwrap(),
        );
        if (u64::from(out.width), u64::from(out.height)) != size {
            failures.push(format!(
                "{name}: canvas {}x{}, upstream's is {}x{}",
                out.width, out.height, size.0, size.1
            ));
            continue;
        }
        match draws::compare(&out.list, &scene["events"], &images()) {
            Ok(n) => compared += n,
            Err(why) => failures.push(format!("{name} {why}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(compared > 180, "only {compared} draws compared");
}

#[test]
fn the_fixture_covers_padding_scale_background_and_sizing() {
    let names: HashSet<String> = scenes()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_owned())
        .collect();
    for name in [
        "default",
        "scale-2",
        "fractional-scale-1.5",
        "padding-0",
        "padding-25.5",
        "no-background",
        "background-transparent",
        "dark",
        "frames",
        "exporting-frame",
        "images",
        "negative-size",
        "utils-max-smaller",
        "utils-max-larger",
        "utils-get-dimensions",
        "utils-plain",
    ] {
        assert!(names.contains(name), "no scene {name}");
    }
}

/// `DEFAULT_EXPORT_PADDING` is 10 on every side, and the export scale
/// multiplies the canvas.
#[test]
fn default_padding_is_10_and_scale_multiplies_the_size() {
    let all = scenes();
    let by_name = |n: &str| all.iter().find(|s| s["name"] == n).unwrap().clone();
    let default = export(&by_name("default"));
    let padding_0 = export(&by_name("padding-0"));
    assert_eq!(default.width, padding_0.width + 20);
    assert_eq!(default.height, padding_0.height + 20);
    let scale_2 = export(&by_name("scale-2"));
    assert_eq!(scale_2.width, default.width * 2);
    assert_eq!(scale_2.height, default.height * 2);
}

// ---------------------------------------------------------------------------
// The canvas's size

#[test]
fn canvas_size_reflects_an_unsigned_long() {
    // WebIDL unsigned long (truncate, modulo 2^32), then HTML's reflection:
    // out of 0..=2147483647 sets the default, 300 wide and 150 high
    assert_eq!(canvas_width(123.9), 123);
    assert_eq!(canvas_width(0.5), 0);
    assert_eq!(canvas_width(-5.0), 300);
    assert_eq!(canvas_height(-5.0), 150);
    assert_eq!(canvas_width(-0.5), 0);
    assert_eq!(canvas_width(f64::NAN), 0);
    assert_eq!(canvas_width(f64::INFINITY), 0);
    assert_eq!(canvas_height(f64::NEG_INFINITY), 0);
    assert_eq!(canvas_width(2_147_483_647.0), 2_147_483_647);
    assert_eq!(canvas_width(2_147_483_648.0), 300);
    assert_eq!(canvas_height(2_147_483_648.0), 150);
    assert_eq!(canvas_width(4_294_967_296.0 + 7.0), 7);
    assert_eq!(canvas_height(-4_294_967_296.0 + 7.0), 7);
}

#[test]
fn export_scale_sizing_multiplies_and_scales() {
    let mut app_state = Map::new();
    app_state.insert("exportScale".into(), Value::from(1.5));
    let canvas = CanvasSizing::ExportScale.create_canvas(181.3, 58.1, &app_state);
    assert_eq!((canvas.width, canvas.height, canvas.scale), (271, 87, 1.5));
    // no exportScale: width × undefined is NaN, a 0 canvas, drawn at 1
    let canvas = CanvasSizing::ExportScale.create_canvas(181.3, 58.1, &Map::new());
    assert_eq!((canvas.width, canvas.height, canvas.scale), (0, 0, 1.0));
}

#[test]
fn utils_sizing_fits_the_larger_side() {
    let state = Map::new();
    let max = |m: f64, s: Option<f64>| CanvasSizing::Utils {
        max_width_or_height: Some(m),
        export_scale: s,
        get_dimensions: None,
    };
    // smaller than the content: the scale that fits the larger side
    let c = max(100.0, Some(2.0)).create_canvas(400.0, 200.0, &state);
    assert_eq!((c.width, c.height, c.scale), (100, 50, 0.25));
    // larger: the caller's exportScale, else 1
    let c = max(1000.0, Some(2.0)).create_canvas(400.0, 200.0, &state);
    assert_eq!((c.width, c.height, c.scale), (800, 400, 2.0));
    let c = max(1000.0, None).create_canvas(400.0, 200.0, &state);
    assert_eq!((c.width, c.height, c.scale), (400, 200, 1.0));
    // equal to the larger side: not smaller, the export scale
    let c = max(400.0, Some(3.0)).create_canvas(400.0, 200.0, &state);
    assert_eq!((c.width, c.height, c.scale), (1200, 600, 3.0));
    // 0 is falsy: getDimensions, else the content size at 1
    let c = max(0.0, Some(3.0)).create_canvas(400.0, 200.0, &state);
    assert_eq!((c.width, c.height, c.scale), (400, 200, 1.0));
    let half = |w: f64, h: f64| Dimensions {
        width: w / 2.0,
        height: h / 2.0,
        scale: None,
    };
    let c = CanvasSizing::Utils {
        max_width_or_height: None,
        export_scale: Some(3.0),
        get_dimensions: Some(&half),
    }
    .create_canvas(401.0, 201.0, &state);
    assert_eq!((c.width, c.height, c.scale), (200, 100, 1.0));
}

// ---------------------------------------------------------------------------
// The embedded scene

#[test]
fn embedded_scene_is_upstreams_serialized_scene() {
    let doc = fixture();
    let source = doc["origin"].as_str().unwrap();
    let mut checked = 0;
    for scene in scenes() {
        let Some(expected) = scene["metadata"].as_str() else {
            continue;
        };
        let name = scene["name"].as_str().unwrap();
        let (elements, app_state) = match scene["sizing"]["kind"].as_str().unwrap() {
            // the editor's exportCanvas: what it exported, its app state
            "exportScale" => (elements(&scene["elements"]), object(&scene["appState"])),
            // utils exportToBlob: the elements restored (the golden's are),
            // the app state the caller passed
            _ => (
                elements(&scene["elements"]),
                object(&scene["utilsAppState"]),
            ),
        };
        let payload = png_payload(
            &elements,
            &app_state,
            Some(&object(&scene["files"])),
            source,
        );
        assert_eq!(payload.keyword, "application/vnd.excalidraw+json");
        // the tEXt chunk upstream reads back is the scene text
        let chunk = Chunk {
            name: *b"tEXt",
            data: payload.chunk_data(),
        };
        let text = decode_text_chunk(&chunk.data).unwrap();
        assert_eq!(text.keyword, payload.keyword);
        assert_eq!(text.text, payload.text);
        let png = encode_chunks(&[
            Chunk {
                name: *b"IHDR",
                data: vec![0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0],
            },
            chunk,
            Chunk {
                name: *b"IEND",
                data: vec![],
            },
        ]);
        let decoded = decode_png_metadata(&png).unwrap().unwrap();
        assert_eq!(decoded, expected, "{name}");
        checked += 1;
    }
    assert!(checked >= 6, "only {checked} embedded scenes");
}

#[test]
fn a_png_export_embeds_the_scene_when_asked() {
    let doc = fixture();
    let source = doc["origin"].as_str().unwrap();
    for scene in scenes()
        .iter()
        .filter(|s| s["sizing"]["kind"] == "exportScale")
    {
        let all = elements(&scene["elements"]);
        let app_state = object(&scene["appState"]);
        let files = object(&scene["files"]);
        let frame = scene["opts"]["exportingFrame"]
            .as_str()
            .map(|id| all.iter().find(|e| e.base.id == id).unwrap());
        let options = CanvasExportOptions {
            export_background: scene["opts"]["exportBackground"].as_bool().unwrap(),
            export_padding: scene["opts"]["exportPadding"].as_f64(),
            view_background_color: scene["opts"]["viewBackgroundColor"]
                .as_str()
                .unwrap()
                .to_owned(),
            exporting_frame: frame,
            sizing: CanvasSizing::ExportScale,
            text_metrics: &TenPxPerCodeUnit,
            image_loads: &loads,
        };
        let png = export_canvas_png(&all, &app_state, &files, &options, source).unwrap();
        let canvas = export_to_canvas(&all, &app_state, &files, &options);
        assert_eq!((png.width, png.height), (canvas.width, canvas.height));
        assert_eq!(png.list, canvas.list);
        match (&png.payload, scene["metadata"].as_str()) {
            (None, None) => {}
            (Some(p), Some(expected)) => {
                let text = excali_core::encode::decode(
                    &serde_json::from_str(&p.text).expect("payload wrapper JSON"),
                )
                .unwrap();
                assert_eq!(text, expected);
            }
            (p, e) => panic!("{}: payload {p:?}, upstream embeds {e:?}", scene["name"]),
        }
    }
}

#[test]
fn an_empty_canvas_cannot_be_exported() {
    let options = CanvasExportOptions {
        export_background: true,
        export_padding: None,
        view_background_color: "#ffffff".to_owned(),
        exporting_frame: None,
        sizing: CanvasSizing::ExportScale,
        text_metrics: &TenPxPerCodeUnit,
        image_loads: &loads,
    };
    let err = export_canvas_png(&[], &Map::new(), &Map::new(), &options, "x").unwrap_err();
    assert_eq!(err, ExportCanvasError::EmptyCanvas);
}
