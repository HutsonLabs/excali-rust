//! ex-502 harness: the fixture display lists painted by excali-canvas2d.
//!
//! excali-raster's fixture display lists
//! (`crates/excali-raster/tests/fixtures/display-lists/`) are read with the
//! loader excali-raster's own fixture test uses
//! (`crates/excali-raster/tests/support/display_lists.rs`), so both backends
//! draw one parse of each file, and painted on the page's
//! `CanvasRenderingContext2D` by [`excali_canvas2d::paint_scaled`] through
//! [`excali_canvas2d::WebCanvas`], as the editor paints. The Playwright
//! suite `tests/web/canvas2d` drives it; `scripts/web/canvas2d-fixtures.sh`
//! builds it.
//!
//! Images: a fixture's own images are `<img>` elements the page creates and
//! hands over ([`FixturePainter::add_image`]); built-in images
//! (`excalidraw:…` ids) are the ones `WebCanvas::new` loads itself. The
//! page waits for every image [`FixturePainter::images`] returns before it
//! calls [`FixturePainter::paint`].

use std::collections::BTreeSet;

use excali_canvas2d::{paint_scaled, WebCanvas};
use excali_scene::display::{DisplayItem, DisplayList};
use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlImageElement};

#[path = "../../../crates/excali-raster/tests/support/display_lists.rs"]
mod display_lists;

pub use display_lists::ListFixture;

/// A fixture file read into its canvas size, device scale and display list.
pub fn load_fixture(json: &str) -> Result<ListFixture, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    display_lists::list_fixture(&v)
}

/// The ids of every image the list draws, groups included.
pub fn image_ids(list: &DisplayList) -> BTreeSet<String> {
    fn walk(items: &[DisplayItem], out: &mut BTreeSet<String>) {
        for item in items {
            match item {
                DisplayItem::Image(image) => {
                    out.insert(image.id.clone());
                }
                DisplayItem::Group(group) => walk(&group.items, out),
                _ => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(&list.items, &mut out);
    out
}

/// One fixture on one canvas: `new FixturePainter(context, fixtureJson)`.
#[wasm_bindgen]
pub struct FixturePainter {
    canvas: WebCanvas,
    fixture: ListFixture,
}

#[wasm_bindgen]
impl FixturePainter {
    /// Reads the fixture; throws with the loader's message when it cannot.
    #[wasm_bindgen(constructor)]
    pub fn new(context: CanvasRenderingContext2d, fixture_json: &str) -> Result<Self, JsError> {
        let fixture = load_fixture(fixture_json).map_err(|e| JsError::new(&e))?;
        Ok(Self {
            canvas: WebCanvas::new(context),
            fixture,
        })
    }

    /// The canvas width the fixture draws on.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.fixture.width
    }

    /// The canvas height the fixture draws on.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.fixture.height
    }

    /// The ids of the images the list draws.
    #[wasm_bindgen(js_name = imageIds)]
    pub fn image_ids(&self) -> Vec<String> {
        image_ids(&self.fixture.list).into_iter().collect()
    }

    /// Registers the fixture image `id` (the fixture's `images` entry).
    #[wasm_bindgen(js_name = addImage)]
    pub fn add_image(&mut self, id: String, image: HtmlImageElement) {
        self.canvas.images.insert(id, image);
    }

    /// The ids `WebCanvas` holds images for: the built-in ones it loads
    /// itself, and those added.
    #[wasm_bindgen(js_name = heldImageIds)]
    pub fn held_image_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.canvas.images.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Every image `WebCanvas` holds, for the page to await.
    pub fn images(&self) -> Vec<HtmlImageElement> {
        self.canvas.images.values().cloned().collect()
    }

    /// `paint_scaled(list, context, scale)`: the list on the fresh canvas at
    /// the fixture's device scale, as `bootstrapCanvas` scales it.
    pub fn paint(&mut self) {
        paint_scaled(&self.fixture.list, &mut self.canvas, self.fixture.scale);
    }
}
