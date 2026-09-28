//! PNG export, the raster side (ex-405): the canvas `exportToCanvas` sized
//! and drew, encoded as `canvas.toBlob()` encodes it (8-bit RGBA, the
//! canvas's size), with `encodePngMetadata`'s `tEXt` chunk inserted before
//! `IEND` when the scene is embedded (`packages/excalidraw/data/index.ts:
//! 173-186`, `data/image.ts:25-47`).
//!
//! The documents are the port's exports of excali-scene's canvas export
//! golden (`tests/support/golden_scenes.rs`); excali-scene's
//! `canvas_export` test holds their sizes and draws to upstream's. CI also
//! has upstream's own `decodePngMetadata` and `loadFromBlob` read the PNGs
//! the `png_export` example writes (`tools/goldens/png-export.mjs
//! --reimport`).

use std::collections::HashMap;

use excali_core::color::apply_dark_mode_filter;
use excali_core::png::{decode_png_metadata, extract_chunks, get_text_chunk};
use excali_raster::tiny_skia::{self, Mask, Pixmap};
use excali_raster::{export_png, render, PngExportError, TextRasterizer};
use excali_scene::display::{
    CanvasDocument, Color, DisplayItem, DisplayList, FillRule, Path, PngPayload, TextRun,
};

#[path = "support/golden_scenes.rs"]
mod golden_scenes;

use golden_scenes::{document, fixture};

/// Text is the caller's; these PNGs carry none.
struct NoText;

impl TextRasterizer for NoText {
    fn fill_text(
        &mut self,
        _: &mut Pixmap,
        _: &TextRun,
        _: tiny_skia::Color,
        _: tiny_skia::Transform,
        _: Option<&Mask>,
    ) {
    }
}

type Images = HashMap<String, Pixmap>;

fn scenes() -> Vec<serde_json::Value> {
    fixture()["scenes"].as_array().unwrap().clone()
}

fn source() -> String {
    fixture()["origin"].as_str().unwrap().to_owned()
}

fn exported(name: &str) -> (serde_json::Value, CanvasDocument, Vec<u8>) {
    let scene = scenes().into_iter().find(|s| s["name"] == name).unwrap();
    let doc = document(&scene, &source());
    let png = export_png(&doc, &Images::new(), &mut NoText).unwrap();
    (scene, doc, png)
}

/// The decoded PNG's pixel at (x, y), unpremultiplied RGBA.
fn pixel(png: &[u8], x: u32, y: u32) -> [u8; 4] {
    let p = Pixmap::decode_png(png).unwrap();
    let c = p.pixel(x, y).unwrap().demultiply();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

fn rgb(hex: &str) -> [u8; 4] {
    let rgba = Color::new(hex).rgba().unwrap();
    [rgba.r, rgba.g, rgba.b, 255]
}

#[test]
fn every_golden_scene_exports_at_upstreams_canvas_size() {
    let mut checked = 0;
    for scene in scenes() {
        let name = scene["name"].as_str().unwrap();
        let doc = document(&scene, &source());
        let png = export_png(&doc, &Images::new(), &mut NoText).unwrap();
        let chunks = extract_chunks(&png).unwrap();
        let ihdr = &chunks[0];
        assert_eq!(&ihdr.name, b"IHDR", "{name}");
        let w = u32::from_be_bytes(ihdr.data[0..4].try_into().unwrap());
        let h = u32::from_be_bytes(ihdr.data[4..8].try_into().unwrap());
        assert_eq!(
            (u64::from(w), u64::from(h)),
            (
                scene["width"].as_u64().unwrap(),
                scene["height"].as_u64().unwrap()
            ),
            "{name}"
        );
        // 8-bit RGBA, as the canvas encodes
        assert_eq!(ihdr.data[8], 8, "{name} bit depth");
        assert_eq!(ihdr.data[9], 6, "{name} colour type");
        checked += 1;
    }
    assert!(checked > 20);
}

#[test]
fn the_background_fills_the_canvas_or_stays_transparent() {
    let (_, _, png) = exported("default");
    assert_eq!(pixel(&png, 0, 0), [255, 255, 255, 255]);
    let (_, doc, png) = exported("background-coloured");
    assert_eq!(pixel(&png, doc.width - 1, doc.height - 1), rgb("#fff9db"));
    let (_, _, png) = exported("no-background");
    assert_eq!(pixel(&png, 0, 0), [0, 0, 0, 0]);
    let (_, _, png) = exported("background-transparent");
    assert_eq!(pixel(&png, 0, 0), [0, 0, 0, 0]);
    // exportWithDarkMode: the colour through the dark-mode filter
    let (_, _, png) = exported("dark");
    assert_eq!(
        pixel(&png, 0, 0),
        rgb(&apply_dark_mode_filter("#ffc9c9", true))
    );
}

#[test]
fn the_pixels_are_the_rendered_canvas() {
    for name in ["default", "scale-2", "exporting-frame", "utils-max-smaller"] {
        let (_, doc, png) = exported(name);
        let mut canvas = Pixmap::new(doc.width, doc.height).unwrap();
        render(&doc.list, &mut canvas, &Images::new(), &mut NoText);
        let decoded = Pixmap::decode_png(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (doc.width, doc.height));
        for (a, b) in decoded.pixels().iter().zip(canvas.pixels()) {
            // an encode/decode round trip keeps the unpremultiplied colour
            // of every opaque pixel exactly; the rest within rounding
            let (a, b) = (a.demultiply(), b.demultiply());
            assert_eq!(a.alpha(), b.alpha(), "{name}");
            let tolerance = if b.alpha() == 255 {
                0
            } else {
                255 / b.alpha().max(1)
            };
            for (x, y) in [
                (a.red(), b.red()),
                (a.green(), b.green()),
                (a.blue(), b.blue()),
            ] {
                assert!(x.abs_diff(y) <= tolerance, "{name}: {a:?} vs {b:?}");
            }
        }
        // the content is drawn: some pixel is not the background
        assert!(
            canvas
                .pixels()
                .iter()
                .any(|p| p.red() != 255 || p.alpha() != 255),
            "{name}: nothing drawn"
        );
    }
}

#[test]
fn an_embedded_scene_is_a_text_chunk_before_iend() {
    let mut embedded = 0;
    for scene in scenes() {
        let name = scene["name"].as_str().unwrap();
        let doc = document(&scene, &source());
        let png = export_png(&doc, &Images::new(), &mut NoText).unwrap();
        let chunks = extract_chunks(&png).unwrap();
        let names: Vec<&[u8; 4]> = chunks.iter().map(|c| &c.name).collect();
        let text_chunks = names.iter().filter(|n| **n == b"tEXt").count();
        match scene["metadata"].as_str() {
            None => {
                assert!(doc.payload.is_none(), "{name}");
                assert_eq!(text_chunks, 0, "{name}");
                assert!(decode_png_metadata(&png).is_err(), "{name}");
            }
            Some(expected) => {
                assert_eq!(text_chunks, 1, "{name}");
                assert_eq!(names[names.len() - 2], b"tEXt", "{name}");
                assert_eq!(names[names.len() - 1], b"IEND", "{name}");
                let chunk = get_text_chunk(&png).unwrap().unwrap();
                assert_eq!(chunk.keyword, "application/vnd.excalidraw+json");
                assert_eq!(
                    decode_png_metadata(&png).unwrap().as_deref(),
                    Some(expected),
                    "{name}"
                );
                embedded += 1;
            }
        }
    }
    assert!(embedded >= 6, "only {embedded} embedded scenes");
}

fn square(width: u32, height: u32, payload: Option<PngPayload>) -> CanvasDocument {
    CanvasDocument {
        width,
        height,
        list: DisplayList::from_iter([DisplayItem::Fill {
            path: Path::rect(0.0, 0.0, 2.0, 2.0),
            color: Color::new("#ff0000"),
            rule: FillRule::NonZero,
        }]),
        payload,
    }
}

#[test]
fn a_canvas_with_no_pixels_has_no_blob() {
    // toBlob gives null for a canvas without pixels; upstream rejects with
    // CANVAS_POSSIBLY_TOO_BIG
    for (w, h) in [(0, 0), (0, 10), (10, 0)] {
        assert_eq!(
            export_png(&square(w, h, None), &Images::new(), &mut NoText),
            Err(PngExportError::CanvasTooBig)
        );
    }
    let png = export_png(&square(3, 3, None), &Images::new(), &mut NoText).unwrap();
    assert_eq!(pixel(&png, 0, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&png, 2, 2), [0, 0, 0, 0]);
}

#[test]
fn the_payload_chunk_holds_keyword_and_text_as_latin1() {
    let payload = PngPayload {
        keyword: "application/vnd.excalidraw+json".to_owned(),
        text: "{\"encoded\":\"x\u{9c}\u{ff}\"}".to_owned(),
    };
    let png = export_png(
        &square(2, 2, Some(payload.clone())),
        &Images::new(),
        &mut NoText,
    )
    .unwrap();
    let chunk = get_text_chunk(&png).unwrap().unwrap();
    assert_eq!(chunk.keyword, payload.keyword);
    assert_eq!(chunk.text, payload.text);
    let chunks = extract_chunks(&png).unwrap();
    let data = &chunks[chunks.len() - 2].data;
    assert_eq!(
        data.len(),
        payload.keyword.len() + 1 + payload.text.chars().count()
    );
}
