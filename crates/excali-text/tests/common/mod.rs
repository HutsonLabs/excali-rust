//! The vendored fonts, loaded once per test binary.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::OnceLock;

use excali_text::font_store::FontStore;

/// The vendored fonts, `crates/excali-text/assets/fonts/` (ex-307).
pub fn fonts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")
}

/// A font file under `assets/fonts/`.
pub fn font_file(path: &str) -> Vec<u8> {
    std::fs::read(fonts_dir().join(path)).unwrap_or_else(|e| panic!("assets/fonts/{path}: {e}"))
}

/// Every vendored face of `FONT_FACES`.
pub fn store() -> &'static FontStore {
    static STORE: OnceLock<FontStore> = OnceLock::new();
    STORE.get_or_init(|| {
        FontStore::with_vendored_faces(|path| Some(font_file(path)))
            .expect("every vendored face loads")
    })
}
