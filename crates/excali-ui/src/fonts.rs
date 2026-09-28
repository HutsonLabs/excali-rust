//! Scene fonts in the browser: register every range-split face with
//! `document.fonts` without fetching it, then ask for the characters a scene
//! uses so the browser fetches only the files whose unicode-range holds them.
//!
//! Upstream counterpart: `Fonts.loadFontFaces` and `fontFacesLoader`
//! (`packages/excalidraw/fonts/Fonts.ts:219-284`) with the `FontFace`s
//! `ExcalidrawFontFace` builds (`fonts/ExcalidrawFontFace.ts:17-30`). The
//! faces, their files and ranges are `excali_text::font_assets`; which files
//! a scene fetches is [`excali_text::font_assets::faces_to_load`] of its
//! [`scene_font_loads`], which the browser suite (`tests/web`) checks against
//! the requests Chromium makes.

use excali_core::element::{Element, FontFamily};
use excali_text::font_assets::{
    elements_font_loads, registered_families, scene_font_loads, FontFaceAsset, FontLoad,
};
use js_sys::{Array, Object};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Document, FontFace, FontFaceDescriptors};

/// The `FontFace` descriptors upstream registers a face with:
/// `{display: "swap", style: "normal", weight: "400", ...descriptors}`.
pub fn descriptors(face: &FontFaceAsset) -> FontFaceDescriptors {
    let d = FontFaceDescriptors::new();
    d.set_display(face.display);
    d.set_style(face.style);
    d.set_weight(face.weight);
    if let Some(range) = face.unicode_range {
        d.set_unicode_range(range);
    }
    d
}

/// The registered faces of one document: one `FontFace` per non-local face
/// of the manifest, sourced from `base_url` (the directory holding the
/// contents of `assets/fonts/`).
pub struct WebFonts {
    document: Document,
    base_url: String,
    faces: Vec<(&'static FontFaceAsset, FontFace)>,
}

/// What a load did: the faces the browser loaded for the requested
/// characters, and the families whose load failed (upstream logs those and
/// carries on, `Fonts.ts:271-280`).
pub struct FontsLoaded {
    pub faces: Vec<FontFace>,
    pub failed: Vec<(FontFamily, JsValue)>,
}

impl WebFonts {
    /// Creates a `FontFace` for every face of every non-local registered
    /// family and adds it to `document.fonts` unless already there
    /// (`Fonts.ts:224-236`). Nothing is fetched: a face added this way
    /// stays "unloaded" until text needs it.
    pub fn register(document: &Document, base_url: &str) -> Result<WebFonts, JsValue> {
        let set = document.fonts();
        let mut faces = Vec::new();
        for family in registered_families().iter().filter(|f| !f.local) {
            for asset in family.faces {
                let face = FontFace::new_with_str_and_descriptors(
                    asset.family,
                    &asset.src(base_url),
                    &descriptors(asset),
                )?;
                if !set.has(&face) {
                    set.add(&face)?;
                }
                faces.push((asset, face));
            }
        }
        Ok(WebFonts {
            document: document.clone(),
            base_url: base_url.to_owned(),
            faces,
        })
    }

    /// The base URL the faces are sourced from.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Removes this registry's faces from `document.fonts`, e.g. to
    /// register them again from another base URL.
    pub fn unregister(&self) {
        let set = self.document.fonts();
        for (_, face) in &self.faces {
            set.delete(face);
        }
    }

    /// The registered faces with their manifest entries.
    pub fn faces(&self) -> impl Iterator<Item = (&'static FontFaceAsset, &FontFace)> {
        self.faces.iter().map(|(a, f)| (*a, f))
    }

    /// The manifest entry of a `FontFace` this registry created.
    pub fn asset_of(&self, face: &FontFace) -> Option<&'static FontFaceAsset> {
        self.faces
            .iter()
            .find(|(_, f)| Object::is(f.as_ref(), face.as_ref()))
            .map(|(a, _)| *a)
    }

    /// `fonts.loadSceneFonts()` (`Fonts.ts:153-164`): the scene's
    /// non-deleted text, family by family.
    pub async fn load_scene_fonts(&self, elements: &[Element]) -> FontsLoaded {
        self.load(scene_font_loads(elements)).await
    }

    /// `Fonts.loadElementsFonts(elements)` (`Fonts.ts:169-177`): every
    /// element given, as for export.
    pub async fn load_elements_fonts(&self, elements: &[Element]) -> FontsLoaded {
        self.load(elements_font_loads(elements)).await
    }

    /// `fontFacesLoader` (`Fonts.ts:249-284`): for each family whose
    /// characters `document.fonts.check(font, text)` says are not ready,
    /// `document.fonts.load(font, text)`; all loads run at once.
    async fn load(&self, loads: Vec<FontLoad>) -> FontsLoaded {
        let set = self.document.fonts();
        let mut pending = Vec::new();
        for load in &loads {
            match set.check_with_text(&load.font, &load.text) {
                Ok(true) => {}
                Ok(false) => pending.push((
                    load.font_family,
                    JsFuture::from(set.load_with_text(&load.font, &load.text)),
                )),
                Err(e) => pending.push((
                    load.font_family,
                    JsFuture::from(js_sys::Promise::reject(&e)),
                )),
            }
        }
        let mut out = FontsLoaded {
            faces: Vec::new(),
            failed: Vec::new(),
        };
        for (family, future) in pending {
            match future.await {
                Ok(list) => {
                    for face in Array::from(&list).iter() {
                        if let Ok(face) = face.dyn_into::<FontFace>() {
                            if !out.faces.iter().any(|f| Object::is(f, &face)) {
                                out.faces.push(face);
                            }
                        }
                    }
                }
                Err(e) => out.failed.push((family, e)),
            }
        }
        out
    }
}
