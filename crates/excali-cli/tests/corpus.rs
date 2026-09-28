//! The fixture corpus through the command line (ex-409's acceptance: the
//! corpus tests use the CLI).
//!
//! - every catalogue library of `fixtures/libraries` (the 232 of
//!   `fixtures/manifest.json`, ex-003) validates as a library with
//!   `excali validate`, lists with `excali lib list` and merges into one
//!   library with `excali lib merge`, which validates again;
//! - every upstream test fixture that holds a scene or a library validates,
//!   the two plain images are rejected as upstream rejects them
//!   ("Image doesn't contain scene"), and every scene renders to a PNG and
//!   an SVG that validate again (upstream's re-import of its own export).

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
    let merged_items = report[0]["items"].as_u64().unwrap();
    // Items repeated across libraries (same element ids and versionNonces)
    // are merged once.
    assert!(merged_items > 0 && merged_items <= total_items);
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
    for name in ["smiley.png", "deer.png"] {
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
