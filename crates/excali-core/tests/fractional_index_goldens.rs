//! Exact key strings: the port against upstream's own output.
//!
//! `goldens/fractional-indexing.json` and `goldens/fractional-index.json`
//! are written by `tools/goldens/generate.mjs`, which runs the pinned
//! checkout's `packages/fractional-indexing/src/index.ts` and
//! `packages/element/src/fractionalIndex.ts` under Node (inputs in
//! `tools/goldens/fixtures-fractional.mjs`). CI regenerates them and fails
//! when they are stale (`goldens` job in `.github/workflows/gates.yml`).

use excali_core::element::{
    ArrowFields, BoundElement, BoundElementType, Element, ElementBase, ElementKind, FontFamily,
    FractionalIndex, LineFields, LinearFields, StickyNoteFields, TextFields,
};
use excali_core::fractional_index::{
    order_by_fractional_index, sync_invalid_indices, sync_invalid_indices_immutable,
    sync_moved_indices, validate_fractional_indices, ChangeStamp,
};
use excali_core::order_key::{
    generate_key_between, generate_key_between_with, generate_n_keys_between,
    generate_n_keys_between_with, validate_order_key,
};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

fn golden(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../goldens")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run node tools/goldens/generate.mjs)",
            path.display()
        )
    });
    let doc: Value = serde_json::from_str(&text).expect("golden JSON");
    doc["cases"].as_array().expect("cases").clone()
}

fn opt_str(v: &Value) -> Option<&str> {
    v.as_str()
}

fn str_list(v: &Value) -> Vec<String> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|s| s.as_str().expect("string").to_owned())
        .collect()
}

/// `result` / `error` of a case, as Ok / Err(message).
fn expected<T>(case: &Value, result: impl Fn(&Value) -> T) -> Result<T, String> {
    match case.get("error") {
        Some(e) => Err(e.as_str().expect("message").to_owned()),
        None => Ok(result(&case["result"])),
    }
}

#[test]
fn fractional_indexing_matches_upstream() {
    let cases = golden("fractional-indexing.json");
    assert!(cases.len() > 5000);
    let mut checked = 0;
    for case in &cases {
        let id = case["id"].as_str().unwrap();
        let digits = case.get("digits").and_then(Value::as_str);
        match case["fn"].as_str().unwrap() {
            "validateOrderKey" => {
                let got = validate_order_key(case["key"].as_str().unwrap());
                assert_eq!(got.is_ok(), case["valid"].as_bool().unwrap(), "{id}");
                if let Err(e) = got {
                    assert_eq!(e.to_string(), case["error"].as_str().unwrap(), "{id}");
                }
            }
            "generateKeyBetween" => {
                let (a, b) = (opt_str(&case["a"]), opt_str(&case["b"]));
                let got = match digits {
                    Some(d) => generate_key_between_with(a, b, d),
                    None => generate_key_between(a, b),
                };
                let want = expected(case, |r| r.as_str().unwrap().to_owned());
                assert_eq!(got.map_err(|e| e.to_string()), want, "{id}");
            }
            "generateNKeysBetween" => {
                let (a, b) = (opt_str(&case["a"]), opt_str(&case["b"]));
                let n = usize::try_from(case["n"].as_u64().unwrap()).unwrap();
                let got = match digits {
                    Some(d) => generate_n_keys_between_with(a, b, n, d),
                    None => generate_n_keys_between(a, b, n),
                };
                let want = expected(case, str_list);
                assert_eq!(got.map_err(|e| e.to_string()), want, "{id}");
            }
            "walk" => {
                let mut sorted: Vec<String> = Vec::new();
                let mut keys = Vec::new();
                for p in case["picks"].as_array().unwrap() {
                    let p = usize::try_from(p.as_u64().unwrap()).unwrap();
                    let a = p.checked_sub(1).map(|i| sorted[i].as_str());
                    let b = sorted.get(p).map(String::as_str);
                    let key = generate_key_between(a, b).unwrap();
                    sorted.insert(p, key.clone());
                    keys.push(key);
                }
                assert_eq!(keys, str_list(&case["keys"]), "{id}");
            }
            other => panic!("unknown case kind {other}"),
        }
        checked += 1;
    }
    assert_eq!(checked, cases.len());
}

struct Stamp;

impl ChangeStamp for Stamp {
    fn version_nonce(&mut self) -> f64 {
        7.0
    }
    fn updated(&mut self) -> f64 {
        2.0
    }
}

/// The golden's element, with the fields fractionalIndex.ts reads.
fn element(v: &Value) -> Element {
    let id = v["id"].as_str().unwrap();
    let mut base = ElementBase::new(id, 0.0, 0.0, 1.0, 1.0);
    base.index = v["index"].as_str().map(|s| FractionalIndex(s.to_owned()));
    if let Some(version) = v.get("version") {
        base.version = version.as_f64().unwrap();
    }
    if let Some(nonce) = v.get("versionNonce") {
        base.version_nonce = nonce.as_f64().unwrap();
    }
    base.is_deleted = v.get("isDeleted").and_then(Value::as_bool).unwrap_or(false);
    base.locked = v.get("locked").and_then(Value::as_bool).unwrap_or(false);
    base.bound_elements = v
        .get("boundElements")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|b| BoundElement {
                    id: b["id"].as_str().unwrap().to_owned(),
                    kind: match b["type"].as_str().unwrap() {
                        "text" => BoundElementType::Text,
                        _ => BoundElementType::Arrow,
                    },
                })
                .collect()
        });
    let linear = || LinearFields::new(vec![[0.0, 0.0], [10.0, 10.0]]);
    let kind = match v.get("type").and_then(Value::as_str).unwrap_or("rectangle") {
        "rectangle" => ElementKind::Rectangle,
        "diamond" => ElementKind::Diamond,
        "ellipse" => ElementKind::Ellipse,
        "stickynote" => ElementKind::StickyNote(StickyNoteFields { base_height: 0.0 }),
        "arrow" => ElementKind::Arrow(ArrowFields::new(linear(), false)),
        "line" => ElementKind::Line(LineFields {
            linear: linear(),
            polygon: false,
        }),
        "text" => {
            let mut text = TextFields::new("label", FontFamily::default(), 1.25);
            text.container_id = v["containerId"].as_str().map(str::to_owned);
            ElementKind::Text(text)
        }
        other => panic!("no fixture kind for {other}"),
    };
    Element::new(base, kind)
}

fn indices(list: &[Element]) -> Vec<Option<String>> {
    list.iter()
        .map(|e| e.base.index.as_ref().map(|i| i.0.clone()))
        .collect()
}

#[test]
fn fractional_index_matches_upstream() {
    let cases = golden("fractional-index.json");
    let mut counts = [0usize; 5];
    for case in &cases {
        let id = case["id"].as_str().unwrap();
        let input: Vec<Element> = case["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(element)
            .collect();
        match case["fn"].as_str().unwrap() {
            f @ ("syncInvalidIndices" | "syncMovedIndices") => {
                assert_eq!(
                    validate_fractional_indices(&input, true).is_ok(),
                    case["validInput"].as_bool().unwrap(),
                    "{id}: validInput"
                );
                let mut synced = input.clone();
                let result = if f == "syncInvalidIndices" {
                    sync_invalid_indices(&mut synced, &mut Stamp)
                } else {
                    let moved: HashSet<String> = str_list(&case["moved"]).into_iter().collect();
                    sync_moved_indices(&mut synced, &moved, &mut Stamp)
                };
                if let Some(error) = case.get("error") {
                    assert_eq!(
                        result.unwrap_err().to_string(),
                        error.as_str().unwrap(),
                        "{id}"
                    );
                    assert_eq!(synced, input, "{id}: nothing mutated on error");
                    continue;
                }
                result.unwrap_or_else(|e| panic!("{id}: {e}"));
                let want_indices: Vec<Option<String>> = case["indices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().map(str::to_owned))
                    .collect();
                assert_eq!(indices(&synced), want_indices, "{id}: indices");
                let versions: Vec<f64> = synced.iter().map(|e| e.base.version).collect();
                let want_versions: Vec<f64> = case["versions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect();
                assert_eq!(versions, want_versions, "{id}: versions");
                assert_eq!(
                    validate_fractional_indices(&synced, true).is_ok(),
                    case["validOutput"].as_bool().unwrap(),
                    "{id}: validOutput"
                );
                for (after, before) in synced.iter().zip(&input) {
                    if after.base.version != before.base.version {
                        assert_eq!(after.base.version_nonce, 7.0, "{id}");
                        assert_eq!(after.base.updated, 2.0, "{id}");
                    } else {
                        assert_eq!(after, before, "{id}: untouched element");
                    }
                }
                counts[0] += 1;
            }
            "syncInvalidIndicesImmutable" => {
                counts[4] += 1;
                let result = sync_invalid_indices_immutable(&input, &mut Stamp);
                if let Some(error) = case.get("error") {
                    assert_eq!(
                        result.unwrap_err().to_string(),
                        error.as_str().unwrap(),
                        "{id}"
                    );
                    counts[3] += 1;
                    continue;
                }
                let map = result.unwrap_or_else(|e| panic!("{id}: {e}"));
                let want = case["entries"].as_array().unwrap();
                assert_eq!(map.len(), want.len(), "{id}: size");
                for ((key, got), entry) in map.iter().zip(want) {
                    let entry = entry.as_array().unwrap();
                    let from = entry[1].as_u64().unwrap() as usize;
                    assert_eq!(key, entry[0].as_str().unwrap(), "{id}: key order");
                    let source = &input[from];
                    assert_eq!(got.base.id, source.base.id, "{id}: {key}");
                    assert_eq!(
                        got.base.index.as_ref().map(|i| i.0.as_str()),
                        entry[2].as_str(),
                        "{id}: {key} index"
                    );
                    assert_eq!(got.base.version, entry[3].as_f64().unwrap(), "{id}: {key}");
                    if got.base.version == source.base.version {
                        assert_eq!(got, source, "{id}: {key} is input element {from}");
                    } else {
                        // the copy of input element `from`, restamped
                        let mut expected = source.clone();
                        expected.base.index = got.base.index.clone();
                        expected.base.version = got.base.version;
                        expected.base.version_nonce = 7.0;
                        expected.base.updated = 2.0;
                        assert_eq!(got, &expected, "{id}: {key} copies input element {from}");
                    }
                }
            }
            "validateFractionalIndices" => {
                let include = case["includeBoundTextValidation"].as_bool().unwrap();
                let want = str_list(&case["messages"]);
                match validate_fractional_indices(&input, include) {
                    Ok(()) => assert!(want.is_empty(), "{id}: expected {want:?}"),
                    Err(e) => assert_eq!(e.messages, want, "{id}"),
                }
                counts[1] += 1;
            }
            "orderByFractionalIndex" => {
                // tag each element with its input position (the sort never
                // reads `seed`), so equal ids stay distinguishable
                let mut ordered = input.clone();
                for (i, e) in ordered.iter_mut().enumerate() {
                    e.base.seed = i as f64;
                }
                order_by_fractional_index(&mut ordered);
                let got: Vec<u64> = ordered.iter().map(|e| e.base.seed as u64).collect();
                let want: Vec<u64> = case["order"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
                    .collect();
                assert_eq!(got, want, "{id}");
                counts[2] += 1;
            }
            other => panic!("unknown case kind {other}"),
        }
        counts[3] += 1;
    }
    assert_eq!(counts[3], cases.len());
    assert!(counts[0] >= 800, "sync cases {}", counts[0]);
    assert!(counts[1] >= 40, "validate cases {}", counts[1]);
    assert!(counts[2] >= 120, "order cases {}", counts[2]);
    assert!(counts[4] >= 200, "immutable sync cases {}", counts[4]);
}
