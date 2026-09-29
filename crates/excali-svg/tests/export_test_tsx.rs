//! ex-g401's acceptance: the two SVG snapshots of upstream's
//! `packages/excalidraw/tests/export.test.tsx`
//! (`tests/__snapshots__/export.test.tsx.snap`, vendored under
//! `fixtures/upstream/`), reproduced by the port as whole documents, byte
//! for byte after the whitespace normalisation of `upstream_snapshot.rs`.
//!
//! Both tests call `exportToSvg(elements, appState, files)`
//! (`scene/export.ts`) and snapshot `svg.outerHTML`, a string, which vitest
//! prints between double quotes. Upstream ran them under vitest
//! (`isTestEnv()`), so every element's node carries its `data-id`, and the
//! page is `http://localhost:3000`, the embedded scene's `source`.
//!
//! The inputs are rebuilt from the test:
//!
//! - "export svg-embedded scene" (`export.test.tsx:88-97`): `testElements`
//!   (`export.test.tsx:22-35`), one text element `A` of `😀` made by
//!   `API.createElement` (`tests/helpers/api.ts:341-354`,
//!   `newTextElement`) and forced to 16 by 16, exported with
//!   `{ ...getDefaultAppState(), exportEmbedScene: true }` and no files.
//!   Under the test's reseeded randomness the element's `seed` is 1, its
//!   `version` 1, `versionNonce` 0 and `created`/`updated` 1 (vitest's
//!   mocked clock); the literal below is the element as the snapshot's own
//!   payload holds it, in `newTextElement`'s key order.
//! - "exporting svg containing transformed images"
//!   (`export.test.tsx:152-223`): four image elements of `file_A` at 315,
//!   45, 45 and 315 degrees with scales `[1, 1]`, `[-1, 1]`, `[1, -1]`,
//!   `[-1, -1]`, made by `API.createElement` (`api.ts:391-400`,
//!   `newImageElement`), exported with `{ ...getDefaultAppState(),
//!   exportBackground: false }` and `file_A` the data URL of
//!   `tests/fixtures/deer.png` (`getDataURL`, `data/blob.ts`: a
//!   `FileReader` data URL, `data:image/png;base64,` and the file's
//!   base64).
//!
//! The one input the port cannot compute is the test run's `randomId`
//! sequence: the image elements have no explicit id, so they are named by
//! whatever `randomId()` calls the test file made before them
//! (`common/src/random.ts:16`, `id${testIdBase++}`), `id12` to `id15` in
//! the recorded run. The fonts need no recording: upstream's test
//! `FontFace` gives every face the range `U+0000-00FF`
//! (`setupTests.ts:65-86`), which `😀` is outside of, and the image export
//! has no text; both snapshots have an empty `style-fonts` block, and the
//! test checks the port inlines no face either.

mod support;

use std::collections::HashMap;

use excali_core::app_state::{get_default_app_state, AppStateEnv};
use excali_core::element::Element;
use excali_scene::export::{svg_document, SvgExportAppState, SvgExportOptions};
use excali_svg::dom::Tag;
use excali_svg::export_to_svg;
use serde_json::{json, Map, Value};
use support::{Marker, TenPxPerCodeUnit};

/// `window.location.origin` in upstream's vitest environment.
const TEST_ORIGIN: &str = "http://localhost:3000";

const SNAPSHOT_FILE: &str =
    "fixtures/upstream/packages/excalidraw/tests/__snapshots__/export.test.tsx.snap";

const EMBEDDED: &str = "export > export svg-embedded scene > svg-embdedded scene export output 1";
const TRANSFORMED: &str =
    "export > exporting svg containing transformed images > svg export output 1";

fn path(rel: &str) -> String {
    format!("{}/../../{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn snapshots() -> HashMap<String, String> {
    let text = std::fs::read_to_string(path(SNAPSHOT_FILE)).unwrap();
    support::parse_snapshots(&text)
}

/// `expect(svg.outerHTML).toMatchSnapshot()`: the string between double
/// quotes, on lines of its own.
fn outer_html_snapshot(root: &Tag) -> String {
    format!("\n\"{}\"\n", root.outer_html())
}

fn check(name: &str, got: &str) {
    let expected = &snapshots()[name];
    let (g, e) = (support::normalize(got), support::normalize(expected));
    if g != e {
        panic!("{name}: {}", support::first_difference(&g, &e));
    }
}

/// `getDefaultAppState()` in upstream's test build, extended as the test
/// extends it.
fn app_state(extra: Value) -> SvgExportAppState {
    let env = AppStateEnv {
        test_env: true,
        ..AppStateEnv::default()
    };
    let mut state = get_default_app_state(&env).into_map();
    for (key, value) in extra.as_object().unwrap() {
        state.insert(key.clone(), value.clone());
    }
    SvgExportAppState::from_app_state(&state)
}

fn element(value: Value) -> Element {
    Element::from_map(value.as_object().unwrap().clone()).unwrap()
}

/// `exportToSvg(elements, appState, files)` in upstream's test
/// environment.
fn export(elements: &[Element], app_state: &SvgExportAppState, files: &Map<String, Value>) -> Tag {
    let options = SvgExportOptions {
        data_ids: true,
        ..SvgExportOptions::new(TEST_ORIGIN, &TenPxPerCodeUnit)
    };
    let document = svg_document(elements, app_state, Some(files), &options);
    assert!(document.font_faces.is_empty(), "{:?}", document.font_faces);
    export_to_svg(&document, &Marker)
}

// -- export svg-embedded scene ---------------------------------------------------

/// `testElements` (`export.test.tsx:22-35`).
fn test_elements() -> Vec<Element> {
    vec![element(json!({
        "id": "A",
        "type": "text",
        "x": 0,
        "y": 0,
        "width": 16,
        "height": 16,
        "angle": 0,
        "strokeColor": "#1e1e1e",
        "backgroundColor": "transparent",
        "fillStyle": "solid",
        "strokeWidth": 2,
        "strokeStyle": "solid",
        "roughness": 1,
        "opacity": 100,
        "groupIds": [],
        "frameId": null,
        "index": null,
        "roundness": null,
        "seed": 1,
        "version": 1,
        "versionNonce": 0,
        "isDeleted": false,
        "boundElements": null,
        "updated": 1,
        "created": 1,
        "link": null,
        "locked": false,
        "text": "😀",
        "fontSize": 20,
        "baseFontSize": null,
        "fontFamily": 5,
        "textAlign": "left",
        "verticalAlign": "top",
        "containerId": null,
        "originalText": "😀",
        "autoResize": true,
        "lineHeight": 1.25,
        "labelPosition": null
    }))]
}

fn embedded_scene_export() -> Tag {
    export(
        &test_elements(),
        &app_state(json!({ "exportEmbedScene": true })),
        &Map::new(),
    )
}

#[test]
fn export_svg_embedded_scene() {
    check(EMBEDDED, &outer_html_snapshot(&embedded_scene_export()));
}

// -- exporting svg containing transformed images -------------------------------

/// `normalizeAngle` (`export.test.tsx:153`).
fn normalize_angle(angle: f64) -> f64 {
    (angle / 180.0) * std::f64::consts::PI
}

/// `API.createElement({ type: "image", fileId: "file_A", ... })`: the
/// defaults of `newImageElement` under the test's app state.
fn image(id: &str, x: f64, y: f64, scale: [f64; 2], size: f64, degrees: f64) -> Element {
    element(json!({
        "id": id,
        "type": "image",
        "x": x,
        "y": y,
        "width": size,
        "height": size,
        "angle": normalize_angle(degrees),
        "strokeColor": "transparent",
        "backgroundColor": "transparent",
        "fillStyle": "solid",
        "strokeWidth": 2,
        "strokeStyle": "solid",
        "roughness": 1,
        "opacity": 100,
        "groupIds": [],
        "frameId": null,
        "index": null,
        "roundness": null,
        "seed": 1,
        "version": 1,
        "versionNonce": 0,
        "isDeleted": false,
        "boundElements": null,
        "updated": 1,
        "created": 1,
        "link": null,
        "locked": false,
        "status": "saved",
        "fileId": "file_A",
        "scale": scale,
        "crop": null
    }))
}

/// `export.test.tsx:155-197`, with the ids of the recorded run.
fn transformed_images() -> Vec<Element> {
    vec![
        image("id12", 0.0, 0.0, [1.0, 1.0], 100.0, 315.0),
        image("id13", 100.0, 0.0, [-1.0, 1.0], 50.0, 45.0),
        image("id14", 0.0, 100.0, [1.0, -1.0], 100.0, 45.0),
        image("id15", 100.0, 100.0, [-1.0, -1.0], 50.0, 315.0),
    ]
}

/// `getDataURL(await API.loadFile("./fixtures/deer.png"))`.
fn deer_data_url() -> String {
    let bytes = std::fs::read(path(
        "fixtures/upstream/packages/excalidraw/tests/fixtures/deer.png",
    ))
    .unwrap();
    let byte_string = excali_core::encode::to_byte_string(&bytes);
    format!(
        "data:image/png;base64,{}",
        excali_core::encode::btoa(&byte_string).unwrap()
    )
}

/// `files` (`export.test.tsx:199-207`).
fn files() -> Map<String, Value> {
    let files = json!({
        "file_A": {
            "id": "file_A",
            "dataURL": deer_data_url(),
            "mimeType": "image/png",
            "created": 1,
            "lastRetrieved": 1
        }
    });
    files.as_object().unwrap().clone()
}

fn transformed_images_export() -> Tag {
    export(
        &transformed_images(),
        &app_state(json!({ "exportBackground": false })),
        &files(),
    )
}

#[test]
fn exporting_svg_containing_transformed_images() {
    let svg_text = transformed_images_export().outer_html();
    // expect 1 <image> element (deduped)
    assert_eq!(svg_text.matches("<image").count(), 1);
    // expect 4 <use> elements (one for each excalidraw image element)
    assert_eq!(svg_text.matches("<use").count(), 4);
    check(TRANSFORMED, &format!("\n\"{svg_text}\"\n"));
}

#[test]
fn every_snapshot_of_the_file_is_checked() {
    let mut names: Vec<String> = snapshots().into_keys().collect();
    names.sort();
    assert_eq!(names, [EMBEDDED, TRANSFORMED]);
}
