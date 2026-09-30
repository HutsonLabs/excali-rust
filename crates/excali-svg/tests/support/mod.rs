//! Running `exportToSvg`'s port on the scenes of
//! `tests/fixtures/svg-export.json` (upstream's own `exportToSvg` at the
//! pinned commit under jsdom 22.1.0, `tools/goldens/svg-export.mjs`).
//!
//! As in the generator, text measures 10 px per UTF-16 code unit, and a
//! font face's content is `font:<url>#<characters>`, the face's last url
//! (its file under upstream's asset fallback) and the characters it was
//! asked to keep. Upstream ran in test mode, so the documents carry
//! `data-id`s ([`SvgExportOptions::data_ids`]).

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::OnceLock;

use excali_core::element::Element;
use excali_scene::display::{FontFaceSource, SvgDocument};
use excali_scene::export::{svg_document, SvgExportAppState, SvgExportOptions, TextMetrics};
use excali_scene::sticky_note::Clock;
use excali_svg::FontContent;
use serde_json::{Map, Value};

pub fn fixture() -> Value {
    serde_json::from_str(include_str!("../fixtures/svg-export.json")).unwrap()
}

/// `Date.now()` while upstream exported, in UTC (the generator pins both):
/// what sticky note footers read.
pub fn now() -> f64 {
    static NOW: OnceLock<f64> = OnceLock::new();
    *NOW.get_or_init(|| fixture()["now"].as_f64().unwrap())
}

pub struct TenPxPerCodeUnit;

impl TextMetrics for TenPxPerCodeUnit {
    fn measure(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

/// The fixture's font content: the face's last url in upstream (the url
/// `getContent` answers when it cannot fetch the file) and the characters
/// it was asked to keep.
pub struct Marker;

impl FontContent for Marker {
    fn content(&self, face: &FontFaceSource) -> String {
        format!("font:{}#{}", face.fallback_url, face.characters)
    }
}

pub fn elements(scene: &Value) -> Vec<Element> {
    scene["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
        .collect()
}

/// `exportToSvg(elements, appState, files, opts)` for a fixture scene, as
/// the document the scene computes; `source` is also the page's origin.
pub fn document(scene: &Value, source: &str, data_ids: bool) -> SvgDocument {
    let elements = elements(scene);
    let app_state = SvgExportAppState::from_app_state(scene["appState"].as_object().unwrap());
    let files: Option<&Map<String, Value>> = scene["files"].as_object();
    let opts = &scene["opts"];
    let exporting_frame = opts["exportingFrame"]
        .as_str()
        .map(|id| elements.iter().find(|e| e.base.id == id).unwrap());
    let options = SvgExportOptions {
        exporting_frame,
        skip_inlining_fonts: opts["skipInliningFonts"].as_bool().unwrap_or(false),
        render_embeddables: opts["renderEmbeddables"].as_bool().unwrap_or(false),
        reuse_images: opts["reuseImages"].as_bool().unwrap_or(true),
        data_ids,
        clock: Clock::utc(now()),
        ..SvgExportOptions::new(source, &TenPxPerCodeUnit)
    };
    svg_document(&elements, &app_state, files, &options)
}

/// The document's shell: what `exportToSvg` has built when it starts
/// rendering the elements.
pub fn shell(document: &SvgDocument) -> SvgDocument {
    SvgDocument {
        symbols: Vec::new(),
        nodes: Vec::new(),
        ..document.clone()
    }
}

/// Upstream's whole document for a scene, with an embeddable's
/// `border: none` as a browser keeps it ([`chrome_border`]).
pub fn expected_document(scene: &Value) -> String {
    chrome_border(scene["document"].as_str().unwrap())
}

/// `style.border = "none"` as Chrome 153 serializes it in the `style`
/// attribute (a headless Chrome run of the same assignments,
/// `chrome-headless-shell` 1243, 2026-09-28): its four longhands where the
/// declaration was set. Upstream's recorded documents come from jsdom
/// 22.1.0, whose cssstyle drops the declaration: the `<foreignObject>` and
/// the `<iframe>` of a rendered embeddable (`staticSvgScene.ts:380-392`).
pub const CHROME_BORDER_NONE: &str =
    "border-width: medium; border-style: none; border-color: currentcolor; border-image: none;";

/// The jsdom document with [`CHROME_BORDER_NONE`] where upstream sets
/// `border: none`: after the `<foreignObject>`'s width and height, and
/// after the `<iframe>`'s width and height.
pub fn chrome_border(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut rest = markup;
    while let Some(at) = rest.find("<foreignObject style=\"") {
        let start = at + "<foreignObject style=\"".len();
        let end = start + rest[start..].find('"').unwrap();
        out.push_str(&rest[..end]);
        out.push(' ');
        out.push_str(CHROME_BORDER_NONE);
        rest = &rest[end..];
        let iframe = "height: 100%; border-radius:";
        let at = rest.find(iframe).unwrap() + "height: 100%;".len();
        out.push_str(&rest[..at]);
        out.push(' ');
        out.push_str(CHROME_BORDER_NONE);
        rest = &rest[at..];
    }
    out.push_str(rest);
    out
}

/// The snapshots of a vitest `.snap` file by name, as vitest wrote them
/// (between the backticks, `\\`, `` ` `` and `${` unescaped).
pub fn parse_snapshots(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut rest = text;
    while let Some(start) = rest.find("exports[`") {
        rest = &rest[start + "exports[`".len()..];
        let name_end = rest.find("`] = `").unwrap();
        let name = rest[..name_end].to_owned();
        rest = &rest[name_end + "`] = `".len()..];
        let end = rest.find("\n`;").unwrap() + 1;
        let body = rest[..end]
            .replace("\\`", "`")
            .replace("\\${", "${")
            .replace("\\\\", "\\");
        rest = &rest[end..];
        out.insert(name, body);
    }
    out
}

/// Every run of whitespace as one space, none at the ends: how the
/// snapshot tests compare documents.
pub fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Where two strings first differ, with some context.
pub fn first_difference(got: &str, expected: &str) -> String {
    let at = got
        .char_indices()
        .zip(expected.chars())
        .find(|((_, a), b)| a != b)
        .map(|((i, _), _)| i)
        .unwrap_or_else(|| got.len().min(expected.len()));
    let start = at.saturating_sub(200);
    let start = (0..=start)
        .rev()
        .find(|i| got.is_char_boundary(*i) && expected.is_char_boundary(*i))
        .unwrap_or(0);
    let end = |s: &str| {
        let mut e = (at + 300).min(s.len());
        while !s.is_char_boundary(e) {
            e -= 1;
        }
        e
    };
    format!(
        "at byte {at}:\n  got      …{}\n  expected …{}",
        &got[start..end(got)],
        &expected[start..end(expected)]
    )
}
