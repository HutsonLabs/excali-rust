//! ex-407's acceptance: upstream's own snapshot of `exportToSvg`,
//! `packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap`
//! (vendored under `fixtures/upstream/`), reproduced by the port byte for
//! byte after whitespace normalisation.
//!
//! The four snapshots of `tests/scene/export.test.ts` are rebuilt from the
//! elements upstream's test exports (`tools/goldens/svg-export.mjs` records
//! them in `tests/fixtures/svg-export.json`: scenes `fixture`,
//! `fixture-cjk`, `fixture-embed` and `link`), with the test's
//! `DEFAULT_OPTIONS` and upstream's test environment: the page is
//! `http://localhost:3000` (the embedded scene's `source`), every node
//! carries its `data-id`, and the two `toMatchSnapshot()` of an element are
//! printed as vitest's pretty-format prints a DOM element (attributes
//! sorted, one per line, empty elements self-closed), the two of
//! `innerHTML` as a string.
//!
//! The one input the port does not compute is the font faces: upstream's
//! test `FontFace` gives every face the range `U+0000-00FF`
//! (`setupTests.ts:65-86`), so it inlines each face of a family used (what
//! the browser's ranges select is `svg-export.json`'s business, and
//! `tests/document.rs` holds the port to it), and it subsets them with
//! HarfBuzz, whose bytes the port's subsetter does not reproduce (ADR-010).
//! The faces and their data are upstream's, as `tools/goldens/
//! font-subset.mjs` recorded them under that `FontFace` in
//! `tools/font-subset-eval/upstream-subsets.json` (scenes `export-test-*`),
//! and the test checks that the port's own faces are among them, family by
//! family in the same order.

mod support;

use std::collections::HashMap;

use excali_scene::display::{FontFaceSource, SvgDocument};
use excali_scene::export::{svg_document, SvgExportAppState, SvgExportOptions};
use excali_svg::dom::{Node, Tag};
use excali_svg::{export_to_svg, FontContent};
use serde_json::{json, Value};
use support::{elements, fixture, TenPxPerCodeUnit};

/// `window.location.origin` in upstream's vitest environment.
const TEST_ORIGIN: &str = "http://localhost:3000";

fn read(path: &str) -> String {
    std::fs::read_to_string(format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The snapshots of `export.test.ts.snap` by name.
fn snapshots() -> HashMap<String, String> {
    support::parse_snapshots(&read(
        "fixtures/upstream/packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap",
    ))
}

use support::normalize;

// -- pretty-format ---------------------------------------------------------------

/// `escapeHTML` (`pretty-format`, `plugins/lib/escapeHTML`).
fn escape_html(s: &str) -> String {
    s.replace('<', "&lt;").replace('>', "&gt;")
}

/// pretty-format's `DOMElement` plugin (`plugins/DOMElement.ts` and
/// `plugins/lib/markup.ts`) with vitest's snapshot config: indent of two,
/// no colours, no string escaping.
fn pretty(node: &Node, indentation: &str) -> String {
    match node {
        Node::Text(data) => escape_html(data),
        Node::Comment(data) => format!("<!--{}-->", escape_html(data)),
        Node::Tag(tag) => pretty_tag(tag, indentation),
    }
}

fn pretty_tag(tag: &Tag, indentation: &str) -> String {
    let inner = format!("{indentation}  ");
    let name = tag.name.to_lowercase();
    let mut attributes: Vec<&(String, String)> = tag.attributes().iter().collect();
    attributes.sort_by(|a, b| a.0.cmp(&b.0));
    let props: String = attributes
        .iter()
        .map(|(k, v)| format!("\n{inner}{k}=\"{v}\""))
        .collect();
    let children: String = tag
        .children()
        .iter()
        .map(|c| format!("\n{inner}{}", pretty(c, &inner)))
        .collect();
    let mut out = format!("<{name}");
    if !props.is_empty() {
        out.push_str(&props);
        out.push('\n');
        out.push_str(indentation);
    }
    if !children.is_empty() {
        out.push('>');
        out.push_str(&children);
        out.push('\n');
        out.push_str(indentation);
        out.push_str("</");
        out.push_str(&name);
    } else {
        out.push_str(if props.is_empty() { " /" } else { "/" });
    }
    out.push('>');
    out
}

/// `expect(svgElement).toMatchSnapshot()`.
fn element_snapshot(root: &Tag) -> String {
    format!("\n{}\n", pretty_tag(root, ""))
}

/// `expect(svgElement.innerHTML).toMatchSnapshot()`.
fn inner_html_snapshot(root: &Tag) -> String {
    format!("\n\"{}\"\n", root.inner_html())
}

// -- upstream's font faces ---------------------------------------------------------

/// The faces upstream's test inlined and their subsets.
struct UpstreamFaces {
    faces: Vec<FontFaceSource>,
    woff2: HashMap<(String, String), String>,
}

impl FontContent for UpstreamFaces {
    fn content(&self, face: &FontFaceSource) -> String {
        let woff2 = &self.woff2[&(face.file.clone(), face.characters.clone())];
        format!("data:font/woff2;base64,{woff2}")
    }
}

fn upstream_faces(scene: &str) -> UpstreamFaces {
    let subsets: Value =
        serde_json::from_str(&read("tools/font-subset-eval/upstream-subsets.json")).unwrap();
    let scene = subsets["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == scene)
        .unwrap();
    assert_eq!(scene["fontFace"], "vitest");
    let mut faces = Vec::new();
    let mut woff2 = HashMap::new();
    for d in scene["declarations"].as_array().unwrap() {
        let file = d["file"].as_str().unwrap().to_owned();
        let characters: String = d["codePoints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| char::from_u32(c.as_u64().unwrap() as u32).unwrap())
            .collect();
        woff2.insert(
            (file.clone(), characters.clone()),
            d["woff2"].as_str().unwrap().to_owned(),
        );
        faces.push(FontFaceSource {
            family: d["family"].as_str().unwrap().to_owned(),
            file: file.clone(),
            upstream_file: file,
            format: "woff2",
            characters,
            fallback_url: String::new(),
        });
    }
    UpstreamFaces { faces, woff2 }
}

/// Upstream's faces in place of the port's, after checking that the
/// port's are among them (by family, file and characters), families in the
/// same order.
fn with_upstream_faces(mut document: SvgDocument, faces: &UpstreamFaces) -> SvgDocument {
    let families = |f: &[FontFaceSource]| {
        let mut out: Vec<String> = Vec::new();
        for face in f {
            if !out.contains(&face.family) {
                out.push(face.family.clone());
            }
        }
        out
    };
    assert_eq!(families(&document.font_faces), families(&faces.faces));
    for ours in &document.font_faces {
        assert!(
            faces.faces.iter().any(|f| f.family == ours.family
                && f.file == ours.upstream_file
                && f.characters == ours.characters),
            "{} {} is not among upstream's faces",
            ours.family,
            ours.file
        );
    }
    document.font_faces = faces.faces.clone();
    document
}

// -- the export test ----------------------------------------------------------------

/// `exportUtils.exportToSvg(elements, DEFAULT_OPTIONS, null)` for a scene
/// of `svg-export.json`, with `appState` extended as the test's is.
fn export_test(scene_name: &str, app_state: Value, faces: Option<&str>) -> Tag {
    let fixture = fixture();
    let scene = fixture["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == scene_name)
        .unwrap();
    let elements = elements(scene);
    let app_state = SvgExportAppState::from_app_state(app_state.as_object().unwrap());
    let options = SvgExportOptions {
        data_ids: true,
        ..SvgExportOptions::new(TEST_ORIGIN, &TenPxPerCodeUnit)
    };
    let document = svg_document(&elements, &app_state, None, &options);
    match faces {
        Some(name) => {
            let faces = upstream_faces(name);
            export_to_svg(&with_upstream_faces(document, &faces), &faces)
        }
        None => {
            assert!(document.font_faces.is_empty());
            export_to_svg(&document, &upstream_faces("export-test-default"))
        }
    }
}

/// `DEFAULT_OPTIONS` (`export.test.ts:64-68`).
fn default_options() -> Value {
    json!({ "exportBackground": false, "viewBackgroundColor": "#ffffff", "files": {} })
}

fn check(name: &str, got: &str) {
    let snapshots = snapshots();
    let expected = &snapshots[name];
    let (g, e) = (normalize(got), normalize(expected));
    if g != e {
        panic!("{name}: {}", support::first_difference(&g, &e));
    }
}

#[test]
fn with_default_arguments() {
    let root = export_test("fixture", default_options(), Some("export-test-default"));
    check(
        "exportToSvg > with default arguments 1",
        &element_snapshot(&root),
    );
}

#[test]
fn with_a_cjk_font() {
    let root = export_test("fixture-cjk", default_options(), Some("export-test-cjk"));
    check("exportToSvg > with a CJK font 1", &element_snapshot(&root));
}

#[test]
fn with_export_embed_scene() {
    let mut app_state = default_options();
    app_state["exportEmbedScene"] = json!(true);
    let root = export_test("fixture-embed", app_state, Some("export-test-default"));
    check(
        "exportToSvg > with exportEmbedScene 1",
        &inner_html_snapshot(&root),
    );
}

#[test]
fn with_elements_that_have_a_link() {
    let root = export_test("link", default_options(), None);
    check(
        "exportToSvg > with elements that have a link 1",
        &inner_html_snapshot(&root),
    );
}

#[test]
fn every_snapshot_of_the_file_is_checked() {
    let mut names: Vec<String> = snapshots().into_keys().collect();
    names.sort();
    assert_eq!(
        names,
        [
            "exportToSvg > with a CJK font 1",
            "exportToSvg > with default arguments 1",
            "exportToSvg > with elements that have a link 1",
            "exportToSvg > with exportEmbedScene 1",
        ]
    );
}

#[test]
fn pretty_format_prints_as_vitest_does() {
    let mut root = Tag::new("svg");
    root.set_attribute("width", "1");
    root.set_attribute("height", "2");
    root.append(Node::Comment(" c ".into()));
    root.append(Tag::new("metadata"));
    let mut text = Tag::new("text");
    text.set_attribute("x", "0");
    text.append(Node::Text("a < b".into()));
    root.append(text);
    let mut path = Tag::new("path");
    path.set_attribute("d", "M0 0");
    root.append(path);
    assert_eq!(
        element_snapshot(&root),
        "\n<svg\n  height=\"2\"\n  width=\"1\"\n>\n  <!-- c -->\n  <metadata />\n  <text\n    x=\"0\"\n  >\n    a &lt; b\n  </text>\n  <path\n    d=\"M0 0\"\n  />\n</svg>\n"
    );
}
