//! The scenes of excali-scene's canvas export golden
//! (`crates/excali-scene/tests/fixtures/canvas-export.json`, upstream's
//! `exportToCanvas` through `tools/goldens/png-export.mjs`) as the documents
//! a PNG export renders: the editor's `exportCanvas("png")` for an editor
//! scene, the utils `exportToCanvas` plus `exportToBlob`'s embedded scene
//! for a utils scene. Shared by the PNG export tests, the `png_export`
//! example, and excali-cli's comparison with Chrome
//! (`crates/excali-cli/tests/chrome_export.rs`), which exports the same
//! scenes with the vendored fonts' metrics and the decoded image files.

use excali_core::element::Element;
use excali_scene::canvas_export::{
    export_canvas_png, export_to_canvas, png_payload, CanvasExportOptions, CanvasSizing, Dimensions,
};
use excali_scene::display::CanvasDocument;
use excali_scene::sticky_note::Clock;
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{Map, Value};

/// The golden file.
pub const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../excali-scene/tests/fixtures/canvas-export.json"
);

pub fn fixture() -> Value {
    serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap()
}

/// The generator's text metrics: 10 px per UTF-16 code unit.
pub struct TenPxPerCodeUnit;

impl TextMetricsProvider for TenPxPerCodeUnit {
    fn get_line_width(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

/// The generator's one loadable image file.
fn loads(file_id: &str) -> bool {
    file_id == "file-png"
}

fn object(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The document the port exports for `scene`, with the scene embedded when
/// upstream embeds it, measured and loading images as the generator did.
#[allow(dead_code)]
pub fn document(scene: &Value, source: &str) -> CanvasDocument {
    document_with(scene, source, &TenPxPerCodeUnit, &loads)
}

/// [`document`] with frame names and labels measured by `metrics` and
/// the files that load as images answered by `image_loads`.
pub fn document_with(
    scene: &Value,
    source: &str,
    metrics: &dyn TextMetricsProvider,
    image_loads: &dyn Fn(&str) -> bool,
) -> CanvasDocument {
    let all: Vec<Element> = scene["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
        .collect();
    let app_state = object(&scene["appState"]);
    let files = object(&scene["files"]);
    let opts = &scene["opts"];
    let frame = opts["exportingFrame"]
        .as_str()
        .map(|id| all.iter().find(|e| e.base.id == id).unwrap());
    let sizing = &scene["sizing"];
    let spec = sizing["getDimensions"].clone();
    let dims = move |width: f64, height: f64| {
        let factor = spec["factor"].as_f64().unwrap();
        Dimensions {
            width: width * factor,
            height: height * factor,
            scale: spec.get("scale").and_then(Value::as_f64),
        }
    };
    let utils = sizing["kind"] == "utils";
    let options = CanvasExportOptions {
        export_background: opts["exportBackground"].as_bool().unwrap(),
        export_padding: opts["exportPadding"].as_f64(),
        view_background_color: opts["viewBackgroundColor"].as_str().unwrap().to_owned(),
        exporting_frame: frame,
        sizing: if utils {
            CanvasSizing::Utils {
                max_width_or_height: sizing["maxWidthOrHeight"].as_f64(),
                export_scale: sizing["exportScale"].as_f64(),
                get_dimensions: if sizing["getDimensions"].is_null() {
                    None
                } else {
                    Some(&dims)
                },
            }
        } else {
            CanvasSizing::ExportScale
        },
        text_metrics: metrics,
        image_loads,
        clock: Clock::default(),
    };
    if !utils {
        return export_canvas_png(&all, &app_state, &files, &options, source).unwrap();
    }
    let mut doc = export_to_canvas(&all, &app_state, &files, &options);
    let caller = object(&scene["utilsAppState"]);
    if caller
        .get("exportEmbedScene")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        doc.payload = Some(png_payload(&all, &caller, Some(&files), source));
    }
    doc
}
