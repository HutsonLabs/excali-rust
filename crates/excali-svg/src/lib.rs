//! SVG export of a display list.
//!
//! Upstream counterpart: `renderer/staticSvgScene.ts`, `exportToSvg`
//! (`packages/excalidraw/scene/export.ts:293-508`).
//!
//! [`export_to_svg`] writes the document `exportToSvg` builds, from the
//! [`SvgDocument`] the scene computes (`excali_scene::export::svg_document`):
//!
//! ```text
//! <svg version="1.1" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 W H" width="W*s" height="H*s">
//!   <!-- svg-source:excalidraw -->
//!   <metadata>[payload comments and base64]</metadata>
//!   <defs>[<symbol id=image-…>…][<clipPath id=frame>…]<style class="style-fonts">[@font-face rules]</style></defs>
//!   [<rect x="0" y="0" width="W" height="H" fill="background">]
//!   [the elements: <g>, <text>, <use>, <mask>, <clipPath>, <a>, …]
//! </svg>
//! ```
//!
//! as [`dom`] nodes whose [`dom::Tag::outer_html`] is the markup upstream
//! saves. The elements are the drawing `renderSceneToSvg` adds
//! (`renderer/staticSvgScene.ts`, [`SvgDocument::nodes`] and the image
//! symbols it puts first in `<defs>`, [`SvgDocument::symbols`]), which the
//! scene hands over as [`SvgNode`] trees. Numbers print as JavaScript
//! prints them ([`number`]); rough.js path data has two decimals
//! ([`path`], [`number::MAX_DECIMALS_FOR_SVG_EXPORT`]).
//!
//! Targets: native, wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-scene`,
//! reached only through `excali_scene::display`.

pub mod dom;
mod fonts;
pub mod number;
pub mod path;

pub use fonts::{base64, subset_woff2, FontContent, FontFiles, SubsetError};

use excali_scene::display::{SvgDocument, SvgNode, SvgValue};

use dom::{Node, Tag};
use number::js;

/// `SVG_NS` (`packages/common/src/constants.ts:409`).
pub const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// `SVG_DOCUMENT_PREAMBLE` (`packages/common/src/constants.ts:410-412`):
/// what a saved `.svg` file starts with, "so that older software parse the
/// SVG file properly" (`data/index.ts:141-143`).
pub const SVG_DOCUMENT_PREAMBLE: &str = "<?xml version=\"1.0\" standalone=\"no\"?>\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n";

/// The six spaces between `@font-face` rules (`export.ts:447`).
const FONT_FACE_DELIMITER: &str = "\n      ";

/// `createHTMLComment(text)` (`export.ts:286-291`): "surrounding with
/// spaces to maintain prettified consistency with previous iterations".
fn comment(text: &str) -> Node {
    Node::Comment(format!(" {text} "))
}

/// The root `exportToSvg` returns for `document`, before the elements:
/// the root's size, the `svg-source` comment, `<metadata>` with the
/// embedded scene, `<defs>` with the frame clip paths and the
/// `style-fonts` block (each face's `url()` from `fonts`, repeated rules
/// written once), and the background rectangle.
pub fn export_to_svg(document: &SvgDocument, fonts: &dyn FontContent) -> Tag {
    let mut root = Tag::new("svg");
    root.set_attribute("version", "1.1");
    root.set_attribute("xmlns", SVG_NS);
    root.set_attribute(
        "viewBox",
        format!("0 0 {} {}", js(document.width), js(document.height)),
    );
    root.set_attribute("width", js(document.width * document.scale));
    root.set_attribute("height", js(document.height * document.scale));

    root.append(comment("svg-source:excalidraw"));

    let mut metadata = Tag::new("metadata");
    if let Some(payload) = &document.payload {
        metadata.append(comment(&format!("payload-type:{}", payload.mime_type)));
        metadata.append(comment(&format!("payload-version:{}", payload.version)));
        metadata.append(comment("payload-start"));
        metadata.append(Node::Text(payload.base64.clone()));
        metadata.append(comment("payload-end"));
    }
    root.append(metadata);

    let mut defs = Tag::new("defs");
    for symbol in &document.symbols {
        defs.append(node(symbol));
    }
    for clip in &document.frame_clips {
        let mut clip_path = Tag::new("clipPath");
        clip_path.set_attribute("id", clip.id.as_str());
        let mut rect = Tag::new("rect");
        rect.set_attribute(
            "transform",
            format!(
                "translate({} {}) rotate({} {} {})",
                js(clip.x),
                js(clip.y),
                js(clip.angle),
                js(clip.cx),
                js(clip.cy)
            ),
        );
        rect.set_attribute("width", js(clip.width));
        rect.set_attribute("height", js(clip.height));
        if let Some(radius) = clip.radius {
            rect.set_attribute("rx", js(radius));
            rect.set_attribute("ry", js(radius));
        }
        clip_path.append(rect);
        defs.append(clip_path);
    }

    // `Array.from(new Set(fontFaces))`: the rules once each, in order.
    let mut rules: Vec<String> = Vec::new();
    for face in &document.font_faces {
        let rule = format!(
            "@font-face {{ font-family: {}; src: url({}); }}",
            face.family,
            fonts.content(face)
        );
        if !rules.contains(&rule) {
            rules.push(rule);
        }
    }
    let mut style = Tag::new("style");
    style.add_class("style-fonts");
    style.append(Node::Text(format!(
        "{FONT_FACE_DELIMITER}{}",
        rules.join(FONT_FACE_DELIMITER)
    )));
    defs.append(style);
    root.append(defs);

    if let Some(fill) = &document.background {
        let mut rect = Tag::new("rect");
        rect.set_attribute("x", "0");
        rect.set_attribute("y", "0");
        rect.set_attribute("width", js(document.width));
        rect.set_attribute("height", js(document.height));
        rect.set_attribute("fill", fill.as_str());
        root.append(rect);
    }

    for n in &document.nodes {
        root.append(node(n));
    }

    root
}

/// A node of the drawing as a DOM node: each value printed as upstream's
/// `setAttribute` stringifies it.
fn node(n: &SvgNode) -> Node {
    match n {
        SvgNode::Text(text) => Node::Text(text.clone()),
        SvgNode::Tag(t) => {
            let mut tag = Tag::new(t.name);
            for (name, value) in &t.attributes {
                tag.set_attribute(*name, attribute(value));
            }
            for child in &t.children {
                tag.append(node(child));
            }
            Node::Tag(tag)
        }
    }
}

fn attribute(value: &SvgValue) -> String {
    match value {
        SvgValue::Text(s) => s.clone(),
        SvgValue::Number(n) => js(*n),
        SvgValue::RoughPath { path, decimals } => {
            path::rough_path_data(path, *decimals).unwrap_or_default()
        }
    }
}

/// A `.svg` file's text: [`SVG_DOCUMENT_PREAMBLE`] and the root's
/// `outerHTML` (`data/index.ts:139-146`).
pub fn to_svg_file(root: &Tag) -> String {
    format!("{SVG_DOCUMENT_PREAMBLE}{}", root.outer_html())
}
