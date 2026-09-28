//! The fixture corpus through the command line (ex-409's acceptance: the
//! corpus tests use the CLI).
//!
//! - every catalogue library of `fixtures/libraries` (the 232 of
//!   `fixtures/manifest.json`, ex-003) validates as a library with
//!   `excali validate`, lists with `excali lib list` and merges into one
//!   library with `excali lib merge`, which validates again;
//! - every upstream test fixture that holds a scene or a library validates,
//!   the three plain images (two PNGs, one SVG) are rejected as upstream rejects them
//!   ("Image doesn't contain scene"), and every scene renders to a PNG and
//!   an SVG that validate again (upstream's re-import of its own export);
//! - every item of every catalogue library renders to PNG (ex-410):
//!   `excali lib preview` draws each library's items onto the publish
//!   dialog's preview image, and every item's canvas has pixels and fits
//!   the 128 px box.

mod support;

use std::path::PathBuf;

use serde_json::Value;
use support::*;

fn catalogue() -> Vec<PathBuf> {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("fixtures/manifest.json")).unwrap(),
    )
    .unwrap();
    manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["source"] == "libraries")
        .filter_map(|e| e["path"].as_str())
        .filter(|p| p.ends_with(".excalidrawlib.gz"))
        .map(|p| repo().join("fixtures").join(p))
        .collect()
}

#[test]
fn every_catalogue_library_validates_lists_and_merges() {
    let libraries = catalogue();
    assert_eq!(libraries.len(), 232);

    let mut args: Vec<std::ffi::OsString> = vec!["validate".into(), "--json".into()];
    args.extend(libraries.iter().map(|p| p.clone().into_os_string()));
    let out = excali(&args);
    assert_code(&out, 0);
    let report: Value = serde_json::from_str(&stdout(&out)).unwrap();
    let report = report.as_array().unwrap();
    assert_eq!(report.len(), libraries.len());
    let mut total_items = 0;
    for (entry, path) in report.iter().zip(&libraries) {
        assert_eq!(entry["kind"], "library", "{}: {entry}", path.display());
        let items = entry["items"].as_u64().unwrap();
        assert!(items > 0, "{}", path.display());
        total_items += items;

        let listed = excali(&[
            "lib".as_ref(),
            "list".as_ref(),
            "--json".as_ref(),
            path.as_os_str(),
        ]);
        assert_code(&listed, 0);
        let listed: Value = serde_json::from_str(&stdout(&listed)).unwrap();
        assert_eq!(
            listed.as_array().unwrap().len() as u64,
            items,
            "{}",
            path.display()
        );
    }

    let dir = scratch("corpus-merge");
    let merged = dir.join("catalogue.excalidrawlib");
    let mut args: Vec<std::ffi::OsString> = vec!["lib".into(), "merge".into()];
    args.extend(libraries.iter().map(|p| p.clone().into_os_string()));
    args.extend(["-o".into(), merged.clone().into_os_string()]);
    assert_code(&excali(&args), 0);
    let out = excali(&["validate".as_ref(), "--json".as_ref(), merged.as_os_str()]);
    assert_code(&out, 0);
    let report: Value = serde_json::from_str(&stdout(&out)).unwrap();
    // The catalogue as downloaded merges too: its items get fresh nonces
    // as they are restored, so few are found repeated.
    let merged_items = report[0]["items"].as_u64().unwrap();
    assert!(merged_items > 0 && merged_items <= total_items);

    // The expected merge, computed apart from the CLI's merge: each
    // library's restored items, folded with upstream's mergeLibraryItems and
    // isUniqueItem (`packages/excalidraw/data/library.ts:120-157`).
    //
    // Restoring gives elements without a valid fractional index one
    // through mutateElement (syncInvalidIndices, `data/restore.ts:965`),
    // which draws a fresh versionNonce; most catalogue items have no
    // indices, so two restores of them differ. Each library is therefore
    // restored once (`lib merge <file>` writes it alone), and the restored
    // files, whose indices are valid and whose nonces restore unchanged,
    // are what is merged and folded.
    let mut restored = Vec::new();
    let mut expected: Vec<ItemKey> = Vec::new();
    let mut alone_items: Vec<Vec<ItemKey>> = Vec::new();
    for (i, path) in libraries.iter().enumerate() {
        let alone = dir.join(format!("alone-{i}.excalidrawlib"));
        assert_code(
            &excali(&[
                "lib".as_ref(),
                "merge".as_ref(),
                path.as_os_str(),
                "-o".as_ref(),
                alone.as_os_str(),
            ]),
            0,
        );
        let items = item_keys(&alone);
        assert!(!items.is_empty(), "{}", path.display());
        expected = if i == 0 {
            items.clone()
        } else {
            merge_library_items(&expected, &items)
        };
        alone_items.push(items);
        restored.push(alone);
    }
    let merged = dir.join("restored.excalidrawlib");
    let mut args: Vec<std::ffi::OsString> = vec!["lib".into(), "merge".into()];
    args.extend(restored.iter().map(|p| p.clone().into_os_string()));
    args.extend(["-o".into(), merged.clone().into_os_string()]);
    assert_code(&excali(&args), 0);
    let merged = item_keys(&merged);
    let alone_total: usize = alone_items.iter().map(Vec::len).sum();
    assert_eq!(alone_total as u64, total_items);
    assert_eq!(merged.len(), expected.len());
    assert_eq!(merged, expected);
    // Items repeated across libraries (the catalogue has some) are merged
    // once, so fewer come out than went in; none is lost.
    assert!(
        merged.len() < alone_total,
        "{} of {alone_total}",
        merged.len()
    );
    for (path, items) in libraries.iter().zip(&alone_items) {
        for item in items {
            assert!(
                merged.iter().any(|m| m.elements == item.elements),
                "{}: {item:?} missing from the merge",
                path.display()
            );
        }
    }
}

/// What a library item is to mergeLibraryItems: its elements' ids and
/// versionNonces in order; the name and status ride along to tell items
/// apart in a failure.
#[derive(Clone, Debug, PartialEq)]
struct ItemKey {
    name: Value,
    status: Value,
    elements: Vec<(String, i64)>,
}

fn item_keys(path: &std::path::Path) -> Vec<ItemKey> {
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(doc["type"], "excalidrawlib");
    doc["libraryItems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| ItemKey {
            name: item["name"].clone(),
            status: item["status"].clone(),
            elements: item["elements"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    (
                        e["id"].as_str().unwrap().to_owned(),
                        e["versionNonce"].as_i64().unwrap(),
                    )
                })
                .collect(),
        })
        .collect()
}

/// isUniqueItem: no existing item has the same number of elements with the
/// same ids and versionNonces at each index.
fn is_unique_item(existing: &[ItemKey], target: &ItemKey) -> bool {
    !existing.iter().any(|item| item.elements == target.elements)
}

/// mergeLibraryItems: the other items not already local, then the local
/// ones.
fn merge_library_items(local: &[ItemKey], other: &[ItemKey]) -> Vec<ItemKey> {
    let mut merged: Vec<ItemKey> = other
        .iter()
        .filter(|item| is_unique_item(local, item))
        .cloned()
        .collect();
    merged.extend(local.iter().cloned());
    merged
}

#[test]
fn upstream_fixtures_validate_as_upstream_loads_them() {
    let scenes = [
        "test_embedded_v1.png",
        "smiley_embedded_v2.png",
        "test_embedded_v1.svg",
        "smiley_embedded_v2.svg",
    ];
    for name in scenes {
        let out = excali(&["validate".as_ref(), upstream_fixture(name).as_os_str()]);
        assert_code(&out, 0);
    }
    let out = excali(&[
        "validate".as_ref(),
        upstream_fixture("fixture_library.excalidrawlib").as_os_str(),
    ]);
    assert_code(&out, 0);
    for name in [
        "smiley.png",
        "deer.png",
        "svg-image-exporting-reference.svg",
    ] {
        let out = excali(&["validate".as_ref(), upstream_fixture(name).as_os_str()]);
        assert_code(&out, 1);
        assert!(
            stderr(&out).contains("Image doesn't contain scene"),
            "{name}"
        );
    }
}

#[test]
fn upstream_scenes_render_and_export_and_read_back() {
    let dir = scratch("corpus-render");
    for name in [
        "test_embedded_v1.png",
        "smiley_embedded_v2.png",
        "test_embedded_v1.svg",
        "smiley_embedded_v2.svg",
    ] {
        let stem = name.replace('.', "-");
        let png = dir.join(format!("{stem}.png"));
        let svg = dir.join(format!("{stem}.svg"));
        let input = upstream_fixture(name);
        for (command, output) in [("render", &png), ("export", &svg)] {
            let out = excali(&[
                command.as_ref(),
                input.as_os_str(),
                "--embed-scene".as_ref(),
                "-o".as_ref(),
                output.as_os_str(),
            ]);
            assert_code(&out, 0);
            let out = excali(&["validate".as_ref(), "--json".as_ref(), output.as_os_str()]);
            assert_code(&out, 0);
            let report: Value = serde_json::from_str(&stdout(&out)).unwrap();
            assert_eq!(
                report[0]["types"],
                serde_json::json!({"text": 1}),
                "{name} {command}"
            );
        }
    }
}

#[test]
fn every_catalogue_item_renders_to_png() {
    let libraries = catalogue();
    assert_eq!(libraries.len(), 232);
    let dir = scratch("corpus-preview");
    let mut total = 0;
    for (i, path) in libraries.iter().enumerate() {
        let listed = excali(&[
            "lib".as_ref(),
            "list".as_ref(),
            "--json".as_ref(),
            path.as_os_str(),
        ]);
        assert_code(&listed, 0);
        let listed: Value = serde_json::from_str(&stdout(&listed)).unwrap();
        let listed = listed.as_array().unwrap();

        let sheet = dir.join(format!("{i}.png"));
        let out = excali(&[
            "lib".as_ref(),
            "preview".as_ref(),
            path.as_os_str(),
            "-o".as_ref(),
            sheet.as_os_str(),
            "--json".as_ref(),
        ]);
        assert_code(&out, 0);
        let report: Value = serde_json::from_str(&stdout(&out)).unwrap();
        let items = report["items"].as_array().unwrap();
        assert_eq!(items.len(), listed.len(), "{}", path.display());
        for (item, entry) in items.iter().zip(listed) {
            assert_eq!(item["id"], entry["id"], "{}", path.display());
            let (w, h) = (
                item["width"].as_u64().unwrap(),
                item["height"].as_u64().unwrap(),
            );
            assert!(
                (1..=128).contains(&w) && (1..=128).contains(&h),
                "{} {}: {w} x {h}",
                path.display(),
                item["id"]
            );
        }
        total += items.len();

        // Each box is 128 px with 8 px of padding on both sides, less the
        // padding outside the outer boxes: 144 px per column and row.
        let png = read_png(&std::fs::read(&sheet).unwrap());
        assert_eq!(
            (png.width as usize, png.height as usize),
            (items.len().min(6) * 144, items.len().div_ceil(6) * 144),
            "{}",
            path.display()
        );
        let _ = std::fs::remove_file(&sheet);
    }
    assert_eq!(total, 4187);
}
