//! The catalogue corpus (ex-114): every one of the 232 public libraries of
//! `fixtures/libraries`, as listed in `fixtures/manifest.json` (ex-003,
//! `excalidraw/excalidraw-libraries` at the commit the manifest pins),
//! parses, is written back with `serialize_library_as_json` and parses
//! again to the same items, element for element; the second write is byte
//! for byte the first.
//!
//! What a load loses is recorded in
//! `tests/fixtures/library-corpus-report.json`, which this test rebuilds
//! and compares: per library, the element keys the file holds that the
//! written library no longer has (the legacy fields restore migrates away,
//! `packages/excalidraw/data/restore.ts` at the pinned commit), the
//! elements dropped and why, the arrow bindings cleared, the legacy
//! bindings that need element geometry (ex-116), the element ids replaced
//! as duplicates, and the envelope keys the v2 envelope does not carry.
//! Run with `EXCALI_BLESS=1` to rewrite the report after a deliberate
//! change; the diff is the review.
//!
//! Whether the written bytes are upstream's own is `library.rs`'s
//! `catalogue_matches_upstream`; this test also checks the corpus it walks
//! is exactly the catalogue that golden covers.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use excali_core::element::Element;
use excali_core::library::{parse_library_json, serialize_library_as_json, LibraryItemStatus};
use excali_core::restore::{
    restore_element, ElementsMap, LegacyBinding, LegacyBindingRequest, RestoreEnv, RestoreOptions,
    TestEnv,
};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const REPORT: &str = "tests/fixtures/library-corpus-report.json";
/// `getExportSource()` for the written files; any value does.
const SOURCE: &str = "https://excalidraw.com";
/// The library whose 24 lines with `strokeWidth: "3"` the typed model drops
/// (ex-117).
const LOGIC_GATES: &str = "libraries/aarondiel/logic-gates.excalidrawlib.gz";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The catalogue entries of the manifest: `(path under fixtures/, text)`,
/// each checked against the manifest's digests.
fn catalogue() -> Vec<(String, String)> {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("fixtures/manifest.json")).expect("manifest"),
    )
    .expect("manifest parses");
    let mut files = Vec::new();
    for entry in manifest["files"].as_array().expect("files") {
        let path = entry["path"].as_str().expect("path");
        if entry["source"] != "libraries" || !path.ends_with(".excalidrawlib.gz") {
            continue;
        }
        let bytes = std::fs::read(repo().join("fixtures").join(path)).expect(path);
        assert_eq!(sha256(&bytes), entry["sha256"], "{path}");
        let mut text = Vec::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_end(&mut text)
            .expect("gzip");
        assert_eq!(sha256(&text), entry["content_sha256"], "{path}");
        files.push((path.to_owned(), String::from_utf8(text).expect("utf-8")));
    }
    files
}

const DESCRIPTION: &str = "What loading each catalogue library (fixtures/libraries; \
parseLibraryJSON with defaultStatus published in upstream's test mode) and writing it with \
serializeLibraryAsJSON loses: keys of the file absent from the written file, elements not \
written, arrow bindings written as null, legacy bindings that need element geometry, \
duplicate element ids replaced, and keys a second load adds. Every kind is explained under \
reasons. Regenerate with EXCALI_BLESS=1 cargo test -p excali-core --test library_corpus.";

/// Every kind of loss the report may list, with the upstream rule behind
/// it (`packages/excalidraw/data/*.ts` at the pinned commit). A loss the
/// table does not explain fails the test.
const REASONS: &[(&str, &str)] = &[
    (
        "envelope_keys_dropped.library",
        "v1 items are written under libraryItems: serializeLibraryAsJSON writes {type, \
         version: 2, source, libraryItems} only (json.ts:137-145)",
    ),
    (
        "element_keys_dropped.strokeSharpness",
        "legacy: read into roundness, then deleted (restore.ts:475-484, 511)",
    ),
    (
        "element_keys_dropped.boundElementIds",
        "legacy: read into boundElements as arrows, then deleted (restore.ts:486-488, 512)",
    ),
    (
        "element_keys_dropped.rawText",
        "legacy obsidian-excalidraw attribute of text elements, deleted (restore.ts:532-534)",
    ),
    (
        "elements_dropped.typed model cannot read it (ex-117)",
        "restore keeps a value of another JSON type (restore.ts:459 keeps strokeWidth \"3\") \
         and upstream loads the element; the typed model reads numbers only and drops it \
         (port gap, ex-117)",
    ),
    (
        "elements_dropped.deleted",
        "a library item keeps its non-deleted elements only (restore.ts:1374-1379)",
    ),
    (
        "elements_dropped.legacy selection element",
        "restoreElements filters type selection out (restore.ts:967-971)",
    ),
    (
        "elements_dropped.unknown type",
        "restoreElement returns null for a type it does not know (restore.ts:751)",
    ),
    (
        "elements_dropped.restore throws",
        "restoreElements drops an element restoreElement throws on (restore.ts:982-985)",
    ),
    (
        "elements_dropped.not an object",
        "restoreElement of a primitive has no type and gives null",
    ),
    (
        "bindings_cleared",
        "repairBinding gives null for a binding whose element is not in the item and for \
         a legacy binding whose migration fails (restore.ts:298-428); a line gets no \
         bindings (restore.ts:636-638)",
    ),
    (
        "legacy_bindings_without_geometry",
        "a binding without mode to an existing element is migrated with element geometry \
         (restore.ts:347-418); restore::TestEnv has none, so it is cleared, counted in \
         bindings_cleared (port gap, ex-116)",
    ),
    (
        "element_ids_replaced",
        "an id repeated within an item gets randomId() (restore.ts:1000-1003)",
    ),
    (
        "element_keys_added_on_reload.line.polygon",
        "a legacy draw is restored to line without polygon (isLineElement is false for \
         draw, restore.ts:645-651); the next load reads a line and adds polygon: false",
    ),
];

/// Every kind of loss in `losses`, as the keys of [`REASONS`] name them.
fn kinds(losses: &Losses) -> Vec<String> {
    let mut kinds = Vec::new();
    for (name, map) in [
        ("envelope_keys_dropped", &losses.envelope_keys_dropped),
        ("item_keys_dropped", &losses.item_keys_dropped),
        ("element_keys_dropped", &losses.element_keys_dropped),
        ("elements_dropped", &losses.elements_dropped),
        (
            "element_keys_added_on_reload",
            &losses.element_keys_added_on_reload,
        ),
    ] {
        kinds.extend(map.keys().map(|k| format!("{name}.{k}")));
    }
    for (name, n) in [
        ("bindings_cleared", losses.bindings_cleared),
        (
            "legacy_bindings_without_geometry",
            losses.legacy_bindings_without_geometry,
        ),
        ("element_ids_replaced", losses.element_ids_replaced),
    ] {
        if n > 0 {
            kinds.push(name.to_owned());
        }
    }
    kinds
}

/// [`TestEnv`] counting the legacy bindings restore asks it to migrate.
#[derive(Default)]
struct Counting {
    env: TestEnv,
    legacy_bindings: u64,
}

impl RestoreEnv for Counting {
    fn now(&mut self) -> f64 {
        self.env.now()
    }
    fn random_id(&mut self) -> String {
        self.env.random_id()
    }
    fn random_integer(&mut self) -> f64 {
        self.env.random_integer()
    }
    fn migrate_legacy_binding(
        &mut self,
        request: LegacyBindingRequest<'_>,
    ) -> Option<LegacyBinding> {
        self.legacy_bindings += 1;
        self.env.migrate_legacy_binding(request)
    }
}

/// What loading one library loses.
#[derive(Debug, Default, Serialize)]
struct Losses {
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<u64>,
    items_in: u64,
    items_out: u64,
    elements_in: u64,
    elements_out: u64,
    /// Envelope keys the written v2 envelope does not have.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    envelope_keys_dropped: BTreeMap<String, u64>,
    /// Keys of a v2 item missing from the written item.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    item_keys_dropped: BTreeMap<String, u64>,
    /// Keys of an element as read missing from it as written.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    element_keys_dropped: BTreeMap<String, u64>,
    /// Elements not written, by reason.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    elements_dropped: BTreeMap<String, u64>,
    /// `startBinding`/`endBinding` objects written as `null`.
    #[serde(skip_serializing_if = "is_zero")]
    bindings_cleared: u64,
    /// Legacy bindings (no `mode`) to an existing element: upstream keeps
    /// them with a computed `mode` and `fixedPoint`; without geometry they
    /// are cleared (ex-116).
    #[serde(skip_serializing_if = "is_zero")]
    legacy_bindings_without_geometry: u64,
    /// Elements whose id repeated an earlier one in the item.
    #[serde(skip_serializing_if = "is_zero")]
    element_ids_replaced: u64,
    /// Keys, as `type.key`, the written file does not have that loading it
    /// again adds (no other change is allowed).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    element_keys_added_on_reload: BTreeMap<String, u64>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl Losses {
    fn add(&mut self, other: &Losses) {
        self.items_in += other.items_in;
        self.items_out += other.items_out;
        self.elements_in += other.elements_in;
        self.elements_out += other.elements_out;
        for (mine, theirs) in [
            (
                &mut self.envelope_keys_dropped,
                &other.envelope_keys_dropped,
            ),
            (&mut self.item_keys_dropped, &other.item_keys_dropped),
            (&mut self.element_keys_dropped, &other.element_keys_dropped),
            (&mut self.elements_dropped, &other.elements_dropped),
            (
                &mut self.element_keys_added_on_reload,
                &other.element_keys_added_on_reload,
            ),
        ] {
            for (k, n) in theirs {
                *mine.entry(k.clone()).or_default() += n;
            }
        }
        self.bindings_cleared += other.bindings_cleared;
        self.legacy_bindings_without_geometry += other.legacy_bindings_without_geometry;
        self.element_ids_replaced += other.element_ids_replaced;
    }
}

fn bump(map: &mut BTreeMap<String, u64>, key: &str) {
    *map.entry(key.to_owned()).or_default() += 1;
}

/// Why restore does not write an element of an item, or `None` if it does:
/// the steps of `restoreElements` then `restoreLibraryItem`
/// (`restore.ts:946-1012, 1374-1379`) and the typed model's reading.
fn dropped_because(element: &Value, targets: &ElementsMap) -> Option<&'static str> {
    let Value::Object(element) = element else {
        return Some("not an object");
    };
    if element.get("type").and_then(Value::as_str) == Some("selection") {
        return Some("legacy selection element");
    }
    let restored = match restore_element(
        element,
        targets,
        None,
        RestoreOptions::default(),
        &mut TestEnv::default(),
    ) {
        Err(_) => return Some("restore throws"),
        Ok(None) => return Some("unknown type"),
        Ok(Some(restored)) => restored,
    };
    match Element::from_map(restored) {
        Err(_) => Some("typed model cannot read it (ex-117)"),
        Ok(e) if e.base.is_deleted => Some("deleted"),
        Ok(_) => None,
    }
}

fn is_binding(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Object(_)))
}

/// The losses of one library: `raw` as read, `written` what
/// `serialize_library_as_json` wrote for it.
fn losses(raw: &Value, written: &Value) -> Losses {
    let mut losses = Losses::default();
    let raw = raw.as_object().expect("envelope");
    let written = written.as_object().expect("written envelope");
    for key in raw.keys() {
        if !written.contains_key(key) {
            bump(&mut losses.envelope_keys_dropped, key);
        }
    }
    // `libraryItems || library`: an array is truthy, empty or not.
    let raw_items = match raw.get("libraryItems") {
        Some(Value::Array(items)) => items,
        _ => raw["library"].as_array().expect("library"),
    };
    let mut out_items = written["libraryItems"]
        .as_array()
        .expect("libraryItems")
        .iter();
    losses.items_in = raw_items.len() as u64;
    for raw_item in raw_items {
        let (elements, item_keys) = match raw_item {
            Value::Array(elements) => (elements, None),
            Value::Object(item) => (item["elements"].as_array().expect("elements"), Some(item)),
            _ => panic!("item of another shape"),
        };
        losses.elements_in += elements.len() as u64;
        let targets = ElementsMap::new(
            &elements
                .iter()
                .filter_map(|e| e.as_object().cloned())
                .collect::<Vec<_>>(),
        );
        let kept: Vec<&Value> = elements
            .iter()
            .filter(|e| match dropped_because(e, &targets) {
                Some(reason) => {
                    bump(&mut losses.elements_dropped, reason);
                    false
                }
                None => true,
            })
            .collect();
        if kept.is_empty() {
            continue;
        }
        let out = out_items.next().expect("written item");
        losses.items_out += 1;
        if let Some(item) = item_keys {
            for key in item.keys() {
                if !out.as_object().expect("item").contains_key(key) {
                    bump(&mut losses.item_keys_dropped, key);
                }
            }
        }
        let out_elements = out["elements"].as_array().expect("written elements");
        assert_eq!(kept.len(), out_elements.len(), "elements of an item");
        losses.elements_out += out_elements.len() as u64;
        let mut seen = BTreeSet::new();
        for (before, after) in kept.into_iter().zip(out_elements) {
            let (before, after) = (
                before.as_object().expect("element"),
                after.as_object().expect("written element"),
            );
            let id = before["id"].to_string();
            if !seen.insert(id) {
                losses.element_ids_replaced += 1;
            } else {
                assert_eq!(before["id"], after["id"], "element order");
            }
            for key in before.keys() {
                if !after.contains_key(key) {
                    bump(&mut losses.element_keys_dropped, key);
                }
            }
            for key in ["startBinding", "endBinding"] {
                if is_binding(before.get(key)) && !is_binding(after.get(key)) {
                    losses.bindings_cleared += 1;
                }
            }
        }
    }
    assert!(out_items.next().is_none(), "more items written than kept");
    losses
}

/// The element keys, as `type.key`, that `second` has and `first` has not;
/// panics on any other difference between the two writes.
fn added_on_reload(first: &Value, second: &Value) -> BTreeMap<String, u64> {
    let items = |v: &Value| v["libraryItems"].as_array().expect("items").clone();
    let (first, second) = (items(first), items(second));
    assert_eq!(first.len(), second.len(), "items on reload");
    let mut added = BTreeMap::new();
    for (a, b) in first.iter().zip(&second) {
        let (a, b) = (a.as_object().expect("item"), b.as_object().expect("item"));
        assert!(a.keys().eq(b.keys()), "item keys on reload");
        for (key, value) in a {
            if key != "elements" {
                assert_eq!(*value, b[key], "item {key} on reload");
            }
        }
        let (a, b) = (
            a["elements"].as_array().expect("elements"),
            b["elements"].as_array().expect("elements"),
        );
        assert_eq!(a.len(), b.len(), "elements on reload");
        for (a, b) in a.iter().zip(b) {
            let (a, b) = (
                a.as_object().expect("element"),
                b.as_object().expect("element"),
            );
            assert!(a.keys().all(|k| b.contains_key(k)), "a key lost on reload");
            for (key, value) in b {
                match a.get(key) {
                    Some(before) => assert_eq!(before, value, "{key} on reload"),
                    None => bump(
                        &mut added,
                        &format!("{}.{key}", b["type"].as_str().expect("type")),
                    ),
                }
            }
        }
    }
    added
}

#[derive(Serialize)]
struct Report {
    generator: &'static str,
    description: &'static str,
    libraries: usize,
    /// Why each kind of loss happens.
    reasons: BTreeMap<&'static str, &'static str>,
    totals: Losses,
    files: Vec<Losses>,
}

fn element_count(items: &[excali_core::library::LibraryItem]) -> usize {
    items.iter().map(|i| i.elements.len()).sum()
}

#[test]
fn every_catalogue_library_round_trips() {
    let catalogue = catalogue();
    assert_eq!(catalogue.len(), 232);

    // The golden of upstream's output covers exactly these files.
    let golden: Value =
        serde_json::from_str(include_str!("fixtures/library.json")).expect("library.json parses");
    let cases: BTreeMap<String, &Value> = golden["catalogue"]
        .as_array()
        .expect("catalogue")
        .iter()
        .map(|c| (c["file"].as_str().expect("file").to_owned(), c))
        .collect();
    let paths: BTreeSet<String> = catalogue
        .iter()
        .map(|(p, _)| format!("fixtures/{p}"))
        .collect();
    assert_eq!(paths, cases.keys().cloned().collect());

    let mut totals = Losses::default();
    let mut files = Vec::new();
    let mut versions = BTreeMap::<u64, usize>::new();
    for (path, text) in &catalogue {
        let mut env = Counting::default();
        let items = parse_library_json(text, LibraryItemStatus::Published, &mut env)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let written = serialize_library_as_json(&items, SOURCE);

        // Loaded again: the same items, element for element.
        let again = parse_library_json(
            &written,
            LibraryItemStatus::Published,
            &mut TestEnv::default(),
        )
        .unwrap_or_else(|e| panic!("{path}: written file: {e}"));
        assert_eq!(again.len(), items.len(), "{path}: items");
        for (a, b) in items.iter().zip(&again) {
            assert_eq!(a.elements, b.elements, "{path}: elements of {}", a.id);
        }
        assert_eq!(again, items, "{path}: items");

        // Written again: what upstream writes when it loads its own output.
        // It is not always the first write (a `draw` restored to `line` has
        // no `polygon` until the next load adds `polygon: false`,
        // restore.ts:646), but it is where the file settles.
        let rewritten = serialize_library_as_json(&again, SOURCE);
        if rewritten != written {
            let third = parse_library_json(
                &rewritten,
                LibraryItemStatus::Published,
                &mut TestEnv::default(),
            )
            .unwrap_or_else(|e| panic!("{path}: rewritten file: {e}"));
            assert_eq!(third, items, "{path}: items after a second reload");
            assert_eq!(
                serialize_library_as_json(&third, SOURCE),
                rewritten,
                "{path}: third write"
            );
        }

        // Both writes are upstream's bytes. aarondiel/logic-gates loses 24
        // lines (ex-117), so its are not; `library.rs` checks what it keeps.
        let case = cases[&format!("fixtures/{path}")];
        let digest = |key: &str| {
            case.get(format!("{key}_without_geometry"))
                .unwrap_or(&case[key])
                .as_str()
                .expect("sha")
                .to_owned()
        };
        if path != LOGIC_GATES {
            assert_eq!(
                sha256(written.as_bytes()),
                digest("output_sha256"),
                "{path}"
            );
            assert_eq!(
                sha256(rewritten.as_bytes()),
                digest("reload_sha256"),
                "{path}: reload"
            );
        }

        let raw: Value = serde_json::from_str(text).expect("json");
        let version = raw["version"].as_u64().expect("version");
        *versions.entry(version).or_default() += 1;
        let written_value: Value = serde_json::from_str(&written).expect("written json");
        let mut file = losses(&raw, &written_value);
        file.legacy_bindings_without_geometry = env.legacy_bindings;
        let rewritten_value: Value = serde_json::from_str(&rewritten).expect("rewritten json");
        file.element_keys_added_on_reload = added_on_reload(&written_value, &rewritten_value);
        assert_eq!(file.items_out, items.len() as u64, "{path}");
        assert_eq!(file.elements_out, element_count(&items) as u64, "{path}");

        assert_eq!(file.items_out, case["items"], "{path}");
        assert_eq!(
            file.legacy_bindings_without_geometry,
            case.get("geometry").and_then(Value::as_u64).unwrap_or(0),
            "{path}"
        );

        totals.add(&file);
        file.path = Some(path.clone());
        file.version = Some(version);
        files.push(file);
    }
    assert_eq!(versions, BTreeMap::from([(1, 70), (2, 162)]));

    for kind in kinds(&totals) {
        assert!(
            REASONS.iter().any(|(k, _)| *k == kind),
            "{kind}: a loss the report does not explain; add it to REASONS"
        );
    }
    let report = Report {
        generator: "crates/excali-core/tests/library_corpus.rs",
        description: DESCRIPTION,
        libraries: files.len(),
        reasons: REASONS.iter().copied().collect(),
        totals,
        files,
    };
    let text = serde_json::to_string_pretty(&report).expect("report") + "\n";
    let report_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(REPORT);
    if std::env::var_os("EXCALI_BLESS").is_some() {
        std::fs::write(&report_path, &text).expect("write report");
    }
    let committed = std::fs::read_to_string(&report_path)
        .unwrap_or_else(|e| panic!("{REPORT}: {e} (run with EXCALI_BLESS=1 to write it)"));
    assert!(
        committed == text,
        "{REPORT} is out of date: run with EXCALI_BLESS=1 and review the diff"
    );
}

/// The report keeps the known losses in view: the ex-116 and ex-117 gaps
/// and the legacy keys upstream's restore migrates away.
#[test]
fn report_records_the_known_losses() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(REPORT);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{REPORT}: {e}"));
    let report: Value = serde_json::from_str(&text).expect("report parses");
    let totals = &report["totals"];
    assert_eq!(report["libraries"], 232);
    assert_eq!(totals["legacy_bindings_without_geometry"], 1245);
    // Those, 7 bindings to elements the item does not hold and 2 on lines.
    assert_eq!(totals["bindings_cleared"], 1254);
    assert_eq!(totals["elements_in"], 55113);
    assert_eq!(totals["elements_out"], 55089);
    assert_eq!(
        totals["elements_dropped"]["typed model cannot read it (ex-117)"],
        24
    );
    let logic_gates = report["files"]
        .as_array()
        .expect("files")
        .iter()
        .find(|f| f["path"] == LOGIC_GATES)
        .expect("logic-gates");
    assert_eq!(
        logic_gates["elements_dropped"]["typed model cannot read it (ex-117)"],
        24
    );
    // Legacy keys restore replaces: `strokeSharpness` by `roundness`
    // (restore.ts), `boundElementIds` by `boundElements`.
    let dropped: &Map<String, Value> = totals["element_keys_dropped"]
        .as_object()
        .expect("element_keys_dropped");
    for key in ["strokeSharpness", "boundElementIds"] {
        assert!(dropped.contains_key(key), "{key}");
    }
    // Every v1 file loses its `library` key to `libraryItems`.
    assert_eq!(totals["envelope_keys_dropped"]["library"], 70);
    // The 115 legacy `draw` elements become lines that get `polygon` on
    // the next load, and nothing else changes then.
    assert_eq!(
        totals["element_keys_added_on_reload"],
        serde_json::json!({"line.polygon": 115})
    );
}
