//! Write PNGs with an embedded scene, for upstream to read back (ex-111).
//!
//! `cargo run -p excali-core --example png_embed -- <dir>` writes, for each
//! case, `<case>.png` (made by [`encode_png_metadata`]) and `<case>.txt`
//! (the scene text embedded, UTF-8). `scripts/fixtures/png-goldens.sh --check`
//! then runs upstream's `decodePngMetadata` on every PNG and requires the
//! text back exactly.

use std::path::{Path, PathBuf};

use excali_core::document::Document;
use excali_core::png::{decode_png_metadata, encode_png_metadata};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> Vec<u8> {
    let path = repo_root().join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn main() {
    let out = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: png_embed <output dir>");
    std::fs::create_dir_all(&out).expect("create the output directory");

    let fixtures = "fixtures/upstream/packages/excalidraw/tests/fixtures";
    let smiley = read(&format!("{fixtures}/smiley.png"));
    let deer = read(&format!("{fixtures}/deer.png"));

    // Scenes as the port writes them: parsed and written by `Document`.
    let scene = |rel: &str| {
        let text = String::from_utf8(read(rel)).expect("fixture is UTF-8");
        Document::from_json(&text).expect("fixture is a scene").to_json()
    };
    let smiley_scene = decode_png_metadata(&read(&format!("{fixtures}/smiley_embedded_v2.png")))
        .expect("smiley_embedded_v2.png decodes")
        .expect("smiley_embedded_v2.png holds a scene");
    let smiley_scene = Document::from_json(&smiley_scene)
        .expect("a scene")
        .to_json();

    let cases: Vec<(&str, &[u8], String)> = vec![
        ("smiley_scene_in_smiley", &smiley, smiley_scene.clone()),
        ("smiley_scene_in_deer", &deer, smiley_scene),
        (
            "every_type_in_smiley",
            &smiley,
            scene("crates/excali-core/tests/fixtures/every-type.excalidraw"),
        ),
        (
            "unknown_keys_in_deer",
            &deer,
            scene("crates/excali-core/tests/fixtures/unknown-keys.excalidraw"),
        ),
        (
            "empty_scene_in_smiley",
            &smiley,
            scene("crates/excali-core/tests/fixtures/empty-scene.excalidraw"),
        ),
        (
            "unicode_text_in_smiley",
            &smiley,
            "\u{1F600} \u{fc}n\u{ef}c\u{f6}d\u{e9} \u{2014} \u{4e2d}\u{6587} \u{0}\u{1f}\u{7f} \"\\ </script>"
                .to_owned(),
        ),
        (
            "long_text_in_deer",
            &deer,
            "excalidraw \u{e9}l\u{e9}ment \u{1F3A8} ".repeat(5000),
        ),
        ("empty_text_in_smiley", &smiley, String::new()),
    ];

    for (name, png, text) in cases {
        let written = encode_png_metadata(png, &text).expect("the fixture is a PNG");
        std::fs::write(out.join(format!("{name}.png")), written).expect("write the PNG");
        std::fs::write(out.join(format!("{name}.txt")), &text).expect("write the text");
        eprintln!("wrote {name}.png ({} bytes of scene text)", text.len());
    }
}
