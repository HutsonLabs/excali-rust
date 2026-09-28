//! The SVG document `exportToSvg` builds (ex-406,
//! `packages/excalidraw/scene/export.ts:293-508`): the root with its size,
//! the `svg-source` comment, `<metadata>` with the embedded scene, `<defs>`
//! with a clip path per frame and the `style-fonts` block, and the
//! background rectangle, serialized as `svgRoot.outerHTML`.
//!
//! Fixture: `tests/fixtures/svg-export.json`, upstream's own `exportToSvg`
//! at the pinned commit under jsdom 22.1.0 (`tools/goldens/svg-export.mjs`;
//! its test checks the shells against upstream's vitest snapshot). Each
//! scene's `shell` is the document when upstream starts rendering the
//! elements (`tests/elements.rs` holds the whole document).

mod support;

use excali_scene::export::{svg_document, SvgExportAppState, SvgExportOptions};
use excali_svg::{export_to_svg, to_svg_file, FontFiles, SVG_DOCUMENT_PREAMBLE};
use support::{document, elements, fixture, shell, Marker, TenPxPerCodeUnit};

#[test]
fn every_scene_matches_upstreams_document_shell() {
    let fixture = fixture();
    let source = fixture["source"].as_str().unwrap();
    let scenes = fixture["scenes"].as_array().unwrap();
    assert!(scenes.len() >= 25);
    let mut failures = Vec::new();
    for scene in scenes {
        let expected = scene["shell"].as_str().unwrap();
        let got = export_to_svg(&shell(&document(scene, source, true)), &Marker).outer_html();
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
    let svg =
        export_to_svg(&document(scene, "https://excalidraw.com", false), &Marker).outer_html();
    assert!(svg.starts_with(
        "<svg version=\"1.1\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 120 120\" \
         width=\"120\" height=\"120\"><!-- svg-source:excalidraw --><metadata></metadata>\
         <defs><style class=\"style-fonts\">\n      @font-face { font-family: Excalifont; "
    ));
    assert!(svg.contains("</style></defs><g stroke-linecap=\"round\" transform=\"translate(10 10) rotate(0 50 50)\"><path d=\"M0.32 50.63 "));
    assert!(svg.ends_with("original text</text></g></svg>"));
}

#[test]
fn a_file_starts_with_the_svg_preamble() {
    let fixture = fixture();
    let scene = &fixture["scenes"][0];
    let elements = elements(scene);
    let app_state = SvgExportAppState::from_app_state(scene["appState"].as_object().unwrap());
    let options = SvgExportOptions {
        skip_inlining_fonts: true,
        ..SvgExportOptions::new("https://excalidraw.com", &TenPxPerCodeUnit)
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

#[test]
fn without_the_font_files_a_face_names_upstreams_asset_url() {
    // getContent answers the face's last url when it cannot fetch the file
    // (ExcalidrawFontFace.ts:58-86): the file under ASSETS_FALLBACK_URL's
    // fonts/ directory, where upstream's package build writes it
    // (scripts/buildPackage.js, `assetNames: "[dir]/[name]"`).
    let fixture = fixture();
    let scene = &fixture["scenes"][0];
    let elements = elements(scene);
    let app_state = SvgExportAppState::from_app_state(scene["appState"].as_object().unwrap());
    let options = SvgExportOptions::new("https://excalidraw.com", &TenPxPerCodeUnit);
    let missing = FontFiles::new(concat!(env!("CARGO_MANIFEST_DIR"), "/no-such-font-dir"));
    let svg = export_to_svg(
        &svg_document(&elements, &app_state, None, &options),
        &missing,
    )
    .outer_html();
    assert!(
        svg.contains(
            "src: url(https://esm.sh/@excalidraw/excalidraw/dist/prod/fonts/Excalifont/\
             Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2);"
        ),
        "{svg}"
    );
}
