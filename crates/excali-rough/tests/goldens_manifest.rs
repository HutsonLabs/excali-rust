//! `goldens/manifest.json` (written by `tools/goldens/generate.mjs`) pins
//! every golden file by sha256 and case count, and names the upstream
//! commit it was generated from. The parity tests read those files, so a
//! golden edited by hand, a file left out of the manifest, or goldens from
//! another upstream commit than the site's pin fail here before any shape
//! is compared.

use std::path::{Path, PathBuf};

use excali_rough::goldens::Manifest;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest() -> Manifest {
    let path = root().join("goldens/manifest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Manifest::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn every_golden_file_matches_its_manifest_entry() {
    let manifest = manifest();
    let mut on_disk = Vec::new();
    for entry in std::fs::read_dir(root().join("goldens")).expect("goldens/") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 name")
            .to_owned();
        if !name.ends_with(".json") || name == "manifest.json" {
            continue;
        }
        let bytes = std::fs::read(&path).expect("golden reads");
        manifest
            .verify(&name, &bytes)
            .unwrap_or_else(|e| panic!("{e}"));
        on_disk.push(name);
    }
    on_disk.sort();
    let mut listed: Vec<String> = manifest.files.iter().map(|f| f.name.clone()).collect();
    listed.sort();
    assert_eq!(
        on_disk, listed,
        "goldens/ and goldens/manifest.json list different files"
    );
    assert!(listed.len() >= 20, "{listed:?}");
}

#[test]
fn the_goldens_come_from_the_pinned_upstream_commit() {
    let config =
        std::fs::read_to_string(root().join("site/config.toml")).expect("site/config.toml");
    let pin = config
        .lines()
        .find_map(|l| l.strip_prefix("upstream_commit = \""))
        .and_then(|l| l.strip_suffix('"'))
        .expect("upstream_commit in site/config.toml");
    assert_eq!(manifest().upstream_commit, pin);
}

#[test]
fn the_goldens_come_from_the_pinned_packages() {
    let packages = manifest().packages;
    assert_eq!(packages["roughjs"], "4.6.4");
    assert_eq!(packages["perfect-freehand"], "1.2.0");
    assert_eq!(packages["points-on-curve"], "1.0.1");
}
