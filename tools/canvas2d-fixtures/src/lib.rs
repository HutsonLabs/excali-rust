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
//!
//! [`blit_bitmap`] (ex-504) paints a list into a bitmap with
//! `WebCanvas::rasterize` and draws it with `excali_canvas2d::blit` at a
//! whole device pixel with smoothing off, as a snapped element is drawn,
//! beside [`paint_translated`], the same list painted directly.

use std::collections::BTreeSet;

use excali_canvas2d::{blit, paint, paint_scaled, WebCanvas};
use excali_scene::display::{
    bitmap_id, Blit, Color, DisplayItem, DisplayList, FillRule, Group, Path, Rect, Stroke,
    Transform,
};
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

/// The bitmap [`blit_bitmap`] draws: a 40 × 30 bitmap holding an
/// anti-aliased triangle, circle and stroke at fractional positions, all
/// inside it.
fn bitmap_list() -> DisplayList {
    let mut triangle = Path::new();
    triangle.move_to(3.3, 2.7);
    triangle.line_to(25.6, 6.1);
    triangle.line_to(9.2, 27.4);
    triangle.close();
    let mut circle = Path::new();
    circle.arc(28.5, 17.25, 8.3, 0.0, std::f64::consts::TAU, false);
    let mut line = Path::new();
    line.move_to(2.5, 24.5);
    line.line_to(37.2, 3.9);
    DisplayList {
        items: vec![
            DisplayItem::Fill {
                path: triangle,
                color: Color::new("#1971c2"),
                rule: FillRule::NonZero,
            },
            DisplayItem::Fill {
                path: circle,
                color: Color::new("rgba(224, 49, 49, 0.6)"),
                rule: FillRule::NonZero,
            },
            DisplayItem::Stroke {
                path: line,
                stroke: Stroke::new(Color::new("#2f9e44"), 1.7),
            },
        ],
    }
}

/// Where both drawings put the bitmap's origin, in device pixels.
const AT: (f64, f64) = (7.0, 5.0);

/// The bitmap drawn by `blit` at [`AT`] with smoothing off, after
/// `WebCanvas::rasterize`. Throws when the bitmap cannot be made.
#[wasm_bindgen(js_name = blitBitmap)]
pub fn blit_bitmap(context: CanvasRenderingContext2d) -> Result<(), JsError> {
    let mut canvas = WebCanvas::new(context);
    let bitmap = canvas
        .rasterize(40.0, 30.0, &bitmap_list())
        .ok_or_else(|| JsError::new("rasterize made no canvas"))?;
    let id = bitmap_id("harness");
    canvas.bitmaps.insert(id.clone(), bitmap);
    blit(
        &mut canvas,
        &Blit {
            id,
            alpha: 1.0,
            smoothing: Some(false),
            clip: None,
            transform: Transform::translate(AT.0, AT.1),
            dest: Rect::new(0.0, 0.0, 40.0, 30.0),
        },
    );
    Ok(())
}

/// The same list painted directly at [`AT`].
#[wasm_bindgen(js_name = paintTranslated)]
pub fn paint_translated(context: CanvasRenderingContext2d) {
    let list = DisplayList {
        items: vec![DisplayItem::Group(Group {
            transform: Transform::translate(AT.0, AT.1),
            ..Group::new(bitmap_list().items)
        })],
    };
    paint(&list, &mut WebCanvas::new(context));
}
