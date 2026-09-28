//! The harness reads every fixture display list the way excali-raster's
//! fixture test does, and every image the lists draw reaches the browser
//! backend: either the fixture supplies it or it is a built-in image
//! `WebCanvas` loads itself, so no fixture passes in the browser by
//! skipping a draw the raster backend makes.

use std::collections::BTreeSet;
use std::path::PathBuf;

use canvas2d_fixtures::{image_ids, load_fixture};
use excali_scene::display::builtin_image_by_id;
use serde_json::Value;

fn lists_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/excali-raster/tests/fixtures/display-lists")
}

fn fixtures() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(lists_dir())
        .expect("the raster fixtures exist")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| {
            (
                p.file_stem().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(&p).unwrap(),
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_fixture_loads_with_its_size_and_scale() {
    let all = fixtures();
    assert!(all.len() >= 29, "the raster fixtures: {}", all.len());
    for (name, text) in all {
        let v: Value = serde_json::from_str(&text).unwrap();
        let f = load_fixture(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(u64::from(f.width), v["width"].as_u64().unwrap(), "{name}");
        assert_eq!(u64::from(f.height), v["height"].as_u64().unwrap(), "{name}");
        let scale = v.get("scale").map_or(1.0, |s| s.as_f64().unwrap());
        assert_eq!(f.scale, scale, "{name}");
        assert!(!f.list.is_empty(), "{name}: the list draws something");
        assert_eq!(
            f.list.len(),
            v["items"].as_array().unwrap().len(),
            "{name}: one display item per fixture item"
        );
    }
}

#[test]
fn every_image_a_list_draws_is_supplied_or_built_in() {
    let mut drawn_images = 0;
    let mut unknown = Vec::new();
    for (name, text) in fixtures() {
        let v: Value = serde_json::from_str(&text).unwrap();
        let supplied: BTreeSet<String> = v
            .get("images")
            .and_then(Value::as_object)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        let f = load_fixture(&text).unwrap();
        for id in image_ids(&f.list) {
            if supplied.contains(&id) || builtin_image_by_id(&id).is_some() {
                drawn_images += 1;
            } else {
                unknown.push(format!("{name}: {id}"));
            }
        }
    }
    assert!(drawn_images > 0, "some fixtures draw images");
    // An id neither backend has draws nothing in either (images.json pins
    // that); any other unknown id is a fixture error.
    assert_eq!(unknown, ["images: missing"]);
}

#[test]
fn image_ids_are_collected_through_groups() {
    let f = load_fixture(
        r#"{"width": 2, "height": 2, "items": [
            {"type": "image", "id": "a", "dest": [0, 0, 1, 1]},
            {"type": "group", "items": [
                {"type": "group", "items": [{"type": "image", "id": "b", "dest": [0, 0, 1, 1]}]},
                {"type": "image", "id": "a", "dest": [0, 0, 1, 1]}
            ]}
        ]}"#,
    )
    .unwrap();
    assert_eq!(
        image_ids(&f.list),
        BTreeSet::from(["a".to_owned(), "b".to_owned()])
    );
}

#[test]
fn a_list_the_loader_cannot_read_is_an_error_naming_the_problem() {
    let cases = [
        ("not json", "expected ident"),
        (
            r#"{"height": 2, "items": []}"#,
            "width is a positive integer",
        ),
        (
            r#"{"width": 2, "height": 0, "items": []}"#,
            "height is a positive integer",
        ),
        (
            r#"{"width": 2, "height": 2, "items": [{"type": "text"}]}"#,
            r#"unknown item type "text""#,
        ),
        (
            r#"{"width": 2, "height": 2, "items": [{"type": "fill", "color": "red", "path": [["X", 1]]}]}"#,
            r#"unknown path call "X""#,
        ),
        (
            r#"{"width": 2, "height": 2, "items": [{"type": "fill", "color": "red", "path": [["L", 1]]}]}"#,
            "L takes 2 numbers",
        ),
        (
            r#"{"width": 2, "height": 2, "items": [{"type": "fill", "path": []}]}"#,
            "a fill has a colour",
        ),
        (
            r#"{"width": 2, "height": 2, "scale": "two", "items": []}"#,
            r#"not a number: "two""#,
        ),
    ];
    for (json, message) in cases {
        let err = load_fixture(json)
            .err()
            .unwrap_or_else(|| panic!("{json} loads"));
        assert!(err.contains(message), "{json}: {err:?} names {message:?}");
    }
}

#[test]
fn non_finite_spellings_reach_the_list() {
    let f = load_fixture(
        r#"{"width": 2, "height": 2, "scale": "Infinity", "items": [
            {"type": "fill", "color": "red", "path": [["M", "NaN", "-Infinity"]]}
        ]}"#,
    )
    .unwrap();
    assert_eq!(f.scale, f64::INFINITY);
    assert_eq!(
        format!("{:?}", f.list),
        format!(
            "{:?}",
            excali_scene::display::DisplayList::from_iter([
                excali_scene::display::DisplayItem::Fill {
                    path: {
                        let mut p = excali_scene::display::Path::new();
                        p.move_to(f64::NAN, f64::NEG_INFINITY);
                        p
                    },
                    color: excali_scene::display::Color::new("red"),
                    rule: excali_scene::display::FillRule::NonZero,
                }
            ])
        )
    );
}
