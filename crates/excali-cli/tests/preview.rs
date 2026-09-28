//! `excali lib preview`: the library preview image the publish dialog
//! generates (`generatePreviewImage`,
//! `packages/excalidraw/components/PublishLibrary.tsx:38-105`), each item
//! drawn by the utils `exportToCanvas` with `maxWidthOrHeight: 128`
//! (`packages/utils/src/export.ts:43-104`).

mod support;

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use support::*;

/// generatePreviewImage's constants.
const MAX_ITEMS_PER_ROW: u32 = 6;
const BOX_SIZE: f64 = 128.0;
/// Math.round(BOX_SIZE / 16)
const BOX_PADDING: f64 = 8.0;

fn rectangle(id: &str, width: f64, height: f64) -> Value {
    json!({
        "type": "rectangle",
        "id": id,
        "x": 0,
        "y": 0,
        "width": width,
        "height": height,
        "strokeColor": "#1e1e1e",
        "backgroundColor": "transparent",
        "fillStyle": "solid",
        "strokeWidth": 4,
        "roughness": 0,
        "opacity": 100,
        "seed": 1,
        "version": 1,
        "versionNonce": 1,
    })
}

fn library(dir: &Path, name: &str, items: &[Value]) -> PathBuf {
    let path = dir.join(name);
    let items: Vec<Value> = items
        .iter()
        .enumerate()
        .map(|(i, elements)| {
            json!({
                "id": format!("item-{i}"),
                "status": "published",
                "created": 1,
                "name": format!("Item {i}"),
                "elements": elements,
            })
        })
        .collect();
    let doc = json!({
        "type": "excalidrawlib",
        "version": 2,
        "source": "https://excalidraw.com",
        "libraryItems": items,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    path
}

/// The sheet's canvas size for `n` items (`rows[0].length` is the first
/// row's).
fn sheet_size(n: u32) -> (u32, u32) {
    let columns = f64::from(n.min(MAX_ITEMS_PER_ROW));
    let rows = f64::from(n.div_ceil(MAX_ITEMS_PER_ROW));
    let side = |k: f64| (k * BOX_SIZE + (k + 1.0) * (BOX_PADDING * 2.0) - BOX_PADDING * 2.0) as u32;
    (side(columns), side(rows))
}

fn preview(args: &[&std::ffi::OsStr]) -> std::process::Output {
    let mut all: Vec<&std::ffi::OsStr> = vec!["lib".as_ref(), "preview".as_ref()];
    all.extend_from_slice(args);
    excali(&all)
}

#[test]
fn the_sheet_has_upstreams_layout() {
    for (n, size) in [
        (1, (144, 144)),
        (6, (864, 144)),
        (7, (864, 288)),
        (13, (864, 432)),
    ] {
        assert_eq!(sheet_size(n), size, "{n} items");
    }
}

#[test]
fn a_small_item_is_drawn_at_scale_one_centred_in_its_box() {
    let dir = scratch("preview-small");
    let lib = library(
        &dir,
        "one.excalidrawlib",
        &[json!([rectangle("r", 50.0, 30.0)])],
    );
    let sheet = dir.join("sheet.png");
    let items = dir.join("items");
    let out = preview(&[
        lib.as_os_str(),
        "-o".as_ref(),
        sheet.as_os_str(),
        "--items".as_ref(),
        items.as_os_str(),
        "--json".as_ref(),
    ]);
    assert_code(&out, 0);
    let report: Value = serde_json::from_str(&stdout(&out)).unwrap();
    // The content (50 x 30) with the default export padding (10) on each
    // side, smaller than 128: the caller's scale, 1.
    assert_eq!(
        report,
        json!({
            "items": [{
                "index": 0,
                "id": "item-0",
                "name": "Item 0",
                "width": 70,
                "height": 50,
                "file": items.join("item-0001.png").display().to_string(),
            }],
            "width": 144,
            "height": 144,
        })
    );

    let png = read_png(&std::fs::read(&sheet).unwrap());
    assert_eq!((png.width, png.height), (144, 144));
    // ctx.fillStyle = "#fff"; fillRect over the canvas
    assert_eq!(png.pixel(0, 0), [255, 255, 255, 255]);
    assert_eq!(png.pixel(143, 143), [255, 255, 255, 255]);
    // strokeRect(4, 4, 136, 136) with lineWidth 2 in #ced4da: x 3..5
    for (x, y) in [(3, 72), (4, 72), (139, 72), (140, 72), (72, 3), (72, 140)] {
        assert_eq!(png.pixel(x, y), [0xce, 0xd4, 0xda, 255], "({x}, {y})");
    }
    assert_eq!(png.pixel(6, 72), [255, 255, 255, 255]);

    // The item's canvas, drawn at (0 + (128 - 70) / 2 + 8, 0 + (128 - 50)
    // / 2 + 8) = (37, 47): whole pixels, so its pixels are copied as they are.
    let item = read_png(&std::fs::read(items.join("item-0001.png")).unwrap());
    assert_eq!((item.width, item.height), (70, 50));
    for y in 0..50 {
        for x in 0..70 {
            assert_eq!(png.pixel(37 + x, 47 + y), item.pixel(x, y), "({x}, {y})");
        }
    }
    // exportBackground and viewBackgroundColor of restoreAppState(undefined):
    // white, and the rectangle's 4 px stroke along x = 10.
    assert_eq!(item.pixel(0, 0), [255, 255, 255, 255]);
    assert_eq!(item.pixel(35, 25), [255, 255, 255, 255]);
    let edge = item.pixel(10, 25);
    assert!(edge[0] < 64 && edge[3] == 255, "{edge:?}");
}

#[test]
fn a_large_item_is_scaled_to_fit_its_box() {
    let dir = scratch("preview-large");
    let lib = library(
        &dir,
        "big.excalidrawlib",
        &[json!([rectangle("r", 400.0, 200.0)])],
    );
    let sheet = dir.join("sheet.png");
    let out = preview(&[
        lib.as_os_str(),
        "-o".as_ref(),
        sheet.as_os_str(),
        "--json".as_ref(),
    ]);
    assert_code(&out, 0);
    let report: Value = serde_json::from_str(&stdout(&out)).unwrap();
    // 420 x 220 with the padding; maxWidthOrHeight < max, so the scale is
    // 128 / 420 and the canvas the size times it, truncated as
    // `canvas.width = value` truncates.
    let scale = BOX_SIZE / 420.0;
    let item = &report["items"][0];
    assert_eq!(item["width"], json!((420.0 * scale) as u32));
    assert_eq!(item["height"], json!((220.0 * scale) as u32));
    assert!(item.get("file").is_none());
    assert_eq!(read_png(&std::fs::read(&sheet).unwrap()).width, 144);
}

#[test]
fn items_fill_rows_of_six() {
    let dir = scratch("preview-rows");
    let items: Vec<Value> = (0..7)
        .map(|i| {
            json!([rectangle(
                &format!("r{i}"),
                20.0 + 10.0 * f64::from(i),
                20.0
            )])
        })
        .collect();
    let lib = library(&dir, "seven.excalidrawlib", &items);
    let sheet = dir.join("sheet.png");
    let out = preview(&[
        lib.as_os_str(),
        "-o".as_ref(),
        sheet.as_os_str(),
        "--json".as_ref(),
    ]);
    assert_code(&out, 0);
    let report: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(report["items"].as_array().unwrap().len(), 7);
    assert_eq!(
        (report["width"].clone(), report["height"].clone()),
        (json!(864), json!(288))
    );
    for (i, item) in report["items"].as_array().unwrap().iter().enumerate() {
        assert_eq!(item["index"], json!(i));
        assert_eq!(item["width"], json!(20 + 10 * i + 20));
        assert_eq!(item["height"], json!(40));
    }
    let png = read_png(&std::fs::read(&sheet).unwrap());
    assert_eq!((png.width, png.height), (864, 288));
    let border = [0xce, 0xd4, 0xda, 255];
    // The seventh item's box: row 1, column 0, border at (4, 148).
    assert_eq!(png.pixel(72, 148), border);
    assert_eq!(png.pixel(72, 283), border);
    // The sixth: row 0, column 5, border at (724, 4).
    assert_eq!(png.pixel(724, 72), border);
    // No seventh box beside it in row 1.
    assert_eq!(png.pixel(148, 200), [255, 255, 255, 255]);
    assert_eq!(png.pixel(724, 200), [255, 255, 255, 255]);
}

#[test]
fn the_upstream_fixture_library_previews() {
    let dir = scratch("preview-fixture");
    let sheet = dir.join("sheet.png");
    let out = preview(&[
        upstream_fixture("fixture_library.excalidrawlib").as_os_str(),
        "-o".as_ref(),
        sheet.as_os_str(),
        "--json".as_ref(),
    ]);
    assert_code(&out, 0);
    let report: Value = serde_json::from_str(&stdout(&out)).unwrap();
    let n = report["items"].as_array().unwrap().len() as u32;
    assert!(n > 0);
    let png = read_png(&std::fs::read(&sheet).unwrap());
    assert_eq!((png.width, png.height), sheet_size(n));
}

#[test]
fn a_plain_line_is_written_per_item_without_json() {
    let dir = scratch("preview-plain");
    let lib = library(
        &dir,
        "one.excalidrawlib",
        &[json!([rectangle("r", 50.0, 30.0)])],
    );
    let sheet = dir.join("sheet.png");
    let out = preview(&[lib.as_os_str(), "-o".as_ref(), sheet.as_os_str()]);
    assert_code(&out, 0);
    assert_eq!(stdout(&out), "item-0\t70x50\tItem 0\n");
}

#[test]
fn an_empty_library_has_no_preview() {
    let dir = scratch("preview-empty");
    let lib = library(&dir, "empty.excalidrawlib", &[]);
    let sheet = dir.join("sheet.png");
    let out = preview(&[lib.as_os_str(), "-o".as_ref(), sheet.as_os_str()]);
    assert_code(&out, 4);
    assert!(stderr(&out).contains("no items"), "{}", stderr(&out));
    assert!(!sheet.exists());
}

#[test]
fn a_scene_is_not_a_library() {
    let dir = scratch("preview-scene");
    let scene = scene_file(
        &dir,
        "a.excalidraw",
        &json!([rectangle("r", 10.0, 10.0)]),
        &json!({}),
        &json!({}),
    );
    let out = preview(&[
        scene.as_os_str(),
        "-o".as_ref(),
        dir.join("s.png").as_os_str(),
    ]);
    assert_code(&out, 1);
}

#[test]
fn the_output_is_required() {
    let dir = scratch("preview-usage");
    let lib = library(
        &dir,
        "one.excalidrawlib",
        &[json!([rectangle("r", 5.0, 5.0)])],
    );
    assert_code(&preview(&[lib.as_os_str()]), 2);
}
