//! The SVG document `exportToSvg` builds (ex-406,
//! `packages/excalidraw/scene/export.ts:293-508`): the root with its size,
//! the `svg-source` comment, `<metadata>` with the embedded scene, `<defs>`
//! with a clip path per frame and the `style-fonts` block, and the
//! background rectangle, serialized as `svgRoot.outerHTML`.
//!
//! Fixture: `tests/fixtures/svg-export.json`, upstream's own `exportToSvg`
//! at the pinned commit under jsdom 22.1.0 (`tools/goldens/svg-export.mjs`;
//! its test checks the shells against upstream's vitest snapshot). A font
//! face's content there is `font:<file>#<characters>` and text measures
//! 10 px per UTF-16 code unit.

use excali_core::element::Element;
use excali_scene::display::FontFaceSource;
use excali_scene::export::{svg_document, SvgExportAppState, SvgExportOptions, TextMetrics};
use excali_svg::{export_to_svg, to_svg_file, FontContent, SVG_DOCUMENT_PREAMBLE};
use serde_json::{Map, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/svg-export.json")).unwrap()
}

struct TenPxPerCodeUnit;

impl TextMetrics for TenPxPerCodeUnit {
    fn measure(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

/// The fixture's font content: the file upstream's face names and the
/// characters it was asked to keep.
struct Marker;

impl FontContent for Marker {
    fn content(&self, face: &FontFaceSource) -> String {
        format!("font:{}#{}", face.upstream_file, face.characters)
    }
}

fn elements(scene: &Value) -> Vec<Element> {
    scene["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
        .collect()
}

/// `exportToSvg(elements, appState, files, opts)` for a fixture scene, as
/// markup.
fn export(scene: &Value, source: &str) -> String {
    let elements = elements(scene);
    let app_state = SvgExportAppState::from_app_state(scene["appState"].as_object().unwrap());
    let files: Option<&Map<String, Value>> = scene["files"].as_object();
    let opts = &scene["opts"];
    let exporting_frame = opts["exportingFrame"]
        .as_str()
        .map(|id| elements.iter().find(|e| e.base.id == id).unwrap());
    let options = SvgExportOptions {
        source,
        exporting_frame,
        skip_inlining_fonts: opts["skipInliningFonts"].as_bool().unwrap_or(false),
        text_metrics: &TenPxPerCodeUnit,
    };
    let document = svg_document(&elements, &app_state, files, &options);
    export_to_svg(&document, &Marker).outer_html()
}

#[test]
fn every_scene_matches_upstreams_document() {
    let fixture = fixture();
    let source = fixture["source"].as_str().unwrap();
    let scenes = fixture["scenes"].as_array().unwrap();
    assert!(scenes.len() >= 25);
    let mut failures = Vec::new();
    for scene in scenes {
        let expected = scene["shell"].as_str().unwrap();
        let got = export(scene, source);
        if got != expected {
            failures.push(format!(
                "{}:\n  got      {got}\n  expected {expected}",
                scene["name"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_upstream_test_scene() {
    let fixture = fixture();
    let scene = &fixture["scenes"][0];
    assert_eq!(scene["name"], "fixture");
    let svg = export(scene, "https://excalidraw.com");
    assert!(svg.starts_with(
        "<svg version=\"1.1\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 120 120\" \
         width=\"120\" height=\"120\"><!-- svg-source:excalidraw --><metadata></metadata>\
         <defs><style class=\"style-fonts\">\n      @font-face { font-family: Excalifont; "
    ));
    assert!(svg.ends_with("</style></defs></svg>"));
}

#[test]
fn a_file_starts_with_the_svg_preamble() {
    let fixture = fixture();
    let scene = &fixture["scenes"][0];
    let elements = elements(scene);
    let app_state = SvgExportAppState::from_app_state(scene["appState"].as_object().unwrap());
    let options = SvgExportOptions {
        source: "https://excalidraw.com",
        exporting_frame: None,
        skip_inlining_fonts: true,
        text_metrics: &TenPxPerCodeUnit,
    };
    let root = export_to_svg(
        &svg_document(&elements, &app_state, None, &options),
        &Marker,
    );
    let file = to_svg_file(&root);
    assert_eq!(
        SVG_DOCUMENT_PREAMBLE,
        "<?xml version=\"1.0\" standalone=\"no\"?>\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \
         \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n"
    );
    assert_eq!(
        file,
        format!("{SVG_DOCUMENT_PREAMBLE}{}", root.outer_html())
    );
}
