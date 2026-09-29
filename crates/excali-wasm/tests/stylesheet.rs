//! `excali.css`, the stylesheet the release ships beside the module
//! (`scripts/web/build.sh`), is the text the element installs:
//! [`excali_wasm::web::stylesheet`] (the primitives' with upstream's theme
//! tokens, the toolbar's, the canvases' and the element's rules). Hosts
//! that link it get the same rules before the module loads.
//!
//! After a stylesheet changes, regenerate it with
//! `EXCALI_WRITE_CSS=1 cargo test -p excali-wasm --test stylesheet`.

use std::path::PathBuf;

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("excali.css")
}

#[test]
fn excali_css_is_the_element_stylesheet() {
    if std::env::var_os("EXCALI_WRITE_CSS").is_some() {
        std::fs::write(path(), excali_wasm::web::stylesheet()).unwrap();
    }
    let shipped = std::fs::read_to_string(path()).expect("crates/excali-wasm/excali.css");
    assert!(
        shipped == excali_wasm::web::stylesheet(),
        "crates/excali-wasm/excali.css is stale; run \
         EXCALI_WRITE_CSS=1 cargo test -p excali-wasm --test stylesheet"
    );
}

#[test]
fn the_element_rules_are_in_it() {
    let css = excali_wasm::web::stylesheet();
    assert!(css.contains(".excalidraw.theme--dark"), "theme tokens");
    assert!(css.contains("excali-editor {\n  display: block;"));
    assert!(css.contains(".excalidraw canvas.interactive"));
}
