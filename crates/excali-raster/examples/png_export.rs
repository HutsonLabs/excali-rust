//! Writes the port's PNG export of every scene of excali-scene's canvas
//! export golden into a directory, for upstream to read back
//! (`node tools/goldens/png-export.mjs --reimport DIR`): `<name>.png`, and
//! `<name>.json` with the scene's name, the canvas size and the scene text
//! embedded in the PNG (`null` when none is).
//!
//!     cargo run -p excali-raster --example png_export -- DIR

use std::collections::HashMap;
use std::path::PathBuf;

use excali_raster::tiny_skia::{self, Mask, Pixmap};
use excali_raster::{export_png, TextRasterizer};
use excali_scene::display::TextRun;

#[path = "../tests/support/golden_scenes.rs"]
mod golden_scenes;

/// Text pixels play no part in reading the scene back.
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

fn main() {
    let dir: PathBuf = std::env::args_os()
        .nth(1)
        .expect("usage: png_export DIR")
        .into();
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = golden_scenes::fixture();
    let source = fixture["origin"].as_str().unwrap();
    let images: HashMap<String, Pixmap> = HashMap::new();
    let mut written = 0;
    for scene in fixture["scenes"].as_array().unwrap() {
        let name = scene["name"].as_str().unwrap();
        let doc = golden_scenes::document(scene, source);
        let png = export_png(&doc, &images, &mut NoText).unwrap_or_else(|e| panic!("{name}: {e}"));
        let metadata = doc.payload.as_ref().map(|p| {
            let wrapper: serde_json::Value = serde_json::from_str(&p.text).unwrap();
            let encoded: excali_core::encode::EncodedData =
                serde_json::from_value(wrapper).unwrap();
            excali_core::encode::decode(&encoded).unwrap()
        });
        std::fs::write(dir.join(format!("{name}.png")), &png).unwrap();
        let meta = serde_json::json!({
            "scene": name,
            "width": doc.width,
            "height": doc.height,
            "metadata": metadata,
        });
        std::fs::write(dir.join(format!("{name}.json")), meta.to_string()).unwrap();
        written += 1;
    }
    println!("wrote {written} PNGs to {}", dir.display());
}
