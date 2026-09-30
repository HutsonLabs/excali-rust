//! The elements of an SVG export (ex-407): what upstream's
//! `renderSceneToSvg` (`packages/excalidraw/renderer/staticSvgScene.ts`)
//! adds to the document `exportToSvg` builds, written by the port.
//!
//! Every scene of `tests/fixtures/svg-export.json` (upstream's own
//! `exportToSvg` at the pinned commit under jsdom 22.1.0,
//! `tools/goldens/svg-export.mjs`) is reproduced whole, byte for byte:
//! rough.js paths at two decimals in a group per element, lines and arrows
//! with their label masks, freedraw outlines filled with the stroke colour,
//! one `<text>` per line, image `<symbol>`s and `<use>`s with crops, flips
//! and rounded clips, frame outlines and clips, embeddables as links or
//! `<foreignObject>`s, and links as anchors. Upstream ran in test mode,
//! so each element's node carries its `data-id`. Sticky notes (ex-703)
//! with their outline clip, shadow, edge and date footer, the clock pinned
//! as upstream's was (`now`, UTC).

mod support;

use excali_scene::display::{SvgDocument, SvgNode, SvgTag};
use excali_svg::export_to_svg;
use serde_json::Value;
use support::{document, expected_document, first_difference, fixture, Marker};

fn scene<'a>(fixture: &'a Value, name: &str) -> &'a Value {
    fixture["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap_or_else(|| panic!("no scene {name}"))
}

fn export(scene: &Value, source: &str) -> String {
    export_to_svg(&document(scene, source, true), &Marker).outer_html()
}

#[test]
fn every_scene_is_upstreams_document() {
    let fixture = fixture();
    let source = fixture["source"].as_str().unwrap();
    let scenes = fixture["scenes"].as_array().unwrap();
    assert!(scenes.len() >= 48);
    let mut failures = Vec::new();
    for scene in scenes {
        let expected = expected_document(scene);
        let got = export(scene, source);
        if got != expected {
            failures.push(format!(
                "{}: {}",
                scene["name"],
                first_difference(&got, &expected)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} scenes differ:\n{}",
        failures.len(),
        scenes.len(),
        failures.join("\n")
    );
}

#[test]
fn the_scenes_cover_every_kind_of_node() {
    // what the documents hold, so a scene dropped from the generator is
    // noticed
    let fixture = fixture();
    let all: String = fixture["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["document"].as_str().unwrap().to_owned())
        .collect();
    for needle in [
        "<symbol id=\"image-",
        "<use href=\"#image-",
        "<mask id=\"mask-image-crop-",
        "<clipPath id=\"image-clipPath-",
        "<mask id=\"mask-",
        "maskUnits=\"userSpaceOnUse\"",
        "<mask></mask>",
        "fill-rule=\"evenodd\"",
        "stroke-dasharray=\"",
        "stroke-opacity=\"",
        "<text x=",
        "direction=\"rtl\"",
        "text-anchor=\"middle\"",
        "text-anchor=\"end\"",
        "clip-path=\"url(#",
        "<a href=",
        "<foreignObject style=",
        "<iframe src=",
        "target=\"_blank\"",
        "filter=\"invert(93%) hue-rotate(180deg)\"",
        "stroke=\"none\" data-id=",
        "rx=\"8\" ry=\"8\" fill=\"none\" stroke=\"#bbb\"",
        "data-id=\"id0\"",
    ] {
        assert!(all.contains(needle), "no scene has {needle}");
    }
}

#[test]
fn without_data_ids_the_nodes_carry_none() {
    // outside upstream's test mode no node has a data-id
    let fixture = fixture();
    let source = fixture["source"].as_str().unwrap();
    for name in ["links", "frame-children", "images", "embeds"] {
        let scene = scene(&fixture, name);
        let got = export_to_svg(&document(scene, source, false), &Marker).outer_html();
        let expected = expected_document(scene);
        assert!(!got.contains("data-id"), "{name}");
        assert_eq!(got, strip_data_ids(&expected), "{name}");
    }
}

/// The document without its ` data-id="…"` attributes: upstream sets
/// `data-id` last, and element ids never hold a `"`.
fn strip_data_ids(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut rest = markup;
    while let Some(at) = rest.find(" data-id=\"") {
        out.push_str(&rest[..at]);
        let after = &rest[at + " data-id=\"".len()..];
        rest = &after[after.find('"').unwrap() + 1..];
    }
    out.push_str(rest);
    out
}

#[test]
fn symbols_go_first_in_defs_in_reverse_order_of_use() {
    let fixture = fixture();
    let source = fixture["source"].as_str().unwrap();
    let doc = document(scene(&fixture, "images"), source, true);
    let ids: Vec<String> = doc
        .symbols
        .iter()
        .map(|s| match s {
            SvgNode::Tag(t) => attribute_text(t, "id"),
            SvgNode::Text(_) => panic!("a symbol is a tag"),
        })
        .collect();
    assert_eq!(
        ids,
        [
            "image-a.b",
            "image-a.b",
            "image-svg",
            "image-png-2",
            "image-crop-png-2-2108472278",
            "image-crop-png-198863462",
            "image-crop-png-2108472278",
            "image-png",
        ]
    );
}

fn attribute_text(tag: &SvgTag, name: &str) -> String {
    match tag.get(name) {
        Some(excali_scene::display::SvgValue::Text(s)) => s.clone(),
        other => panic!("{name}: {other:?}"),
    }
}

#[test]
fn a_rendered_embeddable_has_no_border() {
    // renderEmbeddables: the iframe of a YouTube link, rounded, with
    // `border: none` kept as Chrome keeps it
    let fixture = fixture();
    let source = fixture["source"].as_str().unwrap();
    let got = export(scene(&fixture, "embeds-rendered"), source);
    assert!(got.contains(
        "<foreignObject style=\"width: 320px; height: 180px; border-width: medium; \
         border-style: none; border-color: currentcolor; border-image: none;\">\
         <div xmlns=\"http://www.w3.org/1999/xhtml\" style=\"width: 100%; height: 100%;\">\
         <iframe src=\"https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1&amp;start=90\" \
         style=\"width: 100%; height: 100%; border-width: medium; border-style: none; \
         border-color: currentcolor; border-image: none; border-radius: 32px; top: 0px; \
         left: 0px;\" allowfullscreen=\"\"></iframe></div></foreignObject>"
    ));
    // upstream's recorded document (jsdom) drops the declaration
    let recorded = scene(&fixture, "embeds-rendered")["document"]
        .as_str()
        .unwrap();
    assert!(!recorded.contains("border-style"));
    assert!(recorded.contains("<foreignObject style=\"width: 320px; height: 180px;\">"));
}

#[test]
fn an_empty_scene_draws_nothing() {
    let fixture = fixture();
    let source = fixture["source"].as_str().unwrap();
    let doc: SvgDocument = document(scene(&fixture, "empty"), source, true);
    assert!(doc.nodes.is_empty());
    assert!(doc.symbols.is_empty());
}
