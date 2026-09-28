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
use excali_svg::{base64, subset_woff2, FontContent, FontFiles, SVG_NS};
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
        fallback_url: format!("https://esm.sh/@excalidraw/excalidraw/dist/prod/fonts/{file}"),
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

fn vendored(dir: &str, ext: &str) -> String {
    let name = std::fs::read_dir(std::path::Path::new(FONTS).join(dir))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(ext))
        .min()
        .unwrap();
    format!("{dir}/{name}")
}

/// The sfnt of a font file (WOFF2 decoded as excali-text decodes it).
fn sfnt(bytes: &[u8]) -> Vec<u8> {
    if bytes.starts_with(b"wOF2") {
        wuff::decompress_woff2(bytes).unwrap()
    } else {
        bytes.to_vec()
    }
}

/// Every character of `text` drawn by `subset` with the glyph (advance and
/// outline) `original` draws it with.
fn same_glyphs(original: &[u8], subset: &[u8], text: &str) {
    #[derive(Default, PartialEq, Debug)]
    struct Pen(Vec<(char, [f32; 6])>);
    impl ttf_parser::OutlineBuilder for Pen {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.push(('M', [x, y, 0.0, 0.0, 0.0, 0.0]));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.push(('L', [x, y, 0.0, 0.0, 0.0, 0.0]));
        }
        fn quad_to(&mut self, a: f32, b: f32, x: f32, y: f32) {
            self.0.push(('Q', [a, b, x, y, 0.0, 0.0]));
        }
        fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, x: f32, y: f32) {
            self.0.push(('C', [a, b, c, d, x, y]));
        }
        fn close(&mut self) {
            self.0.push(('Z', [0.0; 6]));
        }
    }
    let a = ttf_parser::Face::parse(original, 0).unwrap();
    let b = ttf_parser::Face::parse(subset, 0).unwrap();
    assert_eq!(a.units_per_em(), b.units_per_em());
    assert_eq!(
        (a.ascender(), a.descender(), a.line_gap()),
        (b.ascender(), b.descender(), b.line_gap())
    );
    for c in text.chars() {
        let Some(ga) = a.glyph_index(c) else { continue };
        let gb = b.glyph_index(c).unwrap_or_else(|| panic!("{c:?} not kept"));
        assert_eq!(a.glyph_hor_advance(ga), b.glyph_hor_advance(gb), "{c:?}");
        let (mut pa, mut pb) = (Pen::default(), Pen::default());
        a.outline_glyph(ga, &mut pa);
        b.outline_glyph(gb, &mut pb);
        assert_eq!(pa, pb, "{c:?}");
    }
}

#[test]
fn font_files_inline_the_face_subset_to_the_scenes_characters() {
    // Upstream subsets every face it inlines to the family's characters and
    // writes WOFF2 (subset-shared.chunk.ts:44-57); ADR-010 decides the port
    // does the same with skera and ttf2woff2.
    for (file, format, characters) in [
        (vendored("Excalifont", ".woff2"), "woff2", "Hello, world!"),
        (vendored("Nunito", ".woff2"), "woff2", "AVATAR To Wa"),
        (
            vendored("Liberation", ".ttf"),
            "truetype",
            "Liberation Sans",
        ),
    ] {
        let mut f = face(&file, format);
        f.characters = characters.into();
        let content = FontFiles::new(FONTS).content(&f);
        let data = content
            .strip_prefix("data:font/woff2;base64,")
            .unwrap_or_else(|| panic!("{file}: {}", &content[..40]));
        let woff2 = decode_base64(data);
        assert_eq!(&woff2[..4], b"wOF2", "{file}");
        let whole = std::fs::read(std::path::Path::new(FONTS).join(&file)).unwrap();
        assert!(
            woff2.len() * 2 < whole.len(),
            "{file}: {} of {}",
            woff2.len(),
            whole.len()
        );
        let subset = sfnt(&woff2);
        same_glyphs(&sfnt(&whole), &subset, characters);
        let parsed = ttf_parser::Face::parse(&subset, 0).unwrap();
        assert!(parsed.glyph_index('z').is_none(), "{file}: kept z");
    }
}

#[test]
fn subset_woff2_keeps_the_layout_upstream_keeps() {
    // Every layout feature is kept ("the equivalent of --font-features=*",
    // harfbuzz-bindings.ts:74-81), so kerned pairs stay kerned.
    let file = vendored("Nunito", ".woff2");
    let whole = std::fs::read(std::path::Path::new(FONTS).join(&file)).unwrap();
    let woff2 = subset_woff2(&whole, "AVATAR").unwrap();
    let subset = sfnt(&woff2);
    let face = ttf_parser::Face::parse(&subset, 0).unwrap();
    for tag in [b"GSUB", b"GPOS", b"GDEF"] {
        assert!(face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(tag))
            .is_some());
    }
    // A face asked for nothing it maps still subsets (to .notdef).
    assert!(subset_woff2(&whole, "你").is_ok());
    assert!(subset_woff2(b"not a font", "abc").is_err());
}

#[test]
fn a_face_that_cannot_be_subset_is_inlined_whole() {
    // subsetToBase64: "Fallback to encoding whole font in case of errors"
    // (subset-shared.chunk.ts:25-39), with the file's own type.
    let dir = std::env::temp_dir().join(format!("excali-svg-fonts-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("X")).unwrap();
    std::fs::write(dir.join("X/broken.woff2"), b"wOF2 but not a font").unwrap();
    std::fs::write(dir.join("X/broken.ttf"), b"\0\x01\0\0 not a font").unwrap();
    let files = FontFiles::new(&dir);
    assert_eq!(
        files.content(&face("X/broken.woff2", "woff2")),
        format!("data:font/woff2;base64,{}", base64(b"wOF2 but not a font"))
    );
    assert_eq!(
        files.content(&face("X/broken.ttf", "truetype")),
        format!("data:font/ttf;base64,{}", base64(b"\0\x01\0\0 not a font"))
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn every_inlined_face_draws_what_upstreams_subset_draws() {
    // tools/font-subset-eval/upstream-subsets.json: upstream's own subsets
    // (harfbuzzjs 0.3.6 in its worker) of each face its SVG export inlines,
    // from tools/goldens/font-subset.mjs. For every face vendored as
    // upstream's own file (all but Liberation Sans, ADR-004), the port's
    // subset draws every code point with upstream's glyph.
    let fixture: Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/font-subset-eval/upstream-subsets.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut checked = 0;
    for scene in fixture["scenes"].as_array().unwrap() {
        if ![
            "fixture-default",
            "labels-excalifont",
            "labels-nunito",
            "assets-comic-shanns",
            "assets-cascadia",
            "assets-lilita",
            "assets-virgil",
        ]
        .contains(&scene["name"].as_str().unwrap())
        {
            continue;
        }
        for d in scene["declarations"].as_array().unwrap() {
            let file = d["file"].as_str().unwrap();
            let characters: String = d["codePoints"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| char::from_u32(c.as_u64().unwrap() as u32).unwrap())
                .collect();
            let mut f = face(file, "woff2");
            f.characters = characters.clone();
            let content = FontFiles::new(FONTS).content(&f);
            let ours = sfnt(&decode_base64(
                content.strip_prefix("data:font/woff2;base64,").unwrap(),
            ));
            let theirs = sfnt(&decode_base64(d["woff2"].as_str().unwrap()));
            same_glyphs(&theirs, &ours, &characters);
            checked += 1;
        }
    }
    assert!(checked >= 8, "{checked}");
}

#[test]
fn a_font_file_that_cannot_be_read_falls_back_to_upstreams_url() {
    // getContent: "in case of issues, at least return the last url as a
    // content" (ExcalidrawFontFace.ts:58-86)
    let missing = face("Excalifont/missing.woff2", "woff2");
    assert_eq!(
        FontFiles::new(FONTS).content(&missing),
        "https://esm.sh/@excalidraw/excalidraw/dist/prod/fonts/Excalifont/missing.woff2"
    );
}
