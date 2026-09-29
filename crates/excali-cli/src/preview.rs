//! A library's preview image: what the publish dialog uploads with a
//! library (`generatePreviewImage`,
//! `packages/excalidraw/components/PublishLibrary.tsx:38-105`), the
//! catalogue's contact sheet of the library's items.
//!
//! Upstream draws each item with the utils `exportToCanvas({elements:
//! item.elements, files: null, maxWidthOrHeight: 128})`
//! (`packages/utils/src/export.ts:43-104`) — [`item_document`]: the
//! elements restored again with `deleteInvisibleElements` and the deleted
//! ones dropped, the app state `restoreAppState(undefined, null)` gives
//! (white background, `exportBackground` on, the default frame
//! rendering), no files (an image element draws the placeholder), and the
//! canvas brought down to 128 px on its longer side when the content is
//! larger, else at scale 1. Then, on a canvas sized for rows of six
//! 128 px boxes with 8 px of padding ([`sheet_size`]), filled white, each
//! item's canvas is drawn centred in its box (`drawImage(itemCanvas, x,
//! y)`, at its own size, smoothed) and the box outlined
//! (`strokeRect`, 2 px, `#ced4da`) — [`sheet_document`].
//!
//! The sheet is a canvas document like any export's, painted by the
//! raster backend with the item canvases as its images, and written as
//! `canvasToBlob` writes it: a PNG. Upstream then re-encodes the blob as a
//! JPEG no larger than 5000 px on a side for the upload
//! (`resizeImageFile`, `data/blob.ts`); the CLI keeps the PNG, the lossless
//! image that step starts from (a catalogue library's sheet is at most
//! 864 px wide and, for the largest, 249 items, 6048 px high).
//!
//! Upstream's `chunk(libraryItems, 6)` of no items has no first row, so an
//! empty library has no preview (a `TypeError` there, a
//! [`Failure::Export`] here), and `drawImage` of a canvas without pixels
//! throws, so an item whose canvas has none fails the whole sheet.

use std::collections::HashMap;

use serde_json::{Map, Value};

use excali_core::app_state::{restore_app_state, AppStateEnv};
use excali_core::element::Element;
use excali_core::library::LibraryItem;
use excali_core::restore::{restore_elements, RestoreElementsOptions, RestoreEnv};
use excali_raster::tiny_skia::{self, Pixmap};
use excali_raster::{paint_canvas, PngExportError, TextRasterizer};
use excali_scene::canvas_export::{export_to_canvas, CanvasExportOptions, CanvasSizing};
use excali_scene::display::{
    CanvasDocument, Color, DisplayItem, DisplayList, ImageItem, Path, Rect, Stroke, TextRun,
};
use excali_text::font_store::FontStore;

use crate::error::Failure;
use crate::export::truthy;
use crate::fonts::GlyphText;

/// `MAX_ITEMS_PER_ROW`.
pub const MAX_ITEMS_PER_ROW: usize = 6;
/// `BOX_SIZE`: the `maxWidthOrHeight` each item is drawn at.
pub const BOX_SIZE: f64 = 128.0;
/// `BOX_PADDING = Math.round(BOX_SIZE / 16)`.
pub const BOX_PADDING: f64 = 8.0;
/// `BORDER_WIDTH = Math.max(Math.round(BOX_SIZE / 64), 2)`.
pub const BORDER_WIDTH: f64 = 2.0;
/// The box outline's `strokeStyle`.
pub const BORDER_COLOR: &str = "#ced4da";
/// The sheet's `fillStyle`.
pub const SHEET_BACKGROUND: &str = "#fff";

/// What an empty library's preview fails with.
pub const NO_ITEMS: &str = "the library has no items to preview";

/// The sheet's canvas size for `count` items: `rows[0].length * BOX_SIZE +
/// (rows[0].length + 1) * (BOX_PADDING * 2) - BOX_PADDING * 2` wide, and
/// the same of the number of rows high. `None` without items.
pub fn sheet_size(count: usize) -> Option<(u32, u32)> {
    if count == 0 {
        return None;
    }
    let side = |k: usize| {
        let k = k as f64;
        k * BOX_SIZE + (k + 1.0) * (BOX_PADDING * 2.0) - BOX_PADDING * 2.0
    };
    let columns = count.min(MAX_ITEMS_PER_ROW);
    let rows = count.div_ceil(MAX_ITEMS_PER_ROW);
    Some((
        excali_scene::canvas_export::canvas_width(side(columns)),
        excali_scene::canvas_export::canvas_height(side(rows)),
    ))
}

/// The top left of item `index`'s box: `(colOffset, rowOffset)`.
fn box_offset(index: usize) -> (f64, f64) {
    let step = BOX_SIZE + BOX_PADDING * 2.0;
    (
        (index % MAX_ITEMS_PER_ROW) as f64 * step,
        (index / MAX_ITEMS_PER_ROW) as f64 * step,
    )
}

/// Where item `index`'s canvas of `width` x `height` is drawn: centred in
/// its box.
pub fn item_origin(index: usize, width: u32, height: u32) -> (f64, f64) {
    let (col, row) = box_offset(index);
    (
        col + (BOX_SIZE - f64::from(width)) / 2.0 + BOX_PADDING,
        row + (BOX_SIZE - f64::from(height)) / 2.0 + BOX_PADDING,
    )
}

/// Item `index`'s outline: `strokeRect(colOffset + BOX_PADDING / 2,
/// rowOffset + BOX_PADDING / 2, BOX_SIZE + BOX_PADDING, BOX_SIZE +
/// BOX_PADDING)`.
pub fn border_rect(index: usize) -> Rect {
    let (col, row) = box_offset(index);
    Rect::new(
        col + BOX_PADDING / 2.0,
        row + BOX_PADDING / 2.0,
        BOX_SIZE + BOX_PADDING,
        BOX_SIZE + BOX_PADDING,
    )
}

/// The image id item `index`'s canvas is drawn from in the sheet.
pub fn item_image_id(index: usize) -> String {
    format!("library-item-{index}")
}

/// The sheet for item canvases of the sizes given, in order: the white
/// fill, then per item its canvas (image [`item_image_id`]) and its
/// outline. `None` without items.
pub fn sheet_document(sizes: &[(u32, u32)]) -> Option<CanvasDocument> {
    let (width, height) = sheet_size(sizes.len())?;
    let mut list = DisplayList::new();
    list.push(DisplayItem::FillRect {
        rect: Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
        color: Color::new(SHEET_BACKGROUND),
    });
    for (index, &(w, h)) in sizes.iter().enumerate() {
        let (x, y) = item_origin(index, w, h);
        list.push(DisplayItem::Image(ImageItem::new(
            item_image_id(index),
            Rect::new(x, y, f64::from(w), f64::from(h)),
        )));
        let border = border_rect(index);
        list.push(DisplayItem::Stroke {
            path: Path::rect(border.x, border.y, border.width, border.height),
            stroke: Stroke::new(Color::new(BORDER_COLOR), BORDER_WIDTH),
        });
    }
    Some(CanvasDocument {
        width,
        height,
        list,
        payload: None,
    })
}

/// `restoreElements(elements, null, {deleteInvisibleElements: true})` and
/// `getNonDeletedElements`, as the utils `exportToCanvas` prepares what it
/// is given.
pub fn restore_for_export(
    elements: &[Element],
    env: &mut dyn RestoreEnv,
) -> Result<Vec<Element>, Failure> {
    let raw: Vec<Value> = elements.iter().map(|e| Value::Object(e.to_map())).collect();
    let opts = RestoreElementsOptions {
        delete_invisible_elements: true,
        ..RestoreElementsOptions::default()
    };
    let restored =
        restore_elements(&raw, None, opts, env).map_err(|e| Failure::Invalid(e.to_string()))?;
    restored
        .into_iter()
        .map(|map| Element::from_restored(map).map_err(|e| Failure::Invalid(e.to_string())))
        .filter(|e| e.as_ref().map_or(true, |e| !e.base.is_deleted))
        .collect()
}

/// `restoreAppState(undefined, null)`: what the utils `exportToCanvas`
/// exports with when it is given no app state.
pub fn default_export_app_state() -> Result<Map<String, Value>, Failure> {
    restore_app_state(None, None, &AppStateEnv::default())
        .map(|state| state.into_map())
        .map_err(|e| Failure::Invalid(e.to_string()))
}

/// The utils `exportToCanvas({elements, files: null, maxWidthOrHeight:
/// BOX_SIZE})` of elements already restored for export.
pub fn item_document(
    elements: &[Element],
    app_state: &Map<String, Value>,
    fonts: &FontStore,
) -> CanvasDocument {
    let no_files = |_: &str| false;
    let options = CanvasExportOptions {
        export_background: truthy(app_state.get("exportBackground")),
        export_padding: None,
        view_background_color: app_state
            .get("viewBackgroundColor")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        exporting_frame: None,
        sizing: CanvasSizing::Utils {
            max_width_or_height: Some(BOX_SIZE),
            // `appState?.exportScale ?? 1` of the caller's (absent) state
            export_scale: None,
            get_dimensions: None,
        },
        text_metrics: fonts,
        image_loads: &no_files,
    };
    export_to_canvas(elements, app_state, &Map::new(), &options)
}

/// One item's canvas.
pub struct ItemCanvas {
    pub pixmap: Pixmap,
}

impl ItemCanvas {
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }
}

/// Item `item`'s canvas, painted. A canvas without pixels (which
/// `drawImage` refuses) or past the browser limits fails.
pub fn item_canvas(
    item: &LibraryItem,
    app_state: &Map<String, Value>,
    fonts: &FontStore,
    env: &mut dyn RestoreEnv,
) -> Result<ItemCanvas, Failure> {
    let elements = restore_for_export(&item.elements, env)?;
    let doc = item_document(&elements, app_state, fonts);
    let images: HashMap<String, Pixmap> = HashMap::new();
    let pixmap = paint_canvas(&doc, &images, &mut GlyphText::new(fonts)).map_err(|e| {
        let why = match e {
            PngExportError::CanvasTooBig if doc.width == 0 || doc.height == 0 => format!(
                "its canvas is {} x {} px, which cannot be drawn",
                doc.width, doc.height
            ),
            e => e.to_string(),
        };
        Failure::Export(format!("item {}: {why}", item.id))
    })?;
    Ok(ItemCanvas { pixmap })
}

/// The preview image of `canvases`, in order, painted.
pub fn sheet(canvases: &[ItemCanvas]) -> Result<Pixmap, Failure> {
    let sizes: Vec<(u32, u32)> = canvases.iter().map(|c| (c.width(), c.height())).collect();
    let doc = sheet_document(&sizes).ok_or_else(|| Failure::Export(NO_ITEMS.to_owned()))?;
    let images: HashMap<String, Pixmap> = canvases
        .iter()
        .enumerate()
        .map(|(i, c)| (item_image_id(i), c.pixmap.clone()))
        .collect();
    paint_canvas(&doc, &images, &mut NoText).map_err(|e| Failure::Export(e.to_string()))
}

/// The sheet draws no text.
struct NoText;

impl TextRasterizer for NoText {
    fn fill_text(
        &mut self,
        _: &mut Pixmap,
        _: &TextRun,
        _: tiny_skia::Color,
        _: tiny_skia::Transform,
        _: Option<&tiny_skia::Mask>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constants_are_upstreams_formulas() {
        assert_eq!(BOX_PADDING, (BOX_SIZE / 16.0).round());
        assert_eq!(BORDER_WIDTH, (BOX_SIZE / 64.0).round().max(2.0));
    }

    #[test]
    fn sheets_hold_rows_of_six() {
        assert_eq!(sheet_size(0), None);
        assert_eq!(sheet_size(1), Some((144, 144)));
        assert_eq!(sheet_size(6), Some((864, 144)));
        assert_eq!(sheet_size(7), Some((864, 288)));
        assert_eq!(sheet_size(249), Some((864, 6048)));
    }

    #[test]
    fn items_are_centred_in_their_boxes() {
        assert_eq!(item_origin(0, 128, 128), (8.0, 8.0));
        assert_eq!(item_origin(0, 70, 50), (37.0, 47.0));
        assert_eq!(item_origin(0, 71, 50), (36.5, 47.0));
        assert_eq!(item_origin(7, 128, 128), (152.0, 152.0));
        assert_eq!(border_rect(0), Rect::new(4.0, 4.0, 136.0, 136.0));
        assert_eq!(border_rect(6), Rect::new(4.0, 148.0, 136.0, 136.0));
    }

    #[test]
    fn the_sheet_is_the_fill_then_each_image_and_outline() {
        let doc = sheet_document(&[(70, 50), (128, 20)]).unwrap();
        assert_eq!((doc.width, doc.height), (288, 144));
        assert_eq!(doc.list.len(), 5);
        let DisplayItem::Image(second) = &doc.list.items[3] else {
            panic!("{:?}", doc.list.items[3]);
        };
        assert_eq!(second.id, item_image_id(1));
        assert_eq!(second.dest, Rect::new(152.0, 62.0, 128.0, 20.0));
        assert!(second.smoothing);
        assert!(sheet_document(&[]).is_none());
    }

    #[test]
    fn export_restores_drop_invisible_elements() {
        let mut env = crate::env::CliEnv::new(0.0, 1);
        let mut tiny = serde_json::json!({"type": "rectangle", "id": "a", "x": 0, "y": 0,
            "width": 0, "height": 0});
        let visible = serde_json::json!({"type": "rectangle", "id": "b", "x": 0, "y": 0,
            "width": 10, "height": 10});
        tiny["isDeleted"] = Value::Bool(false);
        let elements: Vec<Element> = [tiny, visible]
            .into_iter()
            .map(|v| Element::from_restored(v.as_object().unwrap().clone()).unwrap())
            .collect();
        let kept = restore_for_export(&elements, &mut env).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].base.id, "b");
    }

    #[test]
    fn the_default_app_state_exports_on_white() {
        let state = default_export_app_state().unwrap();
        assert_eq!(state["exportBackground"], Value::Bool(true));
        assert_eq!(state["viewBackgroundColor"], "#ffffff");
    }
}
