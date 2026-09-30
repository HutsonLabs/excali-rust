//! The `<excali-editor>` web runtime entry point for hosts.
//!
//! Upstream counterpart: the `Excalidraw` React component's public props.
//!
//! The element (ex-530): [`editor::Editor`] is the editor without the DOM
//! (load, save, export, library import, state, keys, pointer selection
//! and dragging, undo and redo with the real leaf layouts of
//! [`env::EditorEnv`]); [`web::EditorCore`] mounts it in the host element
//! for the custom element shim (`js/excali-editor.js`), whose API is the
//! one on `site/content/architecture/termhut-integration.md`.
//!
//! Fonts: [`register_fonts`] registers every range-split face of the
//! manifest with `document.fonts` (nothing is fetched), and
//! [`load_scene_fonts`] asks the browser for the characters a scene uses,
//! so only the files whose unicode-range holds them are downloaded, as
//! upstream's `Fonts.loadSceneFonts` does (`packages/excalidraw/fonts/Fonts.ts`).
//! [`scene_font_files`] is the same selection computed without a browser.
//!
//! Targets: wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-ui`
//! and `excali-svg`.

mod cropping;
pub mod drag;
pub mod editor;
pub mod env;
mod interact;
mod linear;
mod multi;
mod stats;
pub mod text;
pub mod web;

use std::cell::RefCell;
use std::rc::Rc;

use excali_core::document::Document;
use excali_core::element::Element;
use excali_text::font_assets::{faces_to_load, scene_font_loads};
use excali_ui::fonts::WebFonts;
use wasm_bindgen::prelude::*;

thread_local! {
    static FONTS: RefCell<Option<Rc<WebFonts>>> = const { RefCell::new(None) };
}

/// The elements of a `.excalidraw` file, deleted ones included.
pub fn scene_elements(scene_json: &str) -> Result<Vec<Element>, String> {
    let doc = Document::from_json(scene_json).map_err(|e| e.to_string())?;
    Ok(doc.elements.unwrap_or_default())
}

/// The font files (paths under the fonts directory) that loading this
/// scene's fonts fetches: the faces whose unicode-range holds a character
/// its non-deleted text uses, per family and fallback.
pub fn scene_font_file_list(scene_json: &str) -> Result<Vec<String>, String> {
    let elements = scene_elements(scene_json)?;
    Ok(faces_to_load(&scene_font_loads(&elements))
        .iter()
        .map(|f| f.file.to_owned())
        .collect())
}

/// `sceneFontFiles(sceneJson)`: [`scene_font_file_list`].
#[wasm_bindgen(js_name = sceneFontFiles)]
pub fn scene_font_files(scene_json: &str) -> Result<Vec<String>, JsError> {
    scene_font_file_list(scene_json).map_err(|e| JsError::new(&e))
}

/// `registerFonts(baseUrl)`: registers every face of the manifest with the
/// window's `document.fonts`, sourced from `baseUrl` (the directory the
/// release ships as `fonts/`). Returns how many faces were registered.
/// The faces are created once, as upstream's registry is static
/// (`Fonts.ts:46-77`): calling it again with the same base URL keeps them,
/// with another base URL replaces them.
#[wasm_bindgen(js_name = registerFonts)]
pub fn register_fonts(base_url: &str) -> Result<u32, JsError> {
    let count = |fonts: &WebFonts| u32::try_from(fonts.faces().count()).unwrap_or(u32::MAX);
    if let Some(existing) = FONTS.with(|f| f.borrow().clone()) {
        if existing.base_url() == base_url {
            return Ok(count(&existing));
        }
        existing.unregister();
    }
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsError::new("registerFonts needs a window with a document"))?;
    let fonts = WebFonts::register(&document, base_url)
        .map_err(|e| JsError::new(&format!("registerFonts: {e:?}")))?;
    let n = count(&fonts);
    FONTS.with(|f| *f.borrow_mut() = Some(Rc::new(fonts)));
    Ok(n)
}

/// `loadSceneFonts(sceneJson)`: loads the fonts the scene's text needs and
/// resolves to the files of the faces the browser loaded. Rejects when
/// [`register_fonts`] has not run or a family fails to load.
#[wasm_bindgen(js_name = loadSceneFonts)]
pub async fn load_scene_fonts(scene_json: String) -> Result<Vec<String>, JsError> {
    let fonts = FONTS
        .with(|f| f.borrow().clone())
        .ok_or_else(|| JsError::new("loadSceneFonts: call registerFonts first"))?;
    let elements = scene_elements(&scene_json).map_err(|e| JsError::new(&e))?;
    let loaded = fonts.load_scene_fonts(&elements).await;
    if let Some((family, e)) = loaded.failed.first() {
        return Err(JsError::new(&format!(
            "loadSceneFonts: family {} failed to load: {e:?}",
            family.0
        )));
    }
    Ok(loaded
        .faces
        .iter()
        .map(|face| {
            fonts
                .asset_of(face)
                .map_or_else(|| face.family(), |a| a.file.to_owned())
        })
        .collect())
}
