//! The pieces of the SVG writer: the DOM and its `outerHTML` serialization,
//! JavaScript number formatting, rough.js path data with two decimals, and
//! the font face content.

use excali_core::element::Element;
use excali_rough::RoughGenerator;
use excali_scene::display::{DisplayItem, FontFaceSource, Path};
use excali_scene::display::{LineCap, LineJoin};
use excali_scene::rough_canvas;
use excali_scene::shape::{generate_element_shape, RenderConfig};
use excali_svg::dom::{Node, Tag};
use excali_svg::number::{fixed, js, MAX_DECIMALS_FOR_SVG_EXPORT};
use excali_svg::path::rough_path_data;
use excali_svg::{base64, FontContent, FontFiles, SVG_NS};
use serde_json::Value;

// -- dom ------------------------------------------------------------------------

#[test]
fn tags_serialize_as_outer_html() {
    let mut root = Tag::new("svg");
    root.set_attribute("version", "1.1");
    root.set_attribute("xmlns", SVG_NS);
    root.append(Node::Comment(" svg-source:excalidraw ".into()));
    root.append(Tag::new("metadata"));
    let mut defs = Tag::new("defs");
    let mut style = Tag::new("style");
    style.add_class("style-fonts");
    style.append(Node::Text("\n      ".into()));
    defs.append(style);
    root.append(defs);
    assert_eq!(
        root.outer_html(),
        "<svg version=\"1.1\" xmlns=\"http://www.w3.org/2000/svg\"><!-- svg-source:excalidraw -->\
         <metadata></metadata><defs><style class=\"style-fonts\">\n      </style></defs></svg>"
    );
    assert_eq!(
        root.inner_html(),
        "<!-- svg-source:excalidraw --><metadata></metadata><defs><style class=\"style-fonts\">\n      </style></defs>"
    );
}

#[test]
fn set_attribute_replaces_in_place() {
    let mut rect = Tag::new("rect");
    rect.set_attribute("x", "0");
    rect.set_attribute("y", "0");
    rect.set_attribute("x", "5");
    assert_eq!(rect.attribute("x"), Some("5"));
    assert_eq!(rect.attribute("width"), None);
    assert_eq!(rect.outer_html(), "<rect x=\"5\" y=\"0\"></rect>");
}

#[test]
fn class_list_add_keeps_one_of_each() {
    let mut style = Tag::new("style");
    style.add_class("style-fonts");
    style.add_class("style-fonts");
    style.add_class("other");
    assert_eq!(style.attribute("class"), Some("style-fonts other"));
}

#[test]
fn escaping_follows_the_html_serializer() {
    // attribute values: & " and no-break space; text: & < > and no-break
    // space; comments as they are (the HTML fragment serialization
    // algorithm, which jsdom's outerHTML implements)
    let mut rect = Tag::new("rect");
    rect.set_attribute("fill", "a\"b<c>&d\u{a0}e'f");
    rect.append(Node::Text("x & <y> \u{a0}\"'".into()));
    rect.append(Node::Comment(" a & <b> ".into()));
    assert_eq!(
        rect.outer_html(),
        "<rect fill=\"a&quot;b<c>&amp;d&nbsp;e'f\">x &amp; &lt;y&gt; &nbsp;\"'<!-- a & <b> --></rect>"
    );
}

// -- numbers --------------------------------------------------------------------

#[test]
fn numbers_are_written_as_javascript_writes_them() {
    for (x, s) in [
        (0.0, "0"),
        (-0.0, "0"),
        (120.0, "120"),
        (17.619999999999997, "17.619999999999997"),
        (0.1 + 0.2, "0.30000000000000004"),
        (47.9329999, "47.9329999"),
        (1e21, "1e+21"),
        (1e-7, "1e-7"),
        (-2.5, "-2.5"),
        (f64::NAN, "NaN"),
        (f64::INFINITY, "Infinity"),
    ] {
        assert_eq!(js(x), s, "{x}");
    }
}

#[test]
fn two_decimal_numbers_round_as_to_fixed() {
    assert_eq!(MAX_DECIMALS_FOR_SVG_EXPORT, 2);
    // (x).toFixed(2), read back as a number, in V8
    for (x, y) in [
        (50.625_f64, 50.63_f64),
        (1.005, 1.0),
        (0.32000000000000006, 0.32),
        (-0.005, -0.01),
        (-0.0049, -0.0),
        (2.675, 2.67),
        (99.995, 100.0),
        (1e21, 1e21),
    ] {
        let got = fixed(x, MAX_DECIMALS_FOR_SVG_EXPORT);
        assert_eq!(got, y, "{x}");
        assert_eq!(got.is_sign_negative(), y.is_sign_negative(), "{x}");
    }
    assert!(fixed(f64::NAN, 2).is_nan());
    assert_eq!(js(fixed(-0.0049, 2)), "0");
}

// -- path data ------------------------------------------------------------------

#[test]
fn path_data_is_rough_js_ops_to_path() {
    let mut path = Path::new();
    path.move_to(0.324, 50.625)
        .cubic_to(1.0, 2.0, 3.0, 4.0, 5.555, -6.0)
        .line_to(7.0, 8.0);
    assert_eq!(
        rough_path_data(&path, Some(2)).as_deref(),
        Some("M0.32 50.63 C1 2, 3 4, 5.55 -6 L7 8")
    );
    assert_eq!(
        rough_path_data(&path, None).as_deref(),
        Some("M0.324 50.625 C1 2, 3 4, 5.555 -6 L7 8")
    );
    assert_eq!(rough_path_data(&Path::new(), Some(2)).as_deref(), Some(""));
    let mut closed = Path::new();
    closed.move_to(0.0, 0.0).close();
    assert_eq!(rough_path_data(&closed, Some(2)), None);
}

/// The rough.js paths upstream's export of its test scene writes: the
/// diamond's and the ellipse's hachure and outline, with two decimals
/// (`staticSvgScene.ts:60-74`, `MAX_DECIMALS_FOR_SVG_EXPORT`).
#[test]
fn the_upstream_test_scene_paths_have_two_decimals() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/svg-export.json")).unwrap();
    let scene = &fixture["scenes"][0];
    assert_eq!(scene["name"], "fixture");
    let expected = scene["paths"].as_array().unwrap();
    let generator = RoughGenerator::new();
    let config = RenderConfig {
        is_exporting: true,
        ..RenderConfig::default()
    };
    let mut got = Vec::new();
    for e in &scene["elements"].as_array().unwrap()[..2] {
        let element = Element::from_map(e.as_object().unwrap().clone()).unwrap();
        let drawable = generate_element_shape(&element, &generator, &config).unwrap();
        for item in rough_canvas::draw(&drawable, LineCap::Round, LineJoin::Round).unwrap() {
            let DisplayItem::Stroke { path, stroke } = item else {
                panic!("the fixture's shapes are strokes")
            };
            got.push((
                rough_path_data(&path, Some(MAX_DECIMALS_FOR_SVG_EXPORT)).unwrap(),
                stroke.color.as_str().to_owned(),
                js(stroke.width),
            ));
        }
    }
    assert_eq!(got.len(), expected.len());
    for (g, e) in got.iter().zip(expected) {
        assert_eq!(g.0, e["d"].as_str().unwrap());
        assert_eq!(g.1, e["stroke"].as_str().unwrap());
        assert_eq!(g.2, e["strokeWidth"].as_str().unwrap());
        assert_eq!(e["fill"], "none");
    }
}

// -- fonts ----------------------------------------------------------------------

#[test]
fn base64_is_rfc_4648() {
    for (bytes, s) in [
        (&b""[..], ""),
        (b"f", "Zg=="),
        (b"fo", "Zm8="),
        (b"foo", "Zm9v"),
        (b"foob", "Zm9vYg=="),
        (b"fooba", "Zm9vYmE="),
        (b"foobar", "Zm9vYmFy"),
        (&[0xff, 0xfe, 0x00, 0x3e, 0x3f], "//4APj8="),
    ] {
        assert_eq!(base64(bytes), s);
    }
}

const FONTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../excali-text/assets/fonts");

fn face(file: &str, format: &'static str) -> FontFaceSource {
    FontFaceSource {
        family: "Excalifont".into(),
        file: file.into(),
        upstream_file: file.into(),
        format,
        characters: "abc".into(),
        fallback_url: format!("https://esm.sh/@excalidraw/excalidraw/dist/prod/{file}"),
    }
}

fn decode_base64(s: &str) -> Vec<u8> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut n = 0;
    let mut out = Vec::new();
    for c in s.bytes().filter(|&c| c != b'=') {
        let v = alphabet.iter().position(|&a| a == c).unwrap() as u32;
        bits = (bits << 6) | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
            bits &= (1 << n) - 1;
        }
    }
    out
}

#[test]
fn font_files_inline_the_vendored_file_as_a_data_url() {
    let dir = std::path::Path::new(FONTS).join("Excalifont");
    let name = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .find(|n| n.ends_with(".woff2"))
        .unwrap();
    let file = format!("Excalifont/{name}");
    let content = FontFiles::new(FONTS).content(&face(&file, "woff2"));
    let data = content.strip_prefix("data:font/woff2;base64,").unwrap();
    assert_eq!(
        decode_base64(data),
        std::fs::read(std::path::Path::new(FONTS).join(&file)).unwrap()
    );

    let liberation = std::fs::read_dir(std::path::Path::new(FONTS).join("Liberation"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .find(|n| n.ends_with(".ttf"))
        .unwrap();
    let content =
        FontFiles::new(FONTS).content(&face(&format!("Liberation/{liberation}"), "truetype"));
    assert!(content.starts_with("data:font/ttf;base64,"));
}

#[test]
fn a_font_file_that_cannot_be_read_falls_back_to_upstreams_url() {
    // getContent: "in case of issues, at least return the last url as a
    // content" (ExcalidrawFontFace.ts:58-86)
    let missing = face("Excalifont/missing.woff2", "woff2");
    assert_eq!(
        FontFiles::new(FONTS).content(&missing),
        "https://esm.sh/@excalidraw/excalidraw/dist/prod/Excalifont/missing.woff2"
    );
}
